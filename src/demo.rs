//! The meadow (`grass.exe`; addresses are in it).
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::BlendFactor as B;
use wgpu::Face;

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, MeshId, REPEAT, Sampling, State, Surface, TextureId, Vertex, WHITE,
    from_opengl, look_at, perspective,
};
use crate::image::Image;
use crate::noise::{Noise, Random};

pub const FILES: &[&str] = &[
    "daisy.tga",
    "grass.tga",
    "ground.tga",
    "logo.tga",
    "road.tga",
    "water.tga",
    "rainbow.part",
    "order.part",
    "sky.part",
    "points.part",
    "normals.part",
    "faces.part",
];
/// Files that are parts of a program: name, address, length.
pub const PARTS: &[(&str, usize, usize)] = &[
    ("rainbow.part", 0x415030, 27 * 4),
    ("order.part", 0x4154c4, 256),
    ("sky.part", 0x4155e0, 18 * 4),
    ("points.part", 0x4159e0, 8743 * 12),
    ("normals.part", 0x42f3b4, 3860 * 12),
    ("faces.part", 0x454278, 5770 * 36),
];
pub const SOUNDS: &[&str] = &["bugs4m.wav"];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "field", "flower", "round", "clouds"];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Storm"),
    ("x", "Sky"),
    ("o", "Cloud shadows"),
    ("s", "Sun flare"),
    ("+", "More wind"),
    ("-", "Less wind"),
    ("v", "Fast"),
    ("w", "Wireframe"),
    ("r", "Reset"),
];

const PLAIN: usize = 0;
const TEXTURED: usize = 1;
const FIELD: usize = 2;
const FLOWER: usize = 3;
const ROUND: usize = 4;
const CLOUDS: usize = 5;

/// Blades along each side of the square around the eye.
const SIDE: i32 = 100;
const FLIES: usize = 300;
/// The fog of each sky, which is also what is behind it (0x41513c).
const FOGS: [[f32; 3]; 3] = [[0.8; 3], [0.0; 3], [1.0, 0.0, 0.0]];
/// Pixels the original's window has, for what is measured in them.
const WINDOW: f32 = 512.0;

fn direction(around: f32, up: f32) -> Vec3 {
    let ((sa, ca), (su, cu)) = (around.sin_cos(), up.sin_cos());
    Vec3::new(cu * ca, su, cu * sa)
}

/// One square of the ground, and what grows there (0x4029b0).
#[derive(Clone)]
struct Plot {
    at: [i32; 2],
    height: f32,
    blade: Option<Blade>,
}

#[derive(Clone)]
struct Blade {
    foot: Vec3,
    length: f32,
    /// Direction, its height, and how both change to the tip.
    turns: [f32; 4],
    width: f32,
    widen: f32,
    color: Vec3,
    /// Half the size of the flower.
    flower: Option<f32>,
    /// About the tip and the middle.
    tip: Vec3,
    middle: Vec3,
}

/// Lower where the sign stands (0x402910).
fn height(noise: &Noise, x: f32, z: f32) -> f32 {
    let inside = x > -20.0 && x < 20.0 && z > -20.0 && z < 7.0;
    noise.two(x * 0.13, z * 0.13) * if inside { 1.0 } else { 6.0 }
}

fn plot(noise: &Noise, at: [i32; 2]) -> Plot {
    let [x, z] = at.map(|v| v as f32 - SIDE as f32 * 0.5);
    let mut plot = Plot {
        at,
        height: height(noise, x, z),
        blade: None,
    };
    let x = x + noise.value(x, z, 0.0) * 0.5;
    let z = z + noise.value(x, z, 1.0) * 0.5;
    let y = height(noise, x, z) - 0.1;
    let sign = x > -20.0 && x < 20.0 && z > -25.0 && z < 7.0;
    // Nothing grows around the sign and under water.
    if sign || y < -0.4 {
        return plot;
    }
    let (wide_x, wide_z) = (x * 0.1, z * 0.1);
    let (fine_x, fine_z) = (x * 3.0, z * 3.0);
    let length = (noise.two(wide_x, wide_z + 7.0) * 20.0 + 10.0).max(1.0);
    let flower = length < 5.0;
    let (lean, rise, bend) = if flower {
        (10.0, 10.0, 30.0)
    } else {
        (60.0, 30.0, 60.0)
    };
    let turns = [
        noise.two(wide_x, wide_z) * lean + 270.0,
        noise.two(wide_x, wide_z + 3.0) * rise + 90.0,
        (noise.two(fine_x, fine_z + 13.0) + 1.0) * bend,
        (noise.two(fine_x, fine_z + 21.0) + 1.0) * bend,
    ]
    .map(f32::to_radians);
    let (thin, widen) = if flower {
        (0.01, length * 0.005)
    } else {
        (0.02, 0.0)
    };
    let shade = |k: f32| noise.value(x * 0.3, z * 0.3, k) * 0.1 + 0.9;
    let foot = Vec3::new(x, y, z);
    // 0x402780. Lengths are 20 parts of what they say.
    let reach = |part: f32| {
        let d = direction(turns[0] + turns[2] * part, turns[1] + turns[3] * part);
        foot + d * length * (2.0 * part)
    };
    plot.blade = Some(Blade {
        foot,
        length,
        turns,
        width: length * thin,
        widen,
        color: if flower {
            Vec3::new(0.6, 0.5, 0.5)
        } else {
            Vec3::new(shade(0.0), shade(3.0), shade(7.0))
        },
        flower: flower.then_some(length * 0.1),
        tip: reach(0.5),
        middle: reach(0.25),
    });
    plot
}

