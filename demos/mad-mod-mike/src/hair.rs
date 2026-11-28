//! Hair grown from guide curves (`hair_curves.txt`, `gotti_curves.txt`) with the
//! growth counts of `hairclumpsparams.txt` and `gotti_params.txt`.
//!
//! The viewer's dynamics (gravity, drag, relaxation in `hairparams.txt`) are
//! not reproduced: strands keep their grown shape with a slight sway [I].
use glam::Vec3;

use crate::Error;

pub struct Point {
    pub position: Vec3,
    pub tangent: Vec3,
    pub normal: Vec3,
    /// Which head of hair, and how far along the strand.
    pub color: [f32; 2],
}

pub struct Hair {
    /// Points of one strand after another.
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
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

fn guides(text: &str) -> Result<Vec<Vec<Vec3>>, Error> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("curve") {
            if !current.is_empty() {
                // Three points make a quadratic Bezier, four a cubic.
                if current.len() != 3 && current.len() != 4 {
                    return Err("bad guide curve".into());
                }
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if line == "end" || line == "eof" {
            if !current.is_empty() {
                if current.len() != 3 && current.len() != 4 {
                    return Err("bad guide curve".into());
                }
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if line.starts_with('/') {
            continue;
        }
        let n: Vec<f32> = line
            .split_whitespace()
            .map(|w| w.parse())
            .collect::<Result<_, _>>()
            .map_err(|_| Error::from("bad guide number"))?;
        if n.len() != 3 {
            return Err("bad guide point".into());
        }
        current.push(Vec3::from_slice(&n));
    }
    if !current.is_empty() {
        return Err("unterminated guide curve".into());
    }
    if out.is_empty() {
        return Err("no guide curves".into());
    }
    Ok(out)
}

fn bezier(points: &[Vec3], t: f32) -> Vec3 {
    let s = 1.0 - t;
    if points.len() == 3 {
        s * s * points[0] + 2.0 * t * s * points[1] + t * t * points[2]
    } else {
        s * s * s * points[0]
            + 3.0 * t * s * s * points[1]
            + 3.0 * t * t * s * points[2]
            + t * t * t * points[3]
    }
}

impl Hair {
    /// Grows `per_guide` strands along each guide, resampled to `segments`,
    /// with `radius` of jitter. `set` tells the shader which hair it is.
    pub fn new(
        text: &str,
        per_guide: usize,
        subdiv: usize,
        radius: f32,
        set: f32,
    ) -> Result<Self, Error> {
        let guides = guides(text)?;
        let segments = (subdiv * 2 + 1).clamp(2, 32);
        let per_guide = per_guide.clamp(1, 256);
        let mut rand = Rand(1);
        let mut points = Vec::new();
        for (g, guide) in guides.iter().enumerate() {
            let base: Vec<Vec3> = (0..=segments)
                .map(|i| bezier(guide, i as f32 / segments as f32))
                .collect();
            for _ in 0..per_guide {
                let j = Vec3::new(rand.signed(), rand.signed(), rand.signed()) * radius;
                // Strands share the guide's shape, offset and slightly fanned.
                let fan = if radius == 0.0 {
                    1.0
                } else {
                    1.0 + rand.signed() * 0.15
                };
                let strand: Vec<Vec3> = base
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let t = i as f32 / segments as f32;
                        base[0] + (*p - base[0]) * fan + j * (0.2 + t)
                    })
                    .collect();
                for (i, position) in strand.iter().enumerate() {
                    let at = i.min(segments - 1);
                    let tangent = (strand[at + 1] - strand[at]).normalize_or_zero();
                    let side = tangent.cross(Vec3::Y);
                    let normal = if side.length_squared() > 1e-6 {
                        side.normalize()
                    } else {
                        Vec3::X
                    };
                    points.push(Point {
                        position: *position,
                        tangent,
                        normal,
                        color: [g as f32, i as f32 / segments as f32],
                    });
                }
                let _ = set;
            }
        }
        Ok(Self {
            points,
            per_hair: segments + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_along_guides() {
        let text = "curve1\n0 0 0\n1 0 0\n2 0 0\n\nend\ncurve2\n0 1 0\n1 1 0\n2 1 0\n\nend\n";
        let h = Hair::new(text, 2, 2, 0.0, 0.0).unwrap();
        assert_eq!((h.points.len(), h.per_hair), (2 * 2 * 6, 6));
        assert!((h.points[0].position.x - 0.0).abs() < 1e-5);
        assert!((h.points[5].position.x - 2.0).abs() < 1e-5);
        assert!((h.points[0].normal.length() - 1.0).abs() < 1e-5);
        assert!(Hair::new("curve1\n0 0\nend\n", 1, 1, 0.0, 0.0).is_err());
        assert!(Hair::new("", 1, 1, 0.0, 0.0).is_err());
    }
}
