//! Reads the demo's own files. Needs the user's copy: `CLEAR_SAILING_DATA`, or `data/`.
use clear_sailing::data::Files;
use clear_sailing::demo::{self, FILES, SOUNDS};
use clear_sailing::{nma, nmb, scene};

use std::collections::BTreeMap;

fn files() -> Files {
    let p = std::env::var("CLEAR_SAILING_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn every_picture_reads() {
    let f = files();
    let mut pictures = 0;
    for n in FILES.iter().filter(|n| {
        n.ends_with(".tga") || n.ends_with(".dds") || n.ends_with(".vol") || n.ends_with(".dat")
    }) {
        let data = f.get(n).unwrap();
        if n.ends_with(".vol") {
            assert_eq!(data.len(), 1790, "{n}");
            continue;
        }
        let i = f.image(n).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(i.rgba.len() as u32, i.width * i.height * 4, "{n}");
        pictures += 1;
    }
    assert!(pictures > 130, "{pictures}");
    for s in SOUNDS {
        assert!(f.get(s).unwrap().starts_with(b"RIFF"), "{s}");
    }
}

#[test]
fn text_scenes_parse_to_the_last_byte() {
    let f = files();
    for n in FILES.iter().filter(|n| n.ends_with(".nma")) {
        let scene = nma::read_text(f.get(n).unwrap()).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert_eq!(scene.kind, "Scene", "{n}");
    }
    // Spot checks against the unpacked files.
    let ropes = nma::read_text(files().get("ropes.nma").unwrap()).unwrap();
    assert!(ropes.nodes("geometries").len() > 50);
    assert!(
        ropes
            .nodes("shapes")
            .iter()
            .any(|s| s.text("transform").is_empty())
    );
    let ocean = nma::read_text(files().get("foamy_ocean.nma").unwrap()).unwrap();
    let water = ocean
        .nodes("geometries")
        .iter()
        .find(|g| g.kind == "Ocean")
        .unwrap();
    assert_eq!(water.num("tesselation", 0), 150.0);
    assert_eq!(water.num("amplitude", 0), 0.5);
    let smoke = nma::read_text(files().get("smoke.nma").unwrap()).unwrap();
    let rash = smoke
        .nodes("geometries")
        .iter()
        .find(|g| g.kind == "ParticleSystem" && g.name == "smoke")
        .unwrap();
    assert_eq!(rash.num("maxParticles", 0), 10000.0);
    // Strictness: trailing words and short files fail.
    assert!(nma::read_text(b"Scene { } trailing").is_err());
    assert!(nma::read_text(b"Scene { ").is_err());
}

#[test]
fn binary_scenes_parse_to_the_last_byte() {
    let f = files();
    let ship = nmb::read_binary(f.get("ps_03_19.nmb").unwrap()).unwrap();
    assert_eq!(ship.kind, "Scene");
    let mut meshes = 0;
    let mut sails = 0;
    for m in ship.nodes("geometries") {
        if m.kind == "IndexedFaceSet" {
            scene::mesh(m).unwrap();
            meshes += 1;
        }
    }
    assert_eq!(meshes, 69);
    let sails_file = nmb::read_binary(f.get("sail_animation.nmb").unwrap()).unwrap();
    for a in sails_file.nodes("animations") {
        for c in a.nodes("channels") {
            assert_eq!(c.kind, "GeometryChannel");
            assert!(!c.nodes("keyframes").is_empty());
            sails += 1;
        }
    }
    assert_eq!(sails, 18);
    // The sail targets name the ship's sail meshes.
    let names: Vec<&str> = sails_file
        .nodes("animations")
        .iter()
        .flat_map(|a| a.nodes("channels"))
        .map(|c| c.text("target"))
        .collect();
    assert!(names.contains(&"syCloth5Shape"));
}

#[test]
fn hierarchy_and_winding() {
    let f = files();
    let ship = nmb::read_binary(f.get("ps_03_19.nmb").unwrap()).unwrap();
    let mut worlds = BTreeMap::new();
    scene::worlds(&ship, &mut worlds).unwrap();
    assert!(worlds.len() > 90);
    assert!(worlds.contains_key("Masthead"));
    // Every indexed mesh is valid; strips expand to real triangles.
    let mut tris = 0;
    for m in ship.nodes("geometries") {
        if m.kind != "IndexedFaceSet" {
            continue;
        }
        let mesh = scene::mesh(m).unwrap();
        for t in mesh.idx.chunks_exact(3) {
            assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2]);
            tris += 1;
        }
    }
    assert!(tris > 90000, "{tris}");
}

#[test]
fn shader_is_whole_and_programs_match() {
    let f = files();
    let s = demo::shader(&f).unwrap();
    assert!(s.contains("@fragment"));
    for p in demo::PROGRAMS {
        assert!(s.contains(&format!("fn {p}(")), "{p}");
    }
    assert!(s.contains("fn vsmotion("));
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
