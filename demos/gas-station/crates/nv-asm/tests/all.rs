//! Every listing under `$NV_ASM` (default `data/asm`) parses, translates and validates.
use std::path::{Path, PathBuf};

fn listings(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            listings(&p, out);
        } else if p.extension().is_some_and(|x| x == "asm") {
            out.push(p);
        }
    }
}

fn validate(src: &str) -> Result<(), String> {
    let m = naga::front::wgsl::parse_str(src).map_err(|e| e.emit_to_string(src))?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&m)
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[test]
fn all_listings_translate() {
    let dir = std::env::var("NV_ASM").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/asm")
    });
    let mut files = vec![];
    listings(&dir, &mut files);
    files.sort();
    if files.is_empty() {
        eprintln!("no listings in {}", dir.display());
        return;
    }
    let mut bad = vec![];
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let r = nv_asm::parse(&text).map_err(|e| e.to_string()).and_then(|p| {
            let opts = nv_asm::wgsl::Options::default();
            let (src, _) = match p.stage {
                nv_asm::Stage::Vertex => nv_asm::wgsl::vertex(&p, &opts),
                nv_asm::Stage::Fragment => nv_asm::wgsl::fragment(&p, &opts),
            }
            .map_err(|e| e.to_string())?;
            validate(&src).map_err(|e| format!("{e}\n{src}"))
        });
        if let Err(e) = r {
            bad.push(format!("{}: {}", f.display(), e.chars().take(3000).collect::<String>()));
        }
    }
    for b in &bad {
        eprintln!("{b}\n");
    }
    assert!(bad.is_empty(), "{} of {} failed", bad.len(), files.len());
    eprintln!("{} listings", files.len());
}
