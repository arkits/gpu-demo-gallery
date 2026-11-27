//! The dancer's body: blobs that follow the animation on springs (0x41b2c0), their summed
//! field on a grid (0x419650, 0x419990) and the surface through it (0x41c310, 0x41ff40).
use glam::Vec3;

use crate::tables::{EDGES, TRIANGLES};

/// The grid spans -1 to 1 on every axis in this many cells (`VoxelDim`, 0x412f30).
pub const CELLS: usize = 100;
const LO: f32 = -1.0;
/// The program's 2π.
#[allow(clippy::approx_constant)]
const ROUND: f32 = 6.28;
const SIZE: f32 = 2.0 / CELLS as f32;
/// The field's table (0x41be90): e^(-0.012 i), shifted to reach zero at entry 577.
const TABLE: usize = 577;
/// Table entries for each squared distance over squared radius (0x461250).
const STEPS: f32 = 250.0;

/// Spring and forces (the object at app + 0x188, set from the registry at 0x4166bc and
/// by 0x41be90).
#[derive(Debug, Clone, Copy)]
pub struct Physics {
    /// `Friction`.
    pub damping: f32,
    pub stiffness: f32,
    /// `Inertia`.
    pub mass: f32,
    pub restitution: f32,
    pub gravity: f32,
    /// 0 up, 1 down, 2 +x, 3 -x, 4 +z, 5 -z.
    pub gravity_dir: u32,
    pub vortex: f32,
}

impl Default for Physics {
    fn default() -> Self {
        Self {
            damping: 0.199_92,
            stiffness: 3.0,
            mass: 0.238_64,
            restitution: -0.5,
            gravity: 0.0,
            gravity_dir: 1,
            vortex: 0.0,
        }
    }
}

pub struct Blobs {
    pub pos: Vec<Vec3>,
    vel: Vec<Vec3>,
    table: Vec<f32>,
    field: Vec<f32>,
    marked: Vec<u32>,
    stamp: u32,
    /// Phases of the slow flares (grid object + 0xe0, + 0xe4, + 0xe8) [I: random].
    pub phases: [f32; 3],
}

fn index(x: usize, y: usize, z: usize) -> usize {
    (x * (CELLS + 1) + y) * (CELLS + 1) + z
}

impl Blobs {
    pub fn new(phases: [f32; 3]) -> Self {
        let n = (CELLS + 1).pow(3);
        let end = (-0.012 * TABLE as f32).exp();
        let table = (0..=3 * STEPS as usize + 1)
            .map(|i| match i < TABLE {
                true => ((-0.012 * i as f32).exp() - end) / (1.0 - end),
                false => 0.0,
            })
            .collect();
        Self {
            pos: Vec::new(),
            vel: Vec::new(),
            table,
            field: vec![0.0; n],
            marked: vec![0; CELLS.pow(3)],
            stamp: 0,
            phases,
        }
    }

    /// Blobs start where the animation puts them.
    pub fn reset(&mut self, targets: &[Vec3]) {
        self.pos = targets.to_vec();
        self.vel = vec![Vec3::ZERO; targets.len()];
    }

