//! Prints frames with their matrices and tracks' first keys: `xframes FILE`.
use parhelia_reef::xfile::Model;

fn main() {
    let f = std::env::args().nth(1).expect("file");
    let m = Model::read(&std::fs::read(&f).expect("read")).expect("parse");
    for (i, fr) in m.frames.iter().enumerate() {
        println!(
            "{i} {} parent {:?}\n   {:?}",
            fr.name,
            fr.parent,
            fr.matrix.to_cols_array()
        );
    }
    for t in &m.tracks {
        println!(
            "track {} rot {:?} pos {:?} scale {:?}",
            t.frame,
            t.rotation.first(),
            t.position.first(),
            t.scale.first()
        );
    }
    for x in &m.meshes {
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for p in &x.positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        println!("mesh in {}: {lo} .. {hi}", x.frame);
    }
}
