//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use glam::{Mat4, Vec3};
use wgpu::TextureViewDimension::{Cube, D2};

use crate::Error;
use crate::data::Files;
use crate::gpu::{COLOR, DEPTH, Gpu, SAMPLES, SLOT};
use crate::model::{Blend, Cloth, Fixed, Geometry, Model, Moving, Shape, Texture};
use crate::show::{Face, Frame, Show};
use crate::{dds, demo, tga, tree};

/// Vectors of bones in the shader.
const BONES: usize = 96;
/// Uniform offsets are multiples of this.
const STRIDE: u64 = (BONES as u64 * 16).next_multiple_of(SLOT);
const DIFFUSE: &str = "g_diffuse_Cube";
const SPECULAR: &str = "g_specular_Cube";
const HILIGHT: &str = "g_hilight_Cube";

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Uniforms {
    view: [[f32; 4]; 4],
    projection: [[f32; 4]; 4],
    eye: [f32; 4],
    look: [f32; 4],
    size: [f32; 4],
    badge: [f32; 4],
    sky: [[f32; 4]; 3],
    fade: [f32; 4],
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
        segments: wgpu::Buffer,
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
    /// The fairy and the branches.
    actors: [Actor; 2],
    sunning: crate::anim::Animation,
    uniforms: wgpu::Buffer,
    bones: wgpu::Buffer,
    constants: wgpu::BindGroup,
    sky: (wgpu::RenderPipeline, wgpu::BindGroup),
    badge: (wgpu::RenderPipeline, wgpu::BindGroup),
    fade: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

/// Loads each texture once.
struct Loader<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    views: HashMap<String, wgpu::TextureView>,
    samplers: HashMap<[bool; 2], wgpu::Sampler>,
    layout: wgpu::BindGroupLayout,
}

/// The faces of a cross as `FUN_00460a60` cuts it, +X, -X, +Y, -Y, +Z, -Z. Rows start at
/// the bottom of the picture.
pub fn faces((w, h): (usize, usize), pixels: &[u8]) -> Result<(u32, Vec<Vec<u8>>), Error> {
    let side = w / 3;
    if side == 0 || w != side * 3 || h != side * 4 || pixels.len() != w * h * 4 {
        return Err("not a cross of six squares".into());
    }
    let cut = |column: usize, row: usize, turned: bool| -> Vec<u8> {
        let mut out = Vec::with_capacity(side * side * 4);
        for (y, x) in (0..side).flat_map(|y| (0..side).map(move |x| (y, x))) {
            let (x, y) = if turned {
                (side - 1 - x, side - 1 - y)
            } else {
                (x, y)
            };
            let at = ((row * side + y) * w + column * side + x) * 4;
            out.extend(&pixels[at..at + 4]);
        }
        out
    };
    let six = [
        (0, 1, false),
        (2, 1, false),
        (1, 0, true),
        (1, 2, true),
        (1, 3, true),
        (1, 1, false),
    ];
    Ok((side as u32, six.map(|(c, r, t)| cut(c, r, t)).to_vec()))
}