    /// One frame of the springs (0x41b2c0): `time` in seconds, `distort` the extra wobble.
    pub fn step(&mut self, targets: &[Vec3], p: &Physics, dt: f32, time: f32, distort: f32) {
        if self.pos.len() != targets.len() {
            self.reset(targets);
        }
        // Now and then the blobs grow sluggish, and now and then they wobble.
        let flare = |speed: f32, phase: f32, scale: f32, sub: f32, extra: f32| {
            ((time * speed + phase * ROUND + extra).cos() * scale - sub).clamp(0.0, 1.0)
        };
        let f = flare(0.101, self.phases[1], 14.4, 13.0, 0.0);
        let mass = f * 0.2 + (1.0 - f) * p.mass;
        let distort = distort.max(flare(0.047, self.phases[0], 15.5, 14.0, 0.8));
        let wobble = time * 0.7 - 0.3 + self.phases[2] * ROUND;
        let s = p.gravity;
        let g = match p.gravity_dir {
            0 => Vec3::new(0.0, s, 0.0),
            1 => Vec3::new(0.0, -s, 0.0),
            2 => Vec3::new(s, 0.0, 0.0),
            3 => Vec3::new(-s, 0.0, 0.0),
            4 => Vec3::new(0.0, 0.0, s),
            _ => Vec3::new(0.0, 0.0, -s),
        };
        let vortex = p.vortex
            * if mass < 0.1 {
                (10.090_909 - mass * 90.909_09) * 0.1
            } else {
                1.0
            };
        let wall = 0.99;
        for ((x, v), t) in self.pos.iter_mut().zip(&mut self.vel).zip(targets) {
            *v = v.clamp(Vec3::splat(-10.0), Vec3::splat(10.0));
            let force = Vec3::new(g.x - x.z * vortex, g.y, g.z + x.x * vortex) / mass;
            // m x'' + c x' + k (x - target) = m force, which 0x41a480 solves exactly; here
            // in small steps.
            let steps = 4;
            let h = dt / steps as f32;
            for _ in 0..steps {
                let a = |x: Vec3, v: Vec3| (p.stiffness * (*t - x) - p.damping * v) / mass + force;
                let (k1x, k1v) = (*v, a(*x, *v));
                let (k2x, k2v) = (
                    *v + k1v * h / 2.0,
                    a(*x + k1x * h / 2.0, *v + k1v * h / 2.0),
                );
                let (k3x, k3v) = (
                    *v + k2v * h / 2.0,
                    a(*x + k2x * h / 2.0, *v + k2v * h / 2.0),
                );
                let (k4x, k4v) = (*v + k3v * h, a(*x + k3x * h, *v + k3v * h));
                *x += (k1x + 2.0 * k2x + 2.0 * k3x + k4x) * h / 6.0;
                *v += (k1v + 2.0 * k2v + 2.0 * k3v + k4v) * h / 6.0;
            }
            *v = v.clamp(Vec3::splat(-10.0), Vec3::splat(10.0));
            if distort > 0.0 {
                let o = *x;
                x.x += (o.x * 17.0 + o.y * 11.0 - o.z * 3.0 + wobble).sin() * distort * 0.01;
                x.y += (o.y * 18.0 + o.z * 25.0 - o.x * 27.0 + wobble).sin() * distort * 0.01;
                x.z += (o.z * 5.0 + (o.x * 7.0 - o.y * 21.0) + wobble).sin() * distort * 0.01;
            }
            // Walls of the box bounce them back (0x4180e0).
            for k in 0..3 {
                if x[k] < -wall {
                    x[k] = -wall + (-wall - x[k]) * -p.restitution;
                    v[k] = (v[k] * p.restitution).max(0.01);
                }
                if x[k] > wall {
                    x[k] = wall - (x[k] - wall) * -p.restitution;
                    v[k] = (v[k] * p.restitution).min(-0.01);
                }
            }
        }
    }

    fn range(&self, c: f32, r: f32) -> (usize, usize) {
        let lo = ((c - r - LO) / SIZE).round().clamp(0.0, CELLS as f32) as usize;
        let hi = ((c + r - LO) / SIZE).round().clamp(0.0, CELLS as f32) as usize;
        (lo, hi)
    }

    /// The field (0x419650 clears, 0x419990 adds) and its surface at `threshold`:
    /// positions and normals, three to a triangle.
    pub fn surface(&mut self, radius: f32, threshold: f32) -> Vec<[Vec3; 2]> {
        let blobs: Vec<Vec3> = self.pos.clone();
        let boxes: Vec<[(usize, usize); 3]> = blobs
            .iter()
            .map(|b| [0, 1, 2].map(|k| self.range(b[k], radius)))
            .collect();
        for bx in &boxes {
            for x in bx[0].0..=bx[0].1 {
                for y in bx[1].0..=bx[1].1 {
                    let row = index(x, y, 0);
                    self.field[row + bx[2].0..=row + bx[2].1].fill(0.0);
                }
            }
        }
        let inv = 1.0 / (radius * radius);
        for (b, bx) in blobs.iter().zip(&boxes) {
            for x in bx[0].0..=bx[0].1 {
                let dx = LO + x as f32 * SIZE - b.x;
                for y in bx[1].0..=bx[1].1 {
                    let dy = LO + y as f32 * SIZE - b.y;
                    let row = index(x, y, 0);
                    for z in bx[2].0..=bx[2].1 {
                        let dz = LO + z as f32 * SIZE - b.z;
                        let q = (dx * dx + dy * dy + dz * dz) * inv;
                        let i = (q * STEPS).round() as usize;
                        self.field[row + z] += self.table.get(i).copied().unwrap_or(0.0);
                    }
                }
            }
        }
        self.stamp = self.stamp.wrapping_add(1).max(1);
        let mut out = Vec::new();
        for bx in &boxes {
            for x in bx[0].0..bx[0].1.min(CELLS) {
                for y in bx[1].0..bx[1].1.min(CELLS) {
                    for z in bx[2].0..bx[2].1.min(CELLS) {
                        let m = &mut self.marked[(x * CELLS + y) * CELLS + z];
                        if *m != self.stamp {
                            *m = self.stamp;
                            self.cell(x, y, z, threshold, &mut out);
                        }
                    }
                }
            }
        }
        out
    }