#[derive(Clone, Copy)]
struct Fly {
    at: Vec3,
    speed: Vec3,
    life: i32,
}

impl Fly {
    /// 0x401000.
    fn new(random: &mut Random, eye: Vec3) -> Self {
        let x = random.even() * 50.0 + eye.x;
        let y = random.part() * 10.0;
        Self {
            at: Vec3::new(x, y, random.even() * 50.0 + eye.z),
            speed: Vec3::ZERO,
            life: random.byte() as i32 + 100,
        }
    }
}

struct Flare {
    picture: TextureId,
    color: [f32; 3],
    /// From the middle of the window to the sun.
    along: f32,
    size: f32,
}

struct Pictures {
    daisy: TextureId,
    grass: TextureId,
    ground: [TextureId; 2],
    logo: TextureId,
    road: TextureId,
    water: TextureId,
    skies: [TextureId; 3],
    shadows: TextureId,
}

pub struct Demo {
    noise: Noise,
    pictures: Pictures,
    /// 20, 10 and 5 parts; and their edges.
    blades: [[MeshId; 2]; 3],
    sign: [MeshId; 2],
    land: MeshId,
    plots: Vec<Plot>,
    /// Corner of the square around the eye.
    base: Option<[i32; 2]>,
    land_base: Option<([i32; 2], bool)>,
    flies: Vec<Fly>,
    flares: Vec<Flare>,
    random: Random,
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    turn: Vec2,
    speed: f32,
    /// Degrees.
    sun: Vec2,
    sun_size: f32,
    sun_seen: bool,
    sun_turn: f32,
    wind: f32,
    /// What the keys add to the length, the width and the width at the tip.
    longer: f32,
    wider: f32,
    tip: f32,
    growing: bool,
    /// Of the gusts and of the clouds.
    time: f32,
    cloud_time: f32,
    /// What is left of the black at the start.
    dark: f32,
    /// Frames since anything was pressed.
    idle: u32,
    sky: usize,
    storm: bool,
    shadows: bool,
    flare: bool,
    show_sky: bool,
    show_grass: bool,
    fine_ground: bool,
    lit: bool,
    wireframe: bool,
    arrows: [bool; 4],
    pointer: Vec2,
    pressed_at: Vec2,
    buttons: u8,
    blades_seen: usize,
    cues: Vec<String>,
    due: f64,
}

/// The original turns the rows, so that the first is the top one (0x408db0).
fn flipped(i: &Image) -> Image {
    let row = i.width as usize * 4;
    Image {
        rgba: i.rgba.chunks_exact(row).rev().flatten().copied().collect(),
        ..i.clone()
    }
}

