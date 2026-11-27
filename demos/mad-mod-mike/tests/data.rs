//! Validates the assets when the extracted demo is available.
use std::collections::BTreeSet;

use mad_mod_mike::{
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
fn text_scenes_parse_strictly() {
    let Some(files) = files() else { return };
    for name in demo::MODELS.iter().filter(|n| n.ends_with(".nma")) {
        if *name == demo::PARTICLES {
            let text = files.text(name).unwrap();
            let (node, rest) = tree::read_text_first(&text).unwrap();
            assert_eq!(node.kind, "Scene");
            // The shipped file ends with a corrupt tail after its Scene.
            assert_eq!(rest, ["0", "o3"], "{name}");
        } else {
            let root = files.scene(name).unwrap();
            assert_eq!(root.kind, "Scene", "{name}");
        }
    }
}

#[test]
fn world_parses_as_expected() {
    let Some(files) = files() else { return };
    let root = files.binary(demo::GEOM).unwrap();
    // The viewer registers these node kinds; every one below appears.
    assert_eq!(root.nodes("lights").len(), 11);
    assert_eq!(root.nodes("cameras").len(), 12);
    assert_eq!(root.nodes("transforms").len(), 22);
    assert_eq!(root.nodes("shapes").len(), 24);
    assert_eq!(root.nodes("geometries").len(), 247);
    let model = model(&files, demo::GEOM);
    // Joints, shapes and morph targets of Mad Mod Mike.
    assert_eq!(model.shapes.len(), 1591);
    // Takes drive the skeleton and the blend shapes.
    let mut takes = 0;
    for take in demo::TAKES {
        let root = files.binary(take.file).unwrap();
        let animations = root.nodes("animations");
        assert_eq!(animations.len(), 1, "{}", take.file);
        assert!(!animations[0].nodes("channels").is_empty());
        takes += 1;
    }
    assert_eq!(takes, demo::TAKES.len());
}

#[test]
fn bones_look_sane() {
    let Some(files) = files() else { return };
    let model = model(&files, demo::GEOM);
    assert!(model.shapes.len() > 1000);
    for shape in &model.shapes {
        assert!(shape.bones.len() <= 96, "{}", shape.name);
        if let Geometry::Mesh(mesh) | Geometry::Cage(mesh) = &shape.geometry {
            let limit = shape.bones.len().max(1) as f32;
            for f in &mesh.fixed {
                assert!(
                    f.bones.iter().all(|b| (0.0..limit).contains(b)),
                    "{}",
                    shape.name
                );
                let sum: f32 = f.weights.iter().sum();
                // Unweighted vertices stay put; the rest blend to one.
                assert!(
                    sum == 0.0 || (sum - 1.0).abs() < 1e-3,
                    "{} {sum}",
                    shape.name
                );
            }
        }
    }
}

#[test]
fn enums_match_the_binaries() {
    let Some(files) = files() else { return };
    // Filter and wrap modes by their binary values.
    let root = files.binary(demo::GEOM).unwrap();
    let mut profiles = BTreeSet::new();
    for s in root.nodes("shaders") {
        profiles.insert((s.text("filename"), s.int("profile")));
    }
    assert!(
        profiles
            .iter()
            .any(|(f, p)| f.ends_with(".vp30") && *p == 6148)
    );
    assert!(
        profiles
            .iter()
            .any(|(f, p)| f.ends_with(".fp30") && *p == 6151)
    );
    // Camera, light and blend bindings resolve by name.
    let model = model(&files, demo::GEOM);
    assert!(model.shapes.iter().any(|s| !s.textures.is_empty()));
    assert!(model.shapes.iter().any(|s| !s.matrices.is_empty()));
    assert!(model.shapes.iter().any(|s| !s.lights.is_empty()));
}

#[test]
fn hair_grows() {
    let Some(files) = files() else { return };
    for (curves, params) in [
        ("bin/hair_curves.txt", "bin/hairclumpsparams.txt"),
        ("bin/gotti_curves.txt", "bin/gotti_params.txt"),
    ] {
        let guides = files.text(curves).unwrap();
        assert!(guides.contains("curve1"));
        let hair = mad_mod_mike::hair::Hair::new(&guides, 2, 2, 1.0, 0.0).unwrap();
        assert!(hair.points.len() > 100);
        assert!(mad_mod_mike::hair::Hair::new("", 2, 2, 1.0, 0.0).is_err());
        let _ = params;
    }
}

#[test]
fn cameras_come_from_the_presets() {
    let Some(files) = files() else { return };
    for name in demo::TEXT_FILES {
        let text = files.text(name).unwrap();
        if name.contains("blendcombos") {
            assert!(text.contains("how many targets"), "{name}");
        } else {
            assert!(text.contains("camYaw"), "{name}");
        }
    }
}
