//! Grand Bleu's animals, from their own `.BOF` files, each swimming with its
//! own animation. In `flipper.exe` Space moves the automatic camera to the
//! next animal; the port does the same, one animal at a time. The terrain,
//! plants, water light, schools and the animals' paths are not ported yet
//! (see `docs/ENGINE.md`).
use atb::{Bof, Item, View};
use glam::Vec3;

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu};
use crate::render::{self, Camera, Fog, Frame, Light, Pictures, look_at};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Next animal"),
    ("p", "Freeze"),
    ("w", "Wireframe"),
    ("t", "Textures"),
];

/// The animals `flipper.exe` loads (`0x401ba0`), in its order.
const ANIMALS: [&str; 13] = [
    "dolph5.bof",
    "dolph4.bof",
    "shark.bof",
    "manta.bof",
    "turtle.bof",
    "tuna.bof",
    "salmon.bof",
    "galax.bof",
    "butter.bof",
    "ryellow.bof",
    "blangel1.bof",
    "duckie.bof",
    "galleon1.bof",
];
pub const FILES: &[&str] = &ANIMALS;
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const RATE: f64 = 60.0;
/// Swimming frames a second: the files' `Seq` nodes leave timing to the
/// program, which sets the frame itself. [I]
const STROKE: f64 = 30.0;

struct Animal {
    bof: Bof,
    root: u32,
    centre: Vec3,
    radius: f32,
    pictures: Pictures,
}

pub struct Demo {
    animals: Vec<Animal>,
    current: usize,
    time: f64,
    frozen: bool,
    wireframe: bool,
    textures: bool,
    yaw: f32,
    drag: f32,
    pointer: Option<(f64, f64, u8)>,
    due: f64,
    frames: u64,
}

impl Demo {
    pub fn new(_gpu: &Gpu, _gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let mut animals = Vec::new();
        for name in ANIMALS {
            let bof = Bof::parse(files.get(name)?)?;
            let root = bof
                .dict
                .first()
                .map(|&(_, id)| id)
                .ok_or("animal without a name")?;
            let sphere = bof
                .get(root)
                .and_then(atb::Object::node)
                .map_or([0.0, 0.0, 0.0, 1.0], |n| n.sphere);
            animals.push(Animal {
                bof,
                root,
                centre: Vec3::new(sphere[0], sphere[1], sphere[2]),
                radius: sphere[3].max(1.0),
                pictures: Pictures::default(),
            });
        }
        Ok(Self {
            animals,
            current: 0,
            time: 0.0,
            frozen: false,
            wireframe: false,
            textures: true,
            yaw: 0.6,
            drag: 0.0,
            pointer: None,
            due: 0.0,
            frames: 0,
        })
    }

    fn tick(&mut self) {
        if !self.frozen {
            self.time += 1.0 / RATE;
        }
        self.yaw += std::mem::take(&mut self.drag);
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

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return matches!(key, " " | "p" | "P" | "F8" | "w" | "W" | "t" | "T");
        }
        match key {
            " " => self.current = (self.current + 1) % self.animals.len(),
            // F8 froze the animals.
            "p" | "P" | "F8" => self.frozen = !self.frozen,
            "w" | "W" => self.wireframe = !self.wireframe,
            "t" | "T" => self.textures = !self.textures,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, _, pb)) = self.pointer
                && pb & 1 != 0
            {
                self.drag += (x - px) as f32 * 3.0;
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
            "animal {} t {:.2} frame {}",
            ANIMALS[self.current], self.time, self.frames
        )
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let a = &mut self.animals[self.current];
        // Beside and a little above the animal, two and a half radii off. [I]
        let eye = a.centre + Vec3::new(self.yaw.sin(), 0.35, self.yaw.cos()) * a.radius * 2.5;
        let mut camera = Camera::new(look_at(a.centre, eye, Vec3::Y));
        camera.far = a.radius * 40.0;
        let frame_of = (self.time * STROKE) as usize;
        let mut items: Vec<Item> = Vec::new();
        atb::walk(
            &a.bof,
            a.root,
            &View {
                eye,
                frame: &move |_| Some(frame_of),
                ..View::default()
            },
            &mut items,
        );
        let lights = [
            Light::ambient(Vec3::splat(0.5)),
            Light::directional(Vec3::new(0.9, 1.0, 1.0), Vec3::new(0.2, -1.0, 0.3)),
        ];
        let fog = Fog {
            near: a.radius * 3.0,
            far: a.radius * 12.0,
            color: [0.0, 0.12, 0.25],
        };
        let frame = Frame {
            camera,
            lights: &lights,
            fog: Some(fog),
            texture_image: &|_| None,
            extra: &[],
            pass: None,
            wireframe: self.wireframe,
            textures: self.textures,
            bilinear: true,
        };
        gl.begin([0.0, 0.12, 0.25, 1.0]);
        render::draw(gpu, gl, &mut a.pictures, &a.bof, &items, &frame, aspect);
    }
}
