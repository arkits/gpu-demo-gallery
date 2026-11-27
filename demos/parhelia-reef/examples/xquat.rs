//! Compares each animated frame's own rotation with its first rotation key: `xquat FILE`.
use parhelia_reef::xfile::Model;

fn main() {
    let f = std::env::args().nth(1).expect("file");
    let m = Model::read(&std::fs::read(&f).expect("read")).expect("parse");
    let (mut same, mut conj, mut other) = (0, 0, 0);
    for t in &m.tracks {
        let Some(&(_, q)) = t.rotation.first() else {
            continue;
        };
        let Some(fr) = m.frames.iter().find(|f| f.name == t.frame) else {
            continue;
        };
        let (_, b, _) = fr.matrix.to_scale_rotation_translation();
        let d1 = b.dot(q).abs();
        let d2 = b.dot(q.conjugate()).abs();
        let pos = t
            .position
            .first()
            .map(|p| (p.1 - fr.matrix.w_axis.truncate()).length());
        if same + conj + other < 6 {
            println!(
                "  {} base {b:?} key {q:?} pos diff {pos:?} d {d1:.3} {d2:.3}",
                t.frame
            );
        }
        if d1 > 0.999 && d2 < 0.999 {
            same += 1
        } else if d2 > 0.999 && d1 < 0.999 {
            conj += 1
        } else {
            other += 1
        }
    }
    println!("{f}: same {same} conjugate {conj} other {other}");
}
