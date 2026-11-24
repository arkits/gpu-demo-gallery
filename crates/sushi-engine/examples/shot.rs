//! Renders one frame: `shot <Sushi.ati> <chapter 1-5> <seconds> <out.png>`.
use sushi_engine::{demo::Demo, gpu::Gpu, overlay::Overlay, targets::Targets};

const SIZE: (u32, u32) = (1024, 768);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, chapter, time, out] = args.as_slice() else {
        return Err("usage: shot <Sushi.ati> <chapter 1-5> <seconds> <out.png>".into());
    };
    let chapter = chapter.parse::<usize>()?.max(1) - 1;
    let gpu = pollster::block_on(Gpu::headless())?;
    let mut demo = Demo::load(&gpu, &std::fs::read(data)?)?;
    let mut overlay = Overlay::new(&gpu);
    overlay.set_font(&gpu, demo.font());
    let targets = Targets::new(&gpu, SIZE.0, SIZE.1);
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    let marks = demo.draw(&gpu, &mut enc, &targets, chapter, time.parse()?, None);
    overlay.draw(&gpu, &mut enc, &targets, &marks, false, 1.0);
    gpu.queue.submit([enc.finish()]);

    let mut png = png::Encoder::new(std::fs::File::create(out)?, SIZE.0, SIZE.1);
    png.set_color(png::ColorType::Rgb);
    // Alpha is not coverage.
    let rgba = gpu.read_rgba(&targets.resolved);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    png.write_header()?.write_image_data(&rgb)?;
    demo.warnings().for_each(|w| eprintln!("warning: {w}"));
    let c = &demo.chapters()[chapter];
    println!(
        "wrote {out}: {} at {time} of {:.2}, {} draws",
        c.title,
        c.duration(),
        demo.draw_calls()
    );
    Ok(())
}
