//! Validates the assets when the extracted demo is available.
use ogre::{
    data::Files,
    demo,
    model::{Geometry, Model},
    tree,
};

fn files() -> Option<Files> {
    let path = std::env::var_os(demo::DATA_ENV)?;
    Some(Files::from_dir(std::path::Path::new(&path)).unwrap())
}

#[test]
fn scenes_parse_strictly_and_round_trip() {
    let Some(files) = files() else { return };
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for path in files.needed().unwrap() {
        assert_eq!(
            files.get(&path).unwrap(),
            packed.get(&path).unwrap(),
            "{path}"
        );
    }
    // Every byte of every scene is understood.
    for file in demo::MODELS {
        let data = files.get(file).unwrap();
        tree::read_binary(data).unwrap_or_else(|e| panic!("{file}: {e}"));
    }
}

#[test]
fn the_cast_is_complete() {
    let Some(files) = files() else { return };
    let model = Model::new(&files).unwrap();
    // Nine files merge into one performance.
    assert_eq!(model.shapes.len(), 21, "{}", model.shapes.len());
    // Boss, vest, skirts, belt, eyes, teeth, rings, string, ground,
    // backdrop and five creeps all have geometry.
    for shape in &model.shapes {
        assert!(
            model.geometries.contains_key(&shape.geometry),
            "{}",
            shape.name
        );
        assert!(
            model.transforms.contains_key(&shape.transform),
            "{}",
            shape.name
        );
        assert!(!shape.vertex.is_empty(), "{}", shape.name);
        assert!(!shape.fragment.is_empty(), "{}", shape.name);
    }
    // The key light shines from (1, 1, 1).
    let light = model.key_light_dir;
    assert!((light - glam::Vec3::ONE.normalize()).length() < 1e-6);
}

#[test]
fn the_boss_dances_and_the_creeps_wriggle() {
    let Some(files) = files() else { return };
    let model = Model::new(&files).unwrap();
    // 991 boss frames, hundreds of creep frames.
    let mut frames = 0;
    for geometry in model.geometries.values() {
        match geometry {
            Geometry::Subdiv(control) => {
                assert!(!control.frames.is_empty(), "no frames");
                assert_eq!(control.frames[0].positions.len(), control.positions.len());
                frames += control.frames.len();
            }
            Geometry::Baked(baked) => {
                if !baked.frames.is_empty() {
                    assert_eq!(baked.frames[0].positions.len(), baked.positions.len());
                    frames += baked.frames.len();
                }
            }
            Geometry::Static { .. } => {}
        }
    }
    assert!(frames > 4000, "{frames}");
    // Poses stay finite across the whole performance.
    for anim in model.transform_anims.values() {
        assert!(!anim.poses.is_empty());
        for pose in &anim.poses {
            assert!(pose.translation.is_finite());
            assert!(pose.scale.is_finite());
        }
    }
    // Rest matrices are affine with a unit last row.
    for t in model.transforms.values() {
        let m = t.matrix.to_cols_array_2d();
        assert!((m[3][3] - 1.0).abs() < 1e-4, "{}", t.name);
    }
}
