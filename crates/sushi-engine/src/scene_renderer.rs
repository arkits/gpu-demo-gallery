use std::collections::HashMap;

use glam::Mat4;
use sushi_formats::scene::{Scene, Semantic};
use wgpu::util::DeviceExt;

use crate::gpu::{COLOR_FORMAT, DEPTH_FORMAT, Gpu, extent};

/// Uniform interleaved layout: position, light, color, uv.
const STRIDE: usize = 28;

struct Batch {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// (bind group, first index, count)
    draws: Vec<(usize, u32, u32)>,
}

pub struct SceneRenderer {
    pipeline: wgpu::RenderPipeline,
    frame: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    groups: Vec<wgpu::BindGroup>,
    batches: Vec<Batch>,
}

impl SceneRenderer {
    pub fn new(gpu: &Gpu, scene: &Scene, samples: u32) -> sushi_formats::Result<Self> {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));
        let frame_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
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
        let tex_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
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
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&frame_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Unorm8x4, 2 => Unorm8x4, 3 => Float32x2
        ];
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene preview"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: STRIDE as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let frame = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            }],
        });

        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = d.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("white"),
                size: extent(1, 1, 1),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255; 4],
        );
        let bind = |t: &wgpu::Texture| {
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &tex_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &t.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        // Group 0 is the white texture; scene textures follow on first use.
        let mut groups = vec![bind(&white)];
        let mut by_texture = HashMap::new();
        let mut batch_of = HashMap::new();
        let mut batches: Vec<Batch> = Vec::new();
        for item in scene.groups.iter().flat_map(|g| g.draw_items()) {
            let draw = &scene.draws[item.draw];
            let geometry = &scene.geometry[draw.geometry()];
            let stream = &geometry.chunk.streams[0];
            let attrs = stream.attributes();
            let has = |s| attrs.iter().find(|a| a.semantic == s);
            let mut group = 0;
            if let (Some(_), Some(t)) = (has(Semantic::Uv), draw.texture(0)) {
                let source = &scene.textures[t];
                if !source.is_cube() && !source.texels.is_empty() {
                    if let std::collections::hash_map::Entry::Vacant(e) = by_texture.entry(t) {
                        groups.push(bind(&gpu.upload(source)?));
                        e.insert(groups.len() - 1);
                    }
                    group = by_texture[&t];
                }
            }
            let key = (draw.geometry(), item.vertex_buffer);
            let index = *batch_of.entry(key).or_insert_with(|| {
                let raw = &stream.buffers[0][item.vertex_buffer].1;
                let mut data = Vec::new();
                for v in raw.chunks_exact(stream.stride() as usize) {
                    let mut push = |s, size: usize, fallback: u8| match has(s) {
                        Some(a) => data.extend_from_slice(&v[a.offset as usize..][..size]),
                        None => data.extend(std::iter::repeat_n(fallback, size)),
                    };
                    push(Semantic::Position, 12, 0);
                    push(Semantic::VertexLight, 4, 255);
                    push(Semantic::BaseColor, 4, 255);
                    push(Semantic::Uv, 8, 0);
                }
                let ib = stream
                    .index_buffer_of(item.vertex_buffer)
                    .expect("index buffer");
                let mut indices = geometry.chunk.indices[ib].1.clone();
                if indices.len() % 2 == 1 {
                    indices.push(0); // buffer sizes must be multiples of 4
                }
                let make = |contents: &[u8], usage| {
                    d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: None,
                        contents,
                        usage,
                    })
                };
                batches.push(Batch {
                    vertices: make(&data, wgpu::BufferUsages::VERTEX),
                    indices: make(bytemuck::cast_slice(&indices), wgpu::BufferUsages::INDEX),
                    draws: Vec::new(),
                });
                batches.len() - 1
            });
            batches[index].draws.push((group, item.start, item.count));
        }
        Ok(Self {
            pipeline,
            frame,
            frame_group,
            groups,
            batches,
        })
    }

    pub fn prepare(&self, gpu: &Gpu, view_proj: Mat4) {
        let m = view_proj.to_cols_array();
        gpu.queue
            .write_buffer(&self.frame, 0, bytemuck::cast_slice(&m));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame_group, &[]);
        for b in &self.batches {
            pass.set_vertex_buffer(0, b.vertices.slice(..));
            pass.set_index_buffer(b.indices.slice(..), wgpu::IndexFormat::Uint16);
            for &(group, start, count) in &b.draws {
                pass.set_bind_group(1, &self.groups[group], &[]);
                pass.draw_indexed(start..start + count, 0, 0..1);
            }
        }
    }
}
