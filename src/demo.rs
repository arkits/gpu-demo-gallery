//! The cube (`borg.exe`; addresses are in it and in `gmt.dll`).
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::BlendFactor as B;

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Gl, Gpu, REPEAT, Sampling, State, Surface, TextureId, Vertex, from_opengl, perspective,
};

pub const FILES: &[&str] = &[
    "p06.tga",
    "p01.tga",
    "p24.tga",
    "cube1.ppm",
    "cube2.ppm",
    "cube3.ppm",
    "cube4.ppm",
    "lights.ppm",
    "spot1.ppm",
    "envmatt.ppm",
];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["textured", "ship", "glow", "spot", "windows"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("1", "Ship 1"),
    ("2", "Ship 2"),
    ("3", "Ship 3"),
    ("4", "Ship 4"),
    ("5", "Lights 1"),
    ("6", "Lights 2"),
    ("7", "Spotlight"),
    ("8", "Reflections"),
    ("0", "None"),
    (" ", "Freeze"),
];

const TEXTURED: usize = 0;
const SHIP: usize = 1;
const GLOW: usize = 2;
const SPOT: usize = 3;
const WINDOWS: usize = 4;

/// Times each picture repeats across a side (`-r`).
const REPEATS: f32 = 3.5;
const DIFFUSE: f32 = 230.0;

/// Corners, and the normal of one side kept with each of the first six (0x4015f0).
const CORNERS: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0],
    [0.0, 1.0, 1.0],
    [1.0, 0.0, 0.0],
    [1.0, 0.0, 1.0],
    [1.0, 1.0, 0.0],
    [1.0, 1.0, 1.0],
];
/// Corners around each side, and its normal. The last two have no pictures of their own.
const SIDES: [([usize; 4], [f32; 3]); 6] = [
    ([4, 5, 7, 6], [1.0, 0.0, 0.0]),
    ([2, 3, 1, 0], [-1.0, 0.0, 0.0]),
    ([2, 0, 4, 6], [0.0, 0.0, -1.0]),
    ([1, 3, 7, 5], [0.0, 0.0, 1.0]),
    ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ([6, 7, 3, 2], [0.0, 1.0, 0.0]),
];
/// Picture coordinates at corners 0 to 7 of the four sides around.
const AROUND: [[f32; 2]; 8] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [0.0, 1.0],
    [1.0, 1.0],
    [1.0, 0.0],
    [0.0, 0.0],
    [1.0, 1.0],
    [0.0, 1.0],
];
/// And on the two ends, corner by corner as in `SIDES`.
const ENDS: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
/// Turned, for the second glow and the windows.
const TURNED: [[f32; 2]; 8] = [
    [0.0, 0.0],
    [0.0, 1.0],
    [1.0, 0.0],
    [1.0, 1.0],
    [0.0, 1.0],
    [0.0, 0.0],
    [1.0, 1.0],
    [1.0, 0.0],
];

#[derive(Clone, Copy)]
struct Set {
    /// Port, starboard and front views of space.
    space: [TextureId; 3],
    cube: [TextureId; 4],
    lights: TextureId,
    windows: TextureId,
}

pub struct Demo {
    /// By whether filtered.
    sets: [Set; 2],
    spot: TextureId,
    on: u32,
    background: bool,
    linear: bool,
    ambient: f32,
    distance: f32,
    speed: f32,
    frozen: bool,
    rotation: Mat4,
    frame: u32,
    due: f64,
    held: Option<(f64, f64)>,
}

fn rotate(axis: Vec3, degrees: f32) -> Mat4 {
    Mat4::from_axis_angle(axis, degrees.to_radians())
}

/// The turn of each frame changes every 64 (0x4015f0, 0x402da0).
fn turn(frame: u32) -> Mat4 {
    match frame >> 6 & 3 {
        0 => rotate(Vec3::X, 0.077) * rotate(Vec3::Y, 0.189),
        1 => rotate(Vec3::Y, 0.091) * rotate(Vec3::Z, 0.147),
        2 => rotate(Vec3::Z, 0.091) * rotate(Vec3::X, 0.175),
        _ => rotate(Vec3::Y, 0.133),
    }
}

/// From the eye to the spotlight's picture (0x4015f0).
fn spotlight() -> Mat4 {
    Mat4::from_translation(Vec3::new(0.5, 0.5, 0.0))
        * perspective(20.0, 1.0, 1.0, 20000.0)
        * Mat4::from_translation(Vec3::Z * -11.0)
        * rotate(Vec3::Y, 13.0)
        * rotate(Vec3::X, -9.0)
        * Mat4::from_translation(Vec3::Z * 5.0)
}

