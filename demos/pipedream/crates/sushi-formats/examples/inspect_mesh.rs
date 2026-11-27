//! Dumps object mesh internals: `inspect_mesh <Sushi.ati> <name-filter>`.
use sushi_formats::reader::{cstr, f32_at, u32_at};
use sushi_formats::{anim, archive::Archive, object};
fn main() {
    let mut a = std::env::args().skip(1);
    let ar = Archive::open(&std::fs::read(a.next().unwrap()).unwrap()).unwrap();
    let filter = a.next().unwrap_or_default().to_lowercase();
    for n in ar.names().filter(|n| n.contains(&filter)) {
        if n.ends_with(".so2") {
            let o = object::read_object(ar.get(n).unwrap()).unwrap();
            println!(
                "== {n} bounds {:?} textures {:?}",
                (0..10)
                    .map(|i| f32_at(&o.bounds, i * 4))
                    .collect::<Vec<_>>(),
                o.textures
                    .iter()
                    .map(|t| t.name.rsplit('/').next().unwrap().to_string())
                    .collect::<Vec<_>>()
            );
            for m in &o.meshes {
                println!(
                    " mesh {} flags {:x} raw0 {:?} tail {:?}",
                    m.shader,
                    u32_at(&m.raw, 0),
                    (0x84..0xc8)
                        .step_by(4)
                        .map(|k| u32_at(&m.raw, k))
                        .collect::<Vec<_>>(),
                    m.frame_keys
                );
                println!("  misc {:?} tex {:?}", m.header.misc, m.header.textures);
                println!(
                    "  streams {:?} maps {:?}",
                    m.header.streams, m.header.stream_maps
                );
                for f in &m.frames {
                    for v in f {
                        println!(
                            "  vb stride {} n {} raw {:?} first {:02x?}",
                            v.stride,
                            v.count,
                            (0..6).map(|k| u32_at(&v.raw, k * 4)).collect::<Vec<_>>(),
                            &v.data[..v.stride.min(v.data.len())]
                        );
                    }
                }
                for i in &m.index_buffers {
                    println!(
                        "  ib n {} raw {:?} {:?}",
                        i.indices.len(),
                        (0..4).map(|k| u32_at(&i.raw, k * 4)).collect::<Vec<_>>(),
                        &i.indices[..i.indices.len().min(12)]
                    );
                }
                for c in &m.commands {
                    println!("  cmds {} {:?}", c.len(), &c[..c.len().min(8)]);
                }
                for s in &m.stream_arrays {
                    println!(
                        "  sa raw {:?} vb {:?} off {:?} tail {:?}",
                        (0..7).map(|k| u32_at(&s.raw, k * 4)).collect::<Vec<_>>(),
                        s.vertex_buffers,
                        s.offsets,
                        s.tail
                    );
                }
                println!("  smaps {:?}", m.stream_maps);
                for p in &m.params {
                    println!(
                        "  param {} raw {:?} {:?}",
                        p.name,
                        (0x40..0x50)
                            .step_by(4)
                            .map(|k| u32_at(&p.raw, k))
                            .collect::<Vec<_>>(),
                        p.values
                    );
                }
            }
        } else if n.ends_with(".sa2") {
            let an = anim::read_animation(ar.get(n).unwrap()).unwrap();
            println!(
                "== {n} unk0 {} dur {} bones {:?} morph {:?} meshes {} max {} nb {} ",
                an.unk0,
                an.duration,
                an.bones
                    .iter()
                    .map(|b| (b.kind(), b.dts().len()))
                    .collect::<Vec<_>>(),
                an.morph_header,
                an.morph_meshes.len(),
                an.max_time,
                an.bounds.len()
            );
            for p in &an.params {
                println!(
                    "  param {} raw {:?} keys {} {:?}",
                    cstr(&p.raw[4..]),
                    (0x44..0x5c)
                        .step_by(4)
                        .map(|k| u32_at(&p.raw, k))
                        .collect::<Vec<_>>(),
                    p.keys.len(),
                    &p.keys[..p.keys.len().min(5)]
                );
            }
        }
    }
}
