//! Lists the textures of objects: `textures <Sushi.ati> [name filter]`.
use sushi_formats::{archive::Archive, object};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = Archive::open(&std::fs::read(&args[1]).unwrap()).unwrap();
    for n in a.names().filter(|n| n.ends_with(".so2")) {
        if args.get(2).is_some_and(|f| !n.contains(f.as_str())) {
            continue;
        }
        let o = object::read_object(a.get(n).unwrap()).unwrap();
        for t in &o.textures {
            let mean = t.decode().ok().and_then(|f| {
                let p = &f.first()?.first()?.rgba;
                let s: u64 = p.iter().map(|&b| u64::from(b)).sum();
                Some(s as f64 / p.len().max(1) as f64)
            });
            println!(
                "{n} {} fmt {:?} {}x{} faces {} cube {} texels {} mean {mean:?}",
                t.name,
                t.format(),
                t.width(),
                t.height(),
                t.faces(),
                t.is_cube(),
                t.texels.len()
            );
        }
    }
}
