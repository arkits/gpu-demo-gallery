//! Multisampled frame targets and the pass that shows them on a surface.
use crate::gpu::{COLOR_FORMAT, DEPTH_FORMAT, Gpu, extent};

/// `multisampleType = 4` in `Sushi.ini`.
pub const SAMPLES: u32 = 4;

pub struct Targets {
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved_view: wgpu::TextureView,
    pub resolved: wgpu::Texture,
}

impl Targets {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let make = |format, samples, usage| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame target"),
                size: extent(width.max(1) as usize, height.max(1) as usize, 1),
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: usage | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };
        let none = wgpu::TextureUsages::empty();
        let resolved = make(
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        Self {
            color: view(&make(COLOR_FORMAT, SAMPLES, none)),
            depth: view(&make(DEPTH_FORMAT, SAMPLES, none)),
            resolved_view: view(&resolved),
            resolved,
        }
    }

    /// Begins the frame with `Clear Z(1.0) Stencil(128)` from `Normal.sds`.
    pub fn begin<'e>(&self, enc: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.color,
                depth_slice: None,
                resolve_target: Some(&self.resolved_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(128),
                    store: wgpu::StoreOp::Discard,
                }),
            }),
            ..Default::default()
        })
    }
}

pub struct Presenter {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl Presenter {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::include_wgsl!("present.wgsl"));
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(if format.is_srgb() { "fs_srgb" } else { "fs" }),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = d.create_sampler(&Default::default());
        Self { pipeline, sampler }
    }

    pub fn present(
        &self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        surface: &wgpu::TextureView,
    ) {
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&targets.resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface,
                depth_slice: None,
                resolve_target: None,
                ops: Default::default(),
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}
