//! Direct3D 6-8 fixed-function rendering on wgpu.
//!
//! The demo hands over the same calls it made to Direct3D: render states and texture stage
//! states by their D3D numbers, transforms, lights, a material, and indexed or plain
//! primitives. Calls are recorded and drawn together at the end of the frame by one shader
//! that interprets the states (`shaders/ff.wgsl`).
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::Error;
use crate::image::Image;

pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
pub const SAMPLES: u32 = 4;
pub const STAGES: usize = 8;
/// Stages that may hold a cube map.
const CUBES: usize = 4;
const SLOT: usize = 256;

// Render states (D3DRENDERSTATETYPE).
pub const ZENABLE: u32 = 7;
pub const FILLMODE: u32 = 8;
pub const SHADEMODE: u32 = 9;
pub const ZWRITEENABLE: u32 = 14;
pub const ALPHATESTENABLE: u32 = 15;
pub const SRCBLEND: u32 = 19;
pub const DESTBLEND: u32 = 20;
pub const CULLMODE: u32 = 22;
pub const ZFUNC: u32 = 23;
pub const ALPHAREF: u32 = 24;
pub const ALPHAFUNC: u32 = 25;
pub const ALPHABLENDENABLE: u32 = 27;
pub const FOGENABLE: u32 = 28;
pub const SPECULARENABLE: u32 = 29;
pub const FOGCOLOR: u32 = 34;
pub const FOGTABLEMODE: u32 = 35;
pub const FOGSTART: u32 = 36;
pub const FOGEND: u32 = 37;
pub const FOGDENSITY: u32 = 38;
pub const ZBIAS: u32 = 47;
pub const RANGEFOGENABLE: u32 = 48;
pub const STENCILENABLE: u32 = 52;
pub const STENCILFAIL: u32 = 53;
pub const STENCILZFAIL: u32 = 54;
pub const STENCILPASS: u32 = 55;
pub const STENCILFUNC: u32 = 56;
pub const STENCILREF: u32 = 57;
pub const STENCILMASK: u32 = 58;
pub const STENCILWRITEMASK: u32 = 59;
pub const TEXTUREFACTOR: u32 = 60;
pub const LIGHTING: u32 = 137;
pub const AMBIENT: u32 = 139;
pub const FOGVERTEXMODE: u32 = 140;
pub const COLORVERTEX: u32 = 141;
pub const LOCALVIEWER: u32 = 142;
pub const NORMALIZENORMALS: u32 = 143;
pub const DIFFUSEMATERIALSOURCE: u32 = 145;
pub const SPECULARMATERIALSOURCE: u32 = 146;
pub const AMBIENTMATERIALSOURCE: u32 = 147;
pub const EMISSIVEMATERIALSOURCE: u32 = 148;
pub const CLIPPLANEENABLE: u32 = 152;
pub const COLORWRITEENABLE: u32 = 168;
pub const BLENDOP: u32 = 171;

// Texture stage states (D3DTEXTURESTAGESTATETYPE).
pub const COLOROP: u32 = 1;
pub const COLORARG1: u32 = 2;
pub const COLORARG2: u32 = 3;
pub const ALPHAOP: u32 = 4;
pub const ALPHAARG1: u32 = 5;
pub const ALPHAARG2: u32 = 6;
pub const BUMPENVMAT00: u32 = 7;
pub const BUMPENVMAT01: u32 = 8;
pub const BUMPENVMAT10: u32 = 9;
pub const BUMPENVMAT11: u32 = 10;
pub const TEXCOORDINDEX: u32 = 11;
pub const ADDRESS: u32 = 12;
pub const ADDRESSU: u32 = 13;
pub const ADDRESSV: u32 = 14;
pub const MAGFILTER: u32 = 16;
pub const MINFILTER: u32 = 17;
pub const MIPFILTER: u32 = 18;
pub const MAXANISOTROPY: u32 = 21;
pub const BUMPENVLSCALE: u32 = 22;
pub const BUMPENVLOFFSET: u32 = 23;
pub const TEXTURETRANSFORMFLAGS: u32 = 24;
pub const COLORARG0: u32 = 26;
pub const ALPHAARG0: u32 = 27;
pub const RESULTARG: u32 = 28;

// Texture operations (D3DTEXTUREOP).
pub const DISABLE: u32 = 1;
pub const SELECTARG1: u32 = 2;
pub const SELECTARG2: u32 = 3;
pub const MODULATE: u32 = 4;
pub const MODULATE2X: u32 = 5;
pub const ADD: u32 = 7;
pub const BUMPENVMAP: u32 = 22;
pub const BUMPENVMAPLUMINANCE: u32 = 23;
// Arguments (D3DTA).
pub const TA_DIFFUSE: u32 = 0;
pub const TA_CURRENT: u32 = 1;
pub const TA_TEXTURE: u32 = 2;
pub const TA_TFACTOR: u32 = 3;
pub const TA_SPECULAR: u32 = 4;

// Primitive types.
pub const POINTLIST: u32 = 1;
pub const LINELIST: u32 = 2;
pub const LINESTRIP: u32 = 3;
pub const TRIANGLELIST: u32 = 4;
pub const TRIANGLESTRIP: u32 = 5;
pub const TRIANGLEFAN: u32 = 6;

// Vertex contents (D3DFVF).
pub const FVF_XYZ: u32 = 0x002;
pub const FVF_XYZRHW: u32 = 0x004;
pub const FVF_NORMAL: u32 = 0x010;
pub const FVF_DIFFUSE: u32 = 0x040;
pub const FVF_SPECULAR: u32 = 0x080;

pub const LIGHT_POINT: u32 = 1;
pub const LIGHT_SPOT: u32 = 2;
pub const LIGHT_DIRECTIONAL: u32 = 3;

/// One vertex with room for everything a flexible vertex format can hold. Colors are
/// D3DCOLOR, 0xAARRGGBB.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// x, y, z and, for transformed vertices, 1/w.
    pub pos: [f32; 4],
    pub normal: [f32; 3],
    pub diffuse: u32,
    pub specular: u32,
    pub uv: [[f32; 2]; STAGES],
}

impl Default for Vertex {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0, 0.0, 1.0],
            normal: [0.0; 3],
            diffuse: 0xffff_ffff,
            specular: 0,
            uv: [[0.0; 2]; STAGES],
        }
    }
}

