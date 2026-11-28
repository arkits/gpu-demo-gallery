//! The teleporter's noise (0x40a640): Ken Perlin's reference `noise2` (0x4103a0, set up by
//! 0x410510 from `srand(0x15f)` and the C library's `rand`), six octaves of `|0.5 - n| / f`
//! a texel, kept in alpha and faded in towards the bottom rows. The picture is 256 by 512.
use crate::image::Image;

const B: usize = 0x100;
const N: f32 = 4096.0;

/// MSVC's `rand`.
struct Rand(u32);

impl Rand {
    fn next(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        ((self.0 >> 16) & 0x7fff) as i32
    }
}

struct Perlin {
    p: Vec<usize>,
    g2: Vec<[f32; 2]>,
}

impl Perlin {
    fn new() -> Self {
        let mut r = Rand(0x15f);
        let mut p = vec![0usize; B + B + 2];
        let mut g2 = vec![[0.0f32; 2]; B + B + 2];
        let unit = |r: &mut Rand| ((r.next() % (B + B) as i32) - B as i32) as f32 / B as f32;
        for i in 0..B {
            p[i] = i;
            let _g1 = unit(&mut r);
            let (x, y) = (unit(&mut r), unit(&mut r));
            let s = (x * x + y * y).sqrt();
            g2[i] = if s > 0.0 { [x / s, y / s] } else { [x, y] };
            // g3 is drawn too, which moves the generator on.
            for _ in 0..3 {
                unit(&mut r);
            }
        }
        for i in (1..B).rev() {
            let j = (r.next() % B as i32) as usize;
            p.swap(i, j);
        }
        for i in 0..B + 2 {
            p[B + i] = p[i];
            g2[B + i] = g2[i];
        }
        Self { p, g2 }
    }

    fn noise2(&self, x: f32, y: f32) -> f32 {
        let setup = |v: f32| {
            let t = v + N;
            let i = t as i32;
            let b0 = (i & 0xff) as usize;
            let r0 = t - i as f32;
            (b0, (b0 + 1) & 0xff, r0, r0 - 1.0)
        };
        let (bx0, bx1, rx0, rx1) = setup(x);
        let (by0, by1, ry0, ry1) = setup(y);
        let (i, j) = (self.p[bx0], self.p[bx1]);
        let (b00, b10, b01, b11) = (
            self.p[i + by0],
            self.p[j + by0],
            self.p[i + by1],
            self.p[j + by1],
        );
        let s = |t: f32| t * t * (3.0 - 2.0 * t);
        let (sx, sy) = (s(rx0), s(ry0));
        let at = |q: [f32; 2], rx: f32, ry: f32| rx * q[0] + ry * q[1];
        let lerp = |t: f32, a: f32, b: f32| a + t * (b - a);
        let a = lerp(sx, at(self.g2[b00], rx0, ry0), at(self.g2[b10], rx1, ry0));
        let b = lerp(sx, at(self.g2[b01], rx0, ry1), at(self.g2[b11], rx1, ry1));
        lerp(sy, a, b)
    }
}

pub fn teleport() -> Image {
    let perlin = Perlin::new();
    let (w, h) = (256u32, 512u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let v = row as f32 * (1.0 / 511.0);
        for col in 0..w {
            let u = col as f32 * (1.0 / 255.0);
            let (mut f, mut s) = (1.0f32, 0.0f32);
            for _ in 0..6 {
                s += (0.5 - perlin.noise2(f * u * 16.0, f * v * 16.0)).abs() / f;
                f += f;
            }
            let a = ((s + 0.05) * v).min(1.0) * 255.0;
            rgba.extend([255, 255, 255, a as u8]);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}
