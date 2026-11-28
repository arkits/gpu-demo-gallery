//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use glam::{Mat4, Vec3};

use crate::Error;
use crate::camera_collision::{Obstacle, keep_clear};
use crate::data::Files;
use crate::gpu::{DEPTH, Gpu, SAMPLES, SLOT};
use crate::model::{Fixed, Geometry, Model, Moving, Pass, Shape};
use crate::show::{Camera, Frame, Preset, Shot, Show};
use crate::{dds, demo, hair, tga, tree};

/// Bone matrices in the shader.
const BONES: usize = 96;
/// Uniform offsets are multiples of this.
const STRIDE: u64 = (BONES as u64 * 64).next_multiple_of(SLOT);

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Uniforms {
    view: [[f32; 4]; 4],
    projection: [[f32; 4]; 4],
    eye: [f32; 4],
    time: [f32; 4],
    /// Up to eight lights: color and intensity, then position and eyespace flag.
    lights: [[f32; 4]; 16],
}

enum Buffers {
    Mesh {
        moving: wgpu::Buffer,
        fixed: wgpu::Buffer,
        indices: wgpu::Buffer,
        count: u32,
        /// The morph targets now in `moving`.
        loaded: Vec<(usize, f32)>,
    },
    Hair {
        points: wgpu::Buffer,
        count: u32,
    },
    Particles {
        points: wgpu::Buffer,
        count: u32,
    },
}

struct GpuShape {
    buffers: Buffers,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

struct Actor {
    model: Model,
    shapes: Vec<GpuShape>,
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
}

pub struct Scene {
    /// The world and Mike.
    world: Actor,
    /// The scalp that the hair grows on.
    scalp: Actor,
    /// Menu cards, particles and other overlays.
    overlays: Vec<Actor>,
    /// Grown hair and goatee.
    strands: Actor,
    takes: Vec<crate::anim::Animation>,
    movie_rigs: Vec<Option<usize>>,
    visible_in_take: Vec<Vec<bool>>,
    camera_obstacles: Vec<Obstacle>,
    uniforms: wgpu::Buffer,
    bones: wgpu::Buffer,
    constants: wgpu::BindGroup,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
    fallback_presets: Vec<Option<Preset>>,
    menu: Menu,
    menu_pipeline: wgpu::RenderPipeline,
    moon: wgpu::BindGroup,
    moon_pipeline: wgpu::RenderPipeline,
}

struct MenuBatch {
    group: wgpu::BindGroup,
    start: u32,
    count: u32,
}

struct Menu {
    points: wgpu::Buffer,
    batches: Vec<MenuBatch>,
}

/// Loads each texture once.
struct Loader<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    views: HashMap<String, wgpu::TextureView>,
    samplers: HashMap<[bool; 2], wgpu::Sampler>,
    layout: wgpu::BindGroupLayout,
}

impl Loader<'_> {
    fn view(&mut self, file: Option<&str>) -> Result<wgpu::TextureView, Error> {
        let file = file.unwrap_or("");
        if let Some(v) = self.views.get(file) {
            return Ok(v.clone());
        }
        let view = if file.is_empty() {
            self.gpu.upload_rgba((1, 1), &[vec![0; 4]], false)
        } else {
            let data = self
                .files
                .get(file)
                .map_err(|_| format!("missing texture {file}"))?;
            if file.to_ascii_lowercase().ends_with(".dds") {
                self.gpu.upload(&[dds::Image::read(data)?])?
            } else if file.to_ascii_lowercase().ends_with(".t3d") {
                volume(self.gpu, data)?
            } else {
                let (w, h, pixels) = tga::read(data)?;
                self.gpu.upload_rgba((w as u32, h as u32), &[pixels], true)
            }
        };
        let view = view.create_view(&Default::default());
        self.views.insert(file.to_owned(), view.clone());
        Ok(view)
    }

    fn group(&mut self, planes: Vec<wgpu::TextureView>) -> wgpu::BindGroup {
        let sampler = self.samplers.entry([true; 2]).or_insert_with(|| {
            self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        });
        let mut entries: Vec<_> = planes
            .iter()
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: wgpu::BindingResource::TextureView(v),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::Sampler(sampler),
        });
        self.gpu
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &entries,
            })
    }

    fn material(&mut self, model: &Model, shape: &Shape) -> Result<wgpu::BindGroup, Error> {
        use crate::model::Texture as Tex;
        // Roles by parameter name: albedo, bump, detail, and blank for the
        // environment and shadow maps the port does not sample.
        let pick = |wants: &[&str]| wants.iter().find_map(|w| shape.textures.get(*w));
        let file = |name: Option<&String>| match name.and_then(|n| model.textures.get(n)) {
            Some(Tex::Plane { file, .. }) | Some(Tex::Volume { file }) => Some(file.as_str()),
            _ => None,
        };
        let t0 = file(pick(&["colorTex"]).or_else(|| shape.textures.values().next()));
        // Room shaders use the bump map with shadow and radiance passes that
        // are unavailable in the port. Use the mesh normal for this pass.
        let t1 = if shape.fragment.starts_with("room_") {
            None
        } else {
            file(pick(&["bumpTex", "detailBumpTex", "faceNormalCubeTex"]))
        };
        let t2 = file(pick(&["detailColTex", "ambTex", "miscTex"]));
        let (v0, v1, v2, v3) = (
            self.view(t0)?,
            self.view(t1)?,
            self.view(t2)?,
            self.view(None)?,
        );
        Ok(self.group(vec![v0, v1, v2, v3]))
    }
}

