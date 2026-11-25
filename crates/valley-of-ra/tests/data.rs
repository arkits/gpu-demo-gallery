//! Checks the demo's own files load through the strict ATB readers.
use std::path::PathBuf;
use valley_of_ra::data::Files;

fn data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("VALLEY_OF_RA_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    p.is_dir().then_some(p)
}

#[test]
fn real_assets_load() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).expect("need the demo's files");
    files.check().unwrap();
    let scene = atb::Scene::load(&files).unwrap();
    assert!(!scene.dict.is_empty() || !scene.meshes.is_empty() || !scene.textures.is_empty());
}

#[test]
fn pack_roundtrip() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).unwrap();
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for f in valley_of_ra::demo::FILES {
        assert_eq!(files.get(f).unwrap(), packed.get(f).unwrap(), "{f}");
    }
}
