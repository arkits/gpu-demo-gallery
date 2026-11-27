use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};
use glam::{Mat4, Quat, Vec3};
use sushi_formats::texture::Level;
use wgpu::{CurrentSurfaceTexture, util::DeviceExt};

use crate::{Demo, Effect, Motion, Result, Scene, Vertex, camera_on};

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
    eye_time: [f32; 4],
    fog: [f32; 4],
    options: [f32; 4],
    lights: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshUniform {
    tint: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GeoUniform {
    centers: [[f32; 4]; 6],
    options: [f32; 4],
    aspect_fov: [f32; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    material: wgpu::BindGroup,
    count: u32,
    source: usize,
}

struct Geometry {
    opaque: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    meshes: Vec<GpuMesh>,
    water: Option<Water>,
}

struct WaterTargets {
    refraction: wgpu::TextureView,
    reflection: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

struct Water {
    plane: f32,
    camera: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    copy: wgpu::RenderPipeline,
    surface: wgpu::RenderPipeline,
    targets: WaterTargets,
}

impl WaterTargets {
    fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let target = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent(size.0, size.1),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let refraction = target("water refraction");
        let reflection = target("water reflection");
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water captures"),
            layout,
            entries: &[
                entry(0, wgpu::BindingResource::TextureView(&refraction)),
                entry(1, wgpu::BindingResource::TextureView(&reflection)),
                entry(2, wgpu::BindingResource::Sampler(sampler)),
            ],
        });
        Self {
            refraction,
            reflection,
            bind,
        }
    }
}

struct Geo {
    pipeline: wgpu::RenderPipeline,
    frame: wgpu::Buffer,
    binds: Vec<wgpu::BindGroup>,
}

enum Pass {
    Geometry(Box<Geometry>),
    Geo(Geo),
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    pass: Pass,
    scene: Scene,
    depth: wgpu::TextureView,
    size: (u32, u32),
}

fn uniform(device: &wgpu::Device, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("frame uniforms"),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource }
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn texture_view(device: &wgpu::Device, queue: &wgpu::Queue, levels: &[Level]) -> wgpu::TextureView {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("original demo texture"),
        size: extent(levels[0].width as u32, levels[0].height as u32),
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (i, level) in levels.iter().enumerate() {
        let size = extent(level.width as u32, level.height as u32);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: i as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );
    }
    texture.create_view(&Default::default())
}

fn depth(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene depth"),
        size: extent(size.0, size.1),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&Default::default())
}

fn build_geo(
    device: &wgpu::Device,
    textures: &[wgpu::TextureView],
    sampler: &wgpu::Sampler,
    settings: &crate::geoforms::Settings,
    format: wgpu::TextureFormat,
) -> Geo {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("GeoForms resources"),
        entries: &[
            uniform_entry(0),
            texture_entry(1),
            texture_entry(2),
            texture_entry(3),
            sampler_entry(4),
        ],
    });
    let frame = uniform(device, std::mem::size_of::<GeoUniform>());
    let binds = std::iter::once(0)
        .chain(settings.stencils.iter().copied())
        .map(|stencil| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("GeoForms volume and stencil"),
                layout: &layout,
                entries: &[
                    entry(0, frame.as_entire_binding()),
                    entry(
                        1,
                        wgpu::BindingResource::TextureView(&textures[settings.noise]),
                    ),
                    entry(
                        2,
                        wgpu::BindingResource::TextureView(&textures[settings.warp]),
                    ),
                    entry(3, wgpu::BindingResource::TextureView(&textures[stencil])),
                    entry(4, wgpu::BindingResource::Sampler(sampler)),
                ],
            })
        })
        .collect();
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("GeoForms pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        ..Default::default()
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("geoforms.wgsl"));
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GeoForms liquid surface"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_geo"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_geo"),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    Geo {
        pipeline,
        frame,
        binds,
    }
}

