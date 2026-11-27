//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use glam::{Mat4, Vec3};
use wgpu::TextureViewDimension::{Cube, D2};

use crate::{
    Error,
    data::Files,
    dds, demo,
    gpu::{COLOR, DEPTH, Gpu, SAMPLES, SLOT},
    model::{Model, Texture},
    pass::{CONSTANTS, Context, Pass, State, Stream, put},
    show::{Frame, Show},
    stage::Stage,
};

/// Bytes a program can address.
const BLOCK: u64 = CONSTANTS as u64 * 16;

struct Mesh {
    vertices: wgpu::Buffer,
    geometry: usize,
    streams: Vec<Stream>,
}

struct GpuPass {
    pipeline: usize,
    group: wgpu::BindGroup,
    /// Groups with other textures in the first units, for shells and skins.
    alternates: Vec<wgpu::BindGroup>,
    mesh: usize,
    blended: bool,
}

struct GpuShape {
    passes: Vec<GpuPass>,
    fur: Option<GpuPass>,
}

struct Actor {
    model: Model,
    /// Index buffers by geometry.
    indices: Vec<(wgpu::Buffer, u32)>,
    meshes: Vec<Mesh>,
    shapes: Vec<GpuShape>,
    /// Shapes in drawing order.
    order: Vec<usize>,
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

/// Something a demo draws that is not a shape.
struct GpuEffect {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    vertices: u32,
    instances: u32,
}

pub struct Scene {
    actors: Vec<Actor>,
    stage: Stage,
    effects: Vec<GpuEffect>,
    /// Textures drawn before the picture: color, depth, and what they start from.
    offscreen: Vec<(wgpu::TextureView, Option<wgpu::TextureView>, wgpu::Color)>,
    pipelines: Vec<wgpu::RenderPipeline>,
    uniforms: wgpu::Buffer,
    /// Draws the buffer has room for, in bytes.
    capacity: u64,
    layout: wgpu::BindGroupLayout,
    constants: wgpu::BindGroup,
    sky: Option<(wgpu::RenderPipeline, wgpu::BindGroup)>,
    badge: (wgpu::RenderPipeline, wgpu::BindGroup),
    logo_size: [f32; 2],
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
    scratch: Vec<u8>,
}

pub(crate) fn rgba(files: &Files, name: &str) -> Result<((u32, u32), Vec<u8>), Error> {
    let data = files.get(name)?;
    let (w, h, pixels) = if name.ends_with(".tga") {
        crate::tga::read(data)?
    } else if name.ends_with(".png") {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut r = decoder.read_info()?;
        let mut buffer = vec![0; r.output_buffer_size().ok_or("PNG too large")?];
        let image = r.next_frame(&mut buffer)?;
        let raw = &buffer[..image.buffer_size()];
        let pixels = match image.color_type {
            png::ColorType::Rgba => raw.to_vec(),
            png::ColorType::Rgb => raw
                .chunks_exact(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::Grayscale => raw.iter().flat_map(|p| [*p, *p, *p, 255]).collect(),
            png::ColorType::GrayscaleAlpha => raw
                .chunks_exact(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Indexed => return Err("unexpanded PNG".into()),
        };
        (image.width as usize, image.height as usize, pixels)
    } else {
        dds::Image::read(data)?.decode(0)
    };
    if w == 0 || h == 0 {
        return Err("empty texture".into());
    }
    Ok(((u32::try_from(w)?, u32::try_from(h)?), pixels))
}

/// Loads textures once and makes bind groups of four.
struct Loader<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    views: HashMap<String, wgpu::TextureView>,
    samplers: HashMap<[bool; 2], wgpu::Sampler>,
}

type Unit<'a> = (Option<&'a Texture>, bool);

impl Loader<'_> {
    fn view(&mut self, file: Option<&str>, cube: bool) -> Result<wgpu::TextureView, Error> {
        let key = file.unwrap_or(if cube { "#cube" } else { "#plane" });
        if let Some(view) = self.views.get(key) {
            return Ok(view.clone());
        }
        let (size, pixels) = match (file, cube) {
            (Some(file), true) => {
                let faces = self
                    .files
                    .faces(file)?
                    .iter()
                    .map(|face| rgba(self.files, face).map_err(|e| format!("{face}: {e}")))
                    .collect::<Result<Vec<_>, _>>()?;
                let size = faces[0].0;
                if faces.iter().any(|(s, _)| *s != size) {
                    return Err(format!("{file}: inconsistent cube face sizes").into());
                }
                (size, faces.into_iter().map(|(_, p)| p).collect())
            }
            (Some(file), false) => {
                let (size, data) = rgba(self.files, file).map_err(|e| format!("{file}: {e}"))?;
                (size, vec![data])
            }
            (None, true) => ((1, 1), vec![vec![0, 0, 0, 255]; 6]),
            (None, false) => ((1, 1), vec![vec![255; 4]]),
        };
        let view =
            self.gpu
                .upload_rgba(size, &pixels, true)
                .create_view(&wgpu::TextureViewDescriptor {
                    dimension: cube.then_some(Cube),
                    ..Default::default()
                });
        self.views.insert(key.to_owned(), view.clone());
        Ok(view)
    }

