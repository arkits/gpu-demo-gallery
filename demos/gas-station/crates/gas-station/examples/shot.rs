//! Renders frames of the demo to PNG: `cargo run --example shot -- SECONDS [OUT.png]`.
use std::path::PathBuf;

use nv_engine::gl::{Gl, Gpu};
use nv_engine::load::{self, Assets};

struct Dir(PathBuf);
impl Assets for Dir {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(path)).ok()
    }
}

fn main() {
    let t: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let out = std::env::args().nth(2).unwrap_or_else(|| "target/shot.png".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let assets = Dir(root);
    let args = gas_station::args::parse(&String::from_utf8_lossy(&assets.read("bin/args.txt").unwrap())).unwrap();
    let scenes: Vec<String> = args.scenes.iter().map(|s| load::path_of(s)).collect();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    let gpu = pollster::block_on(Gpu::from_adapter(&adapter)).unwrap();
    gpu.device.on_uncaptured_error(std::sync::Arc::new(|e| eprintln!("wgpu: {e}")));
    let gl = Gl::new(gpu, args.window);
    let opts = load::LoadOptions {
        schema: nv_scene::Viewer::GasStation,
        scenes,
        window: args.window,
        depth_formats: &[(14, wgpu::TextureFormat::Depth16Unorm), (15, wgpu::TextureFormat::Depth24Plus)],
    };
    let start = std::time::Instant::now();
    let mut v = load::load(&assets, gl, &opts, &mut |s| eprintln!("loading {s}")).unwrap();
    eprintln!("loaded in {:?}", start.elapsed());
    if std::env::var("NV_PASSES").is_ok() {
        for p in &v.world.passes {
            let t = |i: Option<usize>| i.map(|i| { let t = &v.world.textures[i]; format!("{} {:?} fmt {}", t.name, t.source, t.format) });
            eprintln!("pass {} color {:?} depth {:?} cam {:?} scene_shapes {}", p.name, t(p.color), t(p.depth), p.camera, p.use_scene_shapes);
        }
    }
    if std::env::var("NV_MENUS").is_ok() {
        eprintln!("{:#?}", v.world.menus);
    }
    let mut demo = gas_station::demo::Demo::new(&mut v);
    if let Some(c) = assets.read("textures/collision1.tga") {
        demo.set_collision(&c);
    }
    // NV_KEYS: characters typed at the start; NV_CLICK=x,y;... presses at 0.5 s.
    if let Ok(k) = std::env::var("NV_KEYS") {
        for c in k.chars() {
            demo.key(&mut v, c, 0.0);
        }
    }
    if let Ok(k) = std::env::var("NV_SKIP") {
        v.skip = k.split(',').map(String::from).collect();
    }
    if std::env::var("NV_LIST").is_ok() {
        for s in &v.world.shapes {
            let st = &s.settings[0];
            let name = |i: Option<usize>| i.map(|i| v.world.shaders[i].name.clone()).unwrap_or_default();
            eprintln!("{} set {} type {} {} {} r {}", s.name, st.sort_set, st.sort_type, name(st.vertex), name(st.fragment), s.radius);
        }
    }
    // The movie starts after 30 s without input; T is seconds into it (negative: the
    // walking camera at 1 s). The clock steps by 0.1 s as frames would.
    let end = if t < 0.0 { 1.0 } else { 30.05 + t };
    let mut now = 0.0;
    let clicks: Vec<(f32, f32)> = std::env::var("NV_CLICK").ok().map(|s| s.split(';').filter_map(|p| { let (a, b) = p.split_once(',')?; Some((a.parse().ok()?, b.parse().ok()?)) }).collect()).unwrap_or_default();
    let mut clicked = false;
    while now < end {
        if !clicked && now >= 0.5 {
            clicked = true;
            for &(x, y) in &clicks {
                demo.mouse_button(&mut v, 0, true, x, y, now);
                demo.mouse_button(&mut v, 0, false, x, y, now);
            }
        }
        demo.frame(&mut v, now);
        now += 0.1;
    }
    demo.frame(&mut v, end);
    v.draw(&mut demo).unwrap();
    if std::env::var("NV_DEBUG").is_ok() {
        for (i, c) in v.world.cameras.iter().enumerate() {
            eprintln!("camera {i} {} eye {:?} dir {:?} fov {}", c.name, c.eye, c.dir, c.fov);
        }
        for tr in &v.world.transforms {
            if tr.name.contains("cam") {
                eprintln!("{} {:?}", tr.name, tr.kind);
            }
        }
        eprintln!("day {} anim {:?}", demo.day, demo.anim);
    }
    // NV_DUMP=texture name: write that render target instead of the window.
    let dump = std::env::var("NV_DUMP").ok();
    let src = match dump.as_deref().and_then(|n| v.world.texture(n)).and_then(|t| v.textures[t]) {
        Some(t) => t,
        None => usize::MAX,
    };
    let (w, h) = if src == usize::MAX {
        v.gl.window
    } else {
        let t = v.gl.textures[src].as_ref().unwrap();
        (t.width, t.height)
    };
    let buffer = v.gl.gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (w * h * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    v.gl.submit(|enc, gl| {
        enc.copy_texture_to_buffer(
            if src == usize::MAX { gl.window_color.texture.as_image_copy() } else { gl.textures[src].as_ref().unwrap().texture.as_image_copy() },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    });
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    v.gl.gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    // OpenGL's rows start at the bottom.
    let mut img = vec![0u8; data.len()];
    for y in 0..h as usize {
        let src = &data[y * w as usize * 4..(y + 1) * w as usize * 4];
        img[(h as usize - 1 - y) * w as usize * 4..(h as usize - y) * w as usize * 4].copy_from_slice(src);
    }
    // Alpha apart: the demo keeps HDR data in it.
    let alpha: Vec<u8> = img.chunks_exact(4).flat_map(|p| [p[3], p[3], p[3], 255]).collect();
    for p in img.chunks_exact_mut(4) {
        p[3] = 255;
    }
    {
        let f = std::fs::File::create(out.replace(".png", "-alpha.png")).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&alpha).unwrap();
    }
    let f = std::fs::File::create(&out).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.write_header().unwrap().write_image_data(&img).unwrap();
    eprintln!("{} draws, wrote {out}", v.draws);
}