fn build_geometry(
    device: &wgpu::Device,
    textures: &[wgpu::TextureView],
    sampler: &wgpu::Sampler,
    scene: &Scene,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) -> Geometry {
    let camera = uniform(device, std::mem::size_of::<CameraUniform>());
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene camera layout"),
        entries: &[uniform_entry(0)],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene camera"),
        layout: &camera_layout,
        entries: &[entry(0, camera.as_entire_binding())],
    });
    let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene material layout"),
        entries: &[
            uniform_entry(0),
            texture_entry(1),
            texture_entry(2),
            sampler_entry(3),
            texture_entry(4),
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene pipeline layout"),
        bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
        ..Default::default()
    });
    let shader = device.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));
    let pipeline = |layout: &wgpu::PipelineLayout, fragment, blend: bool| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if blend {
                "transparent geometry"
            } else {
                "opaque geometry"
            }),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(!blend),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let mut order = (0..scene.meshes.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| {
        let mat = scene.meshes[i].material;
        (
            mat.blend,
            !matches!(mat.effect, Effect::Sky | Effect::CloudSky),
        )
    });
    let meshes = order
        .into_iter()
        .filter_map(|source| {
            let mesh = &scene.meshes[source];
            if mesh.indices.is_empty() || mesh.vertices.is_empty() {
                return None;
            }
            let mat = mesh.material;
            let data = MeshUniform {
                tint: mat.tint,
                params: [
                    mat.effect as u8 as f32,
                    mat.alpha_cut,
                    mat.strength,
                    if mat.blend { 1.0 } else { 0.0 },
                ],
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh material"),
                contents: bytes_of(&data),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mesh textures"),
                layout: &material_layout,
                entries: &[
                    entry(0, buffer.as_entire_binding()),
                    entry(
                        1,
                        wgpu::BindingResource::TextureView(&textures[mat.texture]),
                    ),
                    entry(2, wgpu::BindingResource::TextureView(&textures[mat.detail])),
                    entry(3, wgpu::BindingResource::Sampler(sampler)),
                    entry(4, wgpu::BindingResource::TextureView(&textures[mat.third])),
                ],
            });
            let subdivision = match mesh.motion.map(|m| &scene.motions[m]) {
                Some(Motion::Skeletal { pn_level, .. }) => *pn_level,
                _ => 1,
            };
            let (verts, indices) = if subdivision > 1 {
                pn_triangles(&mesh.vertices, &mesh.indices, subdivision, true)
            } else {
                (mesh.vertices.clone(), mesh.indices.clone())
            };
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh vertices"),
                contents: cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh indices"),
                contents: cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            Some(GpuMesh {
                vertices,
                indices: index_buffer,
                material,
                count: indices.len() as u32,
                source,
            })
        })
        .collect();
    let water = scene
        .meshes
        .iter()
        .find(|m| m.material.effect == Effect::PortalWater && !m.vertices.is_empty())
        .map(|mesh| {
            let plane =
                mesh.vertices.iter().map(|v| v.pos[2]).sum::<f32>() / mesh.vertices.len() as f32;
            let water_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("water render targets layout"),
                entries: &[texture_entry(0), texture_entry(1), sampler_entry(2)],
            });
            let sample_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("water surface pipeline layout"),
                bind_group_layouts: &[
                    Some(&camera_layout),
                    Some(&material_layout),
                    Some(&water_layout),
                ],
                ..Default::default()
            });
            let copy_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("water background pipeline layout"),
                bind_group_layouts: &[None, None, Some(&water_layout)],
                ..Default::default()
            });
            let copy = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("copy refraction color to main scene"),
                layout: Some(&copy_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_copy"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_copy"),
                    targets: &[Some(format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            let water_camera = uniform(device, std::mem::size_of::<CameraUniform>());
            let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mirrored water camera"),
                layout: &camera_layout,
                entries: &[entry(0, water_camera.as_entire_binding())],
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let targets = WaterTargets::new(device, size, format, &water_layout, &sampler);
            Water {
                plane,
                camera: water_camera,
                camera_bind,
                layout: water_layout,
                sampler,
                targets,
                copy,
                surface: pipeline(&sample_layout, "fs_portal_water", false),
            }
        });
    Geometry {
        opaque: pipeline(&layout, "fs_main", false),
        blend: pipeline(&layout, "fs_main", true),
        camera,
        bind,
        meshes,
        water,
    }
}

