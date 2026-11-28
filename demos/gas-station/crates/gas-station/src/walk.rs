//! Walking (`NVWalkNavigator`, with Gas Station's collision update `0x402e30`).
//!
//! The camera stands 3.2 above the ground (there is no height field, so the ground is 0)
//! and turns by yaw and pitch: orientation = yaw about y · pitch about x (`0x42e160`).
//! Arrow keys move it at 20 units a second, easing in and out over a fifth of a second
//! (`0x42e0b0`, `0x42e040`). Dragging with the left button walks by the vertical offset from
//! where the drag began and turns by the horizontal one; moving with the right button held
//! looks around (`0x42e2c0`), pitch kept within ±1.5698. Nothing moves while the last mouse
//! event went to a menu. `textures/collision1.tga` keeps the walk off
//! walls (`0x402bf0`): (255, 127, 127) is open ground, black is solid, other colours are wall
//! normals off which a step slides.

use nv_engine::math::Vec3;
use nv_engine::viewer::Viewer;

/// The collision maps' extents in x and z (`0x5ba300`).
const X0: f32 = -32.416;
const Z0: f32 = 27.647;
const X1: f32 = 22.913;
const Z1: f32 = -16.906;
const PI_F: f32 = 3.14159274;
const PITCH: f32 = 1.56979632;

pub struct Map {
    width: u32,
    height: u32,
    /// TGA bytes in file order (blue, green, red).
    bgr: Vec<u8>,
}

impl Map {
    /// `collision1.tga`: 24-bit, rows from the bottom.
    pub fn read(d: &[u8]) -> Option<Map> {
        if d.len() < 18 || d[2] != 2 || d[16] != 24 {
            return None;
        }
        let width = u16::from_le_bytes([d[12], d[13]]) as u32;
        let height = u16::from_le_bytes([d[14], d[15]]) as u32;
        let at = 18 + d[0] as usize;
        let bgr = d.get(at..at + (width * height * 3) as usize)?.to_vec();
        Some(Map { width, height, bgr })
    }

    /// World x, z to a pixel (`0x401730`).
    fn pixel(&self, p: Vec3) -> (u32, u32) {
        let dx = (X1 - X0).abs();
        let dz = (Z1 - Z0).abs();
        let px = ((p[0] - X0) / dx * self.width as f32) as i32;
        let pz = (-((p[2] - Z0) / dz) * self.height as f32) as i32;
        (px.clamp(0, self.width as i32 - 1) as u32, pz.clamp(0, self.height as i32 - 1) as u32)
    }

    fn at(&self, px: u32, pz: u32) -> [u8; 3] {
        let i = ((pz * self.width + px) * 3) as usize;
        [self.bgr[i], self.bgr[i + 1], self.bgr[i + 2]]
    }

    /// Whether a step from `cur` to `new` may be taken; `new` may slide (`0x402bf0`).
    fn allow(&self, cur: Vec3, new: &mut Vec3) -> bool {
        let (px, pz) = self.pixel(*new);
        if px == 0 || px == self.width - 1 || pz == 0 || pz == self.height - 1 {
            // At the edge, the pixel's place (0x401110).
            new[0] = ((X1 - X0) * (px as f32 / self.width as f32) + X0).clamp(X0, X1);
            new[2] = ((Z1 - Z0) * (pz as f32 / self.height as f32) + Z0).clamp(Z1, Z0);
            return true;
        }
        let [b, g, r] = self.at(px, pz);
        if b == 0xff && g == 0x7f && r == 0x7f {
            return true;
        }
        if b == 0 && g == 0 && r == 0 {
            return false;
        }
        let n = [(r as u32 * 2) as f32 * (1.0 / 255.0) - 1.0, (g as u32 * 2) as f32 * (1.0 / 255.0) - 1.0];
        let l = (n[0] * n[0] + n[1] * n[1]).sqrt();
        let n = [-n[0] / l, -n[1] / l];
        let m = [new[0] - cur[0], new[2] - cur[2]];
        if m[0] == 0.0 && m[1] == 0.0 {
            return false;
        }
        let len = (m[0] * m[0] + m[1] * m[1]).sqrt();
        let d = [m[0] / len, m[1] / len];
        let k = (n[0] * d[0] + n[1] * d[1]) * 2.0;
        let s = [d[0] - n[0] * k, d[1] - n[1] * k];
        let c = [s[0] * len + new[0], new[1], s[1] * len + new[2]];
        let (cx, cz) = self.pixel(c);
        let [b, g, r] = self.at(cx, cz);
        if b == 0xff && g == 0x7f && r == 0x7f {
            new[0] = c[0];
            new[2] = c[2];
        }
        true
    }
}

/// Arrow keys: up, down, left, right.
#[derive(Clone, Copy, Default)]
struct Key {
    down: bool,
    since: f64,
}

