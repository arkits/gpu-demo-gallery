//! Renders one frame: `shot DATA SECONDS TAKE YAW PITCH KEY OUT.png [WIDTHxHEIGHT]`.
//! A key of `-` is none.
use mad_mod_mike::{data::Files, demo, gpu, scene::Scene};

const SIZE: (u32, u32) = (1024, 768);

fn main() -> Result<(), mad_mod_mike::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, seconds, take, yaw, pitch, key, out, rest @ ..] = args.as_slice() else {
        return Err("usage: shot DATA SECONDS TAKE YAW PITCH KEY OUT.png [WIDTHxHEIGHT]".into());
    };
    let size = match rest.first().and_then(|s| s.split_once('x')) {
        Some((w, h)) => (w.parse()?, h.parse()?),
        None => SIZE,
    };
    let data = std::path::Path::new(data);
    let files = match data.is_dir() {
        true => Files::from_dir(data)?,
        false => Files::open(&std::fs::read(data)?)?,
    };
    let g = gpu::Gpu::headless()?;
    let mut scene = Scene::new(&g, &files, gpu::COLOR)?;
    let mut show = scene.show();
    let take: usize = take.parse()?;
    assert!(take < demo::TAKES.len(), "take out of range");
    show.key(&((take + 1) % 10).to_string());
    show.freeze(
        seconds.parse()?,
        yaw.parse()?,
        pitch.parse()?,
        key.trim_matches('-'),
    );
    let target = g.target(size.0, size.1, gpu::COLOR, 1);
    let mut enc = g.device.create_command_encoder(&Default::default());
    let view = target.create_view(&Default::default());
    scene.draw(&g, &mut enc, &show.frame(), &view, size);
    g.queue.submit([enc.finish()]);
    let rgba = g.read(&target);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut png = png::Encoder::new(std::fs::File::create(out)?, size.0, size.1);
    png.set_color(png::ColorType::Rgb);
    png.write_header()?.write_image_data(&rgb)?;
    println!("wrote {out}");
    Ok(())
}
