//! An OpenGL-like layer over wgpu, as much as the viewers use: NV vertex and fragment
//! programs (translated by nv-asm), the program parameter state they keep between draws,
//! texture objects with their own sampling state, render states, render targets.
//!
//! GL state the viewers rely on is kept as GL keeps it: vertex program constants
//! `c[0..255]` are global (`glProgramParameter4fvNV`) and persist from draw to draw;
//! fragment program parameters belong to their program; texture units keep their binding;
//! a texture's filter and wrap modes belong to the texture.
//!
//! Images stay the way up OpenGL has them (row 0 at the bottom): the translated vertex
//! programs turn clip y over, so WebGPU row 0 is OpenGL row 0 in every target, texture
//! coordinates and `f[WPOS]` need no change, and viewports are given in OpenGL's
//! coordinates. Only the final copy to the screen turns the picture over.

use std::collections::HashMap;

use nv_asm::parse::{Program as Listing, Stage, Target};
use nv_asm::wgsl;

use crate::world::{Alpha, Blend, Cull, Depth, Raster, Stencil, TextureState};

pub type Error = Box<dyn std::error::Error>;

pub const WINDOW_COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const WINDOW_DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
const UNIFORM_ALIGN: u64 = 256;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// DXT textures are given to the GPU compressed when it can take them.
    pub bc: bool,
}

