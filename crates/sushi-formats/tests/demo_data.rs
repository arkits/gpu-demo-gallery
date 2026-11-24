//! Parses the original demo data end to end. Needs the user's own copy:
//! set `SUSHI_ATI`, or keep the demo folder at the repository root.
use std::collections::HashSet;
use std::sync::OnceLock;

use sushi_formats::{anim, archive::Archive, object, path, scene, texture};

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

#[test]
fn every_texture_decodes_with_an_exact_mip_chain() {
    let mut n = 0;
    let mut check = |t: &texture::Texture| {
        if t.texels.is_empty() {
            return;
        }
        n += 1;
        let faces = t.decode().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(faces.len(), if t.is_cube() { 6 } else { 1 }, "{}", t.name);
        for l in faces.iter().flatten() {
            assert_eq!(l.rgba.len(), l.width * l.height * 4);
        }
    };
    for f in ["Globals.ati", "Scenes/Animusic.ati"] {
        scene::read_scene(demo().get(f).unwrap())
            .unwrap()
            .textures
            .iter()
            .for_each(&mut check);
    }
    for (_, d) in each(".so2") {
        object::read_object(d)
            .unwrap()
            .textures
            .iter()
            .for_each(&mut check);
    }
    assert!(n > 30, "only {n} textures");
}

#[test]
fn scene_draws_reference_valid_data() {
    let s = scene::read_scene(demo().get("Scenes/Animusic.ati").unwrap()).unwrap();
    let mut triangles = 0;
    for item in s.groups.iter().flat_map(|g| g.draw_items()) {
        let d = &s.draws[item.draw];
        let g = &s.geometry[d.geometry()];
        assert!(g.shader() < s.shaders.len());
        let st = &g.chunk.streams[0];
        let ib = st.index_buffer_of(item.vertex_buffer).unwrap();
        let verts = st.buffers[0][item.vertex_buffer].1.len() as u32 / st.stride();
        let range = item.start as usize..(item.start + item.count) as usize;
        let idx = g.chunk.indices[ib].1.get(range).expect("index range");
        assert_eq!(idx.len() % 3, 0);
        assert!(idx.iter().all(|&i| u32::from(i) < verts));
        triangles += idx.len() / 3;
        for l in d.lights() {
            assert!(s.lights[l].is_fast_rt(), "light {l}");
        }
    }
    // Every index of every buffer is drawn exactly once.
    let total: usize = s
        .geometry
        .iter()
        .flat_map(|g| &g.chunk.indices)
        .map(|i| i.1.len())
        .sum();
    assert_eq!(triangles, total / 3);
    let end = |a: &scene::Attribute| a.offset + a.size;
    for st in s.geometry.iter().flat_map(|g| &g.chunk.streams) {
        assert_eq!(st.attributes().iter().map(end).max(), Some(st.stride()));
    }
}

#[test]
fn fast_rt_lights_have_orthonormal_axes() {
    let s = scene::read_scene(demo().get("Scenes/Animusic.ati").unwrap()).unwrap();
    let fast: Vec<_> = s.lights.iter().filter(|l| l.is_fast_rt()).collect();
    assert_eq!(fast.len(), 7);
    for l in fast {
        let m = l.fast_rt_matrix();
        let dot = |a: [f32; 4], b: [f32; 4]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for i in 1..4 {
            for j in 1..4 {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((dot(m[i], m[j]) - want).abs() < 1e-3, "{m:?}");
            }
        }
        assert!(s.textures[l.texture().unwrap()].name.contains("SpotLight"));
        println!("{:?} {:?} {:?}", l.position(), l.color(), m);
    }
}