    fn sampler(&mut self, repeat: [bool; 2]) -> wgpu::Sampler {
        let mode = |r| match r {
            true => wgpu::AddressMode::Repeat,
            false => wgpu::AddressMode::ClampToEdge,
        };
        let device = &self.gpu.device;
        self.samplers
            .entry(repeat)
            .or_insert_with(|| {
                device.create_sampler(&wgpu::SamplerDescriptor {
                    address_mode_u: mode(repeat[0]),
                    address_mode_v: mode(repeat[1]),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    mipmap_filter: wgpu::MipmapFilterMode::Linear,
                    ..Default::default()
                })
            })
            .clone()
    }

    fn group(
        &mut self,
        layout: &wgpu::BindGroupLayout,
        units: &[Unit<'_>],
    ) -> Result<wgpu::BindGroup, Error> {
        let mut views = Vec::new();
        let mut samplers = Vec::new();
        for (texture, cube) in units {
            views.push(self.view(texture.map(|t| t.file.as_str()), *cube)?);
            samplers.push(self.sampler(texture.map_or([false; 2], |t| t.repeat)));
        }
        let textures = views.iter().map(wgpu::BindingResource::TextureView);
        let all = textures.chain(samplers.iter().map(wgpu::BindingResource::Sampler));
        let entries: Vec<_> = (0..)
            .zip(all)
            .map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource })
            .collect();
        Ok(self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &entries,
            }))
    }
}

/// Textures at bindings from 0, then their samplers.
fn texture_layout(device: &wgpu::Device, cubes: &[bool]) -> wgpu::BindGroupLayout {
    let texture = |cube: &bool| wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: if *cube { Cube } else { D2 },
        multisampled: false,
    };
    let sampler = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
    let types = cubes
        .iter()
        .map(texture)
        .chain(cubes.iter().map(|_| sampler));
    let entries: Vec<_> = (0..)
        .zip(types)
        .map(|(binding, ty)| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        })
        .collect();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    })
}

fn blend_factor(n: u32) -> Result<wgpu::BlendFactor, Error> {
    use wgpu::BlendFactor::*;
    Ok(match n {
        0 => Zero,
        1 => One,
        2 => Src,
        3 => OneMinusSrc,
        4 => SrcAlpha,
        5 => OneMinusSrcAlpha,
        6 => Dst,
        7 => OneMinusDst,
        8 => DstAlpha,
        9 => OneMinusDstAlpha,
        10 => SrcAlphaSaturated,
        n => return Err(format!("unsupported blend factor {n}").into()),
    })
}

fn compare(n: u32) -> Result<wgpu::CompareFunction, Error> {
    use wgpu::CompareFunction::*;
    Ok(match n {
        0 => Never,
        1 => Less,
        2 => LessEqual,
        3 => Greater,
        4 => GreaterEqual,
        5 => Equal,
        6 => NotEqual,
        7 => Always,
        n => return Err(format!("unsupported comparison {n}").into()),
    })
}

/// Builds pipelines, each once.
struct Pipelines<'a> {
    device: &'a wgpu::Device,
    format: wgpu::TextureFormat,
    constants: &'a wgpu::BindGroupLayout,
    layouts: HashMap<[bool; 4], wgpu::BindGroupLayout>,
    shaders: HashMap<String, wgpu::ShaderModule>,
    known: HashMap<(String, usize, State, Option<usize>), usize>,
    /// Whether the stage's textures have depth.
    depths: Vec<bool>,
    made: Vec<wgpu::RenderPipeline>,
}

