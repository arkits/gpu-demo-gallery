//! Packs what the demo reads: `cargo run --example pack -- DEMO_FOLDER OUT [verify]`.
//! The demo is loaded once to learn which files it reads; with `verify`, OUT (a pack,
//! possibly compressed) is opened and loaded instead.
use gas_station::files::{Files, Recorder};
use nv_engine::gl::Gpu;

fn gpu() -> Gpu {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    pollster::block_on(Gpu::from_adapter(&adapter)).expect("device")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (dir, out) = (&a[0], &a[1]);
    if a.get(2).map(String::as_str) == Some("verify") {
        let files = Files::open(&std::fs::read(out).expect("pack")).expect("pack");
        gas_station::start(&files, gpu(), Some(0), &mut |_| {}).expect("demo loads from the pack");
        eprintln!("{out}: {} files, loads", files.names().count());
        return;
    }
    let files = Files::from_dir(std::path::Path::new(dir)).expect("demo folder");
    let rec = Recorder { files: &files, read: Default::default() };
    gas_station::start(&rec, gpu(), Some(0), &mut |_| {}).expect("demo loads");
    let names: Vec<String> = rec.read.borrow().iter().cloned().collect();
    let pack = files.pack(&names).expect("pack");
    std::fs::write(out, &pack).expect("write");
    eprintln!("{out}: {} files, {} bytes", names.len(), pack.len());
}
