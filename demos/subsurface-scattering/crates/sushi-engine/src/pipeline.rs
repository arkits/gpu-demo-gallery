//! Pipelines, textures and uniform storage for translated passes.
use std::collections::HashMap;

use sushi_shader::wgsl::{ElementType, Module};
use sushi_shader::{Address, BlendOp, Compare, Cull, Factor, Filter, Pass, Sampler, StencilOp};

use crate::gpu::{COLOR_FORMAT, DEPTH_FORMAT, Gpu};

pub type TextureId = usize;

pub struct Textures {
    views: Vec<(wgpu::TextureView, bool)>,
    names: HashMap<String, TextureId>,
    samplers: HashMap<(u8, [Address; 2], u16, u32), wgpu::Sampler>,
}

pub const BLACK: TextureId = 0;
const BLACK_CUBE: TextureId = 1;

impl Textures {
    pub fn new(gpu: &Gpu) -> Self {
        let mut t = Self {
            views: Vec::new(),
            names: HashMap::new(),
            samplers: HashMap::new(),
        };
        for layers in [1, 6] {
            let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("black"),
                size: crate::gpu::extent(1, 1, layers),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            t.push(&tex, layers == 6);
        }
        t
    }

    fn push(&mut self, tex: &wgpu::Texture, cube: bool) -> TextureId {
        let view = tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(if cube {
                wgpu::TextureViewDimension::Cube
            } else {
                wgpu::TextureViewDimension::D2
            }),
            ..Default::default()
        });
        self.views.push((view, cube));
        self.views.len() - 1
    }

    /// Uploads a texture, or registers it under its name for later references.
    /// The same file may exist in two formats, so the format is part of the key.
    pub fn add(
        &mut self,
        gpu: &Gpu,
        t: &sushi_formats::texture::Texture,
    ) -> sushi_formats::Result<Option<TextureId>> {
        let key = Self::key(t);
        if let Some(id) = self.names.get(&key) {
            return Ok(Some(*id));
        }
        if t.texels.is_empty() {
            return Ok(None);
        }
        let id = self.push(&gpu.upload(t)?, t.is_cube());
        self.names.insert(key, id);
        Ok(Some(id))
    }

    fn key(t: &sushi_formats::texture::Texture) -> String {
        format!("{}|{:?}|{}", t.name, t.format(), t.is_cube())
    }

    pub fn find(&self, t: &sushi_formats::texture::Texture) -> Option<TextureId> {
        self.names.get(&Self::key(t)).copied()
    }

    fn view(&self, id: TextureId, cube: bool) -> &wgpu::TextureView {
        let (view, is_cube) = &self.views[id];
        if *is_cube == cube {
            view
        } else {
            &self.views[if cube { BLACK_CUBE } else { BLACK }].0
        }
    }

    /// A view that can be sampled like a texture, such as a render target's.
    pub fn add_view(&mut self, view: wgpu::TextureView, cube: bool) -> TextureId {
        self.views.push((view, cube));
        self.views.len() - 1
    }

    /// Shows another view under the same id.
    pub fn replace_view(&mut self, id: TextureId, view: wgpu::TextureView) {
        self.views[id].0 = view;
    }

    fn sampler(&mut self, gpu: &Gpu, s: Option<&Sampler>, unfiltered: bool) -> &wgpu::Sampler {
        let (mut filter, clamp, aniso, mips) = s
            .map_or((Filter::Linear, [Address::Wrap; 2], 1, 32), |s| {
                (s.filter, s.clamp, s.aniso, s.max_mip)
            });
        if unfiltered {
            filter = Filter::Nearest;
        }
        // Filtering along the slope needs linear filtering throughout.
        let aniso = match filter {
            Filter::Trilinear => aniso.clamp(1, 16),
            _ => 1,
        };
        self.samplers
            .entry((filter as u8, clamp, aniso, mips))
            .or_insert_with(|| {
                let address = clamp.map(|c| match c {
                    Address::Clamp => wgpu::AddressMode::ClampToEdge,
                    Address::Wrap => wgpu::AddressMode::Repeat,
                    Address::Mirror => wgpu::AddressMode::MirrorRepeat,
                });
                let linear = match filter {
                    Filter::Nearest => wgpu::FilterMode::Nearest,
                    _ => wgpu::FilterMode::Linear,
                };
                gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                    address_mode_u: address[0],
                    address_mode_v: address[1],
                    address_mode_w: address[1],
                    mag_filter: linear,
                    min_filter: linear,
                    mipmap_filter: match filter {
                        Filter::Trilinear => wgpu::MipmapFilterMode::Linear,
                        _ => wgpu::MipmapFilterMode::Nearest,
                    },
                    anisotropy_clamp: aniso,
                    lod_max_clamp: match filter {
                        Filter::Linear | Filter::Nearest => 0.0,
                        _ => mips as f32,
                    },
                    ..Default::default()
                })
            })
    }
}

