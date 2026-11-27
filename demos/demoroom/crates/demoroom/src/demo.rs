//! Demoroom: a room lit by a projector you steer, drawn in two passes. The
//! scene is `demolite` in `demoroom.bof`; the frame (`0x401770`) and set-up
//! (`0x401e00`, `0x401ed0`) are described in `docs/ENGINE.md`.
use atb::bof::Object;
use atb::{Bof, Item, Material, View};
use glam::{Mat4, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu, State, Surface, TextureId, Vertex, WHITE};
use crate::image::Image;
use crate::render::{self, Camera, Fog, Frame, Light, Pictures};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Next light"),
    ("l", "Light on/off"),
    ("f", "Fog"),
    ("c", "Centre"),
    ("w", "Wireframe"),
    ("t", "Textures"),
    ("h", "Help"),
];

/// The demo reads only its scene file: every picture is inside it.
pub const FILES: &[&str] = &["demoroom.bof"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const TEXTURED: usize = 1;
/// The original advances once a refresh; the port steps at 60 Hz.
const RATE: f64 = 60.0;
/// The projector pictures `Space` steps through (`0x401fd0`), then the
/// twenty-frame film (`0x4020c0`).
const SLIDES: [&str; 9] = [
    "projblue.3df",
    "projblu2.3df",
    "projred.3df",
    "projred2.3df",
    "projgrn.3df",
    "projgrn2.3df",
    "projmlti.3df",
    "projwht.3df",
    "projgord.3df",
];
const FILM: usize = 20;
/// Turn per frame: 3 degrees times one half (`0x45c15c`, `0x45b02c`).
const TURN: f32 = 0.052_359_8 * 0.5;

#[derive(Clone, Copy)]
struct Pose {
    /// Angles about x, z and y, applied in that order.
    pitch: f32,
    roll: f32,
    yaw: f32,
    pos: Vec3,
}

impl Pose {
    fn matrix(&self) -> Mat4 {
        // Row-vector `Rx * Rz * Ry` is column-vector `Ry * Rz * Rx`, with
        // the same angles: ATB's rotation arrays read as columns are glam's.
        Mat4::from_translation(self.pos)
            * Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_z(self.roll)
            * Mat4::from_rotation_x(self.pitch)
    }
    fn forward(&self) -> Vec3 {
        self.matrix().z_axis.truncate()
    }
}

const CAMERA: Pose = Pose {
    pitch: 0.0,
    roll: 0.0,
    yaw: 0.0,
    pos: Vec3::new(4.0, 2.0, -500.0),
};
const LAMP: Pose = Pose {
    pitch: 0.0,
    roll: 0.0,
    yaw: 0.0,
    pos: Vec3::new(4.0, 2.0, 5.0),
};

pub struct Demo {
    bof: Bof,
    root: u32,
    pictures: Pictures,
    slides: Vec<u32>,
    film: Vec<u32>,
    help: TextureId,
    /// The projected-light pass (`0x402230`, preset 0x10b) and the lamp
    /// marker's material (`0x402340`, preset 0x6000).
    extra: [Material; 2],
    /// The texture the projector shows; its picture is swapped each frame.
    slide_texture: u32,
    slide: usize,
    film_frame: usize,
    camera: Pose,
    lamp: Pose,
    speed: f32,
    fog_far: f32,
    fog: bool,
    light: bool,
    wireframe: bool,
    textures: bool,
    show_help: bool,
    shift: bool,
    held: [bool; 8],
    drag: (f32, f32, f32),
    pointer: Option<(f64, f64, u8)>,
    due: f64,
    frames: u64,
    triangles: usize,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let bof = Bof::parse(files.get("demoroom.bof")?)?;
        let find = |name: &str| {
            bof.find(name)
                .ok_or_else(|| format!("demoroom.bof has no {name}"))
        };
        let root = find("demolite")?;
        let slides = SLIDES
            .iter()
            .map(|n| find(n))
            .collect::<Result<Vec<_>, _>>()?;
        let film = (0..FILM)
            .map(|i| find(&format!("{i:04}.3df")))
            .collect::<Result<Vec<_>, _>>()?;
        let help = match bof.image(find("helpscreen")?).map(|i| i.rgba()) {
            Some(Ok(p)) => gl.texture(
                gpu,
                &Image {
                    width: p.width,
                    height: p.height,
                    rgba: p.rgba,
                },
                crate::gl::CLAMP,
            ),
            _ => WHITE,
        };
        // The fog reaches the scene's bounding radius (`0x401ed0`).
        let fog_far = match bof.get(root).and_then(Object::node) {
            Some(n) => n.sphere[3],
            None => 2000.0,
        };
        let extra = [Material::preset(0x10b), {
            let mut m = Material::preset(0x6000);
            m.words[1..4].copy_from_slice(&[1.0f32.to_bits(); 3]);
            m
        }];
        // The projector's texture is the pass material's; the port keeps one
        // past the file's objects and swaps its picture.
        let slide_texture = bof.objects.len() as u32 + 100;
        let mut extra = extra;
        extra[0].words[14] = slide_texture;
        Ok(Self {
            root,
            pictures: Pictures::default(),
            slides,
            film,
            help,
            extra,
            slide_texture,
            slide: 0,
            film_frame: 0,
            camera: CAMERA,
            lamp: LAMP,
            speed: fog_far * 0.04,
            fog_far,
            fog: false,
            light: true,
            wireframe: false,
            textures: true,
            show_help: false,
            shift: false,
            held: [false; 8],
            drag: (0.0, 0.0, 0.0),
            pointer: None,
            due: 0.0,
            frames: 0,
            triangles: 0,
            bof,
        })
    }

    /// One refresh of `0x401770`.
    fn tick(&mut self) {
        let axis = |a: bool, b: bool| f32::from(u8::from(a)) - f32::from(u8::from(b));
        let [left, right, up, down, cl, cr, cu, cd] = self.held;
        // Arrows turn the lamp; with Shift they move it along its beam.
        if self.shift {
            let f = self.lamp.forward();
            self.lamp.pos += f * axis(up, down) * self.speed * 0.1;
        } else {
            self.lamp.yaw += axis(right, left) * TURN;
            self.lamp.pitch += axis(down, up) * TURN;
        }
        // The keypad (here WASD and the mouse) turns or moves the camera.
        let (dx, dy, dz) = std::mem::take(&mut self.drag);
        let fwd = axis(cu, cd) + dz;
        if fwd != 0.0 {
            let f = self.camera.forward();
            self.camera.pos += f * fwd * self.speed;
        }
        self.camera.yaw += (axis(cr, cl) * TURN) + dx;
        self.camera.pitch += dy;
        if self.slide == SLIDES.len() {
            self.film_frame = (self.film_frame + 1) % FILM;
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
        let hold = match key {
            "ArrowLeft" => Some(0),
            "ArrowRight" => Some(1),
            "ArrowUp" => Some(2),
            "ArrowDown" => Some(3),
            "a" | "A" | "4" => Some(4),
            "d" | "D" | "6" => Some(5),
            "8" => Some(6),
            "2" => Some(7),
            _ => None,
        };
        if let Some(i) = hold {
            self.held[i] = down;
            return true;
        }
        if key == "Shift" {
            self.shift = down;
            return true;
        }
        if !down {
            return matches!(
                key,
                " " | "l" | "L" | "f" | "F" | "c" | "C" | "w" | "W" | "t" | "T" | "h" | "H" | "F1"
            );
        }
        match key {
            " " | "n" | "N" => self.slide = (self.slide + 1) % (SLIDES.len() + 1),
            "l" | "L" => self.light = !self.light,
            "f" | "F" => self.fog = !self.fog,
            "c" | "C" => {
                self.camera = CAMERA;
                self.lamp = LAMP;
            }
            "w" | "W" => self.wireframe = !self.wireframe,
            "t" | "T" => self.textures = !self.textures,
            "h" | "H" | "F1" => self.show_help = !self.show_help,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, py, pb)) = self.pointer
                && pb & 1 != 0
            {
                self.drag.0 += (x - px) as f32 * 2.5;
                self.drag.1 += (y - py) as f32 * 2.0;
            }
            self.pointer = Some((x, y, buttons));
        } else {
            self.pointer = None;
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.drag.2 += amount as f32 * 2.0;
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "slide {} light {} fog {} camera {:.0},{:.0},{:.0} tris {} frame {}",
            self.slide,
            self.light,
            self.fog,
            self.camera.pos.x,
            self.camera.pos.y,
            self.camera.pos.z,
            self.triangles,
            self.frames,
        )
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let camera = Camera::new(self.camera.matrix());
        let mut items: Vec<Item> = Vec::new();
        atb::walk(
            &self.bof,
            self.root,
            &View {
                eye: camera.eye(),
                ..View::default()
            },
            &mut items,
        );
        // The lamp: a projector of 100 degrees each way (`0x4022a0`).
        let spread = (1.745_33f32 * 0.5).tan();
        let lamp = Light {
            kind: 4,
            color: Vec3::ONE,
            frame: self.lamp.matrix(),
            spread: (spread, spread),
        };
        let lights = [lamp];
        let image = if self.slide < SLIDES.len() {
            self.slides[self.slide]
        } else {
            self.film[self.film_frame]
        };
        let slide_texture = self.slide_texture;
        let n = self.bof.objects.len() as u32;
        let frame = Frame {
            camera,
            lights: &lights,
            fog: self.fog.then_some(Fog {
                near: self.fog_far * 0.6,
                far: self.fog_far,
                color: [0.0; 3],
            }),
            texture_image: &move |t| (t == slide_texture).then_some(image),
            extra: &self.extra,
            pass: self.light.then_some(n + 1),
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
        let vp = frame.camera.projection(aspect) * frame.camera.view();
        self.marker(gl, vp);
        if self.show_help {
            // The help picture (`helpscreen`), centred at its own size.
            let (hw, hh) = (512.0 / w.max(1) as f32, 400.0 / h.max(1) as f32);
            let c = [1.0; 4];
            let q = |x: f32, y: f32, u: f32, v: f32| {
                Vertex::new(glam::Vec4::new(x, y, 0.0, 1.0), c).uv(0, u, v)
            };
            gl.quad(
                State::new(TEXTURED),
                &Surface::textures(&[self.help]),
                [
                    q(-hw, hh, 0.0, 0.0),
                    q(hw, hh, 1.0, 0.0),
                    q(hw, -hh, 1.0, 1.0),
                    q(-hw, -hh, 0.0, 1.0),
                ],
            );
        }
    }

    /// The lamp's marker: a flat white arrow ten units long pointing along
    /// its beam (`0x421ef0`).
    fn marker(&self, gl: &mut Gl, vp: Mat4) {
        let s = 10.0;
        let p = |x: f32, z: f32| Vec3::new(x * s, 0.0, z * s);
        let tris = [
            [p(-0.3, 0.8), p(0.0, 1.0), p(0.3, 0.8)],
            [p(-0.1, 0.8), p(0.1, 0.8), p(-0.1, 0.0)],
            [p(-0.1, 0.0), p(0.1, 0.8), p(0.1, 0.0)],
        ];
        let m = self.lamp.matrix();
        let verts: Vec<Vertex> = tris
            .iter()
            .flatten()
            .map(|&v| Vertex::new(vp * m.transform_point3(v).extend(1.0), [1.0; 4]))
            .collect();
        gl.draw(
            State::new(0).depth(crate::gl::Depth::LessEqual(true)),
            &Surface::default(),
            &verts,
        );
    }
}
