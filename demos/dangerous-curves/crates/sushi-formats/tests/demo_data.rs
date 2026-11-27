//! Reads the original demo archives when data/Sushi.ati or SUSHI_ATI is available.
use sushi_formats::{anim, archive::Archive, object, particles, path, scene};

#[test]
fn reads_original_archive() {
    let archive = std::env::var_os("SUSHI_ATI");
    let default = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/Sushi.ati");
    let data = match std::fs::read(archive.as_deref().unwrap_or(default.as_ref())) {
        Ok(data) => data,
        Err(e) if archive.is_none() && e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("read Sushi.ati: {e}"),
    };
    let a = Archive::open(&data).expect("open Sushi.ati");
    assert!(a.get("Sushi2.ini").is_some());
    assert!(a.get("Globals.ati").is_some());
    for name in a.names() {
        let data = a.get(name).unwrap();
        let result = if name.ends_with(".so2") {
            object::read_object(data).map(|_| ())
        } else if name.ends_with(".sa2") {
            anim::read_animation(data).map(|_| ())
        } else if name.ends_with(".pth") {
            path::read_path(&String::from_utf8_lossy(data)).map(|p| assert!(p.length() > 0.0))
        } else if name == "globals.ati" {
            scene::read_globals(data).map(|_| ())
        } else if name.starts_with("scenes/") && name.ends_with(".ati") {
            scene::read_scene(data).map(|_| ())
        } else if name.ends_with(".spg") || name.ends_with(".sp2") {
            particles::read_particles(data).map(|_| ())
        } else {
            continue;
        };
        result.unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}
