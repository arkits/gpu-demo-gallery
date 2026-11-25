//! 60Hz / 30Hz / 15Hz: the same scene, updated at different rates, side by side.
//!
//! The original (`.rea3dfx/d6030`) draws a room from nine saved camera positions
//! (`init1.dat`..`init9.dat`, a 0x1e8-byte state block), a bouncing ball, and a
//! spinning scene. Its point is to show motion at 60 fps against a screen that
//! only updates every 2nd or 4th frame. Its `.daf` models are raw memory dumps
//! with baked pointers, so the room here is reconstructed from the demo's own
//! textures rather than extracted from those dumps; see `docs/ENGINE.md`.
use std::f32::consts::PI;

use glam::{Mat4, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{
    Depth, Gl, Gpu, REPEAT, State, Surface, TextureId, Vertex, WHITE, from_opengl, look_at,
    perspective,
};

/// Lower-case base names: `data.rs` matches files case-insensitively but only
/// keeps those listed here. All of these exist in the staged `data/`.
pub const FILES: &[&str] = &[
    "scene.daf",
    "animmodel1.daf",
    "bouncing.daf",
    "tdxobject.daf",
    "init1.dat",
    "init2.dat",
    "init3.dat",
    "init4.dat",
    "init5.dat",
    "init6.dat",
    "init7.dat",
    "init8.dat",
    "init9.dat",
    "chrome.tga",
    "crystal2.tga",
    "tile.tga",
    "wicker.tga",
    "paint_a.tga",
    "paint_b.tga",
    "paint_c.tga",
    "3dfx.tga",
    "plate.tga",
    "bee.tga",
];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
/// 0 `plain` and 1 `textured` come from `prelude.wgsl`; 2 `surface` is here.
pub const PROGRAMS: &[&str] = &["plain", "textured", "surface"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("1", "Cam 1"),
    ("2", "Cam 2"),
    ("3", "Cam 3"),
    ("4", "Cam 4"),
    ("r", "30/15Hz"),
    (" ", "Spin"),
    ("b", "Bounce"),
    (".", "Step"),
    ("h", "Help"),
];

const PLAIN: usize = 0;
const TEXTURED: usize = 1;
const SURFACE: usize = 2;

/// One saved camera position, decoded from an `init*.dat` state block.
#[derive(Clone, Copy)]
struct Preset {
    yaw: f32,
    dist: f32,
    height: f32,
}

/// Pulls a camera out of the real 121-float (0x1e8-byte) state block. The
/// rotation matrix lives around words 71..90; words 65/66 carry position-like
/// values. Field semantics are reconstructed (see `docs/ENGINE.md`), but the
/// numbers are the demo's own, so the nine presets differ as the originals do.
fn parse_init(data: &[u8]) -> Preset {
    let words: Vec<f32> = data
        .chunks_exact(4)
        .take(121)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let w = |i: usize| words.get(i).copied().unwrap_or(0.0);
    let yaw = w(71).atan2(w(72));
    let dist = (14.0 + w(66).abs() * 0.35).clamp(10.0, 60.0);
    let height = (4.0 + w(65).abs() * 0.12).clamp(3.0, 14.0);
    Preset { yaw, dist, height }
}

pub struct Demo {
    floor: TextureId,
    wall: TextureId,
    paint_a: TextureId,
    paint_b: TextureId,
    painting: TextureId,
    plate: TextureId,
    chrome: TextureId,
    crystal: TextureId,
    bee: TextureId,
    presets: [Preset; 9],
    camera: usize,
    /// Right screen updates at 30 Hz (2) or 15 Hz (4) times slower.
    slow: u32,
    spin: bool,
    bounce: bool,
    help: bool,
    stepping: bool,
    time: f64,
    yaw: f32,
    pitch: f32,
    held: Option<(f64, f64, u8)>,
    drag: (f32, f32),
    /// Non-blank marker, so a shot can be checked without reading the frame.
    frames: u64,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let mut tex = |name: &str| -> Result<TextureId, Error> {
            Ok(gl.texture(gpu, &files.image(name)?, REPEAT))
        };
        let floor = tex("tile.tga")?;
        let wall = tex("wicker.tga")?;
        let paint_a = tex("paint_a.tga")?;
        let paint_b = tex("paint_b.tga")?;
        let painting = tex("3dfx.tga")?;
        let plate = tex("plate.tga")?;
        let chrome = tex("chrome.tga")?;
        let crystal = tex("crystal2.tga")?;
        let bee = tex("bee.tga")?;
        let mut presets = [Preset {
            yaw: 0.0,
            dist: 20.0,
            height: 6.0,
        }; 9];
        for (i, p) in presets.iter_mut().enumerate() {
            let name = format!("init{}.dat", i + 1);
            if let Ok(data) = files.get(&name) {
                *p = parse_init(data);
            }
        }
        Ok(Self {
            floor,
            wall,
            paint_a,
            paint_b,
            painting,
            plate,
            chrome,
            crystal,
            bee,
            presets,
            camera: 0,
            slow: 2,
            spin: true,
            bounce: true,
            help: false,
            stepping: false,
            time: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            held: None,
            drag: (0.0, 0.0),
            frames: 0,
        })
    }

    pub fn step(&mut self, dt: f64) {
        if !self.stepping {
            self.time += dt;
        }
        if self.held.is_some_and(|h| h.2 & 1 != 0) {
            let drag = std::mem::take(&mut self.drag);
            self.yaw += drag.0.to_radians();
            self.pitch = (self.pitch + drag.1.to_radians()).clamp(-0.4, 0.8);
        }
        self.frames += 1;
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        self.time = seconds;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return matches!(
                key,
                "1" | "2"
                    | "3"
                    | "4"
                    | "5"
                    | "6"
                    | "7"
                    | "8"
                    | "9"
                    | "r"
                    | "R"
                    | " "
                    | "b"
                    | "B"
                    | "."
                    | "h"
                    | "H"
            );
        }
        match key {
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                self.camera = key.chars().next().unwrap() as usize - '1' as usize;
                self.yaw = 0.0;
                self.pitch = 0.0;
            }
            "r" | "R" => self.slow = if self.slow == 2 { 4 } else { 2 },
            " " => self.spin = !self.spin,
            "b" | "B" => self.bounce = !self.bounce,
            "." => self.stepping = !self.stepping,
            "h" | "H" => self.help = !self.help,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, py, pb)) = self.held
                && pb & 1 != 0
            {
                self.drag.0 += ((x - px) * 160.0) as f32;
                self.drag.1 += ((y - py) * 120.0) as f32;
            }
            self.held = Some((x, y, buttons));
        } else {
            self.held = None;
        }
    }

    pub fn wheel(&mut self, _amount: f64) {}

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "camera {} right {}Hz spin {} bounce {} step {} t {:.2}",
            self.camera + 1,
            60 / self.slow,
            self.spin,
            self.bounce,
            self.stepping,
            self.time,
        )
    }

    pub fn draw(&mut self, _gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        // Two independent viewports: each half gets half the horizontal clip range.
        let aspect = (w.max(1) as f32 / 2.0) / h.max(1) as f32;
        gl.begin([0.02, 0.02, 0.03, 1.0]);

        // Left: the smooth, 60 Hz image. Right: sampled every `slow`-th frame.
        let left = self.time;
        let rate = (60 / self.slow) as f64;
        let right = (self.time * rate).floor() / rate;

        self.draw_half(gl, aspect, left, -1.0);
        self.draw_half(gl, aspect, right, 1.0);

        // A divider down the middle, in plain colour.
        let bar = [
            Vertex::new(Vec4::new(-0.004, -1.0, 0.0, 1.0), [0.7, 0.75, 0.85, 1.0]),
            Vertex::new(Vec4::new(0.004, -1.0, 0.0, 1.0), [0.7, 0.75, 0.85, 1.0]),
            Vertex::new(Vec4::new(0.004, 1.0, 0.0, 1.0), [0.7, 0.75, 0.85, 1.0]),
            Vertex::new(Vec4::new(-0.004, 1.0, 0.0, 1.0), [0.7, 0.75, 0.85, 1.0]),
        ];
        gl.quad(State::new(PLAIN), &Surface::default(), bar);
    }

    fn draw_half(&self, gl: &mut Gl, aspect: f32, t: f64, side: f32) {
        let p = self.presets[self.camera];
        let yaw = p.yaw + self.yaw + if self.spin { t as f32 * 0.25 } else { 0.0 };
        let eye = Vec3::new(
            yaw.sin() * p.dist,
            p.height + self.pitch * 8.0,
            yaw.cos() * p.dist,
        );
        let proj = from_opengl(perspective(60.0, aspect, 0.1, 200.0));
        let view = look_at(eye, Vec3::new(0.0, 3.0, 0.0), Vec3::Y);
        let vp = proj * view;
        let lit = [1.0, 1.0, 1.0, 1.0];

        // Everything is emitted in clip space; `remap` folds it into this half.
        let mut emit = |state: State, surface: Surface, verts: &[Vertex]| {
            let folded: Vec<Vertex> = verts.iter().map(|&v| remap(v, side)).collect();
            gl.draw(state, &surface, &folded);
        };

        // Floor.
        let q = quad(
            vp,
            Vec3::new(-8.0, 0.0, -8.0),
            Vec3::new(16.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 16.0),
            lit,
            [0.0, 0.0, 4.0, 4.0],
        );
        emit(
            State::new(TEXTURED).depth(Depth::Less(true)),
            Surface::textures(&[self.floor]),
            &q,
        );

        // Back wall and side walls.
        let q = quad(
            vp,
            Vec3::new(-8.0, 0.0, -8.0),
            Vec3::new(16.0, 0.0, 0.0),
            Vec3::new(0.0, 8.0, 0.0),
            [0.85, 0.85, 0.9, 1.0],
            [0.0, 0.0, 3.0, 1.0],
        );
        emit(
            State::new(TEXTURED).depth(Depth::Less(true)),
            Surface::textures(&[self.wall]),
            &q,
        );
        let q = quad(
            vp,
            Vec3::new(-8.0, 0.0, -8.0),
            Vec3::new(0.0, 0.0, 16.0),
            Vec3::new(0.0, 8.0, 0.0),
            [0.8, 0.8, 0.85, 1.0],
            [0.0, 0.0, 3.0, 1.0],
        );
        emit(
            State::new(TEXTURED).depth(Depth::Less(true)),
            Surface::textures(&[self.paint_a]),
            &q,
        );
        let q = quad(
            vp,
            Vec3::new(8.0, 0.0, -8.0),
            Vec3::new(0.0, 0.0, 16.0),
            Vec3::new(0.0, 8.0, 0.0),
            [0.8, 0.8, 0.85, 1.0],
            [0.0, 0.0, 3.0, 1.0],
        );
        emit(
            State::new(TEXTURED).depth(Depth::Less(true)),
            Surface::textures(&[self.paint_b]),
            &q,
        );

        // The scrolling "3Dfx" painting on the back wall: this is the texture the
        // readme says looks shaky on the low-rate screen. It slides with time.
        let slide = (t as f32 * 1.5) % 4.0 - 2.0;
        let q = quad(
            vp,
            Vec3::new(-3.0 + slide, 3.0, -7.9),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
            lit,
            [0.0, 1.0, 1.0, 0.0],
        );
        emit(
            State::new(TEXTURED).depth(Depth::LessEqual(false)),
            Surface::textures(&[self.painting]),
            &q,
        );

        // A small wall plate.
        let q = quad(
            vp,
            Vec3::new(4.0, 2.0, -7.9),
            Vec3::new(2.5, 0.0, 0.0),
            Vec3::new(0.0, 2.5, 0.0),
            lit,
            [0.0, 1.0, 1.0, 0.0],
        );
        emit(
            State::new(TEXTURED).depth(Depth::LessEqual(false)),
            Surface::textures(&[self.plate]),
            &q,
        );

        // Bouncing ball: a chrome sphere whose height steps on the right screen.
        let bob = if self.bounce {
            (t as f32 * 2.2).sin().abs() * 3.2
        } else {
            1.2
        };
        let ball = sphere(
            vp,
            Vec3::new(-2.2, 1.0 + bob, 0.5),
            1.1,
            [0.95, 0.95, 1.0, 1.0],
        );
        let surf = Surface::textures(&[self.chrome, self.bee, WHITE, WHITE])
            .param(0, [0.35, 0.0, 0.0, 0.0]);
        emit(State::new(SURFACE).depth(Depth::Less(true)), surf, &ball);

        // Spinning crystal object in the middle of the room.
        let spin = Mat4::from_rotation_y(if self.spin { t as f32 * 1.3 } else { 0.4 });
        let model = Mat4::from_translation(Vec3::new(2.4, 1.6, 1.0)) * spin;
        let obj = cube(vp, model, 1.1, [0.9, 0.95, 1.0, 1.0]);
        emit(
            State::new(TEXTURED).depth(Depth::Less(true)),
            Surface::textures(&[self.crystal]),
            &obj,
        );

        if self.help {
            // A plain banner so the help toggle is visible in stills.
            let q = quad(
                vp,
                Vec3::new(-3.0, 6.0, -7.85),
                Vec3::new(6.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                [0.15, 0.2, 0.3, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            );
            emit(
                State::new(PLAIN).depth(Depth::LessEqual(false)),
                Surface::default(),
                &q,
            );
        }
    }
}

/// Fold a full-screen clip-space vertex into the left or right half.
fn remap(mut v: Vertex, side: f32) -> Vertex {
    v.position[0] = v.position[0] * 0.5 + side * 0.5 * v.position[3];
    v
}

fn vertex(vp: Mat4, world: Vec4, color: [f32; 4], u: f32, v: f32) -> Vertex {
    Vertex::new(vp * world, color).uv(0, u, v)
}

/// A quad as two triangles, given a corner and two edge vectors.
fn quad(vp: Mat4, o: Vec3, ex: Vec3, ey: Vec3, color: [f32; 4], uv: [f32; 4]) -> Vec<Vertex> {
    let p = |a: f32, b: f32| {
        Vec4::new(
            o.x + ex.x * a + ey.x * b,
            o.y + ex.y * a + ey.y * b,
            o.z + ex.z * a + ey.z * b,
            1.0,
        )
    };
    let [u0, v0, u1, v1] = uv;
    let c = [
        vertex(vp, p(0.0, 0.0), color, u0, v0),
        vertex(vp, p(1.0, 0.0), color, u1, v0),
        vertex(vp, p(1.0, 1.0), color, u1, v1),
        vertex(vp, p(0.0, 1.0), color, u0, v1),
    ];
    vec![c[0], c[1], c[2], c[0], c[2], c[3]]
}

fn cube(vp: Mat4, model: Mat4, s: f32, color: [f32; 4]) -> Vec<Vertex> {
    let pts = [
        Vec3::new(-s, -s, -s),
        Vec3::new(s, -s, -s),
        Vec3::new(s, s, -s),
        Vec3::new(-s, s, -s),
        Vec3::new(-s, -s, s),
        Vec3::new(s, -s, s),
        Vec3::new(s, s, s),
        Vec3::new(-s, s, s),
    ];
    let faces = [
        [0, 1, 2, 3],
        [5, 4, 7, 6],
        [4, 0, 3, 7],
        [1, 5, 6, 2],
        [3, 2, 6, 7],
        [4, 5, 1, 0],
    ];
    let shades = [1.0, 0.82, 0.7, 0.75, 0.9, 0.62];
    let mut out = Vec::new();
    for (f, sh) in faces.iter().zip(shades) {
        let c = [color[0] * sh, color[1] * sh, color[2] * sh, 1.0];
        let w = |i: usize| model * pts[f[i]].extend(1.0);
        let q = [
            vertex(vp, w(0), c, 0.0, 0.0),
            vertex(vp, w(1), c, 1.0, 0.0),
            vertex(vp, w(2), c, 1.0, 1.0),
            vertex(vp, w(3), c, 0.0, 1.0),
        ];
        out.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
    }
    out
}

/// A small UV sphere, used for the chrome bouncing ball.
fn sphere(vp: Mat4, center: Vec3, r: f32, color: [f32; 4]) -> Vec<Vertex> {
    let (stacks, slices) = (12u32, 20u32);
    let at = |i: u32, j: u32| {
        let phi = PI * i as f32 / stacks as f32;
        let theta = 2.0 * PI * j as f32 / slices as f32;
        let n = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
        (center + n * r).extend(1.0)
    };
    let uv = |i: u32, j: u32| (j as f32 / slices as f32, i as f32 / stacks as f32);
    let mut out = Vec::with_capacity((stacks * slices * 6) as usize);
    for i in 0..stacks {
        for j in 0..slices {
            let (pa, pb, pc, pd) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            let (ua, va) = uv(i, j);
            let (ub, vb) = uv(i + 1, j);
            let (uc, vc) = uv(i + 1, j + 1);
            let (ud, vd) = uv(i, j + 1);
            for (p, u, v) in [
                (pa, ua, va),
                (pb, ub, vb),
                (pc, uc, vc),
                (pa, ua, va),
                (pc, uc, vc),
                (pd, ud, vd),
            ] {
                out.push(vertex(vp, p, color, u, v));
            }
        }
    }
    out
}
