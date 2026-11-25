//! Validate the assets when the corresponding extracted demo is available.
use tidepool::{data::Files, demo};

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
    for file in demo::MODELS {
        let model = files.model(file).unwrap();
        assert!(!model.shapes.is_empty(), "{file}");
        let (min, max) = model.bounds();
        assert!(
            min.is_finite() && max.is_finite() && min.cmple(max).all(),
            "{file}"
        );
        for clip in &model.animations {
            for ms in (0..15_000).step_by(17) {
                assert!(
                    model
                        .pose(Some(clip), f64::from(ms))
                        .iter()
                        .all(|m| m.is_finite()),
                    "{file} at {ms}"
                );
            }
        }
    }
}