impl Gpu {
    pub async fn from_adapter(adapter: &wgpu::Adapter) -> Result<Gpu, Error> {
        let bc = wgpu::Features::TEXTURE_COMPRESSION_BC;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nvdemo"),
                required_features: adapter.features() & bc,
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await?;
        let bc = device.features().contains(bc);
        Ok(Gpu { device, queue, bc })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexKind {
    D2,
    Cube,
    D3,
}

pub struct Tex {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub kind: TexKind,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    /// The texture object's sampling state (`glTexParameter`).
    pub state: TextureState,
    /// Render targets with mip levels regenerate them after each pass that draws them.
    pub levels: u32,
}

impl Tex {
    pub fn is_depth(&self) -> bool {
        self.format.is_depth_stencil_format()
    }
}

/// Decoded or compressed texel data, OpenGL row order (as the file has it).
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub format: wgpu::TextureFormat,
    /// Levels, largest first; for cubes, six faces of levels in +x, -x, +y, -y, +z, -z order.
    pub layers: Vec<Vec<Vec<u8>>>,
    pub kind: TexKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SamplerKey {
    min: u32,
    mag: u32,
    mip: u32,
    wrap: [u32; 3],
    anisotropy: u16,
    compare: bool,
    levels: bool,
}

/// A compiled vertex program: the listing, its module, what its parameters map to.
pub struct VertexProgram {
    pub listing: Listing,
    module: wgpu::ShaderModule,
    pub iface: wgsl::Interface,
    /// Uniform name to first constant register and count.
    pub vars: HashMap<String, (u32, u32)>,
    /// Input name (`a2v.objCoord`) to attribute.
    pub inputs: HashMap<String, u32>,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
struct FragmentVariant {
    shadow: Vec<u32>,
    border: Vec<u32>,
    expand: Vec<u32>,
    alpha: Option<(u32, u32, bool)>,
}

struct FragmentModule {
    module: wgpu::ShaderModule,
    iface: wgsl::Interface,
    layout: wgpu::BindGroupLayout,
}

pub struct FragmentProgram {
    pub listing: Listing,
    variants: HashMap<FragmentVariant, FragmentModule>,
    /// Sampler name to texture unit.
    pub units: HashMap<String, u32>,
    /// Named parameter values, kept with the program.
    pub params: Vec<[f32; 4]>,
}

impl FragmentProgram {
    pub fn set(&mut self, name: &str, v: [f32; 4]) {
        if let Some(i) = self.listing.declares.iter().position(|d| d.0 == name) {
            self.params[i] = v;
        }
    }
}

/// Every render state of a draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct States {
    pub cull: Cull,
    pub blend: Blend,
    pub depth: Depth,
    pub alpha: Alpha,
    pub stencil: Stencil,
    pub raster: Raster,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    vp: usize,
    fp: usize,
    variant: FragmentVariant,
    color: Option<wgpu::TextureFormat>,
    depth: Option<wgpu::TextureFormat>,
    samples: u32,
    topology: u8,
    strip_restart: bool,
    cull: u8,
    front_ccw: bool,
    blend: Option<[u32; 6]>,
    mask: u8,
    depth_test: u32,
    depth_write: bool,
    bias: [u32; 2],
    stencil: [u32; 7],
    layout: Vec<(u32, u8, u32)>,
}

/// One vertex buffer stream: buffer, format, stride.
#[derive(Clone)]
pub struct Stream {
    pub buffer: wgpu::Buffer,
    pub format: wgpu::VertexFormat,
    pub stride: u64,
}

/// A geometry's buffers on the GPU.
pub struct Mesh {
    /// By channel name.
    pub streams: HashMap<String, Stream>,
    /// Index lists: topology (3 list, 5 strip, 1 lines), buffer, count.
    pub lists: Vec<(u32, wgpu::Buffer, u32)>,
    /// Line lists of the triangle edges (polygon mode line).
    pub lines: Vec<(wgpu::Buffer, u32)>,
    pub vertices: u32,
}

struct Draw {
    pipeline: usize,
    vs: u64,
    fs: u64,
    fs_group: wgpu::BindGroup,
    streams: Vec<Stream>,
    index: Option<(wgpu::Buffer, u32)>,
    vp: usize,
    count: u32,
    blend_constant: [f64; 4],
    stencil_ref: u32,
}

pub enum ColorTarget {
    Window,
    Texture(usize),
    None,
}

struct PassRec {
    color: Option<(wgpu::TextureView, Option<wgpu::TextureView>)>,
    depth: Option<wgpu::TextureView>,
    color_format: Option<wgpu::TextureFormat>,
    depth_format: Option<wgpu::TextureFormat>,
    samples: u32,
    clear_color: Option<[f64; 4]>,
    clear_depth: Option<f32>,
    clear_stencil: Option<u32>,
    viewport: [f32; 4],
    draws: Vec<Draw>,
}

pub struct Gl {
    pub gpu: Gpu,
    pub textures: Vec<Option<Tex>>,
    pub vertex_programs: Vec<VertexProgram>,
    pub fragment_programs: Vec<FragmentProgram>,
    /// Global vertex program constants.
    pub env: [[f32; 4]; 256],
    /// Texture units: texture.
    pub units: [Option<usize>; 16],
    samplers: HashMap<SamplerKey, wgpu::Sampler>,
    pipelines: Vec<wgpu::RenderPipeline>,
    pipeline_index: HashMap<PipelineKey, usize>,
    uniforms: Vec<u8>,
    uniform_buffer: wgpu::Buffer,
    passes: Vec<PassRec>,
    /// The window: the size the viewer was asked for.
    pub window: (u32, u32),
    pub window_color: Tex,
    pub window_depth: Tex,
    /// Holds a (0,0,0,1)-style attribute for inputs no array feeds.
    defaults: HashMap<u32, Stream>,
    dummy_2d: wgpu::TextureView,
    dummy_cube: wgpu::TextureView,
    dummy_3d: wgpu::TextureView,
    dummy_depth: wgpu::TextureView,
    /// Render targets drawn without a depth target keep their own (a pbuffer's).
    own_depth: HashMap<usize, Tex>,
}

fn align(n: u64) -> u64 {
    n.div_ceil(UNIFORM_ALIGN) * UNIFORM_ALIGN
}

const UNIFORM_CAPACITY: u64 = 16 << 20;

impl Gl {
    pub fn new(gpu: Gpu, window: (u32, u32)) -> Gl {
        let uniform_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: UNIFORM_CAPACITY,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let make = |gpu: &Gpu, kind: TexKind, format: wgpu::TextureFormat| {
            let layers = if kind == TexKind::Cube { 6 } else { 1 };
            let t = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dummy"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: layers },
                mip_level_count: 1,
                sample_count: 1,
                dimension: if kind == TexKind::D3 { wgpu::TextureDimension::D3 } else { wgpu::TextureDimension::D2 },
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(match kind {
                    TexKind::D2 => wgpu::TextureViewDimension::D2,
                    TexKind::Cube => wgpu::TextureViewDimension::Cube,
                    TexKind::D3 => wgpu::TextureViewDimension::D3,
                }),
                ..Default::default()
            })
        };
        let dummy_2d = make(&gpu, TexKind::D2, wgpu::TextureFormat::Rgba8Unorm);
        let dummy_cube = make(&gpu, TexKind::Cube, wgpu::TextureFormat::Rgba8Unorm);
        let dummy_3d = make(&gpu, TexKind::D3, wgpu::TextureFormat::Rgba8Unorm);
        let dummy_depth = make(&gpu, TexKind::D2, wgpu::TextureFormat::Depth32Float);
        let window_color = Self::target_tex(&gpu, window.0, window.1, WINDOW_COLOR, 1, 1);
        let window_depth = Self::target_tex(&gpu, window.0, window.1, WINDOW_DEPTH, 1, 1);
        Gl {
            gpu,
            textures: vec![],
            vertex_programs: vec![],
            fragment_programs: vec![],
            env: [[0.0; 4]; 256],
            units: [None; 16],
            samplers: HashMap::new(),
            pipelines: vec![],
            pipeline_index: HashMap::new(),
            uniforms: vec![],
            uniform_buffer,
            passes: vec![],
            window,
            window_color,
            window_depth,
            defaults: HashMap::new(),
            dummy_2d,
            dummy_cube,
            dummy_3d,
            dummy_depth,
            own_depth: HashMap::new(),
        }
    }

