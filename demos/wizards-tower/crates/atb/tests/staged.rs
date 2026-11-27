//! Checks against the user's local copies of the demos' files. Nothing is
//! committed: set `ATB_SCAN` to a folder holding unpacked ATB demos to run
//! these; they are skipped otherwise.
use std::path::{Path, PathBuf};

use atb::{Bof, Object, read_mesh_3ds};

fn files(ext: &str) -> Vec<PathBuf> {
    let Some(root) = std::env::var_os("ATB_SCAN") else {
        return Vec::new();
    };
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !p.file_name().is_some_and(|n| n == "target" || n == ".git") {
                    walk(&p, ext, out);
                }
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new(&root), ext, &mut out);
    out
}

#[test]
fn every_staged_bof_reads_to_the_end() {
    let mut count = 0;
    for p in files("bof") {
        let bytes = std::fs::read(&p).unwrap();
        let b = Bof::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(!b.dict.is_empty(), "{}", p.display());
        // Every picture decodes.
        for o in &b.objects {
            if let Object::Image(i) = o {
                i.rgba().unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            }
        }
        count += 1;
    }
    println!("read {count} staged .BOF files");
}

#[test]
fn every_staged_3ds_reads() {
    for p in files("3ds") {
        let bytes = std::fs::read(&p).unwrap();
        let s = read_mesh_3ds(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(!s.meshes.is_empty(), "{}: no meshes", p.display());
    }
}