fn draw_mesh(pass: &mut wgpu::RenderPass<'_>, mesh: &GpuMesh) {
    pass.set_bind_group(1, &mesh.material, &[]);
    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.count, 0, 0..1);
}

fn pn_triangles(
    vertices: &[Vertex],
    indices: &[u32],
    level: usize,
    curved: bool,
) -> (Vec<Vertex>, Vec<u32>) {
    let n = level * level * indices.len();
    let mut output = Vec::with_capacity(n);
    for index in indices.chunks_exact(3) {
        let vs = [index[0], index[1], index[2]].map(|i| vertices[i as usize]);
        let [a, b, c] = vs.map(|v| Vec3::from(v.pos));
        let [na, nb, nc] = vs.map(|v| Vec3::from(v.normal));
        let edge = |a: Vec3, b: Vec3, n: Vec3| (a * 2.0 + b - n * (b - a).dot(n)) / 3.0;
        let controls = [
            edge(a, b, na),
            edge(b, a, nb),
            edge(b, c, nb),
            edge(c, b, nc),
            edge(c, a, nc),
            edge(a, c, na),
        ];
        let average = controls.iter().sum::<Vec3>() / 6.0;
        let center = (average * 3.0 - (a + b + c) / 3.0) / 2.0;
        let sample = |i: usize, j: usize| {
            let v = i as f32 / level as f32;
            let w = j as f32 / level as f32;
            let u = 1.0 - v - w;
            let point = a * u.powi(3)
                + b * v.powi(3)
                + c * w.powi(3)
                + 3.0
                    * (controls[0] * (u * u * v)
                        + controls[1] * (u * v * v)
                        + controls[2] * (v * v * w)
                        + controls[3] * (v * w * w)
                        + controls[4] * (w * w * u)
                        + controls[5] * (w * u * u))
                + center * (6.0 * u * v * w);
            let point = if curved { point } else { a * u + b * v + c * w };
            let normal = (na * u + nb * v + nc * w).normalize_or_zero();
            let uv = [0, 1].map(|k| vs[0].uv[k] * u + vs[1].uv[k] * v + vs[2].uv[k] * w);
            Vertex::new(point.into(), normal.into(), uv)
        };
        for i in 0..level {
            for j in 0..level - i {
                output.extend([sample(i, j), sample(i + 1, j), sample(i, j + 1)]);
                if i + j + 1 < level {
                    output.extend([sample(i + 1, j), sample(i + 1, j + 1), sample(i, j + 1)]);
                }
            }
        }
    }
    let indices = (0..output.len() as u32).collect();
    (output, indices)
}

fn perspective(fov: f32, aspect: f32, near: f32, far: f32, hand: f32) -> Mat4 {
    let y = (fov * 0.5).tan().recip();
    let z = far / (far - near);
    Mat4::from_cols_array_2d(&[
        [y / aspect, 0.0, 0.0, 0.0],
        [0.0, y, 0.0, 0.0],
        [0.0, 0.0, hand * z, hand],
        [0.0, 0.0, -near * z, 0.0],
    ])
}

fn transform_vertex(vertex: Vertex, transform: Mat4) -> Vertex {
    Vertex {
        pos: transform.transform_point3(vertex.pos.into()).to_array(),
        normal: transform
            .transform_vector3(vertex.normal.into())
            .normalize_or_zero()
            .to_array(),
        ..vertex
    }
}