/// A `.t3d` smoke volume: 64 frames of 64 by 64 bytes.
fn volume(gpu: &Gpu, data: &[u8]) -> Result<wgpu::Texture, Error> {
    if data.len() != 16 + 64 * 64 * 64 {
        return Err("bad volume".into());
    }
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 64,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data[16..],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(64),
            rows_per_image: Some(64),
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 64,
        },
    );
    Ok(texture)
}

fn samples(coverage: bool) -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: SAMPLES,
        alpha_to_coverage_enabled: coverage,
        ..Default::default()
    }
}

struct Pipelines<'a> {
    gpu: &'a Gpu,
    layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    made: Vec<(String, wgpu::RenderPipeline)>,
}

/// How to draw something.
struct Style<'a> {
    vertex: &'a str,
    fragment: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    blend: crate::model::Blend,
    cull: bool,
    depth: Option<bool>,
}

impl Pipelines<'_> {
    fn make(&mut self, s: Style<'_>) -> wgpu::RenderPipeline {
        let key = format!("{} {} {} {}", s.vertex, s.fragment, s.blend as u8, s.cull);
        if let Some((_, p)) = self.made.iter().find(|m| m.0 == key) {
            return p.clone();
        }
        let component = |src, dst| wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        };
        use crate::model::Blend;
        use wgpu::BlendFactor::{One, OneMinusSrcAlpha, SrcAlpha};
        let both = |c| wgpu::BlendState { color: c, alpha: c };
        let pipeline = self
            .gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(s.fragment),
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some(s.vertex),
                    compilation_options: Default::default(),
                    buffers: s.buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(s.fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.format,
                        blend: match s.blend {
                            Blend::None => None,
                            Blend::Alpha => Some(both(component(SrcAlpha, OneMinusSrcAlpha))),
                            Blend::Add => Some(both(component(One, One))),
                        },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: s.cull.then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(s.depth.is_some()),
                    depth_compare: Some(match s.depth {
                        Some(_) => wgpu::CompareFunction::LessEqual,
                        None => wgpu::CompareFunction::Always,
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: samples(false),
                multiview_mask: None,
                cache: None,
            });
        self.made.push((key, pipeline.clone()));
        pipeline
    }

    fn shape(&mut self, shape: &Shape) -> wgpu::RenderPipeline {
        let fixed = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x2, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
        let moving = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let segment = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x2,
            4 => Float32x3, 5 => Float32x3, 6 => Float32x3, 7 => Float32x2
        ];
        let seed = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        fn layout(
            attributes: &[wgpu::VertexAttribute],
            stride: usize,
        ) -> Option<wgpu::VertexBufferLayout<'_>> {
            Some(wgpu::VertexBufferLayout {
                array_stride: stride as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes,
            })
        }
        let particle = Some(wgpu::VertexBufferLayout {
            array_stride: 28,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &seed,
        });
        let (vertex, fragment, buffers) = match &shape.geometry {
            Geometry::Hair(_) => (
                "vs_hair",
                "fs_hair",
                vec![Some(wgpu::VertexBufferLayout {
                    array_stride: 88,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &segment,
                })],
            ),
            Geometry::Mesh(_) | Geometry::Cage(_) if shape.system.is_some() => {
                ("vs_particle", "fs_particle", vec![particle])
            }
            _ => (
                "vs_main",
                if shape.fragment.starts_with("mike_wetball") {
                    "fs_wetball"
                } else {
                    match shape.pass {
                        Pass::Card => "fs_card",
                        _ => "fs_main",
                    }
                },
                vec![
                    layout(&moving, size_of::<Moving>()),
                    layout(&fixed, size_of::<Fixed>()),
                ],
            ),
        };
        // The wet cornea layer blends over the eyeball; the file leaves
        // it on the default (opaque) blend.
        let blend = if shape.fragment.starts_with("mike_wetball") {
            crate::model::Blend::Alpha
        } else {
            shape.blend
        };
        self.make(Style {
            vertex,
            fragment,
            buffers: &buffers,
            blend,
            cull: shape.cull,
            depth: Some(true),
        })
    }
}

