//! Renders one frame: `shot DATA SECONDS YAW PITCH ACTION OUT.png`.
use tidepool::{data::Files, gpu, scene::Scene};

const SIZE: (u32, u32) = (1024, 768);

fn main() -> Result<(), tidepool::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, seconds, yaw, pitch, action, out] = args.as_slice() else {
        return Err("usage: shot DATA SECONDS YAW PITCH ACTION OUT.png".into());
    };
    let data = std::path::Path::new(data);
    let files = match data.is_dir() {
        true => Files::from_dir(data)?,
        false => Files::open(&std::fs::read(data)?)?,
    };
    let g = gpu::Gpu::headless()?;
    let mut scene = Scene::new(&g, &files, gpu::COLOR)?;
    let mut show = scene.show();
    show.freeze(
        seconds.parse()?,
        yaw.parse()?,
        pitch.parse()?,
        action == "1",
    );
    let target = g.target(SIZE.0, SIZE.1, gpu::COLOR, 1);
    let mut enc = g.device.create_command_encoder(&Default::default());
    let view = target.create_view(&Default::default());
    scene.draw(&g, &mut enc, &show.frame(), &view, SIZE);
    g.queue.submit([enc.finish()]);
    let rgba = g.read(&target);
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
