//! Lists the Cg programs the scenes name, one JSON object per line, for
//! `tools/nv_compile.py`: `cargo run --example programs -- INSTALL_FOLDER`.
use nv_engine::load::path_of;
use nv_engine::world::{PendingNames, World};

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("demo folder"));
    let read = |p: &str| std::fs::read(root.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let args = gas_station::args::parse(&String::from_utf8_lossy(&read("bin/args.txt"))).expect("args.txt");
    let mut world = World::default();
    let mut pending = PendingNames::default();
    for s in &args.scenes {
        let node = nv_scene::read(&read(&path_of(s)), nv_scene::Viewer::GasStation).expect("scene");
        world.add_scene(&node, &mut pending);
    }
    world.resolve(pending);
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    for s in &world.shaders {
        let entry = if s.entry.is_empty() { "main" } else { &s.entry };
        println!("{{\"filename\": {}, \"entry\": {}, \"profile\": {}}}", q(&s.filename), q(entry), s.profile);
    }
}
