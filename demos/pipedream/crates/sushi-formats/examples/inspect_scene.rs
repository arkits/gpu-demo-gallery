//! Prints scene and shader-param summaries: `inspect_scene <Sushi.ati>`.
use std::collections::BTreeMap;
use sushi_formats::{archive::Archive, object, scene};
fn main() {
    let ar = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for n in ["Globals.ati", "Scenes/Animusic.ati"] {
        let s = scene::read_scene(ar.get(n).unwrap()).unwrap();
        println!(
            "{n}: cells {} groups {} geometry {} draws {} lights {} textures {} shaders {}",
            s.cells.len(),
            s.groups.len(),
            s.geometry.len(),
            s.draws.len(),
            s.lights.len(),
            s.textures.len(),
            s.shaders.len()
        );
        for g in &s.geometry {
            let vb: Vec<usize> = g
                .chunk
                .streams
                .iter()
                .flat_map(|s| s.buffers.iter().flatten().map(|b| b.1.len()))
                .collect();
            println!(
                "  geom: streams {} elems {:?} vb bytes {:?} index counts {:?}",
                g.chunk.streams.len(),
                g.chunk
                    .streams
                    .iter()
                    .map(|s| s.elements.len())
                    .collect::<Vec<_>>(),
                &vb[..vb.len().min(6)],
                g.chunk
                    .indices
                    .iter()
                    .map(|i| i.1.len())
                    .take(6)
                    .collect::<Vec<_>>()
            );
        }
        for sh in &s.shaders {
            let nm: String = sh
                .raw
                .iter()
                .take_while(|b| **b != 0)
                .map(|b| *b as char)
                .collect();
            println!(
                "  shader {nm:?} misc {:?} streams {:?} textures {:?}",
                sh.header.misc,
                sh.header
                    .streams
                    .iter()
                    .map(|s| (
                        &s.name,
                        s.elements.iter().map(|e| &e.source).collect::<Vec<_>>()
                    ))
                    .collect::<Vec<_>>(),
                sh.header.textures
            );
        }
    }
    let mut strides: BTreeMap<String, usize> = BTreeMap::new();
    let mut verts = 0;
    let mut tris = 0;
    for n in ar.names().filter(|n| n.ends_with(".so2")) {
        for m in object::read_object(ar.get(n).unwrap()).unwrap().meshes {
            let k = format!(
                "{} strides {:?}",
                m.shader,
                m.frames
                    .iter()
                    .flatten()
                    .map(|v| v.stride)
                    .collect::<Vec<_>>()
            );
            *strides.entry(k).or_default() += 1;
            verts += m
                .frames
                .iter()
                .flatten()
                .map(|v| v.count)
                .max()
                .unwrap_or(0);
            tris += m
                .index_buffers
                .iter()
                .map(|i| i.indices.len())
                .sum::<usize>();
        }
    }
    println!("objects: {verts} verts, {tris} indices");
    for (k, v) in strides {
        println!("  {v:3} x {k}");
    }
}
