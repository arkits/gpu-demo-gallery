//! A small immediate-mode renderer: the demo hands over triangles with their state, as
//! it would have to OpenGL or Glide, and they are drawn together at the end of the frame.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::Error;
use crate::image::Image;

/// The originals work in gamma space, so nothing is an sRGB format.
pub const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
pub const SAMPLES: u32 = 4;
/// Vectors the programs get with each draw.
pub const PARAMS: usize = 32;
const SLOT: u64 = 512;

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

/// OpenGL's depth runs from -1 to 1, the GPU's here from 0 to 1.
pub fn from_opengl(projection: Mat4) -> Mat4 {
    Mat4::from_cols_array_2d(&[
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.5, 0.0],
        [0.0, 0.0, 0.5, 1.0],
    ]) * projection
}

/// `gluPerspective`, with a vertical angle in degrees.
pub fn perspective(degrees: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (degrees.to_radians() / 2.0).tan();
    let d = near - far;
    Mat4::from_cols_array_2d(&[
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (far + near) / d, -1.0],
        [0.0, 0.0, 2.0 * far * near / d, 0.0],
    ])
}

/// `gluLookAt`.
pub fn look_at(eye: Vec3, at: Vec3, up: Vec3) -> Mat4 {
    let back = (eye - at).normalize();
    let right = up.cross(back).normalize();
    let turn = Mat3::from_cols(right, back.cross(right), back).transpose();
    Mat4::from_mat3(turn) * Mat4::from_translation(-eye)
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// Clip space, unless the vertex program says otherwise.
    pub position: [f32; 4],
    pub color: [f32; 4],
    pub uv: [[f32; 4]; 4],
}

impl Vertex {
    pub fn new(position: Vec4, color: [f32; 4]) -> Self {
        Self {
            position: position.to_array(),
            color,
            uv: [[0.0, 0.0, 0.0, 1.0]; 4],
        }
    }
    pub fn uv(mut self, set: usize, u: f32, v: f32) -> Self {
        self.uv[set] = [u, v, 0.0, 1.0];
        self
    }
    pub fn uvw(mut self, set: usize, c: [f32; 4]) -> Self {
        self.uv[set] = c;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Depth {
    Off,
    /// Writes whatever is there.
    Always,
    /// Compares, and writes if asked.
    Less(bool),
    LessEqual(bool),
    Equal,
}

/// How triangles are drawn. Equal states share a pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct State {
    /// Name of the vertex program.
    pub vertex: &'static str,
    /// Index into the fragment programs.
    pub program: usize,
    pub blend: Option<(wgpu::BlendFactor, wgpu::BlendFactor)>,
    pub depth: Depth,
    pub cull: Option<wgpu::Face>,
    pub lines: bool,
    pub write: wgpu::ColorWrites,
}

impl State {
    pub const fn new(program: usize) -> Self {
        Self {
            vertex: "vs",
            program,
            blend: None,
            depth: Depth::Off,
            cull: None,
            lines: false,
            write: wgpu::ColorWrites::ALL,
        }
    }
    pub const fn vertex(mut self, name: &'static str) -> Self {
        self.vertex = name;
        self
    }
    pub const fn blend(mut self, src: wgpu::BlendFactor, dst: wgpu::BlendFactor) -> Self {
        self.blend = Some((src, dst));
        self
    }
    pub const fn depth(mut self, depth: Depth) -> Self {
        self.depth = depth;
        self
    }
    pub const fn cull(mut self, face: wgpu::Face) -> Self {
        self.cull = Some(face);
        self
    }
    pub const fn lines(mut self) -> Self {
        self.lines = true;
        self
    }
}

pub type TextureId = usize;
pub type MeshId = usize;
/// Stands in for textures a program does not use.
pub const WHITE: TextureId = 0;
const WHITE_CUBE: TextureId = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sampling {
    /// Beyond the edge in u and v: the edge's color instead of the picture again.
    pub clamp: [bool; 2],
    pub mips: bool,
    pub linear: bool,
    pub anisotropy: u16,
}

pub const REPEAT: Sampling = Sampling {
    clamp: [false; 2],
    mips: true,
    linear: true,
    anisotropy: 1,
};
pub const CLAMP: Sampling = Sampling {
    clamp: [true; 2],
    ..REPEAT
};

/// What a draw shows: up to four pictures, a cube of six, and numbers.
#[derive(Debug, Clone, Copy)]
pub struct Surface {
    pub textures: [TextureId; 4],
    pub cube: TextureId,
    pub params: [[f32; 4]; PARAMS],
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            textures: [WHITE; 4],
            cube: WHITE_CUBE,
            params: [[0.0; 4]; PARAMS],
        }
    }
}