fn joint_transforms(
    origin: Mat4,
    track: &crate::sushi::Track,
    time: f32,
    skin: &[crate::sushi::Skin],
) -> Vec<Mat4> {
    let max_joint = skin.iter().flat_map(|skin| skin.joints).max().unwrap_or(0);
    (0..=max_joint.min(256))
        .map(|joint| origin * track.matrix(time, joint))
        .collect()
}

fn skin_vertex(v: Vertex, skin: &crate::sushi::Skin, joints: &[Mat4], origin: Mat4) -> Vertex {
    let mut pos = Vec3::ZERO;
    let mut normal = Vec3::ZERO;
    let sum = skin.weights.iter().sum::<f32>();
    if sum <= 0.0 {
        return transform_vertex(v, joints.get(skin.joints[0]).copied().unwrap_or(origin));
    }
    for (joint, weight) in skin.joints.into_iter().zip(skin.weights) {
        if weight > 0.0 {
            let m = joints.get(joint).copied().unwrap_or(origin);
            pos += m.transform_point3(v.pos.into()) * weight;
            normal += m.transform_vector3(v.normal.into()) * weight;
        }
    }
    Vertex {
        pos: (pos / sum).into(),
        normal: normal.normalize_or_zero().into(),
        ..v
    }
}

#[derive(Default)]
pub struct Controls {
    pub time: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub paused: bool,
    pub audio: Option<f32>,
    pub preset: usize,
    pub stencil: usize,
    pub flat_triangles: bool,
    pub showcase: u8,
    pub showcase_started: f32,
    last: Option<f64>,
}

impl Controls {
    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " => self.paused = !self.paused,
            "r" | "R" => {
                let audio = self.audio;
                *self = Self {
                    audio,
                    ..Default::default()
                };
            }
            "ArrowLeft" => self.time = (self.time - 2.0).max(0.0),
            "ArrowRight" => self.time += 2.0,
            "m" | "M" => self.preset = self.preset.wrapping_add(1),
            "s" | "S" => self.stencil = self.stencil.wrapping_add(1),
            "t" | "T" => self.flat_triangles = !self.flat_triangles,
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" => {
                self.showcase = key.as_bytes()[0] - b'0';
                self.showcase_started = self.time;
            }
            _ => return false,
        }
        true
    }

    fn advance(&mut self, now: f64) {
        let elapsed = now - self.last.replace(now).unwrap_or(now);
        if !self.paused {
            self.time += elapsed.clamp(0.0, 0.15) as f32;
        }
    }
}