/// Where each vertex semantic lives in one vertex buffer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamLayout {
    pub stride: u32,
    /// Semantic, index, type, offset.
    pub elements: Vec<(String, u32, ElementType, u32)>,
}

/// All vertex buffers of a draw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct VertexLayout(pub Vec<StreamLayout>);

impl VertexLayout {
    pub fn find(&self, semantic: &str, index: u32) -> Option<ElementType> {
        let mut all = self.0.iter().flat_map(|s| &s.elements);
        all.find(|e| e.0 == semantic && e.1 == index).map(|e| e.2)
    }
}

pub struct Pipeline {
    pub raw: wgpu::RenderPipeline,
    pub module: Module,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: Option<wgpu::BindGroupLayout>,
    pub stencil_ref: Option<u32>,
    uniforms: Option<(u64, wgpu::BindGroup)>,
    textures: HashMap<Vec<TextureId>, wgpu::BindGroup>,
}

pub type PipelineId = usize;

/// Formats of the color targets in use, and whether there is a depth buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Formats {
    pub colors: [Option<wgpu::TextureFormat>; 4],
    pub depth: bool,
    pub samples: u32,
}

impl Formats {
    /// Without the unused targets at the end.
    pub fn colors(&self) -> &[Option<wgpu::TextureFormat>] {
        let n = self.colors.iter().rposition(Option::is_some);
        &self.colors[..n.map_or(0, |n| n + 1)]
    }
}

#[derive(Default)]
pub struct Pipelines {
    pub list: Vec<Pipeline>,
}