impl Vertex {
    pub fn at(p: Vec3) -> Self {
        Self {
            pos: [p.x, p.y, p.z, 1.0],
            ..Self::default()
        }
    }
    /// Already transformed: x and y in pixels, z from 0 to 1.
    pub fn screen(x: f32, y: f32, z: f32, rhw: f32) -> Self {
        Self {
            pos: [x, y, z, rhw],
            ..Self::default()
        }
    }
    pub fn normal(mut self, n: Vec3) -> Self {
        self.normal = n.to_array();
        self
    }
    pub fn color(mut self, c: u32) -> Self {
        self.diffuse = c;
        self
    }
    pub fn specular(mut self, c: u32) -> Self {
        self.specular = c;
        self
    }
    pub fn uv(mut self, set: usize, u: f32, v: f32) -> Self {
        self.uv[set] = [u, v];
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Light {
    pub kind: u32,
    pub diffuse: Vec4,
    pub specular: Vec4,
    pub ambient: Vec4,
    pub position: Vec3,
    pub direction: Vec3,
    pub range: f32,
    pub falloff: f32,
    pub attenuation: [f32; 3],
    pub theta: f32,
    pub phi: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Material {
    pub diffuse: Vec4,
    pub ambient: Vec4,
    pub specular: Vec4,
    pub emissive: Vec4,
    pub power: f32,
}

pub fn color(c: u32) -> Vec4 {
    let b = |s: u32| ((c >> s) & 0xff) as f32 / 255.0;
    Vec4::new(b(16), b(8), b(0), b(24))
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self, Error> {
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("demo"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        Ok(Self { device, queue })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn headless() -> Result<Self, Error> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
        pollster::block_on(Self::from_adapter(&adapter))
    }

    pub fn target(
        &self,
        (w, h): (u32, u32),
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> wgpu::Texture {
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if samples == 1 {
            usage |= wgpu::TextureUsages::COPY_SRC;
            if format == COLOR {
                usage |= wgpu::TextureUsages::TEXTURE_BINDING;
            }
        }
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent(w, h, 1),
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// Texel rows of a texture, tightly packed. Native only: it waits.
    pub fn read(&self, t: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (t.width(), t.height());
        let padded = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            t.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            t.size(),
        );
        self.queue.submit([enc.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll device");
        let data = buffer.slice(..).get_mapped_range().expect("mapped range");
        data.chunks(padded as usize)
            .flat_map(|r| r[..w as usize * 4].to_vec())
            .collect()
    }
}

fn extent(w: u32, h: u32, layers: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: layers,
    }
}

/// D3DXMatrixPerspectiveFovLH.
pub fn perspective_lh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let h = 1.0 / (fov_y / 2.0).tan();
    let q = far / (far - near);
    Mat4::from_cols_array_2d(&[
        [h / aspect, 0.0, 0.0, 0.0],
        [0.0, h, 0.0, 0.0],
        [0.0, 0.0, q, 1.0],
        [0.0, 0.0, -q * near, 0.0],
    ])
}

/// D3DXMatrixLookAtLH, in D3D's row layout (the fourth row moves).
pub fn look_at_lh(eye: Vec3, at: Vec3, up: Vec3) -> Mat4 {
    let z = (at - eye).normalize();
    let x = up.cross(z).normalize();
    let y = z.cross(x);
    Mat4::from_cols_array_2d(&[
        [x.x, y.x, z.x, 0.0],
        [x.y, y.y, z.y, 0.0],
        [x.z, y.z, z.z, 0.0],
        [-x.dot(eye), -y.dot(eye), -z.dot(eye), 1.0],
    ])
}

/// A D3D matrix from its sixteen numbers in memory order (_11, _12, ... _44).
pub fn d3d_matrix(m: [f32; 16]) -> Mat4 {
    Mat4::from_cols_array(&m)
}

/// Row vector times D3D matrix: `p * m`.
pub fn transform(m: Mat4, p: Vec3) -> Vec3 {
    let r = m * p.extend(1.0);
    r.truncate() / r.w
}

pub type TextureId = usize;
pub type BufferId = usize;

#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    /// Copied into the frame.
    Data(&'a [Vertex]),
    /// A buffer made with `buffer`, from this vertex on.
    Buffer(BufferId, u32),
}

struct Texture {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    cube: bool,
    /// Samples and depth of a picture that is drawn to.
    target: Option<(wgpu::TextureView, wgpu::TextureView)>,
    size: (u32, u32),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct StageU {
    op: [u32; 4],
    aop: [u32; 4],
    misc: [u32; 4],
    bump: [f32; 4],
    bump2: [f32; 4],
    tm: [f32; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawU {
    world: [f32; 16],
    view: [f32; 16],
    proj: [f32; 16],
    normal: [f32; 16],
    flags: [u32; 4],
    sources: [u32; 4],
    fog: [f32; 4],
    fog_color: [f32; 4],
    fog_mode: [u32; 4],
    ambient: [f32; 4],
    tfactor: [f32; 4],
    alpha: [u32; 4],
    viewport: [f32; 4],
    m_diffuse: [f32; 4],
    m_ambient: [f32; 4],
    m_specular: [f32; 4],
    m_emissive: [f32; 4],
    m_power: [f32; 4],
    stages: [StageU; STAGES],
    clip_mask: [u32; 4],
    clip: [[f32; 4]; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightU {
    kind: [u32; 4],
    diffuse: [f32; 4],
    specular: [f32; 4],
    ambient: [f32; 4],
    position: [f32; 4],
    direction: [f32; 4],
    attenuation: [f32; 4],
    spot: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightsU {
    lights: [LightU; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    blend: Option<(u32, u32, u32)>,
    zfunc: u32,
    zwrite: bool,
    cull: u32,
    topology: u32,
    write: u32,
    stencil: Option<(u32, u32, u32, u32, u32, u32)>,
    format: wgpu::TextureFormat,
    depth_bias: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SamplerKey {
    address: [u32; 2],
    mag: u32,
    min: u32,
    mip: u32,
    aniso: u32,
}

type GroupKey = (
    [TextureId; STAGES],
    [TextureId; CUBES],
    [SamplerKey; STAGES],
);

struct Call {
    pipeline: PipelineKey,
    textures: [TextureId; STAGES],
    cubes: [TextureId; CUBES],
    samplers: [SamplerKey; STAGES],
    draw: u32,
    lights: u32,
    stencil_ref: u32,
    viewport: [f32; 6],
    vertices: Option<BufferId>,
    base: i32,
    indices: std::ops::Range<u32>,
}

enum Item {
    Draw(Call),
    Program(ProgCall),
}

struct Pass {
    target: Option<TextureId>,
    clear_color: Option<[f64; 4]>,
    clear_depth: Option<(f32, u32)>,
    items: Vec<Item>,
}

/// The device: states, resources and the frame's calls.
pub struct Device {
    /// 7 for DirectX 6 and 7 numbering of filters, 8 for DirectX 8.
    pub version: u32,
    rs: Vec<u32>,
    tss: [[u32; 32]; STAGES],
    bound: [TextureId; STAGES],
    world: Mat4,
    view: Mat4,
    proj: Mat4,
    texture_transforms: [Mat4; STAGES],
    clip_planes: [[f32; 4]; 6],
    lights: [Light; 8],
    enabled: [bool; 8],
    material: Material,
    viewport: [f32; 6],
    target: Option<TextureId>,
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    samplers: HashMap<SamplerKey, wgpu::Sampler>,
    groups: HashMap<GroupKey, wgpu::BindGroup>,
    textures: Vec<Texture>,
    buffers: Vec<(wgpu::Buffer, u32)>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    draws: Vec<u8>,
    light_blocks: Vec<u8>,
    last_lights: Option<(LightsU, u32)>,
    passes: Vec<Pass>,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    uniform: Option<(wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup)>,
    window: Option<((u32, u32), wgpu::TextureView, wgpu::TextureView)>,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    white: TextureId,
    white_cube: TextureId,
    white_volume: TextureId,
    programs: Vec<wgpu::ShaderModule>,
    prog_layout: Option<(
        wgpu::PipelineLayout,
        wgpu::BindGroupLayout,
        wgpu::BindGroupLayout,
    )>,
    prog_pipelines: HashMap<(usize, PipelineKey), wgpu::RenderPipeline>,
    prog_groups: HashMap<ProgGroupKey, wgpu::BindGroup>,
    prog_consts: Vec<u8>,
    prog_uniform: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    pbuffers: Vec<(wgpu::Buffer, u32)>,
    pindices: Vec<(wgpu::Buffer, u32)>,
}

const SHADER: &str = include_str!("../shaders/ff.wgsl");

impl Device {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, version: u32) -> Self {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fixed function"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform = |binding, size: usize| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(size as u64),
            },
            count: None,
        };
        let uniform_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                uniform(0, size_of::<DrawU>()),
                uniform(1, size_of::<LightsU>()),
            ],
        });
        let fragment = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let mut entries = Vec::new();
        let tex = |cube: bool| wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: match cube {
                true => wgpu::TextureViewDimension::Cube,
                false => wgpu::TextureViewDimension::D2,
            },
            multisampled: false,
        };
        let sampler = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        for i in 0..STAGES as u32 {
            entries.push(fragment(i * 2, tex(false)));
            entries.push(fragment(i * 2 + 1, sampler));
        }
        for i in 0..CUBES as u32 {
            entries.push(fragment(32 + i, tex(true)));
        }
        let texture_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let mut rs = vec![0u32; 256];
        for (k, v) in [
            (ZENABLE, 1),
            (FILLMODE, 3),
            (SHADEMODE, 2),
            (ZWRITEENABLE, 1),
            (SRCBLEND, 2),
            (DESTBLEND, 1),
            (CULLMODE, 3),
            (ZFUNC, 4),
            (ALPHAFUNC, 8),
            (FOGDENSITY, 1.0f32.to_bits()),
            (FOGEND, 1.0f32.to_bits()),
            (STENCILFAIL, 1),
            (STENCILZFAIL, 1),
            (STENCILPASS, 1),
            (STENCILFUNC, 8),
            (STENCILMASK, 0xffff_ffff),
            (STENCILWRITEMASK, 0xffff_ffff),
            (TEXTUREFACTOR, 0xffff_ffff),
            (LIGHTING, 1),
            (COLORVERTEX, 1),
            (LOCALVIEWER, 1),
            (DIFFUSEMATERIALSOURCE, 1),
            (SPECULARMATERIALSOURCE, 2),
            (COLORWRITEENABLE, 15),
            (BLENDOP, 1),
        ] {
            rs[k as usize] = v;
        }
        let mut tss = [[0u32; 32]; STAGES];
        for (s, t) in tss.iter_mut().enumerate() {
            t[COLOROP as usize] = if s == 0 { MODULATE } else { DISABLE };
            t[COLORARG1 as usize] = TA_TEXTURE;
            t[COLORARG2 as usize] = TA_CURRENT;
            t[ALPHAOP as usize] = if s == 0 { SELECTARG1 } else { DISABLE };
            t[ALPHAARG1 as usize] = TA_TEXTURE;
            t[ALPHAARG2 as usize] = TA_CURRENT;
            t[TEXCOORDINDEX as usize] = s as u32;
            t[ADDRESSU as usize] = 1;
            t[ADDRESSV as usize] = 1;
            t[MAGFILTER as usize] = 1;
            t[MINFILTER as usize] = 1;
            t[MIPFILTER as usize] = if version >= 8 { 0 } else { 1 };
            t[MAXANISOTROPY as usize] = 1;
            t[RESULTARG as usize] = TA_CURRENT;
        }
        let mut dev = Self {
            version,
            rs,
            tss,
            bound: [0; STAGES],
            world: Mat4::IDENTITY,
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
            texture_transforms: [Mat4::IDENTITY; STAGES],
            clip_planes: [[0.0; 4]; 6],
            lights: [Light::default(); 8],
            enabled: [false; 8],
            material: Material::default(),
            viewport: [0.0, 0.0, 640.0, 480.0, 0.0, 1.0],
            target: None,
            module,
            layout,
            uniform_layout,
            texture_layout,
            pipelines: HashMap::new(),
            samplers: HashMap::new(),
            groups: HashMap::new(),
            textures: Vec::new(),
            buffers: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            draws: Vec::new(),
            light_blocks: Vec::new(),
            last_lights: None,
            passes: Vec::new(),
            vertex_buffer: None,
            index_buffer: None,
            uniform: None,
            window: None,
            format,
            size: (640, 480),
            white: 0,
            white_cube: 0,
            white_volume: 0,
            programs: Vec::new(),
            prog_layout: None,
            prog_pipelines: HashMap::new(),
            prog_groups: HashMap::new(),
            prog_consts: Vec::new(),
            prog_uniform: None,
            pbuffers: Vec::new(),
            pindices: Vec::new(),
        };
        let white = Image::solid([255; 4]);
        dev.white = dev.texture(gpu, &white, false);
        dev.white_cube = dev.add(gpu, &[&white; 6], false, true);
        dev.white_volume = dev.volume(gpu, std::slice::from_ref(&white));
        dev.bound = [0; STAGES];
        dev
    }

    // ----- resources

    fn add(&mut self, gpu: &Gpu, layers: &[&Image], mips: bool, cube: bool) -> TextureId {
        let first = layers[0];
        let levels = match mips {
            true => first.width.max(first.height).ilog2() + 1,
            false => 1,
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent(first.width, first.height, layers.len() as u32),
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, image) in layers.iter().enumerate() {
            write(gpu, &texture, layer as u32, image);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(match cube {
                true => wgpu::TextureViewDimension::Cube,
                false => wgpu::TextureViewDimension::D2,
            }),
            ..Default::default()
        });
        self.textures.push(Texture {
            texture,
            view,
            cube,
            target: None,
            size: (first.width, first.height),
        });
        self.textures.len() - 1
    }

    /// A picture, with a chain of smaller ones if `mips`.
    pub fn texture(&mut self, gpu: &Gpu, image: &Image, mips: bool) -> TextureId {
        self.add(gpu, &[image], mips, false)
    }

    /// A picture with its smaller pictures given, largest first, as a texture loaded level by
    /// level. Levels past the last are not made: sampling stops at it.
    pub fn texture_levels(&mut self, gpu: &Gpu, levels: &[Image]) -> TextureId {
        let first = &levels[0];
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent(first.width, first.height, 1),
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, image) in levels.iter().enumerate() {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width * 4),
                    rows_per_image: None,
                },
                extent(image.width, image.height, 1),
            );
        }
        let view = texture.create_view(&Default::default());
        self.textures.push(Texture {
            texture,
            view,
            cube: false,
            target: None,
            size: (first.width, first.height),
        });
        self.textures.len() - 1
    }

    /// Faces +X, -X, +Y, -Y, +Z, -Z.
    pub fn cube(&mut self, gpu: &Gpu, faces: &[Image; 6], mips: bool) -> TextureId {
        let faces: Vec<&Image> = faces.iter().collect();
        self.add(gpu, &faces, mips, true)
    }

    /// Replaces the picture of a texture of the same size, as a Lock and Unlock would.
    pub fn replace(&mut self, gpu: &Gpu, id: TextureId, image: &Image) {
        write(gpu, &self.textures[id].texture, 0, image);
    }

    pub fn size_of(&self, id: TextureId) -> (u32, u32) {
        self.textures[id].size
    }

    /// A picture that can be drawn to and then used.
    pub fn render_target(&mut self, gpu: &Gpu, size: (u32, u32)) -> TextureId {
        let texture = gpu.target(size, COLOR, 1);
        let view = |t: wgpu::Texture| t.create_view(&Default::default());
        self.textures.push(Texture {
            view: view(texture.clone()),
            texture,
            cube: false,
            target: Some((
                view(gpu.target(size, COLOR, SAMPLES)),
                view(gpu.target(size, DEPTH, SAMPLES)),
            )),
            size,
        });
        self.textures.len() - 1
    }

    /// Vertices that stay until replaced.
    pub fn buffer(&mut self, gpu: &Gpu, vertices: &[Vertex]) -> BufferId {
        self.buffers.push(make_buffer(gpu, vertices));
        self.buffers.len() - 1
    }

    pub fn replace_buffer(&mut self, gpu: &Gpu, id: BufferId, vertices: &[Vertex]) {
        if self.buffers[id].1 as usize == vertices.len() {
            gpu.queue
                .write_buffer(&self.buffers[id].0, 0, bytemuck::cast_slice(vertices));
        } else {
            self.buffers[id] = make_buffer(gpu, vertices);
        }
    }

    // ----- states

    pub fn set_render_state(&mut self, state: u32, value: u32) {
        self.rs[state as usize] = value;
    }
    pub fn render_state(&self, state: u32) -> u32 {
        self.rs[state as usize]
    }
    pub fn set_texture_stage_state(&mut self, stage: usize, state: u32, value: u32) {
        if state == ADDRESS {
            self.tss[stage][ADDRESSU as usize] = value;
            self.tss[stage][ADDRESSV as usize] = value;
        }
        self.tss[stage][state as usize] = value;
    }
    pub fn texture_stage_state(&self, stage: usize, state: u32) -> u32 {
        self.tss[stage][state as usize]
    }
    pub fn set_texture(&mut self, stage: usize, t: Option<TextureId>) {
        self.bound[stage] = t.unwrap_or(0);
    }
    pub fn texture_at(&self, stage: usize) -> Option<TextureId> {
        Some(self.bound[stage]).filter(|t| *t != 0)
    }
    pub fn set_world(&mut self, m: Mat4) {
        self.world = m;
    }
    pub fn set_view(&mut self, m: Mat4) {
        self.view = m;
    }
    pub fn set_projection(&mut self, m: Mat4) {
        self.proj = m;
    }
    pub fn world(&self) -> Mat4 {
        self.world
    }
    pub fn view(&self) -> Mat4 {
        self.view
    }
    pub fn projection(&self) -> Mat4 {
        self.proj
    }
    /// IDirect3DDevice7::SetClipPlane: a, b, c, d of a world-space plane; points with
    /// a x + b y + c z + d below zero are clipped where D3DRS_CLIPPLANEENABLE has its bit.
    pub fn set_clip_plane(&mut self, i: usize, plane: [f32; 4]) {
        if let Some(p) = self.clip_planes.get_mut(i) {
            *p = plane;
        }
    }
    pub fn set_texture_transform(&mut self, stage: usize, m: Mat4) {
        self.texture_transforms[stage] = m;
    }
    pub fn set_light(&mut self, i: usize, l: Light) {
        self.lights[i] = l;
    }
    pub fn light(&self, i: usize) -> Light {
        self.lights[i]
    }
    pub fn light_enable(&mut self, i: usize, on: bool) {
        self.enabled[i] = on;
    }
    pub fn set_material(&mut self, m: Material) {
        self.material = m;
    }
    pub fn material(&self) -> Material {
        self.material
    }
    /// x, y, width, height, min z, max z.
    pub fn set_viewport(&mut self, v: [f32; 6]) {
        self.viewport = v;
    }
    pub fn viewport(&self) -> [f32; 6] {
        self.viewport
    }
    /// The window's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// IDirect3DVertexBuffer7::ProcessVertices with D3DVOP_TRANSFORM, and D3DVOP_LIGHT if
    /// `light`: screen positions with 1/w, and the colours the current lights, material and
    /// render states give. Texture coordinates pass through.
    pub fn process_vertices(&self, src: &[Vertex], fvf: u32, light: bool) -> Vec<Vertex> {
        let wv = self.view * self.world;
        let wvp = self.proj * wv;
        let n3 = Mat3::from_mat4(wv).inverse().transpose();
        let [vx, vy, vw, vh, z0, z1] = self.viewport;
        let rs = |s: u32| self.rs[s as usize];
        let lights: Vec<(Light, Vec3, Vec3)> = (0..8)
            .filter(|&i| self.enabled[i] && self.lights[i].kind != 0)
            .map(|i| {
                let l = self.lights[i];
                let p = (self.view * l.position.extend(1.0)).truncate();
                let d = (self.view * l.direction.extend(0.0))
                    .truncate()
                    .normalize_or_zero();
                (l, p, d)
            })
            .collect();
        let m = self.material;
        let colorvertex = rs(COLORVERTEX) != 0;
        let pick = |which: u32, mat: Vec4, v: &Vertex| -> Vec4 {
            if colorvertex && which == 1 && fvf & FVF_DIFFUSE != 0 {
                color(v.diffuse)
            } else if colorvertex && which == 2 && fvf & FVF_SPECULAR != 0 {
                color(v.specular)
            } else {
                mat
            }
        };
        let amb = color(rs(AMBIENT)).truncate();
        let specular_on = rs(SPECULARENABLE) != 0;
        let local = rs(LOCALVIEWER) != 0;
        src.iter()
            .map(|v| {
                let p = Vec3::from_slice(&v.pos[..3]);
                let c = wvp * p.extend(1.0);
                let rhw = 1.0 / c.w;
                let mut o = *v;
                o.pos = [
                    vx + (1.0 + c.x * rhw) * vw * 0.5,
                    vy + (1.0 - c.y * rhw) * vh * 0.5,
                    z0 + c.z * rhw * (z1 - z0),
                    rhw,
                ];
                if !light {
                    return o;
                }
                let ep = (wv * p.extend(1.0)).truncate();
                let mut n = n3 * Vec3::from(v.normal);
                if rs(NORMALIZENORMALS) != 0 {
                    n = n.normalize_or_zero();
                }
                let md = pick(rs(DIFFUSEMATERIALSOURCE), m.diffuse, v);
                let ms = pick(rs(SPECULARMATERIALSOURCE), m.specular, v);
                let ma = pick(rs(AMBIENTMATERIALSOURCE), m.ambient, v);
                let me = pick(rs(EMISSIVEMATERIALSOURCE), m.emissive, v);
                let eye = if local {
                    -ep.normalize_or_zero()
                } else {
                    Vec3::new(0.0, 0.0, -1.0)
                };
                let (mut acc_a, mut acc_d, mut acc_s) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
                for (l, lp, ld) in &lights {
                    let (dir, mut att) = if l.kind == LIGHT_DIRECTIONAL {
                        (-*ld, 1.0)
                    } else {
                        let to = *lp - ep;
                        let dist = to.length();
                        if dist > l.range {
                            continue;
                        }
                        let a = l.attenuation;
                        (
                            to / dist.max(1e-9),
                            1.0 / (a[0] + a[1] * dist + a[2] * dist * dist).max(1e-9),
                        )
                    };
                    if l.kind == LIGHT_SPOT {
                        let rho = (-dir).dot(*ld);
                        let (ct, cp) = ((l.theta / 2.0).cos(), (l.phi / 2.0).cos());
                        if rho <= cp {
                            att = 0.0;
                        } else if rho < ct {
                            att *= ((rho - cp) / (ct - cp).max(1e-6)).powf(l.falloff);
                        }
                    }
                    acc_a += l.ambient.truncate() * att;
                    let ndl = n.dot(dir);
                    if ndl > 0.0 {
                        acc_d += l.diffuse.truncate() * ndl * att;
                        if specular_on {
                            let h = (dir + eye).normalize_or_zero();
                            let nh = n.dot(h).max(0.0);
                            if nh > 0.0 {
                                acc_s += l.specular.truncate() * nh.powf(m.power) * att;
                            }
                        }
                    }
                }
                let d = me.truncate()
                    + ma.truncate() * amb
                    + ma.truncate() * acc_a
                    + md.truncate() * acc_d;
                let pack = |c: Vec3, a: f32| {
                    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
                    b(a) << 24 | b(c.x) << 16 | b(c.y) << 8 | b(c.z)
                };
                o.diffuse = pack(d, md.w);
                let spec_alpha = if fvf & FVF_SPECULAR != 0 {
                    color(v.specular).w
                } else {
                    1.0
                };
                o.specular = pack((ms.truncate() * acc_s).min(Vec3::ONE), spec_alpha);
                o
            })
            .collect()
    }

    // ----- frames

    /// Begins a frame on the window of `size`, with a full viewport.
    pub fn begin(&mut self, size: (u32, u32)) {
        self.size = size;
        self.vertices.clear();
        self.indices.clear();
        self.draws.clear();
        self.light_blocks.clear();
        self.prog_consts.clear();
        self.last_lights = None;
        self.passes.clear();
        self.target = None;
        self.viewport = [0.0, 0.0, size.0 as f32, size.1 as f32, 0.0, 1.0];
        self.new_pass();
    }

    fn new_pass(&mut self) {
        self.passes.push(Pass {
            target: self.target,
            clear_color: None,
            clear_depth: None,
            items: Vec::new(),
        });
    }

    /// Draws into a picture made with `render_target`, or the window, from now on.
    pub fn set_render_target(&mut self, t: Option<TextureId>) {
        self.target = t;
        let size = t.map_or(self.size, |t| self.textures[t].size);
        self.viewport = [0.0, 0.0, size.0 as f32, size.1 as f32, 0.0, 1.0];
        self.new_pass();
    }

    /// D3DCLEAR_TARGET is 1, _ZBUFFER 2, _STENCIL 4. Clears the whole target.
    pub fn clear(&mut self, flags: u32, c: u32, z: f32, stencil: u32) {
        let pass = self.passes.last().expect("frame begun");
        if !pass.items.is_empty() {
            self.new_pass();
        }
        let pass = self.passes.last_mut().expect("frame begun");
        if flags & 1 != 0 {
            let v = color(c);
            pass.clear_color = Some([v.x as f64, v.y as f64, v.z as f64, v.w as f64]);
        }
        if flags & 6 != 0 {
            let (oz, os) = pass.clear_depth.unwrap_or((1.0, 0));
            pass.clear_depth = Some((
                if flags & 2 != 0 { z } else { oz },
                if flags & 4 != 0 { stencil } else { os },
            ));
        }
    }

    /// DrawPrimitive and DrawIndexedPrimitive with the vertex contents `fvf`.
    pub fn draw(&mut self, prim: u32, fvf: u32, vertices: Source, indices: Option<&[u16]>) {
        let count = match (vertices, indices) {
            (_, Some(i)) => i.len(),
            (Source::Data(v), None) => v.len(),
            (Source::Buffer(b, start), None) => (self.buffers[b].1 - start) as usize,
        };
        self.draw_range(prim, fvf, vertices, indices, count);
    }

    /// As `draw`, but only the first `count` vertices or indices.
    pub fn draw_range(
        &mut self,
        prim: u32,
        fvf: u32,
        vertices: Source,
        indices: Option<&[u16]>,
        count: usize,
    ) {
        let (buffer, base) = match vertices {
            Source::Data(v) => {
                let base = self.vertices.len() as i32;
                self.vertices.extend_from_slice(v);
                (None, base)
            }
            Source::Buffer(b, start) => (Some(b), start as i32),
        };
        let b = base as u32;
        let list = |i: usize| b + indices.map_or(i as u32, |x| u32::from(x[i]));
        let start = self.indices.len() as u32;
        let (topology, n) = match prim {
            TRIANGLESTRIP => {
                for i in 2..count {
                    let (a, b) = if i % 2 == 0 {
                        (i - 2, i - 1)
                    } else {
                        (i - 1, i - 2)
                    };
                    self.indices.extend([list(a), list(b), list(i)]);
                }
                (TRIANGLELIST, 0)
            }
            TRIANGLEFAN => {
                for i in 2..count {
                    self.indices.extend([list(0), list(i - 1), list(i)]);
                }
                (TRIANGLELIST, 0)
            }
            LINESTRIP => {
                for i in 1..count {
                    self.indices.extend([list(i - 1), list(i)]);
                }
                (LINELIST, 0)
            }
            p => (p, count),
        };
        for i in 0..n {
            self.indices.push(list(i));
        }
        let end = self.indices.len() as u32;
        if end == start {
            return;
        }
        self.call(topology, fvf, buffer, 0, start..end);
    }

    fn call(
        &mut self,
        topology: u32,
        fvf: u32,
        vertices: Option<BufferId>,
        base: i32,
        indices: std::ops::Range<u32>,
    ) {
        let rs = |s: u32| self.rs[s as usize];
        let blend =
            (rs(ALPHABLENDENABLE) != 0).then(|| (rs(SRCBLEND), rs(DESTBLEND), rs(BLENDOP).max(1)));
        let (zfunc, zwrite) = match rs(ZENABLE) {
            0 => (8, false),
            _ => (rs(ZFUNC), rs(ZWRITEENABLE) != 0),
        };
        let stencil = (rs(STENCILENABLE) != 0).then(|| {
            (
                rs(STENCILFUNC),
                rs(STENCILFAIL),
                rs(STENCILZFAIL),
                rs(STENCILPASS),
                rs(STENCILMASK) & 0xff,
                rs(STENCILWRITEMASK) & 0xff,
            )
        });
        let pipeline = PipelineKey {
            blend,
            zfunc,
            zwrite,
            cull: rs(CULLMODE),
            topology,
            write: rs(COLORWRITEENABLE) & 15,
            stencil,
            format: match self.target {
                Some(_) => COLOR,
                None => self.format,
            },
            depth_bias: rs(ZBIAS) as i32,
        };
        let mut textures = [0; STAGES];
        let mut samplers = [SamplerKey {
            address: [1, 1],
            mag: 1,
            min: 1,
            mip: 0,
            aniso: 1,
        }; STAGES];
        let mut stages = [StageU::zeroed(); STAGES];
        let mut used = 0;
        for s in 0..STAGES {
            let t = &self.tss[s];
            if t[COLOROP as usize] == DISABLE {
                break;
            }
            used = s + 1;
            let bound = self.bound[s];
            let cube = bound != 0 && self.textures[bound].cube;
            let kind = match (bound, cube) {
                (0, _) => 0,
                (_, true) if s < CUBES => 2,
                (_, true) => 0,
                _ => 1,
            };
            textures[s] = if kind == 1 { bound } else { self.white };
            samplers[s] = SamplerKey {
                address: [t[ADDRESSU as usize], t[ADDRESSV as usize]],
                mag: t[MAGFILTER as usize],
                min: t[MINFILTER as usize],
                mip: t[MIPFILTER as usize],
                aniso: t[MAXANISOTROPY as usize],
            };
            let f = |i: u32| f32::from_bits(t[i as usize]);
            stages[s] = StageU {
                op: [
                    t[COLOROP as usize],
                    t[COLORARG1 as usize],
                    t[COLORARG2 as usize],
                    t[COLORARG0 as usize],
                ],
                aop: [
                    t[ALPHAOP as usize],
                    t[ALPHAARG1 as usize],
                    t[ALPHAARG2 as usize],
                    t[ALPHAARG0 as usize],
                ],
                misc: [
                    t[TEXCOORDINDEX as usize],
                    t[TEXTURETRANSFORMFLAGS as usize],
                    t[RESULTARG as usize],
                    kind,
                ],
                bump: [
                    f(BUMPENVMAT00),
                    f(BUMPENVMAT01),
                    f(BUMPENVMAT10),
                    f(BUMPENVMAT11),
                ],
                bump2: [f(BUMPENVLSCALE), f(BUMPENVLOFFSET), 0.0, 0.0],
                tm: self.texture_transforms[s].to_cols_array(),
            };
        }
        let mut cubes = [self.white_cube; CUBES];
        for (s, c) in cubes.iter_mut().enumerate() {
            if stages[s].misc[3] == 2 {
                *c = self.bound[s];
            }
        }
        let wv = self.view * self.world;
        let n3 = Mat3::from_mat4(wv).inverse().transpose();
        let fl = |s: u32| f32::from_bits(self.rs[s as usize]);
        let m = self.material;
        let draw = DrawU {
            world: self.world.to_cols_array(),
            view: self.view.to_cols_array(),
            proj: self.proj.to_cols_array(),
            normal: Mat4::from_mat3(n3).to_cols_array(),
            flags: [
                fvf,
                rs(LIGHTING)
                    | (rs(COLORVERTEX) << 1)
                    | (rs(SPECULARENABLE).min(1) << 2)
                    | (rs(NORMALIZENORMALS).min(1) << 3)
                    | (rs(LOCALVIEWER).min(1) << 4),
                used as u32,
                rs(SHADEMODE),
            ],
            sources: [
                rs(DIFFUSEMATERIALSOURCE),
                rs(SPECULARMATERIALSOURCE),
                rs(AMBIENTMATERIALSOURCE),
                rs(EMISSIVEMATERIALSOURCE),
            ],
            fog: [fl(FOGSTART), fl(FOGEND), fl(FOGDENSITY), 0.0],
            fog_color: color(rs(FOGCOLOR)).to_array(),
            fog_mode: [
                rs(FOGTABLEMODE),
                rs(FOGVERTEXMODE),
                rs(FOGENABLE),
                rs(RANGEFOGENABLE),
            ],
            ambient: color(rs(AMBIENT)).to_array(),
            tfactor: color(rs(TEXTUREFACTOR)).to_array(),
            alpha: [rs(ALPHATESTENABLE), rs(ALPHAFUNC), rs(ALPHAREF) & 0xff, 0],
            viewport: [
                self.viewport[0],
                self.viewport[1],
                self.viewport[2],
                self.viewport[3],
            ],
            m_diffuse: m.diffuse.to_array(),
            m_ambient: m.ambient.to_array(),
            m_specular: m.specular.to_array(),
            m_emissive: m.emissive.to_array(),
            m_power: [m.power, 0.0, 0.0, 0.0],
            stages,
            clip_mask: [rs(CLIPPLANEENABLE), 0, 0, 0],
            clip: self.clip_planes,
        };
        let draw_at = self.draws.len() as u32;
        self.draws.extend_from_slice(bytemuck::bytes_of(&draw));
        self.draws
            .resize(self.draws.len().next_multiple_of(SLOT), 0);
        let mut lights = LightsU::zeroed();
        for (i, l) in self.lights.iter().enumerate() {
            let v = self.view;
            let pos = v * l.position.extend(1.0);
            let dir = (v * l.direction.extend(0.0)).truncate().normalize_or_zero();
            lights.lights[i] = LightU {
                kind: [if self.enabled[i] { l.kind } else { 0 }, 0, 0, 0],
                diffuse: l.diffuse.to_array(),
                specular: l.specular.to_array(),
                ambient: l.ambient.to_array(),
                position: [pos.x, pos.y, pos.z, l.range],
                direction: [dir.x, dir.y, dir.z, l.falloff],
                attenuation: [l.attenuation[0], l.attenuation[1], l.attenuation[2], 0.0],
                spot: [(l.theta / 2.0).cos(), (l.phi / 2.0).cos(), 0.0, 0.0],
            };
        }
        let lights_at = match self.last_lights {
            Some((last, at)) if bytemuck::bytes_of(&last) == bytemuck::bytes_of(&lights) => at,
            _ => {
                let at = self.light_blocks.len() as u32;
                self.light_blocks
                    .extend_from_slice(bytemuck::bytes_of(&lights));
                self.light_blocks
                    .resize(self.light_blocks.len().next_multiple_of(SLOT), 0);
                self.last_lights = Some((lights, at));
                at
            }
        };
        let call = Call {
            pipeline,
            textures,
            cubes,
            samplers,
            draw: draw_at,
            lights: lights_at,
            stencil_ref: rs(STENCILREF) & 0xff,
            viewport: self.viewport,
            vertices,
            base,
            indices,
        };
        self.passes
            .last_mut()
            .expect("frame begun")
            .items
            .push(Item::Draw(call));
    }

    fn pipeline(&mut self, gpu: &Gpu, k: PipelineKey) {
        if self.pipelines.contains_key(&k) {
            return;
        }
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x3,
            2 => Unorm8x4, 3 => Unorm8x4, 4 => Float32x2, 5 => Float32x2, 6 => Float32x2,
            7 => Float32x2, 8 => Float32x2, 9 => Float32x2, 10 => Float32x2, 11 => Float32x2];
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let (blend, depth, primitive) = self.pipeline_parts(k);
        let made = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive,
                depth_stencil: Some(depth),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: k.format,
                        blend,
                        write_mask: wgpu::ColorWrites::from_bits_truncate(k.write),
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        self.pipelines.insert(k, made);
    }

