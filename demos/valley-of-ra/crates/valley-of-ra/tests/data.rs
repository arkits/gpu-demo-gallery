//! Checks the demo's own files load through the strict readers.
use std::path::PathBuf;
use valley_of_ra::data::Files;

fn data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("VALLEY_OF_RA_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    p.is_dir().then_some(p)
}

#[test]
fn scene_reads() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).expect("need the demo's files");
    let bof = atb::Bof::parse(files.get("fight.bof").unwrap()).unwrap();
    for name in [
        "an_moves",
        "nef_moves",
        "refl",
        "top",
        "background",
        "scene",
    ] {
        assert!(bof.find(name).is_some(), "{name}");
    }
}

#[test]
fn pack_roundtrip() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).unwrap();
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for f in valley_of_ra::demo::FILES {
        assert_eq!(files.get(f).unwrap(), packed.get(f).unwrap(), "{f}");
    }
}

/// An on-screen button presses and lets go before the next tick; the
/// fighter must still see it, and a held block must stay held.
#[test]
fn taps_between_ticks_count() {
    use valley_of_ra::demo::{Demo, PROGRAMS, SHADER};
    use valley_of_ra::gl::{COLOR, Gl, Gpu};
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).unwrap();
    let gpu = Gpu::headless().expect("a GPU");
    let mut gl = Gl::new(&gpu, SHADER, PROGRAMS, COLOR);
    let mut demo = Demo::new(&gpu, &mut gl, &files).unwrap();
    // `step` catches up at most eight ticks at a time.
    let run = |demo: &mut Demo, seconds: f64| {
        for _ in 0..(seconds * 60.0).round() as usize {
            demo.step(1.0 / 60.0);
        }
    };
    run(&mut demo, 0.1);
    demo.key("8", true);
    demo.key("8", false);
    demo.key("o", true);
    demo.key("o", false);
    run(&mut demo, 1.0 / 60.0);
    let r = demo.report();
    assert!(
        r.starts_with("anubis 1 ") && r.contains("nefertiti 2 "),
        "{r}"
    );
    // A held key is not dropped: block stays in move 11 until let go.
    run(&mut demo, 3.0);
    demo.key("u", true);
    run(&mut demo, 0.5);
    assert!(demo.report().contains("nefertiti 11 "), "{}", demo.report());
    demo.key("u", false);
    run(&mut demo, 1.0);
    assert!(demo.report().contains("nefertiti 0 "), "{}", demo.report());
}