impl Loader<'_> {
    fn view(
        &mut self,
        model: &Model,
        name: Option<&String>,
        cube: bool,
    ) -> Result<wgpu::TextureView, Error> {
        let texture = name.map(|n| {
            model
                .textures
                .get(n)
                .ok_or_else(|| format!("no texture {n}"))
        });
        let file = match texture.transpose()? {
            Some(Texture::Plane { file, .. }) if !cube => file.as_str(),
            Some(Texture::Cross { file }) if cube => file.as_str(),
            Some(Texture::Normalize) | None => "",
            Some(_) => return Err("a texture has the wrong shape".into()),
        };
        self.load(file, cube)
    }

    fn load(&mut self, file: &str, cube: bool) -> Result<wgpu::TextureView, Error> {
        let key = format!("{file} {cube}");
        if let Some(v) = self.views.get(&key) {
            return Ok(v.clone());
        }
        let texture = if file.is_empty() {
            // Stands in where a program reads nothing.
            let layers = vec![vec![0; 4]; if cube { 6 } else { 1 }];
            self.gpu.upload_rgba((1, 1), &layers, false)
        } else {
            let data = self.files.get(file)?;
            if file.to_ascii_lowercase().ends_with(".dds") {
                self.gpu.upload(&[dds::Image::read(data)?])?
            } else {
                let (w, h, pixels) = tga::read(data)?;
                if cube {
                    let (side, layers) = faces((w, h), &pixels)?;
                    self.gpu.upload_rgba((side, side), &layers, true)
                } else {
                    self.gpu.upload_rgba((w as u32, h as u32), &[pixels], true)
                }
            }
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(if cube { Cube } else { D2 }),
            ..Default::default()
        });
        self.views.insert(key, view.clone());
        Ok(view)
    }

    fn group(
        &mut self,
        planes: [wgpu::TextureView; 3],
        cubes: [wgpu::TextureView; 3],
        repeat: [bool; 2],
    ) -> wgpu::BindGroup {
        let [u, v] = repeat.map(|r| match r {
            true => wgpu::AddressMode::Repeat,
            false => wgpu::AddressMode::ClampToEdge,
        });
        let sampler = self.samplers.entry(repeat).or_insert_with(|| {
            self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: u,
                address_mode_v: v,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        });
        let mut entries: Vec<_> = planes
            .iter()
            .chain(&cubes)
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: wgpu::BindingResource::TextureView(v),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 6,
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
        let names = match shape.program.as_str() {
            "hair" => [
                "anisoTex",
                "hairDiff",
                "",
                "cube_diffuse_map",
                "",
                "cube_bleed_map",
            ],
            "_texture" => ["texture", "", "", "", "", ""],
            p if p.starts_with("finalSkin") || p == "finalTearDucts" => [
                "skinColor_frontSpec",
                "skin_norm_sideSpec",
                "g_specular_colorShift",
                DIFFUSE,
                SPECULAR,
                HILIGHT,
            ],
            _ => [
                "diffCol",
                "normalMap",
                "translucenceMap",
                DIFFUSE,
                SPECULAR,
                HILIGHT,
            ],
        };
        let mut views = Vec::new();
        let mut wrap = None;
        for (i, name) in names.iter().enumerate() {
            let texture = shape.textures.get(*name);
            if let Some(Texture::Plane { signed, repeat, .. }) =
                texture.and_then(|t| model.textures.get(t))
            {
                if *signed != (i == 1 && shape.program != "hair") {
                    return Err(format!("{name} has the wrong range").into());
                }
                if *wrap.get_or_insert(*repeat) != *repeat {
                    return Err(format!("{name} wraps differently").into());
                }
            }
            views.push(self.view(model, texture, i >= 3)?);
        }
        let [a, b, c, d, e, f] = views.try_into().map_err(|_| "six textures")?;
        Ok(self.group([a, b, c], [d, e, f], wrap.unwrap_or_default()))
    }
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
    blend: Blend,
    cull: bool,
    cloth: bool,
    depth: Option<bool>,
    coverage: bool,
}

