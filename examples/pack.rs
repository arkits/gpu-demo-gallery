//! Packs the demo files the port needs: `pack DATA OUT`; `pack DATA PACK verify`;
//! `pack DATA FOLDER sounds` writes the sounds.
use pond::{Error, data::Files, demo};

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, out, rest @ ..] = args.as_slice() else {
        return Err("usage: pack DATA OUT [verify|sounds]".into());
    };
    let files = Files::from_path(std::path::Path::new(data))?;
    match rest.first().map(String::as_str) {
        None => std::fs::write(out, files.pack()?)?,
        Some("verify") => {
            let packed = Files::open(&std::fs::read(out)?)?;
            for f in demo::FILES {
                if packed.get(f)? != files.get(f)? {
                    return Err(format!("{f} differs").into());
                }
            }
            println!("{out}: {} files match", demo::FILES.len());
        }
        Some("sounds") => {
            for s in demo::SOUNDS {
                std::fs::write(std::path::Path::new(out).join(s), files.get(s)?)?;
            }
        }
        Some(other) => return Err(format!("unknown {other}").into()),
    }
    Ok(())
}
