//! Reads the demo's own files. Needs the user's copy: `SCATTER_DATA`, or `data/`.
use light_scattering::data::Files;
use light_scattering::demo::{FILES, SOUNDS};

fn files() -> Files {
    let p = std::env::var("SCATTER_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn every_picture_reads() {
    let f = files();
    for n in FILES.iter().filter(|n| {
        [".png", ".tga", ".ppm", ".bmp", ".dds"]
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
fn field_and_movie_are_whole() {
    let f = files();
    assert_eq!(f.get("hf129_3.raw").unwrap().len(), 129 * 129);
    let movie = String::from_utf8_lossy(f.get("movie.dat").unwrap()).into_owned();
    let numbers: Vec<f32> = movie
        .split_ascii_whitespace()
        .map(|w| w.parse().unwrap())
        .collect();
    // A track of 64 keys, each a frame and 17 numbers; the last at frame 2940.
    assert_eq!(&numbers[..2], &[1.0, 64.0]);
    assert_eq!(numbers.len(), 3 + 64 * 18);
    assert_eq!(numbers[3 + 63 * 18], 2940.0);
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
