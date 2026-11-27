//! Checks against the demo's own files; skipped without `CREATURE_DATA`.
use std::path::PathBuf;

use creature::data::Files;
use creature::model::Kind;
use glam::Vec3;

fn files() -> Option<Files> {
    let dir = PathBuf::from(std::env::var_os("CREATURE_DATA")?);
    Some(Files::from_dir(&dir).expect("data folder"))
}

#[test]
fn the_creature_reads() {
    let Some(files) = files() else { return };
    let m = files.model("creature.nmb").unwrap();
    assert_eq!((m.shapes.len(), m.nodes.len()), (219, 348));
    let bumps = m.shapes.iter().filter(|s| s.kind == Kind::Bump).count();
    assert_eq!(bumps, 8);
    assert!((10.0..20.0).contains(&m.radius()));
    for s in &m.shapes {
        assert!(
            s.vertices
                .iter()
                .all(|v| v.normal == [0.0; 3] || (Vec3::from(v.normal).length() - 1.0).abs() < 0.02),
            "{}",
            s.name
        );
    }
}

#[test]
fn every_channel_moves_a_node_and_nothing_flies_off() {
    let Some(files) = files() else { return };
    let m = files.model("creature.nmb").unwrap();
    let a = files.animation(&m).unwrap();
    let (mut before, mut most) = (m.pose(&a, 0.0), 0.0f32);
    for i in 1..1800 {
        let ms = f64::from(i) * 1000.0 / 60.0;
        let pose = m.pose(&a, ms);
        // Some hairs' curves end 10 degrees from where they start over.
        let seam = ms % 7000.0 < 40.0;
        for s in &m.shapes {
            let p: Vec3 = s.vertices[0].pos.into();
            let (x, y) = (
                before[s.node].transform_point3(p),
                pose[s.node].transform_point3(p),
            );
            assert!(y.is_finite() && y.length() < 40.0, "{} at {y}", s.name);
            if !seam {
                most = most.max(x.distance(y));
            }
        }
        before = pose;
    }
    assert!(most > 0.01, "nothing moves");
    assert!(most < 0.5, "something jumps by {most}");
}

#[test]
fn packing_keeps_everything() {
    let Some(files) = files() else { return };
    let again = Files::open(&files.pack().unwrap()).unwrap();
    for n in files.needed().unwrap() {
        assert_eq!(files.get(&n).unwrap(), again.get(&n).unwrap());
    }
}
