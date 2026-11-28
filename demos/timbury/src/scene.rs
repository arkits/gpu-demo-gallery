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
/// Fullscreen pass settings: one SLOT each.
const POST_SLOTS: u64 = 16;
/// The tree-shadow blurs' settings follow the primary chain's.
const TREE_SLOT: u64 = 8;
/// `glassLenses_f_sceneTex`, `bumpedEnv2Tex_f_shadowtex` and the tree-shadow
/// targets, at their filed `ProcTexture` sizes.
const LENS: (u32, u32) = (1024, 512);
const SHADOW_MAP: u32 = 2048;
const TREE_MAP: u32 = 256;
/// `PrimaryBlurX/Y`, `LightLevelTarget1` and `LightLevelTarget2a/b` (blur.nma).
const BLUR: (u32, u32) = (256, 128);
const LEVEL: u32 = 8;
const EXPOSURE: u32 = 2;
/// The show restarts after 63 seconds (0x41c840), fading in over the first
/// two and out from 57.3 to 60.9.
pub const SHOW_LENGTH: f64 = 63.0;

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Uniforms {
    view: [[f32; 4]; 4],
    projection: [[f32; 4]; 4],
    eye: [f32; 4],
    time: [f32; 4],
    look: [f32; 4],
    size: [f32; 4],
    light: [f32; 4],
    fade: [f32; 4],
    /// viewProjWin_Target0: the shadow camera's view and projection.
    shadow: [[f32; 4]; 4],
    tim_look: [f32; 4],
    tim_side: [f32; 4],
    tim_up: [f32; 4],
    /// g_bitten, bkg g_brightnessScale, g_grassBrightScale.
    runtime: [f32; 4],
}

/// A shape's filed constants with the Cg defaults; see `Params` in the shader.
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Params {
    diffuse: [f32; 4],
    ambient: [f32; 4],
    light: [f32; 4],
    spec: [f32; 4],
    extra: [f32; 4],
    more: [f32; 4],
}

