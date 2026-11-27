use std::collections::BTreeMap;
use sushi_formats::{anim, archive::Archive, object, reader::*, scene, texture::Texture};
fn main() {
    let a = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut fm: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    let mut total = 0;
    let mut tex = |t: &Texture, n: &str| {
        let k = format!(
            "fmt {} faces {} 1d5={} ",
            u32_at(&t.raw, 0x100),
            t.raw[0x1d4],
            t.raw[0x1d5]
        );
        let e = fm.entry(k).or_default();
        e.0 += 1;
        e.1 += t.texels.len();
        if e.2.len() < 3 && !t.texels.is_empty() {
            e.2.push(format!(
                "{n}:{} {}x{} {}",
                t.name,
                t.width(),
                t.height(),
                t.texels.len()
            ));
        }
    };
    let mut sem: BTreeMap<String, usize> = BTreeMap::new();
    let mut cmds: BTreeMap<u32, usize> = BTreeMap::new();
    let mut adb: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut meshes, mut frames) = (0, vec![]);
    let mut vis = 0;
    for n in a.names() {
        let d = a.get(n).unwrap();
        total += d.len();
        let mut mesh = |m: &sushi_formats::mesh::Mesh| {
            meshes += 1;
            if m.frames.len() != 1 {
                frames.push(format!("{n} {} {}", m.shader, m.frames.len()));
            }
            for s in &m.header.streams {
                for e in &s.elements {
                    *sem.entry(format!(
                        "{}{} kind{} usage{} {}",
                        e.semantic().0,
                        e.semantic().1,
                        e.kind,
                        e.usage,
                        e.source
                    ))
                    .or_default() += 1;
                }
            }
            for c in m.commands.iter().flatten() {
                *cmds.entry(c[0]).or_default() += 1;
            }
        };
        let mut blk = |b: &Vec<sushi_formats::mesh::Blocks>| {
            for b in b {
                adb.entry(n.into()).or_default().push(format!(
                    "id{} {:?}",
                    b.raw_id,
                    b.blocks
                        .iter()
                        .map(|x| (x.0, x.1.len()))
                        .collect::<Vec<_>>()
                ));
            }
        };
        if n.ends_with(".so2") {
            let o = object::read_object(d).unwrap();
            for t in &o.textures {
                tex(t, n);
            }
            o.meshes.iter().for_each(&mut mesh);
            blk(&o.blocks);
        }
        if n.ends_with(".sa2") {
            let o = anim::read_animation(d).unwrap();
            o.morph_meshes.iter().for_each(&mut mesh);
            blk(&o.blocks);
            if o.params.iter().any(|p| p.name == "SOLVisibility") {
                vis += 1;
            }
        }
        if n.ends_with(".ati") {
            let s = if n == "globals.ati" {
                scene::read_globals(d)
            } else {
                scene::read_scene(d)
            }
            .unwrap();
            for t in s.textures.iter().chain(&s.textures_182) {
                tex(t, n);
            }
            println!(
                "{n}: cells {} groups {} geom {} draws {} lights {} tex {} shaders {} mirrors {} quads {} particles {} t18a {} t192 {}",
                s.cells.len(),
                s.groups.len(),
                s.geometry.len(),
                s.draws.len(),
                s.lights.len(),
                s.textures.len(),
                s.shaders.len(),
                s.mirrors.len(),
                s.table_112.len(),
                s.particles.len(),
                s.table_18a.len(),
                s.table_192.len()
            );
        }
    }
    println!("total {total} meshes {meshes} vis {vis}");
    for (k, v) in fm {
        println!("{k} {} {} {:?}", v.0, v.1, v.2);
    }
    println!("{sem:#?}\n{cmds:?}");
    for (k, v) in adb {
        println!("{k} {:?}", v.iter().take(4).collect::<Vec<_>>());
    }
    println!("{:?}", &frames[..frames.len().min(30)]);
}
