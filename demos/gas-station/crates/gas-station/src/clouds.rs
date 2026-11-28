//! The clouds (`NVClouds`: constructor `0x40a830`, update and draw `0x40d490`, called after
//! `CloudsShape` draws, with its programs and states still bound).
//!
//! Fifty billboards over a 400 × 50 × 400 box 100 units up, each fading in, living a while
//! of the day's clock, fading out and starting again elsewhere, with one of four cells of
//! `clouds.dds`. The layer turns about y with the wind. Placement uses the C library's
//! `rand()` seeded with the time, as the original does, so each run's sky differs.

use nv_engine::gl::MeshArray;
use nv_engine::math::{add, cross, dot, length, scale, sub, Vec3};
use nv_engine::sort::sort;
use nv_engine::viewer::{crt_rand, Viewer};
use nv_engine::world::TransformKind;

pub const COUNT: usize = 50;

/// Atlas cells: per cell, the corners' (u, v) (`0x5c2e50`).
const CELLS: [[[f32; 2]; 4]; 4] = [
    [[0.5, 0.0], [0.0, 0.0], [0.0, 0.5], [0.5, 0.5]],
    [[0.5, 0.5], [0.0, 0.5], [0.0, 1.0], [0.5, 1.0]],
    [[1.0, 0.0], [0.5, 0.0], [0.5, 0.5], [1.0, 0.5]],
    [[1.0, 0.5], [0.5, 0.5], [0.5, 1.0], [1.0, 1.0]],
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Fading in (0).
    In,
    /// Fading out (1).
    Out,
    /// Alive (2).
    Alive,
    /// To start again (3).
    Again,
}

pub struct Clouds {
    pos: [Vec3; COUNT],
    alpha: [f32; COUNT],
    phase: [Phase; COUNT],
    life: [f32; COUNT],
    cell: [usize; COUNT],
    order: [usize; COUNT],
    fade: f32,
    extent: Vec3,
    /// Billboard half-size (`+0x688`), the clouds slider's 50–150.
    pub size: f32,
    wind: f32,
    life_range: (f32, f32),
    /// The layer's turn about y (`0x5c08a4`).
    angle: f32,
    /// Day clock this frame and last (`+0x660`, `+0x668`).
    pub time: f64,
    pub prev: f64,
    /// Inputs from the day (`+0x670`, `+0x674`, `+0x68c`).
    pub light: f32,
    pub sun: Vec3,
    pub day: f32,
    pub rand: u32,
    pub shape: Option<usize>,
    transform: Option<usize>,
}

fn rand01(r: &mut u32) -> f32 {
    crt_rand(r) as f32 * f32::from_bits(0x38000100)
}

impl Clouds {
    /// `0x40a830`: `srand(time(NULL))`, then every cloud alive at full strength.
    pub fn new(seed: u32) -> Clouds {
        let mut c = Clouds {
            pos: [[0.0; 3]; COUNT],
            alpha: [1.0; COUNT],
            phase: [Phase::Alive; COUNT],
            life: [0.0; COUNT],
            cell: [0; COUNT],
            order: std::array::from_fn(|i| i),
            fade: 250.0,
            extent: [400.0, 50.0, 400.0],
            size: 57.0,
            wind: 3.0,
            life_range: (0.015, 0.04),
            angle: 0.0,
            time: 0.0,
            prev: 0.0,
            light: 0.0,
            sun: [0.0; 3],
            day: 0.0,
            rand: seed,
            shape: None,
            transform: None,
        };
        for i in 0..COUNT {
            c.place(i);
            let r = crt_rand(&mut c.rand) as f32;
            c.life[i] = (c.life_range.1 - c.life_range.0) * r * f32::from_bits(0x38000100) + c.life_range.0;
            c.cell[i] = (crt_rand(&mut c.rand) & 3) as usize;
        }
        c
    }

    /// A random place: three `rand()`s, the third giving x and the first z.
    fn place(&mut self, i: usize) {
        let r1 = rand01(&mut self.rand);
        let r2 = rand01(&mut self.rand);
        let r3 = rand01(&mut self.rand);
        let margin = self.extent[0] * 0.2;
        self.pos[i] = [
            (r3 + r3 - 1.0) * (self.extent[0] - margin),
            (r2 + r2 - 1.0) * self.extent[1] + 100.0,
            (r1 + r1 - 1.0) * (self.extent[2] - margin),
        ];
    }

    pub fn attach(&mut self, v: &Viewer) {
        self.shape = v.world.shapes.iter().position(|s| s.name == "CloudsShape");
        self.transform = v.world.transform("pClouds1");
    }

