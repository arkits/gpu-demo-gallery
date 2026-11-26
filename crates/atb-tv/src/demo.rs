//! ATB-TV: a television on a turning stand, playing the `.3df` frames in
//! `movie/` on its screen. `atbtv.exe` builds the cylinder, base, cabinet
//! and screen in code (`0x401b90`, `0x401e20`, `0x402140`), turns them a
//! degree a frame and shows the next movie frame every frame (`0x402df0`).
//! See `docs/ENGINE.md`.
use atb::bof::{Node, Object, Shape, Strip, Texture, TriSet};
use atb::{Bof, Image, Item, Material, Rgba};
use glam::{Mat4, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu};
use crate::render::{self, Camera, Frame, Light, Pictures};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Spin"),
    ("a", "Movie"),
    ("c", "Movie on"),
    ("l", "Lighting"),
    ("m", "Flat"),
    ("w", "Wireframe"),
    ("t", "Textures"),
];

const PICTURES: [&str; 4] = ["cylinder.3df", "base.3df", "tvcabnt.3df", "tvscrn.3df"];
const FRAMES: usize = 73;

pub const FILES: &[&str] = &[
    "cylinder.3df",
    "base.3df",
    "tvcabnt.3df",
    "tvscrn.3df",
    "0000.3df",
    "0001.3df",
    "0002.3df",
    "0003.3df",
    "0004.3df",
    "0005.3df",
    "0006.3df",
    "0007.3df",
    "0008.3df",
    "0009.3df",
    "0010.3df",
    "0011.3df",
    "0012.3df",
    "0013.3df",
    "0014.3df",
    "0015.3df",
    "0016.3df",
    "0017.3df",
    "0018.3df",
    "0019.3df",
    "0020.3df",
    "0021.3df",
    "0022.3df",
    "0023.3df",
    "0024.3df",
    "0025.3df",
    "0026.3df",
    "0027.3df",
    "0028.3df",
    "0029.3df",
    "0030.3df",
    "0031.3df",
    "0032.3df",
    "0033.3df",
    "0034.3df",
    "0035.3df",
    "0036.3df",
    "0037.3df",
    "0038.3df",
    "0039.3df",
    "0040.3df",
    "0041.3df",
    "0042.3df",
    "0043.3df",
    "0044.3df",
    "0045.3df",
    "0046.3df",
    "0047.3df",
    "0048.3df",
    "0049.3df",
    "0050.3df",
    "0051.3df",
    "0052.3df",
    "0053.3df",
    "0054.3df",
    "0055.3df",
    "0056.3df",
    "0057.3df",
    "0058.3df",
    "0059.3df",
    "0060.3df",
    "0061.3df",
    "0062.3df",
    "0063.3df",
    "0064.3df",
    "0065.3df",
    "0066.3df",
    "0067.3df",
    "0068.3df",
    "0069.3df",
    "0070.3df",
    "0071.3df",
    "0072.3df",
];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const RATE: f64 = 60.0;
const DEGREE: f32 = 0.017_453_28;
/// Half the cylinder's height, which every part is placed by.
const H: f32 = std::f32::consts::FRAC_PI_2;

/// A vertex: position, normal, texture coordinates; color white.
fn v(p: [f32; 3], n: [f32; 3], uv: [f32; 2]) -> [f32; 16] {
    let mut v = [0.0; 16];
    v[..3].copy_from_slice(&p);
    v[3..6].copy_from_slice(&n);
    v[6..8].copy_from_slice(&uv);
    v[8..12].copy_from_slice(&[1.0; 4]);
    v
}

fn strip(vertices: Vec<[f32; 16]>, triangles: Vec<[u8; 3]>) -> TriSet {
    TriSet {
        header: [0; 6],
        strips: vec![Strip {
            flags: 0,
            vertices,
            triangles,
            images: Vec::new(),
        }],
    }
}

/// The cylinder: 32 sides, radius 1, from -H to H, the texture once round.
fn cylinder() -> TriSet {
    let n = 32;
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let (s, c) = (f * std::f32::consts::TAU).sin_cos();
        top.push(v([c, H, s], [c, 0.0, s], [-(1.0 - f), 0.0]));
        bottom.push(v([c, -H, s], [c, 0.0, s], [-(1.0 - f), 1.0]));
    }
    let b = (n + 1) as u8;
    let mut tris = Vec::new();
    for k in 0..n as u8 {
        tris.push([b + k, k, b + k + 1]);
        tris.push([k, k + 1, b + k + 1]);
    }
    top.extend(bottom);
    strip(top, tris)
}

/// The floor: 12 units square, the texture four times each way.
fn base() -> TriSet {
    let up = [0.0, 1.0, 0.0];
    strip(
        vec![
            v([-6.0, -H, -6.0], up, [4.0, 0.0]),
            v([6.0, -H, -6.0], up, [0.0, 0.0]),
            v([6.0, -H, 6.0], up, [0.0, 4.0]),
            v([-6.0, -H, 6.0], up, [4.0, 4.0]),
        ],
        vec![[1, 0, 2], [2, 0, 3]],
    )
}

