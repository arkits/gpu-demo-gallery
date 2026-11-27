//! Reads the demo's own files. Needs the user's copy: `BLOBBY_DATA`, or `data/`.
use blobby_dancer::data::Files;
use blobby_dancer::demo::{self, FILES};
use blobby_dancer::{fp30, vab};

fn files() -> Files {
    let p = std::env::var("BLOBBY_DATA");
    let p = p.unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());
    Files::from_path(std::path::Path::new(&p)).expect("need the demo's files")
}

#[test]
fn every_picture_reads() {
    let f = files();
    for n in FILES.iter().filter(|n| n.ends_with(".tga")) {
        let i = f.image(n).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(i.rgba.len() as u32, i.width * i.height * 4, "{n}");
    }
    let music = f.get("music000.mp3").unwrap();
    assert!(music.starts_with(b"ID3") || music[0] == 0xff);
}

#[test]
fn the_dancer_has_twelve_clips_and_103_blobs() {
    let d = vab::read(files().get("dancer_11_to_28_103v.vab").unwrap()).unwrap();
    assert_eq!(d.clips(), 12);
    let mut points = Vec::new();
    d.place(0, 0.0, glam::Mat4::IDENTITY, &mut points);
    assert_eq!(points.len(), 103);
    let lengths: Vec<u32> = (0..12).map(|c| d.length(c).round() as u32).collect();
    assert_eq!(lengths[..3], [13699, 12899, 14399]);
}

#[test]
fn every_program_translates_and_the_shader_is_whole() {
    let f = files();
    let s = demo::shader(&f).unwrap();
    for p in demo::PROGRAMS {
        assert!(s.contains(&format!("fn {p}(")), "{p}");
    }
    let floor = std::str::from_utf8(f.get("floor2.fp30.asm").unwrap()).unwrap();
    assert!(
        fp30::translate("floor2", floor)
            .unwrap()
            .contains("textureSample(t0")
    );
}

#[test]
fn packs_open_again() {
    let f = files();
    let again = Files::open(&f.pack().unwrap()).unwrap();
    for n in FILES {
        assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
    }
}
