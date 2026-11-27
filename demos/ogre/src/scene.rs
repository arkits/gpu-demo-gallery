//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use glam::{Mat4, Vec3};
use wgpu::TextureViewDimension::D2;
use wgpu::util::DeviceExt;

use crate::Error;
use crate::data::Files;
use crate::dds;
use crate::gpu::{DEPTH, Gpu, SLOT};
use crate::model::{Geometry, Model, bracket};
use crate::show::{self, Frame, ROOT};
use crate::tga;

/// One vertex: position, normal, tangent handedness, texture, occlusion.
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    tangent: [f32; 4],
    uv: [f32; 2],
    occ: f32,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    eye: [f32; 4],
    shadow_proj: [[f32; 4]; 4],
    _pad: [[f32; 4]; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct ShapeUniforms {
    model: [[f32; 4]; 4],
    diffuse: [f32; 4],
    specular: [f32; 4],
    ambient: [f32; 4],
    shadow_ambient: [f32; 2],
    bump_scale: [f32; 2],
    displace: f32,
    flags: u32,
    _pad: [f32; 2],
}

const FLAG_KILL: u32 = 1;
const FLAG_OCC: u32 = 2;
const FLAG_DIFFUSE_ONLY: u32 = 4;

/// Two Catmull-Clark levels as a linear operator: refined vertices as
/// weighted control vertices, with static texture coordinates and triangles.
struct SubdivOp {
    rows: Vec<Vec<(u32, f32)>>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

fn triangulate_faces(faces: &[Vec<u32>]) -> Vec<u32> {
    let mut out = Vec::with_capacity(faces.len() * 6);
    for f in faces {
        for k in 1..f.len().saturating_sub(1) {
            out.extend([f[0], f[k], f[k + 1]]);
        }
    }
    out
}

fn combine(next: &[Vec<(u32, f32)>], parts: &[(usize, f32)]) -> Vec<(u32, f32)> {
    let mut acc: HashMap<u32, f32> = HashMap::new();
    for &(i, w) in parts {
        for &(j, v) in &next[i] {
            *acc.entry(j).or_default() += w * v;
        }
    }
    acc.into_iter().collect()
}

impl SubdivOp {
    fn build(faces: &[Vec<u32>], n_control: usize) -> Self {
        let mut rows: Vec<Vec<(u32, f32)>> =
            (0..n_control as u32).map(|i| vec![(i, 1.0)]).collect();
        // Two Catmull-Clark levels, matching the demo's tessellation [I].
        let mut quads = faces.to_vec();
        for _ in 0..2 {
            let n = rows.len();
            let mut next: Vec<Vec<(u32, f32)>> = rows.clone();
            let mut face_of = vec![0usize; quads.len()];
            for (qi, q) in quads.iter().enumerate() {
                let w = 1.0 / q.len() as f32;
                let parts: Vec<(usize, f32)> = q.iter().map(|&i| (i as usize, w)).collect();
                face_of[qi] = next.len();
                let row = combine(&next, &parts);
                next.push(row);
            }
            let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
            for (qi, q) in quads.iter().enumerate() {
                for k in 0..4 {
                    let key = if q[k] < q[(k + 1) % q.len()] {
                        (q[k], q[(k + 1) % q.len()])
                    } else {
                        (q[(k + 1) % q.len()], q[k])
                    };
                    edge_faces.entry(key).or_default().push(qi);
                }
            }
            let mut edge_of: HashMap<(u32, u32), usize> = HashMap::new();
            for q in quads.iter() {
                for k in 0..4 {
                    let (a, b) = (q[k], q[(k + 1) % q.len()]);
                    let key = if a < b { (a, b) } else { (b, a) };
                    if edge_of.contains_key(&key) {
                        continue;
                    }
                    let row = if edge_faces[&key].len() == 2 {
                        combine(
                            &next,
                            &[
                                (a as usize, 0.25),
                                (b as usize, 0.25),
                                (face_of[edge_faces[&key][0]], 0.25),
                                (face_of[edge_faces[&key][1]], 0.25),
                            ],
                        )
                    } else {
                        combine(&next, &[(a as usize, 0.5), (b as usize, 0.5)])
                    };
                    edge_of.insert(key, next.len());
                    next.push(row);
                }
            }
            let mut adjacent: Vec<Vec<usize>> = vec![Vec::new(); n];
            let mut edges: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
            for (qi, q) in quads.iter().enumerate() {
                let len = q.len();
                for k in 0..len {
                    adjacent[q[k] as usize].push(qi);
                    let (a, b) = (q[k], q[(k + 1) % len]);
                    let key = if a < b { (a, b) } else { (b, a) };
                    if !edges[a as usize].contains(&key) {
                        edges[a as usize].push(key);
                    }
                    if !edges[b as usize].contains(&key) {
                        edges[b as usize].push(key);
                    }
                }
            }
            let mut vert_of = vec![0usize; n];
            for i in 0..n {
                let boundary: Vec<(u32, u32)> = edges[i]
                    .iter()
                    .filter(|k| edge_faces[*k].len() == 1)
                    .copied()
                    .collect();
                // Degenerate topology stays put instead of extrapolating.
                let row = if adjacent[i].len() < 3 && boundary.len() != 2 {
                    vec![(i as u32, 1.0)]
                } else if boundary.len() == 2 {
                    let mut others = [0u32; 2];
                    for (k, &(a, b)) in boundary.iter().enumerate() {
                        others[k] = if a as usize == i { b } else { a };
                    }
                    combine(
                        &next,
                        &[
                            (i, 0.75),
                            (others[0] as usize, 0.125),
                            (others[1] as usize, 0.125),
                        ],
                    )
                } else if adjacent[i].is_empty() {
                    vec![(i as u32, 1.0)]
                } else {
                    let valence = adjacent[i].len() as f32;
                    let mut parts = Vec::new();
                    for &fi in &adjacent[i] {
                        parts.push((face_of[fi], 1.0 / valence / valence));
                    }
                    // R is the average of control edge midpoints [V].
                    let e = edges[i].len() as f32;
                    for &key in &edges[i] {
                        parts.push((key.0 as usize, 1.0 / valence / e));
                        parts.push((key.1 as usize, 1.0 / valence / e));
                    }
                    parts.push((i, (valence - 3.0) / valence));
                    combine(&next, &parts)
                };
                vert_of[i] = next.len();
                next.push(row);
            }
            let mut grown = Vec::with_capacity(quads.len() * 4);
            for (qi, q) in quads.iter().enumerate() {
                for k in 0..4 {
                    let v = q[k] as usize;
                    let e_next = {
                        let key = if q[k] < q[(k + 1) % q.len()] {
                            (q[k], q[(k + 1) % q.len()])
                        } else {
                            (q[(k + 1) % q.len()], q[k])
                        };
                        edge_of[&key]
                    };
                    let e_prev = {
                        let prev = q[(k + q.len() - 1) % q.len()];
                        let key = if prev < q[k] {
                            (prev, q[k])
                        } else {
                            (q[k], prev)
                        };
                        edge_of[&key]
                    };
                    grown.push(vec![
                        vert_of[v] as u32,
                        e_next as u32,
                        face_of[qi] as u32,
                        e_prev as u32,
                    ]);
                }
            }
            rows = next;
            quads = grown;
        }
        let indices = triangulate_faces(&quads);
        // Static uvs through the same operator, from the control uvs.
        Self {
            rows,
            uvs: Vec::new(),
            indices,
        }
    }

    fn apply(&self, control: &[[f32; 3]], occlusion: Option<&[f32]>) -> (Vec<[f32; 3]>, Vec<f32>) {
        let control: Vec<Vec3> = control.iter().map(|p| Vec3::from(*p)).collect();
        let mut positions = Vec::with_capacity(self.rows.len());
        let mut occ = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            let mut p = Vec3::ZERO;
            let mut o = 0.0;
            for &(i, w) in row {
                p += control[i as usize] * w;
                if let Some(list) = occlusion {
                    o += list[i as usize] * w;
                }
            }
            positions.push(p.to_array());
            occ.push(o);
        }
        (positions, occ)
    }

    fn apply_uv(&self, control: &[[f32; 2]]) -> Vec<[f32; 2]> {
        let control: Vec<glam::Vec2> = control.iter().map(|t| glam::Vec2::from(*t)).collect();
        self.rows
            .iter()
            .map(|row| {
                let mut p = glam::Vec2::ZERO;
                for &(i, w) in row {
                    p += control[i as usize] * w;
                }
                p.to_array()
            })
            .collect()
    }
}

/// Smooth normals of a triangle soup, area-weighted.
fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (
            Vec3::from(positions[t[0] as usize]),
            Vec3::from(positions[t[1] as usize]),
            Vec3::from(positions[t[2] as usize]),
        );
        let n = (b - a).cross(c - a);
        normals[t[0] as usize] += n;
        normals[t[1] as usize] += n;
        normals[t[2] as usize] += n;
    }
    normals
        .into_iter()
        .map(|n| n.normalize_or_zero().to_array())
        .collect()
}

