//! Every scene under `data/models` reads to its last byte.
use std::path::PathBuf;

#[test]
fn scenes_read_to_the_end() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/models");
    let viewer = match std::env::var("NV_VIEWER").as_deref() {
        Ok("vulcan") => nv_scene::Viewer::Vulcan,
        Ok("tm") => nv_scene::Viewer::TimeMachine,
        Ok("toys") => nv_scene::Viewer::Toys,
        _ => nv_scene::Viewer::GasStation,
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("no {}", dir.display());
        return;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "nmb") {
            let d = std::fs::read(&p).unwrap();
            let s = nv_scene::read(&d, viewer).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert_eq!(s.kind, "Scene");
            n += 1;
        }
    }
    assert!(n > 0);
    eprintln!("{n} scenes");
}