impl Renderer {
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        scene: Scene,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Self {
        let textures = scene
            .textures
            .iter()
            .map(|t| texture_view(&device, &queue, t))
            .collect::<Vec<_>>();
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("wrap and trilinear filtering"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let pass = if let Some(settings) = &scene.geo {
            Pass::Geo(build_geo(&device, &textures, &sampler, settings, format))
        } else {
            Pass::Geometry(Box::new(build_geometry(
                &device, &textures, &sampler, &scene, format, size,
            )))
        };
        let depth = depth(&device, size);
        Self {
            device,
            queue,
            format,
            pass,
            scene,
            depth,
            size,
        }
    }

    fn resize(&mut self, size: (u32, u32)) {
        if size != self.size {
            self.depth = depth(&self.device, size);
            if let Pass::Geometry(geometry) = &mut self.pass
                && let Some(water) = &mut geometry.water
            {
                water.targets = WaterTargets::new(
                    &self.device,
                    size,
                    self.format,
                    &water.layout,
                    &water.sampler,
                );
            }
            self.size = size;
        }
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        controls: &Controls,
    ) {
        let aspect = self.size.0 as f32 / self.size.1 as f32;
        let time = self.scene.time(self.scene.initial_time + controls.time);
        let srgb = if self.format.is_srgb() { 1.0 } else { 0.0 };
        let [r, g, b] = self.scene.clear;
        let colors = [Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                store: wgpu::StoreOp::Store,
            },
        })];

        match &self.pass {
            Pass::Geo(geo) => {
                let settings = self.scene.geo.as_ref().unwrap();
                let rhythm = (time * 2.1).fract();
                let energy = controls
                    .audio
                    .unwrap_or(0.18 + (-rhythm * 5.0).exp() * 0.75);
                let centers = std::array::from_fn(|i| {
                    let shift = i as f32 * std::f32::consts::FRAC_PI_3;
                    let phase = time * (0.45 + i as f32 * 0.023) + shift;
                    let radius = 0.93 + 0.32 * (phase * 0.78).sin() + energy * 0.26;
                    [
                        (phase * 1.03).cos() * radius,
                        (phase * 1.27).sin() * radius * 0.74,
                        (phase * 1.83).cos() * 0.63,
                        0.51 + 0.13 * (phase * 1.59).cos() + energy * 0.25,
                    ]
                });
                let stencil = controls.stencil % geo.binds.len();
                let data = GeoUniform {
                    centers,
                    options: [time, energy, (controls.preset % 4) as f32, stencil as f32],
                    aspect_fov: [aspect, settings.fov, srgb, 0.0],
                };
                self.queue.write_buffer(&geo.frame, 0, bytes_of(&data));
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("GeoForms"),
                    color_attachments: &colors,
                    ..Default::default()
                });
                pass.set_pipeline(&geo.pipeline);
                pass.set_bind_group(0, &geo.binds[stencil], &[]);
                pass.draw(0..3, 0..1);
            }
            Pass::Geometry(geometry) => {
                let is_chest = self.scene.demo == Demo::TreasureChest;
                let (path, camera_time) = if let Some(chest) = &self.scene.chest
                    && controls.showcase > 0
                {
                    (&chest.path, controls.time - controls.showcase_started)
                } else {
                    (&self.scene.path, time)
                };
                let (eye, rotation, fov) = camera_on(path, camera_time, self.scene.fov);
                let mut lights = [[0.0; 4]; 4];
                if let Some(chest) = &self.scene.chest {
                    for (index, path) in chest.lights.iter().enumerate() {
                        let (pos, _, _) = camera_on(path, time, 55.0);
                        lights[index] = [pos.x, pos.y, pos.z, 1.0];
                    }
                }
                let zoom = controls.zoom.exp().clamp(0.35, 2.5);
                let rotation_delta =
                    Quat::from_euler(glam::EulerRot::YXZ, controls.yaw, controls.pitch, 0.0);
                let (projection, view) = if is_chest {
                    (
                        perspective(fov.to_radians() * zoom, aspect, 0.035, self.scene.far, 1.0),
                        Mat4::from_quat((rotation * rotation_delta).conjugate())
                            * Mat4::from_translation(-eye),
                    )
                } else {
                    let vertical = 2.0 * ((fov.to_radians() * 0.5).tan() / (4.0 / 3.0)).atan();
                    (
                        perspective(vertical * zoom, aspect, 2.0, self.scene.far, -1.0),
                        Mat4::from_quat(rotation_delta * rotation) * Mat4::from_translation(-eye),
                    )
                };
                let fog = match self.scene.demo {
                    Demo::Dolphin => 0.00033,
                    Demo::Nature => 0.000039,
                    Demo::Island1 | Demo::Island2 | Demo::Island3 => 0.000027,
                    _ => 0.0,
                };
                let data = CameraUniform {
                    view_projection: (projection * view).to_cols_array_2d(),
                    eye_time: [eye.x, eye.y, eye.z, time],
                    fog: [r as f32, g as f32, b as f32, fog],
                    options: [if is_chest { 1.0 } else { 0.0 }, srgb, 0.0, 0.0],
                    lights,
                };
                self.queue
                    .write_buffer(&geometry.camera, 0, bytes_of(&data));
                let mut visible = Vec::with_capacity(geometry.meshes.len());
                for mesh in &geometry.meshes {
                    let src = &self.scene.meshes[mesh.source];
                    if src.showcase != 0 && src.showcase != controls.showcase {
                        continue;
                    }
                    let Some(motion) = src.motion.map(|i| &self.scene.motions[i]) else {
                        visible.push(mesh);
                        continue;
                    };
                    let vertices: Vec<Vertex> = match motion {
                        Motion::ChestLid => {
                            let cycle = time.rem_euclid(13.0);
                            let open = ((cycle - 2.5) / 2.0).clamp(0.0, 1.0)
                                * ((12.5 - cycle) / 2.0).clamp(0.0, 1.0);
                            let pivot = Vec3::new(0.0, 2.49, 0.97);
                            let matrix = Mat4::from_translation(pivot)
                                * Mat4::from_rotation_x(-open * 0.88)
                                * Mat4::from_translation(-pivot);
                            src.vertices
                                .iter()
                                .map(|&v| transform_vertex(v, matrix))
                                .collect()
                        }
                        Motion::Skeletal {
                            origin,
                            start,
                            track,
                            skin,
                            pn_level,
                        } => {
                            let local = time - start;
                            if local < 0.0 {
                                continue;
                            }
                            let joints = joint_transforms(*origin, track, local, skin);
                            let vertices: Vec<Vertex> = src
                                .vertices
                                .iter()
                                .zip(skin)
                                .map(|(&v, skin)| skin_vertex(v, skin, &joints, *origin))
                                .collect();
                            if *pn_level > 1 {
                                pn_triangles(
                                    &vertices,
                                    &src.indices,
                                    *pn_level,
                                    !controls.flat_triangles,
                                )
                                .0
                            } else {
                                vertices
                            }
                        }
                        Motion::Morph {
                            origin,
                            start,
                            track,
                            frames,
                            skin,
                        } => {
                            let local = time - start;
                            if local < 0.0 {
                                continue;
                            }
                            let phase = track.animation_time(local) / track.duration.max(0.001)
                                * (frames.len() - 1) as f32;
                            let a = phase as usize;
                            let b = (a + 1).min(frames.len() - 1);
                            let fraction = phase.fract();
                            let joints = joint_transforms(*origin, track, local, skin);
                            frames[a]
                                .iter()
                                .zip(frames[b].iter())
                                .zip(skin)
                                .map(|((&a, b), skin)| {
                                    let pos = Vec3::from(a.pos).lerp(b.pos.into(), fraction);
                                    let normal =
                                        Vec3::from(a.normal).lerp(b.normal.into(), fraction);
                                    skin_vertex(
                                        Vertex {
                                            pos: pos.into(),
                                            normal: normal.into(),
                                            ..a
                                        },
                                        skin,
                                        &joints,
                                        *origin,
                                    )
                                })
                                .collect()
                        }
                    };
                    self.queue
                        .write_buffer(&mesh.vertices, 0, cast_slice(&vertices));
                    visible.push(mesh);
                }
                let depth_attachment = |load, store| wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations { load, store }),
                    stencil_ops: None,
                };
                let clear = wgpu::LoadOp::Clear(1.0);
                let is_portal = |mesh: &GpuMesh| {
                    self.scene.meshes[mesh.source].material.effect == Effect::PortalWater
                };
                let is_blended = |mesh: &GpuMesh| self.scene.meshes[mesh.source].material.blend;
                if let Some(water) = &geometry.water {
                    let mirror = Mat4::from_translation(Vec3::Z * (water.plane * 2.0))
                        * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
                    let mut mirror_data = data;
                    mirror_data.view_projection = (projection * view * mirror).to_cols_array_2d();
                    mirror_data.eye_time[2] = water.plane * 2.0 - eye.z;
                    mirror_data.options[2] = 1.0;
                    mirror_data.options[3] = water.plane - 5.0;
                    self.queue
                        .write_buffer(&water.camera, 0, bytes_of(&mirror_data));
                    for (capture, camera_bind, store) in [
                        (
                            &water.targets.reflection,
                            &water.camera_bind,
                            wgpu::StoreOp::Discard,
                        ),
                        (
                            &water.targets.refraction,
                            &geometry.bind,
                            wgpu::StoreOp::Store,
                        ),
                    ] {
                        let attachment = wgpu::RenderPassColorAttachment {
                            view: capture,
                            ..colors[0].clone().unwrap()
                        };
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("water scene capture"),
                            color_attachments: &[Some(attachment)],
                            depth_stencil_attachment: Some(depth_attachment(clear, store)),
                            ..Default::default()
                        });
                        pass.set_pipeline(&geometry.opaque);
                        pass.set_bind_group(0, camera_bind, &[]);
                        for mesh in visible
                            .iter()
                            .copied()
                            .filter(|m| !is_portal(m) && !is_blended(m))
                        {
                            draw_mesh(&mut pass, mesh);
                        }
                    }
                }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ATI demo"),
                    color_attachments: &colors,
                    depth_stencil_attachment: Some(depth_attachment(
                        if geometry.water.is_some() {
                            wgpu::LoadOp::Load
                        } else {
                            clear
                        },
                        wgpu::StoreOp::Discard,
                    )),
                    ..Default::default()
                });
                if let Some(water) = &geometry.water {
                    pass.set_pipeline(&water.copy);
                    pass.set_bind_group(2, &water.targets.bind, &[]);
                    pass.draw(0..3, 0..1);
                    pass.set_pipeline(&water.surface);
                    pass.set_bind_group(0, &geometry.bind, &[]);
                    for mesh in visible.iter().copied().filter(|m| is_portal(m)) {
                        draw_mesh(&mut pass, mesh);
                    }
                } else {
                    pass.set_pipeline(&geometry.opaque);
                    pass.set_bind_group(0, &geometry.bind, &[]);
                    for mesh in visible.iter().copied().filter(|m| !is_blended(m)) {
                        draw_mesh(&mut pass, mesh);
                    }
                }
                pass.set_pipeline(&geometry.blend);
                pass.set_bind_group(0, &geometry.bind, &[]);
                for mesh in visible.iter().copied().filter(|m| is_blended(m)) {
                    draw_mesh(&mut pass, mesh);
                }
            }
        }
    }
}