    /// Blending, depth and stencil, and primitive states of a pipeline key.
    fn pipeline_parts(
        &self,
        k: PipelineKey,
    ) -> (
        Option<wgpu::BlendState>,
        wgpu::DepthStencilState,
        wgpu::PrimitiveState,
    ) {
        let cmp = |f: u32| match f {
            1 => wgpu::CompareFunction::Never,
            2 => wgpu::CompareFunction::Less,
            3 => wgpu::CompareFunction::Equal,
            4 => wgpu::CompareFunction::LessEqual,
            5 => wgpu::CompareFunction::Greater,
            6 => wgpu::CompareFunction::NotEqual,
            7 => wgpu::CompareFunction::GreaterEqual,
            _ => wgpu::CompareFunction::Always,
        };
        let op = |o: u32| match o {
            2 => wgpu::StencilOperation::Zero,
            3 => wgpu::StencilOperation::Replace,
            4 => wgpu::StencilOperation::IncrementClamp,
            5 => wgpu::StencilOperation::DecrementClamp,
            6 => wgpu::StencilOperation::Invert,
            7 => wgpu::StencilOperation::IncrementWrap,
            8 => wgpu::StencilOperation::DecrementWrap,
            _ => wgpu::StencilOperation::Keep,
        };
        let stencil = match k.stencil {
            None => wgpu::StencilState::default(),
            Some((func, fail, zfail, pass, read, write)) => {
                let face = wgpu::StencilFaceState {
                    compare: cmp(func),
                    fail_op: op(fail),
                    depth_fail_op: op(zfail),
                    pass_op: op(pass),
                };
                wgpu::StencilState {
                    front: face,
                    back: face,
                    read_mask: read,
                    write_mask: write,
                }
            }
        };
        let factor = |b: u32| match b {
            1 => wgpu::BlendFactor::Zero,
            3 => wgpu::BlendFactor::Src,
            4 => wgpu::BlendFactor::OneMinusSrc,
            5 | 12 => wgpu::BlendFactor::SrcAlpha,
            6 | 13 => wgpu::BlendFactor::OneMinusSrcAlpha,
            7 => wgpu::BlendFactor::DstAlpha,
            8 => wgpu::BlendFactor::OneMinusDstAlpha,
            9 => wgpu::BlendFactor::Dst,
            10 => wgpu::BlendFactor::OneMinusDst,
            11 => wgpu::BlendFactor::SrcAlphaSaturated,
            _ => wgpu::BlendFactor::One,
        };
        let blend = k.blend.map(|(s, d, o)| {
            let (src, dst) = match s {
                12 => (
                    wgpu::BlendFactor::SrcAlpha,
                    wgpu::BlendFactor::OneMinusSrcAlpha,
                ),
                13 => (
                    wgpu::BlendFactor::OneMinusSrcAlpha,
                    wgpu::BlendFactor::SrcAlpha,
                ),
                _ => (factor(s), factor(d)),
            };
            let operation = match o {
                2 => wgpu::BlendOperation::Subtract,
                3 => wgpu::BlendOperation::ReverseSubtract,
                4 => wgpu::BlendOperation::Min,
                5 => wgpu::BlendOperation::Max,
                _ => wgpu::BlendOperation::Add,
            };
            let c = wgpu::BlendComponent {
                src_factor: src,
                dst_factor: dst,
                operation,
            };
            wgpu::BlendState { color: c, alpha: c }
        });
        let primitive = wgpu::PrimitiveState {
            topology: match k.topology {
                POINTLIST => wgpu::PrimitiveTopology::PointList,
                LINELIST => wgpu::PrimitiveTopology::LineList,
                _ => wgpu::PrimitiveTopology::TriangleList,
            },
            // Direct3D's front faces turn clockwise on the screen.
            front_face: wgpu::FrontFace::Cw,
            cull_mode: match k.cull {
                2 => Some(wgpu::Face::Front),
                3 => Some(wgpu::Face::Back),
                _ => None,
            },
            ..Default::default()
        };
        let depth = wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(k.zwrite),
            depth_compare: Some(cmp(k.zfunc)),
            stencil,
            bias: wgpu::DepthBiasState {
                constant: -k.depth_bias * 2,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        };
        (blend, depth, primitive)
    }

