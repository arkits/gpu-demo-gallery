//! Repacks an archive uncompressed, for xz: `pack <in> <out>`. With `verify` as the
//! third argument, instead checks that both archives hold identical files.
use sushi_formats::archive::Archive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, rest @ ..] = a.as_slice() else {
        return Err("usage: pack <in> <out> [verify]".into());
    };
    let source = Archive::open(&std::fs::read(input)?)?;
    if rest.first().is_some_and(|r| r == "verify") {
        let start = std::time::Instant::now();
        let other = Archive::open(&std::fs::read(output)?)?;
        let time = start.elapsed();
        if !source.names().eq(other.names()) {
            return Err("file lists differ".into());
        }
        if let Some(n) = source.names().find(|n| source.get(n) != other.get(n)) {
            return Err(format!("{n} differs").into());
        }
        println!(
            "{} files identical, opened in {time:?}",
            source.names().count()
        );
    } else {
        std::fs::write(output, source.pack())?;
    }
    Ok(())
}
