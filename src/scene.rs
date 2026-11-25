//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::VertexStepMode::{Instance, Vertex};

use crate::{
    Error,
    data::Files,
    dds,
    demo::{self, DEMO, Demo},
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
    fur: Vec<wgpu::BindGroup>,
    skins: Vec<wgpu::BindGroup>,
}

struct Actor {
    model: Model,
    meshes: Vec<Mesh>,
    sky: wgpu::BindGroup,
    focus: (Vec3, f32),
    ground: Option<Ground>,
}

struct Ground {
    heights: Vec<f32>,
    columns: usize,
    rows: usize,
    min: Vec3,
    max: Vec3,
}

impl Ground {
    fn new(model: &Model) -> Option<Self> {
        let shape = model.shapes.iter().find(|s| s.geometry == "ground")?;
        let (first, last) = (shape.vertices.first()?, shape.vertices.last()?);
        let columns = shape
            .vertices
            .iter()
            .take_while(|v| v.pos[2] == first.pos[2])
            .count();
        let rows = shape.vertices.len() / columns;
        if columns < 2 || rows < 2 {
            return None;
        }
        let transform = model.pose(None, 0.0)[shape.node];
        Some(Self {
            columns,
            rows,
            min: transform.transform_point3(first.pos.into()),
            max: transform.transform_point3(last.pos.into()),
            heights: shape
                .vertices
                .iter()
                .map(|v| transform.transform_point3(v.pos.into()).y)
                .collect(),
        })
    }

    fn height(&self, x: f32, z: f32) -> f32 {
        let tx = ((x - self.min.x) / (self.max.x - self.min.x)).clamp(0.0, 1.0)
            * (self.columns - 1) as f32;
        let tz =
            ((z - self.min.z) / (self.max.z - self.min.z)).clamp(0.0, 1.0) * (self.rows - 1) as f32;
        let (x, z) = (tx.floor() as usize, tz.floor() as usize);
        let (next_x, next_z) = ((x + 1).min(self.columns - 1), (z + 1).min(self.rows - 1));
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let a = lerp(
            self.heights[z * self.columns + x],
            self.heights[z * self.columns + next_x],
            tx.fract(),
        );
        let b = lerp(
            self.heights[next_z * self.columns + x],
            self.heights[next_z * self.columns + next_x],
            tx.fract(),
        );
        lerp(a, b, tz.fract())
    }
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

struct Morph {
    clip: usize,
    tracks: Vec<(String, Vec<MorphKey>)>,
    animation: Option<crate::model::Animation>,
}

struct MorphKey {
    ms: u32,
    positions: Vec<f32>,
    normals: Vec<f32>,
}

impl Morph {
    fn new(clip: usize, bytes: &[u8], model: &Model) -> Result<Self, Error> {
        let root = crate::tree::read_binary(bytes)?;
        let animation = root.nodes("animations").first();
        let mut tracks = Vec::new();
        if let Some(node) = animation {
            for channel in node
                .nodes("channels")
                .iter()
                .filter(|c| c.kind == "MeshChannel")
            {
                let keys = channel
                    .nodes("curves")
                    .iter()
                    .map(|key| MorphKey {
                        ms: key.int("time"),
                        positions: key.floats("coords").to_vec(),
                        normals: key.floats("normals").to_vec(),
                    })
                    .collect();
                tracks.push((channel.text("targetNode").to_owned(), keys));
            }
        }
        let animation = animation
            .map(|node| crate::model::Animation::new(model, crate::anim::read(node)?))
            .transpose()?;
        Ok(Self {
            clip,
            tracks,
            animation,
        })
    }

