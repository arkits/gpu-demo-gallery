//! ATB Flip: one model at a time, turned with the arrow keys. `atsflip.exe`
//! loads a `.3ds` or `.vq` through ATB's converters (`tdstoats.c`,
//! `vqtoats.c`); its set-up is `0x401aa0` and its frame `0x401580`. See
//! `docs/ENGINE.md`.
use atb::tds;
use atb::{Bof, Item, View};
use glam::{Mat4, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu, State, Surface, Vertex};
use crate::render::{self, Camera, Fog, Frame, Light, Pictures};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("1", "Anubis"),
    ("2", "AT-AT"),
    ("3", "Dragon"),
    ("4", "Ludwig"),
    ("5", "Skeleton"),
    ("6", "Martini"),
    ("7", "VW"),
    ("8", "Hind"),
    ("f", "Fog"),
    ("c", "Centre"),
    ("w", "Wireframe"),
    ("t", "Textures"),
];

/// The models the launchers name (`ANUBIS.BAT` and the rest), then the
/// helicopter `hpflip`'s `HIND.BAT` shows.
const MODELS: [&str; 8] = [
    "a_moves.3ds",
    "atat.3ds",
    "dragon.3ds",
    "ludwig.3ds",
    "skell.3ds",
    "martini.vq",
    "vw.vq",
    "hind24h.3ds",
];

pub const FILES: &[&str] = &[
    "a_moves.3ds",
    "atat.3ds",
    "dragon.3ds",
    "ludwig.3ds",
    "skell.3ds",
    "martini.vq",
    "vw.vq",
    "hind24h.3ds",
    "anubelt.3df",
    "anubfrnt.3df",
    "anucolr.3df",
    "anucuff.3df",
    "anuear.3df",
    "anupant.3df",
    "chrome1.3df",
    "drag-fin.3df",
    "dragbod2.3df",
    "dragbodt.3df",
    "house2_l.3df",
    "pat0003.3df",
    "ra.3df",
];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

/// The original steps once a refresh; the port at 60 Hz.
const RATE: f64 = 60.0;
/// Turn per frame per unit of input: 3 degrees (`0x401aa0`).
const TURN: f32 = 0.052_359_8;

struct Model {
    bof: Bof,
    root: u32,
    centre: Vec3,
    radius: f32,
}

pub struct Demo {
    models: Vec<Option<Model>>,
    current: usize,
    pictures: Vec<Pictures>,
    /// Angles about x and y (`0x45b03c`, `0x45b040`).
    pitch: f32,
    yaw: f32,
    /// Camera distance ahead of its start, and the zoom step.
    zoom: f32,
    turn: f32,
    fog: bool,
    wireframe: bool,
    textures: bool,
    held: [bool; 4],
    drag: (f32, f32, f32),
    pointer: Option<(f64, f64, u8)>,
    due: f64,
    frames: u64,
    triangles: usize,
}

fn picture(files: &Files, name: &str) -> Option<(atb::Rgba, bool)> {
    let t = atb::read_texture_3df(files.get(name).ok()?).ok()?;
    let base = t.base();
    let alpha = t.format.starts_with('a');
    Some((
        atb::Rgba {
            width: base.width,
            height: base.height,
            rgba: base.rgba.clone(),
        },
        alpha,
    ))
}

fn load(files: &Files, name: &str) -> Result<Model, Error> {
    let data = files.get(name)?;
    let (bof, root) = if name.ends_with(".vq") {
        let (chrome, _) = picture(files, "ra.3df").ok_or("missing ra.3df")?;
        tds::vq_to_bof(std::str::from_utf8(data)?, chrome)?
    } else {
        let scene = atb::read_mesh_3ds(data)?;
        tds::to_bof(&scene, &|n| picture(files, n))
    };
    let mut items = Vec::new();
    atb::walk(&bof, root, &View::default(), &mut items);
    let (centre, radius) = tds::bounds(&bof, &items);
    Ok(Model {
        bof,
        root,
        centre,
        radius,
    })
}

impl Demo {
    pub fn new(_gpu: &Gpu, _gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let models: Vec<Option<Model>> = MODELS.iter().map(|m| load(files, m).ok()).collect();
        if models.iter().all(Option::is_none) {
            return Err("no model could be read".into());
        }
        let current = models.iter().position(Option::is_some).unwrap_or(0);
        Ok(Self {
            pictures: (0..models.len()).map(|_| Pictures::default()).collect(),
            models,
            current,
            pitch: 0.0,
            yaw: 0.0,
            zoom: 0.0,
            turn: TURN,
            fog: false,
            wireframe: false,
            textures: true,
            held: [false; 4],
            drag: (0.0, 0.0, 0.0),
            pointer: None,
            due: 0.0,
            frames: 0,
            triangles: 0,
        })
    }

