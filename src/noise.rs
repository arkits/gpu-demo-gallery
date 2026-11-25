//! The original's noise, and the pictures it makes of it.
use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use crate::Error;
use crate::image::Image;

/// `rand` of the original's C library.
pub struct Random(u32);

impl Random {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn raw(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        self.0 >> 16 & 0x7fff
    }
    /// 0 to 1.
    pub fn part(&mut self) -> f32 {
        self.raw() as f32 / 32767.0
    }
    /// -1 to 1.
    pub fn even(&mut self) -> f32 {
        self.part() * 2.0 - 1.0
    }
    pub fn byte(&mut self) -> u32 {
        self.raw() & 0xff
    }
}

pub struct Noise {
    order: [u8; 256],
    gradients: [Vec3; 256],
    values: [f32; 256],
    /// 16 by 16 values, for pictures.
    table: [f32; 256],
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Noise {
    /// From the original's table of 256 places (0x4154c4).
    pub fn new(order: &[u8]) -> Result<Self, Error> {
        const SEED: u32 = 0x17e6a55;
        let order: [u8; 256] = order.try_into()?;
        let mut random = Random::new(SEED);
        // 0x4068a0.
        let gradients = std::array::from_fn(|_| {
            let z = 1.0 - 2.0 * random.part();
            let (y, x) = (random.part() * TAU).sin_cos();
            let r = (1.0 - z * z).sqrt();
            Vec3::new(x * r, y * r, z)
        });
        let mut random = Random::new(SEED);
        let values = std::array::from_fn(|_| -random.even());
        let mut random = Random::new(0x3d331);
        let table = std::array::from_fn(|_| random.even());
        Ok(Self {
            order,
            gradients,
            values,
            table,
        })
    }

    fn at(&self, i: i32) -> i32 {
        i32::from(self.order[(i & 255) as usize])
    }

    /// 0x4069f0.
    pub fn three(&self, p: Vec3) -> f32 {
        self.periodic(p, None)
    }

    /// Repeats after so many steps along each axis (0x406e40).
    fn periodic(&self, p: Vec3, period: Option<[i32; 3]>) -> f32 {
        let cell = p.floor();
        let f = p - cell;
        let cell = cell.as_ivec3();
        let corner = |x: i32, y: i32, z: i32| {
            let mut at = [cell.x + x, cell.y + y, cell.z + z];
            if let Some(n) = period {
                at = std::array::from_fn(|i| at[i].rem_euclid(n[i]));
            }
            let i = self.at(self.at(self.at(at[2]) + at[1]) + at[0]);
            let off = Vec3::new(x as f32, y as f32, z as f32);
            self.gradients[i as usize].dot(f - off)
        };
        let (u, v, w) = (ease(f.x), ease(f.y), ease(f.z));
        let row = |y, z| mix(corner(0, y, z), corner(1, y, z), u);
        let layer = |z| mix(row(0, z), row(1, z), v);
        mix(layer(0), layer(1), w)
    }

    /// 0x406c60.
    pub fn two(&self, x: f32, y: f32) -> f32 {
        let p = Vec2::new(x, y);
        let cell = p.floor();
        let f = p - cell;
        let cell = cell.as_ivec2();
        let corner = |x: i32, y: i32| {
            let i = self.at(self.at(cell.y + y) + cell.x + x);
            let off = Vec2::new(x as f32, y as f32);
            self.gradients[i as usize].truncate().dot(f - off)
        };
        let row = |y| mix(corner(0, y), corner(1, y), ease(f.x));
        mix(row(0), row(1), ease(f.y))
    }

    /// One value for each whole place (0x4072c0).
    pub fn value(&self, x: f32, y: f32, z: f32) -> f32 {
        let [x, y, z] = [x, y, z].map(|v| v as i32);
        self.values[self.at(self.at(self.at(z) + y) + x) as usize]
    }

    /// Of the table (0x4074b0).
    pub fn cell(&self, x: i32, y: i32) -> f32 {
        self.table[((y & 15) * 16 + (x & 15)) as usize]
    }

    /// The table, smoothly (0x4074e0).
    fn smooth(&self, x: f32, y: f32) -> f32 {
        let weights = |t: f32| {
            const A: f32 = -0.75;
            let w = |t: f32| match t {
                t if t < 1.0 => ((A + 2.0) * t - (A + 3.0)) * t * t + 1.0,
                t => ((t * A - A * 5.0) * t + A * 8.0) * t - A * 4.0,
            };
            [w(t + 1.0), w(t), w(1.0 - t), w(2.0 - t)]
        };
        let (cx, cy) = (x.floor(), y.floor());
        let (wx, wy) = (weights(x - cx), weights(y - cy));
        let mut sum = 0.0;
        for (j, wy) in (cy as i32 - 1..).zip(wy) {
            for (i, wx) in (cx as i32 - 1..).zip(wx) {
                sum += wx * wy * self.cell(i, j);
            }
        }
        sum.clamp(-1.0, 1.0)
    }

    /// 256 by 256 (0x4076b0): clouds, plasma, fire, shadows of clouds.
    pub fn picture(&self, kind: usize, sky: &[f32]) -> Image {
        let mut rgba = Vec::with_capacity(256 * 256 * 4);
        for j in 0..256 {
            for i in 0..256 {
                let n = self.smooth(j as f32 / 16.0, i as f32 / 16.0);
                let even = (n + 1.0) * 0.5;
                let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
                rgba.extend(match kind {
                    0 => {
                        let [r, g, b] = spline(even, sky).map(byte);
                        [r, g, b, 255]
                    }
                    1 => {
                        let v = byte(1.0 - 2.0 * n.abs());
                        [0, v, v, 255]
                    }
                    2 => [255, byte(n.abs()), 0, 255],
                    _ => [byte(even + 0.25); 4],
                });
            }
        }
        Image {
            width: 256,
            height: 256,
            rgba,
        }
    }

    /// Parts of the sun's flare (0x401a90).
    pub fn flare(&self, kind: usize, size: u32, rainbow: &[f32]) -> Image {
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for j in 0..size {
            for i in 0..size {
                let at = |k: u32| 2.0 * k as f32 / size as f32 - 1.0;
                let (x, y) = (at(j), at(i));
                let r = x.hypot(y).min(1.0);
                let disk = 1.0 - smooth_step(0.9, 1.0, r);
                let gray = match kind {
                    0 => {
                        let color = ring(x.hypot(y), rainbow);
                        rgba.extend(color.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8));
                        rgba.push(255);
                        continue;
                    }
                    1 => {
                        let turn = y.atan2(x) / TAU * 20.0;
                        let n = self.periodic(Vec3::new(turn, 0.0, 0.0), Some([20, 100, 100]));
                        2.0 * (1.0 - r + 0.5 * n).max(0.0).powi(2)
                    }
                    3 => disk,
                    4 => smooth_step(0.8, 0.9, r) * disk,
                    5 => (1.0 - r).powi(3),
                    6 => r.powi(3) * disk,
                    _ => {
                        let arm = |v: f32| (v * 10.0 * PI).clamp(-PI / 2.0, PI / 2.0).cos();
                        let star = arm(x) * (1.0 - y.abs()) + arm(y) * (1.0 - x.abs());
                        (1.0 - r).powf(0.4) * 4.0 + star.clamp(0.0, 1.0)
                    }
                };
                let v = (gray.clamp(0.0, 1.0) * 255.0) as u8;
                rgba.extend([v, v, v, 255]);
            }
        }
        Image {
            width: size,
            height: size,
            rgba,
        }
    }
}

