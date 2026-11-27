//! Draws every chapter at fixed steps and reports problems: `sweep <Sushi.ati> [step]`.
use sushi_engine::{demo::Demo, gpu::Gpu, targets::Targets};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let data = std::fs::read(args.next().ok_or("usage: sweep <Sushi.ati> [step]")?)?;
    let step: f32 = args.next().map_or(Ok(0.5), |s| s.parse())?;
    if !step.is_finite() || step <= 0.0 {
        return Err("step must be finite and positive".into());
    }
    let ten = std::env::var_os("SUSHI_TEN_BIT").is_some();
    let gpu = pollster::block_on(Gpu::headless())?;
    let mut demo = Demo::load(&gpu, &[&data], ten)?;
    let targets = Targets::new(&gpu, 256, 192, ten);
    let (mut frames, mut dark) = (0, 0);
    for c in 0..demo.chapters().len() {
        let n = (demo.chapters()[c].duration() / step).ceil() as usize;
        for i in 0..n {
            let mut enc = gpu.device.create_command_encoder(&Default::default());
            demo.draw(&gpu, &mut enc, &targets, c, i as f32 * step, None);
            gpu.queue.submit([enc.finish()]);
            let rgba = gpu.read_rgba(&targets.color.resolved);
            let lit = rgba
                .chunks_exact(4)
                .filter(|p| p[0].max(p[1]).max(p[2]) > 40);
            if lit.count() < 256 * 192 / 50 {
                println!(
                    "near-black: {} at {:.2}s",
                    demo.chapters()[c].title,
                    i as f32 * step
                );
                dark += 1;
            }
            frames += 1;
        }
    }
    demo.warnings().for_each(|w| println!("warning: {w}"));
    if dark > 0 {
        println!("warning: {dark} frames are almost black");
    }
    println!("{frames} frames");
    Ok(())
}
