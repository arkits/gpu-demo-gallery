//! Packs the demo files the port needs: `pack DATA OUT`; `pack DATA PACK verify`;
//! `pack DATA FOLDER sounds` writes the sounds.
use mad_mod_mike::{Error, data::Files, demo};

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data, out, rest @ ..] = args.as_slice() else {
        return Err("usage: pack DATA OUT [verify|sounds]".into());
    };
    let files = Files::from_dir(std::path::Path::new(data))?;
    match rest.first().map(String::as_str) {
        None => std::fs::write(out, files.pack()?)?,
        Some("verify") => {
            let packed = Files::open(&std::fs::read(out)?)?;
            for f in files.needed()? {
                if packed.get(&f)? != files.get(&f)? {
                    return Err(format!("{f} differs").into());
                }
            }
            println!("{out}: files match");
        }
        Some("sounds") => {
            for s in ["soundtrack", "musicloop"] {
                let from = std::path::Path::new(data)
                    .join(demo::SOUND_DIR)
                    .join(format!("{s}.wav"));
                std::fs::write(
                    std::path::Path::new(out).join(format!("{s}.wav")),
                    std::fs::read(&from)?,
                )?;
            }
        }
        Some(other) => return Err(format!("unknown {other}").into()),
    }
    Ok(())
}
