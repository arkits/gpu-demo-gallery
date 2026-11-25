//! Integration checks against ATB_DATA (any demo folder). Skipped when unset.
use atb::{Bof, read_mesh_3ds, read_texture_3df};
use std::path::{Path, PathBuf};

#[test]
fn staged_assets_parse() {
    let Some(root) = std::env::var_os("ATB_DATA") else {
        return;
    };
    fn walk(d: &Path, parsed: &mut usize, total: &mut usize) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, parsed, total);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "bof" | "3ds" | "3df") {
                continue;
            }
            *total += 1;
            let b = std::fs::read(&p).unwrap();
            let ok = match ext.as_str() {
                "bof" => Bof::parse(&b).is_ok(),
                "3ds" => read_mesh_3ds(&b).is_ok(),
                "3df" => read_texture_3df(&b).is_ok(),
                _ => false,
            };
            if ok {
                *parsed += 1;
            }
        }
    }
    let (mut parsed, mut total) = (0, 0);
    walk(Path::new(&PathBuf::from(root)), &mut parsed, &mut total);
    assert!(total > 0, "no ATB assets under ATB_DATA");
    assert!(parsed > 0, "no ATB asset parsed");
    println!("parsed {parsed}/{total} staged ATB assets");
}