impl Pipelines<'_> {
    fn textures(&mut self, cubes: [bool; 4]) -> wgpu::BindGroupLayout {
        let device = self.device;
        self.layouts
            .entry(cubes)
            .or_insert_with(|| texture_layout(device, &cubes))
            .clone()
    }

    fn get(&mut self, pass: &Pass) -> Result<usize, Error> {
        let key = (
            pass.wgsl.clone(),
            pass.streams.len(),
            pass.state,
            pass.offscreen,
        );
        if let Some(&i) = self.known.get(&key) {
            return Ok(i);
        }
        let textures = self.textures(pass.units.map(|u| u == Some(true)));
        let device = self.device;
        let shader = self
            .shaders
            .entry(pass.wgsl.clone())
            .or_insert_with(|| {
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: None,
                    source: wgpu::ShaderSource::Wgsl(pass.wgsl.as_str().into()),
                })
            })
            .clone();
        let attributes: Vec<_> = (0..pass.streams.len())
            .map(|i| wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16 * i as u64,
                shader_location: i as u32,
            })
            .collect();
        let state = pass.state;
        let blend = match state.blend {
            Some((src, dst)) => {
                let c = wgpu::BlendComponent {
                    src_factor: blend_factor(src)?,
                    dst_factor: blend_factor(dst)?,
                    operation: wgpu::BlendOperation::Add,
                };
                Some(wgpu::BlendState { color: c, alpha: c })
            }
            None => None,
        };
        let mut mask = wgpu::ColorWrites::empty();
        if state.color {
            mask |= wgpu::ColorWrites::COLOR;
        }
        // The picture stays opaque, or a page shows through it.
        if state.alpha && pass.offscreen.is_some() {
            mask |= wgpu::ColorWrites::ALPHA;
        }
        let (depth_compare, write) = match state.depth {
            Some((func, write)) => (compare(func)?, write),
            None => (wgpu::CompareFunction::Always, false),
        };
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(self.constants), Some(&textures)],
            immediate_size: 0,
        });
        self.made.push(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 16 * pass.streams.len() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: pass.offscreen.map_or(self.format, |_| COLOR),
                        blend,
                        write_mask: mask,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    front_face: match state.ccw_front {
                        true => wgpu::FrontFace::Ccw,
                        false => wgpu::FrontFace::Cw,
                    },
                    cull_mode: match state.cull {
                        0 => Some(wgpu::Face::Front),
                        1 => Some(wgpu::Face::Back),
                        _ => None,
                    },
                    ..Default::default()
                },
                depth_stencil: pass.offscreen.is_none_or(|i| self.depths[i]).then_some(
                    wgpu::DepthStencilState {
                        format: DEPTH,
                        depth_write_enabled: Some(write),
                        depth_compare: Some(depth_compare),
                        stencil: Default::default(),
                        bias: Default::default(),
                    },
                ),
                multisample: match pass.offscreen {
                    Some(_) => Default::default(),
                    None => samples(),
                },
                multiview_mask: None,
                cache: None,
            }),
        );
        let i = self.made.len() - 1;
        self.known.insert(key, i);
        Ok(i)
    }
}

fn samples() -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: SAMPLES,
        ..Default::default()
    }
}

/// A pipeline without vertex buffers, that leaves depths alone.
fn backdrop(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layouts: [&wgpu::BindGroupLayout; 2],
    source: &str,
    blend: Option<wgpu::BlendState>,
    depth: wgpu::CompareFunction,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &layouts.map(Some),
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(false),
            depth_compare: Some(depth),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: samples(),
        multiview_mask: None,
        cache: None,
    })
}

