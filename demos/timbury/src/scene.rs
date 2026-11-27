//! GPU resources and frame submission shared by the native and browser players.
use std::collections::HashMap;

use glam::{Mat3, Mat4, Quat, Vec3, Vec4};
use wgpu::TextureViewDimension::{Cube, D2};

use crate::Error;
use crate::data::Files;
use crate::gpu::{DEPTH, Gpu, SAMPLES, SLOT};
use crate::model::{Blend, Fixed, Geometry, Model, Moving, Shape, Texture};
use crate::show::{Frame, Show};
use crate::{dds, demo, exr, tga, tree};

/// Vectors of bones in the shader.
const BONES: usize = 96;
/// Uniform offsets are multiples of this.
const STRIDE: u64 = (BONES as u64 * 64).next_multiple_of(SLOT);
const PARAM_STRIDE: u64 = SLOT;

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Uniforms {
    view: [[f32; 4]; 4],
    projection: [[f32; 4]; 4],
    eye: [f32; 4],
    time: [f32; 4],
    look: [f32; 4],
    size: [f32; 4],
    menu: [f32; 4],
    fade: [f32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Params {
    spec: [f32; 4],
    misc: [f32; 4],
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
    Grass {
        blades: wgpu::Buffer,
        indices: wgpu::Buffer,
        count: u32,
    },
}

struct GpuShape {
    buffers: Buffers,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    param: u64,
}

struct Actor {
    model: Model,
    shapes: Vec<GpuShape>,
}

struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved: wgpu::TextureView,
    blur_a: wgpu::TextureView,
    #[allow(dead_code)]
    blur_b: wgpu::TextureView,
}

pub struct Scene {
    timbury: Actor,
    bugs: Actor,
    bug_anim: crate::anim::Animation,
    set: Actor,
    sky: Actor,
    menu_def: crate::menu::MenuDef,
    menu_views: HashMap<String, wgpu::TextureView>,
    menu_pipeline: wgpu::RenderPipeline,
    menu_layout: wgpu::BindGroupLayout,
    menu_verts: wgpu::Buffer,
    menu_indices: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    bones: wgpu::Buffer,
    constants: wgpu::BindGroup,
    post: Post,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

struct Post {
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct MenuVert {
    pos: [f32; 2],
    uv: [f32; 2],
}

/// A shape to draw: actor, shape, bone slot, and optional bug.
struct Draw {
    actor: usize,
    shape: usize,
    bones: usize,
}

/// Loads each texture once.
struct Loader<'a> {
    gpu: &'a Gpu,
    files: &'a Files,
    views: HashMap<String, wgpu::TextureView>,
    samplers: HashMap<[bool; 2], wgpu::Sampler>,
    layout: wgpu::BindGroupLayout,
}

/// The faces of a cross as `FUN_00460a60` cuts it, +X, -X, +Y, -Y, +Z, -Z.
pub fn faces((w, h): (usize, usize), pixels: &[u16]) -> Result<(u32, Vec<Vec<u16>>), Error> {
    let side = w / 3;
    if side == 0 || w != side * 3 || h != side * 4 || pixels.len() != w * h * 4 {
        return Err("not a cross of six squares".into());
    }
    let cut = |column: usize, row: usize, turned: bool| -> Vec<u16> {
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
        models: &[&Model],
        name: Option<&String>,
        cube: bool,
    ) -> Result<wgpu::TextureView, Error> {
        // Texture nodes live in whichever file declares them.
        let texture = name.map(|n| {
            models
                .iter()
                .find_map(|m| m.textures.get(n))
                .ok_or_else(|| format!("no texture {n}"))
        });
        let file = match texture.transpose()? {
            Some(Texture::Plane { file, signed }) if !cube => (file.as_str(), *signed),
            Some(Texture::Cross { file }) if cube => (file.as_str(), false),
            Some(Texture::Normalize) | None => ("", false),
            Some(_) => return Err("a texture has the wrong shape".into()),
        };
        self.load(file.0, cube, file.1)
    }

    fn load(&mut self, file: &str, cube: bool, signed: bool) -> Result<wgpu::TextureView, Error> {
        let key = format!("{file} {cube} {signed}");
        if let Some(v) = self.views.get(&key) {
            return Ok(v.clone());
        }
        let lower = file.to_ascii_lowercase();
        let view = if file.is_empty() {
            // Stands in where a program reads nothing.
            self.blank(cube, signed)
        } else if lower.ends_with(".dds") {
            let data = self.files.get(file)?;
            self.gpu
                .upload(&[dds::Image::read(data)?])?
                .create_view(&Default::default())
        } else if lower.ends_with(".tga") {
            let data = self.files.get(file)?;
            let (w, h, pixels) = tga::read(data)?;
            if signed {
                self.gpu.upload_snorm((w as u32, h as u32), &[pixels])
            } else {
                self.gpu.upload_rgba((w as u32, h as u32), &[pixels], true)
            }
            .create_view(&Default::default())
        } else if lower.ends_with(".exr") {
            self.load_exr(file, cube)?
        } else {
            return Err(format!("unknown picture {file}").into());
        };
        self.views.insert(key, view.clone());
        Ok(view)
    }

    fn blank(&mut self, cube: bool, signed: bool) -> wgpu::TextureView {
        let key = if cube {
            "blank cube"
        } else if signed {
            "blank signed"
        } else {
            "blank plane"
        };
        if let Some(v) = self.views.get(key) {
            return v.clone();
        }
        // Neutral bump: no perturbation; neutral color: white.
        let px = if signed {
            [0, 0, 0, 0]
        } else {
            [255, 255, 255, 255]
        };
        let layers = vec![px.to_vec(); if cube { 6 } else { 1 }];
        let texture = if signed {
            self.gpu.upload_snorm((1, 1), &layers)
        } else {
            self.gpu.upload_rgba((1, 1), &layers, false)
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(if cube { Cube } else { D2 }),
            ..Default::default()
        });
        self.views.insert(key.to_owned(), view.clone());
        view
    }

    fn load_exr(&mut self, file: &str, cube: bool) -> Result<wgpu::TextureView, Error> {
        let (w, h, half) = exr::read(self.files.get(file)?)?;
        let device = &self.gpu.device;
        if cube {
            let (side, layers) = faces((w, h), &half)?;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (i, layer) in layers.iter().enumerate() {
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: i as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(layer),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(side * 8),
                        rows_per_image: Some(side),
                    },
                    wgpu::Extent3d {
                        width: side,
                        height: side,
                        depth_or_array_layers: 1,
                    },
                );
            }
            Ok(texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(Cube),
                ..Default::default()
            }))
        } else {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&half),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w as u32 * 8),
                    rows_per_image: Some(h as u32),
                },
                wgpu::Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
            );
            Ok(texture.create_view(&Default::default()))
        }
    }

    fn group(
        &mut self,
        planes: [wgpu::TextureView; 3],
        cubes: [wgpu::TextureView; 2],
        repeat: [bool; 2],
        signed: bool,
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
        let _ = signed;
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
            binding: 5,
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

    /// Which pictures a shape's fragment program reads.
    fn material(&mut self, model: &Model, shape: &Shape) -> Result<wgpu::BindGroup, Error> {
        // (color, bump, extra) planes and (diffuse, specular) cubes.
        let names: [&str; 5] = match shape.program.as_str() {
            p if p.starts_with("hair") => {
                ["colorTex", "anisoTex", "", "crossEnvDiffuseCubemap", ""]
            }
            p if p.starts_with("grass") => ["grassTex", "", "", "", ""],
            p if p.starts_with("lambert") => ["c_diffCol", "", "", "", ""],
            p if p.starts_with("_texture") || p.starts_with("_bkg") => ["texture", "", "", "", ""],
            _ => [
                "colorTex",
                "bump",
                "specularTex",
                "crossEnvDiffuseCubemap",
                "crossEnvSpecularCubemap",
            ],
        };
        let models: Vec<&Model> = vec![model];
        let mut views = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let texture = if name.is_empty() {
                None
            } else {
                shape.textures.get(*name)
            };
            views.push(self.view(&models, texture, i >= 3)?);
        }
        let [a, b, c, d, e] = views.try_into().map_err(|_| "five textures")?;
        Ok(self.group([a, b, c], [d, e], [false; 2], false))
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
    depth: Option<bool>,
    coverage: bool,
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
                multisample: samples(s.coverage),
                multiview_mask: None,
                cache: None,
            });
        self.made.push((key, pipeline.clone()));
        pipeline
    }

    fn shape(&mut self, shape: &Shape) -> (&'static str, Blend, bool) {
        let program = shape.program.as_str();
        let fragment = if program.starts_with("hair") {
            "fs_hair"
        } else if program.starts_with("grass") {
            "fs_grass"
        } else if program.starts_with("lambert") {
            "fs_lambert"
        } else if program.starts_with("glass") {
            "fs_glass"
        } else if program.starts_with("_texture") || program.starts_with("_bkg") {
            "fs_flat"
        } else {
            "fs_lit"
        };
        let blend = if shape.blend == Blend::Alpha
            || program.starts_with("glass")
            || program.starts_with("hair")
        {
            Blend::Alpha
        } else {
            Blend::None
        };
        (fragment, blend, shape.cull)
    }
}

