//! Integration checks against the user's local copy of the demo files. Nothing
//! is committed: set `ATB_DATA` to a folder holding `demoroom.bof` and `0000.3DF`
//! (as staged under `demoroom/data/`) to run these. They are skipped otherwise.
use std::path::{Path, PathBuf};

use atb::{Bof, Db, read_mesh_3ds, read_texture_3df};

fn data() -> Option<PathBuf> {
    let p = std::env::var_os("ATB_DATA").map(PathBuf::from)?;
    p.is_dir().then_some(p)
}

fn read(dir: &Path, name: &str) -> Vec<u8> {
    std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn real_bof_dict() {
    let Some(dir) = data() else { return };
    let b = Bof::parse(&read(&dir, "demoroom.bof")).unwrap();
    assert_eq!(b.names.len(), 38, "Demoroom's dict has 38 entries");
    assert_eq!(b.names[0], "helpscreen");
    assert_eq!(b.names.last().unwrap(), "demolite");
    assert!(b.find("0000.3DF").is_some());
}

#[test]
fn real_3df_pyramid() {
    let Some(dir) = data() else { return };
    let t = read_texture_3df(&read(&dir, "0000.3DF")).unwrap();
    assert_eq!(t.version, "3df v1.1");
    assert_eq!((t.lod_min, t.lod_max), (1, 256));
    assert_eq!(t.levels.len(), 9); // 256,128,...,1
    assert_eq!((t.base().width, t.base().height), (256, 256));
    assert_eq!(t.base().rgba.len(), 256 * 256 * 4);
    assert_eq!(t.levels.last().unwrap().width, 1); // smallest mip
}

#[test]
fn real_db_facade() {
    let Some(dir) = data() else { return };
    let db = Db::open(&read(&dir, "demoroom.bof")).unwrap();
    assert_eq!(db.names().len(), 38);
    assert!(db.texture(&read(&dir, "0000.3DF")).is_ok());
    assert!(db.mesh(&read(&dir, "DEMOLITE.3DS")).is_ok());
}

/// Set `ATB_SCAN` to the workspace root to prove the 3DS reader accepts every
/// staged ATB mesh across all packages. Skipped otherwise.
#[test]
fn all_staged_3ds_meshes_parse() {
    let Some(root) = std::env::var_os("ATB_SCAN") else {
        return;
    };
    let mut count = 0;
    fn walk(dir: &Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "target" || n == ".git") {
                    continue;
                }
                walk(&p, count);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("3ds")) {
                let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                let s = read_mesh_3ds(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                assert!(!s.meshes.is_empty(), "{}: no meshes", p.display());
                *count += 1;
            }
        }
    }
    walk(Path::new(&root), &mut count);
    assert!(
        count > 10,
        "expected many staged .3DS meshes, found {count}"
    );
    println!("parsed {count} staged .3DS meshes");
}

#[test]
fn real_3ds_scene() {
    let Some(dir) = data() else { return };
    let s = read_mesh_3ds(&read(&dir, "DEMOLITE.3DS")).unwrap();
    // 14 object chunks; only the geometry ones produce meshes with faces.
    assert_eq!(s.meshes.len(), 14);
    assert!(
        s.meshes
            .iter()
            .any(|m| m.name == "room01" && m.faces.len() == 4)
    );
    assert!(
        s.meshes
            .iter()
            .any(|m| m.name == "ankh" && m.faces.len() == 117)
    );
    assert!(
        s.meshes
            .iter()
            .any(|m| m.name == "floor01" && m.vertices.len() == 4)
    );
    let room = s.meshes.iter().find(|m| m.name == "room01").unwrap();
    assert_eq!(room.vertices.len(), 8);
    assert!(room.materials.iter().any(|n| n == "WALL MAIN"));
}