    fn tick(&mut self) {
        let axis = |a: bool, b: bool| f32::from(u8::from(a)) - f32::from(u8::from(b));
        let [left, right, up, down] = self.held;
        let (dx, dy, dz) = std::mem::take(&mut self.drag);
        // Flip mode (`0x401580`): input turns the model.
        self.pitch -= (axis(up, down) * self.turn) + dy;
        self.yaw -= (axis(right, left) * self.turn) + dx;
        self.zoom += dz;
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
        for k in keys.chars() {
            let k = k.to_string();
            self.key(&k, true);
            self.key(&k, false);
        }
        for _ in 0..(seconds * RATE).round() as usize {
            self.tick();
        }
    }

    fn centre(&mut self) {
        self.pitch = 0.0;
        self.yaw = 0.0;
        self.zoom = 0.0;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        let hold = match key {
            "ArrowLeft" => Some(0),
            "ArrowRight" => Some(1),
            "ArrowUp" => Some(2),
            "ArrowDown" => Some(3),
            _ => None,
        };
        if let Some(i) = hold {
            self.held[i] = down;
            return true;
        }
        if !down {
            return key.len() == 1 && "12345678fcwtadFCWTAD".contains(key);
        }
        match key {
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => {
                let i = key.parse::<usize>().unwrap_or(1) - 1;
                if self.models.get(i).is_some_and(Option::is_some) {
                    self.current = i;
                    self.centre();
                }
            }
            "f" | "F" => self.fog = !self.fog,
            "c" | "C" => self.centre(),
            "w" | "W" => self.wireframe = !self.wireframe,
            "t" | "T" => self.textures = !self.textures,
            // A and D change the turning speed by 0.1 (`0x4011f0`).
            "a" | "A" => self.turn += 0.1 * TURN,
            "d" | "D" => self.turn = (self.turn - 0.1 * TURN).max(0.0),
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, py, pb)) = self.pointer
                && pb & 1 != 0
            {
                self.drag.0 -= (x - px) as f32 * 4.0;
                self.drag.1 -= (y - py) as f32 * 4.0;
            }
            self.pointer = Some((x, y, buttons));
        } else {
            self.pointer = None;
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.drag.2 += amount as f32 * 0.25;
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "model {} pitch {:.2} yaw {:.2} fog {} tris {} frame {}",
            MODELS[self.current], self.pitch, self.yaw, self.fog, self.triangles, self.frames,
        )
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let Some(model) = &self.models[self.current] else {
            return;
        };
        // Camera three radii back, looking along +z; the zoom moves it
        // along its view by a radius a unit.
        let z = model.radius * (-3.0 + self.zoom);
        let camera = Camera::new(Mat4::from_translation(Vec3::new(0.0, 0.0, z)));
        // Row-vector `T(-c) * Rx * Ry` is column-vector `Ry * Rx * T(-c)`.
        let turn = Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_x(self.pitch)
            * Mat4::from_translation(-model.centre);
        let mut items: Vec<Item> = Vec::new();
        atb::walk(
            &model.bof,
            model.root,
            &View {
                eye: camera.eye(),
                ..View::default()
            },
            &mut items,
        );
        for i in &mut items {
            i.matrix = turn * i.matrix;
        }
        // One white light with an identity frame shines along +z, from the
        // viewer (`0x401aa0`).
        let lights = [Light::directional(Vec3::ONE, Vec3::Z)];
        let frame = Frame {
            camera,
            lights: &lights,
            fog: self.fog.then_some(Fog {
                near: model.radius * 5.0 * 0.2,
                far: model.radius * 5.0,
                color: [0.5; 3],
            }),
            texture_image: &|_| None,
            extra: &[],
            pass: None,
            wireframe: self.wireframe,
            textures: self.textures,
            bilinear: true,
        };
        // Clear to 0.1 grey, or the fog's 0.5 (`grBufferClear` in `0x401580`).
        let clear = if self.fog { 0.5 } else { 0.1 };
        gl.begin([clear, clear, clear, 1.0]);
        self.triangles = render::draw(
            gpu,
            gl,
            &mut self.pictures[self.current],
            &model.bof,
            &items,
            &frame,
            aspect,
        );
        reticle(gl, (w, h));
    }
}

/// The red corner brackets round the screen's centre (`0x401000`), placed
/// as on the original's 640 by 480 screen.
fn reticle(gl: &mut Gl, (w, h): (u32, u32)) {
    let (x0, y0, x1, y1) = (299.0, 219.0, 340.0, 260.0);
    let scale = (h.max(1) as f32 / 480.0).min(w.max(1) as f32 / 640.0);
    let at = |x: f32, y: f32| {
        let px = (x - 320.0) * scale / (w.max(1) as f32 * 0.5);
        let py = (240.0 - y) * scale / (h.max(1) as f32 * 0.5);
        Vertex::new(Vec4::new(px, py, 0.0, 1.0), [1.0, 0.0, 0.0, 1.0])
    };
    let (dx, dy) = ((x1 - x0) * 0.25, (y1 - y0) * 0.25);
    let lines = [
        at(x0, y1),
        at(x0 + dx, y1),
        at(x0, y1),
        at(x0, y1 - dy),
        at(x1, y0),
        at(x1 - dx, y0),
        at(x1, y0),
        at(x1, y0 + dy),
    ];
    gl.draw(State::new(0).lines(), &Surface::default(), &lines);
}