impl Pipelines<'_> {
    fn make(&mut self, s: Style<'_>) -> wgpu::RenderPipeline {
        let key = format!(
            "{} {} {} {} {}",
            s.vertex, s.fragment, s.blend as u8, s.cull, s.cloth
        );
        if let Some((_, p)) = self.made.iter().find(|m| m.0 == key) {
            return p.clone();
        }
        let component = |src, dst| wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        };
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
                    bias: wgpu::DepthBiasState {
                        slope_scale: if s.cloth { -2.0 } else { 0.0 },
                        ..Default::default()
                    },
                }),
                multisample: samples(s.coverage),
                multiview_mask: None,
                cache: None,
            });
        self.made.push((key, pipeline.clone()));
        pipeline
    }

    fn shape(&mut self, shape: &Shape) -> wgpu::RenderPipeline {
        let fixed = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x2, 4 => Float32x4, 5 => Float32x4];
        let moving = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let segment = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x2,
            4 => Float32x3, 5 => Float32x3, 6 => Float32x3, 7 => Float32x2
        ];
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
        let hair = matches!(shape.geometry, Geometry::Hair(_));
        let mut buffers = vec![
            layout(&moving, size_of::<Moving>()),
            layout(&fixed, size_of::<Fixed>()),
        ];
        if hair {
            buffers = vec![Some(wgpu::VertexBufferLayout {
                array_stride: 88,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &segment,
            })];
        }
        self.make(Style {
            vertex: match (hair, shape.cloth) {
                (true, _) => "vs_hair",
                (_, Cloth::None) => "vs",
                (_, Cloth::Under) => "vs_under",
                (_, Cloth::Over) => "vs_over",
            },
            fragment: &shape.program,
            buffers: &buffers,
            blend: shape.blend,
            // A line has no back.
            cull: shape.cull && !hair,
            cloth: shape.cloth != Cloth::None,
            depth: Some(true),
            coverage: hair,
        })
    }
}

impl Actor {
    fn new(
        gpu: &Gpu,
        model: Model,
        loader: &mut Loader<'_>,
        pipelines: &mut Pipelines<'_>,
    ) -> Result<Self, Error> {
        let mut shapes = Vec::new();
        for shape in &model.shapes {
            use wgpu::BufferUsages as Use;
            let buffers = match &shape.geometry {
                Geometry::Mesh(m) => Buffers::Mesh {
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
                            data.extend(p.color.to_array());
                        }
                    }
                    Buffers::Hair {
                        segments: gpu.buffer(bytemuck::cast_slice(&data), Use::VERTEX),
                        count: (data.len() / 22) as u32,
                    }
                }
            };
            if shape.bones.len() * 3 > BONES {
                return Err(format!("{}: too many bones", shape.name).into());
            }
            shapes.push(GpuShape {
                buffers,
                pipeline: pipelines.shape(shape),
                group: loader
                    .material(&model, shape)
                    .map_err(|e| format!("{}: {e}", shape.name))?,
            });
        }
        Ok(Self { model, shapes })
    }
}

