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
    pub fn new(gpu: &Gpu) -> Self {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::include_wgsl!("overlay.wgsl"));
        let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay"),
            layout: None,
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
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
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
            white,
            logos: Vec::new(),
        }
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
        logos: bool,
        brightness: f32,
    ) {
        let size = targets.resolved.size();
        let aspect = size.width as f32 / size.height as f32;
        let mut quads: Vec<(&wgpu::BindGroup, [[f32; 8]; 6])> = Vec::new();
        let quad = |x0: f32, x1: f32, y0: f32, y1: f32, c: [f32; 4]| {
            let v = |x: f32, y: f32, u: f32, w: f32| {
                [x * 2.0 - 1.0, y * 2.0 - 1.0, u, w, c[0], c[1], c[2], c[3]]
            };
            let (a, b) = (v(x0, y1, 0.0, 0.0), v(x1, y1, 1.0, 0.0));
            let (c, d) = (v(x0, y0, 0.0, 1.0), v(x1, y0, 1.0, 1.0));
            [a, b, c, c, b, d]
        };
        if logos {
            for l in &self.logos {
                // Sized against a 4:3 screen so they do not grow on wide ones.
                let w = l.width * (crate::camera::ORIGINAL_ASPECT / aspect).min(1.0);
                let h = w * aspect / l.aspect;
                let (x0, x1) = match l.corner {
                    Corner::BottomLeft => (0.0, w),
                    Corner::BottomRight => (1.0 - w, 1.0),
                };
                quads.push((&l.group, quad(x0, x1, 0.0, h, [1.0; 4])));
            }
        }
        if brightness < 1.0 {
            let black = [0.0, 0.0, 0.0, 1.0 - brightness];
            quads.push((&self.white, quad(0.0, 1.0, 0.0, 1.0, black)));
        }
        if quads.is_empty() {
            return;
        }
        let vertices: Vec<[f32; 8]> = quads.iter().flat_map(|q| q.1).collect();
        let buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let mut pass = targets.begin_overlay(enc);
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (i, (group, _)) in quads.iter().enumerate() {
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
