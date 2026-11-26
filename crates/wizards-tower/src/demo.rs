//! The Wizard's Tower: five rooms lit by light maps, walked through in the
//! first person. `wiz.exe` loads `room1` to `room5` and the `floor`
//! collision mesh from `wiz.bof` (`0x403c50`); each lit triangle carries its
//! own light map, multiplied over its texture in a second pass. See
//! `docs/ENGINE.md`.
use atb::bof::Object;
use atb::{Bof, Item, View};
use glam::{Mat4, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu};
use crate::render::{self, Camera, Frame, Light, Pictures};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("1", "Room 1"),
    ("2", "Room 2"),
    ("3", "Room 3"),
    ("5", "Room 5"),
    ("ArrowUp", "Forward"),
    ("ArrowDown", "Back"),
    ("ArrowLeft", "Turn left"),
    ("ArrowRight", "Turn right"),
    ("w", "Wireframe"),
    ("t", "Textures"),
];

pub const FILES: &[&str] = &["wiz.bof"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const RATE: f64 = 60.0;
const ROOMS: [&str; 5] = ["room1", "room2", "room3", "room4", "room5"];
/// Eye height over the floor, and the walking and turning speeds. [I]
const EYE: f32 = 55.0;
const WALK: f32 = 4.0;
const TURN: f32 = 0.04;

pub struct Demo {
    bof: Bof,
    rooms: Vec<u32>,
    /// The walkable floor: triangles as three corners.
    floor: Vec<[Vec3; 3]>,
    pictures: Pictures,
    pos: Vec3,
    /// The room drawn: only the one the player is in (`0x402fb0`).
    room: usize,
    yaw: f32,
    pitch: f32,
    held: [bool; 4],
    drag: (f32, f32),
    pointer: Option<(f64, f64, u8)>,
    wireframe: bool,
    textures: bool,
    due: f64,
    frames: u64,
    triangles: usize,
}

impl Demo {
    pub fn new(_gpu: &Gpu, _gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let bof = Bof::parse(files.get("wiz.bof")?)?;
        let rooms = ROOMS
            .iter()
            .map(|r| bof.find(r).ok_or_else(|| format!("wiz.bof has no {r}")))
            .collect::<Result<Vec<_>, _>>()?;
        // The collision mesh: each record a plane, then three corners.
        let floor = match bof.find("floor").and_then(|id| bof.get(id)) {
            Some(Object::CMesh(_, tris)) => tris
                .iter()
                .map(|t| [4, 7, 10].map(|i| Vec3::new(t[i], t[i + 1], t[i + 2])))
                .collect(),
            _ => return Err("wiz.bof has no floor mesh".into()),
        };
        let mut demo = Self {
            bof,
            rooms,
            floor,
            pictures: Pictures::default(),
            pos: Vec3::ZERO,
            room: 0,
            yaw: 0.0,
            pitch: 0.0,
            held: [false; 4],
            drag: (0.0, 0.0),
            pointer: None,
            wireframe: false,
            textures: true,
            due: 0.0,
            frames: 0,
            triangles: 0,
        };
        demo.warp(0);
        Ok(demo)
    }

    /// The places the room keys put the player (`0x402fb0`).
    const STARTS: [(f32, f32); 5] = [
        (5.0, -360.0),
        (-472.0, 295.0),
        (-230.0, 867.0),
        (-472.0, 295.0),
        (305.0, 836.0),
    ];

    fn warp(&mut self, room: usize) {
        let (x, z) = Self::STARTS[room];
        let p = Vec3::new(x, 0.0, z);
        self.pos = Vec3::new(x, self.ground(p, 1.0e4).unwrap_or(0.0) + EYE, z);
        self.room = room;
    }

    /// The highest floor under `p` no higher than `below`.
    fn ground(&self, p: Vec3, below: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for [a, b, c] in &self.floor {
            let d = (b.x - a.x) * (c.z - a.z) - (c.x - a.x) * (b.z - a.z);
            if d.abs() < 1e-6 {
                continue;
            }
            let u = ((p.x - a.x) * (c.z - a.z) - (c.x - a.x) * (p.z - a.z)) / d;
            let v = ((b.x - a.x) * (p.z - a.z) - (p.x - a.x) * (b.z - a.z)) / d;
            if u < 0.0 || v < 0.0 || u + v > 1.0 {
                continue;
            }
            let y = a.y + u * (b.y - a.y) + v * (c.y - a.y);
            if y <= below && best.is_none_or(|h| y > h) {
                best = Some(y);
            }
        }
        best
    }

    /// The room whose bounding sphere's centre is nearest. [I]
    fn nearest_room(&self) -> usize {
        let d = |&r: &u32| {
            self.bof
                .get(r)
                .and_then(Object::node)
                .map_or(f32::MAX, |n| {
                    Vec3::new(n.sphere[0] - self.pos.x, 0.0, n.sphere[2] - self.pos.z).length()
                        / n.sphere[3]
                })
        };
        (0..self.rooms.len())
            .min_by(|&a, &b| d(&self.rooms[a]).total_cmp(&d(&self.rooms[b])))
            .unwrap_or(0)
    }

    fn tick(&mut self) {
        let axis = |a: bool, b: bool| f32::from(u8::from(a)) - f32::from(u8::from(b));
        let [up, down, left, right] = self.held;
        let (dx, dy) = std::mem::take(&mut self.drag);
        self.yaw += axis(right, left) * TURN + dx;
        self.pitch = (self.pitch + dy).clamp(-1.2, 1.2);
        let step = axis(up, down) * WALK;
        if step != 0.0 {
            let next = self.pos + Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos()) * step;
            // Walk only where there is floor, and climb at most a step.
            if let Some(y) = self.ground(next, self.pos.y - EYE + 30.0) {
                self.pos = Vec3::new(next.x, y + EYE, next.z);
                self.room = self.nearest_room();
            }
        }
        self.frames += 1;
    }

    pub fn step(&mut self, dt: f64) {
        self.due = (self.due + dt * RATE).min(8.0);
        while self.due >= 1.0 {
            self.due -= 1.0;
            self.tick();
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        let mut hold = None;
        for k in keys.chars() {
            let k = k.to_string();
            match k.as_str() {
                "^" => hold = Some("ArrowUp"),
                "<" => hold = Some("ArrowLeft"),
                ">" => hold = Some("ArrowRight"),
                _ => {
                    self.key(&k, true);
                    self.key(&k, false);
                }
            }
        }
        if let Some(h) = hold {
            self.key(h, true);
        }
        for _ in 0..(seconds * RATE).round() as usize {
            self.tick();
        }
        if let Some(h) = hold {
            self.key(h, false);
        }
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        let hold = match key {
            "ArrowUp" => Some(0),
            "ArrowDown" => Some(1),
            "ArrowLeft" => Some(2),
            "ArrowRight" => Some(3),
            _ => None,
        };
        if let Some(i) = hold {
            self.held[i] = down;
            return true;
        }
        if !down {
            return matches!(key, "w" | "W" | "t" | "T" | "1" | "2" | "3" | "4" | "5");
        }
        match key {
            "1" | "2" | "3" | "4" | "5" => self.warp(key.parse::<usize>().unwrap_or(1) - 1),
            "w" | "W" => self.wireframe = !self.wireframe,
            "t" | "T" => self.textures = !self.textures,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, py, pb)) = self.pointer
                && pb & 1 != 0
            {
                self.drag.0 += (x - px) as f32 * 3.0;
                self.drag.1 += (y - py) as f32 * 2.0;
            }
            self.pointer = Some((x, y, buttons));
        } else {
            self.pointer = None;
        }
    }

    pub fn wheel(&mut self, _amount: f64) {}

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "room {} at {:.0},{:.0},{:.0} yaw {:.2} tris {} frame {}",
            self.room + 1,
            self.pos.x,
            self.pos.y,
            self.pos.z,
            self.yaw,
            self.triangles,
            self.frames
        )
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let frame = Mat4::from_translation(self.pos)
            * Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_x(self.pitch);
        // 70 degrees, far 5000 (`0x4039b0`).
        let mut camera = Camera::new(frame);
        camera.fov = 1.221_730_5;
        camera.far = 5000.0;
        let mut items: Vec<Item> = Vec::new();
        for &r in &self.rooms[self.room..=self.room] {
            atb::walk(
                &self.bof,
                r,
                &View {
                    eye: camera.eye(),
                    ..View::default()
                },
                &mut items,
            );
        }
        let lights = [Light::ambient(Vec3::ONE)];
        let frame = Frame {
            camera,
            lights: &lights,
            fog: None,
            texture_image: &|_| None,
            extra: &[],
            pass: None,
            wireframe: self.wireframe,
            textures: self.textures,
            bilinear: true,
        };
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        self.triangles = render::draw(
            gpu,
            gl,
            &mut self.pictures,
            &self.bof,
            &items,
            &frame,
            aspect,
        );
    }
}
