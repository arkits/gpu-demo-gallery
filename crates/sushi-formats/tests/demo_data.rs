//! Parses the original demo data end to end. Needs the user's own copy:
//! set `SUSHI_ATI`, or keep it in `data/`.
use std::collections::HashSet;
use std::sync::OnceLock;

use sushi_formats::{anim, archive::Archive, mesh::Mesh, object, path, scene, texture};

const SCENE: &str = "Scenes/bear.ati";
const OBJECTS: usize = 1;
const TEXTURES: usize = 58;
/// Camera paths and their seconds.
const PATHS: &[(&str, f32)] = &[("bear", 24.0)];

fn demo() -> &'static Archive {
    static A: OnceLock<Archive> = OnceLock::new();
    A.get_or_init(|| {
        let p = std::env::var("SUSHI_ATI").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/Sushi.ati").into()
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

fn scene() -> scene::Scene {
    scene::read_scene(demo().get(SCENE).unwrap()).unwrap()
}

/// Readers fail on unread bytes.
#[test]
fn everything_parses_to_eof() {
    let (objects, animations) = (each(".so2"), each(".sa2"));
    assert_eq!((objects.len(), animations.len()), (OBJECTS, OBJECTS));
    for (n, d) in objects {
        object::read_object(d).unwrap_or_else(|e| panic!("{n}: {e}"));
    }
    for (n, d) in animations {
        anim::read_animation(d).unwrap_or_else(|e| panic!("{n}: {e}"));
    }
    for n in ["Globals.ati", SCENE] {
        scene::read_scene(demo().get(n).unwrap()).unwrap_or_else(|e| panic!("{n}: {e}"));
    }
}

#[test]
fn fly_paths() {
    assert_eq!(each(".pth").len(), PATHS.len());
    for (name, seconds) in PATHS {
        let t = std::str::from_utf8(demo().get(&format!("FlyPaths/{name}.pth")).unwrap()).unwrap();
        let p = path::read_path(t).unwrap();
        let length = p.frames.len() as f32 * p.step;
        assert!((length - seconds).abs() < 0.05, "{name} {length}");
        for f in &p.frames {
            let n: f32 = f.quat.iter().map(|q| q * q).sum();
            assert!((n - 1.0).abs() < 1e-2, "{name}: non-unit quaternion {f:?}");
        }
    }
}

/// Structural checks: would fail if fields were mislabelled even at EOF.
#[test]
fn mesh_indices_are_in_range() {
    let check = |n: &str, m: &Mesh| {
        assert!(m.shader.ends_with(".ssh"), "{n}: shader {:?}", m.shader);
        let count = |v: &sushi_formats::mesh::VertexBuffer| v.count;
        let verts = m.frames.iter().flatten().map(count).max().unwrap_or(0);
        for ib in &m.index_buffers {
            let max = ib.indices.iter().copied().max().unwrap_or(0) as usize;
            assert!(max < verts, "{n}: index {max} of {verts} vertices");
        }
        // Shapes change between frames of the same size.
        for f in &m.frames {
            assert_eq!(f[0].count, m.frames[0][0].count, "{n}");
        }
    };
    let mut meshes = 0;
    for (n, d) in each(".so2") {
        let o = object::read_object(d).unwrap();
        o.meshes.iter().for_each(|m| check(n, m));
        meshes += o.meshes.len();
    }
    for (n, d) in each(".sa2") {
        let a = anim::read_animation(d).unwrap();
        a.morph_meshes.iter().for_each(|m| check(n, m));
        meshes += a.morph_meshes.len();
    }
    assert_eq!(meshes == 0, OBJECTS == 0);
}

#[test]
fn animation_keys_are_sane() {
    for (n, d) in each(".sa2") {
        let a = anim::read_animation(d).unwrap();
        for t in &a.bones {
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
}

#[test]
fn every_texture_has_data_and_decodes_with_an_exact_mip_chain() {
    let (mut have, mut want) = (HashSet::new(), HashSet::new());
    let mut check = |t: &texture::Texture| {
        if !t.name.is_empty() {
            want.insert(t.name.clone());
        }
        if t.texels.is_empty() {
            return;
        }
        have.insert(t.name.clone());
        let faces = t.decode().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(faces.len(), if t.is_cube() { 6 } else { 1 }, "{}", t.name);
        let size = match t.format() {
            Some(texture::Format::Abgr16) => 8,
            _ => 4,
        };
        for l in faces.iter().flatten() {
            assert_eq!(l.rgba.len(), l.width * l.height * size);
        }
    };
    scene().textures.iter().for_each(&mut check);
    for (_, d) in each(".so2") {
        object::read_object(d)
            .unwrap()
            .textures
            .iter()
            .for_each(&mut check);
    }
    let missing: Vec<_> = want.difference(&have).collect();
    assert!(missing.is_empty(), "{missing:?}");
    assert_eq!(have.len(), TEXTURES);
}

#[test]
fn scene_draws_reference_valid_data() {
    let s = scene();
    let mut triangles = 0;
    let mirrored = s
        .mirrors
        .iter()
        .flat_map(|m| m.sets.iter().flat_map(|v| &v.groups));
    for item in s.groups.iter().chain(mirrored).flat_map(|g| g.draw_items()) {
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
    }
    // Every index of every buffer is drawn exactly once.
    let total: usize = s
        .geometry
        .iter()
        .flat_map(|g| &g.chunk.indices)
        .map(|i| i.1.len())
        .sum();
    assert_eq!(triangles, total / 3);
    // Rectangles that scripts draw by name.
    for q in &s.table_112 {
        assert!((sushi_formats::reader::u32_at(q, 0x20) as usize) < s.draws.len());
    }
}