    fn sampler(&mut self, gpu: &Gpu, k: SamplerKey) {
        if self.samplers.contains_key(&k) {
            return;
        }
        let address = k.address.map(|a| match a {
            2 => wgpu::AddressMode::MirrorRepeat,
            3..=5 => wgpu::AddressMode::ClampToEdge,
            _ => wgpu::AddressMode::Repeat,
        });
        let v8 = self.version >= 8;
        let linear = |f: u32, mip: bool| match (v8, mip) {
            (true, _) => f >= 2,
            (false, false) => f >= 2,
            (false, true) => f == 3,
        };
        let filter = |l: bool| match l {
            true => wgpu::FilterMode::Linear,
            false => wgpu::FilterMode::Nearest,
        };
        let no_mips = match v8 {
            true => k.mip == 0,
            false => k.mip <= 1,
        };
        let aniso = if k.min == 3 {
            k.aniso.clamp(1, 16) as u16
        } else {
            1
        };
        let mag = filter(linear(k.mag, false) || aniso > 1);
        let min = filter(linear(k.min, false) || aniso > 1);
        let mip = match linear(k.mip, true) {
            true => wgpu::MipmapFilterMode::Linear,
            false => wgpu::MipmapFilterMode::Nearest,
        };
        let aniso = if mag == wgpu::FilterMode::Linear
            && min == wgpu::FilterMode::Linear
            && mip == wgpu::MipmapFilterMode::Linear
        {
            aniso
        } else {
            1
        };
        let s = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: address[0],
            address_mode_v: address[1],
            address_mode_w: address[1],
            mag_filter: mag,
            min_filter: min,
            mipmap_filter: mip,
            lod_max_clamp: if no_mips { 0.0 } else { 32.0 },
            anisotropy_clamp: aniso,
            ..Default::default()
        });
        self.samplers.insert(k, s);
    }
}

