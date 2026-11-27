//! Reads the demo's own files. Needs the user's copy: `REEF_DATA`, or `data/`.
use parhelia_reef::data::Files;
use parhelia_reef::script::{self, SHADER_KEY, WORLD_KEY};
use parhelia_reef::shader;
use parhelia_reef::xfile::Model;

fn files() -> Files {
    let p = std::env::var("REEF_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn scripts_decode() {
    let f = files();
    let world = script::parse(&script::decode(
        f.get("reefdemoscene.w").unwrap(),
        WORLD_KEY,
    ))
    .unwrap();
    assert_eq!(world.name, "tdrg_world_version2.0");
    let count = |k: &str| world.all(k).count();
    assert_eq!(count("TDRG_MeshActor"), 150);
    assert_eq!(count("TDRG_CameraActor"), 11);
    assert_eq!(count("TDRG_LogoActor"), 2);
    assert_eq!(count("TDRG_RelationshipObject"), 36);
    let shaders = script::parse(&script::decode(
        f.get("reefdemoscene.s").unwrap(),
        SHADER_KEY,
    ))
    .unwrap();
    assert_eq!(shaders.name, "tdrg_shader_version2.0");
    assert_eq!(shaders.all("Shader").count(), 57);
}

#[test]
fn every_shader_pair_translates() {
    let f = files();
    let s = script::parse(&script::decode(
        f.get("reefdemoscene.s").unwrap(),
        SHADER_KEY,
    ))
    .unwrap();
    let text = |n: &str| String::from_utf8_lossy(f.get(n).unwrap()).into_owned();
    let mut pairs = 0;
    for pass in s.all("Shader").flat_map(|s| s.all("Pass")) {
        let (vs, ps) = (
            pass.text("VertexShader").to_ascii_lowercase(),
            pass.text("PixelShader").to_ascii_lowercase(),
        );
        if vs.is_empty() || ps.is_empty() {
            continue;
        }
        let volumes = std::array::from_fn(|i| {
            pass.all("Layer")
                .any(|l| l.num("Stage") as usize == i && l.num("TextureType") == 1.0)
        });
        shader::program(&text(&vs), &text(&ps), volumes)
            .unwrap_or_else(|e| panic!("{vs} {ps}: {e}"));
        pairs += 1;
    }
    assert!(pairs > 50);
}

/// Rotation keys are four numbers in D3DXQUATERNION's order (x, y, z, w): read so, the
/// first key of a frame is that frame's own pose (nearly always: a few camera helpers were
/// moved after keying).
#[test]
fn models_read_and_keys_start_at_the_pose() {
    let f = files();
    let (mut models, mut keys, mut matching) = (0, 0, 0);
    for name in f.names().into_iter().filter(|n| n.ends_with(".x")) {
        let m = Model::read(f.get(&name).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        for x in &m.meshes {
            let n = x.positions.len() as u32;
            assert!(
                x.triangles.iter().all(|t| t.0.iter().all(|&i| i < n)),
                "{name}"
            );
            assert!(
                x.uv_sets.iter().all(|s| s.len() == x.positions.len()),
                "{name}"
            );
        }
        for t in &m.tracks {
            let (Some(&(0.0, q)), Some(fr)) = (
                t.rotation.first(),
                m.frames.iter().find(|f| f.name == t.frame),
            ) else {
                continue;
            };
            let (_, base, _) = fr.matrix.to_scale_rotation_translation();
            keys += 1;
            matching += usize::from(base.dot(q).abs() > 0.99);
        }
        models += 1;
    }
    assert!(models > 90, "{models}");
    assert!(matching * 100 > keys * 97, "{matching} of {keys}");
    // The reef's extra texture sets feed v9 and v10 (blend and light) in its shader.
    let reef = Model::read(f.get("scenery_reef.x").unwrap()).unwrap();
    assert_eq!(reef.meshes[0].uv_sets.len(), 4);
}

#[test]
fn every_picture_reads() {
    let f = files();
    for name in f.names().into_iter().filter(|n| n.ends_with(".tga")) {
        let i = f.image(&name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(i.rgba.len() as u32, i.width * i.height * 4, "{name}");
    }
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in f.names().iter().filter(|n| !n.starts_with("music.")) {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