impl Pipelines {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &mut self,
        gpu: &Gpu,
        label: &str,
        pass: &Pass,
        module: Module,
        layout: &VertexLayout,
        mirrored: bool,
        formats: &Formats,
    ) -> PipelineId {
        let d = &gpu.device;
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(module.source.as_str().into()),
        });
        let uniform = |binding, visibility, n: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(u64::from(n) * 16),
            },
            count: None,
        };
        let uniform_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX, module.vs_consts),
                uniform(1, wgpu::ShaderStages::FRAGMENT, module.ps_consts),
            ],
        });
        let entries: Vec<_> = module
            .samplers
            .iter()
            .enumerate()
            .flat_map(|(stage, (_, cube, unfiltered))| {
                let stage = stage as u32;
                let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty,
                    count: None,
                };
                [
                    entry(
                        stage * 2,
                        wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: !unfiltered,
                            },
                            view_dimension: if *cube {
                                wgpu::TextureViewDimension::Cube
                            } else {
                                wgpu::TextureViewDimension::D2
                            },
                            multisampled: false,
                        },
                    ),
                    entry(
                        stage * 2 + 1,
                        wgpu::BindingType::Sampler(match unfiltered {
                            true => wgpu::SamplerBindingType::NonFiltering,
                            false => wgpu::SamplerBindingType::Filtering,
                        }),
                    ),
                ]
            })
            .collect();
        let texture_layout = (!entries.is_empty()).then(|| {
            d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &entries,
            })
        });
        let mut groups = vec![Some(&uniform_layout)];
        groups.extend(texture_layout.as_ref().map(Some));
        let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &groups,
            immediate_size: 0,
        });
        let attributes: Vec<Vec<_>> = layout
            .0
            .iter()
            .map(|stream| {
                let used = module
                    .inputs
                    .iter()
                    .enumerate()
                    .filter_map(|(location, i)| {
                        let mut all = stream.elements.iter();
                        let e = all.find(|e| e.0 == i.semantic && e.1 == i.index)?;
                        Some(wgpu::VertexAttribute {
                            format: match i.ty {
                                ElementType::Float1 => wgpu::VertexFormat::Float32,
                                ElementType::Float2 => wgpu::VertexFormat::Float32x2,
                                ElementType::Float3 => wgpu::VertexFormat::Float32x3,
                                ElementType::Float4 => wgpu::VertexFormat::Float32x4,
                                ElementType::Color => wgpu::VertexFormat::Unorm8x4,
                                ElementType::Ubyte4 => wgpu::VertexFormat::Uint8x4,
                                ElementType::Short4 => wgpu::VertexFormat::Sint16x4,
                                ElementType::Short4n => wgpu::VertexFormat::Snorm16x4,
                            },
                            offset: u64::from(e.3),
                            shader_location: location as u32,
                        })
                    });
                used.collect()
            })
            .collect();
        let buffers: Vec<_> = layout
            .0
            .iter()
            .zip(&attributes)
            .map(|(stream, attributes)| {
                Some(wgpu::VertexBufferLayout {
                    array_stride: u64::from(stream.stride),
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes,
                })
            })
            .collect();
        let write_mask = {
            use wgpu::ColorWrites as W;
            let all = [W::RED, W::GREEN, W::BLUE, W::ALPHA];
            let on = all.into_iter().zip(pass.color_write);
            on.filter(|w| w.1).fold(W::empty(), |a, w| a | w.0)
        };
        let targets: Vec<_> = formats
            .colors()
            .iter()
            .enumerate()
            .map(|(n, format)| {
                format.map(|format| wgpu::ColorTargetState {
                    format,
                    // Shadow maps hold single numbers, which cannot be blended.
                    blend: pass
                        .blend
                        .filter(|_| format != crate::gpu::SHADOW_FORMAT)
                        .map(|b| {
                            let most = matches!(b.op, BlendOp::Min | BlendOp::Max);
                            let of = |f| {
                                if most {
                                    wgpu::BlendFactor::One
                                } else {
                                    factor(f)
                                }
                            };
                            let c = wgpu::BlendComponent {
                                src_factor: of(b.src),
                                dst_factor: of(b.dst),
                                operation: match b.op {
                                    BlendOp::Add => wgpu::BlendOperation::Add,
                                    BlendOp::Subtract => wgpu::BlendOperation::Subtract,
                                    BlendOp::ReverseSubtract => {
                                        wgpu::BlendOperation::ReverseSubtract
                                    }
                                    BlendOp::Min => wgpu::BlendOperation::Min,
                                    BlendOp::Max => wgpu::BlendOperation::Max,
                                },
                            };
                            wgpu::BlendState { color: c, alpha: c }
                        }),
                    write_mask: match (n as u32) < module.outputs {
                        true if format == crate::gpu::SHADOW_FORMAT => wgpu::ColorWrites::ALL,
                        true => write_mask,
                        false => wgpu::ColorWrites::empty(),
                    },
                })
            })
            .collect();
        let raw = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: wgpu::PrimitiveState {
                // D3D9 calls clockwise faces front faces; a mirror turns them around.
                front_face: match mirrored {
                    true => wgpu::FrontFace::Ccw,
                    false => wgpu::FrontFace::Cw,
                },
                cull_mode: match pass.cull {
                    Cull::None => None,
                    Cull::Cw => Some(wgpu::Face::Front),
                    Cull::Ccw => Some(wgpu::Face::Back),
                },
                ..Default::default()
            },
            depth_stencil: formats.depth.then(|| wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(pass.z_write),
                depth_compare: Some(if pass.z_test {
                    compare(pass.z_func)
                } else {
                    wgpu::CompareFunction::Always
                }),
                stencil: stencil(pass),
                // Read as steps of a 24 bit buffer.
                bias: wgpu::DepthBiasState {
                    constant: pass.z_bias as i32,
                    slope_scale: pass.z_slope,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: formats.samples,
                alpha_to_coverage_enabled: pass.alpha_to_coverage && formats.samples > 1,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        self.list.push(Pipeline {
            raw,
            module,
            uniform_layout,
            texture_layout,
            stencil_ref: pass.stencil.map(|s| s.reference),
            uniforms: None,
            textures: HashMap::new(),
        });
        self.list.len() - 1
    }

    /// Makes the bind groups a draw needs; `textures` is in sampler order.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        store: &mut Textures,
        id: PipelineId,
        textures: &[TextureId],
        sampler: &dyn Fn(&str) -> Option<Sampler>,
    ) {
        let p = &mut self.list[id];
        let Some(layout) = &p.texture_layout else {
            return;
        };
        if p.textures.contains_key(textures) {
            return;
        }
        let made: Vec<wgpu::Sampler> = p
            .module
            .samplers
            .iter()
            .map(|(name, _, unfiltered)| {
                store
                    .sampler(gpu, sampler(name).as_ref(), *unfiltered)
                    .clone()
            })
            .collect();
        let mut entries = Vec::new();
        for (k, (_, cube, _)) in p.module.samplers.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: k as u32 * 2,
                resource: wgpu::BindingResource::TextureView(store.view(textures[k], *cube)),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: k as u32 * 2 + 1,
                resource: wgpu::BindingResource::Sampler(&made[k]),
            });
        }
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &entries,
        });
        p.textures.insert(textures.to_vec(), group);
    }

    pub fn forget_textures(&mut self) {
        self.list.iter_mut().for_each(|p| p.textures.clear());
    }

    pub fn bind_uniforms(&mut self, gpu: &Gpu, arena: &Arena) {
        for p in &mut self.list {
            if p.uniforms.as_ref().is_some_and(|u| u.0 == arena.generation) {
                continue;
            }
            let entry = |binding, n: u32| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &arena.buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(u64::from(n) * 16),
                }),
            };
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &p.uniform_layout,
                entries: &[entry(0, p.module.vs_consts), entry(1, p.module.ps_consts)],
            });
            p.uniforms = Some((arena.generation, group));
        }
    }

    pub fn record(&self, pass: &mut wgpu::RenderPass, d: &DrawCall, buffers: &[wgpu::Buffer]) {
        let p = &self.list[d.pipeline];
        pass.set_pipeline(&p.raw);
        if let Some(r) = p.stencil_ref {
            pass.set_stencil_reference(r);
        }
        let (_, uniforms) = p.uniforms.as_ref().expect("uniforms bound");
        pass.set_bind_group(0, uniforms, &[d.vs_offset, d.ps_offset]);
        if let Some(g) = p.textures.get(&d.textures) {
            pass.set_bind_group(1, g, &[]);
        }
        for (i, v) in d.vertices.iter().enumerate() {
            pass.set_vertex_buffer(i as u32, buffers[*v].slice(..));
        }
        pass.set_index_buffer(buffers[d.indices.0].slice(..), d.indices.1);
        pass.draw_indexed(d.start..d.start + d.count, 0, 0..1);
    }
}