/// Tangents from positions and texture coordinates, handed by `w`.
fn tangents(positions: &[[f32; 3]], uvs: &[[f32; 2]], indices: &[u32]) -> Vec<[f32; 4]> {
    if uvs.len() != positions.len() {
        return vec![[1.0, 0.0, 0.0, 1.0]; positions.len()];
    }
    let mut tan = vec![Vec3::ZERO; positions.len()];
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (
            Vec3::from(positions[t[0] as usize]),
            Vec3::from(positions[t[1] as usize]),
            Vec3::from(positions[t[2] as usize]),
        );
        let (ua, ub, uc) = (
            glam::Vec2::from(uvs[t[0] as usize]),
            glam::Vec2::from(uvs[t[1] as usize]),
            glam::Vec2::from(uvs[t[2] as usize]),
        );
        let e1 = b - a;
        let e2 = c - a;
        let d1 = ub - ua;
        let d2 = uc - ua;
        let denom = d1.x * d2.y - d2.x * d1.y;
        if denom.abs() > 1e-12 {
            let s = (e1 * d2.y - e2 * d1.y) / denom;
            tan[t[0] as usize] += s;
            tan[t[1] as usize] += s;
            tan[t[2] as usize] += s;
        }
    }
    tan.into_iter()
        .map(|t| {
            let t = t.normalize_or_zero();
            [t.x, t.y, t.z, 1.0]
        })
        .collect()
}

