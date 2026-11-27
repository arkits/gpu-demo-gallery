//! Renders one frame: `shot <Sushi.ati> <chapter> <seconds> <out.png>`.
use sushi_engine::{
    demo::Demo,
    gpu::Gpu,
    overlay::{Overlay, read_tga},
    targets::Targets,
};

const SIZE: (u32, u32) = (1024, 768);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, chapter, time, out] = args.as_slice() else {
        return Err("usage: shot <Sushi.ati> <chapter> <seconds> <out.png>".into());
    };
    let chapter = chapter
        .parse::<usize>()?
        .checked_sub(1)
        .ok_or("chapters start at 1")?;
    let time: f32 = time.parse()?;
    if !time.is_finite() || time < 0.0 {
        return Err("seconds must be finite and nonnegative".into());
    }
    let ten = std::env::var_os("SUSHI_TEN_BIT").is_some();
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device
        .on_uncaptured_error(std::sync::Arc::new(|e| eprintln!("gpu error: {e}")));
    gpu.device
        .set_device_lost_callback(|r, m| eprintln!("device lost: {r:?} {m}"));
    let mut demo = Demo::load(&gpu, &[&std::fs::read(data)?], ten)?;
    if std::env::var_os("SUSHI_STEP").is_some() {
        gpu.device.poll(wgpu::PollType::wait_indefinitely()).ok();
        eprintln!("loaded");
    }
    if chapter >= demo.chapters().len() {
        return Err("chapter does not exist".into());
    }
    if let Some(dir) = std::path::Path::new(data).parent()
        && let Ok(bytes) = std::fs::read(dir.join("Logo2.tga"))
        && let Some(logo) = read_tga(&bytes)
    {
        demo.set_logo(&gpu, &logo);
    }
    let targets = Targets::new(&gpu, SIZE.0, SIZE.1, ten);
    let mut overlay = Overlay::new(&gpu, targets.color.format());
    overlay.set_font(&gpu, demo.font());
    // SUSHI_RUN=seconds draws the frames before too, at 30 a second, for effects that
    // build on earlier frames (simulations on buffers).
    if let Some(run) = std::env::var("SUSHI_RUN")
        .ok()
        .and_then(|r| r.parse::<f32>().ok())
    {
        let mut t = (time - run).max(0.0);
        while t < time {
            let mut enc = gpu.device.create_command_encoder(&Default::default());
            demo.draw(&gpu, &mut enc, &targets, chapter, t, None);
            gpu.queue.submit([enc.finish()]);
            t += 1.0 / 30.0;
        }
    }
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    let marks = demo.draw(&gpu, &mut enc, &targets, chapter, time, None);
    overlay.draw(&gpu, &mut enc, &targets, &marks, false, 1.0);
    gpu.queue.submit([enc.finish()]);
    if std::env::var_os("SUSHI_STEP").is_some() {
        gpu.device.poll(wgpu::PollType::wait_indefinitely()).ok();
        eprintln!("drawn");
    }

    if let Ok(names) = std::env::var("SUSHI_DUMP") {
        for n in names.split(',') {
            let t = demo.script_buffer(n).ok_or("no such buffer")?;
            let mut png = png::Encoder::new(
                std::fs::File::create(format!("{out}.{n}.png"))?,
                t.width(),
                t.height(),
            );
            png.set_color(png::ColorType::Rgba);
            png.write_header()?.write_image_data(&gpu.read_rgba(t))?;
        }
    }
    let mut png = png::Encoder::new(std::fs::File::create(out)?, SIZE.0, SIZE.1);
    png.set_color(png::ColorType::Rgb);
    // Alpha is not coverage.
    let rgba = gpu.read_rgba(&targets.color.resolved);
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