/// A grass blade: root, height, width direction, phase, and color.
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Blade {
    root: [f32; 3],
    params: [f32; 4],
    color: [f32; 4],
}

/// One vertex of a blade strip.
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct GrassVert {
    root: [f32; 3],
    dir: [f32; 2],
    tall: f32,
    shape: [f32; 2],
    phase: f32,
    color: [f32; 3],
}

/// Grows blades over the set's earth meshes [I].
fn grow_grass(
    p: &crate::model::GrassParams,
    set: &Model,
    files: &Files,
) -> Result<(Vec<GrassVert>, Vec<u32>), Error> {
    let mut tris: Vec<[Vec3; 3]> = Vec::new();
    // The geometry names its ground; the set's earth is only a fallback.
    if !p.terrain.is_empty()
        && let Ok(data) = files.get(&crate::data::asset_name(&p.terrain))
        && let Ok(root) = tree::read_text(&String::from_utf8_lossy(data))
        && let Ok(ground) = Model::new(&root, files)
    {
        for shape in &ground.shapes {
            if let Geometry::Mesh(m) = &shape.geometry {
                for t in m.indices.chunks_exact(3) {
                    tris.push([
                        Vec3::from(m.moving[t[0] as usize].position),
                        Vec3::from(m.moving[t[1] as usize].position),
                        Vec3::from(m.moving[t[2] as usize].position),
                    ]);
                }
            }
        }
    }
    if tris.is_empty() {
        let rest = set.pose(None, &[]);
        for shape in &set.shapes {
            let Geometry::Mesh(m) = &shape.geometry else {
                continue;
            };
            // Only the set's earth, not the character.
            if !shape.name.contains("BL_SET") {
                continue;
            }
            // Rigid ground rides one bone.
            let placement = if shape.bones.len() == 1 {
                set.bones(shape, &rest).into_iter().next().unwrap()
            } else {
                Mat4::IDENTITY
            };
            for t in m.indices.chunks_exact(3) {
                tris.push([
                    placement.transform_point3(Vec3::from(m.moving[t[0] as usize].position)),
                    placement.transform_point3(Vec3::from(m.moving[t[1] as usize].position)),
                    placement.transform_point3(Vec3::from(m.moving[t[2] as usize].position)),
                ]);
            }
        }
    }
    if tris.is_empty() {
        return Err("grass has no ground".into());
    }
    // A coarse bucket grid so height queries stay cheap.
    let (mut lo, mut hi) = ([1e30f32; 3], [-1e30f32; 3]);
    for t in &tris {
        for v in t {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
    }
    let cell = 16.0;
    let nx = ((hi[0] - lo[0]) / cell).ceil().max(1.0) as usize;
    let nz = ((hi[2] - lo[2]) / cell).ceil().max(1.0) as usize;
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); nx * nz];
    for (i, t) in tris.iter().enumerate() {
        let (mut ax, mut az, mut bx, mut bz) = (1e30f32, 1e30f32, -1e30f32, -1e30f32);
        for v in t {
            ax = ax.min(v.x);
            az = az.min(v.z);
            bx = bx.max(v.x);
            bz = bz.max(v.z);
        }
        let (x0, x1) = (
            ((ax - lo[0]) / cell).floor().max(0.0) as usize,
            ((bx - lo[0]) / cell).ceil().min(nx as f32 - 1.0) as usize,
        );
        let (z0, z1) = (
            ((az - lo[2]) / cell).floor().max(0.0) as usize,
            ((bz - lo[2]) / cell).ceil().min(nz as f32 - 1.0) as usize,
        );
        for z in z0..=z1 {
            for x in x0..=x1 {
                grid[z * nx + x].push(i);
            }
        }
    }
    let height = |x: f32, z: f32| {
        let (cx, cz) = (
            ((x - lo[0]) / cell).floor() as isize,
            ((z - lo[2]) / cell).floor() as isize,
        );
        if cx < 0 || cz < 0 || cx >= nx as isize || cz >= nz as isize {
            return None;
        }
        let mut best: Option<f32> = None;
        for &i in &grid[cz as usize * nx + cx as usize] {
            let [a, b, c] = tris[i];
            let d = (b - a).cross(c - a);
            if d.y.abs() < 1e-6 {
                continue;
            }
            // Barycentric in xz.
            let v0 = Vec3::new(b.x - a.x, b.z - a.z, 0.0);
            let v1 = Vec3::new(c.x - a.x, c.z - a.z, 0.0);
            let v2 = Vec3::new(x - a.x, z - a.z, 0.0);
            let (d00, d01, d11, d20, d21) = (
                v0.x * v0.x + v0.y * v0.y,
                v0.x * v1.x + v0.y * v1.y,
                v1.x * v1.x + v1.y * v1.y,
                v2.x * v0.x + v2.y * v0.y,
                v2.x * v1.x + v2.y * v1.y,
            );
            let denom = d00 * d11 - d01 * d01;
            if denom.abs() < 1e-9 {
                continue;
            }
            let (v, w) = (
                (d11 * d20 - d01 * d21) / denom,
                (d00 * d21 - d01 * d20) / denom,
            );
            if v >= 0.0 && w >= 0.0 && v + w <= 1.0 {
                let t = -(d.x * (x - a.x) + d.z * (z - a.z)) / d.y + a.y;
                best = Some(best.map_or(t, |b: f32| if t > b { t } else { b }));
            }
        }
        best
    };
    let (min_x, max_x, min_z, max_z) = (lo[0], hi[0], lo[2], hi[2]);
    // Life-map gates density; white means alive [I].
    let life = if p.life_map.is_empty() {
        None
    } else {
        files
            .get(&crate::data::asset_name(&p.life_map))
            .ok()
            .and_then(|d| tga::read(d).ok())
    };
    let gate = |x: f32, z: f32| {
        let Some((w, h, px)) = &life else {
            return 1.0;
        };
        let (ix, iz) = (
            ((x - min_x) / (max_x - min_x) * *w as f32).clamp(0.0, *w as f32 - 1.0) as usize,
            ((z - min_z) / (max_z - min_z) * *h as f32).clamp(0.0, *h as f32 - 1.0) as usize,
        );
        // TGA rows start at the bottom, like the program's pictures.
        // Density lives in the green channel [I].
        px[(iz * w + ix) * 4 + 1] as f32 / 255.0
    };
    let mut blades = Vec::new();
    let mut lcg = 0x12345678u32;
    let mut rand = move || {
        lcg = lcg.wrapping_mul(1664525).wrapping_add(1013904223);
        (lcg >> 8) as f32 / 16777216.0
    };
    // The meadow around the character, in world units [I].
    let (cx, cz) = (141.0, 118.0);
    let radius = 260.0;
    let (mut min_x, mut max_x) = (cx - radius, cx + radius);
    let (mut min_z, mut max_z) = (cz - radius, cz + radius);
    if let Some(a) = p.area {
        (min_x, max_x, min_z, max_z) = (a[0], a[1], a[2], a[3]);
    }
    // Step for tens of thousands of blades at most [I].
    let area = (max_x - min_x) * (max_z - min_z);
    let step = (area / (15000.0 * p.density.max(0.25))).sqrt().max(1.0);
    let mut x = min_x;
    while x <= max_x {
        let mut z = min_z;
        while z <= max_z {
            let (jx, jz) = ((rand() - 0.5) * step, (rand() - 0.5) * step);
            let (gx, gz) = (x + jx, z + jz);
            if rand() < gate(gx, gz)
                && let Some(y) = height(gx, gz)
            {
                let angle = rand() * std::f32::consts::TAU;
                let tall = p.blade_size * (0.7 + 0.6 * rand());
                blades.push(Blade {
                    root: [gx, y, gz],
                    params: [
                        angle.cos(),
                        angle.sin(),
                        tall,
                        rand() * std::f32::consts::TAU,
                    ],
                    color: [0.55 + 0.2 * rand(), 0.65 + 0.2 * rand(), 0.3, 1.0],
                });
            }
            z += step;
        }
        x += step;
    }
    // One tapered strip per blade; the shader sways and widens it.
    let mut verts: Vec<GrassVert> = Vec::new();
    let mut indices = Vec::new();
    for b in &blades {
        let base = verts.len() as u32;
        let dir = [b.params[0], b.params[1]];
        for s in 0..=p.segments {
            let t = s as f32 / p.segments as f32;
            for side in [0.0, 1.0] {
                verts.push(GrassVert {
                    root: b.root,
                    dir,
                    tall: b.params[2],
                    shape: [t, side],
                    phase: b.params[3],
                    color: [b.color[0], b.color[1], b.color[2]],
                });
            }
        }
        for s in 0..p.segments as u32 {
            let r = base + s * 2;
            indices.extend([r, r + 1, r + 2, r + 1, r + 3, r + 2]);
        }
    }
    Ok((verts, indices))
}

