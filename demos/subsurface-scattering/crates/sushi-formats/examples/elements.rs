//! Prints the vertex elements of meshes whose shader matches: `elements <Sushi.ati> <part>`.
use sushi_formats::{archive::Archive, object};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = Archive::open(&std::fs::read(&args[1]).unwrap()).unwrap();
    for n in a.names().filter(|n| n.ends_with(".so2")) {
        let Ok(o) = object::read_object(a.get(n).unwrap()) else {
            continue;
        };
        for m in o
            .meshes
            .iter()
            .filter(|m| m.shader.to_lowercase().contains(&args[2]))
        {
            for s in &m.header.streams {
                for e in &s.elements {
                    println!(
                        "{n} {} {} kind {} usage {} index {}",
                        m.shader, e.source, e.kind, e.usage, e.usage_index
                    );
                }
            }
            return;
        }
    }
}