impl Device {
    fn group(&mut self, gpu: &Gpu, key: GroupKey) {
        if self.groups.contains_key(&key) {
            return;
        }
        for s in key.2 {
            self.sampler(gpu, s);
        }
        let (textures, cubes, samplers) = key;
        let mut entries = Vec::new();
        for i in 0..STAGES {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 2,
                resource: wgpu::BindingResource::TextureView(&self.textures[textures[i]].view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 2 + 1,
                resource: wgpu::BindingResource::Sampler(&self.samplers[&samplers[i]]),
            });
        }
        for (i, c) in cubes.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 32 + i as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[*c].view),
            });
        }
        let made = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_layout,
            entries: &entries,
        });
        self.groups.insert(key, made);
    }

    /// Compiles what a frame needs without drawing, so the first frames do not stall.
    pub fn prepare(&mut self, gpu: &Gpu) {
        let mut keys = Vec::new();
        for p in &self.passes {
            for item in &p.items {
                if let Item::Draw(c) = item {
                    keys.push((c.pipeline, (c.textures, c.cubes, c.samplers)));
                }
            }
        }
        for (pipeline, group) in keys {
            self.pipeline(gpu, pipeline);
            self.group(gpu, group);
        }
        let mut pkeys = Vec::new();
        for p in &self.passes {
            for item in &p.items {
                if let Item::Program(c) = item {
                    pkeys.push((c.program, c.pipeline, c.textures, c.volumes, c.samplers));
                }
            }
        }
        for (program, key, textures, volumes, samplers) in pkeys {
            self.prog_pipeline(gpu, program, key);
            self.prog_group(gpu, textures, volumes, samplers);
        }
    }

    /// Draws everything handed over since `begin` into `view` of the window's size.
    pub fn finish(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let d = &gpu.device;
        let size = self.size;
        if self.window.as_ref().is_none_or(|w| w.0 != size) {
            let v = |t: wgpu::Texture| t.create_view(&Default::default());
            self.window = Some((
                size,
                v(gpu.target(size, self.format, SAMPLES)),
                v(gpu.target(size, DEPTH, SAMPLES)),
            ));
        }
        self.prepare(gpu);
        self.prog_upload(gpu);
        let grow =
            |b: &Option<wgpu::Buffer>, n: usize| b.as_ref().is_some_and(|b| b.size() >= n as u64);
        let vbytes: &[u8] = bytemuck::cast_slice(&self.vertices);
        if !grow(&self.vertex_buffer, vbytes.len()) {
            self.vertex_buffer = Some(d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame vertices"),
                size: (vbytes.len().max(1 << 16) as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let ibytes: &[u8] = bytemuck::cast_slice(&self.indices);
        if !grow(&self.index_buffer, ibytes.len()) {
            self.index_buffer = Some(d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame indices"),
                size: (ibytes.len().max(1 << 16) as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let vb = self.vertex_buffer.as_ref().expect("made above");
        let ib = self.index_buffer.as_ref().expect("made above");
        gpu.queue.write_buffer(vb, 0, vbytes);
        gpu.queue.write_buffer(ib, 0, ibytes);
        let fits = self.uniform.as_ref().is_some_and(|u| {
            u.0.size() >= self.draws.len() as u64 && u.1.size() >= self.light_blocks.len() as u64
        });
        if !fits {
            let make = |n: usize| {
                d.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("uniforms"),
                    size: (n.max(1 << 16) as u64).next_power_of_two(),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            let draws = make(self.draws.len() * 2);
            let lights = make(self.light_blocks.len() * 2);
            fn entry(binding: u32, buffer: &wgpu::Buffer, size: usize) -> wgpu::BindGroupEntry<'_> {
                wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size as u64),
                    }),
                }
            }
            let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.uniform_layout,
                entries: &[
                    entry(0, &draws, size_of::<DrawU>()),
                    entry(1, &lights, size_of::<LightsU>()),
                ],
            });
            self.uniform = Some((draws, lights, group));
        }
        let (dbuf, lbuf, uniforms) = self.uniform.as_ref().expect("made above");
        gpu.queue.write_buffer(dbuf, 0, &self.draws);
        gpu.queue.write_buffer(lbuf, 0, &self.light_blocks);
        let (_, wsamples, wdepth) = self.window.as_ref().expect("made above");
        for p in &self.passes {
            if p.items.is_empty() && p.clear_color.is_none() && p.clear_depth.is_none() {
                continue;
            }
            let (samples, depth, resolved) = match p.target {
                None => (wsamples, wdepth, view),
                Some(t) => {
                    let t = &self.textures[t];
                    let (s, z) = t.target.as_ref().expect("made to be drawn to");
                    (s, z, &t.view)
                }
            };
            let store = wgpu::StoreOp::Store;
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: samples,
                    depth_slice: None,
                    resolve_target: Some(resolved),
                    ops: wgpu::Operations {
                        load: p.clear_color.map_or(wgpu::LoadOp::Load, |[r, g, b, a]| {
                            wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a })
                        }),
                        store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: p
                            .clear_depth
                            .map_or(wgpu::LoadOp::Load, |(z, _)| wgpu::LoadOp::Clear(z)),
                        store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: p
                            .clear_depth
                            .map_or(wgpu::LoadOp::Load, |(_, s)| wgpu::LoadOp::Clear(s)),
                        store,
                    }),
                }),
                ..Default::default()
            });
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            let tsize = p.target.map_or(size, |t| self.textures[t].size);
            for item in &p.items {
                let c = match item {
                    Item::Draw(c) => c,
                    Item::Program(c) => {
                        let Some((pbuf, pgroup)) = &self.prog_uniform else {
                            continue;
                        };
                        let _ = pbuf;
                        let [x, y, w, h, z0, z1] = c.viewport;
                        let (w, h) = (w.min(tsize.0 as f32 - x), h.min(tsize.1 as f32 - y));
                        if w <= 0.0 || h <= 0.0 {
                            continue;
                        }
                        pass.set_viewport(x, y, w, h, z0, z1);
                        pass.set_pipeline(&self.prog_pipelines[&(c.program, c.pipeline)]);
                        pass.set_bind_group(0, pgroup, &[c.consts]);
                        pass.set_bind_group(
                            1,
                            &self.prog_groups[&(c.textures, c.volumes, c.samplers)],
                            &[],
                        );
                        pass.set_vertex_buffer(0, self.pbuffers[c.vertices].0.slice(..));
                        pass.set_index_buffer(
                            self.pindices[c.index].0.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.set_stencil_reference(c.stencil_ref);
                        pass.draw_indexed(c.range.clone(), 0, 0..1);
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        continue;
                    }
                };
                let vertices = c.vertices.map_or(vb, |b| &self.buffers[b].0);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_pipeline(&self.pipelines[&c.pipeline]);
                pass.set_bind_group(0, uniforms, &[c.draw, c.lights]);
                pass.set_bind_group(1, &self.groups[&(c.textures, c.cubes, c.samplers)], &[]);
                let [x, y, w, h, z0, z1] = c.viewport;
                let (w, h) = (w.min(tsize.0 as f32 - x), h.min(tsize.1 as f32 - y));
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                pass.set_viewport(x, y, w, h, z0, z1);
                pass.set_stencil_reference(c.stencil_ref);
                pass.draw_indexed(c.indices.clone(), c.base, 0..1);
            }
        }
    }
}