fn smooth_step(a: f32, b: f32, x: f32) -> f32 {
    ease(((x - a) / (b - a)).clamp(0.0, 1.0))
}

/// Through colors, of which the first and last only bend it (0x4071d0).
fn spline(x: f32, knots: &[f32]) -> [f32; 3] {
    let spans = knots.len() / 3 - 3;
    let x = x.clamp(0.0, 1.0) * spans as f32;
    let i = (x as usize).min(spans - 1);
    let t = x - i as f32;
    std::array::from_fn(|c| {
        let k = |j: usize| knots[(i + j) * 3 + c];
        let a = 1.5 * k(1) - 1.5 * k(2) + 0.5 * k(3) - 0.5 * k(0);
        let b = k(0) - 2.5 * k(1) + 2.0 * k(2) - 0.5 * k(3);
        (((a * t + b) * t) + 0.5 * k(2) - 0.5 * k(0)) * t + k(1)
    })
}

/// 0x401520.
fn ring(r: f32, rainbow: &[f32]) -> [f32; 3] {
    const WIDTH: f32 = 0.2;
    let inner = 1.0 - WIDTH;
    if r < inner || r > 1.0 {
        return [0.0; 3];
    }
    let across = (r - inner) / WIDTH;
    let part = smooth_step(inner, inner + 0.05, r) * (1.0 - smooth_step(0.95, 1.0, r));
    spline(1.0 - ((across - 0.5) * 1.2 + 0.5), rainbow).map(|v| v.clamp(0.0, 1.0) * part)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise() -> Noise {
        let order: Vec<u8> = (0..=255).collect();
        Noise::new(&order).unwrap()
    }

    #[test]
    fn noise_is_smooth_and_none_at_whole_places() {
        let n = noise();
        assert!(n.two(3.0, -7.0).abs() < 1e-6);
        assert!(n.three(Vec3::new(-2.0, 5.0, 1.0)).abs() < 1e-6);
        for i in 0..1000 {
            let x = i as f32 * 0.037 - 18.0;
            let d = n.two(x, 0.3) - n.two(x + 0.001, 0.3);
            assert!(d.abs() < 0.01, "{x}");
        }
        let a = n.periodic(Vec3::new(0.25, 0.5, 0.0), Some([20, 100, 100]));
        let b = n.periodic(Vec3::new(20.25, 0.5, 0.0), Some([20, 100, 100]));
        assert!((a - b).abs() < 1e-4);
    }

    #[test]
    fn splines_pass_their_colors() {
        let knots: Vec<f32> = (0..18).map(|i| i as f32).collect();
        assert_eq!(spline(0.0, &knots), [3.0, 4.0, 5.0]);
        assert_eq!(spline(1.0, &knots), [12.0, 13.0, 14.0]);
    }

    #[test]
    fn random_numbers_are_the_library_s() {
        // The first three of `srand(1)`.
        let mut r = Random::new(1);
        assert_eq!([r.raw(), r.raw(), r.raw()], [41, 18467, 6334]);
    }
}
