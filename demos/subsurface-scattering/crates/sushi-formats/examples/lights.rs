use sushi_formats::{archive::Archive, reader::*, scene};
fn main() {
    let a = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for n in a.names().filter(|n| n.starts_with("scenes/")) {
        let s = scene::read_scene(a.get(n).unwrap()).unwrap();
        for (i, l) in s.lights.iter().enumerate() {
            let r = &l.raw;
            let f = |o: usize| f32_at(r, o);
            println!(
                "{n} {i} {:?} type {} 28={:#x} oa={} rt={} int {} range {} col {:?} cone {} 57={} inner {} pos {:?} dir {:?} 46={} step {} nkeys {} 19b={} 19c={} 19d={} mode={} 1a3={} 1ac={} 7b={} tex {:?}",
                cstr(&r[..0x20]),
                u32_at(r, 0x20),
                r[0x28],
                r[0x2a],
                r[0x2b],
                f(0x2e),
                f(0x32),
                [f(0x36), f(0x3a), f(0x3e), f(0x42)],
                f(0x53),
                f(0x57),
                f(0x5b),
                [f(0x5f), f(0x63), f(0x67), f(0x6b)],
                [f(0x8f), f(0x93), f(0x97)],
                r[0x46],
                f(0x47),
                l.table_4b.len(),
                r[0x19b],
                r[0x19c],
                r[0x19d],
                r[0x19e],
                r[0x1a3],
                r[0x1ac],
                f(0x7b),
                l.texture()
            );
            for k in l.table_4b.iter().take(2).chain(l.table_4b.last()) {
                println!(
                    "   {:?}",
                    k.chunks(4).map(|c| f32_at(c, 0)).collect::<Vec<_>>()
                );
            }
        }
        for m in &s.mirrors {
            println!(
                "{n} mirror {:?} plane {:?} b5..7 {:?} {}",
                cstr(&m.raw[0x10..0x40]),
                m.plane(),
                &m.raw[0..8],
                m.sets
                    .iter()
                    .map(|v| v
                        .groups
                        .iter()
                        .map(|g| g.items.len())
                        .sum::<usize>()
                        .to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        for (i, d) in s.draws.iter().enumerate().take(40) {
            let g = &s.geometry[d.geometry()];
            println!(
                "{n} draw {i} {} trans {} streams {} params {:?}",
                cstr(&s.shaders[g.shader()].raw),
                g.transparent(),
                g.chunk.streams.len(),
                d.params
                    .iter()
                    .map(|p| (p.name.clone(), p.kind))
                    .take(3)
                    .collect::<Vec<_>>()
            );
        }
    }
}