    /// `0x40d490`.
    pub fn update_and_draw(&mut self, v: &mut Viewer) -> Result<(), Box<dyn std::error::Error>> {
        let Some(t) = self.transform else { return Ok(()) };
        let (center, pivot) = {
            let tr = &v.world.transforms[t];
            let m = tr.world;
            let rp = match &tr.kind {
                TransformKind::Maya { rotation_pivot, .. } => *rotation_pivot,
                _ => [0.0; 3],
            };
            ([m.at(0, 3) + rp[0], m.at(1, 3) + rp[1], m.at(2, 3) + rp[2]], rp)
        };
        let _ = pivot;
        let dt = ((self.time - self.prev) * 0.001).min(0.033);
        let mut a = self.angle as f64 - self.wind as f64 * dt;
        let two_pi = std::f64::consts::PI + std::f64::consts::PI;
        if a >= two_pi {
            a -= two_pi;
        }
        self.angle = a as f32;
        if let TransformKind::Maya { rotation, .. } = &mut v.world.transforms[t].kind {
            *rotation = [0.0, self.angle, 0.0];
        }
        let dtf = dt as f32;
        for i in 0..COUNT {
            self.order[i] = i;
            self.life[i] -= dtf;
            if self.phase[i] == Phase::Alive && self.life[i] <= 0.0 {
                self.phase[i] = Phase::Out;
            }
            if self.phase[i] == Phase::Out {
                self.alpha[i] = (self.alpha[i] - dtf * self.fade).max(0.0).min(1.0);
                if self.alpha[i] * 0.3 == 0.0 {
                    self.phase[i] = Phase::Again;
                }
            }
            if self.phase[i] == Phase::Again {
                self.place(i);
                let r = crt_rand(&mut self.rand) as f32;
                self.life[i] = (self.life_range.1 - self.life_range.0) * r * f32::from_bits(0x38000100) + self.life_range.0;
                self.phase[i] = Phase::In;
                self.alpha[i] = 0.0;
                self.cell[i] = (crt_rand(&mut self.rand) & 3) as usize;
            }
            if self.phase[i] == Phase::In {
                self.alpha[i] += dtf * self.fade;
                if self.alpha[i] >= 1.0 {
                    self.alpha[i] = 1.0;
                    self.phase[i] = Phase::Alive;
                }
            }
        }
        // Nearest along the view first (0x40d460, 0x40aa50).
        let cam = v.camera.map(|c| v.world.cameras[c].dir).unwrap_or([0.0, 0.0, -1.0]);
        let pos = self.pos;
        sort(&mut self.order, |a, b| dot(cam, pos[*a]) < dot(cam, pos[*b]));
        // Quads: corners P - R - U, P + R - U, P + R + U, P - R + U.
        let mut p4 = Vec::with_capacity(COUNT * 16);
        let mut data = Vec::with_capacity(COUNT * 16);
        let mut uv = Vec::with_capacity(COUNT * 16);
        for &i in &self.order {
            let p = add(center, self.pos[i]);
            let n = scale(p, 1.0 / length(p));
            let flat = (self.pos[i][0] * self.pos[i][0] + self.pos[i][2] * self.pos[i][2]).sqrt() <= 150.0;
            let (r, u) = if flat {
                ([1.0, 0.0, 0.0], scale([0.0, 0.0, 1.0], self.size))
            } else {
                let c = cross([0.0, 1.0, 0.0], n);
                (scale(c, 1.0 / length(c)), scale([0.0, 1.0, 0.0], self.size))
            };
            let rs = scale(r, self.size);
            let shade = 1.0 - (dot(n, self.sun) + 1.0) * 0.5;
            let corners = [sub(sub(p, rs), u), sub(add(p, rs), u), add(add(p, rs), u), add(sub(p, rs), u)];
            for (k, c) in corners.iter().enumerate() {
                p4.extend_from_slice(&[c[0], c[1], c[2], 1.0]);
                data.extend_from_slice(&[self.light, shade, self.alpha[i] * 0.4, self.day]);
                let t = CELLS[self.cell[i]][k];
                uv.extend_from_slice(&[t[0], t[1], 0.0, 0.0]);
            }
        }
        // GL_QUADS, split as (0, 1, 2), (0, 2, 3).
        let mut idx = Vec::with_capacity(COUNT * 6);
        for q in 0..COUNT as u16 {
            let b = q * 4;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let mesh = v.gl.mesh(
            &[
                ("a2v.objCoord".into(), MeshArray::F32(p4, 4)),
                ("a2v.textoto".into(), MeshArray::F32(data, 4)),
                ("a2v.texCol".into(), MeshArray::F32(uv, 4)),
            ],
            &[(3, idx)],
            false,
            false,
        );
        v.draw_current(&mesh)
    }
}