pub async fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
    Ok(adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("GPU demo gallery"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
            ..Default::default()
        })
        .await?)
}

pub struct Player {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    pub controls: Controls,
}

impl Player {
    pub async fn new(
        instance: &wgpu::Instance,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: (u32, u32),
        scene: Scene,
    ) -> Result<Self> {
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        let (device, queue) = device(&adapter).await?;
        let size = (size.0.max(1), size.1.max(1));
        let mut config = surface
            .get_default_config(&adapter, size.0, size.1)
            .ok_or("GPU can't configure this surface")?;
        if let Some(format) = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
        {
            config.format = format;
        }
        surface.configure(&device, &config);
        let renderer = Renderer::new(device, queue, scene, config.format, size);
        Ok(Self {
            surface,
            config,
            renderer,
            controls: Controls::default(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let max = self.renderer.device.limits().max_texture_dimension_2d;
        let size = (width.clamp(1, max), height.clamp(1, max));
        if self.renderer.size == size {
            return;
        }
        self.renderer.resize(size);
        (self.config.width, self.config.height) = size;
        self.surface.configure(&self.renderer.device, &self.config);
    }

    pub fn frame(&mut self, now: f64) -> bool {
        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture)
            | CurrentSurfaceTexture::Suboptimal(texture) => texture,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.renderer.device, &self.config);
                return false;
            }
            _ => return false,
        };
        self.controls.advance(now);
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .renderer
            .device
            .create_command_encoder(&Default::default());
        self.renderer.draw(&mut encoder, &view, &self.controls);
        self.renderer.queue.submit([encoder.finish()]);
        self.renderer.queue.present(frame);
        true
    }
}
