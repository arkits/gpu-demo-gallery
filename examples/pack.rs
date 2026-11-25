//! Packs the demo files the port needs: `pack DIR OUT`; `pack DIR PACK verify`.
use tidepool::data::Files;

fn main() -> Result<(), tidepool::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, out, rest @ ..] = args.as_slice() else {
        return Err("usage: pack DIR OUT [verify]".into());
    };
    let files = Files::from_dir(std::path::Path::new(dir))?;
    if rest.is_empty() {
        std::fs::write(out, files.pack()?)?;
        return Ok(());
    }
    let packed = Files::open(&std::fs::read(out)?)?;
    let names = files.needed()?;
    for f in &names {
        if packed.get(f)? != files.get(f)? {
            return Err(format!("{f} differs").into());
        }
    }
    println!("{out}: {} files match", names.len());
    Ok(())
}
