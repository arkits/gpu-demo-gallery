//! Logos and fades, drawn over the finished frame (0x418af0, 0x41a950).
use sushi_formats::texture::Level;
use wgpu::util::DeviceExt;

use crate::gpu::{COLOR_FORMAT, Gpu, extent};
use crate::targets::Targets;

#[derive(Debug, Clone, Copy)]
pub enum Corner {
    BottomLeft,
    BottomRight,
}

/// Lines and text of a frame script. Positions are fractions, bottom up.
#[derive(Debug, Clone)]
pub enum Mark {
    Outline {
        viewport: [f32; 4],
        color: [f32; 3],
    },
    /// `x` is the center and `y` the bottom of the first line; `size` its height.
    Text {
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        text: String,
        background: bool,
    },
}

/// Character width over height (0x404810, command 0xc).
const CHARACTER: f32 = 0.3125;

struct Logo {
    group: wgpu::BindGroup,
    aspect: f32,
    corner: Corner,
    /// Fraction of the 4:3 screen width.
    width: f32,
}

pub struct Overlay {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    /// Replaces the frame with the image, keeping as much of it as the alpha says.
    text: wgpu::RenderPipeline,
    font: Option<wgpu::BindGroup>,
    white: wgpu::BindGroup,
    logos: Vec<Logo>,
}

/// Uncompressed 32-bit TGA, as `Logo2.tga` is stored.
pub fn read_tga(data: &[u8]) -> Option<Level> {
    let u16_at = |o: usize| {
        Some(usize::from(u16::from_le_bytes([
            *data.get(o)?,
            *data.get(o + 1)?,
        ])))
    };
    if *data.get(2)? != 2 || *data.get(16)? != 32 {
        return None;
    }
    let (width, height) = (u16_at(12)?, u16_at(14)?);
    if width == 0 || height == 0 {
        return None;
    }
    let start = 18 + usize::from(data[0]);
    let body = data.get(start..start + width * height * 4)?;
    let top_first = data[17] & 0x20 != 0;
    let rows: Vec<&[u8]> = body.chunks_exact(width * 4).collect();
    let order: Vec<&&[u8]> = if top_first {
        rows.iter().collect()
    } else {
        rows.iter().rev().collect()
    };
    let rgba = order
        .into_iter()
        .flat_map(|r| r.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]))
        .collect();
    Some(Level {
        width,
        height,
        rgba,
    })
}

