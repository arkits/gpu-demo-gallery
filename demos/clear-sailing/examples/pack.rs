//! Packs the demo files the port needs, split for the 25 MiB Pages limit:
//! `pack DATA OUTDIR`; `pack DATA OUTDIR verify`; `pack DATA OUTDIR sounds`
//! writes the sounds. Parts hold 28 MB raw at most.
use clear_sailing::{Error, data::Files, demo};

/// Raw bytes at most in one part.
const PART: usize = 28 << 20;

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, out, rest @ ..] = args.as_slice() else {
        return Err("usage: pack DATA OUTDIR [verify|sounds]".into());
    };
    let files = Files::from_path(std::path::Path::new(data))?;
    match rest.first().map(String::as_str) {
        None => {
            std::fs::create_dir_all(out)?;
            for (n, part) in parts(&files)?.iter().enumerate() {
                std::fs::write(format!("{out}/clear-sailing-{n}.pack"), part)?;
            }
            Ok(())
        }
        Some("verify") => {
            let mut names = Vec::new();
            let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
            for n in 0.. {
                let pack = std::path::Path::new(out).join(format!("clear-sailing-{n}.pack"));
                if !pack.exists() {
                    break;
                }
                let part = Files::open(&std::fs::read(&pack)?)?;
                for f in demo::FILES {
                    if let Ok(d) = part.get(f) {
                        entries.push((f.to_string(), d.to_vec()));
                    }
                }
                names.push(pack);
            }
            for f in demo::FILES {
                let have = entries.iter().find(|(n, _)| n == f);
                let want = files.get(f)?;
                match have {
                    Some((_, d)) if d == want => {}
                    _ => return Err(format!("{f} differs").into()),
                }
            }
            println!("{}: {} files match", names.len(), demo::FILES.len());
            Ok(())
        }
        Some("sounds") => {
            for s in demo::SOUNDS {
                std::fs::write(std::path::Path::new(out).join(s), files.get(s)?)?;
            }
            Ok(())
        }
        Some(other) => Err(format!("unknown {other}").into()),
    }
}

fn parts(files: &Files) -> Result<Vec<Vec<u8>>, Error> {
    let mut groups: Vec<Vec<&str>> = vec![Vec::new()];
    let mut size = 0;
    let mut names = demo::FILES.to_vec();
    names.sort_by_key(|n| std::cmp::Reverse(files.get(n).map(<[u8]>::len).unwrap_or(0)));
    for n in names {
        let len = files.get(n)?.len();
        if size + len > PART && !groups.last().is_none_or(Vec::is_empty) {
            groups.push(Vec::new());
            size = 0;
        }
        groups.last_mut().expect("a group").push(n);
        size += len;
    }
    groups
        .iter()
        .map(|g| files.pack_some(g))
        .collect::<Result<Vec<_>, Error>>()
}