#[derive(Default)]
pub struct Mouse {
    pub left: bool,
    pub middle: bool,
    pub right: bool,
    /// The last press or move went to a menu (`+0xc`): no walking until the next one.
    pub handled: bool,
    /// Where the left drag began, and the pointer now and before, as window fractions.
    pub press: [f32; 2],
    pub now: [f32; 2],
    pub prev: [f32; 2],
}

pub struct Walk {
    pub camera: Option<usize>,
    eye: f32,
    speed: f32,
    pub yaw: f32,
    pub pitch: f32,
    last: f64,
    keys: [Key; 4],
    pub mouse: Mouse,
    pub map: Option<Map>,
}

/// Quaternion (w, x, y, z) of an angle about an axis (`0x403020`).
fn axis_angle(a: f32, axis: Vec3) -> [f32; 4] {
    let h = 0.5 * a;
    let s = h.sin();
    [h.cos(), s * axis[0], s * axis[1], s * axis[2]]
}

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

/// q · v · q* (`0x403110`).
fn rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let c = [q[0], -q[1], -q[2], -q[3]];
    let r = qmul(qmul(q, [0.0, v[0], v[1], v[2]]), c);
    [r[1], r[2], r[3]]
}

/// A key's ease (`0x42e040`): 0 to 1 over 0.2 s after a press, back after a release.
fn ease(down: bool, dt: f32) -> f32 {
    let x = (dt * 5.0).clamp(0.0, 1.0);
    if down { x } else { 1.0 - x }
}

impl Walk {
    pub fn new() -> Walk {
        Walk {
            camera: None,
            eye: 3.2,
            speed: 20.0,
            yaw: 0.0,
            pitch: 0.0,
            last: 0.0,
            keys: [Key::default(); 4],
            mouse: Mouse::default(),
            map: None,
        }
    }

    /// The camera's orientation (`0x42e160`).
    fn orient(&self, v: &mut Viewer) {
        if let Some(c) = self.camera {
            let q = qmul(axis_angle(self.yaw, [0.0, 1.0, 0.0]), axis_angle(self.pitch, [1.0, 0.0, 0.0]));
            v.world.cameras[c].orientation = q;
        }
    }

    /// Takes the camera (`0x42e210`, `0x42e2b0` with yaw -4): eye height over the ground.
    pub fn attach(&mut self, v: &mut Viewer, camera: Option<usize>, now: f64) {
        self.camera = camera;
        self.yaw = -4.0;
        if let Some(c) = camera {
            let p = v.world.cameras[c].position;
            v.world.cameras[c].position = [p[0], self.eye, p[2]];
        }
        self.orient(v);
        self.last = now;
        for k in &mut self.keys {
            k.since = now;
        }
    }

    pub fn key(&mut self, which: usize, down: bool, now: f64) {
        self.keys[which] = Key { down, since: now };
    }

    /// The right button's look (`0x42e2c0`), on each move of the pointer.
    pub fn look(&mut self, v: &mut Viewer) {
        if !self.mouse.right {
            return;
        }
        let m = &self.mouse;
        self.yaw -= (m.now[0] - m.prev[0]) * PI_F;
        self.pitch = (self.pitch - (m.now[1] - m.prev[1]) * PI_F).clamp(-PITCH, PITCH);
        self.orient(v);
    }

    /// One step at clock time `now` (`0x402e30`).
    pub fn update(&mut self, v: &mut Viewer, now: f64) {
        let dt = (now - self.last) as f32;
        let k = |i: usize| ease(self.keys[i].down, (now - self.keys[i].since) as f32);
        // Up 0, down 1, left 2, right 3.
        let mut mv = [(k(3) - k(2)) * 0.5, 0.0, (k(1) - k(0)) * 0.5];
        let mut go = !self.mouse.handled;
        if self.mouse.handled {
            // A menu took the mouse.
        } else if self.mouse.left {
            let dx = self.mouse.now[0] - self.mouse.press[0];
            let dy = self.mouse.now[1] - self.mouse.press[1];
            self.yaw -= dx * dt * PI_F;
            self.orient(v);
            mv[2] += dy;
        } else if mv[0] * mv[0] + mv[1] * mv[1] + mv[2] * mv[2] <= 0.0 {
            go = false;
        }
        if go {
            if let Some(c) = self.camera {
                let s = dt * self.speed;
                let step = rotate(axis_angle(self.yaw, [0.0, 1.0, 0.0]), [mv[0] * s, mv[1] * s, mv[2] * s]);
                let cur = v.world.cameras[c].position;
                let mut new = [cur[0] + step[0], self.eye, cur[2] + step[2]];
                let ok = match &self.map {
                    Some(m) => m.allow(cur, &mut new),
                    None => true,
                };
                if ok {
                    v.world.cameras[c].position = new;
                }
            }
        }
        self.last = now;
    }
}

impl Default for Walk {
    fn default() -> Self {
        Self::new()
    }
}