fn to_vertex(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    tangents: &[[f32; 4]],
    uvs: &[[f32; 2]],
    occ: Option<&[f32]>,
) -> Vec<Vertex> {
    positions
        .iter()
        .enumerate()
        .map(|(i, p)| Vertex {
            pos: *p,
            normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
            tangent: tangents.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 1.0]),
            uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
            occ: occ.map_or(1.0, |o| o[i]),
        })
        .collect()
}

struct GGeom {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    capacity: usize,
    subdiv: SubdivKind,
}

enum SubdivKind {
    None,
    /// Baked split topology: tessellated positions are control positions
    /// through `split`, with baked texture coordinates and triangles.
    Baked {
        split: Vec<u32>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
    },
    /// Runtime Catmull-Clark levels.
    Refine {
        op: SubdivOp,
    },
}

struct Draw {
    geom: usize,
    shape: usize,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    slot: u32,
    casts_shadow: bool,
    displace: bool,
}

pub struct Scene {
    model: Model,
    geoms: Vec<GGeom>,
    draws: Vec<Draw>,
    geom_index: HashMap<String, usize>,
    uniforms: wgpu::Buffer,
    shape_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    global_group: wgpu::BindGroup,
    depth_globals: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    shadow_pipeline: wgpu::RenderPipeline,
    depth: Option<(u32, u32, wgpu::TextureView)>,
}