fn floats(d: &[u8]) -> Vec<f32> {
    d.as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

fn lines(v: &[Vertex]) -> Vec<Vertex> {
    let of = |t: &[Vertex]| [t[0], t[1], t[1], t[2], t[2], t[0]];
    v.chunks_exact(3).flat_map(of).collect()
}

/// The sign (0x403af0): corners name a point, a normal and what is not used.
fn sign(files: &Files) -> Result<Vec<Vertex>, Error> {
    let vectors = |n: &str| {
        let v = floats(files.get(n)?);
        Ok::<_, Error>(
            v.as_chunks::<3>()
                .0
                .iter()
                .map(|&v| Vec3::from(v))
                .collect::<Vec<_>>(),
        )
    };
    let (points, normals) = (vectors("points.part")?, vectors("normals.part")?);
    let faces = files.get("faces.part")?.as_chunks::<4>().0.iter();
    let faces: Vec<usize> = faces.map(|b| u32::from_le_bytes(*b) as usize).collect();
    let mut out = Vec::new();
    for face in faces.as_chunks::<9>().0 {
        for k in 0..3 {
            let point = points.get(face[k]).ok_or("sign")?;
            let normal = normals.get(face[k + 3]).ok_or("sign")?;
            let v = Vertex::new(point.extend(1.0), [1.0; 4]);
            out.push(v.uvw(1, normal.extend(0.0).to_array()));
        }
    }
    Ok(out)
}

fn strip(parts: usize) -> Vec<Vertex> {
    let v =
        |i: usize, side: f32| Vertex::new(Vec4::new(i as f32, side, parts as f32, 1.0), [1.0; 4]);
    let quad = |i: usize| {
        let [a, b, c, d] = [v(i, -1.0), v(i, 1.0), v(i + 1, 1.0), v(i + 1, -1.0)];
        [a, b, c, a, c, d]
    };
    (0..parts).flat_map(quad).collect()
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let noise = Noise::new(files.get("order.part")?)?;
        let sky = floats(files.get("sky.part")?);
        let rainbow = floats(files.get("rainbow.part")?);
        let mut load =
            |n: &str, s: Sampling| Ok::<_, Error>(gl.texture(gpu, &flipped(&files.image(n)?), s));
        let sharp = Sampling {
            anisotropy: 2,
            ..REPEAT
        };
        let ground = [load("ground.tga", REPEAT)?, load("ground.tga", sharp)?];
        let pictures = Pictures {
            daisy: load("daisy.tga", REPEAT)?,
            grass: load("grass.tga", REPEAT)?,
            ground,
            logo: load("logo.tga", CLAMP)?,
            road: load("road.tga", REPEAT)?,
            water: load("water.tga", REPEAT)?,
            skies: std::array::from_fn(|i| gl.texture(gpu, &noise.picture(i, &sky), REPEAT)),
            shadows: gl.texture(gpu, &noise.picture(3, &sky), REPEAT),
        };
        // 0x4022c0.
        let mut random = Random::new(0x3d331);
        let mut flare = |kind, size| gl.texture(gpu, &noise.flare(kind, size, &rainbow), CLAMP);
        let mut flares = vec![
            Flare {
                picture: flare(1, 256),
                color: [0.2; 3],
                along: 1.0,
                size: 100.0,
            },
            Flare {
                picture: flare(0, 256),
                color: [0.1; 3],
                along: -1.5,
                size: 300.0,
            },
        ];
        for i in 0..18 {
            let picture = flare((i + 3) % 5 + 3, 128);
            let size = random.even() * 20.0 + 40.0;
            let color = std::array::from_fn(|_| random.part() * 0.1);
            let along = i as f32 / 9.0 - 1.0;
            flares.push(Flare {
                picture,
                color,
                along,
                size,
            });
        }
        let blades = [20, 10, 5].map(|n| {
            let v = strip(n);
            [gl.mesh(gpu, &v), gl.mesh(gpu, &lines(&v))]
        });
        let sign = sign(files)?;
        let sign = [gl.mesh(gpu, &sign), gl.mesh(gpu, &lines(&sign))];
        let nowhere = Plot {
            at: [i32::MIN; 2],
            height: 0.0,
            blade: None,
        };
        let mut demo = Self {
            noise,
            pictures,
            blades,
            sign,
            land: gl.mesh(gpu, &[]),
            plots: vec![nowhere; (SIDE * SIDE) as usize],
            base: None,
            land_base: None,
            flies: Vec::new(),
            flares,
            random,
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            turn: Vec2::ZERO,
            speed: 0.0,
            sun: Vec2::ZERO,
            sun_size: 100.0,
            sun_seen: true,
            sun_turn: 0.0,
            wind: 0.2,
            longer: 0.0,
            wider: 0.0,
            tip: 0.0,
            growing: true,
            time: 0.0,
            cloud_time: 0.0,
            dark: 1.0,
            idle: 0,
            sky: 0,
            storm: false,
            shadows: false,
            flare: true,
            show_sky: true,
            show_grass: true,
            fine_ground: true,
            lit: true,
            wireframe: false,
            arrows: [false; 4],
            pointer: Vec2::ZERO,
            pressed_at: Vec2::ZERO,
            buttons: 0,
            blades_seen: 0,
            cues: vec!["bugs4m*".into()],
            due: 0.0,
        };
        demo.reset();
        let eye = demo.eye;
        demo.flies = (0..FLIES)
            .map(|_| Fly::new(&mut demo.random, eye))
            .collect();
        Ok(demo)
    }

    /// 0x405640.
    fn reset(&mut self) {
        self.eye = Vec3::new(24.0, 0.5, -38.0);
        (self.yaw, self.pitch, self.speed) = (2.1, 0.0, 0.0);
        self.turn = Vec2::ZERO;
        self.sun = Vec2::new(101.0, 20.0);
        (self.longer, self.wider, self.tip) = (-20.0, 0.0, 0.0);
        self.growing = true;
        self.cloud_time = 0.0;
        self.dark = 1.0;
        self.survey();
    }

    fn plot(&self, x: i32, z: i32) -> &Plot {
        &self.plots[(z.rem_euclid(SIDE) * SIDE + x.rem_euclid(SIDE)) as usize]
    }

    /// Plots of a square around the eye. Those that stay are kept (0x402dd0).
    fn survey(&mut self) {
        let base = [self.eye.x as i32, self.eye.z as i32];
        if self.base == Some(base) {
            return;
        }
        self.base = Some(base);
        for z in base[1]..base[1] + SIDE {
            for x in base[0]..base[0] + SIDE {
                let i = (z.rem_euclid(SIDE) * SIDE + x.rem_euclid(SIDE)) as usize;
                if self.plots[i].at != [x, z] {
                    self.plots[i] = plot(&self.noise, [x, z]);
                }
            }
        }
    }

    /// The original does all this once a frame, which is taken as 60 a second.
    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            self.tick();
        }
    }

    /// 0x404d20, 0x404f30.
    fn tick(&mut self) {
        self.idle += 1;
        self.time = (self.time + 0.02) % 10000.0;
        self.cloud_time = (self.cloud_time + self.wind * 0.01) % 10000.0;
        if self.growing {
            self.longer += 0.1;
            self.dark = (self.dark - 0.015).max(0.0);
            if self.longer >= 0.0 {
                self.growing = false;
            }
        }
        let [left, right, up, down] = self.arrows;
        self.turn.x += (f32::from(right) - f32::from(left)) * 0.00349;
        if up && self.speed < 1.0 {
            self.speed += 0.02;
        }
        if down && self.speed > 0.0 {
            self.speed -= 0.02;
        }
        let (ahead, back, look) = (
            self.buttons & 1 != 0,
            self.buttons & 2 != 0,
            self.buttons & 4 != 0,
        );
        if self.idle > 500 {
            // Wanders by itself.
            self.pitch = 0.0;
            self.yaw -= (self.cloud_time * 5.0).sin() / 300.0;
            if self.eye.y < 7.0 {
                self.eye.y += 0.02;
            }
            if self.speed < 0.2 {
                self.speed += 0.01;
            }
        } else if ahead {
            let d = (self.pointer - self.pressed_at) * WINDOW * 0.0001;
            self.yaw += d.x;
            self.pitch -= d.y;
            if self.speed < 0.2 {
                self.speed += 0.01;
            }
        } else if self.speed >= 0.01 && !up {
            self.speed -= 0.01;
        }
        if back {
            if self.speed > -0.2 {
                self.speed -= 0.01;
            }
        } else if self.speed <= -0.02 {
            self.speed += 0.01;
        }
        if look {
            self.speed = 0.0;
        }
        self.yaw += self.turn.x;
        self.pitch = (self.pitch + self.turn.y).clamp(-1.5, 1.5);
        self.turn *= 0.9;
        self.eye += direction(self.yaw, self.pitch) * self.speed;
        let least = height(&self.noise, self.eye.x, self.eye.z) + 1.0;
        self.eye.y = self.eye.y.max(least).min(15.0);
        self.survey();
        self.swarm();
        self.sun_turn += 0.5;
        self.sun_seen = self.sees_sun();
        let to = if self.sun_seen { 200.0 } else { 100.0 };
        self.sun_size += (to - self.sun_size).clamp(-50.0, 50.0);
    }

    /// 0x401170.
    fn swarm(&mut self) {
        let (noise, eye, wind) = (&self.noise, self.eye, self.wind);
        for f in &mut self.flies {
            let p = f.at * 0.1;
            let push = Vec3::new(
                noise.three(p) * wind * 0.05 - 0.001,
                noise.three(p + Vec3::Z * 7.0) * wind * 0.05,
                noise.three(p + Vec3::Z * 13.0) * wind * 0.05,
            );
            f.speed += push;
            f.at += f.speed;
            f.speed *= 0.95;
            f.life -= 1;
            let d = f.at - eye;
            let near = d.x.abs() <= 50.0 && d.z.abs() <= 50.0;
            if !near || !(0.0..=20.0).contains(&f.at.y) || f.life < 0 {
                *f = Fly::new(&mut self.random, eye);
            }
        }
    }

    fn light(&self) -> Vec3 {
        direction(self.sun.x.to_radians(), self.sun.y.to_radians())
    }

    /// Whether land is in the way. The original looks at the depth buffer.
    fn sees_sun(&self) -> bool {
        let light = self.light();
        (1..120).all(|i| {
            let p = self.eye + light * (i as f32 * 0.5);
            height(&self.noise, p.x, p.z) < p.y
        })
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.step(1.0 / 60.0));
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        let arrow = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"];
        if let Some(i) = arrow.iter().position(|k| *k == key) {
            self.arrows[i] = down;
            self.idle = 0;
            return true;
        }
        if !down {
            return false;
        }
        self.idle = 0;
        match key.to_ascii_lowercase().as_str() {
            " " => {
                self.storm = !self.storm;
                self.shadows = self.storm;
                self.wind = if self.storm { 0.8 } else { 0.2 };
                self.sun.y = if self.storm { 80.0 } else { 20.0 };
            }
            "a" => self.fine_ground = !self.fine_ground,
            "g" => self.show_grass = !self.show_grass,
            "k" => self.show_sky = !self.show_sky,
            "l" => self.lit = !self.lit,
            "o" => self.shadows = !self.shadows,
            "r" => self.reset(),
            "s" => self.flare = !self.flare,
            "v" => self.speed = 1.0,
            "w" => self.wireframe = !self.wireframe,
            "x" => self.sky = (self.sky + 1) % 3,
            "+" | "=" if self.wind < 1.0 => self.wind += 0.05,
            "-" if self.wind > 0.05 => self.wind -= 0.05,
            "]" => self.wider += 0.01,
            "[" => self.wider -= 0.01,
            "'" => self.tip += 0.01,
            ";" => self.tip = (self.tip - 0.01).max(0.0),
            ">" | "." if self.longer < 0.0 => self.longer += 0.1,
            "<" | "," => self.longer -= 0.1,
            _ => return false,
        }
        true
    }

    /// Left flies to where the pointer is moved, right back, middle looks, left and middle
    /// move the sun.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        let at = Vec2::new(x as f32, y as f32);
        let d = (at - self.pointer) * WINDOW;
        if buttons != 0 {
            self.idle = 0;
        }
        if buttons != self.buttons {
            self.pressed_at = at;
        } else if buttons == 5 {
            self.sun.x += d.x * 0.1;
            self.sun.y = (self.sun.y - d.y * 0.1).max(0.0);
        } else if buttons == 4 {
            self.turn += d * 0.0005;
        }
        (self.pointer, self.buttons) = (at, buttons);
    }

    /// Not in the original.
    pub fn wheel(&mut self, amount: f64) {
        self.idle = 0;
        self.speed = (self.speed + amount as f32 * 0.5).clamp(-1.0, 1.0);
    }

    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }

    pub fn report(&self) -> String {
        let sky = ["clouds", "plasma", "fire"][self.sky];
        let weather = if self.storm { "storm" } else { "calm" };
        format!(
            "{weather}, {sky}, {} blades, wind {:.2}, length {:.1}, eye {:.1}",
            self.blades_seen, self.wind, self.longer, self.eye
        )
    }

    /// The square of land as triangles (0x403c30).
    fn land(&self, base: [i32; 2]) -> Vec<Vertex> {
        let corner = |i: i32, j: i32| {
            let h = |i: i32, j: i32| {
                let keep = |k: i32| k.clamp(0, SIDE - 1);
                self.plot(base[0] + keep(i), base[1] + keep(j)).height
            };
            let half = SIDE as f32 * 0.5;
            let [x, z] = [(base[0] + i) as f32 - half, (base[1] + j) as f32 - half];
            let normal = Vec3::new(h(i - 1, j) - h(i, j), 1.0, h(i, j - 1) - h(i, j));
            Vertex::new(Vec4::new(x, h(i, j), z, 1.0), [1.0; 4])
                .uv(0, (x + half) * 0.2, (z + half) * 0.2)
                .uvw(1, normal.extend(0.0).to_array())
        };
        let n = SIDE as usize;
        let all: Vec<Vertex> = (0..SIDE * SIDE)
            .map(|k| corner(k % SIDE, k / SIDE))
            .collect();
        let mut out = Vec::with_capacity((n - 1) * (n - 1) * 6);
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = j * n + i;
                out.extend([a, a + n, a + 1, a + 1, a + n, a + n + 1].map(|k| all[k]));
            }
        }
        out
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let fog = FOGS[self.sky];
        let [r, g, b] = fog.map(f64::from);
        gl.begin([r, g, b, 1.0]);
        let aspect = w as f32 / h as f32;
        let ahead = direction(self.yaw, self.pitch);
        let view = look_at(self.eye, self.eye + ahead, Vec3::Y);
        let projection = from_opengl(perspective(60.0, aspect, 0.1, 100.0));
        let all = projection * view;
        let light = self.light();
        let part = |on: bool| if on { 1.0 } else { 0.0 };
        let matrix = |mut s: Surface, at: usize, m: &Mat4| {
            for (i, c) in m.to_cols_array_2d().into_iter().enumerate() {
                s = s.param(at + i, c);
            }
            s
        };
        let outdoors = |t: TextureId| {
            let s = Surface::textures(&[t, self.pictures.shadows]);
            matrix(s, 0, &all)
                .param(4, light.extend(self.cloud_time).to_array())
                .param(5, (-ahead).extend(part(self.shadows)).to_array())
                .param(6, self.eye.extend(part(self.lit)).to_array())
                .param(7, Vec3::from(fog).extend(1.0).to_array())
        };
        let wires = usize::from(self.wireframe);
        let shape = |s: State| State {
            lines: self.wireframe,
            ..s
        };
        if self.show_sky {
            self.draw_sky(gl, &all, fog);
        }
        // The sign (0x403f10).
        let solid = Depth::Less(true);
        if self.eye.x.abs() <= 100.0 && self.eye.z.abs() <= 100.0 {
            let place = Mat4::from_translation(Vec3::Y * -0.2)
                * Mat4::from_rotation_y(std::f32::consts::PI)
                * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
                * Mat4::from_scale(Vec3::splat(0.08));
            let s = matrix(Surface::textures(&[self.pictures.road]), 0, &(all * place));
            let s = matrix(s, 8, &(view * place));
            let state = State::new(TEXTURED).vertex("mirror").depth(solid);
            let state = if self.wireframe {
                state
            } else {
                state.cull(Face::Back)
            };
            gl.draw_mesh(shape(state), &s, self.sign[wires]);
        }
        let mut seen = 0;
        if self.show_grass {
            seen = self.draw_grass(gl, &all, ahead, light, &outdoors, &shape);
        }
        // Land and water (0x404420).
        let base = self.base.expect("surveyed");
        if self.land_base != Some((base, self.wireframe)) {
            let mut v = self.land(base);
            if self.wireframe {
                v = lines(&v);
            }
            gl.replace_mesh(gpu, self.land, &v);
        }
        let soil = self.pictures.ground[usize::from(self.fine_ground)];
        let state = State::new(FIELD).vertex("ground").depth(solid);
        gl.draw_mesh(shape(state), &outdoors(soil), self.land);
        let flow = self.cloud_time * 0.05;
        let up = if self.lit {
            (0.3 + light.y.max(0.0)).min(1.0)
        } else {
            1.0
        };
        let corner = |i: i32, j: i32| {
            let half = SIDE as f32 * 0.5;
            let at = Vec3::new(
                base[0] as f32 - half + (i * SIDE / 2) as f32,
                -0.5,
                base[1] as f32 - half + (j * SIDE / 2) as f32,
            );
            let u = 20.0 * (i as f32 * 0.5 + base[0] as f32 * 0.01 + flow);
            let v = 20.0 * (j as f32 * 0.5 + base[1] as f32 * 0.01 - flow);
            Vertex::new(all * at.extend(1.0), [up, up, up, 0.4])
                .uv(0, u, v)
                .uvw(1, at.extend(1.0).to_array())
                .uvw(2, [0.0; 4])
        };
        let mut water = Vec::new();
        for k in 0..4 {
            let (i, j) = (k % 2, k / 2);
            let [a, b, c, d] = [
                corner(i, j),
                corner(i, j + 1),
                corner(i + 1, j + 1),
                corner(i + 1, j),
            ];
            water.extend([a, b, c, a, c, d]);
        }
        let over = State::new(FIELD)
            .depth(solid)
            .blend(B::SrcAlpha, B::OneMinusSrcAlpha);
        gl.draw(over, &outdoors(self.pictures.water), &water);

        // Flies are points that get smaller (0x401100).
        let pixel = Vec2::new(2.0 / (WINDOW * aspect), 2.0 / WINDOW);
        let mut points = Vec::new();
        for f in &self.flies {
            let p = all * f.at.extend(1.0);
            if p.w < 0.1 {
                continue;
            }
            let size = (10.0 / (1.0 + (f.at - self.eye).length()).sqrt()).clamp(2.0, 10.0) * 0.5;
            let v = |x: f32, y: f32| {
                let off = Vec2::new(x, y) * pixel * size * p.w;
                let at = p + off.extend(0.0).extend(0.0);
                Vertex::new(at, [0.0, 0.0, 0.0, 1.0]).uv(0, x, y)
            };
            let [a, b, c, d] = [v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0)];
            points.extend([a, b, c, a, c, d]);
        }
        let state = State::new(ROUND)
            .depth(solid)
            .blend(B::SrcAlpha, B::OneMinusSrcAlpha);
        gl.draw(state, &Surface::default(), &points);
        self.draw_screen(gl, &all, light, aspect);
        self.land_base = Some((base, self.wireframe));
        self.blades_seen = seen;
    }

    /// 0x403e20, 0x402f90, 0x403080.
    fn draw_grass(
        &self,
        gl: &mut Gl,
        all: &Mat4,
        ahead: Vec3,
        light: Vec3,
        outdoors: &dyn Fn(TextureId) -> Surface,
        shape: &dyn Fn(State) -> State,
    ) -> usize {
        let mut near: [Vec<Vertex>; 3] = Default::default();
        let mut flowers = Vec::new();
        let mut seen = 0;
        let outside = |p: Vec3, room: f32| {
            let c = *all * p.extend(1.0);
            let w = c.w.abs() * room;
            u32::from(c.x < -w)
                | u32::from(c.x > w) << 1
                | u32::from(c.y < -w) << 2
                | u32::from(c.y > w) << 3
                | u32::from(c.w < 0.0) << 4
                | u32::from(c.w > 100.0) << 5
        };
        let base = self.base.expect("surveyed");
        for k in 0..SIDE * SIDE {
            let plot = self.plot(base[0] + k % SIDE, base[1] + k / SIDE);
            let Some(b) = &plot.blade else { continue };
            let room = if b.flower.is_some() { 1.5 } else { 1.0 };
            if outside(b.foot, room) & outside(b.tip, room) & outside(b.middle, room) != 0 {
                continue;
            }
            let length = b.length + self.longer;
            if length < 0.0 {
                continue;
            }
            let gust = |z: f32| {
                let p = Vec3::new(b.foot.x * 0.1, self.time, (b.foot.z + z) * 0.1);
                self.noise.three(p) * self.wind * b.length * 0.25
            };
            let far = b.foot.distance_squared(self.eye);
            let (kind, parts) = match far {
                d if d < 400.0 => (0, 20),
                d if d <= 900.0 => (1, 10),
                _ => (2, 5),
            };
            let part = 1.0 / parts as f32;
            let around = (b.turns[2] + gust(7.0)) * part;
            let up = (b.turns[3] + gust(0.0)) * part;
            let width = (b.width + self.wider).max(0.1);
            let widen = part * (b.widen + self.tip - width);
            let mut turn = Vec2::new(b.turns[0] + around * 0.5, b.turns[1] + up * 0.5);
            let length = length * part;
            near[kind].push(
                Vertex::new(b.foot.extend(width), b.color.extend(widen).to_array())
                    .uvw(0, [turn.x, turn.y, around, up])
                    .uvw(1, [length, 0.0, 0.0, 0.0]),
            );
            let Some(size) = b.flower else { continue };
            let mut at = b.foot;
            for _ in 0..parts {
                at += direction(turn.x, turn.y) * length;
                turn += Vec2::new(around, up);
            }
            let along = direction(turn.x, turn.y);
            let across = (-ahead).cross(along).normalize_or_zero();
            let out = along.cross(across);
            let shade = if self.lit {
                0.3 + light.dot(along).max(0.0)
            } else {
                1.0
            };
            let c = shade.min(1.0);
            let v = |p: Vec3, u: f32, t: f32| {
                Vertex::new(*all * p.extend(1.0), [c, c, c, 1.0])
                    .uv(0, u, t)
                    .uvw(1, p.extend(1.0).to_array())
                    .uvw(2, [0.0; 4])
            };
            let edge = |x: f32, y: f32| {
                let p = at + (across * x + out * y) * size - along * size * 0.2;
                v(p, x * 0.5 + 0.5, y * 0.5 + 0.5)
            };
            let middle = v(at + along * 0.01, 0.5, 0.5);
            let around = [
                edge(-1.0, -1.0),
                edge(1.0, -1.0),
                edge(1.0, 1.0),
                edge(-1.0, 1.0),
            ];
            for i in 0..4 {
                flowers.extend([middle, around[i], around[(i + 1) % 4]]);
            }
        }
        let surface = outdoors(self.pictures.grass);
        let state = State::new(FIELD).vertex("blade").depth(Depth::Less(true));
        for (kind, v) in near.iter().enumerate() {
            seen += v.len();
            let mesh = self.blades[kind][usize::from(self.wireframe)];
            gl.draw_instances(shape(state), &surface, mesh, v);
        }
        let state = State::new(FLOWER).depth(Depth::Less(true));
        gl.draw(state, &outdoors(self.pictures.daisy), &flowers);
        seen
    }

    /// A flat dome around the eye (0x408330). It fades to the fog at its foot.
    fn draw_sky(&self, gl: &mut Gl, all: &Mat4, fog: [f32; 3]) {
        const AROUND: usize = 20;
        const UP: usize = 10;
        let corner = |i: usize, j: usize, color: [f32; 4]| {
            let d = direction(
                i as f32 * std::f32::consts::TAU / AROUND as f32,
                j as f32 * std::f32::consts::FRAC_PI_2 / UP as f32,
            );
            let p = self.eye + Vec3::new(d.x * 100.0, (d.y * 100.0 - 20.0) * 0.2, d.z * 100.0);
            Vertex::new(*all * p.extend(1.0), color).uv(0, d.x, d.z)
        };
        let dome = |color: &dyn Fn(usize) -> [f32; 4]| {
            let mut out = Vec::new();
            for j in 0..UP {
                for i in 0..AROUND {
                    let v = |i, j| corner(i, j, color(j));
                    let [a, b, c, d] = [v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)];
                    out.extend([a, b, c, a, c, d]);
                }
            }
            out
        };
        let mut s = Surface::textures(&[self.pictures.skies[self.sky]]);
        for k in 0..4 {
            let t = (k + 1) as f32 * self.cloud_time;
            // 0x408610, 0x408650.
            let m = match self.sky {
                0 => [1.0, 0.0, t * 0.1, -t * 0.1],
                _ => {
                    let a = self.noise.cell(k, 0) * std::f32::consts::PI;
                    [a.cos(), a.sin(), t.cos() * 0.1, t.sin() * 0.1]
                }
            };
            s = s.param(4 + k as usize, m);
        }
        gl.draw(State::new(CLOUDS), &s, &dome(&|_| [1.0; 4]));
        let [r, g, b] = fog;
        let veil = dome(&|j| [r, g, b, 1.0 - j as f32 / UP as f32]);
        let state = State::new(PLAIN).blend(B::SrcAlpha, B::OneMinusSrcAlpha);
        gl.draw(state, &Surface::default(), &veil);
    }

    /// The flare (0x402040), the logo, and the black at the start.
    fn draw_screen(&self, gl: &mut Gl, all: &Mat4, light: Vec3, aspect: f32) {
        let sun = *all * (self.eye + light * 100.0).extend(1.0);
        let inside = sun.w > 0.0 && sun.x.abs() <= sun.w && sun.y.abs() <= sun.w;
        if self.flare && inside {
            let at = Vec2::new(sun.x, sun.y) / sun.w;
            let scale = Vec2::new(2.0 / (WINDOW * aspect), 2.0 / WINDOW);
            for (i, f) in self.flares.iter().enumerate() {
                if i > 0 && !self.sun_seen {
                    break;
                }
                let (size, turn) = match i {
                    0 => (self.sun_size, self.sun_turn.to_radians()),
                    _ => (f.size, 0.0),
                };
                let [r, g, b] = f.color;
                let v = |x: f32, y: f32| {
                    let p = at * f.along
                        + Vec2::from_angle(turn).rotate(Vec2::new(x, y)) * size * scale;
                    Vertex::new(p.extend(0.0).extend(1.0), [r, g, b, 1.0]).uv(
                        0,
                        x * 0.5 + 0.5,
                        y * 0.5 + 0.5,
                    )
                };
                let corners = [v(-1.0, -1.0), v(1.0, -1.0), v(1.0, 1.0), v(-1.0, 1.0)];
                let state = State::new(TEXTURED).blend(B::One, B::One);
                gl.quad(state, &Surface::textures(&[f.picture]), corners);
            }
        }
        let show = |x: [f32; 2], y: [f32; 2], color: [f32; 4]| {
            let v = |i: usize, j: usize| {
                Vertex::new(Vec4::new(x[i], y[j], 0.0, 1.0), color).uv(0, i as f32, j as f32)
            };
            [v(0, 0), v(1, 0), v(1, 1), v(0, 1)]
        };
        let over = |p| State::new(p).blend(B::SrcAlpha, B::OneMinusSrcAlpha);
        // Raised, so that buttons fit below.
        let x = 1.0 - (1.0 / aspect).min(0.6);
        let logo = show([x, 1.0], [-0.5, -0.75], [1.0; 4]);
        gl.quad(
            over(TEXTURED),
            &Surface::textures(&[self.pictures.logo]),
            logo,
        );
        if self.dark > 0.0 {
            let all = show([-1.0, 1.0], [-1.0, 1.0], [0.0, 0.0, 0.0, self.dark]);
            gl.quad(over(PLAIN), &Surface::textures(&[WHITE]), all);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plots_depend_only_on_their_place() {
        let order: Vec<u8> = (0..=255).rev().collect();
        let noise = Noise::new(&order).unwrap();
        let mut grown = 0;
        for x in -60..160 {
            let p = plot(&noise, [x, 37]);
            let Some(b) = p.blade else { continue };
            grown += 1;
            assert!(b.length >= 1.0 && b.foot.y >= -0.4);
            assert!(
                Vec2::new(b.foot.x, b.foot.z).distance(Vec2::new(x as f32 - 50.0, -13.0)) < 1.5
            );
        }
        assert!(grown > 20);
    }

    #[test]
    fn strips_have_a_quad_for_each_part() {
        for n in [5, 10, 20] {
            let v = strip(n);
            assert_eq!(v.len(), n * 6);
            assert_eq!(v.iter().map(|v| v.position[0] as usize).max(), Some(n));
        }
    }
}
