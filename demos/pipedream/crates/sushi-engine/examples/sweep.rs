//! Renders the whole timeline offscreen and reports problems and cost:
//! `sweep <Sushi.ati> [step seconds]`.
use std::time::Instant;

use sushi_engine::{demo::Demo, gpu::Gpu, targets::Targets};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let data = std::fs::read(args.next().ok_or("usage: sweep <Sushi.ati> [step]")?)?;
    let step: f32 = args.next().map_or(Ok(0.5), |s| s.parse())?;
    let gpu = pollster::block_on(Gpu::headless())?;
    let t = Instant::now();
    let mut demo = Demo::load(&gpu, &data)?;
    println!("load {:?}", t.elapsed());
    let targets = Targets::new(&gpu, 1024, 768);
    let (mut n, mut cpu, mut worst, mut calls) = (0, 0.0, (0.0f64, 0.0f32), 0);
    let mut time = 0.0;
    while time < demo.duration() {
        let t = Instant::now();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        demo.draw(&gpu, &mut enc, &targets, time);
        gpu.queue.submit([enc.finish()]);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        if n > 0 && ms > worst.0 {
            worst = (ms, time);
        }
        cpu += ms;
        calls = calls.max(demo.draw_calls());
        n += 1;
        time += step;
    }
    println!(
        "{n} frames, cpu mean {:.2} ms, worst {:.2} ms at t={}, up to {calls} draws",
        cpu / f64::from(n),
        worst.0,
        worst.1
    );
    demo.warnings().for_each(|w| println!("warning: {w}"));
    Ok(())
}
