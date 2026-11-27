//! Renders one frame: `shot DATA STEPS OUT.png [KEYS [AZIMUTH]]`. Keys are pressed
//! first; `@N` waits N steps.
use box_of_smoke::{App, data::Files, gpu};

const SIZE: (u32, u32) = (1024, 768);

fn main() -> Result<(), box_of_smoke::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, steps, out, rest @ ..] = args.as_slice() else {
        return Err("usage: shot DATA STEPS OUT.png [KEYS [AZIMUTH]]".into());
    };
    let data = std::path::Path::new(data);
    let files = match data.is_dir() {
        true => Files::from_dir(data)?,
        false => Files::open(&std::fs::read(data)?)?,
    };
    let mut app = App::new(gpu::Gpu::headless()?, &files, gpu::COLOR)?;
    let run = |app: &mut App| {
        let mut enc = app.gpu.device.create_command_encoder(&Default::default());
        app.sim.begin();
        app.step(&mut enc);
        app.gpu.queue.submit([enc.finish()]);
    };
    if let Some(a) = rest.get(1) {
        app.controls.rotate(a.parse::<f32>()? / 2.0, 0.0);
    }
    for word in rest.first().map_or("", |k| k).split(',') {
        match word.strip_prefix('@') {
            Some(n) => (0..n.parse()?).for_each(|_| run(&mut app)),
            None => {
                app.controls.key(word);
            }
        }
    }
    for _ in 0..steps.parse()? {
        run(&mut app);
    }
    let target = app.gpu.target(SIZE.0, SIZE.1, gpu::COLOR);
    let mut enc = app.gpu.device.create_command_encoder(&Default::default());
    app.draw(&mut enc, &target.create_view(&Default::default()), SIZE);
    app.gpu.queue.submit([enc.finish()]);
    let rgba = app.gpu.read(&target);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut png = png::Encoder::new(std::fs::File::create(out)?, SIZE.0, SIZE.1);
    png.set_color(png::ColorType::Rgb);
    png.write_header()?.write_image_data(&rgb)?;
    println!("wrote {out}");
    Ok(())
}