impl Surface {
    pub fn textures(list: &[TextureId]) -> Self {
        let mut s = Self::default();
        s.textures[..list.len()].copy_from_slice(list);
        s
    }
    pub fn cube(mut self, cube: TextureId) -> Self {
        self.cube = cube;
        self
    }
    pub fn param(mut self, i: usize, v: [f32; 4]) -> Self {
        self.params[i] = v;
        self
    }
}

/// A pass over one target: the window, or a picture.
struct Pass {
    target: Option<TextureId>,
    clear: Option<[f64; 4]>,
    calls: Vec<Call>,
}

struct Call {
    state: State,
    textures: [TextureId; 4],
    cube: TextureId,
    params: u32,
    /// A mesh that stays, instead of the frame's vertices.
    mesh: Option<MeshId>,
    vertices: std::ops::Range<u32>,
}

struct Texture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    /// Samples and depth of a picture that is drawn to.
    target: Option<(wgpu::TextureView, wgpu::TextureView)>,
}

type PipelineKey = (State, wgpu::TextureFormat);

pub struct Gl {
    programs: Vec<&'static str>,
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    textures: Vec<Texture>,
    meshes: Vec<(wgpu::Buffer, u32)>,
    groups: HashMap<([TextureId; 4], TextureId), wgpu::BindGroup>,
    vertices: Vec<Vertex>,
    params: Vec<u8>,
    passes: Vec<Pass>,
    vertex_buffer: Option<wgpu::Buffer>,
    uniform: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    window: Option<((u32, u32), wgpu::TextureView, wgpu::TextureView)>,
    format: wgpu::TextureFormat,
}

const PRELUDE: &str = include_str!("../shaders/prelude.wgsl");

