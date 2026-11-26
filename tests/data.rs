//! Validates the assets when the extracted demo is available.
use bugs::{data::Files, demo, show::Vision, stage::Stage};
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

#[test]
fn terrain_and_show_follow_the_original_stage() {
    let Some(path) = std::env::var_os(demo::DATA_ENV) else {
        return;
    };
    let files = Files::from_dir(std::path::Path::new(&path)).unwrap();
    let mut models = files.models().unwrap();
    let ground = models[0]
        .geometries
        .iter()
        .find(|g| g.name == "ground")
        .unwrap();
    let points: Vec<_> = ground.points().collect();
    assert_eq!(points.len(), 101 * 101);
    let (width, height, pixels) =
        bugs::tga::read(files.get("textures/heightmap.tga").unwrap()).unwrap();
    let blue = pixels[(height / 2 * width + width / 2) * 4 + 2];
    assert_eq!(
        points[50 * 101 + 50].y,
        (f32::from(blue) / 256.0 - 0.5) * 800.0
    );

    let stage = Stage::new(&files, &mut models).unwrap();
    let mut show = stage.show(&[]);
    show.freeze(24.9, 0.0, 0.0, false);
    assert!(show.frame().bugs.is_empty());
    show.freeze(0.2, 0.0, 0.0, false);
    assert_eq!(show.frame().bugs.len(), 1);
    assert!(show.frame().bugs[0].at.x < -1700.0);
    show.freeze(90.0, 0.0, 0.0, false);
    let frame = show.frame();
    assert_eq!(frame.fov, 0.7);
    assert_eq!(frame.bugs.len(), demo::BUGS);
    assert!(
        frame
            .bugs
            .iter()
            .all(|b| b.at.is_finite() && b.heading.is_finite())
    );
    let cues = show.take_cues();
    assert!(
        cues.iter()
            .any(|c| c.name == "heli" && c.repeat && c.gain > 0.0)
    );
    assert!(
        cues.iter()
            .any(|c| c.name == "horde_rumble" && c.repeat && c.gain > 0.0)
    );
    assert!(show.key("-"));
    assert_eq!(show.frame().bugs.len(), demo::BUGS - 1);
    assert!(show.key("+"));
    assert_eq!(show.frame().bugs.len(), demo::BUGS);
    assert!(show.key("2") && show.frame().vision == Vision::Night);
    assert!(show.key("3") && show.frame().vision == Vision::Depth);
}
