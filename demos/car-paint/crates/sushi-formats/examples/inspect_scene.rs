//! Prints scene and object summaries: `inspect_scene <Sushi.ati>`.
use sushi_formats::{archive::Archive, object, reader::u32_at, scene};

fn main() {
    let ar = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for n in ["Globals.ati", "Scenes/CarRoom.ati"] {
        let s = scene::read_scene(ar.get(n).unwrap()).unwrap();
        println!(
            "{n}: cells {} groups {:?} geometry {} draws {} lights {} (fastrt {}, ambient {}) mirrors {}",
            s.cells.len(),
            s.groups.iter().map(|g| g.items.len()).collect::<Vec<_>>(),
            s.geometry.len(),
            s.draws.len(),
            s.lights.len(),
            s.lights.iter().filter(|l| l.is_fast_rt()).count(),
            s.lights.iter().filter(|l| l.is_object_ambient()).count(),
            s.mirrors.len(),
        );
        for (i, t) in s.textures.iter().enumerate() {
            println!(
                "  texture {i} {} {:?} {}x{}",
                t.name,
                t.format(),
                t.width(),
                t.height()
            );
        }
        for (i, g) in s.geometry.iter().enumerate() {
            let sh = &s.shaders[g.shader()];
            let name = sushi_formats::reader::cstr(&sh.raw);
            let st = &g.chunk.streams[0];
            println!(
                "  geometry {i} trans {} {name} stride {} {:?}",
                g.transparent(),
                st.stride(),
                st.attributes()
                    .iter()
                    .map(|a| a.semantic)
                    .collect::<Vec<_>>()
            );
        }
        for m in &s.mirrors {
            println!(
                "  mirror plane {:?} sets {:?}",
                m.plane(),
                m.sets
                    .iter()
                    .map(|v| v.groups.iter().map(|g| g.items.len()).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            );
        }
    }
    for n in ar.names().filter(|n| n.ends_with(".so2")) {
        let o = object::read_object(ar.get(n).unwrap()).unwrap();
        println!("{n}");
        for m in &o.meshes {
            println!(
                "  {} flags {:#x} lods? {} params {:?}",
                m.shader,
                u32_at(&m.raw, 0),
                m.commands.len(),
                m.params.iter().map(|p| &p.name).collect::<Vec<_>>()
            );
        }
        for t in &o.textures {
            println!(
                "  texture {} {:?} {}x{} cube {} bytes {}",
                t.name,
                t.format(),
                t.width(),
                t.height(),
                t.is_cube(),
                t.texels.len()
            );
        }
    }
}