impl Gl {
    /// `source` holds the fragment programs, by the names in `programs`.
    pub fn new(
        gpu: &Gpu,
        source: &str,
        programs: &[&'static str],
        format: wgpu::TextureFormat,
    ) -> Self {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("programs"),
            source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}\n{source}").into()),
        });
        let fragment = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let uniform_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(PARAMS as u64 * 16),
                },
                count: None,
            }],
        });
        let mut entries = Vec::new();
        for i in 0..5 {
            entries.push(fragment(
                i * 2,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: match i {
                        4 => wgpu::TextureViewDimension::Cube,
                        _ => wgpu::TextureViewDimension::D2,
                    },
                    multisampled: false,
                },
            ));
            let sampler = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
            entries.push(fragment(i * 2 + 1, sampler));
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
        let mut gl = Self {
            programs: programs.to_vec(),
            module,
            layout,
            uniform_layout,
            texture_layout,
            pipelines: HashMap::new(),
            textures: Vec::new(),
            meshes: Vec::new(),
            groups: HashMap::new(),
            vertices: Vec::new(),
            params: Vec::new(),
            passes: Vec::new(),
            vertex_buffer: None,
            uniform: None,
            window: None,
            format,
        };
        let white = Image::solid([255; 4]);
        gl.texture(gpu, &white, REPEAT);
        gl.cube(gpu, &std::array::from_fn(|_| white.clone()));
        gl
    }

    fn add(&mut self, gpu: &Gpu, layers: &[&Image], s: Sampling) -> TextureId {
        let first = layers[0];
        let levels = match s.mips {
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
            dimension: Some(match layers.len() {
                6 => wgpu::TextureViewDimension::Cube,
                _ => wgpu::TextureViewDimension::D2,
            }),
            ..Default::default()
        });
        self.textures.push(Texture {
            texture,
            view,
            sampler: sampler(gpu, s),
            target: None,
        });
        self.textures.len() - 1
    }

    pub fn texture(&mut self, gpu: &Gpu, image: &Image, s: Sampling) -> TextureId {
        self.add(gpu, &[image], s)
    }

    /// Faces +X, -X, +Y, -Y, +Z, -Z.
    pub fn cube(&mut self, gpu: &Gpu, faces: &[Image; 6]) -> TextureId {
        let faces: Vec<&Image> = faces.iter().collect();
        self.add(gpu, &faces, CLAMP)
    }

    /// Replaces the picture of a texture of the same size.
    pub fn replace(&mut self, gpu: &Gpu, id: TextureId, image: &Image) {
        write(gpu, &self.textures[id].texture, 0, image);
    }

    /// A picture that can be drawn to and then shown.
    pub fn target(&mut self, gpu: &Gpu, size: (u32, u32), s: Sampling) -> TextureId {
        let texture = gpu.target(size, COLOR, 1);
        let view = |t: wgpu::Texture| t.create_view(&Default::default());
        self.textures.push(Texture {
            view: view(texture.clone()),
            texture,
            sampler: sampler(gpu, Sampling { mips: false, ..s }),
            target: Some((
                view(gpu.target(size, COLOR, SAMPLES)),
                view(gpu.target(size, DEPTH, SAMPLES)),
            )),
        });
        self.textures.len() - 1
    }

    /// Begins a frame on the window, cleared.
    pub fn begin(&mut self, clear: [f64; 4]) {
        self.vertices.clear();
        self.params.clear();
        self.passes.clear();
        self.to(None, Some(clear));
    }

    /// Goes on with another target, which keeps what it showed unless cleared.
    pub fn to(&mut self, target: Option<TextureId>, clear: Option<[f64; 4]>) {
        self.passes.push(Pass {
            target,
            clear,
            calls: Vec::new(),
        });
    }

    pub fn draw(&mut self, state: State, surface: &Surface, vertices: &[Vertex]) {
        let start = self.vertices.len() as u32;
        self.vertices.extend_from_slice(vertices);
        self.call(state, surface, None, start..start + vertices.len() as u32);
    }

    /// Triangles that stay from frame to frame. Places are what the vertex program makes of
    /// them.
    pub fn mesh(&mut self, gpu: &Gpu, vertices: &[Vertex]) -> MeshId {
        self.meshes.push(mesh(gpu, vertices));
        self.meshes.len() - 1
    }

    pub fn replace_mesh(&mut self, gpu: &Gpu, id: MeshId, vertices: &[Vertex]) {
        self.meshes[id] = mesh(gpu, vertices);
    }

    pub fn draw_mesh(&mut self, state: State, surface: &Surface, mesh: MeshId) {
        self.call(state, surface, Some(mesh), 0..self.meshes[mesh].1);
    }

    fn call(
        &mut self,
        state: State,
        surface: &Surface,
        mesh: Option<MeshId>,
        vertices: std::ops::Range<u32>,
    ) {
        if vertices.is_empty() {
            return;
        }
        let params = self.params.len() as u32;
        self.params
            .extend_from_slice(bytemuck::cast_slice(&surface.params));
        self.params
            .resize(self.params.len().next_multiple_of(SLOT as usize), 0);
        let call = Call {
            state,
            textures: surface.textures,
            cube: surface.cube,
            params,
            mesh,
            vertices,
        };
        self.passes
            .last_mut()
            .expect("frame begun")
            .calls
            .push(call);
    }

    /// Four corners in order around, as two triangles.
    pub fn quad(&mut self, state: State, surface: &Surface, v: [Vertex; 4]) {
        self.draw(state, surface, &[v[0], v[1], v[2], v[0], v[2], v[3]]);
    }

    fn pipeline(&mut self, gpu: &Gpu, state: State, format: wgpu::TextureFormat) {
        if self.pipelines.contains_key(&(state, format)) {
            return;
        }
        let attributes = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4,
            2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4];
        let (compare, write) = match state.depth {
            Depth::Off => (wgpu::CompareFunction::Always, false),
            Depth::Always => (wgpu::CompareFunction::Always, true),
            Depth::Less(w) => (wgpu::CompareFunction::Less, w),
            Depth::LessEqual(w) => (wgpu::CompareFunction::LessEqual, w),
            Depth::Equal => (wgpu::CompareFunction::Equal, false),
        };
        let d = &gpu.device;
        let name = self.programs[state.program];
        let made = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(name),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some(state.vertex),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: match state.lines {
                    true => wgpu::PrimitiveTopology::LineList,
                    false => wgpu::PrimitiveTopology::TriangleList,
                },
                cull_mode: state.cull,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some(name),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: state.blend.map(|(src_factor, dst_factor)| {
                        let c = wgpu::BlendComponent {
                            src_factor,
                            dst_factor,
                            operation: wgpu::BlendOperation::Add,
                        };
                        wgpu::BlendState { color: c, alpha: c }
                    }),
                    write_mask: state.write,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert((state, format), made);
    }

    fn group(&mut self, gpu: &Gpu, textures: [TextureId; 4], cube: TextureId) {
        if self.groups.contains_key(&(textures, cube)) {
            return;
        }
        let mut entries = Vec::new();
        for (i, t) in textures.into_iter().chain([cube]).enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 2,
                resource: wgpu::BindingResource::TextureView(&self.textures[t].view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 * 2 + 1,
                resource: wgpu::BindingResource::Sampler(&self.textures[t].sampler),
            });
        }
        let made = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_layout,
            entries: &entries,
        });
        self.groups.insert((textures, cube), made);
    }

    /// Draws everything handed over since `begin`.
    pub fn finish(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        let d = &gpu.device;
        if self.window.as_ref().is_none_or(|w| w.0 != size) {
            let view = |t: wgpu::Texture| t.create_view(&Default::default());
            self.window = Some((
                size,
                view(gpu.target(size, self.format, SAMPLES)),
                view(gpu.target(size, DEPTH, SAMPLES)),
            ));
        }
        let passes = std::mem::take(&mut self.passes);
        for p in &passes {
            let format = p.target.map_or(self.format, |_| COLOR);
            for c in &p.calls {
                self.pipeline(gpu, c.state, format);
                self.group(gpu, c.textures, c.cube);
            }
        }
        let vertices: &[u8] = bytemuck::cast_slice(&self.vertices);
        let room = |b: &wgpu::Buffer, n: usize| b.size() >= n as u64;
        if !self
            .vertex_buffer
            .as_ref()
            .is_some_and(|b| room(b, vertices.len()))
        {
            self.vertex_buffer = Some(d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vertices"),
                size: (vertices.len().max(1 << 16) as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let vertex_buffer = self.vertex_buffer.as_ref().expect("made above");
        gpu.queue.write_buffer(vertex_buffer, 0, vertices);
        if !self
            .uniform
            .as_ref()
            .is_some_and(|u| room(&u.0, self.params.len()))
        {
            self.params
                .resize(self.params.len().max(1 << 14).next_power_of_two(), 0);
            let buffer = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("parameters"),
                contents: &self.params,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let group = d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(PARAMS as u64 * 16),
                    }),
                }],
            });
            self.uniform = Some((buffer, group));
        }
        let (buffer, uniforms) = self.uniform.as_ref().expect("made above");
        gpu.queue.write_buffer(buffer, 0, &self.params);
        let (_, samples, depth) = self.window.as_ref().expect("made above");
        for p in &passes {
            let (samples, depth, resolved, format) = match p.target {
                None => (samples, depth, view, self.format),
                Some(t) => {
                    let t = &self.textures[t];
                    let (samples, depth) = t.target.as_ref().expect("made to be drawn to");
                    (samples, depth, &t.view, COLOR)
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
                        load: p.clear.map_or(wgpu::LoadOp::Load, |[r, g, b, a]| {
                            wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a })
                        }),
                        store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: match p.clear {
                            Some(_) => wgpu::LoadOp::Clear(1.0),
                            None => wgpu::LoadOp::Load,
                        },
                        store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            for c in &p.calls {
                let vertices = c.mesh.map_or(vertex_buffer, |m| &self.meshes[m].0);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_pipeline(&self.pipelines[&(c.state, format)]);
                pass.set_bind_group(0, uniforms, &[c.params]);
                pass.set_bind_group(1, &self.groups[&(c.textures, c.cube)], &[]);
                pass.draw(c.vertices.clone(), 0..1);
            }
        }
    }
}

fn mesh(gpu: &Gpu, vertices: &[Vertex]) -> (wgpu::Buffer, u32) {
    let buffer = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh"),
            contents: match vertices {
                [] => &[0; 16],
                v => bytemuck::cast_slice(v),
            },
            usage: wgpu::BufferUsages::VERTEX,
        });
    (buffer, vertices.len() as u32)
}

fn sampler(gpu: &Gpu, s: Sampling) -> wgpu::Sampler {
    let address = s.clamp.map(|c| match c {
        true => wgpu::AddressMode::ClampToEdge,
        false => wgpu::AddressMode::Repeat,
    });
    let filter = match s.linear {
        true => wgpu::FilterMode::Linear,
        false => wgpu::FilterMode::Nearest,
    };
    gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: address[0],
        address_mode_v: address[1],
        address_mode_w: address[1],
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: s.anisotropy,
        ..Default::default()
    })
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
