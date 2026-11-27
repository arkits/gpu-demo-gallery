//! Checks the demo's own files load through the strict readers.
use grand_bleu::data::Files;
use std::path::PathBuf;

fn data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GRAND_BLEU_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    p.is_dir().then_some(p)
}

#[test]
fn every_animal_reads() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).expect("need the demo's files");
    atb::read_texture_3df(files.get("light1.3df").unwrap()).unwrap();
    for f in grand_bleu::demo::FILES
        .iter()
        .filter(|f| f.ends_with(".bof"))
    {
        let bof = atb::Bof::parse(files.get(f).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert!(!bof.dict.is_empty(), "{f}");
    }
}

#[test]
fn pack_roundtrip() {
    let Some(dir) = data() else { return };
    let files = Files::from_path(&dir).unwrap();
    let packed = Files::open(&files.pack().unwrap()).unwrap();
    for f in grand_bleu::demo::FILES {
        assert_eq!(files.get(f).unwrap(), packed.get(f).unwrap(), "{f}");
    }
}
