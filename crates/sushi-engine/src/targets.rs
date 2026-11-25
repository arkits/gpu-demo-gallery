//! Buffers that frames are drawn to, and the pass that shows one on a surface.
use crate::gpu::{COLOR_FORMAT, DEEP_FORMAT, DEPTH_FORMAT, Gpu, extent};

/// `multisampleType` in `Sushi.ini`; 0 there is 1 here.
pub const SAMPLES: u32 = 1;

/// Stencil value that scripts and shaders treat as "no shadow".
const STENCIL: u32 = 128;

pub struct ColorBuffer {
    /// Drawn to instead of `resolved` while multisampling.
    samples: Option<wgpu::TextureView>,
    pub resolved_view: wgpu::TextureView,
    pub resolved: wgpu::Texture,
}

pub struct DepthBuffer(wgpu::TextureView, (u32, u32));

fn make(
    gpu: &Gpu,
    (width, height): (u32, u32),
    format: wgpu::TextureFormat,
    samples: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("buffer"),
        size: extent(width.max(1) as usize, height.max(1) as usize, 1),
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: usage | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

impl ColorBuffer {
    pub fn new(gpu: &Gpu, size: (u32, u32), deep: bool) -> Self {
        let format = if deep { DEEP_FORMAT } else { COLOR_FORMAT };
        let shown = wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING;
        let resolved = make(gpu, size, format, 1, shown);
        let none = wgpu::TextureUsages::empty();
        Self {
            samples: (SAMPLES > 1)
                .then(|| make(gpu, size, format, SAMPLES, none).create_view(&Default::default())),
            resolved_view: resolved.create_view(&Default::default()),
            resolved,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.resolved.width(), self.resolved.height())
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.resolved.format()
    }
}

impl DepthBuffer {
    pub fn new(gpu: &Gpu, size: (u32, u32)) -> Self {
        let t = make(
            gpu,
            size,
            DEPTH_FORMAT,
            SAMPLES,
            wgpu::TextureUsages::empty(),
        );
        Self(t.create_view(&Default::default()), size)
    }
    pub fn size(&self) -> (u32, u32) {
        self.1
    }
}

/// Clears a pass begins with; what is not cleared is kept.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Clears {
    pub color: [Option<wgpu::Color>; 4],
    pub depth: Option<f32>,
    pub stencil: Option<u32>,
}

impl Clears {
    pub fn all(color: wgpu::Color) -> Self {
        Self {
            color: [Some(color); 4],
            depth: Some(1.0),
            stencil: Some(STENCIL),
        }
    }
}

/// Starts drawing to some color buffers and a depth buffer of the same size.
pub fn begin<'e>(
    enc: &'e mut wgpu::CommandEncoder,
    colors: &[Option<&ColorBuffer>],
    depth: Option<&DepthBuffer>,
    clears: Clears,
) -> wgpu::RenderPass<'e> {
    let store = wgpu::StoreOp::Store;
    let attachments: Vec<_> = colors
        .iter()
        .zip(clears.color)
        .map(|(c, clear)| {
            c.map(|c| wgpu::RenderPassColorAttachment {
                view: c.samples.as_ref().unwrap_or(&c.resolved_view),
                depth_slice: None,
                resolve_target: c.samples.is_some().then_some(&c.resolved_view),
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store,
                },
            })
        })
        .collect();
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("frame"),
        color_attachments: &attachments,
        depth_stencil_attachment: depth.map(|d| wgpu::RenderPassDepthStencilAttachment {
            view: &d.0,
            depth_ops: Some(wgpu::Operations {
                load: clears.depth.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                store,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: clears
                    .stencil
                    .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                store,
            }),
        }),
        ..Default::default()
    })
}

/// The back buffer.
pub struct Targets {
    pub color: ColorBuffer,
    pub depth: DepthBuffer,
}

impl Targets {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let size = (width.max(1), height.max(1));
        Self {
            color: ColorBuffer::new(gpu, size, false),
            depth: DepthBuffer::new(gpu, size),
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.color.size()
    }

    /// Draws on top of the finished frame, without depth or multisampling.
    pub fn begin_overlay<'e>(&self, enc: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.color.resolved_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
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
                    resource: wgpu::BindingResource::TextureView(&targets.color.resolved_view),
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