/// The cabinet's three sides: four wide, three high, two deep.
fn cabinet() -> TriSet {
    let r = std::f32::consts::FRAC_1_SQRT_2;
    let (y0, y1, v0) = (H, H + 3.0, 0.3325);
    let corners = [
        ([-2.0, 1.0], [-1.0, 0.0, 0.0], 0.0),
        ([-2.0, -1.0], [-r, 0.0, -r], 0.25),
        ([2.0, -1.0], [r, 0.0, -r], 0.75),
        ([2.0, 1.0], [1.0, 0.0, 0.0], 1.0),
    ];
    let mut verts = Vec::new();
    for (y, t) in [(y0, v0), (y1, 0.0)] {
        for ([x, z], n, u) in corners {
            verts.push(v([x, y, z], n, [u, t]));
        }
    }
    strip(
        verts,
        vec![
            [0, 5, 1],
            [0, 4, 5],
            [1, 6, 2],
            [1, 5, 6],
            [2, 7, 3],
            [2, 6, 7],
        ],
    )
}

/// The screen: an arc of radius sqrt 8 from 45 to 135 degrees, in six
/// pieces, bulging out of the cabinet's open side.
fn screen() -> TriSet {
    let r = 8.0f32.sqrt();
    let mut bottom = Vec::new();
    let mut top = Vec::new();
    for i in 0..7 {
        let a = (i + 3) as f32 * 15.0 * DEGREE;
        let (s, c) = a.sin_cos();
        let u = 1.0 - (6 - i) as f32 / 6.0;
        let (x, z) = (r * c, r * s - 1.0);
        bottom.push(v([x, H, z], [c, 0.0, s], [u, 1.0]));
        top.push(v([x, H + 3.0, z], [c, 0.0, s], [u, 0.0]));
    }
    let mut tris = Vec::new();
    for k in 0..6u8 {
        tris.push([k, 7 + k + 1, k + 1]);
        tris.push([k, 7 + k, 7 + k + 1]);
    }
    bottom.extend(top);
    strip(bottom, tris)
}

fn rgba(files: &Files, name: &str) -> Result<Rgba, Error> {
    let t = atb::read_texture_3df(files.get(name)?)?;
    let b = t.base();
    Ok(Rgba {
        width: b.width,
        height: b.height,
        rgba: b.rgba.clone(),
    })
}

fn image(p: Rgba) -> Image {
    let mut data = Vec::with_capacity(p.rgba.len());
    for px in p.rgba.chunks_exact(4) {
        data.extend([px[3], px[0], px[1], px[2]]);
    }
    Image {
        format: 16,
        width: p.width,
        height: p.height,
        levels: 1,
        file: None,
        data,
        table: None,
    }
}

pub struct Demo {
    bof: Bof,
    shapes: [u32; 4],
    /// Texture ids of the four parts, and image ids of the movie frames.
    textures: [u32; 4],
    movie: Vec<u32>,
    pictures: Pictures,
    angle: f32,
    frame: usize,
    spin: bool,
    playing: bool,
    /// The part the movie plays on: 0 the screen, 1 the cylinder, 2 the base.
    target: usize,
    /// 0: ambient 0.25 and a 0.75 light; 1: shiny; 2: ambient only.
    lighting: usize,
    flat: bool,
    shine: u32,
    wireframe: bool,
    textured: bool,
    camera: Mat4,
    fov: f32,
    due: f64,
    frames: u64,
}

impl Demo {
    pub fn new(_gpu: &Gpu, _gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let mut objects: Vec<Object> = Vec::new();
        let mut add = |o: Object| {
            objects.push(o);
            objects.len() as u32
        };
        let mut textures = [0; 4];
        for (i, name) in PICTURES.iter().enumerate() {
            let picture = add(Object::Image(image(rgba(files, name)?)));
            textures[i] = add(Object::Texture(Texture {
                handle: 0,
                images: vec![picture],
            }));
        }
        let mut movie = Vec::new();
        for i in 0..FRAMES {
            if let Ok(p) = rgba(files, &format!("{i:04}.3df")) {
                movie.push(add(Object::Image(image(p))));
            }
        }
        if movie.is_empty() {
            return Err("no movie frames".into());
        }
        let geometry = [cylinder(), base(), cabinet(), screen()];
        let mut shapes = [0; 4];
        for (i, g) in geometry.into_iter().enumerate() {
            let set = add(Object::TriSet(g));
            let material = add(Object::Material(Box::new(Material::preset(0x5001))));
            shapes[i] = add(Object::Shape(Shape {
                node: Node {
                    flags: [0, u32::MAX],
                    name: None,
                    sphere: [0.0; 4],
                },
                parts: vec![(material, set)],
            }));
        }
        // The camera: back 14, turned 15 degrees down (`0x401480`).
        let camera = Mat4::from_rotation_x(15.0 * DEGREE)
            * Mat4::from_translation(Vec3::new(0.0, 0.0, -14.0));
        let mut demo = Self {
            bof: Bof {
                objects,
                dict: Vec::new(),
            },
            shapes,
            textures,
            movie,
            pictures: Pictures::default(),
            angle: std::f32::consts::TAU,
            frame: 0,
            spin: true,
            playing: true,
            target: 0,
            lighting: 0,
            flat: false,
            shine: 25,
            wireframe: false,
            textured: true,
            camera,
            fov: 60.0 * DEGREE,
            due: 0.0,
            frames: 0,
        };
        demo.materials();
        Ok(demo)
    }

