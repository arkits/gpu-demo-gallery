//! Checks against the demo's own files; skipped without `POS_DATA`.
use std::path::PathBuf;

use principles_of_shading::combiner::Coords;
use principles_of_shading::data::Files;
use principles_of_shading::dds::Image;

fn files() -> Option<Files> {
    let dir = PathBuf::from(std::env::var_os("POS_DATA")?);
    Some(Files::from_dir(&dir).expect("data folder"))
}

#[test]
fn models_and_styles_match() {
    let Some(files) = files() else { return };
    for (i, shapes) in [16, 16, 27].into_iter().enumerate() {
        let m = files.model(i).unwrap();
        assert_eq!((m.shapes.len(), m.styles.len()), (shapes, 3));
        for s in m.styles.iter().flatten() {
            s.program.wgsl().unwrap();
            for (t, (coords, cube)) in s.textures.iter().zip(&s.program.textures) {
                assert_eq!(*coords == Coords::EyeReflect, *cube, "{}", t.file);
            }
        }
        // Both have unit normals, and nothing is out of sight.
        for v in m.shapes.iter().flat_map(|s| &s.vertices) {
            assert!((glam::Vec3::from(v.normal).length() - 1.0).abs() < 0.01);
            assert!(glam::Vec3::from(v.pos).length() < 200.0);
        }
    }
}

#[test]
fn bump_mapping_is_the_last_model() {
    let Some(files) = files() else { return };
    let uses = |i| {
        let m = files.model(i).unwrap();
        m.styles.iter().flatten().any(|s| s.light_in_color0)
    };
    assert_eq!([0, 1, 2].map(uses), [false, false, true]);
}

#[test]
fn every_image_reads() {
    let Some(files) = files() else { return };
    let names = files.needed().unwrap();
    for n in names.iter().filter(|n| n.ends_with(".dds")) {
        let i = Image::read(files.get(n).unwrap()).unwrap_or_else(|e| panic!("{n}: {e}"));
        i.decode(i.levels.len() - 1);
    }
}

#[test]
fn packing_keeps_everything() {
    let Some(files) = files() else { return };
    let again = Files::open(&files.pack().unwrap()).unwrap();
    for n in files.needed().unwrap() {
        assert_eq!(files.get(&n).unwrap(), again.get(&n).unwrap());
    }
}
