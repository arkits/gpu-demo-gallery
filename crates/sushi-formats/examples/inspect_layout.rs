//! Dumps undecoded record fields while recovering layouts: `inspect_layout <Sushi.ati>`.
use std::collections::BTreeMap;
use sushi_formats::reader::{cstr, f32_at, u32_at};
use sushi_formats::{archive::Archive, object, scene};

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn main() {
    let ar = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut tex: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut add = |t: &sushi_formats::texture::Texture| {
        let r = &t.raw;
        let k = format!(
            "fmt {} type {} app {:?} mips {} bytes1bd {} b1be {} b1bf {} w {} h {} faces {} cube {} size {:?}",
            u32_at(r, 0x100),
            u32_at(r, 0x104),
            cstr(&r[0x10c..0x140]),
            r[0x1bc],
            r[0x1bd],
            r[0x1be],
            r[0x1bf],
            u16::from_le_bytes([r[0x1d0], r[0x1d1]]),
            u16::from_le_bytes([r[0x1d2], r[0x1d3]]),
            r[0x1d4],
            r[0x1d5],
            (u32_at(r, 0x1c0), t.texels.len())
        );
        let e = tex.entry(k).or_insert((0, t.name.clone()));
        e.0 += 1;
    };
    let s = scene::read_scene(ar.get("Scenes/Animusic.ati").unwrap()).unwrap();
    s.textures.iter().for_each(&mut add);
    let g = scene::read_scene(ar.get("Globals.ati").unwrap()).unwrap();
    g.textures.iter().for_each(&mut add);
    for n in ar.names().filter(|n| n.ends_with(".so2")) {
        object::read_object(ar.get(n).unwrap())
            .unwrap()
            .textures
            .iter()
            .for_each(&mut add);
    }
    for (k, (n, name)) in &tex {
        println!("{n:3} {k} e.g. {}", name.rsplit('/').next().unwrap());
    }
    for (i, g) in s.geometry.iter().enumerate() {
        println!("geom {i} raw {}", hex(&g.raw));
        for d in &g.chunk.decls {
            println!("  decl {} : {}", hex(&d.0), hex(&d.1));
        }
        for st in &g.chunk.streams {
            println!("  stream {}", hex(&st.raw));
            for e in &st.elements {
                println!("    elem {}", hex(e));
            }
            for b in st.buffers.iter().flatten() {
                println!(
                    "    vb {} len {} first {}",
                    hex(&b.0),
                    b.1.len(),
                    hex(&b.1[..b.1.len().min(44)])
                );
            }
        }
        for (r, ix) in &g.chunk.indices {
            println!(
                "  ib {} n {} {:?}",
                hex(r),
                ix.len(),
                &ix[..ix.len().min(9)]
            );
        }
        for m in &g.chunk.maps {
            println!("  map {} : {}", hex(&m.0), hex(&m.1));
        }
    }
    println!(
        "list_be {:?}\nlist_c6 {:?}",
        s.list_be.iter().map(|x| *x as i32).collect::<Vec<_>>(),
        s.list_c6
    );
    for (i, g) in s.groups.iter().enumerate().take(6) {
        println!(
            "group {i} {} items {} {:?}",
            hex(&g.raw),
            g.items.len(),
            &g.items[..g.items.len().min(4)]
        );
    }
    for (i, d) in s.draws.iter().enumerate() {
        let t: Vec<_> = (0..64)
            .map(|k| u32_at(&d.raw, 4 + k * 4) as i32)
            .enumerate()
            .filter(|x| x.1 != -1)
            .collect();
        println!(
            "draw {i} geom {} tex {t:?} nl {} lights {:?} tail {} t118 {} t120 {}",
            u32_at(&d.raw, 0),
            u32_at(&d.raw, 0x104),
            (0..4)
                .map(|k| u32_at(&d.raw, 0x108 + k * 4) as i32)
                .collect::<Vec<_>>(),
            hex(&d.raw[0x118..]),
            d.table_118.len(),
            d.table_120.len()
        );
    }
    for (i, l) in s.lights.iter().enumerate() {
        println!(
            "light {i} {:?}",
            (0..0x4b / 4)
                .map(|k| f32_at(&l.raw, k * 4))
                .collect::<Vec<_>>()
        );
        println!("   tail {}", hex(&l.raw[0x4b..]));
    }
    println!(
        "header floats {:?}",
        (0x80..0x9a - 3)
            .step_by(4)
            .map(|o| f32_at(&s.header, o))
            .collect::<Vec<_>>()
    );
    println!("header {}", hex(&s.header[0x80..]));
    for c in s.cells.iter().take(3) {
        println!(
            "cell {} ee {} fe {} 106 {:?}",
            hex(&c.raw),
            c.list_ee.len(),
            c.list_fe.len(),
            c.list_106
        );
    }
    for (i, m) in s.table_112.iter().enumerate() {
        println!("t112 {i} {}", hex(m));
    }
    for (i, m) in s.materials98.iter().enumerate().take(3) {
        println!("m98 {i} {}", hex(m));
    }
}