#[derive(Debug, Clone)]
pub struct DrawCall {
    pub pipeline: PipelineId,
    pub vs_offset: u32,
    pub ps_offset: u32,
    pub textures: Vec<TextureId>,
    pub vertices: Vec<usize>,
    /// And whether they have 16 or 32 bits.
    pub indices: (usize, wgpu::IndexFormat),
    pub start: u32,
    pub count: u32,
}

/// Per-frame constants, uploaded once and addressed by dynamic offset.
pub struct Arena {
    data: Vec<u8>,
    buffer: wgpu::Buffer,
    generation: u64,
}

const ALIGN: usize = 256;
/// Room for the largest binding past the last offset.
const TAIL: usize = 16384;

impl Arena {
    pub fn new(gpu: &Gpu) -> Self {
        Self {
            data: Vec::new(),
            buffer: Self::make(gpu, 1 << 20),
            generation: 1,
        }
    }
    fn make(gpu: &Gpu, size: usize) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("constants"),
            size: size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }
    pub fn clear(&mut self) {
        self.data.clear();
    }
    pub fn push(&mut self, values: &[[f32; 4]]) -> u32 {
        let at = self.data.len();
        self.data.extend_from_slice(bytemuck::cast_slice(values));
        self.data.resize(self.data.len().next_multiple_of(ALIGN), 0);
        at as u32
    }
    pub fn upload(&mut self, gpu: &Gpu) {
        let need = self.data.len() + TAIL;
        if need as u64 > self.buffer.size() {
            self.buffer = Self::make(gpu, need.next_power_of_two());
            self.generation += 1;
        }
        gpu.queue.write_buffer(&self.buffer, 0, &self.data);
    }
}

