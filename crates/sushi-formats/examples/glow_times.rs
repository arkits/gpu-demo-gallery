//! Lists when marimba bars glow: `glow_times <Sushi.ati>`.
use sushi_formats::{anim, archive::Archive};
fn main() {
    let ar = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut out = Vec::new();
    for n in ar.names().filter(|n| n.ends_with("-g.sa2")) {
        let a = anim::read_animation(ar.get(n).unwrap()).unwrap();
        for p in a.params.iter().filter(|p| p.name == "SOLGlow") {
            let mut t = 0.0;
            for k in &p.keys {
                if k[1] > 0.5 {
                    out.push((t, n, k[1]));
                }
                t += k[0];
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (t, n, g) in out {
        println!("{t:.2} {n} {g:.2}");
    }
}
