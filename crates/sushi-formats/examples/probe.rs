use sushi_formats::{anim, archive::Archive, object, scene};
fn main() {
    let a = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let (mut ok, mut bad) = (std::collections::BTreeMap::new(), 0);
    for n in a.names() {
        let d = a.get(n).unwrap();
        let ext = n.rsplit('.').next().unwrap();
        let r = match ext {
            "so2" => object::read_object(d).err(),
            "sa2" => anim::read_animation(d).err(),
            "spg" => sushi_formats::particles::read_particles(d).err(),
            "ati" if n == "globals.ati" => scene::read_globals(d).err(),
            "ati" => scene::read_scene(d).err(),
            _ => continue,
        };
        match r {
            None => *ok.entry(ext.to_string()).or_insert(0) += 1,
            Some(e) => {
                bad += 1;
                if bad < 40 {
                    println!("{n} ({}): {e}", d.len());
                }
            }
        }
    }
    println!("{ok:?} bad {bad}");
}
