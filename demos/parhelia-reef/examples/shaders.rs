//! Translates every vertex and pixel shader pair in a folder and compiles each: `shaders DIR`.
use parhelia_reef::d3d::{COLOR, Device, Gpu};
use parhelia_reef::shader;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: shaders DIR [VS PS]");
    if let (Some(vs), Some(ps)) = (std::env::args().nth(2), std::env::args().nth(3)) {
        let r = |n: &str| {
            String::from_utf8_lossy(&std::fs::read(format!("{dir}/{n}")).unwrap()).into_owned()
        };
        println!(
            "{}",
            shader::program(&r(&vs), &r(&ps), [false, false, false, true]).unwrap()
        );
        return;
    }
    let gpu = Gpu::headless().expect("gpu");
    let mut dev = Device::new(&gpu, COLOR, 8);
    let mut list: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    list.sort();
    let ps_default = "ps.1.1\ntex t0\nmov r0, t0\n";
    let vs_default = "vs.1.1\nmov oPos, v0\n";
    for p in list {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let src = String::from_utf8_lossy(&std::fs::read(&p).unwrap()).into_owned();
        let (vs, ps) = if name.ends_with(".vsh") {
            (src.as_str(), ps_default)
        } else {
            (vs_default, src.as_str())
        };
        match shader::program(vs, ps, [false, false, false, true]) {
            Ok(w) => {
                let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
                dev.program(&gpu, &w);
                let e = pollster::block_on(scope.pop());
                println!(
                    "{name}: {}",
                    if e.is_some() {
                        format!("INVALID {e:?}")
                    } else {
                        "ok".into()
                    }
                );
            }
            Err(e) => println!("{name}: TRANSLATE {e}"),
        }
    }
}
