//! Validates the assets when the extracted demo is available.
use wgpu::naga;
use zoltar::{data::Files, demo, stage::Stage, tree::read_binary};

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
    // Nothing reads the picture's alpha, which is never written.
    for pass in models
        .iter()
        .flat_map(|m| &m.shapes)
        .flat_map(|s| &s.passes)
    {
        let reads = |n: u32| (8..=10).contains(&n);
        assert!(pass.state.blend.is_none_or(|(s, d)| !reads(s) && !reads(d)));
    }
    // Every program of every pass is valid, and every clip fits its meshes.
    let stage = Stage::new(&files, &mut models).unwrap();
    stage.check_clips(&models[0]).unwrap();
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

/// The clips are as long as the table says, and move one mesh.
#[test]
fn clips_have_their_lengths() {
    let Some(path) = std::env::var_os(demo::DATA_ENV) else {
        return;
    };
    let files = Files::from_dir(std::path::Path::new(&path)).unwrap();
    assert_eq!(demo::CLIPS.len(), demo::IDLE.end);
    for clip in demo::CLIPS {
        let root = read_binary(files.get(&format!("models/{}", clip.scene)).unwrap()).unwrap();
        let [animation] = root.nodes("animations") else {
            panic!("{}: not one animation", clip.scene);
        };
        let (mut end, mut meshes) = (0, 0);
        for channel in animation.nodes("channels") {
            let keys = channel.nodes("curves");
            meshes += usize::from(!keys.is_empty());
            let mesh = keys.iter().map(|k| k.int("time"));
            let curves = ["xCurve", "yCurve", "zCurve"];
            let curves = curves.iter().filter_map(|f| channel.node(f));
            end = mesh
                .chain(curves.map(|c| c.int("stopTime")))
                .fold(end, u32::max);
        }
        assert_eq!(meshes, 1, "{}", clip.scene);
        assert_eq!(end as f32 / 1000.0, clip.length, "{}", clip.scene);
    }
}
