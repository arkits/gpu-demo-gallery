//! Water pools (a room's 0x84 records; simulated by 0x425430 and 0x425750, drawn by 0x425ee0):
//! a grid of heights pulled towards their eight neighbours, its normals by a fixed stencil,
//! and for each vertex the eye's ray bent into the water (index 0.75) and followed to the
//! pool's floor or walls, which an unfolded box picture shows; the surface reflects the
//! environment by a weight that falls as the eye looks straight down.
use glam::Vec3;

#[derive(Debug, Clone)]
pub struct Water {
    pub nx: usize,
    pub ny: usize,
    /// Depth (+0x58) and width (+0x5c) of the pool, in the grid's own measure.
    pub depth: f32,
    pub width: f32,
    /// The environment's texture (+0x60) and the pool's (+0x64).
    pub reflection: i32,
    pub pool: i32,
    pub indices: Vec<u16>,
    /// Place, normal, colour byte and texture coordinates per vertex.
    pub pos: Vec<Vec3>,
    pub normal: Vec<Vec3>,
    pub shade: Vec<u8>,
    pub uv: Vec<[f32; 2]>,
    accel: Vec<f32>,
    vel: Vec<f32>,
    start: Vec<Vec3>,
}

/// Each spring's pull per step (0x4392fc) and the diagonal springs' weight (0x439300).
const STEP: f32 = 0.03;
const DIAGONAL: f32 = 4.949_747_6;

impl Water {
    /// From the record and its two arrays; the loader lifts two vertices to start the
    /// ripples (0x4125f0): one to the first vertex's height plus 2, one plus 2.4.
    pub fn new(rec: &[u8], indices: &[u8], vertices: &[u8]) -> Option<Self> {
        let i = |o: usize| crate::ati::i32_at(rec, o);
        let f = |o: usize| crate::ati::f32_at(rec, o);
        let (nx, ny) = (i(0x50).max(0) as usize, i(0x54).max(0) as usize);
        let n = vertices.len() / 0x24;
        if nx < 2 || ny < 2 || n != nx * ny {
            return None;
        }
        let vf = |k: usize, o: usize| crate::ati::f32_at(vertices, k * 0x24 + o);
        let mut pos: Vec<Vec3> = (0..n)
            .map(|k| Vec3::new(vf(k, 0), vf(k, 4), vf(k, 8)))
            .collect();
        let z0 = pos[0].z;
        pos[nx + 1].z = z0 + 2.0;
        pos[(ny - 1) * nx - 2].z = z0 + 2.4;
        let start = pos.clone();
        Some(Self {
            start,
            nx,
            ny,
            depth: f(0x58),
            width: f(0x5c),
            reflection: i(0x60),
            pool: i(0x64),
            indices: indices
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect(),
            normal: vec![Vec3::Z; n],
            shade: vec![0; n],
            uv: vec![[0.0; 2]; n],
            accel: vec![0.0; n],
            vel: vec![0.0; n],
            pos,
        })
    }

    /// Back to the loaded state.
    pub fn reset(&mut self) {
        self.pos.clone_from(&self.start);
        self.vel.iter_mut().for_each(|v| *v = 0.0);
    }

    /// The lowest corner's height: drawn only from above it (0x425750).
    pub fn level(&self) -> f32 {
        self.pos[0].z
    }

    fn h(&self, r: usize, c: usize) -> f32 {
        self.pos[r * self.nx + c].z
    }

    /// One step of the springs (0x425430).
    pub fn step(&mut self) {
        let nx = self.nx;
        self.accel.iter_mut().for_each(|a| *a = 0.0);
        for r in 2..self.ny.saturating_sub(2) {
            for c in 2..nx.saturating_sub(2) {
                let v = r * nx + c;
                let h = self.pos[v].z;
                let pull = |o: usize, w: f32, this: &mut Self| {
                    let d = (h - this.pos[o].z) * w;
                    this.accel[v] -= d;
                    this.accel[o] += d;
                };
                pull(v - 1, 1.0, self);
                pull(v - nx, 1.0, self);
                pull(v + 1, 1.0, self);
                pull(v + nx, 1.0, self);
                pull(v + nx + 1, DIAGONAL, self);
                pull(v - nx - 1, DIAGONAL, self);
                pull(v + nx - 1, DIAGONAL, self);
                pull(v - nx + 1, DIAGONAL, self);
            }
        }
        for k in 0..self.pos.len() {
            self.vel[k] += self.accel[k] * STEP;
            self.pos[k].z += self.vel[k];
        }
    }

