//! Checks the demo's own files load through the strict ATB readers.
use flip::data::Files;
use std::path::PathBuf;

fn data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FLIP_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    p.is_dir().then_some(p)
}

#[test]
fn every_model_converts() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).expect("need the demo's files");
    files.check().unwrap();
    for name in [
        "a_moves.3ds",
        "atat.3ds",
        "dragon.3ds",
        "ludwig.3ds",
        "skell.3ds",
        "hind24h.3ds",
    ] {
        let scene = atb::read_mesh_3ds(files.get(name).unwrap()).unwrap();
        let (bof, root) = atb::tds::to_bof(&scene, &|_| None);
        let mut items = Vec::new();
        atb::walk(&bof, root, &atb::View::default(), &mut items);
        assert!(!items.is_empty(), "{name}");
    }
    for name in ["martini.vq", "vw.vq"] {
        let text = std::str::from_utf8(files.get(name).unwrap()).unwrap();
        let chrome = atb::Rgba {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        atb::tds::vq_to_bof(text, chrome).unwrap();
    }
}

#[test]
fn pack_roundtrip() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).unwrap();
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for f in flip::demo::FILES {
        assert_eq!(files.get(f).unwrap(), packed.get(f).unwrap(), "{f}");
    }
}