impl Actor {
    fn new(
        gpu: &Gpu,
        model: Model,
        loader: &mut Loader<'_>,
        pipelines: &mut Pipelines<'_>,
        emitters: &HashMap<String, Vec3>,
    ) -> Result<Self, Error> {
        let mut shapes = Vec::new();
        for shape in &model.shapes {
            use wgpu::BufferUsages as Use;
            let buffers = match &shape.geometry {
                Geometry::Mesh(_) if shape.system.is_some() => {
                    // A particle system (see `Model::new`): billboards
                    // around the emitter's rest position.
                    let at = shape
                        .system
                        .as_ref()
                        .and_then(|s| emitters.get(s))
                        .copied()
                        .unwrap_or(Vec3::ZERO);
                    let mut data: Vec<f32> = Vec::with_capacity(256 * 7);
                    let mut s = 12345u32;
                    for _ in 0..256 {
                        data.extend(at.to_array());
                        for _ in 0..4 {
                            s = s.wrapping_mul(214_013).wrapping_add(2_531_011);
                            data.push((s >> 16 & 0x7fff) as f32 / 32767.0);
                        }
                    }
                    Buffers::Particles {
                        points: gpu.buffer(bytemuck::cast_slice(&data), Use::VERTEX),
                        count: 256,
                    }
                }
                Geometry::Mesh(m) | Geometry::Cage(m) => Buffers::Mesh {
                    moving: gpu.buffer(
                        bytemuck::cast_slice(&m.morphed(&[])),
                        Use::VERTEX | Use::COPY_DST,
                    ),
                    fixed: gpu.buffer(bytemuck::cast_slice(&m.fixed), Use::VERTEX),
                    indices: gpu.buffer(bytemuck::cast_slice(&m.indices), Use::INDEX),
                    count: m.indices.len() as u32,
                    loaded: Vec::new(),
                },
                Geometry::Hair(h) => {
                    let mut data: Vec<f32> = Vec::new();
                    for pair in h.points.chunks(h.per_hair).flat_map(|s| s.windows(2)) {
                        for p in pair {
                            data.extend(p.position.to_array());
                            data.extend(p.tangent.to_array());
                            data.extend(p.normal.to_array());
                            data.extend(p.color);
                        }
                    }
                    Buffers::Hair {
                        points: gpu.buffer(bytemuck::cast_slice(&data), Use::VERTEX),
                        count: (data.len() / 22) as u32,
                    }
                }
            };
            if shape.bones.len() > BONES {
                return Err(format!("{}: too many bones", shape.name).into());
            }
            let pipeline = pipelines.shape(shape);
            let group = loader
                .material(&model, shape)
                .map_err(|e| format!("{}: {e}", shape.name))?;
            shapes.push(GpuShape {
                buffers,
                pipeline,
                group,
            });
        }
        Ok(Self { model, shapes })
    }
}

/// A shape to draw: actor, shape, and where its bones are.
struct Call {
    actor: usize,
    shape: usize,
    backbone: usize,
    distance: f32,
    transparent: bool,
}

// The game sets the movie lens to 59 degrees at 5:4 and scales it by height/width.
fn movie_fov(aspect: f32) -> f32 {
    let reference = 59_f32.to_radians() / (0.8 * aspect.max(4.0 / 3.0));
    if aspect < 4.0 / 3.0 {
        // Preserve the 4:3 horizontal framing when the window is narrow.
        2.0 * ((reference * 0.5).tan() * (4.0 / 3.0) / aspect).atan()
    } else {
        reference
    }
}

