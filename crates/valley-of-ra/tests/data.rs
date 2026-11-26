//! Checks the demo's own files load through the strict readers.
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
fn scene_reads() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).expect("need the demo's files");
    let bof = atb::Bof::parse(files.get("fight.bof").unwrap()).unwrap();
    for name in [
        "an_moves",
        "nef_moves",
        "refl",
        "top",
        "background",
        "scene",
    ] {
        assert!(bof.find(name).is_some(), "{name}");
    }
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
