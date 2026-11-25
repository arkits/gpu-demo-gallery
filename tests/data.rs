//! Reads the demo's own files. Needs the user's copy: `BORG_DATA`, or `data/`.
use borg::data::Files;
use borg::demo::{FILES, SOUNDS};

fn files() -> Files {
    let p = std::env::var("BORG_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn every_picture_reads() {
    let f = files();
    for n in FILES.iter().filter(|n| {
        [".png", ".tga", ".ppm", ".dat", ".dds"]
            .iter()
            .any(|e| n.ends_with(e))
    }) {
        let i = f.image(n).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(i.rgba.len() as u32, i.width * i.height * 4, "{n}");
    }
    for s in SOUNDS {
        assert!(f.get(s).unwrap().starts_with(b"RIFF"), "{s}");
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
