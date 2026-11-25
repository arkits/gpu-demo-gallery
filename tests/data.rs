//! Validate the assets when the corresponding extracted demo is available.
use toy_soldier::{data::Files, demo};

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
        for (clip_index, clip) in model.animations.iter().enumerate() {
            let mut previous = model.pose(Some(clip), 0.0);
            // Check motion as well as finite values. The root travel used to snap
            // back 8–21 units when an otherwise cyclic clip restarted.
            for ms in 1..4_500 {
                let current = model.pose(Some(clip), f64::from(ms));
                for (i, (a, b)) in previous.iter().zip(&current).enumerate() {
                    assert!(b.is_finite(), "{file} at {ms}");
                    assert!(
                        a.w_axis.distance(b.w_axis) < 0.75,
                        "{file} clip {clip_index}, {} at {ms} ms moved from {:?} to {:?}",
                        model.nodes[i].name,
                        a.w_axis,
                        b.w_axis
                    );
                }
                previous = current;
            }
        }
    }
}