fn visible(model: &Model, shape: &Shape, take: &demo::Take) -> bool {
    use demo::Set;
    model.ancestors(shape.transform).all(|name| match name {
        "bedroom_set" | "bedroom_set_animated" => {
            matches!(take.set, Set::OldBedroom | Set::NewBedroom)
        }
        "workshop_set" | "workshop_set_animated" => take.set == Set::Workshop,
        "OldComputer" | "OldComputer_HI" | "MMB_COMP_Old" => take.set == Set::OldBedroom,
        "NewComputer" | "NewComputer_HI" | "NewCompRot" => take.set == Set::NewBedroom,
        "GrinderGroup" => take.grinder,
        "MMB_Welder" => take.welder,
        "MMB_DoorSwing_C" | "MMB_Door" => take.door,
        "MMB_NewCompCardAsm" => take.graphics_card,
        name if name.starts_with("MMB_DoorHinge") => take.hinges,
        _ => true,
    })
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        let device = &gpu.device;
        let world = Model::new(&files.binary(demo::GEOM)?, files)?;
        let scalp = Model::new(&files.binary(demo::HAIR_MESH)?, files)?;
        let grown = grow(files)?;
        let mut takes = Vec::new();
        for take in demo::TAKES {
            let root = files.binary(take.file)?;
            let a = root
                .nodes("animations")
                .first()
                .ok_or_else(|| format!("{}: no animation", take.file))?;
            takes.push(crate::anim::Animation::read(a)?);
        }
        let movie_rigs: Vec<_> = demo::TAKES
            .iter()
            .map(|take| world.find(take.camera))
            .collect();
        let camera_obstacles = Obstacle::from_model(&world);
        let visible_in_take = demo::TAKES
            .iter()
            .map(|take| {
                world
                    .shapes
                    .iter()
                    .map(|shape| visible(&world, shape, take))
                    .collect()
            })
            .collect();
        let presets = presets(files).unwrap_or_default();
        let fallback_presets = demo::TAKES
            .iter()
            .zip(&takes)
            .zip(&movie_rigs)
            .map(|((take, animation), rig)| {
                if rig.is_some() {
                    return None;
                }
                let seconds = take.start + 2.0 * f64::from(animation.scale);
                let pose = world.pose(Some((animation, seconds)), &[]);
                let hip = world
                    .find("MMM01_c_hip01_xx01_jjj")
                    .map(|at| pose[at].w_axis.truncate())
                    .unwrap_or(Vec3::new(0.0, 150.0, 0.0));
                presets.iter().copied().min_by(|a, b| {
                    a.look
                        .distance_squared(hip)
                        .total_cmp(&b.look.distance_squared(hip))
                })
            })
            .collect();
        // Cards and post-processing quads are in screen space.
        let hula = Model::new(&files.scene(demo::HULA)?, files)?;
        let buffer = |dynamic, size| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: std::num::NonZero::new(size),
        };
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty,
            count: None,
        };
        let frame_size = size_of::<Uniforms>() as u64;
        let first = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(0, buffer(false, frame_size)),
                entry(1, buffer(true, BONES as u64 * 64)),
            ],
        });
        let mut textures: Vec<_> = (0..4)
            .map(|i| {
                entry(
                    i,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                )
            })
            .collect();
        textures.push(entry(
            4,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        ));
        for t in &mut textures {
            t.visibility = wgpu::ShaderStages::FRAGMENT;
        }
        let second = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &textures,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&first), Some(&second)],
            ..Default::default()
        });
        let mut pipelines = Pipelines {
            gpu,
            layout,
            shader: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/mad-mod-mike.wgsl").into(),
                ),
            }),
            format,
            made: Vec::new(),
        };
        let menu_pipeline = pipelines.make(Style {
            vertex: "vs_menu",
            fragment: "fs_card",
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
            })],
            blend: crate::model::Blend::Alpha,
            cull: false,
            depth: None,
        });
        let moon_pipeline = pipelines.make(Style {
            vertex: "vs_card",
            fragment: "fs_moon",
            buffers: &[],
            blend: crate::model::Blend::None,
            cull: false,
            depth: None,
        });
        let mut loader = Loader {
            gpu,
            files,
            views: HashMap::new(),
            samplers: HashMap::new(),
            layout: second,
        };
        // Particle emitters live in the world; the systems in `particles.nma`.
        let rest = world.rest();
        let mut emitters = HashMap::new();
        if let Ok(particles) = files.scene(demo::PARTICLES) {
            for g in particles.nodes("geometries") {
                if g.kind != "ParticleSystem" {
                    continue;
                }
                let mut at = Vec3::ZERO;
                for e in g.nodes("emitters") {
                    if let Some(i) = world.find(e.text("transform")) {
                        at = rest[i].transform_point3(Vec3::ZERO);
                        break;
                    }
                }
                emitters.insert(g.name.clone(), at);
            }
        }
        let empty: HashMap<String, Vec3> = HashMap::new();
        let world = Actor::new(gpu, world, &mut loader, &mut pipelines, &empty)?;
        let scalp = Actor::new(gpu, scalp, &mut loader, &mut pipelines, &empty)?;
        let overlay_actors = vec![Actor::new(
            gpu,
            hula,
            &mut loader,
            &mut pipelines,
            &emitters,
        )?];
        let strands = Actor::new(gpu, grown, &mut loader, &mut pipelines, &empty)?;
        let menu_model = Model::new(&files.scene(demo::MENU)?, files)?;
        let menu = menu_batches(&mut loader, files, &menu_model)?;
        let moon = loader.view(Some("textures/moon_bkg.dds"))?;
        let blank = loader.view(None)?;
        let moon = loader.group(vec![moon, blank.clone(), blank.clone(), blank]);

        let slots: usize = [&world, &scalp]
            .into_iter()
            .chain(overlay_actors.iter())
            .chain([&strands])
            .map(|a| a.shapes.len())
            .sum();
        let make = |size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniforms = make(frame_size.next_multiple_of(SLOT));
        let bones = make(STRIDE * slots as u64);
        let part = |buffer, size| {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: std::num::NonZero::new(size),
            })
        };
        let constants = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &first,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: part(&uniforms, frame_size),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: part(&bones, BONES as u64 * 64),
                },
            ],
        });
        Ok(Self {
            world,
            scalp,
            overlays: overlay_actors,
            strands,
            takes,
            movie_rigs,
            visible_in_take,
            camera_obstacles,
            uniforms,
            bones,
            constants,
            format,
            targets: None,
            fallback_presets,
            menu,
            menu_pipeline,
            moon,
            moon_pipeline,
        })
    }

    pub fn show(&self) -> Show {
        Show::new()
    }

    /// The joint that the free camera turns around.
    fn root(&self, world: &[Mat4]) -> Vec3 {
        let at = self
            .world
            .model
            .find("MMM01_c_hip01_xx01_jjj")
            .or_else(|| self.world.model.find("root"))
            .unwrap_or(0);
        world[at].transform_point3(Vec3::ZERO)
    }

    /// The head joint for the helmet camera.
    fn head_joint(&self) -> usize {
        self.world.model.find("MMM01_c_hed01_xx01_jjj").unwrap_or(0)
    }

    #[allow(clippy::too_many_lines)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        frame: &Frame,
        window: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let view = |format| {
                let t = gpu.target(size.0, size.1, format, SAMPLES);
                t.create_view(&Default::default())
            };
            self.targets = Some(Targets {
                size,
                color: view(self.format),
                depth: view(DEPTH),
            });
        }
        let (width, height) = (size.0 as f32, size.1 as f32);
        let take = frame.take % self.takes.len();
        let animation = &self.takes[take];
        let info = &demo::TAKES[take];
        // Master-timeline seconds: each take plays its own window of the
        // curves (`args.txt` start frame at 30 fps).
        let seconds = info.start + frame.local * f64::from(animation.scale);
        let mut values = HashMap::new();
        for n in &animation.numbers {
            let v = n.curve.at(seconds);
            values.extend(n.nodes.iter().map(|name| (name.clone(), v)));
        }
        if let crate::show::Face::Posed(list) = &frame.face {
            for (i, w) in list {
                if let Some(name) = self.world.model.shape(0).weights.get(*i) {
                    values.insert(name.clone(), *w);
                }
            }
        }
        let world = self.world.model.pose(Some((animation, seconds)), &[]);
        let scalp_world = self.scalp.model.pose(Some((animation, seconds)), &[]);
        let root = self.root(&world);
        let head_joint = self.head_joint();
        let head = world[head_joint];
        let hair_pose = head * self.world.model.rest()[head_joint].inverse();
        let focus = root.lerp(head.w_axis.truncate(), 0.55);
        let aspect = width / height;
        let fov = movie_fov(aspect);
        let clear = |mut shot: Shot, anchor, radius| {
            shot.place = keep_clear(
                &self.camera_obstacles,
                &self.world.model,
                &self.visible_in_take[take],
                &world,
                anchor,
                shot.place,
                radius,
            );
            shot
        };
        let clear_lens = |shot: Shot| {
            let eye = shot.place.w_axis.truncate();
            let forward = -shot.place.z_axis.truncate().normalize();
            // Preserve the composition of scripted shots while clearing their immediate foreground.
            clear(shot, eye + forward * 24.0, 4.0)
        };
        let orbit = |target, yaw, pitch, distance| {
            clear(Shot::orbit(target, yaw, pitch, distance, fov), target, 12.0)
        };
        let shot = match frame.camera {
            Camera::Helmet => {
                // The eyeballs sit along local +Y, with +Z forward and +X up.
                // Place the lens just beyond the helmet brim.
                let eye = head.transform_point3(Vec3::new(5.0, 0.0, 15.0));
                let forward = head.z_axis.truncate().normalize();
                let up = head.x_axis.truncate().normalize();
                let right = forward.cross(up).normalize();
                clear_lens(Shot {
                    place: Mat4::from_cols(
                        right.extend(0.0),
                        right.cross(forward).extend(0.0),
                        (-forward).extend(0.0),
                        eye.extend(1.0),
                    ),
                    fov,
                })
            }
            Camera::Movie => match self.movie_rigs[take] {
                Some(_) if info.set == demo::Set::None && frame.local >= 3.5 => {
                    orbit(focus, -2.3, -0.24, 225.0)
                }
                // These two cuts cross the old computer wall, where the port's
                // simplified bedroom pass leaves props in front of the lens.
                Some(at) if info.set == demo::Set::OldBedroom && world[at].w_axis.x > 280.0 => {
                    let yaw = if info.camera == "camera2" { 0.75 } else { -0.8 };
                    orbit(focus, yaw, -0.28, 170.0)
                }
                Some(at) => clear_lens(Shot {
                    place: world[at],
                    fov,
                }),
                None => {
                    let (yaw, pitch, distance) =
                        self.fallback_presets[take].map_or((-2.28, -0.38, 240.0), |preset| {
                            (
                                preset.yaw,
                                preset.pitch,
                                preset.distance.clamp(170.0, 800.0),
                            )
                        });
                    orbit(focus, yaw, pitch, distance)
                }
            },
            Camera::Free {
                yaw,
                pitch,
                distance,
                pan,
            } => orbit(focus + pan, yaw, pitch, distance),
        };
        let eye = shot.place.w_axis.truncate();
        let view = shot.place.inverse();
        let projection =
            glam::camera::rh::proj::directx::perspective(shot.fov, aspect, 1.0, 4000.0);
        let mut lights = [[0.0; 4]; 16];
        // Moon, omni, lamp and mermaid lights from the world file.
        for (i, (color, position)) in [
            (Vec3::new(0.34, 0.37, 0.49), Vec3::new(60.0, 220.0, -80.0)),
            (Vec3::new(1.0, 0.8, 0.5), Vec3::ZERO),
            (Vec3::new(0.4, 0.4, 0.4), Vec3::new(-40.0, 150.0, 60.0)),
            (Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.0, 170.0, 40.0)),
        ]
        .iter()
        .enumerate()
        {
            lights[i * 2] = [color.x, color.y, color.z, 1.0];
            lights[i * 2 + 1] = [position.x, position.y, position.z, 0.0];
        }
        let uniforms = Uniforms {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            eye: eye.extend(1.0).into(),
            time: [frame.local as f32, width, height, 0.0],
            lights,
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let mut calls: Vec<Call> = Vec::new();
        let mut data = Vec::new();
        let mut backbone = 0;
        // Actor 0 is the world, 1 the scalp, then overlays, then hair.
        backbone = draw_actor(
            0,
            &mut self.world,
            Some(&self.visible_in_take[take]),
            &world,
            &view,
            &values,
            frame,
            gpu,
            &mut data,
            &mut calls,
            backbone,
        );
        backbone = draw_actor(
            1,
            &mut self.scalp,
            None,
            &scalp_world,
            &view,
            &values,
            frame,
            gpu,
            &mut data,
            &mut calls,
            backbone,
        );
        if info.set != demo::Set::None {
            for (n, overlay) in self.overlays.iter_mut().enumerate() {
                backbone = draw_actor_static(
                    2 + n,
                    overlay,
                    Mat4::IDENTITY,
                    &view,
                    &mut data,
                    &mut calls,
                    backbone,
                );
            }
        }
        let hair = 2 + self.overlays.len();
        draw_actor_static(
            hair,
            &mut self.strands,
            hair_pose,
            &view,
            &mut data,
            &mut calls,
            backbone,
        );
        gpu.queue.write_buffer(&self.bones, 0, &data);
        let mut order: Vec<usize> = (0..calls.len()).collect();
        // Opaque shapes first, then transparent ones back to front.
        order.sort_by(|&a, &b| {
            let (s, t) = (&calls[a], &calls[b]);
            s.transparent
                .cmp(&t.transparent)
                .then(t.distance.total_cmp(&s.distance))
        });

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
        if info.set == demo::Set::None {
            pass.set_pipeline(&self.moon_pipeline);
            pass.set_bind_group(1, &self.moon, &[]);
            pass.draw(0..3, 0..1);
        }
        let hair = 2 + self.overlays.len();
        for i in order {
            let call = &calls[i];
            let actor = if call.actor == 0 {
                &self.world
            } else if call.actor == 1 {
                &self.scalp
            } else if call.actor == hair {
                &self.strands
            } else {
                &self.overlays[call.actor - 2]
            };
            let shape = &actor.shapes[call.shape];
            pass.set_pipeline(&shape.pipeline);
            pass.set_bind_group(
                0,
                &self.constants,
                &[(call.backbone as u64 * STRIDE) as u32],
            );
            pass.set_bind_group(1, &shape.group, &[]);
            match &shape.buffers {
                Buffers::Mesh {
                    moving,
                    fixed,
                    indices,
                    count,
                    ..
                } => {
                    pass.set_vertex_buffer(0, moving.slice(..));
                    pass.set_vertex_buffer(1, fixed.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
                Buffers::Hair { points, count } => {
                    pass.set_vertex_buffer(0, points.slice(..));
                    pass.draw(0..6, 0..*count);
                }
                Buffers::Particles { points, count } => {
                    pass.set_vertex_buffer(0, points.slice(..));
                    pass.draw(0..6, 0..*count);
                }
            }
        }
        if frame.menu {
            pass.set_pipeline(&self.menu_pipeline);
            pass.set_vertex_buffer(0, self.menu.points.slice(..));
            for b in &self.menu.batches {
                pass.set_bind_group(1, &b.group, &[]);
                pass.draw(b.start..b.start + b.count, 0..1);
            }
        }
    }
}

/// Submits one actor's shapes: morphs, bones and sort order.
#[allow(clippy::too_many_arguments)]
fn draw_actor(
    id: usize,
    actor: &mut Actor,
    visibility: Option<&[bool]>,
    world: &[Mat4],
    view: &Mat4,
    values: &HashMap<String, f32>,
    frame: &Frame,
    gpu: &Gpu,
    data: &mut Vec<u8>,
    calls: &mut Vec<Call>,
    backbone: usize,
) -> usize {
    let mut base = backbone;
    for (k, (shape, gpu_shape)) in actor
        .model
        .shapes
        .iter()
        .zip(actor.shapes.iter_mut())
        .enumerate()
    {
        if !shape.primary
            || shape.pass == Pass::Depth
            || shape.pass == Pass::Skip
            || visibility.is_some_and(|visible| !visible[k])
        {
            continue;
        }
        let bones = if shape.bones.is_empty() {
            vec![Mat4::IDENTITY]
        } else {
            actor.model.bones(shape, world)
        };
        let distance = (*view * bones[0]).transform_point3(shape.center).length();
        let start = data.len();
        for m in &bones {
            data.extend_from_slice(bytemuck::bytes_of(&m.to_cols_array_2d()));
        }
        data.resize(start + STRIDE as usize, 0);
        if let (Geometry::Mesh(m) | Geometry::Cage(m), Buffers::Mesh { moving, loaded, .. }) =
            (&shape.geometry, &mut gpu_shape.buffers)
            && shape.targets() > 0
        {
            let wanted = match &frame.face {
                crate::show::Face::Animated => shape.animated(values, demo::MORPHS, demo::LEAST),
                crate::show::Face::Posed(list) => list
                    .iter()
                    .copied()
                    .filter(|w| w.1 > demo::LEAST && w.0 < shape.targets())
                    .collect(),
            };
            if *loaded != wanted {
                gpu.queue
                    .write_buffer(moving, 0, bytemuck::cast_slice(&m.morphed(&wanted)));
                *loaded = wanted;
            }
        }
        calls.push(Call {
            actor: id,
            shape: k,
            backbone: base,
            distance,
            transparent: shape.blend != crate::model::Blend::None,
        });
        base += 1;
    }
    base
}

/// Submits an overlay or hair actor with a single transform.
fn draw_actor_static(
    id: usize,
    actor: &mut Actor,
    transform: Mat4,
    view: &Mat4,
    data: &mut Vec<u8>,
    calls: &mut Vec<Call>,
    backbone: usize,
) -> usize {
    let mut base = backbone;
    for (k, (shape, _)) in actor
        .model
        .shapes
        .iter()
        .zip(actor.shapes.iter())
        .enumerate()
    {
        if !shape.primary || shape.pass == Pass::Depth || shape.pass == Pass::Skip {
            continue;
        }
        let distance = (*view * transform).transform_point3(shape.center).length();
        let start = data.len();
        data.extend_from_slice(bytemuck::bytes_of(&transform.to_cols_array_2d()));
        data.resize(start + STRIDE as usize, 0);
        calls.push(Call {
            actor: id,
            shape: k,
            backbone: base,
            distance,
            transparent: shape.blend != crate::model::Blend::None,
        });
        base += 1;
    }
    base
}

fn parse_f32(words: &[&str]) -> Vec<f32> {
    words.iter().filter_map(|w| w.parse().ok()).collect()
}

/// Camera presets from the shipped `vistas.txt`, `effectshots.txt` and
/// `bin/defaultUserCamPositions.txt`: `camYaw`, `camPitch`, `camDistance`
/// and `camLookAt`.
fn presets(files: &Files) -> Result<Vec<Preset>, Error> {
    let mut out = Vec::new();
    for name in demo::TEXT_FILES {
        let Ok(text) = files.text(name) else {
            continue;
        };
        let mut rest = text.as_str();
        while let Some(start) = rest.find("ScreenShot") {
            rest = &rest[start + "ScreenShot".len()..];
            let depth = rest
                .find('{')
                .ok_or_else(|| Error::from(format!("{name}: shot without body")))?;
            let mut level = 0;
            let mut end = None;
            for (i, c) in rest[depth..].char_indices() {
                if c == '{' {
                    level += 1;
                } else if c == '}' {
                    level -= 1;
                    if level == 0 {
                        end = Some(depth + i);
                        break;
                    }
                }
            }
            let Some(end) = end else { break };
            let body = &rest[depth + 1..end];
            rest = &rest[end + 1..];
            let get = |key: &str| -> Vec<f32> {
                let Some(at) = body.find(key) else {
                    return Vec::new();
                };
                let tail = &body[at + key.len()..];
                let words: Vec<&str> = tail
                    .split_whitespace()
                    .take_while(|w| {
                        w.parse::<f32>().is_ok()
                            || *w == "-"
                            || w.starts_with("e")
                            || w.starts_with("E")
                    })
                    .collect();
                // A leading `-` attaches to the next number.
                let mut fixed = Vec::new();
                let mut minus = false;
                for w in words {
                    if w == "-" {
                        minus = true;
                    } else if minus {
                        fixed.push(format!("-{w}"));
                        minus = false;
                    } else {
                        fixed.push(w.to_owned());
                    }
                }
                parse_f32(&fixed.iter().map(String::as_str).collect::<Vec<_>>())
            };
            let animation = body
                .find("animationIndex")
                .and_then(|at| body[at..].split_whitespace().nth(1)?.parse::<usize>().ok());
            let yaw = get("camYaw").first().copied().unwrap_or(-2.28);
            let pitch = get("camPitch").first().copied().unwrap_or(-0.38);
            let distance = get("camDistance").first().copied().unwrap_or(240.0);
            let look = get("camLookAt");
            out.push(Preset {
                yaw,
                pitch,
                distance,
                look: Vec3::new(
                    look.first().copied().unwrap_or(28.0),
                    look.get(1).copied().unwrap_or(143.0),
                    look.get(2).copied().unwrap_or(-12.0),
                ),
                animation,
            });
        }
    }
    if out.is_empty() {
        return Err("no camera presets".into());
    }
    // One preset per animation; keep the first for repeats.
    out.sort_by_key(|p| p.animation.unwrap_or(usize::MAX));
    out.dedup_by_key(|p| p.animation);
    Ok(out)
}

/// The hair curves and growth parameters the port reads.
fn hair_files(files: &Files) -> Result<(&str, &str, &str, &str), Error> {
    let _ = files;
    Ok((
        "bin/hair_curves.txt",
        "bin/hairclumpsparams.txt",
        "bin/gotti_curves.txt",
        "bin/gotti_params.txt",
    ))
}

/// Grows the head hair and the goatee as one model.
fn grow(files: &Files) -> Result<Model, Error> {
    let (head_curves, head_params, chin_curves, chin_params) = hair_files(files)?;
    let number = |text: &str, key: &str| -> f32 {
        text.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .find(|l| l.contains(key))
            .and_then(|l| {
                l.split('=')
                    .nth(1)
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .and_then(|w| w.trim_end_matches(';').parse().ok())
            })
            .unwrap_or(0.0)
    };
    let head_p = files.text(head_params)?;
    let chin_p = files.text(chin_params)?;
    let mut shapes = Vec::new();
    for (curves, params, set) in [
        (head_curves, head_p.as_str(), 0.0),
        (chin_curves, chin_p.as_str(), 1.0),
    ] {
        let hair = hair::Hair::new(
            &files.text(curves)?,
            number(params, "Numinterp") as usize,
            number(params, "Basicbezierdivide") as usize,
            number(params, "Radius"),
            set,
        )?;
        shapes.push(Shape {
            name: curves.to_owned(),
            geometry: Geometry::Hair(hair),
            transform: None,
            primary: true,
            bones: Vec::new(),
            vertex: String::new(),
            fragment: "hair".to_owned(),
            textures: std::collections::BTreeMap::new(),
            lights: Vec::new(),
            matrices: Vec::new(),
            misc: Vec::new(),
            weights: Vec::new(),
            system: None,
            cull: false,
            blend: crate::model::Blend::Alpha,
            pass: Pass::Color,
            set: 5,
            far_first: false,
            center: Vec3::new(0.0, 170.0, -8.0),
        });
    }
    Ok(Model::generated(shapes))
}

/// Menu buttons as NDC quads, grouped by their image.
fn menu_batches(loader: &mut Loader<'_>, files: &Files, menu: &Model) -> Result<Menu, Error> {
    let root = files
        .scene(demo::MENU)
        .map_err(|e| format!("{}: {e}", demo::MENU))?;
    // (rect x, y, w, h in fractions, texture node)
    let mut quads: Vec<([f32; 4], String)> = Vec::new();
    fn buttons(n: &tree::Node, quads: &mut Vec<([f32; 4], String)>) {
        for m in n.nodes("menuItems") {
            let pos: [f32; 2] = m.vec("position").unwrap_or([0.0, 0.0]);
            let size: [f32; 2] = m.vec("size").unwrap_or([0.1, 0.05]);
            if m.kind == "OnOffButton" || m.kind == "UpDownButton" {
                quads.push((
                    [pos[0], pos[1], size[0], size[1]],
                    m.text("buttonImage").to_owned(),
                ));
                if !m.text("buttonImageOff").is_empty() {
                    quads.push((
                        [pos[0], pos[1] - size[1] * 1.2, size[0], size[1]],
                        m.text("buttonImageOff").to_owned(),
                    ));
                }
            }
            if m.kind == "Icon" {
                quads.push((
                    [pos[0], pos[1], size[0], size[1]],
                    m.text("image").to_owned(),
                ));
            }
            buttons(m, quads);
        }
    }
    for m in root.nodes("menus") {
        buttons(m, &mut quads);
    }
    // One vertex buffer; quads grouped by texture for one bind group each.
    let mut by_texture: std::collections::BTreeMap<String, Vec<[f32; 4]>> =
        std::collections::BTreeMap::new();
    for (rect, tex) in quads {
        if !tex.is_empty() {
            by_texture.entry(tex).or_default().push(rect);
        }
    }
    let mut data = Vec::new();
    let mut batches = Vec::new();
    for (tex, rects) in &by_texture {
        let start = (data.len() / 4) as u32;
        for r in rects {
            let (x, y, w, h) = (r[0], r[1], r[2], r[3]);
            // Menu art is stored upside down for its mapping [I].
            let corners = [
                [x, y, 0.0, 1.0],
                [x + w, y, 1.0, 1.0],
                [x, y + h, 0.0, 0.0],
                [x + w, y, 1.0, 1.0],
                [x + w, y + h, 1.0, 0.0],
                [x, y + h, 0.0, 0.0],
            ];
            for c in corners {
                // Fractions with top-left origin to clip space.
                data.extend([c[0] * 2.0 - 1.0, 1.0 - c[1] * 2.0, c[2], c[3]]);
            }
        }
        let count = (data.len() / 4) as u32 - start;
        if count == 0 {
            continue;
        }
        let file = match menu.textures.get(tex) {
            Some(crate::model::Texture::Plane { file, .. }) => Some(file.as_str()),
            _ => None,
        };
        let view = loader.view(file)?;
        let blank = loader.view(None)?;
        let blank2 = loader.view(None)?;
        let blank3 = loader.view(None)?;
        let group = loader.group(vec![view, blank, blank2, blank3]);
        batches.push(MenuBatch {
            group,
            start,
            count,
        });
    }
    let points = loader
        .gpu
        .buffer(bytemuck::cast_slice(&data), wgpu::BufferUsages::VERTEX);
    Ok(Menu { points, batches })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_view_keeps_the_movie_horizontal_framing() {
        let horizontal = |aspect| (movie_fov(aspect) * 0.5).tan() * aspect;
        let reference = horizontal(4.0 / 3.0);
        assert!((horizontal(9.0 / 16.0) - reference).abs() < 1e-6);
    }

    #[test]
    fn storyboard_switches_rooms_and_computers() -> Result<(), Error> {
        let Some(path) = std::env::var_os(demo::DATA_ENV) else {
            return Ok(());
        };
        let files = Files::from_dir(std::path::Path::new(&path))?;
        let model = Model::new(&files.binary(demo::GEOM)?, &files)?;
        let shape = |name: &str| {
            model
                .shapes
                .iter()
                .find(|shape| shape.name == name)
                .unwrap()
        };
        let bedroom_wall = shape("MMB_Wall_Computer_SHAPEShape_nvNearPass");
        let workshop_wall = shape("MMW_BackWall_SHAPEShape_nvNearPass");
        let old_computer = shape("MMB_COMP_OldMainboard_SHAPEShape_nvNearPass");
        let new_computer = shape("MMB_NewComputer_SHAPEShape_nvNearPass");
        let is_visible = |shape, take: usize| visible(&model, shape, &demo::TAKES[take]);

        assert!(bedroom_wall.cull && bedroom_wall.primary && workshop_wall.primary);
        assert!(!shape("MMB_Wall_Door_SHAPEShape_nvRadianceCubePass").primary);
        assert!(shape("MMM_PantsShapeShape_nvNearPass_0").primary);
        assert!(is_visible(bedroom_wall, 3) && !is_visible(workshop_wall, 3));
        assert!(is_visible(workshop_wall, 4) && !is_visible(bedroom_wall, 4));
        assert!(is_visible(old_computer, 5) && !is_visible(new_computer, 5));
        assert!(is_visible(new_computer, 6) && !is_visible(old_computer, 6));
        assert!(!is_visible(bedroom_wall, 8) && !is_visible(workshop_wall, 8));

        // This orbit puts the lens beyond the deforming door frame in Karate.
        let take = &demo::TAKES[1];
        let animation = files.binary(take.file)?;
        let animation = crate::anim::Animation::read(&animation.nodes("animations")[0])?;
        let pose = model.pose(Some((&animation, take.start + 2.0)), &[]);
        let joint = |name| pose[model.find(name).unwrap()].w_axis.truncate();
        let focus = joint("MMM01_c_hip01_xx01_jjj").lerp(joint("MMM01_c_hed01_xx01_jjj"), 0.55);
        let shot = Shot::orbit(focus, 0.0, -0.38, 240.0, movie_fov(4.0 / 3.0));
        let visible: Vec<_> = model
            .shapes
            .iter()
            .map(|shape| is_visible(shape, 1))
            .collect();
        let adjusted = keep_clear(
            &Obstacle::from_model(&model),
            &model,
            &visible,
            &pose,
            focus,
            shot.place,
            12.0,
        );
        assert!(adjusted.w_axis.truncate().distance(focus) < 230.0);
        assert_eq!(adjusted.z_axis, shot.place.z_axis);
        Ok(())
    }
}
