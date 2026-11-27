use sushi_formats::{archive::Archive, object, texture::Format};
fn main() {
    let a = Archive::open(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    for n in a.names().filter(|n| n.ends_with(".so2")) {
        for t in object::read_object(a.get(n).unwrap()).unwrap().textures {
            if t.format() != Some(Format::Ati2n)
                || t.texels.is_empty()
                || !(t.name.to_lowercase().contains("nm") || t.name.to_lowercase().contains("bump"))
            {
                continue;
            }
            let l = &t.decode().unwrap()[0][0];
            let p = |x: usize, y: usize, c: usize| {
                f64::from(l.rgba[((y % l.height) * l.width + x % l.width) * 4 + c]) - 128.0
            };
            let cor = |f: &dyn Fn(usize, usize) -> (f64, f64)| {
                let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
                for y in 0..l.height {
                    for x in 0..l.width {
                        let (u, v) = f(x, y);
                        ab += u * v;
                        aa += u * u;
                        bb += v * v;
                    }
                }
                ab / (aa * bb).sqrt()
            };
            // x is red: d(green)/dx = d(red)/dy
            let asis = cor(&|x, y| (p(x + 1, y, 1) - p(x, y, 1), p(x, y + 1, 0) - p(x, y, 0)));
            let swap = cor(&|x, y| (p(x + 1, y, 0) - p(x, y, 0), p(x, y + 1, 1) - p(x, y, 1)));
            println!(
                "{} asis {asis:.3} swapped {swap:.3}",
                t.name.rsplit('/').next().unwrap()
            );
        }
    }
}
