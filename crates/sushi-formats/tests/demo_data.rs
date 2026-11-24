//! Parses the original demo data end to end. Needs the user's own copy:
//! set `SUSHI_ATI`, or keep the demo folder at the repository root.
use std::collections::HashSet;
use std::sync::OnceLock;

use sushi_formats::{anim, archive::Archive, object, path, scene};

fn demo() -> &'static Archive {
    static A: OnceLock<Archive> = OnceLock::new();
    A.get_or_init(|| {
        let p = std::env::var("SUSHI_ATI").unwrap_or_else(|_| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream/Sushi.ati"
            )
            .into()
        });
        let data = std::fs::read(&p).unwrap_or_else(|e| panic!("need Sushi.ati at {p}: {e}"));
        Archive::open(&data).expect("open archive")
    })
}

fn each(ext: &str) -> Vec<(&'static str, &'static [u8])> {
    let a = demo();
    a.names()
        .filter(|n| n.ends_with(ext))
        .map(|n| (n, a.get(n).unwrap()))
        .collect()
}

#[test]
fn every_object_parses_to_eof() {
    let files = each(".so2");
    assert_eq!(files.len(), 101);
    let bad: Vec<_> = files
        .iter()
        .filter_map(|(n, d)| object::read_object(d).err().map(|e| format!("{n}: {e}")))
        .collect();
    assert!(bad.is_empty(), "{} failed:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn every_animation_parses_to_eof() {
    let files = each(".sa2");
    assert_eq!(files.len(), 344);
    let bad: Vec<_> = files
        .iter()
        .filter_map(|(n, d)| anim::read_animation(d).err().map(|e| format!("{n}: {e}")))
        .collect();
    assert!(bad.is_empty(), "{} failed:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn scenes_parse_to_eof() {
    for n in ["Globals.ati", "Scenes/Animusic.ati"] {
        if let Err(e) = scene::read_scene(demo().get(n).unwrap()) {
            panic!("{n}: {e}");
        }
    }
}

#[test]
fn fly_path_covers_the_music() {
    let t = std::str::from_utf8(demo().get("FlyPaths/Animusic.pth").unwrap()).unwrap();
    let p = path::read_path(t).unwrap();
    assert_eq!(p.frames.len(), 6448);
    assert!((p.frames.len() as f32 * p.step - 214.93).abs() < 0.05);
    for f in &p.frames {
        let n: f32 = f.quat.iter().map(|q| q * q).sum();
        assert!((n - 1.0).abs() < 1e-3, "non-unit quaternion {f:?}");
    }
}

/// Structural checks: would fail if fields were mislabelled even at EOF.
#[test]
fn mesh_indices_are_in_range() {
    for (n, d) in each(".so2") {
        let o = object::read_object(d).unwrap();
        for m in &o.meshes {
            assert!(m.shader.ends_with(".ssh"), "{n}: shader {:?}", m.shader);
            let verts = m
                .frames
                .iter()
                .flatten()
                .map(|v| v.count)
                .max()
                .unwrap_or(0);
            for ib in &m.index_buffers {
                let max = ib.indices.iter().copied().max().unwrap_or(0) as usize;
                assert!(max < verts, "{n}: index {max} of {verts} vertices");
            }
        }
    }
}

#[test]
fn animation_keys_are_sane() {
    let mut tracks = 0;
    for (n, d) in each(".sa2") {
        let a = anim::read_animation(d).unwrap();
        assert!((a.duration - 215.333).abs() < 0.01, "{n}: {}", a.duration);
        for t in &a.bones {
            tracks += 1;
            // The last key is a -1 terminator; the rest span the duration exactly.
            let dts = t.dts();
            let (last, gaps) = dts.split_last().unwrap();
            assert_eq!(*last, -1.0, "{n}");
            assert!(gaps.iter().all(|d| *d > 0.0), "{n}: non-positive gap");
            let total: f64 = gaps.iter().map(|d| f64::from(*d)).sum();
            assert!((total - f64::from(a.duration)).abs() < 0.01, "{n}: {total}");
            if let anim::Track::QuatPos(k) = t {
                for k in k {
                    let q: f32 = k[1..5].iter().map(|q| q * q).sum();
                    assert!((q - 1.0).abs() < 1e-2, "{n}: quat norm {q}");
                }
            }
        }
    }
    assert_eq!(tracks, 942);
}

#[test]
fn every_named_object_texture_has_data_somewhere() {
    let (mut have, mut want) = (HashSet::new(), HashSet::new());
    for (_, d) in each(".so2") {
        for t in object::read_object(d)
            .map(|o| o.textures)
            .unwrap_or_default()
        {
            if t.name.is_empty() {
                continue;
            }
            if !t.texels.is_empty() {
                have.insert(t.name.clone());
            }
            want.insert(t.name);
        }
    }
    let missing: Vec<_> = want.difference(&have).collect();
    assert!(!want.is_empty() && missing.is_empty(), "{missing:?}");
}
