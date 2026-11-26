//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::VertexStepMode::{Instance, Vertex};

use crate::{
    Error,
    data::Files,
    dds, demo,
    gpu::{DEPTH, Gpu, SAMPLES},
    model::{Kind, Model, Vertex as MeshVertex},
    show::{Frame, Show},
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_proj: [f32; 16],
    inverse: [f32; 16],
    eye: [f32; 4],
    light: [f32; 4],
    parameters: [f32; 4],
    /// Axes of the camera.
    axes: [[f32; 4]; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Placement {
    transform: [f32; 16],
    diffuse: [f32; 4],
    ambient_mode: [f32; 4],
}

const MESH_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x2];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4,
                             7 => Float32x4, 8 => Float32x4, 9 => Float32x4];

struct Mesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    group: wgpu::BindGroup,
    count: u32,
    node: usize,
    kind: Kind,
    diffuse: [f32; 4],
    ambient_mode: [f32; 4],
}

struct Actor {
    model: Model,
    meshes: Vec<Mesh>,
    sky: wgpu::BindGroup,
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

pub struct Scene {
    actors: Vec<Actor>,
    uniforms: wgpu::Buffer,
    instances: wgpu::Buffer,
    constants: wgpu::BindGroup,
    badge: wgpu::BindGroup,
    surface: wgpu::RenderPipeline,
    glass: wgpu::RenderPipeline,
    /// The far wall of the ball.
    wall: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    snow: wgpu::RenderPipeline,
    overlay: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

/// The first model has the ball and its stand; the others are castles.
const BALL: usize = 0;
const LOGO: &str = "userinterface/nvidialogo.dds";

/// Two planes and two cube maps.
type Images = (Option<String>, Option<String>, [Option<String>; 2]);

struct ImagesLoader<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    layout: &'a wgpu::BindGroupLayout,
    images: HashMap<String, wgpu::TextureView>,
    groups: HashMap<Images, wgpu::BindGroup>,
}

fn rgba(files: &Files, name: &str) -> Result<((u32, u32), Vec<u8>), Error> {
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

impl ImagesLoader<'_> {
    fn image(&mut self, name: Option<&str>, cube: bool) -> Result<wgpu::TextureView, Error> {
        let key = name.unwrap_or(if cube { "#blank cube" } else { "#white" });
        if let Some(view) = self.images.get(key) {
            return Ok(view.clone());
        }
        let (size, pixels) = match (name, cube) {
            (Some(name), true) => {
                let decoded = self
                    .files
                    .faces(name)?
                    .iter()
                    .map(|face| rgba(self.files, face).map_err(|e| format!("{face}: {e}")))
                    .collect::<Result<Vec<_>, _>>()?;
                let size = decoded[0].0;
                if decoded.iter().any(|(s, _)| *s != size) {
                    return Err(format!("{name}: inconsistent cube face sizes").into());
                }
                (size, decoded.into_iter().map(|(_, p)| p).collect())
            }
            (Some(name), false) => {
                let (size, data) = rgba(self.files, name).map_err(|e| format!("{name}: {e}"))?;
                (size, vec![data])
            }
            (None, true) => ((1, 1), vec![vec![0, 0, 0, 255]; 6]),
            (None, false) => ((1, 1), vec![vec![255; 4]]),
        };
        let texture = self.gpu.upload_rgba(size, &pixels, true);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: cube.then_some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        self.images.insert(key.to_owned(), view.clone());
        Ok(view)
    }

    fn group(&mut self, key: Images) -> Result<wgpu::BindGroup, Error> {
        if let Some(group) = self.groups.get(&key) {
            return Ok(group.clone());
        }
        let views = [
            self.image(key.0.as_deref(), false)?,
            self.image(key.1.as_deref(), false)?,
            self.image(key.2[0].as_deref(), true)?,
            self.image(key.2[1].as_deref(), true)?,
        ];
        let entries = [0, 1, 2, 3].map(|i| wgpu::BindGroupEntry {
            binding: i,
            resource: wgpu::BindingResource::TextureView(&views[i as usize]),
        });
        let group = self
            .gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: self.layout,
                entries: &entries,
            });
        self.groups.insert(key, group.clone());
        Ok(group)
    }
}

