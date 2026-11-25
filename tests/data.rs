//! Validates the assets when the extracted demo is available.
use dawn::{
    data::Files,
    demo,
    model::{Geometry, Model},
    tree,
};

fn files() -> Option<Files> {
    let path = std::env::var_os(demo::DATA_ENV)?;
    Some(Files::from_dir(std::path::Path::new(&path)).unwrap())
}

fn model(files: &Files, name: &str) -> Model {
    Model::new(&tree::read_binary(files.get(name).unwrap()).unwrap(), files).unwrap()
}

#[test]
fn assets_parse_and_round_trip() {
    let Some(files) = files() else { return };
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for path in files.needed().unwrap() {
        assert_eq!(
            files.get(&path).unwrap(),
            packed.get(&path).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn the_fairy_rests_as_modelled_and_moves() {
    let Some(files) = files() else { return };
    let fairy = model(&files, demo::FAIRY);
    assert_eq!(fairy.shapes.len(), 86);
    let rest = fairy.pose(None, &[]);
    for shape in &fairy.shapes {
        let Geometry::Mesh(mesh) = &shape.geometry else {
            continue;
        };
        for (bone, (_, bends)) in fairy.bones(shape, &rest).iter().zip(&shape.bones) {
            let off = *bone - glam::Mat4::IDENTITY;
            let most = off
                .to_cols_array()
                .iter()
                .fold(0.0_f32, |m, x| m.max(x.abs()));
            assert!(!bends || most < 1e-3, "{} {most}", shape.name);
        }
        for f in &mesh.fixed {
            let sum: f32 = f.weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-3, "{}", shape.name);
        }
    }
    let face = |s: &&dawn::model::Shape| s.name == "FaceSmoothShape_0_shape";
    let face = fairy.shapes.iter().find(face).unwrap();
    assert_eq!(face.targets(), 52);
    for e in &demo::EXPRESSIONS {
        assert!(e.targets.iter().all(|t| *t < face.targets()), "{}", e.name);
    }
    let dance = &fairy.animations[0];
    for i in 0..2000 {
        for m in fairy.pose(Some((dance, f64::from(i) * 0.0125)), &[]) {
            assert!(m.is_finite(), "{i}");
        }
    }
    assert!(dance.numbers.iter().all(|n| n.curve.at(5.0).is_finite()));
}

#[test]
fn the_hair_is_as_dense_as_asked() {
    let Some(files) = files() else { return };
    let fairy = model(&files, demo::FAIRY);
    let hair = fairy.shapes.iter().find_map(|s| match &s.geometry {
        Geometry::Hair(h) => Some(h),
        Geometry::Mesh(_) => None,
    });
    let hair = hair.unwrap();
    assert_eq!(hair.per_hair, 6);
    assert_eq!(hair.points.len() / hair.per_hair, 7499);
}