    fn deform(&self, target: &str, ms: u32, vertices: &mut [MeshVertex]) {
        let Some((_, keys)) = self.tracks.iter().find(|(name, _)| name == target) else {
            return;
        };
        if keys.is_empty() {
            return;
        }
        let index = keys.partition_point(|k| k.ms < ms);
        let a = &keys[index.saturating_sub(1)];
        let b = keys.get(index).unwrap_or(a);
        if [a, b].iter().any(|key| {
            key.positions.len() != vertices.len() * 3 || key.normals.len() != vertices.len() * 3
        }) {
            return;
        }
        let t = (ms.saturating_sub(a.ms) as f32 / b.ms.saturating_sub(a.ms).max(1) as f32).min(1.0);
        for (i, vertex) in vertices.iter_mut().enumerate() {
            let v = |values: &[f32]| Vec3::from_slice(&values[i * 3..]);
            vertex.pos = v(&a.positions).lerp(v(&b.positions), t).to_array();
            vertex.normal = v(&a.normals)
                .lerp(v(&b.normals), t)
                .normalize_or_zero()
                .to_array();
        }
    }
}

pub struct Scene {
    actors: Vec<Actor>,
    uniforms: wgpu::Buffer,
    instances: wgpu::Buffer,
    constants: wgpu::BindGroup,
    badge: wgpu::BindGroup,
    surface: wgpu::RenderPipeline,
    alpha: wgpu::RenderPipeline,
    depth_vision: Option<wgpu::RenderPipeline>,
    silt: Option<wgpu::RenderPipeline>,
    bubbles: Option<wgpu::RenderPipeline>,
    sky: wgpu::RenderPipeline,
    overlay: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
    clips: Vec<Vec<u8>>,
    morph: Option<Morph>,
}

type Images = (Option<String>, Option<String>, Option<String>);

struct ImagesLoader<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    layout: &'a wgpu::BindGroupLayout,
    images: HashMap<String, wgpu::TextureView>,
    groups: HashMap<Images, wgpu::BindGroup>,
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
            self.image(key.2.as_deref(), true)?,
        ];
        let entries = [0, 1, 2].map(|i| wgpu::BindGroupEntry {
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
            entries: &[entry(0, D2), entry(1, D2), entry(2, Cube)],
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
                    depth_compare: Some(if vertex == "shadow_vs" {
                        wgpu::CompareFunction::Less
                    } else if test {
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
            if model.shapes.is_empty() {
                return Err(format!("{name}: no drawable shapes").into());
            }
            let fallback = if DEMO == Demo::Tidepool {
                Some("textures/sunol.ifl".to_owned())
            } else {
                None
            };
            let sky = loader.group((None, None, model.environment.clone().or(fallback)))?;
            let meshes = model
                .shapes
                .iter()
                .map(|shape| {
                    let [a, b] = shape.textures.clone();
                    let group = loader.group((a.clone(), b.clone(), shape.cube.clone()))?;
                    let mut skins = Vec::new();
                    if DEMO == Demo::Chameleon
                        && let Some(path) = &a
                        && let Some(prefix) = path.strip_suffix("_red.dds")
                    {
                        for color in ["green", "blue", "real", "xray"] {
                            let alternate = format!("{prefix}_{color}.dds");
                            let variant = files
                                .get(&alternate)
                                .is_ok()
                                .then_some(alternate)
                                .or(a.clone());
                            skins.push(loader.group((variant, b.clone(), shape.cube.clone()))?);
                        }
                    }
                    let fur = shape
                        .fur
                        .iter()
                        .map(|image| loader.group((a.clone(), Some(image.clone()), None)))
                        .collect::<Result<_, _>>()?;
                    Ok::<_, Error>(Mesh {
                        vertices: gpu.buffer(
                            bytemuck::cast_slice(&shape.vertices),
                            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        ),
                        indices: gpu.buffer(
                            bytemuck::cast_slice(&shape.indices),
                            wgpu::BufferUsages::INDEX,
                        ),
                        group,
                        fur,
                        skins,
                    })
                })
                .collect::<Result<_, _>>()?;
            let focus = focus(&model);
            let ground = Ground::new(&model);
            actors.push(Actor {
                model,
                meshes,
                sky,
                focus,
                ground,
            });
        }
        let clips = demo::CLIPS
            .iter()
            .map(|name| files.get(&format!("models/{name}")).map(<[u8]>::to_vec))
            .collect::<Result<Vec<_>, _>>()?;
        let max_instances: usize = actors
            .iter()
            .map(|actor| {
                actor
                    .model
                    .shapes
                    .iter()
                    .map(|s| 1 + s.fur.len())
                    .sum::<usize>()
            })
            .sum::<usize>()
            * 65;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (size_of::<Placement>() * max_instances) as u64,
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
            clips,
            morph: None,
            badge: loader.group((Some("userinterface/nvidialogo.dds".to_owned()), None, None))?,
            surface: pipeline(
                "surface_vs",
                "surface_fs",
                &mesh_buffers,
                None,
                true,
                true,
                None,
            ),
            alpha: pipeline(
                "surface_vs",
                "surface_fs",
                &mesh_buffers,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                true,
                false,
                None,
            ),
            depth_vision: (DEMO == Demo::Bugs).then(|| {
                pipeline(
                    "surface_vs",
                    "overdraw_fs",
                    &mesh_buffers,
                    Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    false,
                    false,
                    None,
                )
            }),
            silt: (DEMO == Demo::Squid).then(|| {
                pipeline(
                    "silt_vs",
                    "silt_fs",
                    &[],
                    Some(wgpu::BlendState::ALPHA_BLENDING),
                    true,
                    false,
                    None,
                )
            }),
            bubbles: (DEMO == Demo::Squid).then(|| {
                pipeline(
                    "bubble_vs",
                    "bubble_fs",
                    &[],
                    Some(wgpu::BlendState::ALPHA_BLENDING),
                    true,
                    false,
                    None,
                )
            }),
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
        let (center, radius) = self.actors[0].focus;
        let center = self.actors[0].ground.as_ref().map_or(center, |g| {
            Vec3::new(center.x, g.height(center.x, center.z) + 15.0, center.z)
        });
        Show::new(center, radius)
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
        if let Some(bytes) = self.clips.get(frame.item)
            && self.morph.as_ref().is_none_or(|m| m.clip != frame.item)
        {
            self.morph = Morph::new(frame.item, bytes, &self.actors[0].model).ok();
        }
        let target = self.targets.as_ref().unwrap();
        let aspect = size.0 as f32 / size.1 as f32;
        let fov = 2.0 * (0.36 * (4.0 / 3.0 / aspect).max(1.0)).atan();
        let eye = frame.view.inverse().transform_point3(Vec3::ZERO);
        let vp =
            glam::camera::rh::proj::directx::perspective(fov, aspect, 0.1, 16000.0) * frame.view;
        let light = Vec3::new(0.45, 0.8, 0.5);
        let uniform = Uniform {
            view_proj: vp.to_cols_array(),
            inverse: vp.inverse().to_cols_array(),
            eye: eye.extend(1.0).to_array(),
            light: light.extend(0.0).to_array(),
            parameters: [
                frame.seconds,
                u32::from(frame.ripple) as f32,
                aspect,
                DEMO as u32 as f32,
            ],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniform));

        let mut data = Vec::new();
        let mut draws = Vec::new();
        for (index, actor) in self.actors.iter().enumerate() {
            let placements = placements(index, frame, eye, self.actors[0].ground.as_ref());
            if placements.is_empty() {
                continue;
            }
            let clock = if DEMO == Demo::Wolfman {
                let starts = [0.0, 5.4, 10.1, 15.8, 21.5, 26.0];
                let start = starts[frame.item.min(5)];
                let end = starts.get(frame.item + 1).copied().unwrap_or(32.2);
                start + frame.local.rem_euclid(end - start)
            } else if DEMO == Demo::Zoltar {
                frame.local
            } else {
                frame.seconds
            };
            let time = f64::from(clock) * 1000.0;
            let clip = self
                .morph
                .as_ref()
                .and_then(|m| m.animation.as_ref())
                .or_else(|| actor.model.animations.first());
            let mut pose = actor.model.pose(clip, time);
            if DEMO == Demo::Bugs && index > 0 {
                let rest = pose.clone();
                for (j, node) in actor.model.nodes.iter().enumerate() {
                    let name = node.name.to_ascii_lowercase();
                    if name.contains("leg") || name.contains("hip") || name.contains("thigh") {
                        let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
                        pose[j] = rest[j]
                            * Mat4::from_rotation_z(
                                sign * 0.13 * (frame.seconds * 16.0 + j as f32).sin(),
                            );
                    }
                }
            }
            for (shape, mesh) in actor.model.shapes.iter().zip(&actor.meshes) {
                if shape.skin.is_some() || (self.morph.is_some() && index == 0) {
                    let mut vertices = shape.vertices.clone();
                    if let Some(skin) = &shape.skin {
                        skin.deform(&mut vertices, &pose);
                    }
                    if let Some(morph) = &self.morph {
                        morph.deform(&shape.geometry, time as u32, &mut vertices);
                    }
                    gpu.queue
                        .write_buffer(&mesh.vertices, 0, bytemuck::cast_slice(&vertices));
                }
                let start = data.len();
                let diffuse = shape.diffuse;
                let group = frame
                    .item
                    .checked_sub(1)
                    .and_then(|i| mesh.skins.get(i))
                    .unwrap_or(&mesh.group);
                for world in &placements {
                    data.push(Placement {
                        transform: (*world * pose[shape.node]).to_cols_array(),
                        diffuse: diffuse.to_array(),
                        ambient_mode: shape
                            .ambient
                            .truncate()
                            .extend(shape.kind as u32 as f32)
                            .to_array(),
                    });
                }
                let count = placements.len();
                draws.push((
                    mesh,
                    group,
                    shape.indices.len() as u32,
                    start,
                    count as u32,
                    shape.kind == Kind::Water,
                ));
                let fur_count = mesh.fur.len();
                for (layer, group) in mesh.fur.iter().take(frame.layers as usize).enumerate() {
                    let start = data.len();
                    let offset = shape.fur_depth * (layer + 1) as f32 / fur_count as f32;
                    for world in &placements {
                        data.push(Placement {
                            transform: (*world * pose[shape.node]).to_cols_array(),
                            diffuse: diffuse.to_array(),
                            ambient_mode: [offset, 0.0, 0.0, Kind::Fur as u32 as f32],
                        });
                    }
                    draws.push((
                        mesh,
                        group,
                        shape.indices.len() as u32,
                        start,
                        count as u32,
                        true,
                    ));
                }
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
        pass.set_bind_group(1, &self.actors[0].sky, &[]);
        pass.set_pipeline(&self.sky);
        if !frame.depth_vision {
            pass.draw(0..3, 0..1);
        }
        for alpha in [false, true] {
            let surface = if alpha { &self.alpha } else { &self.surface };
            let chosen = self
                .depth_vision
                .as_ref()
                .filter(|_| frame.depth_vision)
                .unwrap_or(surface);
            pass.set_pipeline(chosen);
            for &(mesh, group, index_count, start, count, blend) in &draws {
                if blend != alpha {
                    continue;
                }
                pass.set_bind_group(1, group, &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(
                    1,
                    self.instances
                        .slice((start * size_of::<Placement>()) as u64..),
                );
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..index_count, 0, 0..count);
            }
        }
        if let Some(silt) = &self.silt {
            pass.set_bind_group(1, &self.actors[0].sky, &[]);
            pass.set_pipeline(silt);
            pass.draw(0..6, 0..5120);
        }
        if let Some(bubbles) = &self.bubbles {
            pass.set_pipeline(bubbles);
            pass.draw(0..6, 0..1000);
        }
        if frame.logo {
            pass.set_bind_group(1, &self.badge, &[]);
            pass.set_pipeline(&self.overlay);
            pass.draw(0..6, 0..1);
        }
    }
}

fn focus(model: &Model) -> (Vec3, f32) {
    let pose = model.pose(None, 0.0);
    let featured = |s: &crate::model::Shape| match DEMO {
        Demo::Wolfman => {
            s.name.contains("Torso") || s.name.contains("Face") || s.name.contains("Leg")
        }
        Demo::Chameleon => s.name.starts_with("Cham"),
        Demo::Zoltar => s.name.starts_with("FaceGold"),
        Demo::Squid => s.name.starts_with("Main_outer") || s.name.contains("tentacle"),
        Demo::Tidepool => s.name.starts_with("TidePool") || s.name.starts_with("Water_grid"),
        Demo::Bugs => false,
    };
    let selected = model.shapes.iter().filter(|s| featured(s)).flat_map(|s| {
        s.vertices
            .iter()
            .map(|v| pose[s.node].transform_point3(v.pos.into()))
    });
    let (mut min, mut max) = selected.fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), p| (min.min(p), max.max(p)),
    );
    if !min.is_finite() {
        (min, max) = model.bounds();
    }
    if DEMO == Demo::Bugs {
        let first = &model.shapes[0];
        let height = first
            .vertices
            .iter()
            .min_by(|a, b| {
                (a.pos[0].abs() + a.pos[2].abs()).total_cmp(&(b.pos[0].abs() + b.pos[2].abs()))
            })
            .map_or(0.0, |v| v.pos[1]);
        return (Vec3::new(0.0, height + 14.0, 0.0), 53.0);
    }
    ((min + max) * 0.5, ((max - min).length() * 0.5).max(1.0))
}

