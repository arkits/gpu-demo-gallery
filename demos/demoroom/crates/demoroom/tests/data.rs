//! Checks that the demo's own file loads through `crates/atb`.
//! Needs the user's copy: `DEMOROOM_DATA`, or the workspace `demoroom/data/`.
use std::path::{Path, PathBuf};

use atb::bof::Object;
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
fn scene_reads_and_walks() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(Path::new(&dir)).expect("need the demo's files");
    let bof = atb::Bof::parse(files.get("demoroom.bof").unwrap()).unwrap();
    assert_eq!(bof.dict.len(), 38);
    let root = bof.find("demolite").unwrap();
    assert!(matches!(bof.get(root), Some(Object::Anim(_))));
    let mut items = Vec::new();
    atb::walk(&bof, root, &atb::View::default(), &mut items);
    assert_eq!(
        items.len(),
        7,
        "room, side walls, floor, ceiling, top wall, ankh, its light"
    );
    for name in ["projblue.3df", "projgord.3df", "0019.3df", "helpscreen"] {
        let id = bof.find(name).unwrap();
        assert!(bof.image(id).unwrap().rgba().is_ok(), "{name}");
    }
}