fn make_buffer(gpu: &Gpu, vertices: &[Vertex]) -> (wgpu::Buffer, u32) {
    let buffer = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: match vertices {
                [] => &[0; size_of::<Vertex>()],
                v => bytemuck::cast_slice(v),
            },
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
    (buffer, vertices.len() as u32)
}

fn write(gpu: &Gpu, texture: &wgpu::Texture, layer: u32, image: &Image) {
    let mut image = image.clone();
    for level in 0..texture.mip_level_count() {
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &image.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: None,
            },
            extent(image.width, image.height, 1),
        );
        image = image.half();
    }
}

// ---------- programmable shaders (vs.1.1 / ps.1.x translated to WGSL)

/// A vertex for programs: the shader's input registers v0, v1, v2, v3, v5, v6, v7, v8,
/// v14 and v15, then v9 and v10, in that order.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct PVertex {
    pub r: [[f32; 4]; 12],
}

/// What a program draw holds besides its states: 96 vertex shader constants, 8 pixel shader
/// constants, each stage's bump matrix and luminance scale and offset, and alpha test.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ProgConsts {
    pub c: [[f32; 4]; 96],
    pub pc: [[f32; 4]; 8],
    pub bump: [[f32; 4]; 4],
    pub bumpl: [[f32; 4]; 4],
    /// Alpha test on, function, reference (0 to 1).
    pub alpha: [f32; 4],
    pub misc: [f32; 4],
}

