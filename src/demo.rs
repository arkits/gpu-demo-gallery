//! The torus (`donut.exe`; addresses are in it).
use glam::{Mat3, Mat4, Vec3, Vec4};
use wgpu::BlendFactor as B;

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, REPEAT, State, Surface, TextureId, Vertex, from_opengl, perspective,
};
use crate::image::Image;

pub const FILES: &[&str] = &[
    "tex.dat",
    "bg.dat",
    "bump0.dat",
    "bump1.dat",
    "bump2.dat",
    "bump3.dat",
    "bump4.dat",
];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "base", "emboss", "mirror"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("p", "Mode"),
    ("t", "Bumps"),
    ("d", "Deeper"),
    ("D", "Flatter"),
    ("c", "More reflection"),
    ("C", "Less"),
    ("g", "Background"),
    ("w", "Wireframe"),
    ("r", "Reset"),
];

const PLAIN: usize = 0;
const TEXTURED: usize = 1;
const BASE: usize = 2;
const EMBOSS: usize = 3;
const MIRROR: usize = 4;

/// Texels that each bump picture is first shifted by (0x41c610).
const DEPTHS: [f32; 5] = [4.0, 4.0, 3.0, 3.0, 2.0];
/// Ambient, diffuse and highlight, of 255 (0x4150c8).
const LIGHT: [f32; 3] = [32.0, 192.0, 128.0];
const EMBOSSED: u32 = 1;
const MIRRORED: u32 = 2;

/// Turns by degrees a frame about x and y (0x401000).
struct Spin {
    rate: (f32, f32),
    rotation: Mat3,
}

impl Spin {
    fn step(&mut self) {
        let (x, y) = (self.rate.0.to_radians(), self.rate.1.to_radians());
        self.rotation = Mat3::from_rotation_y(y) * Mat3::from_rotation_x(x) * self.rotation;
    }
}

struct Corner {
    position: Vec3,
    normal: Vec3,
    /// Along the tube and around it.
    tangent: Vec3,
    binormal: Vec3,
    uv: [f32; 2],
}

pub struct Demo {
    base: TextureId,
    background: TextureId,
    environment: TextureId,
    bumps: Vec<TextureId>,
    torus: Spin,
    light: Spin,
    scale: f32,
    inner: f32,
    detail: usize,
    grid: Vec<Corner>,
    mode: u32,
    bump: usize,
    depth: f32,
    /// How much of the reflection shows, of 255.
    mirror: f32,
    /// Added to every height.
    excess: f32,
    double: bool,
    show_background: bool,
    wireframe: bool,
    normals: bool,
    /// Dragging turns the light.
    turn_light: bool,
    due: f64,
    held: Option<(f64, f64, u8)>,
    /// Degrees dragged since the last frame.
    drag: (f32, f32),
}

/// Bends a picture so that a sphere mapped flat over a hemisphere shows it (0x407cd0).
fn warp(image: &Image) -> Image {
    let n = image.width as i32;
    let half = n as f32 * 0.5;
    let mut rgba = Vec::with_capacity(image.rgba.len());
    for y in 0..n {
        for x in 0..n {
            let (x, y) = (x as f32 - half, y as f32 - half);
            let d = x.hypot(y);
            if d > half {
                rgba.extend([0, 0, 0, 255]);
                continue;
            }
            let at = |v: f32| ((v * d / half + half) as i32).clamp(0, n - 1);
            let from = ((at(y) * n + at(x)) * 4) as usize;
            rgba.extend(&image.rgba[from..from + 4]);
        }
    }
    Image {
        rgba,
        ..image.clone()
    }
}

