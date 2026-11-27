//! Grand Bleu's animals, from their own `.BOF` files, each swimming with its
//! own animation, drawn as `flipper.exe` draws them: textures unlit under
//! the drifting water light, the rest lit. In `flipper.exe` Space moves the automatic camera to the
//! next animal; the port does the same, one animal at a time. The terrain,
//! plants, water light, schools and the animals' paths are not ported yet
//! (see `docs/ENGINE.md`).
use atb::bof::{Object, Texture};
use atb::{Bof, Image, Item, Material, View};
use glam::{Mat4, Vec3, Vec4};

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
pub const FILES: &[&str] = &[
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
    "light1.3df",
];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const RATE: f64 = 60.0;
/// Swimming frames a second: the files' `Seq` nodes leave timing to the
/// program, which sets the frame itself. [I]
const STROKE: f64 = 30.0;

/// What `flipper.exe` does to every loaded animal with water reflections
/// on, its default (`0x4039a0`, on each shape): a material with a texture
/// becomes preset 1, the texture alone, unlit, with a second pass of the
/// water light (`0x401330`: preset 0x10d, `light1.3df`, multiplied, its
/// coordinates 0.012 a unit of x and z). Untextured materials stay lit.
/// The water light's picture and texture are added after the file's own
/// objects; the pass is the first material past them.
fn water_light(bof: &mut Bof, water: Image) {
    bof.objects.push(Object::Image(water));
    let image = bof.objects.len() as u32;
    bof.objects.push(Object::Texture(Texture {
        handle: 0,
        images: vec![image],
    }));
    let pass = bof.objects.len() as u32 + 1;
    for o in &mut bof.objects {
        if let Object::Material(m) = o
            && m.textures()[0] != 0
            && m.tmu_mode()[0] != 0
        {
            let mut lit = Material::preset(1);
            lit.words[14] = m.words[14];
            lit.words[25] = pass;
            **m = lit;
        }
    }
}

/// Water-light texture units a world unit (`0x401330`, material `+0x44`).
const SCALE: f32 = 0.012;

/// The water-light pass, for `extra`.
fn water_pass(texture: u32) -> Material {
    let mut m = Material::preset(0x10d);
    m.words[14] = texture;
    m.words[17] = SCALE.to_bits();
    m
}

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
    /// The water light's phase.
    ripple: f32,
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
        let light = atb::read_texture_3df(files.get("light1.3df")?)?;
        let light = light.base();
        let mut texels = Vec::with_capacity(light.rgba.len());
        for px in light.rgba.chunks_exact(4) {
            texels.extend([px[3], px[0], px[1], px[2]]);
        }
        let water = Image {
            format: 16,
            width: light.width,
            height: light.height,
            levels: 1,
            file: None,
            data: texels,
            table: None,
        };
        let mut animals = Vec::new();
        for name in ANIMALS {
            let mut bof = Bof::parse(files.get(name)?)?;
            water_light(&mut bof, water.clone());
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
            ripple: 0.0,
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
            self.ripple += 0.01;
            if self.ripple > std::f32::consts::TAU {
                self.ripple = 0.0;
            }
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
        // The water light drifts by ten units each way: its offset is
        // ten times the cosine of a phase that gains 0.01 a frame
        // (`0x401860`, `0x401000`). Mode 0x7000 lays it flat from above,
        // 0.012 a unit (`0x4467a0`); the port's projector gives the same
        // from far overhead, where its division is constant.
        let off = self.ripple.cos() * 10.0;
        let height = 1.0e5;
        let projector = Light {
            kind: 4,
            color: Vec3::ONE,
            frame: Mat4::from_cols(
                Vec4::X,
                Vec4::Z,
                Vec4::NEG_Y,
                Vec4::new(-off, height, -off, 1.0),
            ),
            spread: (0.5 / (SCALE * height), 0.5 / (SCALE * height)),
        };
        // Ambient 0.3 and a (0.7, 0.7, 0.8) light turned a quarter about y,
        // so shining along +x (`0x401330`).
        let lights = [
            Light::ambient(Vec3::splat(0.3)),
            Light::directional(Vec3::new(0.7, 0.7, 0.8), Vec3::X),
            projector,
        ];
        // Fog (0.1, 0.1, 0.4) from 350 to 900 (`0x401020`, `0x401330`).
        let fog = Fog {
            near: 350.0,
            far: 900.0,
            color: [0.1, 0.1, 0.4],
        };
        let n = a.bof.objects.len() as u32;
        let extra = [water_pass(n)];
        let frame = Frame {
            camera,
            lights: &lights,
            fog: Some(fog),
            texture_image: &|_| None,
            extra: &extra,
            pass: None,
            wireframe: self.wireframe,
            textures: self.textures,
            bilinear: true,
        };
        // The clear colour is the fog's. [I]
        gl.begin([0.1, 0.1, 0.4, 1.0]);
        render::draw(gpu, gl, &mut a.pictures, &a.bof, &items, &frame, aspect);
    }
}
