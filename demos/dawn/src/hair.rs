//! Hair: guide curves from a text file and the lines between them (`FUN_0049b310`).
use glam::{Vec2, Vec3};

use crate::Error;

pub struct Point {
    pub position: Vec3,
    pub tangent: Vec3,
    pub normal: Vec3,
    /// Which hair, and how far along it.
    pub color: Vec2,
}

pub struct Hair {
    /// Points of one hair after another.
    pub points: Vec<Point>,
    pub per_hair: usize,
}

/// Microsoft's `rand`, which the program never seeds.
struct Rand(u32);

impl Rand {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.0 >> 16 & 0x7fff) as f32 / 32767.0
    }
}

impl Hair {
    pub fn new(text: &str, density: f32, segments: usize) -> Result<Self, Error> {
        let mut words = text.split_ascii_whitespace();
        let mut number =
            || -> Result<f32, Error> { Ok(words.next().ok_or("hair file is cut short")?.parse()?) };
        let limit = |n: f32| -> Result<usize, Error> {
            match n {
                1.0..=100_000.0 => Ok(n as usize),
                _ => Err("bad count in hair file".into()),
            }
        };
        let segments = segments.clamp(1, 64);
        let count = limit(number()?)?;
        let mut guides = Vec::with_capacity(count);
        for _ in 0..count {
            // Width and three directions are unused by this kind of hair.
            let mut v = [0.0; 22];
            for x in &mut v {
                *x = number()?;
            }
            let p: [Vec3; 4] = std::array::from_fn(|i| Vec3::from_slice(&v[10 + i * 3..]));
            let points = (0..=segments).map(|i| {
                let t = i as f32 / segments as f32;
                let s = 1.0 - t;
                s * s * s * p[0]
                    + 3.0 * t * s * s * p[1]
                    + 3.0 * t * t * s * p[2]
                    + t * t * t * p[3]
            });
            guides.push(points.collect::<Vec<_>>());
        }
        let mut rand = Rand(1);
        let mut strands: Vec<Vec<Vec3>> = Vec::new();
        for _ in 0..limit(number()?)? {
            let mut corner = || -> Result<&Vec<Vec3>, Error> {
                let i = number()?;
                Ok(guides.get(i as usize).ok_or("bad triangle in hair file")?)
            };
            let (a, b, c) = (corner()?, corner()?, corner()?);
            let area = (b[0] - a[0]).cross(c[0] - a[0]).length() * 0.5;
            for _ in 0..((area * density) as usize).max(1) {
                let (mut u, mut v) = (rand.unit(), rand.unit());
                if u + v > 1.0 {
                    (u, v) = (1.0 - u, 1.0 - v);
                }
                let w = 1.0 - (u + v);
                strands.push(
                    (0..=segments)
                        .map(|i| u * a[i] + v * b[i] + w * c[i])
                        .collect(),
                );
            }
        }
        let per_hair = segments + 1;
        let total = strands.len() as f32;
        let mut points = Vec::with_capacity(strands.len() * per_hair);
        for (n, s) in strands.iter().enumerate() {
            for (i, &position) in s.iter().enumerate() {
                let at = i.min(segments - 1);
                points.push(Point {
                    position,
                    tangent: (s[at + 1] - s[at]).normalize_or_zero(),
                    // The program takes the direction from the middle of the head.
                    normal: position.normalize_or_zero(),
                    color: Vec2::new(n as f32 / total, i as f32 / segments as f32),
                });
            }
        }
        Ok(Self { points, per_hair })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_between_guides() {
        let guide = |x: f32, y: f32| {
            format!("0.1\n1 0 0\n0 1 0\n0 0 1\n{x} {y} 0\n{x} {y} 1\n{x} {y} 2\n{x} {y} 3\n")
        };
        let text = format!(
            "3\n{}{}{}1\n0 1 2\n",
            guide(0.0, 0.0),
            guide(1.0, 0.0),
            guide(0.0, 1.0)
        );
        let h = Hair::new(&text, 20.0, 3).unwrap();
        assert_eq!((h.points.len(), h.per_hair), (40, 4));
        assert!((h.points[3].position.z - 3.0).abs() < 1e-5);
        assert!(h.points.iter().all(|p| p.position.x + p.position.y < 1.001));
        assert!((h.points[0].tangent.z - 1.0).abs() < 1e-5);
        assert!(Hair::new(&text[..text.len() - 4], 20.0, 3).is_err());
    }
}