impl Actor {
    fn new(
        gpu: &Gpu,
        model: Model,
        loader: &mut Loader<'_>,
        pipelines: &mut Pipelines<'_>,
        files: &Files,
        heights: &Model,
    ) -> Result<Self, Error> {
        let fixed = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x2, 4 => Float32x4, 5 => Float32x4];
        let moving = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let segment = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x2,
            4 => Float32x3, 5 => Float32x3, 6 => Float32x3, 7 => Float32x2
        ];
        let blade = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x2, 2 => Float32, 3 => Float32x2,
            4 => Float32, 5 => Float32x3
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
        let mut shapes = Vec::new();
        for shape in &model.shapes {
            use wgpu::BufferUsages as Use;
            let (fragment, blend, cull) = pipelines.shape(shape);
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
                Geometry::Plain(m) => Buffers::Mesh {
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
                Geometry::Grass(p) => {
                    let (verts, indices) = grow_grass(p, heights, files).unwrap_or_default();
                    Buffers::Grass {
                        blades: gpu.buffer(bytemuck::cast_slice(&verts), Use::VERTEX),
                        indices: gpu.buffer(bytemuck::cast_slice(&indices), Use::INDEX),
                        count: indices.len() as u32,
                    }
                }
            };
            // Grass indices ride along in a second buffer below.
            let _ = &blade;
            if shape.bones.len() * 3 > BONES {
                return Err(format!("{}: too many bones", shape.name).into());
            }
            let hair_layout = || {
                Some(wgpu::VertexBufferLayout {
                    array_stride: 88,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &segment,
                })
            };
            let grass_layout = || layout(&blade, size_of::<GrassVert>());
            let mesh_layouts = || {
                [
                    layout(&moving, size_of::<Moving>()),
                    layout(&fixed, size_of::<Fixed>()),
                ]
            };
            let hair = [hair_layout()];
            let gr = [grass_layout()];
            let me = mesh_layouts();
            let layouts: &[Option<wgpu::VertexBufferLayout>] = match &shape.geometry {
                Geometry::Hair(_) => &hair,
                Geometry::Grass(_) => &gr,
                _ => &me,
            };
            let pipeline = pipelines.make(Style {
                vertex: match &shape.geometry {
                    Geometry::Hair(_) => "vs_hair",
                    Geometry::Grass(_) => "vs_grass",
                    _ => "vs",
                },
                fragment,
                buffers: layouts,
                blend,
                cull: cull && !matches!(shape.geometry, Geometry::Hair(_)),
                depth: Some(true),
                coverage: false,
            });
            shapes.push(GpuShape {
                buffers,
                pipeline,
                group: loader
                    .material(&model, shape)
                    .map_err(|e| format!("{}: {e}", shape.name))?,
                param: 0,
            });
        }
        // Number the shapes for their parameter slots.
        for (i, s) in shapes.iter_mut().enumerate() {
            s.param = i as u64;
        }
        Ok(Self { model, shapes })
    }
}

