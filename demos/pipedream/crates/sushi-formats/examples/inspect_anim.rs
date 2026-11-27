//! Prints bone track statistics: `inspect_anim <Sushi.ati> [name-filter]`.
use sushi_formats::{anim, archive::Archive};
fn main() {
    let mut a = std::env::args().skip(1);
    let ar = Archive::open(&std::fs::read(a.next().unwrap()).unwrap()).unwrap();
    let filter = a.next().unwrap_or_default();
    for n in ar
        .names()
        .filter(|n| n.ends_with(".sa2") && n.contains(&filter))
    {
        let an = anim::read_animation(ar.get(n).unwrap()).unwrap();
        for (i, t) in an.bones.iter().enumerate() {
            let d = t.dts();
            let bad: Vec<_> = d
                .iter()
                .enumerate()
                .filter(|(_, x)| !x.is_finite() || **x < 0.0)
                .collect();
            let interior: f32 = d[..d.len().saturating_sub(1)].iter().sum();
            println!(
                "{n} bone {i} type {} keys {} dur {} max {} sum(all but last) {interior:.2} first {:?} last {:?} bad {:?}",
                t.kind(),
                d.len(),
                an.duration,
                an.max_time,
                d.first(),
                d.last(),
                &bad[..bad.len().min(4)]
            );
        }
    }
}