fn entry(binding: u32, view_dimension: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        let device = &gpu.device;
        use wgpu::TextureViewDimension::{Cube, D2};
        let textures = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[entry(0, D2), entry(1, D2), entry(2, Cube), entry(3, Cube)],
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniforms = gpu.uniforms(1);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let constants = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipelines = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout), Some(&textures)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("NVIDIA viewer"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scene.wgsl").into()),
        });
        let pipeline = |vertex, fragment, buffers: &[_], blend, test, write, cull_mode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&pipelines),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(if test {
                        wgpu::CompareFunction::LessEqual
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let mesh_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<MeshVertex>() as u64,
                step_mode: Vertex,
                attributes: &MESH_ATTRIBUTES,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Placement>() as u64,
                step_mode: Instance,
                attributes: &INSTANCE_ATTRIBUTES,
            }),
        ];

        let mut loader = ImagesLoader {
            gpu,
            files,
            layout: &textures,
            images: HashMap::new(),
            groups: HashMap::new(),
        };
        let mut actors = Vec::new();
        for name in demo::MODELS {
            let model = files.model(name)?;
            let sky = loader.group((None, None, [model.environment.clone(), None]))?;
            let meshes = model
                .shapes
                .iter()
                .map(|shape| {
                    let [a, b] = shape.textures.clone();
                    // Over the glass lies what is around the ball.
                    let around = model.environment.clone();
                    let cubes = [
                        shape.cube.clone(),
                        around.filter(|_| shape.kind == Kind::Glass),
                    ];
                    let group = loader.group((a, b, cubes))?;
                    let mode = match shape.kind {
                        Kind::Lit => 0.0,
                        Kind::Textured => 1.0,
                        Kind::Bump => 2.0,
                        Kind::Reflect => 3.0,
                        Kind::Metal => 4.0,
                        Kind::Refract => 5.0,
                        Kind::Glass => 6.0,
                        Kind::Table => 7.0,
                    };
                    let mut ambient_mode = shape.ambient.to_array();
                    ambient_mode[3] = mode;
                    Ok::<_, Error>(Mesh {
                        vertices: gpu.buffer(
                            bytemuck::cast_slice(&shape.vertices),
                            wgpu::BufferUsages::VERTEX,
                        ),
                        indices: gpu.buffer(
                            bytemuck::cast_slice(&shape.indices),
                            wgpu::BufferUsages::INDEX,
                        ),
                        group,
                        node: shape.node,
                        kind: shape.kind,
                        count: shape.indices.len() as u32,
                        diffuse: shape.diffuse.to_array(),
                        ambient_mode,
                    })
                })
                .collect::<Result<_, _>>()?;
            actors.push(Actor { model, meshes, sky });
        }
        let capacity: usize = actors.iter().map(|a| a.meshes.len()).sum();
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (size_of::<Placement>() * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            actors,
            uniforms,
            instances,
            constants,
            format,
            targets: None,
            badge: loader.group((Some(LOGO.to_owned()), None, [None, None]))?,
            surface: pipeline(
                "surface_vs",
                "surface_fs",
                &mesh_buffers,
                None,
                true,
                true,
                None,
            ),
            // The more see-through it is, the less of it there is.
            glass: pipeline(
                "surface_vs",
                "surface_fs",
                &mesh_buffers,
                Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        dst_factor: wgpu::BlendFactor::SrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                true,
                false,
                Some(wgpu::Face::Back),
            ),
            wall: pipeline(
                "surface_vs",
                "surface_fs",
                &mesh_buffers,
                None,
                true,
                true,
                Some(wgpu::Face::Front),
            ),
            snow: pipeline(
                "snow_vs",
                "snow_fs",
                &[],
                Some(wgpu::BlendState::ALPHA_BLENDING),
                true,
                false,
                None,
            ),
            sky: pipeline("sky_vs", "sky_fs", &[], None, false, false, None),
            overlay: pipeline(
                "badge_vs",
                "badge_fs",
                &[],
                Some(wgpu::BlendState::ALPHA_BLENDING),
                false,
                false,
                None,
            ),
        })
    }

    pub fn show(&self) -> Show {
        Show::default()
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        frame: &Frame,
        window: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        if self
            .targets
            .as_ref()
            .is_none_or(|target| target.size != size)
        {
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
        let target = self.targets.as_ref().unwrap();
        let aspect = size.0 as f32 / size.1 as f32;
        let fov = 2.0 * (0.36 * (4.0 / 3.0 / aspect).max(1.0)).atan();
        let castle = 1 + frame.castle;
        let actor = &self.actors[if frame.inside { castle } else { BALL }];
        let view = frame.view;
        let eye = view.inverse().transform_point3(Vec3::ZERO);
        let proj = glam::camera::rh::proj::directx::perspective(fov, aspect, 0.15, 2500.0);
        let vp = proj * view;
        let light = match frame.castle {
            1 => Vec3::new(0.38, 0.9, -0.2),
            _ => Vec3::new(0.4, 0.85, 0.53),
        };
        let around = if frame.inside { 1.0 } else { 0.8 };
        let uniform = Uniform {
            view_proj: vp.to_cols_array(),
            inverse: vp.inverse().to_cols_array(),
            eye: eye.extend(1.0).to_array(),
            light: light.extend(0.0).to_array(),
            parameters: [frame.clear, around, aspect, frame.snow_time],
            axes: [0, 1, 2].map(|i| view.row(i).to_array()),
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));

        let mut data = Vec::new();
        let mut draws = Vec::new();
        // The castle stands in the ball.
        let shown: &[usize] = if frame.inside {
            &[castle]
        } else {
            &[BALL, castle]
        };
        for &index in shown {
            let actor = &self.actors[index];
            let pose = actor.model.pose(None, 0.0);
            for mesh in &actor.meshes {
                data.push(Placement {
                    transform: pose[mesh.node].to_cols_array(),
                    diffuse: mesh.diffuse,
                    ambient_mode: mesh.ambient_mode,
                });
                draws.push((mesh, data.len() - 1));
            }
        }
        gpu.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));
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
        pass.set_bind_group(0, &self.constants, &[]);
        pass.set_bind_group(1, &actor.sky, &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);
        let render = |pass: &mut wgpu::RenderPass<'_>, mesh: &Mesh, start: usize| {
            let group = match mesh.kind {
                // The ball's wall shows what is around the castle.
                Kind::Refract => &self.actors[castle].sky,
                _ => &mesh.group,
            };
            pass.set_bind_group(1, group, &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_vertex_buffer(
                1,
                self.instances
                    .slice((start * size_of::<Placement>()) as u64..),
            );
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
        };
        // Solid things, the far wall, snow, and the glass over it all.
        let solid = |k| !matches!(k, Kind::Glass | Kind::Refract);
        pass.set_pipeline(&self.surface);
        for &(mesh, start) in draws.iter().filter(|d| solid(d.0.kind)) {
            render(&mut pass, mesh, start);
        }
        pass.set_pipeline(&self.wall);
        for &(mesh, start) in draws.iter().filter(|d| d.0.kind == Kind::Refract) {
            render(&mut pass, mesh, start);
        }
        if frame.snow_time < 24.0 {
            pass.set_bind_group(1, &actor.sky, &[]);
            pass.set_pipeline(&self.snow);
            pass.draw(0..6, 0..2000);
        }
        pass.set_pipeline(&self.glass);
        for &(mesh, start) in draws.iter().filter(|d| d.0.kind == Kind::Glass) {
            render(&mut pass, mesh, start);
        }
        if frame.logo {
            pass.set_bind_group(1, &self.badge, &[]);
            pass.set_pipeline(&self.overlay);
            pass.draw(0..6, 0..1);
        }
    }
}
