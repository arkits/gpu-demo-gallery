//! Reads the demo's own files. Needs the user's copy: `SIXTY_HERTZ_DATA`, or `data/`.
use sixty_hertz::data::Files;
use sixty_hertz::demo::{FILES, SOUNDS};

fn files() -> Files {
    let p = std::env::var("SIXTY_HERTZ_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn every_picture_reads() {
    let f = files();
    // Only the pictures are decoded; the .dat camera presets and .daf models are
    // binary state/model dumps and are checked below instead.
    for n in FILES.iter().filter(|n| {
        [".png", ".tga", ".ppm", ".dds"]
            .iter()
            .any(|e| n.ends_with(e))
    }) {
        let i = f.image(n).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert_eq!(i.rgba.len() as u32, i.width * i.height * 4, "{n}");
        assert!(i.width > 0 && i.height > 0, "{n}");
    }
    for s in SOUNDS {
        assert!(f.get(s).unwrap().starts_with(b"RIFF"), "{s}");
    }
}

#[test]
fn camera_presets_and_models_exist() {
    let f = files();
    for n in 1..=9 {
        let name = format!("init{n}.dat");
        // The original reads a 0x1e8-byte state block; the staged files are 484 bytes.
        assert!(f.get(&name).unwrap().len() >= 480, "{name}");
    }
    for n in [
        "scene.daf",
        "animmodel1.daf",
        "bouncing.daf",
        "tdxobject.daf",
    ] {
        assert!(f.get(n).unwrap().len() > 1024, "{n}");
    }
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
