//! Lists the data block kinds of every object and animation in an archive.
use sushi_formats::{anim, archive::Archive, object};
fn main() {
    let a = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for n in a.names() {
        let d = a.get(n).unwrap();
        let blocks = match n.rsplit('.').next().unwrap() {
            "so2" => object::read_object(d).map(|o| o.blocks).ok(),
            "sa2" => anim::read_animation(d).map(|o| o.blocks).ok(),
            _ => continue,
        };
        for b in blocks.unwrap_or_default() {
            for (kind, data) in &b.blocks {
                let hex: String = data.iter().take(32).map(|x| format!("{x:02x}")).collect();
                println!(
                    "{n} mesh {} kind {kind:#x} {} bytes {hex}",
                    b.raw_id,
                    data.len()
                );
            }
        }
    }
}