fn mat(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        let model = crate::model::Model::new(files)?;
        let device = &gpu.device;
        let mut pictures: HashMap<String, wgpu::TextureView> = HashMap::new();
        let mut view = |name: &str| -> Result<wgpu::TextureView, Error> {
            if let Some(v) = pictures.get(name) {
                return Ok(v.clone());
            }
            let file = model
                .textures
                .get(name)
                .ok_or_else(|| format!("no texture {name}"))?;
            let data = files.get(file)?;
            let texture = if file.to_ascii_lowercase().ends_with(".dds") {
                gpu.upload(&[dds::Image::read(data)?])?
            } else {
                let (w, h, pixels) = tga::read(data)?;
                gpu.upload_rgba((w as u32, h as u32), &[pixels], true)
            };
            let v = texture.create_view(&Default::default());
            pictures.insert(name.to_owned(), v.clone());
            Ok(v)
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let compare = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"),
            size: wgpu::Extent3d {
                width: 1024,
                height: 1024,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow.create_view(&Default::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ogre"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ogre.wgsl").into()),
        });
        let global_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let shape_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&global_layout),
                Some(&shape_layout),
                Some(&material_layout),
            ],
            ..Default::default()
        });
        let depth_globals_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let depth_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&depth_globals_layout), Some(&shape_layout)],
            ..Default::default()
        });
        let vertex_layout = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32
        ];
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_layout,
        })];
        let pipe = |vertex: &str, fragment: Option<&str>| {
            let targets = [Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: fragment,
                layout: Some(&pipeline_layout),
                cache: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                fragment: fragment.map(|fs| wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &targets,
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    // No culling: the backdrop is viewed from the inside,
                    // and it is also what the depth test expects [I].
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
            })
        };
        let fs_bump = pipe("vs_bump", Some("fs_bump"));
        let fs_bump_occ = pipe("vs_bump", Some("fs_bump_occ"));
        let fs_bump_diffuse = pipe("vs_bump", Some("fs_bump_diffuse"));
        let fs_ground = pipe("vs_ground", Some("fs_ground"));
        let fs_bg = pipe("vs_bg", Some("fs_bg"));
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&depth_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_depth"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                // No culling: the backdrop is viewed from the inside,
                // and it is also what the depth test expects [I].
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let slots = model.shapes.len().max(1) as u64 * SLOT;
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: slots,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shape_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &shape_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms,
                    offset: 0,
                    size: wgpu::BufferSize::new(SLOT),
                }),
            }],
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let global_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &global_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&compare),
                },
            ],
        });
        let depth_globals = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &depth_globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        // Geometries become GPU buffers.
        let mut geoms = Vec::new();
        let mut geom_index: HashMap<String, usize> = HashMap::new();
        for (name, geometry) in &model.geometries {
            let (verts, idx, subdiv) = match geometry {
                Geometry::Subdiv(control) => {
                    if let Some(baked) = &control.baked {
                        // The file's own tessellation: split control
                        // positions, baked texture and triangles.
                        let positions: Vec<[f32; 3]> = baked
                            .split
                            .iter()
                            .map(|&i| control.positions[i as usize])
                            .collect();
                        let occ: Vec<f32> = control
                            .occlusion_rest
                            .as_ref()
                            .map(|rest| baked.split.iter().map(|&i| rest[i as usize]).collect())
                            .unwrap_or_else(|| vec![1.0; positions.len()]);
                        let normals = smooth_normals(&positions, &baked.indices);
                        let tangents = tangents(&positions, &baked.uvs, &baked.indices);
                        (
                            to_vertex(&positions, &normals, &tangents, &baked.uvs, Some(&occ)),
                            baked.indices.clone(),
                            SubdivKind::Baked {
                                split: baked.split.clone(),
                                uvs: baked.uvs.clone(),
                                indices: baked.indices.clone(),
                            },
                        )
                    } else if control.levels > 0 {
                        let op = SubdivOp::build(&control.faces, control.positions.len());
                        let uvs = op.apply_uv(&control.uvs);
                        let (positions, _) = op.apply(&control.positions, None);
                        let normals = smooth_normals(&positions, &op.indices);
                        let tangents = tangents(&positions, &uvs, &op.indices);
                        (
                            to_vertex(
                                &positions,
                                &normals,
                                &tangents,
                                &uvs,
                                Some(&vec![1.0; positions.len()]),
                            ),
                            op.indices.clone(),
                            SubdivKind::Refine { op },
                        )
                    } else {
                        // Level zero: the control mesh itself.
                        let idx = triangulate_faces(&control.faces);
                        let normals = smooth_normals(&control.positions, &idx);
                        let tangents = tangents(&control.positions, &control.uvs, &idx);
                        (
                            to_vertex(&control.positions, &normals, &tangents, &control.uvs, None),
                            idx,
                            SubdivKind::None,
                        )
                    }
                }
                Geometry::Baked(baked) => {
                    let occ = vec![1.0; baked.positions.len()];
                    (
                        to_vertex(
                            &baked.positions,
                            &baked.normals,
                            &baked.tangents,
                            &baked.uvs,
                            Some(&occ),
                        ),
                        baked.indices.clone(),
                        SubdivKind::None,
                    )
                }
                Geometry::Static {
                    positions,
                    normals,
                    tangents,
                    uvs,
                    indices,
                } => (
                    to_vertex(positions, normals, tangents, uvs, None),
                    indices.clone(),
                    SubdivKind::None,
                ),
            };
            let capacity = verts.len();
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&idx),
                usage: wgpu::BufferUsages::INDEX,
            });
            geom_index.insert(name.clone(), geoms.len());
            geoms.push(GGeom {
                vertices,
                indices,
                count: idx.len() as u32,
                capacity,
                subdiv,
            });
        }
        // Shapes become draw calls.
        let mut draws = Vec::new();
        for (si, shape) in model.shapes.iter().enumerate() {
            let Some(&gi) = geom_index.get(&shape.geometry) else {
                continue;
            };
            let pipeline = match shape.fragment.as_str() {
                "bump_occ_f" => &fs_bump_occ,
                "bump_diffuse_f" => &fs_bump_diffuse,
                "shadow_f" => &fs_ground,
                "bg_f" => &fs_bg,
                _ => &fs_bump,
            };
            let group = if shape.fragment == "bg_f" || shape.fragment == "shadow_f" {
                let name = if shape.fragment == "bg_f" {
                    shape.textures.get("texture").map_or("", String::as_str)
                } else {
                    shape
                        .textures
                        .get("colorTexture")
                        .map_or("", String::as_str)
                };
                let color = view(name)?;
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&color),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&color),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&color),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                })
            } else {
                let mut get = |param: &str| -> Result<wgpu::TextureView, Error> {
                    let texture = shape
                        .textures
                        .get(param)
                        .ok_or_else(|| format!("{}: no {param}", shape.name))?;
                    view(texture)
                };
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&get("diffTex")?),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&get("bumpTex")?),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&get("phongTex")?),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                })
            };
            draws.push(Draw {
                geom: gi,
                shape: si,
                pipeline: pipeline.clone(),
                group,
                slot: si as u32,
                casts_shadow: shape.fragment.starts_with("bump"),
                displace: shape.vertex == "bump_displace_v",
            });
        }
        Ok(Self {
            model,
            geoms,
            draws,
            geom_index,
            uniforms,
            shape_group,
            globals,
            global_group,
            depth_globals,
            shadow_view,
            shadow_pipeline,
            depth: None,
        })
    }

    pub fn show(&self) -> show::Show {
        show::Show::new()
    }

    /// Advances the animation and redraws every dynamic buffer.
    fn update(&mut self, gpu: &Gpu, seconds: f64) {
        let wrap = |times: &[f64], t: f64| {
            if times.len() < 2 {
                times.first().copied().unwrap_or(0.0)
            } else {
                let (start, end) = (times[0], times[times.len() - 1]);
                if end <= start {
                    start
                } else {
                    start + (t - start).rem_euclid(end - start)
                }
            }
        };
        struct Update {
            gi: usize,
            verts: Vec<Vertex>,
        }
        let mut updates = Vec::new();
        let names: Vec<String> = self.model.geometries.keys().cloned().collect();
        for name in &names {
            let Some(&gi) = self.geom_index.get(name) else {
                continue;
            };
            let Some(geometry) = self.model.geometries.get(name) else {
                continue;
            };
            match geometry {
                Geometry::Subdiv(control) => {
                    if control.frames.is_empty() {
                        continue;
                    }
                    let times: Vec<f64> = control.frames.iter().map(|f| f.time).collect();
                    let t = wrap(&times, seconds);
                    let (positions, occlusion) =
                        Model::control_at(&control.frames, t).unwrap_or_default();
                    match &self.geoms[gi].subdiv {
                        SubdivKind::Baked {
                            split,
                            uvs,
                            indices,
                        } => {
                            let positions: Vec<[f32; 3]> =
                                split.iter().map(|&i| positions[i as usize]).collect();
                            let occ: Vec<f32> = occlusion.map_or_else(
                                || vec![1.0; positions.len()],
                                |o| split.iter().map(|&i| o[i as usize]).collect(),
                            );
                            let normals = smooth_normals(&positions, indices);
                            let tangents = tangents(&positions, uvs, indices);
                            updates.push(Update {
                                gi,
                                verts: to_vertex(&positions, &normals, &tangents, uvs, Some(&occ)),
                            });
                        }
                        SubdivKind::Refine { op } => {
                            let has_occ = control.occlusion_rest.is_some();
                            let (positions, occ) = op.apply(&positions, occlusion.as_deref());
                            let occ = if has_occ {
                                occ
                            } else {
                                vec![1.0; positions.len()]
                            };
                            let normals = smooth_normals(&positions, &op.indices);
                            let tangents = tangents(&positions, &op.uvs, &op.indices);
                            updates.push(Update {
                                gi,
                                verts: to_vertex(
                                    &positions,
                                    &normals,
                                    &tangents,
                                    &op.uvs,
                                    Some(&occ),
                                ),
                            });
                        }
                        SubdivKind::None => {
                            let idx = triangulate_faces(&control.faces);
                            let normals = smooth_normals(&positions, &idx);
                            let tangents = tangents(&positions, &control.uvs, &idx);
                            updates.push(Update {
                                gi,
                                verts: to_vertex(
                                    &positions,
                                    &normals,
                                    &tangents,
                                    &control.uvs,
                                    None,
                                ),
                            });
                        }
                    }
                }
                Geometry::Baked(baked) => {
                    if baked.frames.is_empty() {
                        continue;
                    }
                    let times: Vec<f64> = baked.frames.iter().map(|f| f.time).collect();
                    let t = wrap(&times, seconds);
                    let (before, after, mix) = bracket(&baked.frames, t, |f| f.time);
                    let lerp3 = |a: &[[f32; 3]], b: &[[f32; 3]]| -> Vec<[f32; 3]> {
                        a.iter()
                            .zip(b)
                            .map(|(x, y)| {
                                glam::Vec3::from(*x)
                                    .lerp(glam::Vec3::from(*y), mix)
                                    .to_array()
                            })
                            .collect()
                    };
                    let lerp4 = |a: &[[f32; 4]], b: &[[f32; 4]]| -> Vec<[f32; 4]> {
                        a.iter()
                            .zip(b)
                            .map(|(x, y)| {
                                glam::Vec4::from(*x)
                                    .lerp(glam::Vec4::from(*y), mix)
                                    .to_array()
                            })
                            .collect()
                    };
                    let pick3 = |a: &[[f32; 3]], b: &[[f32; 3]]| {
                        if mix <= 0.0 {
                            a.to_vec()
                        } else if mix >= 1.0 {
                            b.to_vec()
                        } else {
                            lerp3(a, b)
                        }
                    };
                    let positions = pick3(&before.positions, &after.positions);
                    let normals = match (&before.normals, &after.normals) {
                        (Some(a), Some(b)) => pick3(a, b),
                        _ => smooth_normals(&positions, &baked.indices),
                    };
                    let tangents = match (&before.tangents, &after.tangents) {
                        (Some(a), Some(b)) => {
                            if mix <= 0.0 {
                                a.clone()
                            } else if mix >= 1.0 {
                                b.clone()
                            } else {
                                lerp4(a, b)
                            }
                        }
                        _ => tangents(&positions, &baked.uvs, &baked.indices),
                    };
                    updates.push(Update {
                        gi,
                        verts: to_vertex(&positions, &normals, &tangents, &baked.uvs, None),
                    });
                }
                Geometry::Static { .. } => {}
            }
        }
        for update in updates {
            let g = &self.geoms[update.gi];
            if update.verts.len() != g.capacity {
                continue;
            }
            gpu.queue
                .write_buffer(&g.vertices, 0, bytemuck::cast_slice(&update.verts));
        }
        for draw in &self.draws {
            let shape = &self.model.shapes[draw.shape];
            let matrix = self.model.transform_at(&shape.transform, seconds);
            let get = |name: &str, default: [f32; 4]| -> [f32; 4] {
                shape.constants.get(name).copied().unwrap_or(default)
            };
            let diffuse = get("diffuseColor", [1.0, 1.0, 1.0, 1.0]);
            let specular = get("specularColor", [0.5, 0.5, 0.5, 1.0]);
            let ambient = get("ambientColor", [0.2, 0.2, 0.2, 1.0]);
            let shadow_ambient = get("shadowAmbient", [0.5, 0.5, 0.0, 0.0]);
            let bump_scale = get("bumpScale", [1.0, 1.0, 0.0, 0.0]);
            let mut flags = 0;
            if shape.alpha_kill {
                flags |= FLAG_KILL;
            }
            if shape.fragment == "bump_occ_f" {
                flags |= FLAG_OCC;
            }
            if shape.fragment == "bump_diffuse_f" {
                flags |= FLAG_DIFFUSE_ONLY;
            }
            let uniforms = ShapeUniforms {
                model: mat(matrix),
                diffuse,
                specular,
                ambient,
                shadow_ambient: [shadow_ambient[0], shadow_ambient[1]],
                bump_scale: [bump_scale[0], bump_scale[1]],
                displace: if draw.displace {
                    get("displace", [0.8, 0.0, 0.0, 0.0])[0]
                } else {
                    0.0
                },
                flags,
                _pad: [0.0; 2],
            };
            gpu.queue.write_buffer(
                &self.uniforms,
                u64::from(draw.slot) * SLOT,
                bytemuck::bytes_of(&uniforms),
            );
        }
    }

    /// Light view and projection covering the whole performance [I].
    fn shadow_proj(&self) -> Mat4 {
        let dir = self.model.key_light_dir;
        let eye = ROOT - dir * 1500.0;
        let forward = (ROOT - eye).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        let view = Mat4::from_cols(
            right.extend(-right.dot(eye)),
            up.extend(-up.dot(eye)),
            (-forward).extend(forward.dot(eye)),
            glam::Vec4::W,
        );
        glam::camera::rh::proj::directx::orthographic(-700.0, 700.0, -700.0, 700.0, 1.0, 4000.0)
            * view
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        frame: &Frame,
        view: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        self.update(gpu, frame.local);
        let camera = frame.camera.shot(ROOT);
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let proj = glam::camera::rh::proj::directx::perspective(camera.fov, aspect, 1.0, 8000.0);
        let eye = camera.place.w_axis.truncate();
        let shadow_proj = self.shadow_proj();
        gpu.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                view_proj: mat(proj * camera.place.inverse()),
                light_dir: [
                    self.model.key_light_dir.x,
                    self.model.key_light_dir.y,
                    self.model.key_light_dir.z,
                    0.0,
                ],
                eye: [eye.x, eye.y, eye.z, 1.0],
                shadow_proj: mat(shadow_proj),
                _pad: [[0.0; 4]; 2],
            }),
        );
        // Shadow pass: depth from the light.
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.depth_globals, &[]);
            for draw in &self.draws {
                if !draw.casts_shadow {
                    continue;
                }
                let g = &self.geoms[draw.geom];
                pass.set_bind_group(1, &self.shape_group, &[draw.slot * SLOT as u32]);
                pass.set_vertex_buffer(0, g.vertices.slice(..));
                pass.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..g.count, 0, 0..1);
            }
        }
        if self.depth.as_ref().is_none_or(|(w, h, _)| (*w, *h) != size) {
            let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth = Some((size.0, size.1, depth.create_view(&Default::default())));
        }
        let depth = self.depth.as_ref().map(|(_, _, v)| v).unwrap();
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ogre"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for draw in &self.draws {
                let g = &self.geoms[draw.geom];
                pass.set_pipeline(&draw.pipeline);
                pass.set_bind_group(0, &self.global_group, &[]);
                pass.set_bind_group(1, &self.shape_group, &[draw.slot * SLOT as u32]);
                pass.set_bind_group(2, &draw.group, &[]);
                pass.set_vertex_buffer(0, g.vertices.slice(..));
                pass.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..g.count, 0, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod cc_tests {
    use super::*;
    #[test]
    fn one_quad_subdivides() {
        let op = SubdivOp::build(&[vec![0, 1, 2, 3]], 4);
        // L1: 4 corners + 1 face + 4 edges + 4 moved = 13; L2: 13 + 4 + 12 + 13 = 42.
        assert_eq!(op.rows.len(), 42, "rows={}", op.rows.len());
        assert_eq!(op.indices.len(), 96);
        let pos = [
            [0.0f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let (p, _) = op.apply(&pos, None);
        for v in &p {
            assert!(
                v[0] >= -0.01 && v[0] <= 1.01 && v[1] >= -0.01 && v[1] <= 1.01,
                "{v:?}"
            );
            assert!(v[2].abs() < 1e-5, "{v:?}");
        }
        let mut centers = 0;
        for v in &p {
            if (v[0] - 0.5).abs() < 1e-4 && (v[1] - 0.5).abs() < 1e-4 {
                centers += 1;
            }
        }
        assert!(centers >= 1, "{p:?}");
    }

    #[test]
    fn vest_stays_bounded() {
        let Some(dir) = std::env::var_os(crate::demo::DATA_ENV) else {
            return;
        };
        let files = crate::data::Files::from_dir(std::path::Path::new(&dir)).unwrap();
        let model = crate::model::Model::new(&files).unwrap();
        let g = model.geometries.get("boss_vest_stitch").unwrap();
        let crate::model::Geometry::Subdiv(control) = g else {
            panic!()
        };
        let op = SubdivOp::build(&control.faces, control.positions.len());
        let (p, _) = op.apply(&control.positions, None);
        let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &p {
            for i in 0..3 {
                mn[i] = mn[i].min(v[i]);
                mx[i] = mx[i].max(v[i]);
            }
        }
        // Control bounds are about -19..39; subdivision must stay close.
        assert!(mn[0] > -40.0 && mx[0] < 60.0, "{mn:?} {mx:?}");
        assert!(mn[1] > -10.0 && mx[1] < 60.0, "{mn:?} {mx:?}");
        assert!(mn[2] > -40.0 && mx[2] < 60.0, "{mn:?} {mx:?}");
    }
}