    /// The field's slope at a point of the grid, outward (0x418020).
    fn normal(&self, x: usize, y: usize, z: usize) -> Vec3 {
        let f = |x: usize, y: usize, z: usize| self.field[index(x, y, z)];
        let c = |a: usize, d: isize| (a as isize + d).clamp(0, CELLS as isize) as usize;
        Vec3::new(
            f(c(x, -1), y, z) - f(c(x, 1), y, z),
            f(x, c(y, -1), z) - f(x, c(y, 1), z),
            f(x, y, c(z, -1)) - f(x, y, c(z, 1)),
        )
    }

    /// One cube of marching cubes (0x41ff40, 0x41fd40); triangles come out reversed.
    fn cell(&self, x: usize, y: usize, z: usize, threshold: f32, out: &mut Vec<[Vec3; 2]>) {
        const CORNERS: [[usize; 3]; 8] = [
            [0, 0, 0],
            [1, 0, 0],
            [1, 1, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 0, 1],
            [1, 1, 1],
            [0, 1, 1],
        ];
        const EDGE: [[usize; 2]; 12] = [
            [0, 1],
            [1, 2],
            [2, 3],
            [3, 0],
            [4, 5],
            [5, 6],
            [6, 7],
            [7, 4],
            [0, 4],
            [1, 5],
            [2, 6],
            [3, 7],
        ];
        let at = |c: usize| [x + CORNERS[c][0], y + CORNERS[c][1], z + CORNERS[c][2]];
        let value = |c: usize| {
            let p = at(c);
            self.field[index(p[0], p[1], p[2])]
        };
        let mut case = 0;
        let values: [f32; 8] = std::array::from_fn(value);
        for (c, v) in values.iter().enumerate() {
            if *v < threshold {
                case |= 1 << c;
            }
        }
        if EDGES[case] == 0 {
            return;
        }
        let mut cross = [[Vec3::ZERO; 2]; 12];
        for (e, ends) in EDGE.iter().enumerate() {
            if EDGES[case] & (1 << e) == 0 {
                continue;
            }
            let (a, b) = (ends[0], ends[1]);
            let f = (threshold - values[a]) / (values[b] - values[a]);
            let point = |c: usize| {
                let p = at(c);
                Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) * SIZE + Vec3::splat(LO)
            };
            let normal = |c: usize| {
                let p = at(c);
                self.normal(p[0], p[1], p[2])
            };
            cross[e] = [
                point(a) + (point(b) - point(a)) * f,
                normal(a) + (normal(b) - normal(a)) * f,
            ];
        }
        for t in TRIANGLES[case].chunks(3) {
            if t[0] < 0 {
                break;
            }
            out.push(cross[t[2] as usize]);
            out.push(cross[t[1] as usize]);
            out.push(cross[t[0] as usize]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_blob_makes_a_small_closed_surface() {
        let mut b = Blobs::new([0.0; 3]);
        b.reset(&[Vec3::ZERO]);
        let tris = b.surface(0.3, 0.8);
        assert!(!tris.is_empty() && tris.len().is_multiple_of(3));
        // The surface is where the falloff is 0.8: about 0.27 radii out.
        for [p, n] in &tris {
            assert!((p.length() - 0.3 * 0.272).abs() < 0.03, "{p}");
            assert!(n.dot(*p) > 0.0, "normals point out");
        }
    }
}
