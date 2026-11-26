//! Validates the assets when the extracted demo is available.
use chameleon::{data::Files, demo, stage::Stage};
use wgpu::naga;

#[test]
fn assets_parse_and_round_trip() {
    let Some(path) = std::env::var_os(demo::DATA_ENV) else {
        return;
    };
    let files = Files::from_dir(std::path::Path::new(&path)).unwrap();
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for path in files.needed().unwrap() {
        assert_eq!(
            files.get(&path).unwrap(),
            packed.get(&path).unwrap(),
            "{path}"
        );
    }
    let mut models = files.models().unwrap();
    // Nothing reads the picture's alpha, which is never written.
    for pass in models
        .iter()
        .flat_map(|m| &m.shapes)
        .flat_map(|s| &s.passes)
    {
        let reads = |n: u32| (8..=10).contains(&n);
        assert!(pass.state.blend.is_none_or(|(s, d)| !reads(s) && !reads(d)));
    }
    for (model, file) in models.iter().zip(demo::MODELS) {
        assert!(!model.shapes.is_empty(), "{file}");
        for ms in (0..40_000).step_by(170) {
            let times = model.animations.iter().map(|a| (a, f64::from(ms)));
            assert!(
                model.pose(times).iter().all(|m| m.is_finite()),
                "{file} at {ms}"
            );
        }
    }
    // Every program of every pass is valid.
    Stage::new(&files, &mut models).unwrap();
    let mut seen = std::collections::HashSet::new();
    for shape in models.iter().flat_map(|m| &m.shapes) {
        for pass in &shape.passes {
            if !seen.insert(&pass.wgsl) {
                continue;
            }
            let module = naga::front::wgsl::parse_str(&pass.wgsl)
                .unwrap_or_else(|e| panic!("{}: {}", shape.name, e.emit_to_string(&pass.wgsl)));
            naga::valid::Validator::new(Default::default(), Default::default())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{}: {e:?}", shape.name));
        }
    }
    assert!(seen.len() > 1);
}