impl Default for ProgConsts {
    fn default() -> Self {
        Self::zeroed()
    }
}

const PROG_SLOT: usize = 2048;

type ProgGroupKey = ([TextureId; 4], [TextureId; 4], [SamplerKey; 4]);

struct ProgCall {
    program: usize,
    pipeline: PipelineKey,
    textures: [TextureId; 4],
    volumes: [TextureId; 4],
    samplers: [SamplerKey; 4],
    consts: u32,
    stencil_ref: u32,
    viewport: [f32; 6],
    vertices: usize,
    index: usize,
    range: std::ops::Range<u32>,
}

impl Device {
    /// A volume texture from its slices, front first, without mipmaps.
    pub fn volume(&mut self, gpu: &Gpu, slices: &[Image]) -> TextureId {
        let first = &slices[0];
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("volume"),
            size: extent(first.width, first.height, slices.len() as u32),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: COLOR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (z, image) in slices.iter().enumerate() {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: z as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &image.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width * 4),
                    rows_per_image: None,
                },
                extent(image.width, image.height, 1),
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D3),
            ..Default::default()
        });
        self.textures.push(Texture {
            texture,
            view,
            cube: false,
            target: None,
            size: (first.width, first.height),
        });
        self.textures.len() - 1
    }

    /// Program vertices that stay.
    pub fn program_vertices(&mut self, gpu: &Gpu, v: &[PVertex]) -> usize {
        let b = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("program vertices"),
                contents: if v.is_empty() {
                    &[0; size_of::<PVertex>()]
                } else {
                    bytemuck::cast_slice(v)
                },
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.pbuffers.push((b, v.len() as u32));
        self.pbuffers.len() - 1
    }

    /// Indices that stay.
    pub fn program_indices(&mut self, gpu: &Gpu, i: &[u32]) -> usize {
        let b = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("program indices"),
                contents: if i.is_empty() {
                    &[0; 4]
                } else {
                    bytemuck::cast_slice(i)
                },
                usage: wgpu::BufferUsages::INDEX,
            });
        self.pindices.push((b, i.len() as u32));
        self.pindices.len() - 1
    }

    fn prog_layouts(&mut self, gpu: &Gpu) {
        if self.prog_layout.is_some() {
            return;
        }
        let d = &gpu.device;
        let uniform = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<ProgConsts>() as u64),
                },
                count: None,
            }],
        });
        let mut entries = Vec::new();
        let frag = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        for i in 0..4u32 {
            entries.push(frag(
                i * 3,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ));
            entries.push(frag(
                i * 3 + 1,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
            ));
            entries.push(frag(
                i * 3 + 2,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ));
        }
        let textures = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&uniform), Some(&textures)],
            immediate_size: 0,
        });
        self.prog_layout = Some((layout, uniform, textures));
    }

    /// Compiles a program: WGSL with entry points `vs` and `fs` (see `PROGRAM_PRELUDE`).
    pub fn program(&mut self, gpu: &Gpu, wgsl: &str) -> usize {
        self.prog_layouts(gpu);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("program"),
                source: wgpu::ShaderSource::Wgsl(format!("{PROGRAM_PRELUDE}\n{wgsl}").into()),
            });
        self.programs.push(module);
        self.programs.len() - 1
    }

    /// Draws `range` of an index buffer with a program, the current render states, the
    /// textures and filters of stages 0 to 3 (volumes where `volumes` says) and `consts`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_program(
        &mut self,
        program: usize,
        vertices: usize,
        index: usize,
        range: std::ops::Range<u32>,
        volumes: [bool; 4],
        consts: &ProgConsts,
    ) {
        let rs = |s: u32| self.rs[s as usize];
        let blend =
            (rs(ALPHABLENDENABLE) != 0).then(|| (rs(SRCBLEND), rs(DESTBLEND), rs(BLENDOP).max(1)));
        let (zfunc, zwrite) = match rs(ZENABLE) {
            0 => (8, false),
            _ => (rs(ZFUNC), rs(ZWRITEENABLE) != 0),
        };
        let stencil = (rs(STENCILENABLE) != 0).then(|| {
            (
                rs(STENCILFUNC),
                rs(STENCILFAIL),
                rs(STENCILZFAIL),
                rs(STENCILPASS),
                rs(STENCILMASK) & 0xff,
                rs(STENCILWRITEMASK) & 0xff,
            )
        });
        let pipeline = PipelineKey {
            blend,
            zfunc,
            zwrite,
            cull: rs(CULLMODE),
            topology: TRIANGLELIST,
            write: rs(COLORWRITEENABLE) & 15,
            stencil,
            format: match self.target {
                Some(_) => COLOR,
                None => self.format,
            },
            depth_bias: rs(ZBIAS) as i32,
        };
        let mut textures = [self.white; 4];
        let mut vols = [self.white_volume; 4];
        let mut samplers = [SamplerKey {
            address: [1, 1],
            mag: 1,
            min: 1,
            mip: 0,
            aniso: 1,
        }; 4];
        for s in 0..4 {
            let t = &self.tss[s];
            let bound = self.bound[s];
            if volumes[s] {
                if bound != 0 {
                    vols[s] = bound;
                }
            } else if bound != 0 && !self.textures[bound].cube {
                textures[s] = bound;
            }
            samplers[s] = SamplerKey {
                address: [t[ADDRESSU as usize], t[ADDRESSV as usize]],
                mag: t[MAGFILTER as usize],
                min: t[MINFILTER as usize],
                mip: t[MIPFILTER as usize],
                aniso: t[MAXANISOTROPY as usize],
            };
        }
        let at = self.prog_consts.len() as u32;
        self.prog_consts
            .extend_from_slice(bytemuck::bytes_of(consts));
        self.prog_consts
            .resize(self.prog_consts.len().next_multiple_of(PROG_SLOT), 0);
        let call = ProgCall {
            program,
            pipeline,
            textures,
            volumes: vols,
            samplers,
            consts: at,
            stencil_ref: rs(STENCILREF) & 0xff,
            viewport: self.viewport,
            vertices,
            index,
            range,
        };
        self.passes
            .last_mut()
            .expect("frame begun")
            .items
            .push(Item::Program(call));
    }

    fn prog_pipeline(&mut self, gpu: &Gpu, program: usize, k: PipelineKey) {
        if self.prog_pipelines.contains_key(&(program, k)) {
            return;
        }
        let (layout, _, _) = self.prog_layout.as_ref().expect("made with the program");
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4,
            3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
            8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4];
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<PVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let (blend, depth, primitive) = self.pipeline_parts(k);
        let made = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("program"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &self.programs[program],
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                primitive,
                depth_stencil: Some(depth),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.programs[program],
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: k.format,
                        blend,
                        write_mask: wgpu::ColorWrites::from_bits_truncate(k.write),
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        self.prog_pipelines.insert((program, k), made);
    }

    fn prog_group(
        &mut self,
        gpu: &Gpu,
        textures: [TextureId; 4],
        volumes: [TextureId; 4],
        samplers: [SamplerKey; 4],
    ) {
        let key = (textures, volumes, samplers);
        if self.prog_groups.contains_key(&key) {
            return;
        }
        for s in samplers {
            self.sampler(gpu, s);
        }
        let (_, _, layout) = self.prog_layout.as_ref().expect("made with the program");
        let mut entries = Vec::new();
        for i in 0..4 {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 3,
                resource: wgpu::BindingResource::TextureView(&self.textures[textures[i]].view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 3 + 1,
                resource: wgpu::BindingResource::TextureView(&self.textures[volumes[i]].view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 3 + 2,
                resource: wgpu::BindingResource::Sampler(&self.samplers[&samplers[i]]),
            });
        }
        let made = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &entries,
        });
        self.prog_groups.insert(key, made);
    }

    fn prog_upload(&mut self, gpu: &Gpu) {
        if self.prog_consts.is_empty() {
            return;
        }
        let fits = self
            .prog_uniform
            .as_ref()
            .is_some_and(|u| u.0.size() >= self.prog_consts.len() as u64);
        if !fits {
            let (_, layout, _) = self.prog_layout.as_ref().expect("made with the program");
            let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("program constants"),
                size: ((self.prog_consts.len() * 2).max(1 << 16) as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<ProgConsts>() as u64),
                    }),
                }],
            });
            self.prog_uniform = Some((buffer, group));
        }
        let (buffer, _) = self.prog_uniform.as_ref().expect("made above");
        gpu.queue.write_buffer(buffer, 0, &self.prog_consts);
    }
}

