//! Draws every chapter at fixed steps and reports problems: `sweep <Sushi.ati> [step]`.
use sushi_engine::{demo::Demo, gpu::Gpu, targets::Targets};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let data = std::fs::read(args.next().ok_or("usage: sweep <Sushi.ati> [step]")?)?;
    let step: f32 = args.next().map_or(Ok(0.5), |s| s.parse())?;
    let gpu = pollster::block_on(Gpu::headless())?;
    let mut demo = Demo::load(&gpu, &data)?;
    let targets = Targets::new(&gpu, 256, 192);
    let (mut frames, mut dark) = (0, 0);
    for c in 0..demo.chapters().len() {
        let n = (demo.chapters()[c].duration() / step) as usize;
        for i in 0..n {
            let mut enc = gpu.device.create_command_encoder(&Default::default());
            demo.draw(&gpu, &mut enc, &targets, c, i as f32 * step, None);
            gpu.queue.submit([enc.finish()]);
            let rgba = gpu.read_rgba(&targets.resolved);
            let lit = rgba
                .chunks_exact(4)
                .filter(|p| p[0].max(p[1]).max(p[2]) > 40);
            dark += usize::from(lit.count() < 256 * 192 / 50);
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