fn compare(c: Compare) -> wgpu::CompareFunction {
    use wgpu::CompareFunction as W;
    match c {
        Compare::Never => W::Never,
        Compare::Less => W::Less,
        Compare::Equal => W::Equal,
        Compare::LessEqual => W::LessEqual,
        Compare::Greater => W::Greater,
        Compare::NotEqual => W::NotEqual,
        Compare::GreaterEqual => W::GreaterEqual,
        Compare::Always => W::Always,
    }
}

fn factor(f: Factor) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as W;
    match f {
        Factor::Zero => W::Zero,
        Factor::One => W::One,
        Factor::SrcAlpha => W::SrcAlpha,
        Factor::InvSrcAlpha => W::OneMinusSrcAlpha,
        Factor::DestAlpha => W::DstAlpha,
        Factor::InvDestAlpha => W::OneMinusDstAlpha,
        Factor::SrcColor => W::Src,
        Factor::InvSrcColor => W::OneMinusSrc,
        Factor::DestColor => W::Dst,
        Factor::InvDestColor => W::OneMinusDst,
    }
}

fn stencil(pass: &Pass) -> wgpu::StencilState {
    let Some(s) = pass.stencil else {
        return Default::default();
    };
    let op = |o: StencilOp| match o {
        StencilOp::Keep => wgpu::StencilOperation::Keep,
        StencilOp::IncrSat => wgpu::StencilOperation::IncrementClamp,
        StencilOp::DecrSat => wgpu::StencilOperation::DecrementClamp,
        StencilOp::Incr => wgpu::StencilOperation::IncrementWrap,
        StencilOp::Decr => wgpu::StencilOperation::DecrementWrap,
        StencilOp::Zero => wgpu::StencilOperation::Zero,
        StencilOp::Replace => wgpu::StencilOperation::Replace,
        StencilOp::Invert => wgpu::StencilOperation::Invert,
    };
    let face = |zfail, pass| wgpu::StencilFaceState {
        compare: compare(s.func),
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: op(zfail),
        pass_op: op(pass),
    };
    wgpu::StencilState {
        front: face(s.zfail, s.pass),
        back: face(s.ccw_zfail, s.ccw_pass),
        read_mask: 0xff,
        write_mask: 0xff,
    }
}