    pub fn target_tex(gpu: &Gpu, w: u32, h: u32, format: wgpu::TextureFormat, samples: u32, levels: u32) -> Tex {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: if samples > 1 { 1 } else { levels },
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | if samples > 1 { wgpu::TextureUsages::empty() } else { wgpu::TextureUsages::TEXTURE_BINDING }
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor { mip_level_count: Some(1), ..Default::default() });
        Tex {
            texture,
            view,
            kind: TexKind::D2,
            format,
            width: w,
            height: h,
            samples,
            state: TextureState::default(),
            levels: if samples > 1 { 1 } else { levels },
        }
    }

    pub fn add_texture(&mut self, t: Option<Tex>) -> usize {
        self.textures.push(t);
        self.textures.len() - 1
    }

    /// Makes a texture from an image with all its levels; missing levels are made by
    /// box filtering (GL_GENERATE_MIPMAP) when they are not compressed.
    pub fn upload(&self, img: &Image, state: TextureState, label: &str) -> Tex {
        let layers = img.layers.len() as u32;
        let levels = img.layers[0].len() as u32;
        let (dimension, size) = match img.kind {
            TexKind::D3 => (wgpu::TextureDimension::D3, wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: img.depth }),
            _ => (wgpu::TextureDimension::D2, wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: layers }),
        };
        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: levels,
            sample_count: 1,
            dimension,
            format: img.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let (bw, bh) = img.format.block_dimensions();
        let bsize = img.format.block_copy_size(None).unwrap_or(4);
        for (layer, mips) in img.layers.iter().enumerate() {
            for (level, data) in mips.iter().enumerate() {
                let w = (img.width >> level).max(1);
                let h = (img.height >> level).max(1);
                let d = if img.kind == TexKind::D3 { (img.depth >> level).max(1) } else { 1 };
                let bx = w.div_ceil(bw);
                let by = h.div_ceil(bh);
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    data,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bx * bsize), rows_per_image: Some(by) },
                    wgpu::Extent3d { width: bx * bw, height: by * bh, depth_or_array_layers: d },
                );
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(match img.kind {
                TexKind::D2 => wgpu::TextureViewDimension::D2,
                TexKind::Cube => wgpu::TextureViewDimension::Cube,
                TexKind::D3 => wgpu::TextureViewDimension::D3,
            }),
            ..Default::default()
        });
        Tex { texture, view, kind: img.kind, format: img.format, width: img.width, height: img.height, samples: 1, state, levels }
    }

    fn sampler(&mut self, s: &TextureState, compare: bool, levels: u32) -> wgpu::Sampler {
        let anisotropy = if s.anisotropy > 1.0 && s.min == 2 && s.mag == 2 && s.mip == 2 { s.anisotropy.min(16.0) as u16 } else { 1 };
        let key = SamplerKey { min: s.min, mag: s.mag, mip: s.mip, wrap: s.wrap, anisotropy, compare, levels: levels > 1 };
        if let Some(x) = self.samplers.get(&key) {
            return x.clone();
        }
        let filter = |f: u32| if f == 1 { wgpu::FilterMode::Nearest } else { wgpu::FilterMode::Linear };
        let wrap = |w: u32| match w {
            1 => wgpu::AddressMode::Repeat,
            3 => wgpu::AddressMode::MirrorRepeat,
            // Border colour is not available everywhere; shadow maps compare outside as lit
            // in the shader [I: as their (1,1,1,1) border would].
            _ => wgpu::AddressMode::ClampToEdge,
        };
        // Mip filter 0 samples level 0 only.
        let (mip, lod_max) = match (s.mip, levels > 1) {
            (0, _) | (_, false) => (wgpu::MipmapFilterMode::Nearest, 0.0),
            (1, true) => (wgpu::MipmapFilterMode::Nearest, 32.0),
            _ => (wgpu::MipmapFilterMode::Linear, 32.0),
        };
        let sampler = self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: wrap(s.wrap[0]),
            address_mode_v: wrap(s.wrap[1]),
            address_mode_w: wrap(s.wrap[2]),
            mag_filter: filter(s.mag),
            min_filter: filter(s.min),
            mipmap_filter: mip,
            lod_min_clamp: 0.0,
            lod_max_clamp: lod_max,
            compare: compare.then_some(wgpu::CompareFunction::LessEqual),
            anisotropy_clamp: anisotropy,
            border_color: None,
        });
        self.samplers.insert(key, sampler.clone());
        sampler
    }

    /// Compiles a listing.
    pub fn add_program(&mut self, text: &str, name: &str) -> Result<(Stage, usize), Error> {
        let listing = nv_asm::parse(text).map_err(|e| format!("{name}: {e}"))?;
        match listing.stage {
            Stage::Vertex => {
                let (src, iface) = wgsl::vertex(&listing, &Default::default()).map_err(|e| format!("{name}: {e}"))?;
                let module = self.gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(name),
                    source: wgpu::ShaderSource::Wgsl(src.into()),
                });
                let mut vars = HashMap::new();
                let mut inputs = HashMap::new();
                for v in &listing.vars {
                    if let Some(c) = v.constants() {
                        vars.insert(v.name.clone(), c);
                    }
                    if let Some(a) = v.resource.strip_prefix("ATTR").and_then(|a| a.parse().ok()) {
                        inputs.insert(v.name.clone(), a);
                    } else if let Some(a) = v.binding.strip_prefix("$vin.") {
                        // vp20 names attributes by their conventional meaning.
                        let idx = match a {
                            "POSITION" => Some(0),
                            "BLENDWEIGHT" => Some(1),
                            "NORMAL" => Some(2),
                            "COLOR0" | "COLOR" => Some(3),
                            "COLOR1" => Some(4),
                            "FOGCOORD" => Some(5),
                            t if t.starts_with("TEXCOORD") => t[8..].parse::<u32>().ok().map(|n| 8 + n),
                            t if t.starts_with("ATTR") => t[4..].parse().ok(),
                            _ => None,
                        };
                        if let Some(i) = idx {
                            inputs.insert(v.name.clone(), i);
                        }
                    }
                }
                let layout = self.gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("vp"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
                let group = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("vp"),
                    layout: &layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniform_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(16 * iface.uniforms as u64),
                        }),
                    }],
                });
                self.vertex_programs.push(VertexProgram { listing, module, iface, vars, inputs, layout, group });
                Ok((Stage::Vertex, self.vertex_programs.len() - 1))
            }
            Stage::Fragment => {
                let mut units = HashMap::new();
                for v in &listing.vars {
                    if let Some(u) = v.texunit() {
                        units.insert(v.name.clone(), u);
                    }
                }
                let params = listing.declares.iter().map(|d| d.1.unwrap_or([0.0; 4])).collect();
                self.fragment_programs.push(FragmentProgram { listing, variants: HashMap::new(), units, params });
                let i = self.fragment_programs.len() - 1;
                // Translate once now so that errors show at load.
                self.fragment_module(i, &FragmentVariant::default())?;
                Ok((Stage::Fragment, i))
            }
        }
    }

    fn fragment_module(&mut self, fp: usize, v: &FragmentVariant) -> Result<&FragmentModule, Error> {
        if !self.fragment_programs[fp].variants.contains_key(v) {
            let p = &self.fragment_programs[fp];
            let opts = wgsl::Options {
                shadow_units: v.shadow.clone(),
                border_units: v.border.clone(),
                expand_units: v.expand.clone(),
                alpha_test: v.alpha,
            };
            let (src, iface) = wgsl::fragment(&p.listing, &opts)?;
            let module = self.gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fp"),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            });
            let mut entries = vec![wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }];
            for (k, t) in iface.textures.iter().enumerate() {
                let view_dimension = match t.target {
                    Target::Cube => wgpu::TextureViewDimension::Cube,
                    Target::T3D => wgpu::TextureViewDimension::D3,
                    _ => wgpu::TextureViewDimension::D2,
                };
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: 1 + 2 * k as u32,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: if t.shadow { wgpu::TextureSampleType::Depth } else { wgpu::TextureSampleType::Float { filterable: true } },
                        view_dimension,
                        multisampled: false,
                    },
                    count: None,
                });
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: 2 + 2 * k as u32,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(if t.shadow {
                        wgpu::SamplerBindingType::Comparison
                    } else {
                        wgpu::SamplerBindingType::Filtering
                    }),
                    count: None,
                });
            }
            let layout = self.gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fp"), entries: &entries });
            self.fragment_programs[fp].variants.insert(v.clone(), FragmentModule { module, iface, layout });
        }
        Ok(&self.fragment_programs[fp].variants[v])
    }

    /// Writes a vertex program parameter (by name) into the global constants.
    pub fn set_vertex_param(&mut self, vp: usize, name: &str, rows: &[[f32; 4]]) {
        if let Some(&(first, count)) = self.vertex_programs[vp].vars.get(name) {
            for (i, r) in rows.iter().take(count as usize).enumerate() {
                if let Some(c) = self.env.get_mut(first as usize + i) {
                    *c = *r;
                }
            }
        }
    }

    /// Binding a vertex program loads its literal constants (`#const`).
    pub fn bind_vertex_program(&mut self, vp: usize) {
        for &(i, v) in &self.vertex_programs[vp].listing.consts {
            if let Some(c) = self.env.get_mut(i as usize) {
                *c = v;
            }
        }
    }

    /// Binds a texture to the unit of a fragment program's sampler.
    pub fn bind_texture(&mut self, fp: usize, name: &str, tex: Option<usize>) {
        if let Some(&u) = self.fragment_programs[fp].units.get(name) {
            self.units[u as usize] = tex;
        }
    }

    pub fn mesh(&self, arrays: &[(String, MeshArray)], lists: &[(u32, Vec<u16>)], restart: bool, as_lists: bool) -> Mesh {
        use wgpu::util::DeviceExt;
        let mut streams = HashMap::new();
        let mut vertices = 0;
        for (name, a) in arrays {
            let (bytes, format, stride) = match a {
                MeshArray::F32(v, k) => (
                    bytemuck_f32(v),
                    match k {
                        1 => wgpu::VertexFormat::Float32,
                        2 => wgpu::VertexFormat::Float32x2,
                        3 => wgpu::VertexFormat::Float32x3,
                        _ => wgpu::VertexFormat::Float32x4,
                    },
                    4 * *k as u64,
                ),
                MeshArray::F16(v, k) => {
                    // WebGPU has 2 and 4 half floats; 1 and 3 are padded.
                    let out_k = if *k <= 2 { 2 } else { 4 };
                    let mut o = Vec::with_capacity(v.len() / *k as usize * out_k);
                    for c in v.chunks_exact(*k as usize) {
                        for i in 0..out_k {
                            o.push(if i < *k as usize { c[i] } else if i == 3 { 0x3c00 } else { 0 });
                        }
                    }
                    (
                        o.iter().flat_map(|h| h.to_le_bytes()).collect(),
                        if out_k == 2 { wgpu::VertexFormat::Float16x2 } else { wgpu::VertexFormat::Float16x4 },
                        2 * out_k as u64,
                    )
                }
                MeshArray::U8(v, _) => (v.clone(), wgpu::VertexFormat::Unorm8x4, 4),
            };
            vertices = vertices.max((bytes.len() as u64 / stride) as u32);
            let buffer = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(name),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
            streams.insert(name.clone(), Stream { buffer, format, stride });
        }
        let mut out = vec![];
        let mut lines = vec![];
        for (prim, idx) in lists {
            // Strips without restart, or a pass asking for lists, become lists (0x48b0a0).
            let (prim, idx) = if *prim == 5 && (!restart || as_lists) {
                (3, strip_to_list(idx, restart))
            } else {
                (*prim, idx.clone())
            };
            let tris = match prim {
                3 => idx.clone(),
                5 => strip_to_list(&idx, true),
                _ => vec![],
            };
            let mut l = Vec::with_capacity(tris.len() * 2);
            for t in tris.chunks_exact(3) {
                l.extend_from_slice(&[t[0], t[1], t[1], t[2], t[2], t[0]]);
            }
            let pad = |mut v: Vec<u16>| {
                if v.len() % 2 == 1 {
                    v.push(*v.last().unwrap_or(&0));
                }
                v
            };
            let count = idx.len() as u32;
            let b = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: &pad(idx).iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(),
                usage: wgpu::BufferUsages::INDEX,
            });
            out.push((prim, b, count));
            let lc = l.len() as u32;
            let lb = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("edges"),
                contents: &pad(l).iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(),
                usage: wgpu::BufferUsages::INDEX,
            });
            lines.push((lb, lc));
        }
        Mesh { streams, lists: out, lines, vertices }
    }

    fn default_stream(&mut self, attribute: u32) -> Stream {
        use wgpu::util::DeviceExt;
        if let Some(s) = self.defaults.get(&attribute) {
            return s.clone();
        }
        // NV_vertex_program's aliased attributes start as GL's current values.
        let v: [f32; 4] = match attribute {
            2 => [0.0, 0.0, 1.0, 1.0],
            3 => [1.0, 1.0, 1.0, 1.0],
            _ => [0.0, 0.0, 0.0, 1.0],
        };
        let buffer = self.gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("default attribute"),
            contents: &bytemuck_f32(&v),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let s = Stream { buffer, format: wgpu::VertexFormat::Float32x4, stride: 0 };
        self.defaults.insert(attribute, s.clone());
        s
    }

    pub fn begin_pass(
        &mut self,
        color: ColorTarget,
        depth: Option<usize>,
        clear_color: Option<[f32; 4]>,
        clear_depth: Option<f32>,
        clear_stencil: Option<u32>,
        viewport: [i32; 4],
    ) {
        let mut color_tex = None;
        let (color, color_format, samples) = match color {
            ColorTarget::Window => (Some((self.window_color.view.clone(), None)), Some(WINDOW_COLOR), 1),
            ColorTarget::Texture(t) => {
                color_tex = Some(t);
                let tex = self.textures[t].as_ref().unwrap();
                (Some((tex.view.clone(), None)), Some(tex.format), tex.samples)
            }
            ColorTarget::None => (None, None, 1),
        };
        let (depth_view, depth_format) = match depth {
            Some(d) => {
                let t = self.textures[d].as_ref().unwrap();
                (Some(t.view.clone()), Some(t.format))
            }
            None => match color_tex {
                None if color_format.is_some() => (Some(self.window_depth.view.clone()), Some(WINDOW_DEPTH)),
                None => (None, None),
                Some(t) => {
                    if !self.own_depth.contains_key(&t) {
                        let c = self.textures[t].as_ref().unwrap();
                        let d = Self::target_tex(&self.gpu, c.width, c.height, WINDOW_DEPTH, c.samples, 1);
                        self.own_depth.insert(t, d);
                    }
                    (Some(self.own_depth[&t].view.clone()), Some(WINDOW_DEPTH))
                }
            },
        };
        self.passes.push(PassRec {
            color,
            depth: depth_view,
            color_format,
            depth_format,
            samples,
            clear_color: clear_color.map(|c| c.map(|x| x as f64)),
            clear_depth,
            clear_stencil,
            viewport: viewport.map(|v| v as f32),
            draws: vec![],
        });
    }

    fn push_uniforms(&mut self, data: &[[f32; 4]]) -> u64 {
        let at = align(self.uniforms.len() as u64);
        self.uniforms.resize(at as usize, 0);
        for v in data {
            for x in v {
                self.uniforms.extend_from_slice(&x.to_le_bytes());
            }
        }
        if data.is_empty() {
            self.uniforms.extend_from_slice(&[0; 16]);
        }
        at
    }

    /// Records a draw with the current program parameters and texture units.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, vp: usize, fp: usize, states: &States, mesh: &Mesh, list: usize) -> Result<(), Error> {
        let Some(pass) = self.passes.last() else { return Ok(()) };
        let (color_format, depth_format, samples) = (pass.color_format, pass.depth_format, pass.samples);
        let (prim, _, _) = &mesh.lists[list];
        let wire = states.raster.draw_style != 0;
        // Which units hold shadow maps or expanded normals, from their textures.
        let fpl = &self.fragment_programs[fp];
        let used: Vec<u32> = fpl.units.values().copied().collect();
        let mut variant = FragmentVariant::default();
        for u in used {
            if let Some(t) = self.units[u as usize].and_then(|t| self.textures[t].as_ref()) {
                if t.state.shadow && t.is_depth() {
                    variant.shadow.push(u);
                    if t.state.wrap[0] == 2 {
                        variant.border.push(u);
                    }
                }
                if t.state.remapped {
                    variant.expand.push(u);
                }
            }
        }
        variant.shadow.sort();
        variant.border.sort();
        variant.expand.sort();
        if states.alpha.test {
            let float_target = color_format.is_some_and(|f| matches!(f, wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rgba32Float));
            variant.alpha = Some((states.alpha.func, states.alpha.reference, !float_target));
        }
        self.fragment_module(fp, &variant)?;
        // Vertex streams by the program's inputs.
        let inputs: Vec<(u32, String)> = {
            let p = &self.vertex_programs[vp];
            let mut v: Vec<(u32, String)> = p.iface.attributes.iter().map(|&a| {
                let name = p.inputs.iter().find(|(_, i)| **i == a).map(|(n, _)| n.clone()).unwrap_or_default();
                (a, name)
            }).collect();
            v.sort();
            v
        };
        let mut streams = vec![];
        let mut layout = vec![];
        for (a, name) in &inputs {
            let s = match mesh.streams.get(name) {
                Some(s) => s.clone(),
                None => self.default_stream(*a),
            };
            layout.push((*a, format_code(s.format), s.stride as u32));
            streams.push(s);
        }
        let topology = if wire { 1 } else { *prim as u8 };
        let blend = (states.blend.op != 0).then_some([
            states.blend.color_src,
            states.blend.color_dst,
            states.blend.alpha_src,
            states.blend.alpha_dst,
            states.blend.op,
            0,
        ]);
        let key = PipelineKey {
            vp,
            fp,
            variant: variant.clone(),
            color: color_format,
            depth: depth_format,
            samples,
            topology,
            strip_restart: topology == 5,
            cull: if wire { 3 } else { states.cull.face as u8 },
            front_ccw: states.cull.ccw_front,
            blend,
            mask: states.blend.mask.iter().enumerate().fold(0, |m, (i, &b)| m | ((b as u8) << i)),
            depth_test: if states.depth.test { states.depth.func } else { 7 },
            depth_write: states.depth.write && states.depth.test,
            bias: [states.depth.offset_scale.to_bits(), states.depth.offset_bias.to_bits()],
            stencil: if states.stencil.enabled {
                [1, states.stencil.func, states.stencil.func_mask, states.stencil.fail, states.stencil.zfail, states.stencil.zpass, states.stencil.write_mask as u32]
            } else {
                [0; 7]
            },
            layout,
        };
        let pipeline = self.pipeline(&key)?;
        // Uniforms: the constants the vertex program can read, the fragment parameters.
        let n = self.vertex_programs[vp].iface.uniforms as usize;
        let env = self.env;
        let vs = self.push_uniforms(&env[..n.min(256)]);
        let params = self.fragment_programs[fp].params.clone();
        let fs = self.push_uniforms(&params);
        let fs_group = self.fragment_group(fp, &variant)?;
        let (index, count) = if wire {
            let (b, c) = &mesh.lines[list];
            (Some((b.clone(), *c)), *c)
        } else {
            let (_, b, c) = &mesh.lists[list];
            (Some((b.clone(), *c)), *c)
        };
        let draw = Draw {
            pipeline,
            vs,
            fs,
            fs_group,
            streams,
            index,
            vp,
            count,
            blend_constant: states.blend.color.map(|c| c as f64),
            stencil_ref: states.stencil.reference,
        };
        self.passes.last_mut().unwrap().draws.push(draw);
        Ok(())
    }

    fn fragment_group(&mut self, fp: usize, v: &FragmentVariant) -> Result<wgpu::BindGroup, Error> {
        let textures = self.fragment_module(fp, v)?.iface.textures.clone();
        let mut views = vec![];
        let mut samplers = vec![];
        for t in &textures {
            let tex = self.units[t.unit as usize].and_then(|i| self.textures[i].as_ref().map(|x| (i, x)));
            let (view, state, levels) = match tex {
                Some((_, x)) if x.samples == 1 && kind_matches(x, t.target, t.shadow) => (x.view.clone(), x.state, x.levels),
                _ => (
                    match (t.shadow, t.target) {
                        (true, _) => self.dummy_depth.clone(),
                        (_, Target::Cube) => self.dummy_cube.clone(),
                        (_, Target::T3D) => self.dummy_3d.clone(),
                        _ => self.dummy_2d.clone(),
                    },
                    TextureState::default(),
                    1,
                ),
            };
            views.push(view);
            samplers.push(self.sampler(&state, t.shadow, levels));
        }
        self.fragment_module(fp, v)?;
        let m = &self.fragment_programs[fp].variants[v];
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &self.uniform_buffer,
                offset: 0,
                size: wgpu::BufferSize::new(16 * m.iface.uniforms.max(1) as u64),
            }),
        }];
        for k in 0..views.len() {
            entries.push(wgpu::BindGroupEntry { binding: 1 + 2 * k as u32, resource: wgpu::BindingResource::TextureView(&views[k]) });
            entries.push(wgpu::BindGroupEntry { binding: 2 + 2 * k as u32, resource: wgpu::BindingResource::Sampler(&samplers[k]) });
        }
        Ok(self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fp"), layout: &m.layout, entries: &entries }))
    }

    fn pipeline(&mut self, key: &PipelineKey) -> Result<usize, Error> {
        if let Some(&i) = self.pipeline_index.get(key) {
            return Ok(i);
        }
        let vpp = &self.vertex_programs[key.vp];
        let fm = &self.fragment_programs[key.fp].variants[&key.variant];
        let layout = self.gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nv"),
            bind_group_layouts: &[Some(&vpp.layout), Some(&fm.layout)],
            immediate_size: 0,
        });
        let attrs: Vec<[wgpu::VertexAttribute; 1]> = key
            .layout
            .iter()
            .map(|(a, f, _)| [wgpu::VertexAttribute { format: format_of(*f), offset: 0, shader_location: *a }])
            .collect();
        let buffers: Vec<Option<wgpu::VertexBufferLayout>> = key
            .layout
            .iter()
            .zip(&attrs)
            .map(|((_, _, stride), a)| {
                Some(wgpu::VertexBufferLayout { array_stride: *stride as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: a })
            })
            .collect();
        let factor = |f: u32| match f {
            0 => wgpu::BlendFactor::Zero,
            1 => wgpu::BlendFactor::One,
            2 => wgpu::BlendFactor::Src,
            3 => wgpu::BlendFactor::OneMinusSrc,
            4 => wgpu::BlendFactor::SrcAlpha,
            5 => wgpu::BlendFactor::OneMinusSrcAlpha,
            6 => wgpu::BlendFactor::Dst,
            7 => wgpu::BlendFactor::OneMinusDst,
            8 => wgpu::BlendFactor::DstAlpha,
            9 => wgpu::BlendFactor::OneMinusDstAlpha,
            10 => wgpu::BlendFactor::SrcAlphaSaturated,
            11 | 13 => wgpu::BlendFactor::Constant,
            _ => wgpu::BlendFactor::OneMinusConstant,
        };
        let op = |o: u32| match o {
            2 => wgpu::BlendOperation::Subtract,
            3 => wgpu::BlendOperation::ReverseSubtract,
            4 => wgpu::BlendOperation::Min,
            5 => wgpu::BlendOperation::Max,
            _ => wgpu::BlendOperation::Add,
        };
        let blend = key.blend.map(|b| {
            let minmax = |c: wgpu::BlendComponent| match c.operation {
                wgpu::BlendOperation::Min | wgpu::BlendOperation::Max => {
                    wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, ..c }
                }
                _ => c,
            };
            wgpu::BlendState {
                color: minmax(wgpu::BlendComponent { src_factor: factor(b[0]), dst_factor: factor(b[1]), operation: op(b[4]) }),
                alpha: minmax(wgpu::BlendComponent { src_factor: factor(b[2]), dst_factor: factor(b[3]), operation: op(b[4]) }),
            }
        });
        let compare = |f: u32| match f {
            0 => wgpu::CompareFunction::Never,
            1 => wgpu::CompareFunction::Less,
            2 => wgpu::CompareFunction::LessEqual,
            3 => wgpu::CompareFunction::Greater,
            4 => wgpu::CompareFunction::GreaterEqual,
            5 => wgpu::CompareFunction::Equal,
            6 => wgpu::CompareFunction::NotEqual,
            _ => wgpu::CompareFunction::Always,
        };
        let sop = |o: u32| match o {
            1 => wgpu::StencilOperation::Zero,
            2 => wgpu::StencilOperation::Replace,
            3 => wgpu::StencilOperation::IncrementClamp,
            4 => wgpu::StencilOperation::DecrementClamp,
            5 => wgpu::StencilOperation::Invert,
            6 => wgpu::StencilOperation::IncrementWrap,
            7 => wgpu::StencilOperation::DecrementWrap,
            _ => wgpu::StencilOperation::Keep,
        };
        let depth_stencil = key.depth.map(|format| {
            let has_stencil = format.has_stencil_aspect();
            let st = key.stencil;
            let face = if st[0] == 1 && has_stencil {
                wgpu::StencilFaceState { compare: compare(st[1]), fail_op: sop(st[3]), depth_fail_op: sop(st[4]), pass_op: sop(st[5]) }
            } else {
                wgpu::StencilFaceState::IGNORE
            };
            wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(key.depth_write),
                depth_compare: Some(compare(key.depth_test)),
                stencil: wgpu::StencilState {
                    front: face,
                    back: face,
                    read_mask: if st[0] == 1 { st[2] } else { 0xff },
                    write_mask: if st[0] == 1 && st[6] != 0 { 0xff } else { 0 },
                },
                bias: wgpu::DepthBiasState {
                    constant: f32::from_bits(key.bias[1]) as i32,
                    slope_scale: f32::from_bits(key.bias[0]),
                    clamp: 0.0,
                },
            }
        });
        let topology = match key.topology {
            1 => wgpu::PrimitiveTopology::LineList,
            5 => wgpu::PrimitiveTopology::TriangleStrip,
            _ => wgpu::PrimitiveTopology::TriangleList,
        };
        // Clip y turns over in the vertex programs, so winding does too.
        let front_face = if key.front_ccw { wgpu::FrontFace::Cw } else { wgpu::FrontFace::Ccw };
        let cull_mode = match key.cull {
            0 => Some(wgpu::Face::Front),
            1 => Some(wgpu::Face::Back),
            // FRONT_AND_BACK draws nothing but lines and points; there are none of those here.
            _ => None,
        };
        let mask = wgpu::ColorWrites::from_bits_truncate(key.mask as u32);
        let targets = [key.color.map(|format| wgpu::ColorTargetState { format, blend, write_mask: mask })];
        let pipeline = self.gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nv"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &vpp.module, entry_point: Some("main"), compilation_options: Default::default(), buffers: &buffers },
            primitive: wgpu::PrimitiveState {
                topology,
                strip_index_format: (topology == wgpu::PrimitiveTopology::TriangleStrip).then_some(wgpu::IndexFormat::Uint16),
                front_face,
                cull_mode,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState { count: key.samples, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &fm.module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: if key.color.is_some() { &targets } else { &[] },
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.push(pipeline);
        let i = self.pipelines.len() - 1;
        self.pipeline_index.insert(key.clone(), i);
        Ok(i)
    }

    /// Encodes and submits every recorded pass.
    pub fn submit(&mut self, extra: impl FnOnce(&mut wgpu::CommandEncoder, &Gl)) {
        let device = &self.gpu.device;
        if self.uniforms.len() as u64 > UNIFORM_CAPACITY {
            self.uniforms.truncate(UNIFORM_CAPACITY as usize);
        }
        self.gpu.queue.write_buffer(&self.uniform_buffer, 0, &self.uniforms);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let passes = std::mem::take(&mut self.passes);
        for p in &passes {
            let color_att = p.color.as_ref().map(|(view, resolve)| wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: resolve.as_ref(),
                ops: wgpu::Operations {
                    load: match p.clear_color {
                        Some(c) => wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: c[3] }),
                        None => wgpu::LoadOp::Load,
                    },
                    store: wgpu::StoreOp::Store,
                },
            });
            let depth_att = p.depth.as_ref().map(|view| {
                let stencil = p.depth_format.is_some_and(|f| f.has_stencil_aspect());
                wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: match p.clear_depth {
                            Some(d) => wgpu::LoadOp::Clear(d),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: stencil.then(|| wgpu::Operations {
                        load: match p.clear_stencil {
                            Some(s) => wgpu::LoadOp::Clear(s),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                }
            });
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("pass"),
                    color_attachments: &[color_att],
                    depth_stencil_attachment: depth_att,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let [x, y, w, h] = p.viewport;
                if w > 0.0 && h > 0.0 {
                    rp.set_viewport(x, y, w, h, 0.0, 1.0);
                }
                for d in &p.draws {
                    rp.set_pipeline(&self.pipelines[d.pipeline]);
                    rp.set_bind_group(0, &self.vertex_programs[d.vp].group, &[d.vs as u32]);
                    rp.set_bind_group(1, &d.fs_group, &[d.fs as u32]);
                    rp.set_blend_constant(wgpu::Color { r: d.blend_constant[0], g: d.blend_constant[1], b: d.blend_constant[2], a: d.blend_constant[3] });
                    rp.set_stencil_reference(d.stencil_ref);
                    for (k, s) in d.streams.iter().enumerate() {
                        rp.set_vertex_buffer(k as u32, s.buffer.slice(..));
                    }
                    match &d.index {
                        Some((b, c)) => {
                            rp.set_index_buffer(b.slice(..), wgpu::IndexFormat::Uint16);
                            rp.draw_indexed(0..*c, 0, 0..1);
                        }
                        None => rp.draw(0..d.count, 0..1),
                    }
                }
            }
        }
        extra(&mut encoder, self);
        self.gpu.queue.submit([encoder.finish()]);
        self.uniforms.clear();
    }
}