impl Actor {
    fn new(
        gpu: &Gpu,
        model: Model,
        stage: &Stage,
        loader: &mut Loader<'_>,
        pipelines: &mut Pipelines<'_>,
    ) -> Result<Self, Error> {
        let mut meshes: Vec<Mesh> = Vec::new();
        let mut shapes = Vec::new();
        for (index, shape) in model.shapes.iter().enumerate() {
            let mut make =
                |pass: &Pass, alternates: Vec<Vec<Option<usize>>>| -> Result<GpuPass, Error> {
                    let found = meshes
                        .iter()
                        .position(|m| m.geometry == shape.geometry && m.streams == pass.streams);
                    let mesh = found.unwrap_or_else(|| {
                        let data = model.geometries[shape.geometry].interleave(&pass.streams);
                        let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
                        meshes.push(Mesh {
                            vertices: gpu.buffer(bytemuck::cast_slice(&data), usage),
                            geometry: shape.geometry,
                            streams: pass.streams.clone(),
                        });
                        meshes.len() - 1
                    });
                    let cubes = pass.units.map(|u| u == Some(true));
                    let layout = pipelines.textures(cubes);
                    let mut group = |textures: &[Option<usize>]| {
                        let units: Vec<Unit<'_>> = textures
                            .iter()
                            .zip(cubes)
                            .map(|(t, cube)| (t.map(|i| &model.textures[i]), cube))
                            .collect();
                        loader.group(&layout, &units)
                    };
                    Ok(GpuPass {
                        pipeline: pipelines.get(pass)?,
                        group: group(&pass.textures)?,
                        alternates: alternates
                            .iter()
                            .map(|t| group(t))
                            .collect::<Result<_, _>>()?,
                        mesh,
                        blended: pass.state.blend.is_some(),
                    })
                };
            let error = |e: Error| format!("{}: {e}", shape.name);
            let passes = shape
                .passes
                .iter()
                .enumerate()
                .map(|(i, p)| make(p, stage.alternates(&model, index, i)).map_err(error))
                .collect::<Result<_, _>>()?;
            let fur = shape
                .fur
                .as_ref()
                .map(|f| {
                    let shells = f.textures.iter().map(|&t| {
                        let mut own = f.pass.textures.to_vec();
                        own[1] = Some(t);
                        own
                    });
                    make(&f.pass, shells.collect()).map_err(error)
                })
                .transpose()?;
            shapes.push(GpuShape { passes, fur });
        }
        let indices = model
            .geometries
            .iter()
            .map(|g| {
                let data = bytemuck::cast_slice(&g.indices);
                (
                    gpu.buffer(data, wgpu::BufferUsages::INDEX),
                    g.indices.len() as u32,
                )
            })
            .collect();
        let mut order: Vec<usize> = (0..shapes.len()).collect();
        // Blended shapes come last in their set, otherwise the file's order stays.
        let blended = |s: &GpuShape| s.passes.first().is_some_and(|p| p.blended);
        order.sort_by_key(|&i| (model.shapes[i].order, blended(&shapes[i])));
        Ok(Self {
            model,
            indices,
            meshes,
            shapes,
            order,
        })
    }
}

/// Appends constants and returns where they are.
fn block(out: &mut Vec<u8>, values: &[[f32; 4]]) -> u32 {
    let at = out.len();
    out.extend_from_slice(bytemuck::cast_slice(values));
    let end = out.len().max(at + 1).next_multiple_of(SLOT as usize);
    out.resize(end, 0);
    at as u32
}

