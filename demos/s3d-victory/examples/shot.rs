//! Renders one frame: `shot DATA SECONDS KEYS OUT.png [WIDTHxHEIGHT]`.
use s3d_victory::demo::{Demo, PROGRAMS, SHADER};
use s3d_victory::gl::{COLOR, Gl, Gpu};
use s3d_victory::{Error, data::Files};

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, seconds, keys, out, rest @ ..] = args.as_slice() else {
        return Err("usage: shot DATA SECONDS KEYS OUT.png [WIDTHxHEIGHT]".into());
    };
    let size = match rest.first().and_then(|s| s.split_once('x')) {
        Some((w, h)) => (w.parse()?, h.parse()?),
        None => (1024, 768),
    };
    let files = Files::from_path(std::path::Path::new(data))?;
    let gpu = Gpu::headless()?;
    let mut gl = Gl::new(&gpu, SHADER, PROGRAMS, COLOR);
    let mut demo = Demo::new(&gpu, &mut gl, &files)?;
    demo.freeze(seconds.parse()?, keys);
    let target = gpu.target(size, COLOR, 1);
    demo.draw(&gpu, &mut gl, size);
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    gl.finish(
        &gpu,
        &mut enc,
        &target.create_view(&Default::default()),
        size,
    );
    gpu.queue.submit([enc.finish()]);
    let rgba = gpu.read(&target);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut png = png::Encoder::new(std::fs::File::create(out)?, size.0, size.1);
    png.set_color(png::ColorType::Rgb);
    png.write_header()?.write_image_data(&rgb)?;
    println!("wrote {out}: {}", demo.report());
    Ok(())
}
