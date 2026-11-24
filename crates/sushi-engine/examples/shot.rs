//! Renders one frame to a PNG: `shot <Sushi.ati> <seconds> <out.png>`.
use sushi_engine::{demo::Demo, gpu::Gpu, targets::Targets};

const SIZE: (u32, u32) = (1024, 768);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: shot <Sushi.ati> <seconds> <out.png>";
    let data = std::fs::read(args.next().ok_or(usage)?)?;
    let time: f32 = args.next().ok_or(usage)?.parse()?;
    let out = args.next().ok_or(usage)?;

    let gpu = pollster::block_on(Gpu::headless())?;
    let demo = Demo::load(&gpu, &data)?;
    let targets = Targets::new(&gpu, SIZE.0, SIZE.1);
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    demo.draw(&gpu, &mut enc, &targets, time);
    gpu.queue.submit([enc.finish()]);

    let mut png = png::Encoder::new(std::fs::File::create(&out)?, SIZE.0, SIZE.1);
    png.set_color(png::ColorType::Rgb);
    png.set_depth(png::BitDepth::Eight);
    // Alpha carries shadow data, not coverage.
    let rgba = gpu.read_rgba(&targets.resolved);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    png.write_header()?.write_image_data(&rgb)?;
    println!("wrote {out} at t={time} of {:.2}", demo.duration());
    Ok(())
}