/// A shape to draw: actor, shape, and where its bones are.
struct Call {
    actor: usize,
    shape: usize,
    distance: f32,
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        let device = &gpu.device;
        let read = |name| -> Result<_, Error> {
            tree::read_binary(files.get(name)?).map_err(|e| format!("{name}: {e}").into())
        };
        let model = |name| -> Result<_, Error> {
            Model::new(&read(name)?, files).map_err(|e| format!("{name}: {e}").into())
        };
        let (fairy, branch) = (model(demo::FAIRY)?, model(demo::BRANCH)?);
        let sunning = read(demo::SUNNING)?;
        let sunning = sunning.nodes("animations").first().ok_or("no animation")?;
        let sunning = crate::anim::Animation::read(sunning)?;
        if fairy.animations.is_empty() {
            return Err("the dance is missing".into());
        }

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
                entry(1, buffer(true, BONES as u64 * 16)),
            ],
        });
        let mut textures: Vec<_> = (0..6)
            .map(|i| {
                entry(
                    i,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: if i < 3 { D2 } else { Cube },
                        multisampled: false,
                    },
                )
            })
            .collect();
        textures.push(entry(
            6,
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
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/dawn.wgsl").into()),
            }),
            format,
            made: Vec::new(),
        };
        let mut loader = Loader {
            gpu,
            files,
            views: HashMap::new(),
            samplers: HashMap::new(),
            layout: second,
        };
        let mut screen = |vertex, fragment, blend| {
            pipelines.make(Style {
                vertex,
                fragment,
                buffers: &[],
                blend,
                cull: false,
                cloth: false,
                depth: None,
                coverage: false,
            })
        };
        let sky = screen("vs_sky", "sky", Blend::None);
        let badge = screen("vs_badge", "badge", Blend::Alpha);
        let fade = screen("vs_fade", "fade", Blend::Alpha);
        let empty = loader.load("", false)?;
        let blank = loader.load("", true)?;
        let cube = loader.view(&fairy, fairy.sky.as_ref(), true)?;
        let sky_group = loader.group(
            [empty.clone(), empty.clone(), empty.clone()],
            [cube, blank.clone(), blank.clone()],
            [false; 2],
        );
        let logo = loader.load(demo::BADGE, false)?;
        let badge_group = loader.group(
            [logo, empty.clone(), empty],
            [blank.clone(), blank.clone(), blank],
            [false; 2],
        );
        let actors = [
            Actor::new(gpu, fairy, &mut loader, &mut pipelines)?,
            Actor::new(gpu, branch, &mut loader, &mut pipelines)?,
        ];

        let slots: usize = actors.iter().map(|a| a.shapes.len()).sum();
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
                    resource: part(&bones, BONES as u64 * 16),
                },
            ],
        });
        Ok(Self {
            actors,
            sunning,
            uniforms,
            bones,
            constants,
            sky: (sky, sky_group),
            badge: (badge, badge_group),
            fade,
            format,
            targets: None,
        })
    }

    pub fn show(&self) -> Show {
        Show::new()
    }

    /// Where the fairy's joints are, and the weights of her morph targets by name.
    fn pose(&self, frame: &Frame) -> (Vec<Mat4>, HashMap<String, f32>) {
        let fairy = &self.actors[0].model;
        let animation = if frame.scene == 0 {
            &fairy.animations[0]
        } else {
            &self.sunning
        };
        let seconds = frame.local * f64::from(animation.scale);
        let mut values = HashMap::new();
        for n in &animation.numbers {
            let v = n.curve.at(seconds);
            values.extend(n.nodes.iter().map(|name| (name.clone(), v)));
        }
        (fairy.pose(Some((animation, seconds)), &[]), values)
    }

    /// The joint that the free camera turns around.
    fn root(&self, world: &[Mat4]) -> Vec3 {
        let at = self.actors[0].model.find("root").unwrap_or(0);
        world[at].transform_point3(Vec3::ZERO)
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
        let (world, values) = self.pose(frame);
        let root = self.root(&world);
        let camera = self.actors[0]
            .model
            .find("FairyCam")
            .map_or(Mat4::IDENTITY, |i| world[i]);
        let shot = frame.camera.shot(camera, root);
        let eye = shot.place.w_axis.truncate();
        let view = shot.place.inverse();
        // The clipping planes of the free camera; the movie's starts as near.
        let projection =
            glam::camera::rh::proj::directx::perspective(shot.fov, width / height, 1.0, 200.0);
        let cloth = (50_000.0 - 2_500.0 * eye.distance(root)).max(100.0);
        // The background has a wider lens, which depends on where the camera is.
        let lens = ((eye.length() * 0.02 + 1.5) * 36.0).to_radians().min(2.6);
        let (tall, wide) = ((lens / 2.0).tan(), (lens / 2.0).tan() * width / height);
        let (x, y, z) = (shot.place.x_axis, shot.place.y_axis, shot.place.z_axis);
        // At a 4:3 window the badge takes a tenth of either side.
        let badge = 0.1 * height;
        let uniforms = Uniforms {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            eye: eye.extend(1.0).into(),
            look: [frame.oil, frame.bumps, -0.5 * cloth, -cloth],
            size: [width, height, (height / 768.0).max(1.0), 0.0],
            badge: [
                1.0 - badge * 1.25 / width,
                0.015,
                badge / width,
                badge / height,
            ],
            sky: [(x * wide).into(), (y * tall).into(), (-z).into()],
            fade: [frame.white; 4],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let scene = &demo::SCENES[frame.scene];
        let moved = ("Branches", scene.translation.into(), scene.rotation.into());
        let branches = self.actors[1].model.pose(None, &[moved]);
        let mut calls: Vec<Call> = Vec::new();
        let mut data = Vec::new();
        for (i, (actor, world)) in self.actors.iter_mut().zip([&world, &branches]).enumerate() {
            for (k, (shape, gpu_shape)) in
                actor.model.shapes.iter().zip(&mut actor.shapes).enumerate()
            {
                let bones = actor.model.bones(shape, world);
                let distance = (view * bones[0]).transform_point3(shape.center).length();
                let start = data.len();
                for m in &bones {
                    let rows = m.transpose();
                    for row in [rows.x_axis, rows.y_axis, rows.z_axis] {
                        data.extend_from_slice(bytemuck::bytes_of(&row.to_array()));
                    }
                }
                data.resize(start + STRIDE as usize, 0);
                calls.push(Call {
                    actor: i,
                    shape: k,
                    distance,
                });
                if let (Geometry::Mesh(m), Buffers::Mesh { moving, loaded, .. }) =
                    (&shape.geometry, &mut gpu_shape.buffers)
                    && shape.targets() > 0
                {
                    let wanted = match &frame.face {
                        Face::Animated => shape.animated(&values, demo::MORPHS, demo::LEAST),
                        Face::Posed(list) => list
                            .iter()
                            .copied()
                            .filter(|w| w.1 > demo::LEAST && w.0 < shape.targets())
                            .collect(),
                    };
                    if *loaded != wanted {
                        gpu.queue.write_buffer(
                            moving,
                            0,
                            bytemuck::cast_slice(&m.morphed(&wanted)),
                        );
                        *loaded = wanted;
                    }
                }
            }
        }
        gpu.queue.write_buffer(&self.bones, 0, &data);
        let mut order: Vec<usize> = (0..calls.len()).collect();
        order.sort_by(|&a, &b| {
            let shape = |i: usize| &self.actors[calls[i].actor].model.shapes[calls[i].shape];
            let (s, t) = (shape(a), shape(b));
            let near = calls[a].distance.total_cmp(&calls[b].distance);
            let near = if s.far_first { near.reverse() } else { near };
            (s.set, s.far_first).cmp(&(t.set, t.far_first)).then(near)
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
        pass.set_pipeline(&self.sky.0);
        pass.set_bind_group(1, &self.sky.1, &[]);
        pass.draw(0..3, 0..1);
        for i in order {
            let shape = &self.actors[calls[i].actor].shapes[calls[i].shape];
            pass.set_pipeline(&shape.pipeline);
            pass.set_bind_group(0, &self.constants, &[(i as u64 * STRIDE) as u32]);
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
                Buffers::Hair { segments, count } => {
                    pass.set_vertex_buffer(0, segments.slice(..));
                    pass.draw(0..6, 0..*count);
                }
            }
        }
        pass.set_bind_group(0, &self.constants, &[0]);
        pass.set_bind_group(1, &self.badge.1, &[]);
        if frame.logo {
            pass.set_pipeline(&self.badge.0);
            pass.draw(0..6, 0..1);
        }
        if frame.white > 0.0 {
            pass.set_pipeline(&self.fade);
            pass.draw(0..3, 0..1);
        }
    }
}

/// Formats are unorm: the original works in gamma space.
const _: () = assert!(matches!(COLOR, wgpu::TextureFormat::Rgba8Unorm));

#[cfg(test)]
mod tests {
    #[test]
    fn cuts_crosses() {
        // Each texel holds its square: column, then row from the bottom.
        let (w, h) = (6, 8);
        let texel = |i: usize| [(i % w / 2) as u8, (i / w / 2) as u8, (i % w % 2) as u8, 255];
        let pixels: Vec<u8> = (0..w * h).flat_map(texel).collect();
        let (side, f) = super::faces((w, h), &pixels).unwrap();
        assert_eq!(side, 2);
        let square: Vec<_> = f.iter().map(|f| (f[0], f[1])).collect();
        assert_eq!(square, [(0, 1), (2, 1), (1, 0), (1, 2), (1, 3), (1, 1)]);
        // +X is as it is; +Y is turned half way.
        assert_eq!((f[0][2], f[2][2]), (0, 1));
        assert!(super::faces((6, 6), &pixels[..144]).is_err());
    }
}