impl Overlay {
    pub fn new(gpu: &Gpu, target: wgpu::TextureFormat) -> Self {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::include_wgsl!("overlay.wgsl"));
        let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let group_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
            bind_group_layouts: &[Some(&group_layout)],
            immediate_size: 0,
        });
        let make = |blend| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("overlay"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make(wgpu::BlendState::ALPHA_BLENDING);
        // `gText.ssh` "NormalBG".
        let over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::SrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let text = make(wgpu::BlendState {
            color: over,
            alpha: over,
        });
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = Level {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let white = Self::group(gpu, &pipeline, &sampler, &[white]);
        Self {
            pipeline,
            sampler,
            text,
            font: None,
            white,
            logos: Vec::new(),
        }
    }

    /// `mips` has 8 columns and 16 rows of characters, upright.
    pub fn set_font(&mut self, gpu: &Gpu, mips: &[Level]) {
        self.font =
            (!mips.is_empty()).then(|| Self::group(gpu, &self.pipeline, &self.sampler, mips));
    }

    fn group(
        gpu: &Gpu,
        pipeline: &wgpu::RenderPipeline,
        sampler: &wgpu::Sampler,
        mips: &[Level],
    ) -> wgpu::BindGroup {
        let data: Vec<u8> = mips.iter().flat_map(|m| m.rgba.clone()).collect();
        let texture = gpu.device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("overlay"),
                size: extent(mips[0].width, mips[0].height, 1),
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &data,
        );
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn add_logo(&mut self, gpu: &Gpu, mips: &[Level], corner: Corner, width: f32) {
        self.logos.push(Logo {
            group: Self::group(gpu, &self.pipeline, &self.sampler, mips),
            aspect: mips[0].width as f32 / mips[0].height as f32,
            corner,
            width,
        });
    }

    /// `brightness` 1 leaves the frame untouched, 0 is black.
    pub fn draw(
        &self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        marks: &[Mark],
        logos: bool,
        brightness: f32,
    ) {
        let size = targets.color.resolved.size();
        let aspect = size.width as f32 / size.height as f32;
        let mut quads: Vec<(&wgpu::RenderPipeline, &wgpu::BindGroup, [[f32; 8]; 6])> = Vec::new();
        let part = |x0: f32, x1: f32, y0: f32, y1: f32, c: [f32; 4], uv: [f32; 4]| {
            let v = |x: f32, y: f32, u: f32, w: f32| {
                [x * 2.0 - 1.0, y * 2.0 - 1.0, u, w, c[0], c[1], c[2], c[3]]
            };
            let (a, b) = (v(x0, y1, uv[0], uv[1]), v(x1, y1, uv[2], uv[1]));
            let (c, d) = (v(x0, y0, uv[0], uv[3]), v(x1, y0, uv[2], uv[3]));
            [a, b, c, c, b, d]
        };
        let quad = |x0, x1, y0, y1, c| part(x0, x1, y0, y1, c, [0.0, 0.0, 1.0, 1.0]);
        // Sized against a 4:3 screen, as the logos are.
        let narrow = (crate::camera::ORIGINAL_ASPECT / aspect).min(1.0);
        for m in marks {
            match m {
                Mark::Outline { viewport, color } => {
                    let line = (size.height as f32 / 768.0).round().max(1.0);
                    let (px, py) = (line / size.width as f32, line / size.height as f32);
                    let [x0, y0, x1, y1] = *viewport;
                    let c = [color[0], color[1], color[2], 1.0];
                    for q in [
                        quad(x0, x1, y0, y0 + py, c),
                        quad(x0, x1, y1 - py, y1, c),
                        quad(x0, x0 + px, y0, y1, c),
                        quad(x1 - px, x1, y0, y1, c),
                    ] {
                        quads.push((&self.pipeline, &self.white, q));
                    }
                }
                Mark::Text {
                    x,
                    y,
                    size,
                    color,
                    text,
                    background,
                } => {
                    let Some(font) = &self.font else { continue };
                    let width = size * CHARACTER * narrow;
                    for (row, line) in text.split('\n').enumerate() {
                        let left = x - line.chars().count() as f32 * width * 0.5;
                        let y = y - row as f32 * size;
                        for (i, ch) in line.chars().enumerate() {
                            let n = (ch as u32).min(127);
                            let (col, r) = ((n % 8) as f32 / 8.0, (n / 8) as f32 / 16.0);
                            let uv = [col + 0.025, r, col + 0.125 - 0.025, r + 1.0 / 16.0];
                            let at = left + i as f32 * width;
                            let q = part(at, at + width, y, y + size, *color, uv);
                            let pipe = if *background {
                                &self.text
                            } else {
                                &self.pipeline
                            };
                            quads.push((pipe, font, q));
                        }
                    }
                }
            }
        }
        if logos {
            for l in &self.logos {
                // Sized against a 4:3 screen so they do not grow on wide ones.
                let w = l.width * narrow;
                let h = w * aspect / l.aspect;
                let (x0, x1) = match l.corner {
                    Corner::BottomLeft => (0.0, w),
                    Corner::BottomRight => (1.0 - w, 1.0),
                };
                quads.push((&self.pipeline, &l.group, quad(x0, x1, 0.0, h, [1.0; 4])));
            }
        }
        if brightness < 1.0 {
            let black = [0.0, 0.0, 0.0, 1.0 - brightness];
            quads.push((&self.pipeline, &self.white, quad(0.0, 1.0, 0.0, 1.0, black)));
        }
        if quads.is_empty() {
            return;
        }
        let vertices: Vec<[f32; 8]> = quads.iter().flat_map(|q| q.2).collect();
        let buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let mut pass = targets.begin_overlay(enc);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (i, (pipeline, group, _)) in quads.iter().enumerate() {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, *group, &[]);
            let start = i as u32 * 6;
            pass.draw(start..start + 6, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tga_rows_are_bottom_up_by_default() {
        let mut t = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 32, 0];
        t.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        let l = read_tga(&t).unwrap();
        assert_eq!(l.rgba, [7, 6, 5, 8, 3, 2, 1, 4]);
        assert!(read_tga(&t[..20]).is_none());
    }
}