    /// The materials (`0x401680`, `0x402fe0`): textured and lit, diffuse 0.8
    /// for the cylinder and 0.6 for the rest, the screen glowing 0.3, and in
    /// the shiny style a specular highlight, green on the cylinder.
    fn materials(&mut self) {
        let flags = if self.flat { 0x6000 } else { 0x5001 };
        for (i, &shape) in self.shapes.iter().enumerate() {
            let Some(Object::Shape(s)) = self.bof.get(shape) else {
                continue;
            };
            let id = s.parts[0].0;
            let diffuse = if i == 0 { 0.8 } else { 0.6 };
            let glow = if i == 3 { 0.3 } else { 0.0 };
            let mut m = Material::preset(flags).with_colors([glow; 3], [diffuse; 3]);
            m.words[14] = self.textures[i];
            // Texture coordinates wrap; mipmaps and bilinear on.
            m.words[47] = 0;
            m.words[49] = 0;
            if self.lighting == 1 {
                let s: [f32; 3] = if i == 0 { [0.5, 1.0, 0.5] } else { [1.0; 3] };
                for (w, v) in m.words[7..10].iter_mut().zip(s) {
                    *w = v.to_bits();
                }
                m.words[10] = self.shine;
            }
            self.bof.objects[id as usize - 1] = Object::Material(Box::new(m));
        }
    }

    fn tick(&mut self) {
        if self.spin {
            self.angle -= DEGREE;
            if self.angle < 1.0 {
                self.angle = std::f32::consts::TAU;
            }
        }
        if self.playing {
            self.frame = (self.frame + 1) % self.movie.len();
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
        if !down {
            return matches!(
                key,
                " " | "a"
                    | "A"
                    | "c"
                    | "C"
                    | "l"
                    | "L"
                    | "m"
                    | "M"
                    | "w"
                    | "W"
                    | "t"
                    | "T"
                    | "-"
                    | "="
                    | "ArrowUp"
                    | "ArrowDown"
                    | "ArrowLeft"
                    | "ArrowRight"
            );
        }
        // The keys of `0x401000`.
        match key {
            " " => self.spin = !self.spin,
            "a" | "A" => self.playing = !self.playing,
            "c" | "C" => self.target = (self.target + 1) % 3,
            "l" | "L" => {
                self.lighting = (self.lighting + 1) % 3;
                self.materials();
            }
            "m" | "M" => {
                self.flat = !self.flat;
                self.materials();
            }
            "-" => {
                self.shine = (self.shine - 1).max(1);
                self.materials();
            }
            "=" => {
                self.shine = (self.shine + 1).min(35);
                self.materials();
            }
            "w" | "W" => self.wireframe = !self.wireframe,
            "t" | "T" => self.textured = !self.textured,
            // Up and down move the camera half a unit; left and right widen
            // or narrow the view a degree.
            "ArrowUp" => self.camera *= Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5)),
            "ArrowDown" => self.camera *= Mat4::from_translation(Vec3::new(0.0, 0.0, -0.5)),
            "ArrowLeft" => self.fov = (self.fov + DEGREE).min(3.1),
            "ArrowRight" => self.fov = (self.fov - DEGREE).max(DEGREE),
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}

    pub fn wheel(&mut self, amount: f64) {
        self.camera *= Mat4::from_translation(Vec3::new(0.0, 0.0, amount as f32 * 0.5));
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "frame {} angle {:.2} lighting {} target {} tick {}",
            self.frame, self.angle, self.lighting, self.target, self.frames
        )
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let mut camera = Camera::new(self.camera);
        camera.fov = self.fov;
        let turn = Mat4::from_rotation_y(self.angle);
        let mut items: Vec<Item> = Vec::new();
        for &s in &self.shapes {
            atb::walk(&self.bof, s, &atb::View::default(), &mut items);
        }
        for i in &mut items {
            i.matrix = turn * i.matrix;
        }
        // Lights (`0x4015d0`): ambient, and one shining from (1, 0, -1)
        // towards (-1, 0.2, 1); the third style is ambient at full.
        let (ambient, key) = match self.lighting {
            2 => (1.0, 0.0),
            _ => (0.25, 0.75),
        };
        let lights = [
            Light::ambient(Vec3::splat(ambient)),
            Light::directional(Vec3::splat(key), Vec3::new(-2.0, 0.2, 2.0)),
        ];
        let target = [3, 0, 1][self.target];
        let texture = self.textures[target];
        let picture = self.movie[self.frame];
        let frame = Frame {
            camera,
            lights: &lights,
            fog: None,
            texture_image: &move |t| (t == texture).then_some(picture),
            extra: &[],
            pass: None,
            wireframe: self.wireframe,
            textures: self.textured,
            bilinear: true,
        };
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        render::draw(
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