impl Scene {
    pub fn new(gpu: &Gpu, files: &Files, format: wgpu::TextureFormat) -> Result<Self, Error> {
        let device = &gpu.device;
        let read = |name: &str| -> Result<_, Error> {
            let data = files.get(name)?;
            if name.ends_with(".nmb") {
                tree::read_binary(data)
            } else {
                tree::read_text(&String::from_utf8_lossy(data))
            }
            .map_err(|e| format!("{name}: {e}").into())
        };
        let model = |name: &str| -> Result<_, Error> {
            Model::new(&read(name)?, files).map_err(|e| format!("{name}: {e}").into())
        };
        let mut timbury = model(demo::TIMBURY)?;
        let timbury_anim = read(demo::ANIMATION)?;
        let timbury_anim = timbury_anim.nodes("animations").first().ok_or("no dance")?;
        timbury.add_animation(crate::anim::Animation::read(timbury_anim)?);
        let mut swarm = model(demo::SWARMBUG)?;
        let mut grass = model(demo::GRASS)?;
        let mut sky = model(demo::BACKGROUND)?;
        let mut terrain = model(demo::GRASS_TERRAIN)?;
        let hair = model(demo::HAIR)?;
        // Texture nodes live in whichever file declares them.
        let mut table = HashMap::new();
        for m in [&timbury, &swarm, &grass, &sky, &terrain, &hair] {
            table.extend(m.textures.clone());
        }
        for m in [&mut timbury, &mut swarm, &mut grass, &mut sky, &mut terrain] {
            for (k, v) in table.clone() {
                m.textures.entry(k).or_insert(v);
            }
        }
        timbury.absorb(hair).map_err(|e| format!("hair: {e}"))?;
        let bug_anim = read(demo::SWARMBUG_ANIM)?;
        let bug_anim = bug_anim
            .nodes("animations")
            .first()
            .ok_or("no bug animation")?;
        let bug_anim = crate::anim::Animation::read(bug_anim)?;
        if timbury.animations.is_empty() {
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
                entry(1, buffer(true, BONES as u64 * 64)),
                entry(2, buffer(true, PARAM_STRIDE)),
            ],
        });
        let mut textures: Vec<_> = (0..5)
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
            5,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        ));
        for t in &mut textures {
            t.visibility = wgpu::ShaderStages::FRAGMENT;
        }
        let second = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &textures,
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: D2,
                        multisampled: false,
                    },
                ),
                entry(2, buffer(false, 16)),
                entry(
                    3,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&first), Some(&second)],
            ..Default::default()
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&post_layout)],
            ..Default::default()
        });
        let mut pipelines = Pipelines {
            gpu,
            layout,
            shader: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/timbury.wgsl").into()),
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
        let timbury = Actor::new(gpu, timbury, &mut loader, &mut pipelines, files, &terrain)?;
        let bugs = Actor::new(gpu, swarm, &mut loader, &mut pipelines, files, &terrain)?;
        let set = Actor::new(
            gpu,
            grass,
            &mut loader,
            &mut pipelines,
            files,
            &timbury.model,
        )?;
        let sky = Actor::new(gpu, sky, &mut loader, &mut pipelines, files, &terrain)?;
        // Per-shape material constants, written once.
        let mut param_data = Vec::new();
        for actor in [&timbury, &bugs, &set, &sky] {
            for shape in &actor.model.shapes {
                let c = |name: &str| shape.constants.get(name).copied().unwrap_or([0.0; 4]);
                let spec = c("specularColor");
                let scale = c("specularScale")[0];
                let ambient = c("ambientColor");
                let shadow = c("shadow_str")[0];
                let bump = c("bump_scale")[0];
                let transp = c("transparency")[0];
                param_data.push(Params {
                    spec: [spec[0], spec[1], spec[2], scale],
                    misc: [ambient[0], shadow, bump, transp],
                });
            }
        }

        let slots: usize = [&timbury, &bugs, &set, &sky]
            .iter()
            .map(|a| a.shapes.len())
            .sum();
        assert_eq!(slots, param_data.len());
        let bug_slots = demo::BUGS * 3;
        let make = |size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let uniforms = make(frame_size.next_multiple_of(SLOT));
        let bones = make(STRIDE * (slots + bug_slots) as u64);
        let params = make(PARAM_STRIDE * slots.max(1) as u64);
        for (i, p) in param_data.iter().enumerate() {
            gpu.queue
                .write_buffer(&params, i as u64 * PARAM_STRIDE, bytemuck::bytes_of(p));
        }
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: part(&params, PARAM_STRIDE),
                },
            ],
        });
        // Post pipelines share the LDR format.
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/timbury.wgsl").into()),
        });
        let post = |vertex: &str, fragment: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &post_shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &post_shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let post_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // The menu's pictures.
        let menu_def = crate::menu::MenuDef::load(files, demo::MENU).unwrap_or_default();
        let menu_root = read(demo::MENU).unwrap_or_else(|_| tree::read_text("Scene {}").unwrap());
        let mut menu_views = HashMap::new();
        for t in menu_root.nodes("textures") {
            let file = t.text("filename");
            if file.is_empty() {
                continue;
            }
            let lower = file.to_ascii_lowercase();
            let view = if lower.ends_with(".dds") {
                let data = files.get(&crate::data::asset_name(file))?;
                gpu.upload(&[dds::Image::read(data)?])?
                    .create_view(&Default::default())
            } else if lower.ends_with(".tga") {
                let data = files.get(&crate::data::asset_name(file))?;
                let (w, h, pixels) = tga::read(data)?;
                gpu.upload_rgba((w as u32, h as u32), &[pixels], true)
                    .create_view(&Default::default())
            } else {
                continue;
            };
            menu_views.insert(t.name.clone(), view);
        }
        let menu_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let menu_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&menu_layout)],
            ..Default::default()
        });
        let menu_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
        let menu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("menu"),
            layout: Some(&menu_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &pipelines.shader,
                entry_point: Some("vs_menu"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<MenuVert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &menu_attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &pipelines.shader,
                entry_point: Some("fs_menu"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // Room for the overlay quads; rewritten whenever the menu changes.
        let menu_verts = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("menu"),
            size: (128 * size_of::<MenuVert>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut menu_index_data = Vec::new();
        for q in 0..32u32 {
            let r = q * 4;
            menu_index_data.extend([r, r + 1, r + 2, r + 1, r + 3, r + 2]);
        }
        let menu_indices = gpu.buffer(
            bytemuck::cast_slice(&menu_index_data),
            wgpu::BufferUsages::INDEX,
        );
        Ok(Self {
            timbury,
            bugs,
            bug_anim,
            set,
            sky,
            menu_def,
            menu_views,
            menu_pipeline,
            menu_layout,
            menu_verts,
            menu_indices,
            uniforms,
            bones,
            constants,
            format,
            post: Post {
                blur: post("vs_full", "fs_blur"),
                composite: post("vs_full", "fs_composite"),
                group_layout: post_layout,
                sampler,
                uniforms: post_uniforms,
            },
            targets: None,
        })
    }

    pub fn show(&self) -> Show {
        let world = self.timbury.model.pose(None, &[]);
        Show::new(self.root(&world), self.menu_def.clone())
    }

    /// The joint that the free camera turns around.
    fn root(&self, world: &[Mat4]) -> Vec3 {
        let at = self.timbury.model.find("BL_Head_J").unwrap_or(0);
        world
            .get(at)
            .map_or(Vec3::ZERO, |m| m.transform_point3(Vec3::ZERO))
    }

    /// The movie cameras' placements and lenses. A camera that loses the head
    /// gets turned back toward it; the keys alone stare at the sky mid-dance,
    /// so the original must aim them at run time [I].
    fn movies(&self, world: &[Mat4], head: Vec3) -> Vec<(Mat4, f32)> {
        let place = |name: &str| {
            self.timbury
                .model
                .camera(name)
                .map(|c| {
                    let m = self
                        .timbury
                        .model
                        .find(&c.transform)
                        .map(|i| world[i])
                        .unwrap_or(Mat4::IDENTITY);
                    (Self::aim(m, head), c.fov)
                })
                .unwrap_or((Mat4::IDENTITY, 0.7))
        };
        // Camera1 and Camera2 from the menu [I].
        vec![place("Camera__1_Shape1"), place("Camera__1_CUShape")]
    }

    /// Turns a camera toward the head when its keys look away, keeping the
    /// keyed position, scale and roll blend [I].
    fn aim(place: Mat4, head: Vec3) -> Mat4 {
        let eye = place.w_axis.truncate();
        let to_head = head - eye;
        if to_head.length_squared() < 1.0 {
            return place;
        }
        let fwd = (place * Vec4::new(0.0, 0.0, -1.0, 0.0)).truncate();
        if fwd.length_squared() < 1e-6 {
            return place;
        }
        let cos = fwd.normalize_or_zero().dot(to_head.normalize_or_zero());
        // Fully keyed while the head is well in frame.
        let w = ((0.75 - cos) / 0.3).clamp(0.0, 1.0);
        if w <= 0.0 {
            return place;
        }
        let (scale, turn, _) = place.to_scale_rotation_translation();
        // A camera looks down -Z; build the aim rotation from axes.
        let z = -to_head.normalize_or_zero();
        let x = Vec3::Y.cross(z).normalize_or_zero();
        if x.length_squared() < 1e-6 {
            return place;
        }
        let aim = Quat::from_mat3(&Mat3::from_cols(x, z.cross(x), z));
        let turn = turn.normalize().slerp(aim, w);
        Mat4::from_scale_rotation_translation(scale, turn, eye)
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
            let format = self.format;
            let msaa = |format: wgpu::TextureFormat, samples| {
                let t = gpu.target(size.0, size.1, format, samples);
                t.create_view(&Default::default())
            };
            let plain = |w: u32, h: u32| {
                let t = gpu.target(w, h, format, 1);
                t.create_view(&Default::default())
            };
            self.targets = Some(Targets {
                size,
                color: msaa(format, SAMPLES),
                depth: msaa(DEPTH, SAMPLES),
                resolved: plain(size.0, size.1),
                blur_a: plain(size.0.max(4) / 4, size.1.max(4) / 4),
                blur_b: plain(size.0.max(4) / 4, size.1.max(4) / 4),
            });
        }
        let (width, height) = (size.0 as f32, size.1 as f32);
        // Timbury dances a 59-second loop; the bugs flap theirs every second.
        let dance = &self.timbury.model.animations[0];
        let seconds = frame.time % 59.0;
        let mut values = HashMap::new();
        for n in &dance.numbers {
            let v = n.curve.at(seconds);
            values.extend(n.nodes.iter().map(|name| (name.clone(), v)));
        }
        let world = self.timbury.model.pose(Some((dance, seconds)), &[]);
        let root = self.root(&world);
        let movies = self.movies(&world, root);
        let shot = frame.camera.shot(&movies, root);
        let eye = shot.place.w_axis.truncate();
        let view = shot.place.inverse();
        let projection =
            glam::camera::rh::proj::directx::perspective(shot.fov, width / height, 0.1, 30000.0);
        let uniforms = Uniforms {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            eye: eye.extend(1.0).into(),
            time: [frame.time as f32, 0.0, 0.0, 0.0],
            look: [
                frame.sliders[0],
                frame.sliders[1],
                frame.sliders[2],
                frame.sliders[3],
            ],
            size: [width, height, frame.sliders[4], 0.0],
            menu: [0.0; 4],
            fade: [frame.white; 4],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Timbury's face and body follow the morph weights.
        for (k, shape) in self.timbury.model.shapes.iter().enumerate() {
            if shape.targets() == 0 {
                continue;
            }
            let wanted = shape.animated(&values, 8, 0.01);
            let gpu_shape = &mut self.timbury.shapes[k];
            if let Buffers::Mesh { moving, loaded, .. } = &mut gpu_shape.buffers
                && let Geometry::Mesh(m) = &shape.geometry
                && *loaded != wanted
            {
                gpu.queue
                    .write_buffer(moving, 0, bytemuck::cast_slice(&m.morphed(&wanted)));
                *loaded = wanted;
            }
        }
        // Every draw: actor, shape, bone slot, and optional bug.
        let actors = [&self.sky, &self.set, &self.timbury, &self.bugs];
        let prefix = {
            let mut p = vec![0];
            for a in &actors {
                p.push(p.last().unwrap() + a.shapes.len());
            }
            p
        };
        let mut draws: Vec<Draw> = Vec::new();
        // Sky and set (grass and terrain) draw as themselves.
        for a in 0..2 {
            for k in 0..actors[a].shapes.len() {
                draws.push(Draw {
                    actor: a,
                    shape: k,
                    bones: prefix[a] + k,
                });
            }
        }
        // Timbury: the main pass group only.
        let main = self
            .timbury
            .model
            .shapes_in(Some("nvPrimaryRenderPass_group"));
        for &k in &main {
            draws.push(Draw {
                actor: 2,
                shape: k,
                bones: prefix[2] + k,
            });
        }
        // His hair rides along.
        for &k in &self.timbury.model.absorbed() {
            draws.push(Draw {
                actor: 2,
                shape: k,
                bones: prefix[2] + k,
            });
        }
        // The swarm: every bug draws the body's group.
        let bug_base = prefix[4];
        let body = self.bugs.model.shapes_in(Some("default_group"));
        for (i, _) in frame.bugs.iter().enumerate() {
            for (part, &k) in body.iter().enumerate() {
                draws.push(Draw {
                    actor: 3,
                    shape: k,
                    bones: bug_base + i * 3 + part,
                });
            }
        }
        // Bones for the four actors' own shapes.
        let mut data = Vec::new();
        for (a, actor) in actors.iter().enumerate() {
            let w = if a == 2 { Some(&world) } else { None };
            let rest = actor.model.pose(None, &[]);
            let w = w.unwrap_or(&rest);
            for (k, (shape, _)) in actor.model.shapes.iter().zip(&actor.shapes).enumerate() {
                let _ = k;
                for m in &actor.model.bones(shape, w) {
                    data.extend_from_slice(bytemuck::bytes_of(&m.to_cols_array_2d()));
                }
                data.resize(
                    data.len()
                        + (STRIDE as usize - 64 * actor.model.bones(shape, w).len().min(BONES)),
                    0,
                );
            }
        }
        // Bones for each bug: wings flap, the body rides along.
        for bug in &frame.bugs {
            let at = bug.position;
            let yaw = Vec3::new(0.0, bug.yaw, 0.0);
            let moved = [("BugPlacement", at, yaw), ("BugCombined", at, yaw)];
            let w = self
                .bugs
                .model
                .pose(Some((&self.bug_anim, bug.phase as f64)), &moved);
            for &k in &body {
                let shape = &self.bugs.model.shapes[k];
                let bones = self.bugs.model.bones(shape, &w);
                for m in &bones {
                    data.extend_from_slice(bytemuck::bytes_of(&m.to_cols_array_2d()));
                }
                data.resize(
                    data.len() + STRIDE as usize - 64 * bones.len().min(BONES),
                    0,
                );
            }
        }
        gpu.queue.write_buffer(&self.bones, 0, &data);
        // Post chain settings: texel size, softness, exposure, radius.
        // The original scales by the measured light level so the night stays
        // visible; the port uses a fixed gain times the output slider [I].
        let post: [f32; 8] = [
            4.0 / width,
            4.0 / height,
            frame.sliders[1],
            frame.sliders[0] * 8.0,
            frame.sliders[4] * 2.0 + 0.5,
            frame.sliders[2],
            frame.sliders[3],
            0.0,
        ];
        gpu.queue
            .write_buffer(&self.post.uniforms, 0, bytemuck::cast_slice(&post));
        let target = self.targets.as_ref().expect("made above");
        self.scene_pass(enc, target, &draws);
        self.blur_pass(enc, target, gpu);
        self.composite_pass(enc, target, window, gpu, frame);
    }

    fn scene_pass(&self, enc: &mut wgpu::CommandEncoder, target: &Targets, draws: &[Draw]) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color,
                depth_slice: None,
                resolve_target: Some(&target.resolved),
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
        // NOTE: blur_a doubles as the MSAA resolve target; the blur passes
        // below reuse it afterwards.
        let actors = [&self.sky, &self.set, &self.timbury, &self.bugs];
        let prefix = {
            let mut p = vec![0];
            for a in &actors {
                p.push(p.last().unwrap() + a.shapes.len());
            }
            p
        };
        for d in draws {
            let shape = &actors[d.actor].shapes[d.shape];
            pass.set_pipeline(&shape.pipeline);
            // Bones and material constants ride in the dynamic slots.
            pass.set_bind_group(
                0,
                &self.constants,
                &[
                    (d.bones as u64 * STRIDE) as u32,
                    ((prefix[d.actor] + d.shape) as u64 * PARAM_STRIDE) as u32,
                ],
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
                Buffers::Hair { segments, count } => {
                    pass.set_vertex_buffer(0, segments.slice(..));
                    pass.draw(0..6, 0..*count);
                }
                Buffers::Grass {
                    blades,
                    indices,
                    count,
                } => {
                    pass.set_vertex_buffer(0, blades.slice(..));
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
            }
        }
    }

    fn blur_pass(&self, enc: &mut wgpu::CommandEncoder, target: &Targets, gpu: &Gpu) {
        // Downsamples the resolved scene and blurs it once.
        let (src, dst) = (&target.resolved, &target.blur_a);
        {
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.post.group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.post.uniforms,
                            offset: 0,
                            size: std::num::NonZero::new(32),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.post.sampler),
                    },
                ],
            });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.post.blur);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    fn composite_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        target: &Targets,
        window: &wgpu::TextureView,
        gpu: &Gpu,
        frame: &Frame,
    ) {
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.post.group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&target.resolved),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&target.blur_a),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.post.uniforms,
                        offset: 0,
                        size: std::num::NonZero::new(32),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.post.sampler),
                },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: window,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&self.post.composite);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.menu_pass(enc, window, gpu, frame);
    }

    /// The menu overlay: logo, buttons, sliders and the about box.
    fn menu_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        window: &wgpu::TextureView,
        gpu: &Gpu,
        frame: &Frame,
    ) {
        // Quads as (texture node, x, y, w, h) in fractions, y up.
        let mut quads: Vec<(&str, [f32; 4])> = Vec::new();
        if frame.logo {
            let (image, at, size) = &self.menu_def.logo;
            quads.push((image, [at.x, at.y, size.x, size.y]));
        }
        for b in &self.menu_def.buttons {
            // The actions dropdown starts closed; the port keeps its buttons out [I].
            quads.push((
                b.image.as_str(),
                [b.position.x, b.position.y, b.size.x, b.size.y],
            ));
        }
        if frame.sliders_open {
            for s in &self.menu_def.sliders {
                if s.index >= frame.sliders.len() {
                    continue;
                }
                quads.push((
                    self.menu_def.bar.as_str(),
                    [s.position.x, s.position.y, s.size.x, s.size.y],
                ));
                let v = frame.sliders[s.index];
                let w = s.size.y * 3.0;
                quads.push((
                    self.menu_def.knob.as_str(),
                    [
                        s.position.x + v * s.size.x - w * 0.5,
                        s.position.y - s.size.y * 0.5,
                        w,
                        s.size.y * 2.0,
                    ],
                ));
            }
        }
        if frame.about {
            let (image, at, size) = &self.menu_def.about_box;
            quads.push((image, [at.x, at.y, size.x, size.y]));
        }
        if quads.is_empty() {
            return;
        }
        let mut verts = Vec::with_capacity(quads.len() * 4);
        for (_, r) in &quads {
            // The UI pictures are stored mirrored [I].
            for (x, y) in [
                (r[0], r[1]),
                (r[0] + r[2], r[1]),
                (r[0], r[1] + r[3]),
                (r[0] + r[2], r[1] + r[3]),
            ] {
                verts.push(MenuVert {
                    pos: [x, y],
                    uv: [(x - r[0]) / r[2], (y - r[1]) / r[3]],
                });
            }
        }
        gpu.queue
            .write_buffer(&self.menu_verts, 0, bytemuck::cast_slice(&verts));
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: window,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&self.menu_pipeline);
        pass.set_vertex_buffer(0, self.menu_verts.slice(..));
        pass.set_index_buffer(self.menu_indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut groups: HashMap<&str, wgpu::BindGroup> = HashMap::new();
        let mut quad = 0;
        // Draw runs of one texture at a time.
        let mut i = 0;
        while i < quads.len() {
            let name = quads[i].0;
            let mut n = 0;
            while i + n < quads.len() && quads[i + n].0 == name {
                n += 1;
            }
            let group = groups.entry(name).or_insert_with(|| {
                let view = self.menu_views.get(name).expect("menu picture");
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.menu_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.post.sampler),
                        },
                    ],
                })
            });
            pass.set_bind_group(0, &*group, &[]);
            pass.draw_indexed((quad * 6) as u32..((quad + n) * 6) as u32, 0, 0..1);
            quad += n;
            i += n;
        }
    }
}