fn placements(index: usize, frame: &Frame, eye: Vec3, ground: Option<&Ground>) -> Vec<Mat4> {
    let time = frame.seconds;
    match (DEMO, index) {
        (Demo::Bugs, 0) => vec![Mat4::IDENTITY],
        (Demo::Bugs, model) => (0..60)
            .filter_map(|i| {
                let turn = i as f32 * 2.399_963 + time * if frame.item == 0 { 0.18 } else { 0.62 };
                let x = ((i % 10) as f32 - 4.5) * 46.0 + turn.cos() * 11.0;
                let z = ((i / 10) as f32 - 2.5) * 43.0 + turn.sin() * 11.0;
                let p = Vec3::new(x, ground.map_or(0.0, |g| g.height(x, z)), z);
                let d = eye.distance(p);
                let lod = 1 + [60.0, 180.0, 400.0].partition_point(|cut| d > *cut);
                (model == lod).then(|| Mat4::from_translation(p) * Mat4::from_rotation_y(-turn))
            })
            .collect(),
        (Demo::Squid, 0) => vec![Mat4::from_translation(frame.steering)],
        (Demo::Squid, i) => {
            let count = match i {
                1 => 14,
                2 => 24,
                3 => 12,
                _ => 0,
            };
            (0..count)
                .map(|k| {
                    let f = k as f32;
                    let x = (f * 13.73).sin() * 88.0
                        + if i == 2 {
                            (time * 0.45 + f).sin() * 18.0
                        } else {
                            0.0
                        };
                    let y = if i == 1 {
                        -23.0
                    } else {
                        -17.0 + (f * 4.1).sin() * 13.0
                    };
                    let z =
                        (f * 7.91).cos() * 90.0 + if i == 3 { time.rem_euclid(9.0) } else { 0.0 };
                    Mat4::from_translation(Vec3::new(x, y, z)) * Mat4::from_rotation_y(f * 1.6)
                })
                .collect()
        }
        _ => vec![Mat4::IDENTITY],
    }
}