fn kind_matches(t: &Tex, target: Target, shadow: bool) -> bool {
    let k = match target {
        Target::Cube => TexKind::Cube,
        Target::T3D => TexKind::D3,
        _ => TexKind::D2,
    };
    t.kind == k && (t.is_depth() == shadow || (!shadow && !t.is_depth()))
}

pub enum MeshArray {
    F32(Vec<f32>, u32),
    F16(Vec<u16>, u32),
    U8(Vec<u8>, u32),
}

fn bytemuck_f32(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn format_code(f: wgpu::VertexFormat) -> u8 {
    match f {
        wgpu::VertexFormat::Float32 => 0,
        wgpu::VertexFormat::Float32x2 => 1,
        wgpu::VertexFormat::Float32x3 => 2,
        wgpu::VertexFormat::Float32x4 => 3,
        wgpu::VertexFormat::Float16x2 => 4,
        wgpu::VertexFormat::Float16x4 => 5,
        _ => 6,
    }
}

fn format_of(c: u8) -> wgpu::VertexFormat {
    match c {
        0 => wgpu::VertexFormat::Float32,
        1 => wgpu::VertexFormat::Float32x2,
        2 => wgpu::VertexFormat::Float32x3,
        3 => wgpu::VertexFormat::Float32x4,
        4 => wgpu::VertexFormat::Float16x2,
        5 => wgpu::VertexFormat::Float16x4,
        _ => wgpu::VertexFormat::Unorm8x4,
    }
}

/// Triangle strips to lists; with restart, 0xffff starts a new strip.
pub fn strip_to_list(idx: &[u16], restart: bool) -> Vec<u16> {
    let mut out = vec![];
    let mut strip: Vec<u16> = vec![];
    let flush = |s: &mut Vec<u16>, out: &mut Vec<u16>| {
        for i in 2..s.len() {
            let (a, b, c) = (s[i - 2], s[i - 1], s[i]);
            if i % 2 == 0 {
                out.extend_from_slice(&[a, b, c]);
            } else {
                out.extend_from_slice(&[b, a, c]);
            }
        }
        s.clear();
    };
    for &i in idx {
        if restart && i == 0xffff {
            flush(&mut strip, &mut out);
        } else {
            strip.push(i);
        }
    }
    flush(&mut strip, &mut out);
    out
}
