//! Reads the demo's own files. Needs the user's copy: `VICTORY_DATA`, or `data/`.
use s3d_victory::data::Files;
use s3d_victory::demo::FILES;

fn files() -> Files {
    let p = std::env::var("VICTORY_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn every_file_is_there() {
    let f = files();
    for n in FILES {
        assert!(!f.get(n).unwrap().is_empty(), "{n}");
    }
    // Textures and the background are Windows bitmaps.
    for n in FILES
        .iter()
        .filter(|n| n.ends_with(".tex") || n.ends_with(".bmp"))
    {
        assert!(f.get(n).unwrap().starts_with(b"BM"), "{n}");
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
