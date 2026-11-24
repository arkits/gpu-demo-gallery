//! Checks against the user's demo files; skipped when they are absent.
use box_of_smoke::data::{Files, read_obj, read_path};

#[test]
fn models_load() {
    let Ok(files) = Files::from_dir(std::path::Path::new("data")) else {
        eprintln!("skipped: no demo files in data/");
        return;
    };
    let text = |n: &str| {
        std::str::from_utf8(files.get(n).unwrap())
            .unwrap()
            .to_owned()
    };
    for (name, triangles) in [
        ("stand", 33768),
        ("cushion", 6000),
        ("shadow", 14),
        ("glass", 505),
    ] {
        let m = read_obj(&text(&format!("models/{name}.obj")));
        assert_eq!(
            m.unwrap_or_else(|e| panic!("{name}: {e}")).indices.len(),
            triangles * 3,
            "{name}"
        );
    }
    assert_eq!(read_path(&text("models/trapped.obj")).unwrap().len(), 2074);
}