/// Declarations every program has: constants, four stages of 2D and volume textures.
pub const PROGRAM_PRELUDE: &str = r#"
struct P {
    c: array<vec4<f32>, 96>,
    pc: array<vec4<f32>, 8>,
    bump: array<vec4<f32>, 4>,
    bumpl: array<vec4<f32>, 4>,
    alpha: vec4<f32>,
    misc: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: P;
@group(1) @binding(0) var t2d0: texture_2d<f32>;
@group(1) @binding(1) var t3d0: texture_3d<f32>;
@group(1) @binding(2) var smp0: sampler;
@group(1) @binding(3) var t2d1: texture_2d<f32>;
@group(1) @binding(4) var t3d1: texture_3d<f32>;
@group(1) @binding(5) var smp1: sampler;
@group(1) @binding(6) var t2d2: texture_2d<f32>;
@group(1) @binding(7) var t3d2: texture_3d<f32>;
@group(1) @binding(8) var smp2: sampler;
@group(1) @binding(9) var t2d3: texture_2d<f32>;
@group(1) @binding(10) var t3d3: texture_3d<f32>;
@group(1) @binding(11) var smp3: sampler;

fn rcp_(x: f32) -> f32 {
    if x == 0.0 { return 3.4e38; }
    return 1.0 / x;
}
fn rsq_(x: f32) -> f32 {
    let a = abs(x);
    if a == 0.0 { return 3.4e38; }
    return inverseSqrt(a);
}
fn lit_(s: vec4<f32>) -> vec4<f32> {
    var r = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    if s.x > 0.0 {
        r.y = s.x;
        if s.y > 0.0 {
            r.z = pow(s.y, clamp(s.w, -128.0, 128.0));
        }
    }
    return r;
}
fn passes_alpha(a: f32) -> bool {
    if u.alpha.x == 0.0 { return true; }
    let f = u32(u.alpha.y);
    let r = u.alpha.z;
    switch f {
        case 1u: { return false; }
        case 2u: { return a < r; }
        case 3u: { return a == r; }
        case 4u: { return a <= r; }
        case 5u: { return a > r; }
        case 6u: { return a != r; }
        case 7u: { return a >= r; }
        default: { return true; }
    }
}
struct In {
    @location(0) v0: vec4<f32>,
    @location(1) v1: vec4<f32>,
    @location(2) v2: vec4<f32>,
    @location(3) v3: vec4<f32>,
    @location(4) v5: vec4<f32>,
    @location(5) v6: vec4<f32>,
    @location(6) v7: vec4<f32>,
    @location(7) v8: vec4<f32>,
    @location(8) v14: vec4<f32>,
    @location(9) v15: vec4<f32>,
    @location(10) v9: vec4<f32>,
    @location(11) v10: vec4<f32>,
};
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) d0: vec4<f32>,
    @location(1) d1: vec4<f32>,
    @location(2) t0: vec4<f32>,
    @location(3) t1: vec4<f32>,
    @location(4) t2: vec4<f32>,
    @location(5) t3: vec4<f32>,
};
"#;
