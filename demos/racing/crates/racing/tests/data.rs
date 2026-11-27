//! Checks the demo's own files load through the strict readers.
use racing::data::Files;
use std::path::PathBuf;

fn data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RACING_DATA") {
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
    let bof = atb::Bof::parse(files.get("race.bof").unwrap()).unwrap();
    for name in ["track_noroad", "sgi_indy", "mg_indy", "nin_indy", "vp_indy"] {
        assert!(bof.find(name).is_some(), "{name}");
    }
}

#[test]
fn pack_roundtrip() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).unwrap();
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for f in racing::demo::FILES {
        assert_eq!(files.get(f).unwrap(), packed.get(f).unwrap(), "{f}");
    }
}