impl Params {
    fn of(shape: &Shape) -> Self {
        let c =
            |name: &str, default: [f32; 4]| shape.constants.get(name).copied().unwrap_or(default);
        let one = |name: &str, default: f32| c(name, [default; 4])[0];
        let rgb = |v: [f32; 4], w: f32| [v[0], v[1], v[2], w];
        // _blur2D: one step along the blur axis, in texture units.
        let inv = c("invTexSize", [0.0; 4]);
        let axis = c("blurAxis", [0.0; 4]);
        Self {
            diffuse: rgb(c("diffuseColor", [1.0; 4]), one("specularScale", 0.1)),
            ambient: rgb(c("ambientColor", [0.1; 4]), one("bump_scale", 1.0)),
            light: rgb(
                c("worldShadowCameraVec", [0.0, 1.0, 0.0, 0.0]),
                one("shadow_str", 0.5),
            ),
            spec: rgb(c("specularColor", [1.0; 4]), one("transparency", 0.5)),
            extra: [
                one("rim_ndotv_exp_mult", 1.0),
                one("rim_color_scale", 1.0),
                one("hard_shadow_inv_texsize", 1.0 / 2048.0),
                one("modulate_by", 0.5),
            ],
            more: [
                one("flipUCoord", 0.0),
                inv[0] * axis[0],
                inv[1] * axis[1],
                0.0,
            ],
        }
    }
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

/// Window-sized targets: the scene (multisampled, then resolved).
struct Targets {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved: wgpu::TextureView,
}

/// Fixed-size targets: lens, shadows and the post chain.
struct Offscreen {
    lens_color: wgpu::TextureView,
    lens_depth: wgpu::TextureView,
    lens: wgpu::TextureView,
    shadow: wgpu::TextureView,
    tree: wgpu::TextureView,
    tree_a: wgpu::TextureView,
    soft: wgpu::TextureView,
    blur_x: wgpu::TextureView,
    blur_y: wgpu::TextureView,
    level: wgpu::TextureView,
    exposure: [wgpu::TextureView; 2],
}

/// One camera's uniforms and the group-0 bindings that carry them.
struct View {
    uniforms: wgpu::Buffer,
    constants: wgpu::BindGroup,
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
    bones: wgpu::Buffer,
    /// The watched camera, LensCameraShape and ShadowCameraShape.
    main: View,
    lens: View,
    shadow: View,
    /// Group 2 for the main pass, and for the lens pass (which cannot read
    /// the lens target it draws).
    global_main: wgpu::BindGroup,
    global_lens: wgpu::BindGroup,
    cast_depth: [wgpu::RenderPipeline; 2],
    cast_tree: wgpu::RenderPipeline,
    post: Post,
    hdr: wgpu::TextureFormat,
    targets: Option<Targets>,
    fixed: Offscreen,
    /// Frames drawn, for the exposure ping-pong and its first-frame mix.
    frames: u64,
}

struct Post {
    blur_xy: wgpu::RenderPipeline,
    blur_05: wgpu::RenderPipeline,
    blur_07: wgpu::RenderPipeline,
    blur_2d: wgpu::RenderPipeline,
    lightlev: wgpu::RenderPipeline,
    lightlev2: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// One 16-byte setting per fullscreen pass, SLOT apart.
    uniforms: wgpu::Buffer,
    /// Stands in for an unused texture binding.
    blank: wgpu::TextureView,
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
            if signed {
                // Signed maps go to the GPU in file byte order (x, y, z, ramp).
                let (w, h, pixels) = tga::read_raw(data)?;
                self.gpu.upload_snorm((w as u32, h as u32), &[pixels])
            } else {
                let (w, h, pixels) = tga::read(data)?;
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
        planes: [wgpu::TextureView; 5],
        cubes: [wgpu::TextureView; 2],
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
            binding: 7,
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

    /// Which pictures a shape's fragment program reads, by its parameter
    /// names: (color, bump, specular, special, ramp) planes and (diffuse,
    /// specular) cubes.
    fn material(&mut self, model: &Model, shape: &Shape) -> Result<wgpu::BindGroup, Error> {
        let names: [&str; 7] = match shape.program.as_str() {
            p if p.starts_with("hair") => [
                "colorTex",
                "anisoTex",
                "",
                "",
                "",
                "crossEnvDiffuseCubemap",
                "",
            ],
            p if p.starts_with("grass") => ["grassTex", "", "", "", "", "", ""],
            p if p.starts_with("lambert") => ["c_diffCol", "", "", "", "", "", ""],
            p if p.starts_with("_texture") || p.starts_with("_bkg") => {
                ["texture", "", "", "", "", "", ""]
            }
            p if p.starts_with("eyeballGloss") => [
                "",
                "bumpTex",
                "",
                "",
                "",
                "crossEnvDiffuseCubemap",
                "crossEnvSpecularCubemap",
            ],
            _ => [
                "colorTex",
                "bump",
                "specularTex",
                "specialTex",
                "rampTex",
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
            // A missing bump map reads flat (signed zero); other missing
            // pictures read white.
            views.push(match (texture, i) {
                (None, 1) => self.load("", false, true)?,
                _ => self.view(&models, texture, i >= 5)?,
            });
        }
        let [a, b, c, d, e, f, g] = views.try_into().map_err(|_| "seven textures")?;
        Ok(self.group([a, b, c, d, e], [f, g], [false; 2]))
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
        let depth_key = match s.depth {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        let key = format!(
            "{} {} {} {} {}",
            s.vertex, s.fragment, s.blend as u8, s.cull, depth_key
        );
        if let Some((_, p)) = self.made.iter().find(|m| m.0 == key) {
            return p.clone();
        }
        let component = |src, dst| wgpu::BlendComponent {
            src_factor: src,
            dst_factor: dst,
            operation: wgpu::BlendOperation::Add,
        };
        use wgpu::BlendFactor::{One, OneMinusSrcAlpha, Src, SrcAlpha, Zero};
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
                            Blend::Multiply => Some(both(component(Zero, Src))),
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
                    depth_write_enabled: Some(s.depth == Some(true)),
                    depth_compare: Some(match s.depth {
                        None => wgpu::CompareFunction::Always,
                        Some(_) => wgpu::CompareFunction::LessEqual,
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

    fn shape(&mut self, shape: &Shape) -> (&'static str, Blend, bool, Option<bool>) {
        (
            fragment(&shape.program),
            shape_blend(shape),
            shape.cull,
            shape.depth,
        )
    }
}

/// The fragment entry point that ports a shape's fragment program. Longer
/// names go first: `bumpedEnv2Tex_CLOTHES_f` before `bumpedEnv2Tex_f`.
fn fragment(program: &str) -> &'static str {
    const PORTS: &[(&str, &str)] = &[
        ("hair", "fs_hair"),
        ("grass", "fs_grass"),
        ("lambert", "fs_lambert"),
        ("glassLenses", "fs_glass"),
        ("eyeballGloss", "fs_gloss"),
        ("eyeballShadow", "fs_shadow"),
        ("_texture", "fs_flat"),
        ("_bkg", "fs_flat"),
        ("bumpedEnv2Tex_CLOTHES", "fs_env2_clothes"),
        ("bumpedEnv2Tex_TREE", "fs_env2_tree"),
        ("bumpedEnv2Tex_BITE", "fs_env2_bite"),
        ("bumpedEnv2Tex_NO_SHADOW", "fs_env2_noshadow"),
        ("bumpedEnv2Tex", "fs_env2"),
        ("bumpedEnv3Tex_SKIN", "fs_env3_skin"),
        ("bumpedEnv3Tex_BUG", "fs_env3_bug"),
        ("bumpedEnv3Tex_transp", "fs_env3_transp"),
        ("bumpedEnv3Tex", "fs_env3"),
        ("env0Tex", "fs_env0"),
        ("env1Tex_transp", "fs_env1_transp"),
        ("env2Tex", "fs_env2tex"),
    ];
    PORTS
        .iter()
        .find(|(name, _)| program.starts_with(name))
        .map_or("fs_env0", |(_, entry)| entry)
}

/// The filed blending; hair has no blend proxy and is always see-through.
fn shape_blend(shape: &Shape) -> Blend {
    if shape.program.starts_with("hair") {
        Blend::Alpha
    } else {
        shape.blend
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
            let (fragment, blend, cull, depth) = pipelines.shape(shape);
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
                depth,
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
        let plane = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: D2,
            multisampled: false,
        };
        let cube = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: Cube,
            multisampled: false,
        };
        let filtering = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let frame_size = size_of::<Uniforms>() as u64;
        let first = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(0, buffer(false, frame_size)),
                entry(1, buffer(true, BONES as u64 * 64)),
                entry(2, buffer(true, size_of::<Params>() as u64)),
            ],
        });
        let mut textures: Vec<_> = (0..7)
            .map(|i| entry(i, if i < 5 { plane } else { cube }))
            .collect();
        textures.push(entry(7, filtering));
        for t in &mut textures {
            t.visibility = wgpu::ShaderStages::FRAGMENT;
        }
        let second = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &textures,
        });
        let fragment_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let third = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                fragment_entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: D2,
                        multisampled: false,
                    },
                ),
                fragment_entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                ),
                fragment_entry(2, plane),
                fragment_entry(3, plane),
                fragment_entry(4, cube),
                fragment_entry(5, filtering),
            ],
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                fragment_entry(0, plane),
                fragment_entry(1, plane),
                fragment_entry(2, buffer(false, 16)),
                fragment_entry(3, filtering),
                fragment_entry(4, plane),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&first), Some(&second), Some(&third)],
            ..Default::default()
        });
        let cast_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&first)],
            ..Default::default()
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&post_layout)],
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/timbury.wgsl").into()),
        });
        let hdr = gpu.hdr;
        let mut pipelines = Pipelines {
            gpu,
            layout,
            shader: shader.clone(),
            format: hdr,
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
        // Per-shape material constants, written once, in the draw order
        // (sky, set, Timbury, bugs) that `draw` indexes them by.
        let param_data: Vec<Params> = [&sky, &set, &timbury, &bugs]
            .iter()
            .flat_map(|a| a.model.shapes.iter().map(Params::of))
            .collect();
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
        let bones = make(STRIDE * (slots + bug_slots) as u64);
        let params = make(PARAM_STRIDE * slots.max(1) as u64);
        for (i, p) in param_data.iter().enumerate() {
            gpu.queue
                .write_buffer(&params, i as u64 * PARAM_STRIDE, bytemuck::bytes_of(p));
        }
        fn part(buffer: &wgpu::Buffer, size: u64) -> wgpu::BindingResource<'_> {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: std::num::NonZero::new(size),
            })
        }
        let view = || {
            let uniforms = make(frame_size.next_multiple_of(SLOT));
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
                        resource: part(&params, size_of::<Params>() as u64),
                    },
                ],
            });
            View {
                uniforms,
                constants,
            }
        };
        let (main, lens, shadow) = (view(), view(), view());

        // Offscreen targets at their filed sizes.
        let plain = |(w, h): (u32, u32), format| {
            gpu.target(w, h, format, 1).create_view(&Default::default())
        };
        let lens_color = gpu
            .target(LENS.0, LENS.1, hdr, SAMPLES)
            .create_view(&Default::default());
        let lens_depth = gpu
            .target(LENS.0, LENS.1, DEPTH, SAMPLES)
            .create_view(&Default::default());
        let shadow_depth = gpu.target(SHADOW_MAP, SHADOW_MAP, wgpu::TextureFormat::Depth32Float, 1);
        let tree_format = crate::gpu::COLOR;
        let fixed = Offscreen {
            lens_color,
            lens_depth,
            lens: plain(LENS, hdr),
            shadow: shadow_depth.create_view(&Default::default()),
            tree: plain((TREE_MAP, TREE_MAP), tree_format),
            tree_a: plain((TREE_MAP, TREE_MAP), tree_format),
            soft: plain((TREE_MAP, TREE_MAP), tree_format),
            blur_x: plain(BLUR, hdr),
            blur_y: plain(BLUR, hdr),
            level: plain((LEVEL, LEVEL), hdr),
            exposure: [
                plain((EXPOSURE, EXPOSURE), hdr),
                plain((EXPOSURE, EXPOSURE), hdr),
            ],
        };
        let clamp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Hardware shadow compares filter bilinearly, as NV3x did.
        let compare_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let models = [&timbury.model];
        let reflection_name = "envReflectionCubemap".to_owned();
        let reflection = loader.view(&models, Some(&reflection_name), true)?;
        let blank = loader.load("", false, false)?;
        let global = |lens: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &third,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&fixed.shadow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&compare_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&fixed.soft),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(lens),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&reflection),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&clamp_sampler),
                    },
                ],
            })
        };
        let global_main = global(&fixed.lens);
        let global_lens = global(&blank);

        // Shadow casters: depth only for ShadowRenderTargetProxy, and
        // (0, 0, depth) colors for TreeShadowRenderTargetProxy.
        let fixed_attrs = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x2, 4 => Float32x4, 5 => Float32x4];
        let moving_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let mesh_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Moving>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &moving_attrs,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Fixed>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &fixed_attrs,
            }),
        ];
        let tree_targets = [Some(wgpu::ColorTargetState {
            format: tree_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let cast = |cull: bool, color: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if color { "cast tree" } else { "cast depth" }),
                layout: Some(&cast_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_cast"),
                    compilation_options: Default::default(),
                    buffers: &mesh_buffers,
                },
                fragment: color.then(|| wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_cast"),
                    compilation_options: Default::default(),
                    targets: &tree_targets,
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull.then_some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: (!color).then(|| wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    // A little slope bias keeps lit faces off their own map [I].
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let cast_depth = [cast(false, false), cast(true, false)];
        let cast_tree = cast(false, true);

        // The fullscreen passes.
        let post = |fragment: &str, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
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
            size: SLOT * POST_SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let post = Post {
            blur_xy: post("fs_blur_xy", hdr),
            blur_05: post("fs_blur_05", hdr),
            blur_07: post("fs_blur_07", hdr),
            blur_2d: post("fs_blur2d", tree_format),
            lightlev: post("fs_lightlev", hdr),
            lightlev2: post("fs_lightlev2", hdr),
            composite: post("fs_composite", format),
            group_layout: post_layout,
            sampler,
            uniforms: post_uniforms,
            blank: blank.clone(),
        };
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
            bones,
            main,
            lens,
            shadow,
            global_main,
            global_lens,
            cast_depth,
            cast_tree,
            hdr,
            post,
            targets: None,
            fixed,
            frames: 0,
        })
    }

    /// Forgets the measured exposure, so the next frame sets it outright as
    /// the original's first frame does (g_mixAmount 1). For unseen warm-up
    /// draws, which must not tint the first frames shown.
    pub fn reset_exposure(&mut self) {
        self.frames = 0;
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
            let msaa = |format: wgpu::TextureFormat| {
                gpu.target(size.0, size.1, format, SAMPLES)
                    .create_view(&Default::default())
            };
            self.targets = Some(Targets {
                size,
                color: msaa(self.hdr),
                depth: msaa(DEPTH),
                resolved: gpu
                    .target(size.0, size.1, self.hdr, 1)
                    .create_view(&Default::default()),
            });
        }
        let (width, height) = (size.0 as f32, size.1 as f32);
        // The show runs 63 seconds; the dance holds its last pose at the end.
        let seconds = frame.time % SHOW_LENGTH;
        let dance = &self.timbury.model.animations[0];
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

        // ShadowCameraShape: the filed placement, lens and clip planes.
        let shadow_cam = self.timbury.model.camera("ShadowCameraShape");
        let (shadow_place, shadow_proj) = match shadow_cam {
            Some(c) => {
                let place = self
                    .timbury
                    .model
                    .find(&c.transform)
                    .map_or(Mat4::IDENTITY, |i| world[i]);
                (
                    place,
                    glam::camera::rh::proj::directx::perspective(c.fov, 1.0, 1400.0, 2300.0),
                )
            }
            None => (Mat4::IDENTITY, Mat4::IDENTITY),
        };
        let shadow_view = shadow_place.inverse();
        let shadow_vp = shadow_proj * shadow_view;
        // LensCameraShape rides on the head: its axes give g_timLookDir,
        // g_timSideDir and g_timUpDir (negated Z, X and Y, as 0x41a870 does).
        let lens_place = self
            .timbury
            .model
            .camera("LensCameraShape")
            .and_then(|c| self.timbury.model.find(&c.transform))
            .map_or(Mat4::IDENTITY, |i| world[i]);
        let axis = |v: Vec4| (-v.truncate().normalize_or_zero()).extend(0.0).to_array();
        let (tim_look, tim_side, tim_up) = (
            axis(lens_place.z_axis),
            axis(lens_place.x_axis),
            axis(lens_place.y_axis),
        );
        let lens_fov = self
            .timbury
            .model
            .camera("LensCameraShape")
            .map_or(0.337, |c| c.fov);
        let lens_proj = glam::camera::rh::proj::directx::perspective(
            lens_fov,
            LENS.0 as f32 / LENS.1 as f32,
            0.1,
            100.0,
        );

        // Per-frame settings that 0x41a870 hands the programs.
        let runtime = Runtime::new(frame, eye, shadow_place.w_axis.truncate(), self.frames);
        let light = self
            .timbury
            .model
            .lights
            .first()
            .map_or(Vec3::NEG_Y, |l| l.direction);
        let uniforms = |view: Mat4, projection: Mat4, eye: Vec3| Uniforms {
            view: view.to_cols_array_2d(),
            projection: projection.to_cols_array_2d(),
            eye: eye.extend(1.0).into(),
            time: [frame.time as f32, 0.0, 0.0, 0.0],
            look: [0.0; 4],
            size: [width, height, 0.0, 0.0],
            light: light.extend(0.0).into(),
            fade: [runtime.fade; 4],
            shadow: shadow_vp.to_cols_array_2d(),
            tim_look,
            tim_side,
            tim_up,
            runtime: [runtime.bitten, runtime.sky, runtime.grass, 0.0],
        };
        let write = |v: &View, u: Uniforms| {
            gpu.queue
                .write_buffer(&v.uniforms, 0, bytemuck::bytes_of(&u));
        };
        write(&self.main, uniforms(view, projection, eye));
        write(
            &self.lens,
            uniforms(
                lens_place.inverse(),
                lens_proj,
                lens_place.w_axis.truncate(),
            ),
        );
        write(
            &self.shadow,
            uniforms(shadow_view, shadow_proj, shadow_place.w_axis.truncate()),
        );

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
        let of_group = |name: &str| -> Vec<Draw> {
            self.timbury
                .model
                .shapes_in(Some(name))
                .into_iter()
                .map(|k| Draw {
                    actor: 2,
                    shape: k,
                    bones: prefix[2] + k,
                })
                .collect()
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
        draws.extend(of_group("nvPrimaryRenderPass_group"));
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
        // Bones for the four actors' own shapes. The skybox follows the
        // camera: the original adds the camera position to its vertices
        // (_bkg_split.vp20), so the ±1000 box stays around the eye even on
        // the finale's pull-back; drawn first with no depth write, the rest
        // overdraws it (_DepthNoWrite, sortSet -999).
        let mut data = Vec::new();
        let shift = Mat4::from_translation(eye);
        for (a, actor) in actors.iter().enumerate() {
            let rest = actor.model.pose(None, &[]);
            let shifted;
            let w = match a {
                2 => &world,
                0 => {
                    shifted = rest.iter().map(|m| shift * *m).collect::<Vec<_>>();
                    &shifted
                }
                _ => &rest,
            };
            for shape in &actor.model.shapes {
                let bones = actor.model.bones(shape, w);
                for m in &bones {
                    data.extend_from_slice(bytemuck::bytes_of(&m.to_cols_array_2d()));
                }
                data.resize(
                    data.len() + (STRIDE as usize - 64 * bones.len().min(BONES)),
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

        // The passes in the file's order: hard shadows, tree shadow and its
        // two blurs, the lens picture, then the primary pass and blur.nma.
        let shadow_draws = of_group("ShadowRenderTargetProxy_group");
        let tree_draws = of_group("TreeShadowRenderTargetProxy_group");
        let lens_draws = of_group("LensRenderTargetProxy_group");
        self.cast_pass(enc, &shadow_draws, false);
        self.cast_pass(enc, &tree_draws, true);
        self.tree_blur(enc, gpu);
        let target = self.targets.as_ref().expect("made above");
        self.scene_pass(
            enc,
            (
                &self.fixed.lens_color,
                &self.fixed.lens_depth,
                &self.fixed.lens,
            ),
            &self.lens,
            &self.global_lens,
            &lens_draws,
        );
        self.scene_pass(
            enc,
            (&target.color, &target.depth, &target.resolved),
            &self.main,
            &self.global_main,
            &draws,
        );
        self.post_chain(enc, gpu, window, &runtime, frame);
        self.frames += 1;
    }

    /// The bone and parameter offsets of a draw.
    fn offsets(&self, d: &Draw) -> [u32; 2] {
        let actors = [&self.sky, &self.set, &self.timbury, &self.bugs];
        let before: usize = actors[..d.actor].iter().map(|a| a.shapes.len()).sum();
        [
            (d.bones as u64 * STRIDE) as u32,
            ((before + d.shape) as u64 * PARAM_STRIDE) as u32,
        ]
    }

    /// ShadowRenderTargetProxy (depth only) or TreeShadowRenderTargetProxy
    /// (coverage and depth over white), from ShadowCameraShape.
    fn cast_pass(&self, enc: &mut wgpu::CommandEncoder, draws: &[Draw], tree: bool) {
        let color = [tree.then_some(wgpu::RenderPassColorAttachment {
            view: &self.fixed.tree,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(if tree { "tree shadow" } else { "shadow" }),
            color_attachments: if tree { &color } else { &[] },
            depth_stencil_attachment: (!tree).then_some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.fixed.shadow,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        for d in draws {
            let shape = &self.timbury.shapes[d.shape];
            let Buffers::Mesh {
                moving,
                fixed,
                indices,
                count,
                ..
            } = &shape.buffers
            else {
                continue;
            };
            let pipeline = if tree {
                &self.cast_tree
            } else {
                &self.cast_depth[usize::from(self.timbury.model.shapes[d.shape].cull)]
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.shadow.constants, &self.offsets(d));
            pass.set_vertex_buffer(0, moving.slice(..));
            pass.set_vertex_buffer(1, fixed.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, 0..1);
        }
    }

    /// BlurRenderTargetProxy_A and _B: `_blur2D` across, then down, into
    /// `bumpedEnv2Tex_f_softTreeShadowTex`.
    fn tree_blur(&self, enc: &mut wgpu::CommandEncoder, gpu: &Gpu) {
        let steps = [
            "BlurRenderTargetProxy_A_group",
            "BlurRenderTargetProxy_B_group",
        ]
        .map(|g| {
            self.timbury
                .model
                .shapes_in(Some(g))
                .first()
                .map(|&k| {
                    let p = Params::of(&self.timbury.model.shapes[k]);
                    [p.more[1], p.more[2]]
                })
                .unwrap_or_default()
        });
        let passes = [
            (&self.fixed.tree, &self.fixed.tree_a, steps[0]),
            (&self.fixed.tree_a, &self.fixed.soft, steps[1]),
        ];
        for (i, (src, dst, step)) in passes.into_iter().enumerate() {
            // The filed steps are in the original's (v up) texture space.
            self.fullscreen(
                enc,
                gpu,
                &self.post.blur_2d,
                [src, src, &self.post.blank],
                dst,
                TREE_SLOT + i as u64,
                [step[0], step[1], 0.0, 0.0],
            );
        }
    }

    fn scene_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        (color, depth, resolved): (&wgpu::TextureView, &wgpu::TextureView, &wgpu::TextureView),
        view: &View,
        global: &wgpu::BindGroup,
        draws: &[Draw],
    ) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: Some(resolved),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Discard,
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
            ..Default::default()
        });
        let actors = [&self.sky, &self.set, &self.timbury, &self.bugs];
        pass.set_bind_group(2, global, &[]);
        for d in draws {
            let shape = &actors[d.actor].shapes[d.shape];
            pass.set_pipeline(&shape.pipeline);
            // Bones and material constants ride in the dynamic slots.
            pass.set_bind_group(0, &view.constants, &self.offsets(d));
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

    /// One fullscreen pass of the post chain: three textures, a 16-byte
    /// setting in `slot`, drawn into `dst`.
    #[allow(clippy::too_many_arguments)]
    fn fullscreen(
        &self,
        enc: &mut wgpu::CommandEncoder,
        gpu: &Gpu,
        pipeline: &wgpu::RenderPipeline,
        [t0, t1, t2]: [&wgpu::TextureView; 3],
        dst: &wgpu::TextureView,
        slot: u64,
        setting: [f32; 4],
    ) {
        gpu.queue.write_buffer(
            &self.post.uniforms,
            slot * SLOT,
            bytemuck::cast_slice(&setting),
        );
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.post.group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(t0),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(t1),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.post.uniforms,
                        offset: slot * SLOT,
                        size: std::num::NonZero::new(16),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.post.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(t2),
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// blur.nma: Initial2DBlurPass, PrimaryBlurPassX/Y twice, the two light
    /// level passes, then CompositePass into the window and the menu.
    fn post_chain(
        &self,
        enc: &mut wgpu::CommandEncoder,
        gpu: &Gpu,
        window: &wgpu::TextureView,
        runtime: &Runtime,
        frame: &Frame,
    ) {
        let target = self.targets.as_ref().expect("made in draw");
        let f = &self.fixed;
        let p = &self.post;
        let blank = &p.blank;
        let (s256, s128) = (1.0 / 256.0, 1.0 / 128.0);
        self.fullscreen(
            enc,
            gpu,
            &p.blur_xy,
            [&target.resolved, blank, blank],
            &f.blur_y,
            0,
            [s256, s128, 0.0, 0.0],
        );
        let w05 = [0.2, 0.2];
        let w07 = [0.142857, 0.142857];
        self.fullscreen(
            enc,
            gpu,
            &p.blur_05,
            [&f.blur_y, blank, blank],
            &f.blur_x,
            1,
            [s256, 0.0, w05[0], w05[1]],
        );
        self.fullscreen(
            enc,
            gpu,
            &p.blur_07,
            [&f.blur_x, blank, blank],
            &f.blur_y,
            2,
            [0.0, s256, w07[0], w07[1]],
        );
        self.fullscreen(
            enc,
            gpu,
            &p.blur_05,
            [&f.blur_y, blank, blank],
            &f.blur_x,
            3,
            [s256, 0.0, w05[0], w05[1]],
        );
        self.fullscreen(
            enc,
            gpu,
            &p.blur_07,
            [&f.blur_x, blank, blank],
            &f.blur_y,
            4,
            [0.0, s256, w07[0], w07[1]],
        );
        self.fullscreen(
            enc,
            gpu,
            &p.lightlev,
            [&f.blur_y, blank, blank],
            &f.level,
            5,
            [0.0; 4],
        );
        // LightLevelTarget2a on odd frames, 2b on even ones; each reads the other.
        let now = (self.frames & 1) as usize;
        let (write, read) = (&f.exposure[now], &f.exposure[1 - now]);
        self.fullscreen(
            enc,
            gpu,
            &p.lightlev2,
            [&f.level, read, blank],
            write,
            6,
            [0.0, 0.0, runtime.mix, 0.0],
        );
        self.fullscreen(
            enc,
            gpu,
            &p.composite,
            [&target.resolved, &f.blur_y, write],
            window,
            7,
            [0.0, 0.0, runtime.softness, runtime.output],
        );
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

/// What 0x41a870 and 0x41c840 hand the programs each frame, from the show
/// time, the sliders and the cameras.
struct Runtime {
    /// The fade: up over the first two seconds, down from 57.3 to 60.9.
    fade: f32,
    /// composite's g_userOutputScale.
    output: f32,
    /// composite's g_softness.
    softness: f32,
    /// lightlev2's g_mixAmount: all new on the first frame, then damped.
    mix: f32,
    /// _bkg_split's g_brightnessScale.
    sky: f32,
    /// grass's g_grassBrightScale.
    grass: f32,
    /// bumpedEnv2Tex_BITE's g_bitten, on after 4.77 seconds.
    bitten: f32,
}

impl Runtime {
    fn new(frame: &Frame, eye: Vec3, shadow_eye: Vec3, frames: u64) -> Self {
        let t = frame.time % SHOW_LENGTH;
        let fade = if t < 2.0 {
            t / 2.0
        } else if t <= 57.3 {
            1.0
        } else {
            ((60.9 - t) / 3.6).clamp(0.0, 1.0)
        } as f32;
        // Slider order is the menu file's: output scale, AGC damping, light
        // angle, softening amount, softening radius.
        let [out, agc, _angle, soften, _radius] = frame.sliders;
        // Below the island (y < -100) the picture brightens and the sky dims.
        let low = ((-100.0 - eye.y) / 492.0).clamp(0.0, 1.0);
        let lift = 1.0 + 0.32 * low;
        let smooth = fade * fade * 3.0 - 2.0 * fade * fade * fade;
        let faded = fade.powf(2.7);
        Self {
            fade,
            output: smooth * lift * (out.powf(2.322) * 0.83 + 0.05) * 4.629_629_6,
            softness: soften.powf(1.12) * faded + (1.0 - faded),
            mix: if frames == 0 {
                1.0
            } else {
                (1.0 - agc) * (1.0 - agc) * 0.5 + 0.12
            },
            sky: 1.0 / lift,
            grass: eye.normalize_or_zero().dot(shadow_eye.normalize_or_zero()) * 0.12 + 0.21,
            bitten: if t > 4.77 { 1.0 } else { 0.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    /// Every fragment program the scenes name has its own port; none falls
    /// back. Longer names win over their prefixes.
    #[test]
    fn programs_have_ports() {
        for (program, entry) in [
            ("bumpedEnv2Tex_f", "fs_env2"),
            ("bumpedEnv2Tex_CLOTHES_f", "fs_env2_clothes"),
            ("bumpedEnv2Tex_TREE_f", "fs_env2_tree"),
            ("bumpedEnv2Tex_BITE_f", "fs_env2_bite"),
            ("bumpedEnv2Tex_NO_SHADOW_f", "fs_env2_noshadow"),
            ("bumpedEnv3Tex_f", "fs_env3"),
            ("bumpedEnv3Tex_SKIN_f", "fs_env3_skin"),
            ("bumpedEnv3Tex_BUG_f", "fs_env3_bug"),
            ("bumpedEnv3Tex_transp_f", "fs_env3_transp"),
            ("env0Tex_f", "fs_env0"),
            ("env1Tex_transp_f", "fs_env1_transp"),
            ("env2Tex_f", "fs_env2tex"),
            ("eyeballGloss_f", "fs_gloss"),
            ("eyeballShadow_f", "fs_shadow"),
            ("glassLenses_f", "fs_glass"),
            ("hair_f", "fs_hair"),
            ("grass_f", "fs_grass"),
            ("_texture_f", "fs_flat"),
        ] {
            assert_eq!(super::fragment(program), entry, "{program}");
        }
    }
}