/// `n` by `n` quads around a tube whose ring has an outer radius of 1 (0x408ab0).
fn grid(n: usize, inner: f32) -> Vec<Corner> {
    let tube = (1.0 - inner) * 0.5;
    let angle = |i: usize| std::f32::consts::TAU * i as f32 / n as f32;
    let mut out = Vec::with_capacity((n + 1) * (n + 1));
    for i in 0..=n {
        for j in 0..=n {
            let ((s, c), (sin, cos)) = (angle(i).sin_cos(), angle(j).sin_cos());
            let r = (c + 1.0) * tube + inner;
            let normal = Vec3::new(c * cos, c * sin, s);
            let tangent = Vec3::new(-s * cos, -s * sin, c);
            out.push(Corner {
                position: Vec3::new(cos * r, sin * r, s * tube),
                normal,
                tangent,
                binormal: normal.cross(tangent),
                // Four times around each way; the second runs one step ahead.
                uv: [i as f32 * 4.0 / n as f32, (j + 1) as f32 * 4.0 / n as f32],
            });
        }
    }
    out
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let background = files.image("bg.dat")?;
        let mut bumps = Vec::new();
        for n in &FILES[2..] {
            let gray = files.image(n)?.map(|[r, g, b, _]| {
                let i = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3) as u8;
                [i, i, i, 255]
            });
            bumps.push(gl.texture(gpu, &gray, REPEAT));
        }
        let mut demo = Self {
            base: gl.texture(gpu, &files.image("tex.dat")?, REPEAT),
            environment: gl.texture(gpu, &warp(&background), CLAMP),
            background: gl.texture(gpu, &background, CLAMP),
            bumps,
            torus: Spin {
                rate: (0.0, 0.0),
                rotation: Mat3::IDENTITY,
            },
            light: Spin {
                rate: (0.0, 0.0),
                rotation: Mat3::IDENTITY,
            },
            scale: 1.0,
            inner: 0.25,
            detail: 50,
            grid: grid(50, 0.25),
            mode: EMBOSSED | MIRRORED,
            bump: 0,
            depth: DEPTHS[0],
            mirror: 127.0,
            excess: 0.0,
            double: true,
            show_background: false,
            wireframe: false,
            normals: false,
            turn_light: false,
            due: 0.0,
            held: None,
            drag: (0.0, 0.0),
        };
        demo.reset();
        Ok(demo)
    }

    fn reset(&mut self) {
        self.torus.rotation = Mat3::IDENTITY;
        self.light.rotation = Mat3::IDENTITY;
        self.torus.rate = (30f32.to_radians(), 10f32.to_radians());
        self.light.rate = (0.0, 0.0);
        self.scale = 1.0;
    }

    /// The original turns once a frame, which is taken as 60 a second.
    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            // What the last frame was dragged by goes on after letting go (0x405a00).
            if self.held.is_some_and(|h| h.2 & 1 != 0) {
                let drag = std::mem::take(&mut self.drag);
                self.turned().rate = drag;
            }
            self.torus.step();
            self.light.step();
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.step(1.0 / 60.0));
    }

    fn turned(&mut self) -> &mut Spin {
        match self.turn_light {
            true => &mut self.light,
            false => &mut self.torus,
        }
    }

    fn shape(&mut self, detail: usize, inner: f32) {
        self.detail = detail.clamp(4, 100);
        self.inner = inner.clamp(0.0, 0.98);
        self.grid = grid(self.detail, self.inner);
    }

    /// Capital letters take back, as Control did.
    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key {
            "+" | "=" => self.shape(self.detail + 1, self.inner),
            "-" => self.shape(self.detail - 1, self.inner),
            "PageUp" => self.shape(self.detail, self.inner + 0.01),
            "PageDown" => self.shape(self.detail, self.inner - 0.01),
            "b" | "B" => self.double = !self.double,
            "c" => self.mirror = (self.mirror + 16.0).min(255.0),
            "C" => self.mirror = (self.mirror - 16.0).max(0.0),
            "d" => self.depth = (self.depth + 0.5).min(10.0),
            "D" => self.depth = (self.depth - 0.5).max(0.0),
            "e" => self.excess = (self.excess + 8.0).min(128.0),
            "E" => self.excess = (self.excess - 8.0).max(0.0),
            "g" | "G" => self.show_background = !self.show_background,
            "n" | "N" => self.normals = !self.normals,
            "o" | "O" => self.turn_light = !self.turn_light,
            "p" | "P" => self.mode = (self.mode + 1) & 3,
            "r" | "R" => self.reset(),
            "t" | "T" => {
                self.bump = (self.bump + 1) % self.bumps.len();
                self.depth = DEPTHS[self.bump];
            }
            "w" | "W" => self.wireframe = !self.wireframe,
            _ => return false,
        }
        true
    }

    /// The left button turns, and lets go spinning; the right one scales.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if let Some((x0, y0, held)) = self.held.take() {
            let (dx, dy) = ((x - x0) as f32, (y - y0) as f32);
            if buttons & held & 1 != 0 {
                // A quarter of a degree for each of 640 by 480 pixels.
                self.drag.0 += dy * 120.0;
                self.drag.1 += dx * 160.0;
            } else if buttons & held & 2 != 0 {
                self.wheel(f64::from(dy) * -2.0);
            }
        } else {
            self.drag = (0.0, 0.0);
        }
        if buttons & 3 != 0 {
            self.held = Some((x, y, buttons));
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.scale = (self.scale * (1.0 + amount as f32)).clamp(0.025, 2.25);
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let mode = ["plain", "bumps", "reflection", "bumps and reflection"][self.mode as usize];
        let triangles = self.detail * self.detail * 2;
        format!(
            "{mode}, bump {} by {:.1}, {triangles} triangles",
            self.bump, self.depth
        )
    }

    pub fn draw(&mut self, _: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        let aspect = w as f32 / h as f32;
        // From 2 away, 0.8 of the half width for each unit at the middle (0x4070f0), in 4:3.
        // Wider windows show more at the sides.
        let half = 1.25 / aspect.min(4.0 / 3.0);
        let eye = Mat4::from_translation(Vec3::Z * -2.0);
        let all = from_opengl(perspective(
            2.0 * half.atan().to_degrees(),
            aspect,
            0.05,
            12.0,
        )) * eye
            * Mat4::from_mat3(self.torus.rotation)
            * Mat4::from_scale(Vec3::splat(self.scale));
        let white = [1.0; 4];
        if self.show_background {
            let v = |x: f32, y: f32| {
                Vertex::new(Vec4::new(x, y, 0.0, 1.0), white).uv(0, x * 0.5 + 0.5, y * 0.5 + 0.5)
            };
            let corners = [v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0)];
            let picture = Surface::textures(&[self.background]);
            gl.quad(State::new(TEXTURED), &picture, corners);
        }
        // Light and eye as the torus sees them (0x403d70).
        let back = self.torus.rotation.transpose();
        let light = (back * (self.light.rotation * Vec3::new(1.0, 1.0, 2.0))).normalize();
        let viewer = (back * Vec3::Z).normalize();
        let lit = |c: &Corner| {
            let d = light.dot(c.normal);
            if d < 0.0 {
                return (LIGHT[0], 0.0);
            }
            let r = (c.normal * 2.0 - light).normalize();
            let s = (viewer.dot(r) * d).powi(8);
            (LIGHT[0] + LIGHT[1] * d, LIGHT[2] * s)
        };
        let corners: Vec<(Vec4, f32, f32)> = self
            .grid
            .iter()
            .map(|c| {
                let (d, s) = lit(c);
                (all * c.position.extend(1.0), d / 255.0, s / 255.0)
            })
            .collect();
        let n = self.detail;
        let build = |f: &dyn Fn(usize) -> Vertex| -> Vec<Vertex> {
            let all: Vec<Vertex> = (0..self.grid.len()).map(f).collect();
            let mut out = Vec::with_capacity(n * n * 6);
            for i in 0..n {
                for j in 0..n {
                    let a = i * (n + 1) + j;
                    let [a, b, c, d] = [a, a + n + 1, a + n + 2, a + 1];
                    out.extend([a, b, c, a, c, d].map(|k| all[k]));
                }
            }
            out
        };
        if self.wireframe {
            let mut lines = Vec::new();
            for t in build(&|k| Vertex::new(corners[k].0, white)).chunks_exact(3) {
                lines.extend([t[0], t[1], t[1], t[2], t[2], t[0]]);
            }
            gl.draw(State::new(PLAIN).lines(), &Surface::default(), &lines);
            return;
        }
        let first = Depth::Less(true);
        if self.mode == 0 {
            let v = build(&|k| {
                let ((p, d, s), c) = (corners[k], &self.grid[k]);
                Vertex::new(p, [d, d, d, s]).uv(1, c.uv[0], c.uv[1])
            });
            let surface = Surface::textures(&[self.base, self.base]);
            gl.draw(State::new(BASE).depth(first), &surface, &v);
        }
        let both = self.mode == EMBOSSED | MIRRORED;
        if self.mode & EMBOSSED != 0 {
            let shift = self.depth / 256.0;
            let v = build(&|k| {
                let ((p, d, _), c) = (corners[k], &self.grid[k]);
                let (x, y) = (c.tangent.dot(light) * shift, c.binormal.dot(light) * shift);
                Vertex::new(p, [d, d, d, self.excess / 255.0])
                    .uv(0, c.uv[0] + x, c.uv[1] - y)
                    .uv(1, c.uv[0], c.uv[1])
            });
            let bump = self.bumps[self.bump];
            let only = if both { 0.0 } else { 1.0 };
            let surface = Surface::textures(&[bump, bump]).param(0, [only, 0.0, 0.0, 0.0]);
            gl.draw(State::new(EMBOSS).depth(first), &surface, &v);
        }
        if self.mode & MIRRORED != 0 {
            let v = build(&|k| {
                let ((p, d, _), c) = (corners[k], &self.grid[k]);
                let at = (self.torus.rotation * c.normal) * 0.5 + 0.5;
                Vertex::new(p, [d, d, d, 1.0])
                    .uv(0, at.x, at.y)
                    .uv(1, c.uv[0], c.uv[1])
            });
            let mut state = State::new(MIRROR).depth(first);
            if both {
                // Twice the product, or just that.
                let dst = if self.double { B::Src } else { B::Zero };
                state = state.blend(B::Dst, dst).depth(Depth::LessEqual(false));
            }
            // Four bits of alpha (0x405960).
            let through = ((255.0 - self.mirror) / 16.0).floor() / 15.0;
            let surface = Surface::textures(&[self.environment, self.base]);
            gl.draw(state, &surface.param(0, [0.0, through, 0.0, 0.0]), &v);
        }
        if self.normals {
            let mut lines = Vec::new();
            for (c, (p, ..)) in self.grid.iter().zip(&corners) {
                let end = all * (c.position + c.normal * 0.125).extend(1.0);
                lines.push(Vertex::new(*p, [1.0, 0.0, 0.0, 1.0]));
                lines.push(Vertex::new(end, [1.0, 0.0, 0.0, 1.0]));
            }
            let state = State::new(PLAIN).lines().depth(Depth::LessEqual(false));
            gl.draw(state, &Surface::default(), &lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_torus_is_smooth() {
        for c in grid(20, 0.25) {
            for v in [c.normal, c.tangent, c.binormal] {
                assert!((v.length() - 1.0).abs() < 1e-5);
            }
            assert!(c.normal.dot(c.tangent).abs() < 1e-5);
            // Distance from the ring in the middle of the tube.
            let ring = Vec3::new(c.position.x, c.position.y, 0.0).normalize() * 0.625;
            assert!(((c.position - ring).length() - 0.375).abs() < 1e-4);
        }
    }

    #[test]
    fn warping_keeps_the_middle_and_drops_the_corners() {
        let image = Image {
            width: 8,
            height: 8,
            rgba: vec![200; 256],
        };
        let w = warp(&image);
        assert_eq!(w.rgba[..4], [0, 0, 0, 255]);
        assert_eq!(w.rgba[(4 * 8 + 4) * 4], 200);
    }
}