/// One draw of a frame.
struct Call {
    actor: usize,
    shape: usize,
    /// The pass, or the fur's shell.
    pass: Result<usize, usize>,
    offset: u32,
    alternate: Option<usize>,
    layer: u32,
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let mut loader = Loader {
            gpu,
            files,
            views: HashMap::new(),
            samplers: HashMap::new(),
        };
        let mut pipelines = Pipelines {
            device,
            format,
            constants: &layout,
            layouts: HashMap::new(),
            shaders: HashMap::new(),
            known: HashMap::new(),
            depths: Vec::new(),
            made: Vec::new(),
        };
        let mut models = files.models()?;
        let stage = Stage::new(files, &mut models)?;
        // What is drawn first stands for textures of the scene.
        let mut offscreen = Vec::new();
        for o in stage.offscreen() {
            let view = |format| {
                gpu.target(o.size, o.size, format, 1)
                    .create_view(&Default::default())
            };
            let color = view(COLOR);
            loader.views.insert(o.file.to_owned(), color.clone());
            pipelines.depths.push(o.depth);
            offscreen.push((color, o.depth.then(|| view(DEPTH)), o.clear));
        }
        let mut actors = Vec::new();
        for (model, name) in models.into_iter().zip(demo::MODELS) {
            if model.shapes.is_empty() {
                return Err(format!("{name}: no drawable shapes").into());
            }
            let actor = Actor::new(gpu, model, &stage, &mut loader, &mut pipelines);
            actors.push(actor.map_err(|e| format!("{name}: {e}"))?);
        }
        let mut effects = Vec::new();
        for e in stage.effects() {
            let textures: Vec<Texture> = e
                .textures
                .iter()
                .map(|file| Texture {
                    name: String::new(),
                    file: (*file).to_owned(),
                    cube: false,
                    shadow: false,
                    repeat: [e.repeat; 2],
                })
                .collect();
            let units: Vec<Unit<'_>> = textures.iter().map(|t| (Some(t), false)).collect();
            let planes = texture_layout(device, &vec![false; units.len()]);
            effects.push(GpuEffect {
                pipeline: backdrop(
                    device,
                    format,
                    [&layout, &planes],
                    e.source,
                    Some(e.blend),
                    wgpu::CompareFunction::LessEqual,
                ),
                group: loader.group(&planes, &units)?,
                vertices: e.vertices,
                instances: e.instances,
            });
        }
        let single = [
            texture_layout(device, &[false]),
            texture_layout(device, &[true]),
        ];
        use wgpu::CompareFunction::Always;
        let environment = actors.iter().find_map(|a| {
            let texture = a.model.environment?;
            Some(&a.model.textures[texture])
        });
        let sky = match environment {
            Some(texture) => Some((
                backdrop(device, format, [&layout, &single[1]], SKY, None, Always),
                loader.group(&single[1], &[(Some(texture), true)])?,
            )),
            None => None,
        };
        let logo = Texture {
            name: String::new(),
            file: "userinterface/nvidialogo.dds".to_owned(),
            cube: false,
            shadow: false,
            repeat: [false; 2],
        };
        let badge = (
            backdrop(
                device,
                format,
                [&layout, &single[0]],
                BADGE,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                Always,
            ),
            loader.group(&single[0], &[(Some(&logo), false)])?,
        );
        let ((w, h), _) = rgba(files, &logo.file)?;
        let (uniforms, constants) = uniforms(device, &layout, 64 * BLOCK);
        let pipelines = pipelines.made;
        Ok(Self {
            actors,
            stage,
            effects,
            offscreen,
            pipelines,
            uniforms,
            capacity: 64 * BLOCK,
            layout,
            constants,
            sky,
            badge,
            logo_size: [w as f32, h as f32],
            format,
            targets: None,
            scratch: Vec::new(),
        })
    }

    pub fn show(&self) -> Show {
        let models: Vec<&Model> = self.actors.iter().map(|a| &a.model).collect();
        self.stage.show(&models)
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        frame: &Frame,
        window: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let target = |format| {
                gpu.target(size.0, size.1, format, SAMPLES)
                    .create_view(&Default::default())
            };
            self.targets = Some(Targets {
                size,
                color: target(self.format),
                depth: target(DEPTH),
            });
        }
        let aspect = size.0 as f32 / size.1 as f32;
        let projection = projection(frame, aspect);
        let eye = frame.view.inverse().transform_point3(Vec3::ZERO);
        self.scratch.clear();
        // The sky and the logo share the first block.
        let turn = Mat4::from_mat3(glam::Mat3::from_mat4(frame.view));
        let mut first: Vec<_> = (0..4)
            .map(|i| (projection * turn).inverse().row(i).to_array())
            .collect();
        let [w, h] = self.logo_size;
        // The logo's texels are 1/850 of half the height.
        first.push([0.0, w / 850.0 / aspect, h / 850.0, 0.0]);
        block(&mut self.scratch, &first);

        let mut calls = Vec::new();
        for (index, actor) in self.actors.iter_mut().enumerate() {
            for geometry in self.stage.update(index, &mut actor.model, frame) {
                for m in actor.meshes.iter().filter(|m| m.geometry == geometry) {
                    let data = actor.model.geometries[geometry].interleave(&m.streams);
                    gpu.queue
                        .write_buffer(&m.vertices, 0, bytemuck::cast_slice(&data));
                }
            }
        }
        for index in 0..self.actors.len() {
            let model = &self.actors[index].model;
            let rest = model.rest();
            let instances = self.stage.instances(index, model, frame, projection, eye);
            for instance in &instances {
                let context = Context {
                    projection: instance.camera.map_or(projection, |c| c[0]),
                    view: instance.camera.map_or(frame.view, |c| c[1]),
                    world: &instance.world,
                    rest: &rest,
                    values: &instance.values,
                };
                let actor = &self.actors[index];
                for &shape in &actor.order {
                    let s = &actor.model.shapes[shape];
                    for d in self
                        .stage
                        .draws(index, &actor.model, shape, frame, instance)
                    {
                        let context = Context {
                            values: d.values.as_ref().unwrap_or(&instance.values),
                            ..context
                        };
                        let mut blocks = Vec::new();
                        for i in d.passes {
                            if let Some(p) = s.passes.get(i) {
                                blocks.push((Ok(i), p.constants(&context), d.alternate));
                            }
                        }
                        if let Some(f) = &s.fur {
                            for shell in 0..d.shells.min(f.textures.len()) {
                                let values = self.stage.shell(f, shell, frame);
                                let context = Context {
                                    values: &values,
                                    ..context
                                };
                                blocks.push((Err(shell), f.pass.constants(&context), Some(shell)));
                            }
                        }
                        for (pass, mut values, alternate) in blocks {
                            for (at, row) in &instance.registers {
                                put(&mut values, *at, &[*row]);
                            }
                            calls.push(Call {
                                actor: index,
                                shape,
                                pass,
                                offset: block(&mut self.scratch, &values),
                                alternate,
                                layer: d.layer,
                            });
                        }
                    }
                }
            }
        }
        let effects: Vec<_> = (0..self.effects.len())
            .map(|i| {
                let values = self
                    .stage
                    .effect_values(i, frame, projection * frame.view, eye);
                values.map(|v| block(&mut self.scratch, &v))
            })
            .collect();
        calls.sort_by_key(|c| c.layer);
        // The last block must be whole.
        let needed = self.scratch.len() as u64 + BLOCK;
        if needed > self.capacity {
            self.capacity = needed.next_power_of_two();
            (self.uniforms, self.constants) = uniforms(&gpu.device, &self.layout, self.capacity);
        }
        gpu.queue.write_buffer(&self.uniforms, 0, &self.scratch);

        let first = |c: &Call| match c.pass {
            Ok(i) => self.actors[c.actor].model.shapes[c.shape].passes[i].offscreen,
            Err(_) => None,
        };
        for (i, (color, depth, clear)) in self.offscreen.iter().enumerate() {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(*clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: depth.as_ref().map(|view| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }
                }),
                ..Default::default()
            });
            for call in calls.iter().filter(|c| first(c) == Some(i)) {
                self.call(&mut pass, call);
            }
        }
        let target = self.targets.as_ref().expect("made above");
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color,
                depth_slice: None,
                resolve_target: Some(window),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.constants, &[0]);
        if let Some((pipeline, group)) = &self.sky {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(1, group, &[]);
            pass.draw(0..3, 0..1);
        }
        for call in calls.iter().filter(|c| first(c).is_none()) {
            self.call(&mut pass, call);
        }
        for (effect, offset) in self.effects.iter().zip(effects) {
            if let Some(offset) = offset {
                pass.set_pipeline(&effect.pipeline);
                pass.set_bind_group(0, &self.constants, &[offset]);
                pass.set_bind_group(1, &effect.group, &[]);
                pass.draw(0..effect.vertices, 0..effect.instances);
            }
        }
        if frame.logo {
            pass.set_pipeline(&self.badge.0);
            pass.set_bind_group(0, &self.constants, &[0]);
            pass.set_bind_group(1, &self.badge.1, &[]);
            pass.draw(0..6, 0..1);
        }
    }

    fn call(&self, pass: &mut wgpu::RenderPass<'_>, call: &Call) {
        let actor = &self.actors[call.actor];
        let shape = &actor.shapes[call.shape];
        let p = match (call.pass, &shape.fur) {
            (Ok(i), _) => &shape.passes[i],
            (Err(_), Some(fur)) => fur,
            (Err(_), None) => return,
        };
        let mesh = &actor.meshes[p.mesh];
        let (indices, count) = &actor.indices[mesh.geometry];
        let group = call
            .alternate
            .and_then(|i| p.alternates.get(i))
            .unwrap_or(&p.group);
        pass.set_pipeline(&self.pipelines[p.pipeline]);
        pass.set_bind_group(0, &self.constants, &[call.offset]);
        pass.set_bind_group(1, group, &[]);
        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..*count, 0, 0..1);
    }
}

/// OpenGL's projection, depths from -1 to 1. Wider than 4:3 shows more at the sides,
/// narrower more above and below.
fn projection(frame: &Frame, aspect: f32) -> Mat4 {
    let half = (frame.fov * 0.5).tan() * (4.0 / 3.0 / aspect).max(1.0);
    glam::camera::rh::proj::opengl::perspective(2.0 * half.atan(), aspect, frame.near, frame.far)
}

fn uniforms(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: u64,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("constants"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: wgpu::BufferSize::new(BLOCK),
            }),
        }],
    });
    (buffer, group)
}

/// The viewers draw a cube around the eye whose coordinates are its corners.
const SKY: &str = include_str!("../shaders/sky.wgsl");
const BADGE: &str = include_str!("../shaders/badge.wgsl");