    /// Normals, shades and texture coordinates for an eye (0x425750).
    pub fn shade_for(&mut self, eye: Vec3) {
        let (nx, ny) = (self.nx, self.ny);
        for r in 0..ny {
            for c in 0..nx {
                let h = self.h(r, c);
                let (mut a, mut b);
                if r == 0 {
                    a = -h - (h + h) - h;
                    b = 0.0;
                } else {
                    let ul = if c == 0 {
                        self.h(r - 1, c)
                    } else {
                        self.h(r - 1, c - 1)
                    };
                    a = -ul;
                    b = -ul;
                    a -= 2.0 * self.h(r - 1, c);
                    let ur = if c == nx - 1 {
                        self.h(r - 1, c)
                    } else {
                        self.h(r - 1, c + 1)
                    };
                    a -= ur;
                    b += ur;
                }
                let left = if c == 0 { h } else { self.h(r, c - 1) };
                b -= 2.0 * left;
                let right = if c == nx - 1 { h } else { self.h(r, c + 1) };
                b += 2.0 * right;
                if r == ny - 1 {
                    a += h * 4.0;
                } else {
                    let dl = if c == 0 {
                        self.h(r + 1, c)
                    } else {
                        self.h(r + 1, c - 1)
                    };
                    a += dl;
                    b -= dl;
                    a += 2.0 * self.h(r + 1, c);
                    let dr = if c == nx - 1 {
                        self.h(r + 1, c)
                    } else {
                        self.h(r + 1, c + 1)
                    };
                    a += dr;
                    b += dr;
                }
                let f = 1.0 / (a * a + b * b + 1.0).sqrt();
                self.normal[r * nx + c] = Vec3::new(a * f, b * f, f);
            }
        }
        let fx = 1.0 / (nx as f32 - 1.0);
        let fy = 1.0 / (ny as f32 - 1.0);
        for r in 0..ny {
            let y = r as f32 * fy * 0.998 + 0.001;
            for c in 0..nx {
                let k = r * nx + c;
                let n = self.normal[k];
                let v = (eye - self.pos[k]).normalize_or_zero();
                let cos = v.dot(n);
                self.shade[k] = ((1.0 - (cos * 0.8 + 0.1)) * 255.0).clamp(0.0, 85.0) as u8;
                let t = cos * 0.75 - (1.0 - (1.0 - cos * cos) * 0.5625).max(0.0).sqrt();
                let ray = (n * t - v * 0.75).normalize_or_zero();
                let x = (c as f32 * fx * 0.998 + 0.001) * self.width;
                let hit = |d: f32, along: f32| {
                    if along == 0.0 || d / -along < 0.0 {
                        1e9
                    } else {
                        d / -along
                    }
                };
                let ts = [
                    hit(x, ray.x),
                    hit(x - self.width, ray.x),
                    hit(y, ray.y),
                    hit(y - 1.0, ray.y),
                    hit(self.depth, ray.z),
                ];
                let mut best = usize::from(ts[1] < ts[0]);
                for (j, &tj) in ts.iter().enumerate().skip(2) {
                    if tj < ts[best] {
                        best = j;
                    }
                }
                let wall = |t: f32| -0.25 / self.depth * t * ray.z;
                self.uv[k] = match best {
                    0 | 1 => {
                        let s = wall(ts[best]);
                        let v = (1.0 - 2.0 * s) * (ts[best] * ray.y + y) + s;
                        [if best == 0 { s } else { 1.0 - s }, v]
                    }
                    2 | 3 => {
                        let s = wall(ts[best]);
                        let u = (ts[best] * ray.x + x) / self.width * (1.0 - 2.0 * s) + s;
                        [u, if best == 2 { s } else { 1.0 - s }]
                    }
                    _ => [
                        (ts[4] * ray.x + x) / self.width * 0.5 + 0.25,
                        (ts[4] * ray.y + y) * 0.5 + 0.25,
                    ],
                };
            }
        }
    }
}
