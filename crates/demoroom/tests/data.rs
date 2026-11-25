//! Checks that the demo's own files load and parse through `crates/atb`.
//! Needs the user's copy: `DEMOROOM_DATA`, or the workspace `demoroom/data/`.
use std::path::{Path, PathBuf};

use demoroom::data::Files;

fn data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DEMOROOM_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    p.is_dir().then_some(p)
}

#[test]
fn real_assets_parse_end_to_end() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(Path::new(&dir)).expect("need the demo's files");
    // BOF name table.
    let bof = atb::Bof::parse(files.get("demoroom.bof").unwrap()).unwrap();
    assert_eq!(bof.names().len(), 38);
    assert!(bof.find("0000.3DF").is_some());
    assert_eq!(bof.names().last().unwrap(), "demolite");
    // 3DF texture pyramid.
    let tex = atb::read_texture_3df(files.get("0000.3DF").unwrap()).unwrap();
    assert_eq!((tex.base().width, tex.base().height), (256, 256));
    assert_eq!(tex.levels.len(), 9);
    // 3DS room geometry.
    let scene = atb::read_mesh_3ds(files.get("DEMOLITE.3DS").unwrap()).unwrap();
    assert_eq!(scene.meshes.len(), 14);
    assert!(
        scene
            .meshes
            .iter()
            .any(|m| m.name == "room01" && m.faces.len() == 4)
    );
    assert!(
        scene
            .meshes
            .iter()
            .any(|m| m.name == "ankh" && m.faces.len() == 117)
    );
}