/// Where something at `p` with normal `n` mirrors space: which view, and where in it
/// (0x401110). The first corner of a side chooses the view.
fn mirrored(p: Vec3, n: Vec3, view: Option<usize>) -> (usize, Vec2) {
    let e = p.normalize();
    let r = n * -n.dot(e) * 2.0 - e;
    let view = view.unwrap_or_else(|| {
        let least = if r.z >= 0.0 { r.z * 1.2 } else { 0.0001 };
        if r.x > least {
            1
        } else if r.x < -least {
            0
        } else {
            2
        }
    });
    let uv = if view == 2 {
        let q = p - r * (p.z / r.z);
        Vec2::new(q.y * 9.0 + 20.0, q.x * 9.0 + 0.5)
    } else {
        let x = [-10.0, 10.0][view];
        let q = p - r * ((p.x - x) / r.x);
        Vec2::new((q.z + 40.0) / 40.0, (q.y + 20.0) * 6.4 / 256.0)
    };
    (view, uv)
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let matte = files.image("envmatt.ppm")?.map(|[r, g, b, _]| {
            // Windows are wherever there is any color (0x401040).
            [r, g, b, if [r, g, b] == [0; 3] { 0 } else { 255 }]
        });
        let mut sets = Vec::new();
        for linear in [false, true] {
            let s = Sampling { linear, ..REPEAT };
            let mut load = |n: &str, s| Ok::<_, Error>(gl.texture(gpu, &files.image(n)?, s));
            let edge = Sampling { linear, ..CLAMP };
            sets.push(Set {
                space: [
                    load("p06.tga", edge)?,
                    load("p01.tga", edge)?,
                    load("p24.tga", s)?,
                ],
                cube: [
                    load("cube1.ppm", s)?,
                    load("cube2.ppm", s)?,
                    load("cube3.ppm", s)?,
                    load("cube4.ppm", s)?,
                ],
                lights: load("lights.ppm", s)?,
                windows: gl.texture(gpu, &matte, s),
            });
        }
        let spot = Sampling {
            mips: false,
            ..CLAMP
        };
        Ok(Self {
            sets: [sets[0], sets[1]],
            spot: gl.texture(gpu, &files.image("spot1.ppm")?, spot),
            // The original begins with none and lets the presenter add them.
            on: 0xff,
            background: true,
            linear: true,
            ambient: 50.0,
            distance: -5.0,
            speed: 0.0,
            frozen: false,
            rotation: Mat4::IDENTITY,
            frame: 0,
            due: 0.0,
            held: None,
        })
    }

    /// The original moves once a frame, which is taken as 60 a second.
    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            self.distance = (self.distance + self.speed).clamp(-40.0, -1.2);
            if !self.frozen && self.held.is_none() {
                self.rotation *= turn(self.frame);
                self.frame = self.frame.wrapping_add(1);
            }
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.step(1.0 / 60.0));
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key {
            "0" | "c" => self.on = 0,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => {
                self.on ^= 1 << (key.as_bytes()[0] - b'1');
            }
            "a" => self.ambient = (self.ambient - 2.0).max(0.0),
            "A" => self.ambient = (self.ambient + 2.0).min(255.0),
            "b" => self.linear = false,
            "B" => self.linear = true,
            "e" => self.background = false,
            "E" => self.background = true,
            "x" => (self.distance, self.speed) = (self.distance + 0.01, 0.0),
            "z" => (self.distance, self.speed) = (self.distance - 0.01, 0.0),
            "X" => self.speed += 0.01,
            "Z" => self.speed -= 0.01,
            " " => (self.frozen, self.speed) = (!self.frozen, 0.0),
            _ => return false,
        }
        true
    }

    /// Dragging turns the cube. Not in the original.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 == 0 {
            self.held = None;
            return;
        }
        if let Some((x0, y0)) = self.held.replace((x, y)) {
            let (dx, dy) = ((x - x0) as f32, (y - y0) as f32);
            self.rotation =
                rotate(Vec3::Y, dx * 220.0) * rotate(Vec3::X, dy * 160.0) * self.rotation;
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.distance = (self.distance * (1.0 - amount as f32)).clamp(-40.0, -1.2);
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let on: Vec<String> = (0..8)
            .filter(|i| self.on >> i & 1 == 1)
            .map(|i| (i + 1).to_string())
            .collect();
        format!("textures {} distance {:.2}", on.concat(), -self.distance)
    }

    pub fn draw(&mut self, _: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        // 30 degrees up and down; wider windows show more at the sides.
        let projection = from_opengl(perspective(30.0, w as f32 / h as f32, 0.06, 200.0));
        let set = self.sets[usize::from(self.linear)];
        let white = [1.0; 4];
        if self.background {
            // Two walls (0x401c10).
            let (far, near) = (-62.0, -22.0);
            for (x, t) in [(-10.0, set.space[0]), (10.0, set.space[1])] {
                let v = |y: f32, z: f32, u: f32, v: f32| {
                    Vertex::new(projection * Vec4::new(x, y, z, 1.0), white).uv(0, u, v)
                };
                gl.quad(
                    State::new(TEXTURED),
                    &Surface::textures(&[t]),
                    [
                        v(-20.0, near, 1.0, 0.0),
                        v(-20.0, far, 0.0, 0.0),
                        v(20.0, far, 0.0, 1.0),
                        v(20.0, near, 1.0, 1.0),
                    ],
                );
            }
        }
        // The cube turns about a corner, as in the original.
        let model = Mat4::from_translation(Vec3::new(-0.5, -0.5, self.distance)) * self.rotation;
        let all = projection * model;
        let corner = |c: usize| Vec3::from(CORNERS[c]);
        // Sides that face the eye; the order of the rest does not matter then.
        let shown = |(corners, normal): &([usize; 4], [f32; 3])| {
            let p = model.transform_point3(corner(corners[0]));
            model.transform_vector3(Vec3::from(*normal)).dot(p) < 0.0
        };
        let sides = || SIDES.iter().enumerate().filter(|s| shown(s.1));
        let on = |bit: u32| if self.on >> bit & 1 == 1 { 1.0 } else { 0.0 };

        // Lit: the light is at the viewer's upper right.
        let light = self
            .rotation
            .transpose()
            .transform_vector3(Vec3::splat(0.577));
        let surface = Surface::textures(&set.cube[..2]).param(0, [on(0), on(1), 0.0, 0.0]);
        for (k, (corners, normal)) in sides() {
            let n = Vec3::from(*normal).dot(light).max(0.0);
            let c = ((self.ambient + DIFFUSE * n) / 255.0).min(1.0);
            let v = std::array::from_fn(|i| {
                let [s, t] = if k < 4 { AROUND[corners[i]] } else { ENDS[i] };
                Vertex::new(all * corner(corners[i]).extend(1.0), [c, c, c, 1.0])
                    .uv(0, s, t)
                    .uv(1, s * REPEATS, t * REPEATS)
            });
            gl.quad(State::new(SHIP), &surface, v);
        }
        let around = || sides().filter(|s| s.0 < 4);
        let place = |c: usize| all * corner(c).extend(1.0);
        if self.on & 0xc != 0 {
            let mut state = State::new(SHIP);
            if self.on & 3 != 0 {
                // Twice the product with what is there.
                state = state.blend(B::Dst, B::Src);
            }
            let surface = Surface::textures(&set.cube[2..]).param(0, [on(2), on(3), 0.0, 0.0]);
            for (_, (corners, _)) in around() {
                let v = corners.map(|c| {
                    let [s, t] = AROUND[c].map(|a| a * REPEATS * 2.7);
                    Vertex::new(place(c), white)
                        .uv(0, s, t)
                        .uv(1, s * 1.3, t * 1.3)
                });
                gl.quad(state, &surface, v);
            }
        }
        let add = |p| State::new(p).blend(B::One, B::One);
        if self.on & 0x40 != 0 {
            let spot = spotlight() * model;
            for (_, (corners, _)) in sides() {
                let v = corners.map(|c| {
                    let s = spot * corner(c).extend(1.0);
                    Vertex::new(place(c), white).uvw(0, s.to_array())
                });
                gl.quad(add(SPOT), &Surface::textures(&[self.spot]), v);
            }
        }
        if self.on & 0x30 != 0 {
            let surface = Surface::textures(&[set.lights, set.lights]);
            let surface = surface.param(0, [on(4), on(5), 0.0, 0.0]);
            for (_, (corners, _)) in around() {
                let v = corners.map(|c| {
                    let [s, t] = AROUND[c].map(|a| a * REPEATS * 0.27);
                    let [u, w] = TURNED[c].map(|a| a * REPEATS * 0.9045);
                    Vertex::new(place(c), white).uv(0, s, t).uv(1, u, w)
                });
                gl.quad(add(GLOW), &surface, v);
            }
        }
        if self.on & 0x80 != 0 {
            let over = State::new(WINDOWS).blend(B::SrcAlpha, B::OneMinusSrcAlpha);
            for (_, (corners, normal)) in around() {
                let n = model.transform_vector3(Vec3::from(*normal));
                let mut view = None;
                let v = corners.map(|c| {
                    let (chosen, at) = mirrored(model.transform_point3(corner(c)), n, view);
                    view = Some(chosen);
                    let [u, w] = TURNED[c].map(|a| a * REPEATS * 1.62);
                    Vertex::new(place(c), white).uv(0, at.x, at.y).uv(1, u, w)
                });
                let space = set.space[view.expect("four corners")];
                gl.quad(over, &Surface::textures(&[space, set.windows]), v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sides_are_flat_and_face_out() {
        for (corners, normal) in SIDES {
            let n = Vec3::from(normal);
            let middle = Vec3::splat(0.5);
            for c in corners {
                let d = (Vec3::from(CORNERS[c]) - middle).dot(n);
                assert!((d - 0.5).abs() < 1e-6, "{corners:?}");
            }
        }
    }

    #[test]
    fn turns_stay_turns() {
        let m = (0..10_000).fold(Mat4::IDENTITY, |m, f| m * turn(f));
        assert!((m.determinant() - 1.0).abs() < 0.01);
    }

    #[test]
    fn walls_are_mirrored_by_sides_that_face_them() {
        let (view, uv) = mirrored(Vec3::new(-4.0, 0.0, -0.5), Vec3::new(0.9, 0.0, 0.436), None);
        assert_eq!(view, 1);
        assert!((uv.y - 0.5).abs() < 0.01, "{uv}");
    }
}
