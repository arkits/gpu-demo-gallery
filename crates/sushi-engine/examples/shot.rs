//! Renders one frame to a PNG: `shot <Sushi.ati> <seconds> <out.png>`.
use sushi_engine::{camera::Camera, gpu::*, scene_renderer::SceneRenderer};
use sushi_formats::{archive::Archive, path, scene};

const SIZE: (usize, usize) = (1024, 768);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: shot <Sushi.ati> <seconds> <out.png>";
    let ar = Archive::open(&std::fs::read(args.next().ok_or(usage)?)?)?;
    let time: f32 = args.next().ok_or(usage)?.parse()?;
    let out = args.next().ok_or(usage)?;
    let scene = scene::read_scene(ar.get("Scenes/Animusic.ati").ok_or("no scene")?)?;
    let fly = path::read_path(std::str::from_utf8(
        ar.get("FlyPaths/Animusic.pth").unwrap(),
    )?)?;

    let gpu = pollster::block_on(Gpu::headless())?;
    let renderer = SceneRenderer::new(&gpu, &scene, 4)?;
    let target = |format, samples, usage| {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent(SIZE.0, SIZE.1, 1),
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
    let color = target(COLOR_FORMAT, 4, attach);
    let depth = target(DEPTH_FORMAT, 4, attach);
    let resolved = target(COLOR_FORMAT, 1, attach | wgpu::TextureUsages::COPY_SRC);
    let camera = Camera::on_path(&fly, time);
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    renderer.prepare(&gpu, Camera::projection(aspect) * camera.view());
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    {
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view(&color),
                depth_slice: None,
                resolve_target: Some(&view(&resolved)),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view(&depth),
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
        });
        renderer.draw(&mut pass);
    }
    gpu.queue.submit([enc.finish()]);
    let mut png = png::Encoder::new(std::fs::File::create(&out)?, SIZE.0 as u32, SIZE.1 as u32);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?
        .write_image_data(&gpu.read_rgba(&resolved))?;
    println!("wrote {out}: t={time} camera {camera:?}");
    Ok(())
}
