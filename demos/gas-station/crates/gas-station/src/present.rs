//! Shows the viewer's 1024×768 window on the screen: turned over (OpenGL's rows start at the
//! bottom), at 4:3 in the middle with black bars, as the full-screen original showed it on a
//! 4:3 monitor. Alpha holds the demo's HDR intensity and is not shown.

const SHADER: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct V { @builtin(position) p: vec4f, @location(0) uv: vec2f }
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var v: V;
    v.p = vec4f(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    // Screen y up is OpenGL row up.
    v.uv = vec2f(x, y);
    return v;
}
@fragment fn fs(v: V) -> @location(0) vec4f {
    return vec4f(textureSampleLevel(t, s, v.uv, 0.0).rgb, 1.0);
}
"#;

pub struct Present {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Present {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Present {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("present"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("present"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("present"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("present"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Present { pipeline, layout, sampler }
    }

    /// The 4:3 rectangle within a `w`×`h` surface: x, y, width, height.
    pub fn rect(window: (u32, u32), w: u32, h: u32) -> [f32; 4] {
        let aspect = window.0 as f32 / window.1 as f32;
        let (w, h) = (w as f32, h as f32);
        let (rw, rh) = if w / h > aspect { (h * aspect, h) } else { (w, w / aspect) };
        [((w - rw) * 0.5).floor(), ((h - rh) * 0.5).floor(), rw.round().max(1.0), rh.round().max(1.0)]
    }

    pub fn draw(&self, device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, source: &wgpu::TextureView, target: &wgpu::TextureView, rect: [f32; 4]) {
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_viewport(rect[0], rect[1], rect[2], rect[3], 0.0, 1.0);
        pass.draw(0..3, 0..1);
    }
}
