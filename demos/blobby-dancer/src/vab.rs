//! `.vab` animations (read by 0x415ca0 and 0x415af0): a tree of transforms whose
//! numbers follow key lists, with points (where blobs go) at the leaves.
use glam::{Mat4, Vec3, Vec4};

use crate::Error;

/// Keys of one channel in one clip: milliseconds and value.
pub type Curve = Vec<(f32, f32)>;

#[derive(Debug, Clone, Default)]
pub struct Node {
    /// 0 moves, 2 turns, 4 scales (0x410f10); other kinds leave the matrix alone.
    pub kind: u32,
    pub value: Vec3,
    /// For x, y and z: a curve for each clip.
    pub curves: [Vec<Curve>; 3],
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub extra: Vec<f32>,
    pub kids: Vec<Node>,
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> Result<u32, Error> {
        let b = self.d.get(self.at..self.at + 4).ok_or("vab: cut short")?;
        self.at += 4;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn vec3(&mut self) -> Result<Vec3, Error> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn count(&mut self, size: usize) -> Result<usize, Error> {
        let n = self.u32()? as usize;
        if n.saturating_mul(size) > self.d.len() - self.at {
            return Err("vab: count too large".into());
        }
        Ok(n)
    }

    /// 0x415af0: a node's value, then tagged parts until its end.
    fn node(&mut self, kind: u32, depth: usize) -> Result<Node, Error> {
        if depth > 200 {
            return Err("vab: too deep".into());
        }
        let mut n = Node {
            kind,
            value: self.vec3()?,
            ..Default::default()
        };
        loop {
            let tag = self.u32()?;
            match tag {
                0 | 2 | 4 => n.kids.push(self.node(tag, depth + 1)?),
                1 | 3 | 5 => return Ok(n),
                6 | 8 | 10 => {
                    let count = self.count(8)?;
                    let keys = (0..count)
                        .map(|_| Ok((self.f32()?, self.f32()?)))
                        .collect::<Result<Curve, Error>>()?;
                    n.curves[(tag as usize - 6) / 2].push(keys);
                    self.u32()?;
                }
                12 | 16 => {
                    let count = self.count(12)?;
                    let v = (0..count)
                        .map(|_| self.vec3())
                        .collect::<Result<Vec<_>, _>>()?;
                    if tag == 12 {
                        n.points = v;
                    } else {
                        n.normals = v;
                    }
                    self.u32()?;
                }
                14 => {
                    let count = self.count(4)?;
                    n.extra = (0..count).map(|_| self.f32()).collect::<Result<_, _>>()?;
                    self.u32()?;
                }
                // Ends of parts.
                7 | 9 | 11 | 13 | 15 => {}
                t => return Err(format!("vab: tag {t}").into()),
            }
        }
    }
}

/// 0x415ca0: the root's kind, then the root.
pub fn read(d: &[u8]) -> Result<Node, Error> {
    let mut r = Reader { d, at: 0 };
    let kind = r.u32()?;
    let root = r.node(kind, 0)?;
    if r.at != d.len() {
        return Err("vab: bytes left over".into());
    }
    Ok(root)
}

/// The value of a curve at a time, which repeats after its last key (0x40abf0).
pub fn sample(keys: &[(f32, f32)], t: f32) -> f32 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return 0.0;
    };
    let t = if last.0 > 0.0 { t % last.0 } else { t };
    if t <= first.0 {
        return first.1;
    }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t < b.0 {
            let f = (t - a.0) / (b.0 - a.0);
            return f * b.1 + (1.0 - f) * a.1;
        }
    }
    last.1
}

/// The turn 0x4225d0 makes from three angles, element by element, columns first.
pub fn turn(x: f32, y: f32, z: f32) -> Mat4 {
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    Mat4::from_cols(
        Vec4::new(cz * cy, -(cy * sz), sy, 0.0),
        Vec4::new(
            sz * cx + cz * sy * sx,
            cz * cx - sz * sy * sx,
            -(cy * sx),
            0.0,
        ),
        Vec4::new(sz * sx - cz * sy * cx, cz * sx + sz * sy * cx, cy * cx, 0.0),
        Vec4::W,
    )
}

impl Node {
    /// Number of clips: the most curves any channel has.
    pub fn clips(&self) -> usize {
        let own = self.curves.iter().map(Vec::len).max().unwrap_or(0);
        self.kids.iter().map(Node::clips).fold(own, usize::max)
    }

    /// The longest key time of a clip in this subtree.
    pub fn length(&self, clip: usize) -> f32 {
        let own = self
            .curves
            .iter()
            .filter_map(|c| c.get(clip)?.last().map(|k| k.0))
            .fold(0.0f32, f32::max);
        self.kids.iter().map(|k| k.length(clip)).fold(own, f32::max)
    }

    /// Where the points are at a time of a clip (0x410f10): each node's matrix after its
    /// parent's, its points through it, then its children.
    pub fn place(&self, clip: usize, t: f32, parent: Mat4, out: &mut Vec<Vec3>) {
        let mut v = self.value;
        for k in 0..3 {
            if let Some(keys) = self.curves[k].get(clip) {
                v[k] = sample(keys, t);
            }
        }
        let local = match self.kind {
            0 => Mat4::from_translation(v),
            2 => turn(v.x, v.y, v.z),
            4 => Mat4::from_scale(v),
            _ => Mat4::IDENTITY,
        };
        let world = parent * local;
        out.extend(self.points.iter().map(|p| world.transform_point3(*p)));
        for k in &self.kids {
            k.place(clip, t, world, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_hold_ends_and_repeat() {
        let k = vec![(0.0, 1.0), (10.0, 3.0), (20.0, 5.0)];
        assert_eq!(sample(&k, 5.0), 2.0);
        assert_eq!(sample(&k, 25.0), 2.0);
        assert_eq!(sample(&k, 0.0), 1.0);
    }

    #[test]
    fn turn_about_z_moves_x_to_y_as_the_program_does() {
        let m = turn(0.0, 0.0, std::f32::consts::FRAC_PI_2);
        let p = m.transform_point3(Vec3::X);
        // The program's matrix turns the other way from glam's.
        assert!((p - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-5, "{p}");
    }
}
