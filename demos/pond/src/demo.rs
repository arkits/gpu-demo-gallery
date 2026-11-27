//! The pond (`NvWavesGL.exe`; addresses are in it).
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
pub use crate::frames::FILES;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, MeshId, State, Surface, TextureId, Vertex, from_opengl, perspective,
};
use crate::image::{self, Image};

/// Files that are parts of a program: name, address, length.
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &["pool.wav"];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["textured", "around", "basin", "below", "wires"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("x", "Ripple"),
    ("p", "Play the day"),
    (" ", "Stop"),
    (".", "Step"),
    ("w", "Wireframe"),
    ("h", "Help"),
];

const TEXTURED: usize = 0;
const AROUND: usize = 1;
const BASIN: usize = 2;
const BELOW: usize = 3;
const WIRES: usize = 4;

/// Photographs, and steps of a day: at night the first photograph stays (0x4096ff).
const FRAMES: usize = 117;
const DAY: usize = 140;
/// Points of the water to a side, from -1 to 1.
const N: usize = 81;
const CELL: f32 = 0.025;
/// How fast waves run, and their phase for each millisecond.
const SPEED: f32 = 0.025;
const PHASE: f32 = 0.01;
/// The eye is turned about a point this far below the water's middle.
const LIFT: f32 = -0.2;
const GROUND: f32 = -0.45;
/// One step is a frame of the original, which is taken as 60 a second.
const MS: f64 = 1000.0 / 60.0;

fn place(i: usize) -> f32 {
    i as f32 * CELL - 1.0
}

/// A ring that runs outward (0x408802, 0x408940).
struct Splash {
    start: f32,
    last: f32,
    at: Vec2,
    strength: f32,
    /// When it gets to the walls: +x, -x, +z, -z.
    walls: [f32; 4],
    end: f32,
    arrival: Vec<f32>,
}

impl Splash {
    fn new(start: f32, at: Vec2, strength: f32) -> Self {
        let far = [1.0 - at.x, at.x + 1.0, 1.0 - at.y, at.y + 1.0].map(|d| d.abs() / SPEED);
        let inside = [at.x < 1.0, at.x > -1.0, at.y < 1.0, at.y > -1.0];
        let arrival = (0..N * N).map(|i| {
            let p = Vec2::new(place(i % N), place(i / N));
            start + p.distance(at) / SPEED
        });
        Self {
            start,
            last: start,
            at,
            strength,
            walls: std::array::from_fn(|i| match inside[i] {
                true => start + far[i],
                false => f32::MAX,
            }),
            end: start + far[0].max(far[1]).hypot(far[2].max(far[3])),
            arrival: arrival.collect(),
        }
    }

    /// What it looks like from behind a wall.
    fn mirrored(&self, wall: usize) -> Self {
        let mut at = self.at;
        match wall {
            0 => at.x = 2.0 - at.x,
            1 => at.x = -2.0 - at.x,
            2 => at.y = 2.0 - at.y,
            _ => at.y = -2.0 - at.y,
        }
        Self::new(self.start, at, self.strength * 0.2)
    }
}

struct Water {
    /// Milliseconds.
    time: f64,
    splashes: Vec<Splash>,
    /// What of each point's motion goes with the cosine and the sine of the time.
    parts: Vec<[f32; 2]>,
    height: Vec<f32>,
    normal: Vec<[f32; 2]>,
    /// Normals from both neighbors instead of one.
    better: bool,
}

impl Water {
    fn new() -> Self {
        Self {
            time: 0.0,
            splashes: Vec::new(),
            parts: vec![[0.0; 2]; N * N],
            height: vec![0.0; N * N],
            normal: vec![[0.0; 2]; N * N],
            better: false,
        }
    }

    fn now(&self) -> f32 {
        self.time as f32 * PHASE
    }

    /// 0x410c0d, 0x408d58, 0x40964a.
    fn step(&mut self) {
        self.time += MS;
        let now = self.now();
        let mut i = 0;
        while i < self.splashes.len() {
            let s = &self.splashes[i];
            let fresh = |t: f32| t < now && s.last <= t;
            let count = self.splashes.len();
            let more =
                count < 25 && s.strength >= 0.001 || count * 2 < 25 && s.strength * 5.0 >= 0.001;
            let walls = (0..4).filter(|&w| more && fresh(s.walls[w]));
            let mirrored: Vec<Splash> = walls.map(|w| s.mirrored(w)).collect();
            for (part, &t) in self.parts.iter_mut().zip(&s.arrival) {
                if fresh(t) {
                    part[0] += s.strength * t.sin();
                    part[1] += s.strength * t.cos();
                }
            }
            self.splashes[i].last = now;
            self.splashes.extend(mirrored);
            i += 1;
        }
        self.splashes.retain(|s| s.end >= now);
        let (sin, cos) = now.sin_cos();
        for (h, part) in self.height.iter_mut().zip(&mut self.parts) {
            *h = sin * part[1] - cos * part[0];
            *part = part.map(|v| v * 0.95);
        }
        self.normals();
    }

    /// 0x4106fd.
    fn normals(&mut self) {
        let slope = |i: usize, along: usize, step: usize| {
            // Before the first there is none, and after the last.
            let (low, high) = match self.better {
                true => (along > 0, along < N - 1),
                false => (along > 0, along == 0),
            };
            let a = if low { i - step } else { i };
            let b = if high { i + step } else { i };
            let cells = f32::from(u8::from(low) + u8::from(high));
            (self.height[a] - self.height[b]) / (cells * CELL)
        };
        for i in 0..N * N {
            self.normal[i] = [slope(i, i % N, 1), slope(i, i / N, N)];
        }
    }

    /// How high the water is at a point from 0 to 1 (0x40ffd9).
    fn sample(&self, at: Vec2) -> f32 {
        let at = (at * N as f32).clamp(Vec2::ZERO, Vec2::splat(N as f32 - 1.0001));
        let (low, part) = (at.floor(), at - at.floor());
        let i = low.y as usize * N + low.x as usize;
        let h = &self.height;
        let a = h[i] * (1.0 - part.x) + h[i + 1] * part.x;
        let b = h[i + N] * (1.0 - part.x) + h[i + N + 1] * part.x;
        a * (1.0 - part.y) + b * part.y
    }
}

/// How far from `p` along `v` a line meets the basin's floor or walls (0x40fc63).
fn depth(p: Vec3, v: Vec3) -> f32 {
    let mut d = match v.y <= 0.0 {
        true => (p.y + 0.3535) / -v.y,
        false => f32::MAX,
    };
    for (side, along) in [(p.x, v.x), (-p.x, -v.x), (p.z, v.z), (-p.z, -v.z)] {
        let toward = -along - v.y;
        if toward >= 0.0 {
            d = d.min((p.y + 1.0 + side) / toward);
        }
    }
    d
}

/// Where the sun is at a moment of the day, 0 to 140 (0x411014).
fn sun(day: f32) -> Vec3 {
    use std::f32::consts::PI;
    const SET: f32 = FRAMES as f32 - 7.0;
    let angle = match day < SET {
        true => PI / SET * day,
        false => PI + PI / (DAY as f32 - SET) * (day - SET),
    };
    let (y, z) = angle.sin_cos();
    // The turns of 0x421bd1 by -0.5 and 0.5.
    let ((sb, cb), (sc, cc)) = ((-0.5f32).sin_cos(), 0.5f32.sin_cos());
    Vec3::new(y * sc - z * sb * cc, y * cc + z * sb * sc, z * cb)
}

/// Six pictures of a frame.
fn cube(dds: &[Vec<u8>], frame: usize) -> [Image; 6] {
    std::array::from_fn(|i| {
        // All were read at the start.
        image::read(FILES[frame * 6 + i], &dds[frame * 6 + i])
            .unwrap_or_else(|_| Image::solid([0; 4]))
    })
}

fn strip(v: &[Vertex]) -> Vec<Vertex> {
    let each = |i: usize| [v[i], v[i + 1], v[i + 2]];
    (0..v.len() - 2).flat_map(each).collect()
}

fn wired(v: &[Vertex]) -> Vec<Vertex> {
    let sides = |t: &[Vertex]| [t[0], t[1], t[1], t[2], t[2], t[0]];
    v.chunks_exact(3).flat_map(sides).collect()
}

fn vertex(p: Vec3) -> Vertex {
    Vertex::new(p.extend(1.0), [1.0; 4])
}

/// A bowl of 64 by 16 and its lid, with where in the cube map each corner looks (0x41bd20,
/// 0x40e289).
fn surroundings() -> Vec<Vertex> {
    let bowl = |i: usize, j: usize| {
        let (u, out) = (i as f32 / 64.0, j as f32 / 16.0);
        let turn = (1.0 - u) * 4.0;
        let side = turn.floor();
        let s = (turn - side) * 2.0 - 1.0;
        let r = out * 9.0 + 1.0;
        let [x, z] = [[r * s, r], [r, -r * s], [-r * s, -r], [-r, r * s]][side as usize % 4];
        let p = Vec3::new(x, 10.45 * out * out * out - 0.45, z);
        let look = p - Vec3::Y * (1.5 * (1.0 - out) + 0.2 * out);
        vertex(p).uvw(0, look.normalize().extend(0.0).to_array())
    };
    let lid = |i: usize, j: usize| {
        let p = Vec3::new(
            i as f32 / 16.0 * 20.0 - 10.0,
            10.0,
            j as f32 / 16.0 * 20.0 - 10.0,
        );
        vertex(p).uvw(0, p.normalize().extend(0.0).to_array())
    };
    let mut v = Vec::new();
    let mut grid = |across: usize, f: &dyn Fn(usize, usize) -> Vertex| {
        for (i, j) in (0..16).flat_map(|j| (0..across).map(move |i| (i, j))) {
            let c = [f(i, j), f(i + 1, j), f(i, j + 1), f(i + 1, j + 1)];
            v.extend([c[0], c[1], c[2], c[2], c[1], c[3]]);
        }
    };
    grid(64, &bowl);
    grid(16, &lid);
    v
}

/// 0x40e350.
fn basin() -> Vec<Vertex> {
    const RINGS: [(f32, f32); 4] = [(0.9, -0.1), (1.05, 0.05), (1.1, 0.05), (1.1, GROUND)];
    const STRIP: [usize; 30] = [
        0, 4, 1, 5, 2, 6, 3, 7, 0, 4, 4, 8, 5, 9, 6, 10, 7, 11, 4, 8, 8, 12, 9, 13, 10, 14, 11, 15,
        8, 12,
    ];
    let corner = |i: usize| {
        let (r, y) = RINGS[i / 4];
        let [x, z] = [[r, r], [r, -r], [-r, -r], [-r, r]][i % 4];
        let [u, v] = match i / 4 {
            3 => [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]][i % 4],
            _ => [z * 0.3125 + 0.5, 0.5 - x * 0.3125],
        };
        vertex(Vec3::new(x, y, z)).uv(0, u, v)
    };
    strip(&STRIP.map(corner))
}

/// What the basin hides from the sun, on the ground (0x40e350).
fn shadow(sun: Vec3) -> Vec<Vertex> {
    let (sx, sz) = (sun.x * -2.0, sun.z * -2.0);
    // Two sides of the basin, and which way each end reaches.
    let (ends, t) = match (sx > 0.0, sz > 0.0) {
        (true, true) => ([[-1.1, 1.1], [1.1, 1.1], [1.1, -1.1]], sz),
        (true, false) => ([[1.1, 1.1], [1.1, -1.1], [-1.1, -1.1]], sz),
        (false, true) => ([[-1.1, -1.1], [-1.1, 1.1], [1.1, 1.1]], sx),
        (false, false) => ([[1.1, -1.1], [-1.1, -1.1], [-1.1, 1.1]], sz),
    };
    let t = t * t / (sx * sx + sz * sz);
    let mut v = Vec::new();
    for (&[x, z], u) in ends.iter().zip([0.0, t, 1.0]) {
        v.push(vertex(Vec3::new(x, GROUND, z)).uv(0, u, 1.0));
        let z = z - 0.1f32.copysign(z);
        v.push(vertex(Vec3::new(sx + x, GROUND, sz + z)).uv(0, u, 0.0));
    }
    strip(&v)
}

pub struct Demo {
    faces: Vec<Vec<u8>>,
    /// Photographs that are on the GPU.
    cubes: [(usize, TextureId); 2],
    pool: TextureId,
    bumps: TextureId,
    dirt: TextureId,
    shadow: TextureId,
    logo: TextureId,
    help: TextureId,
    around: [MeshId; 2],
    basin: [MeshId; 2],
    corners: [MeshId; 2],
    water: Water,
    /// Frames of the original since the day began, `steps` to a photograph.
    count: usize,
    steps: usize,
    playing: bool,
    running: bool,
    single: bool,
    wires: bool,
    shows_help: bool,
    strength: f32,
    pitch: f32,
    yaw: f32,
    /// Radians a millisecond.
    turn: f32,
    distance: f32,
    zoom: [bool; 2],
    mouse: Option<(Vec2, Vec2)>,
    right: bool,
    aspect: f32,
    drops: u32,
    due: f64,
    cues: Vec<String>,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let mut faces = Vec::new();
        for n in &FILES[..FRAMES * 6] {
            faces.push(files.get(n)?.to_vec());
        }
        files.image(FILES[0])?;
        let mut texture = |n: &str| Ok::<_, Error>(gl.texture(gpu, &files.image(n)?, CLAMP));
        let (pool, dirt) = (texture("pool.dds")?, texture("dirt.dds")?);
        let (shadow, logo) = (texture("shadow.dds")?, texture("logo.dds")?);
        let help = texture("help.dds")?;
        let bumps = files.image("pool_bump.tga")?.flipped();
        let mut both = |v: Vec<Vertex>| [gl.mesh(gpu, &v), gl.mesh(gpu, &wired(&v))];
        let square = [0, 2, 1, 1, 2, 3].map(|k| vertex(Vec3::X * k as f32));
        let around = both(surroundings());
        let basin = both(basin());
        let corners = both(square.to_vec());
        let cubes = [60, 61].map(|f| (f, gl.cube(gpu, &cube(&faces, f))));
        Ok(Self {
            faces,
            cubes,
            pool,
            dirt,
            shadow,
            logo,
            help,
            around,
            basin,
            corners,
            bumps: gl.texture(gpu, &bumps, CLAMP),
            water: Water::new(),
            count: 300,
            steps: 5,
            playing: false,
            running: true,
            single: false,
            wires: false,
            shows_help: false,
            strength: 0.01,
            pitch: 0.1,
            yaw: 1.0,
            turn: 1e-4,
            distance: 1.5,
            zoom: [false; 2],
            mouse: None,
            right: false,
            aspect: 4.0 / 3.0,
            drops: 0,
            due: 0.0,
            cues: vec!["pool*".into()],
        })
    }

    pub fn step(&mut self, dt: f64) {
        self.due += dt * 1000.0;
        while self.due >= MS - 1e-6 {
            self.due -= MS;
            self.frame();
        }
    }

    /// 0x411014.
    fn frame(&mut self) {
        let ms = MS as f32;
        if self.running || self.single {
            self.count += usize::from(self.playing);
            self.water.step();
            self.single = false;
        }
        if self.mouse.is_none() {
            self.yaw = (self.yaw + self.turn * ms).rem_euclid(std::f32::consts::TAU);
        }
        let zoom = f32::from(self.zoom[0]) - f32::from(self.zoom[1]);
        self.zoom_by(zoom * 0.001 * ms);
    }

    fn zoom_by(&mut self, amount: f32) {
        self.distance = (self.distance - amount).clamp(1.35, 3.0);
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.step(1.0 / 60.0));
    }

    /// The day's hour: photograph, the next one, how much of it.
    fn hour(&self) -> (usize, usize, f32) {
        let frame = |i: usize| if i % DAY < FRAMES { i % DAY } else { 0 };
        let i = self.count / self.steps;
        let part = (self.count % self.steps) as f32 / self.steps as f32;
        (frame(i), frame(i + 1), part)
    }

    /// 0x40dba9.
    fn view(&self) -> Mat4 {
        let back = Mat4::from_translation(Vec3::NEG_Z * self.distance);
        let pitch = Mat4::from_rotation_x(self.pitch);
        let first = match self.pitch >= 0.0 {
            true => back * pitch,
            false => pitch * back,
        };
        first * Mat4::from_rotation_y(self.yaw) * Mat4::from_translation(Vec3::Y * LIFT)
    }

    fn eye(&self) -> Vec3 {
        let turn = Mat4::from_rotation_y(-self.yaw) * Mat4::from_rotation_x(-self.pitch);
        turn.transform_vector3(Vec3::Z * self.distance) - Vec3::Y * LIFT
    }

    /// One radian high.
    fn all(&self, near: f32, far: f32) -> Mat4 {
        let projection = perspective(1f32.to_degrees(), self.aspect, near, far);
        from_opengl(projection) * self.view()
    }

    fn drop_at(&mut self, at: Vec2) {
        self.water
            .splashes
            .push(Splash::new(self.water.now(), at, self.strength));
        self.drops += 1;
    }

    /// Where the line from the eye through a point of the window meets the water (0x411efb).
    fn pick(&self, at: Vec2) -> Vec2 {
        let back = self.all(0.1, 10.0).inverse();
        let [a, b] = [0.0, 1.0].map(|z| {
            let p = back * Vec4::new(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, z, 1.0);
            p.truncate() / p.w
        });
        let p = a - (b - a) * (a.y / (b.y - a.y));
        Vec2::new(p.x, p.z)
    }

    /// 0x411a77.
    pub fn key(&mut self, key: &str, down: bool) -> bool {
        match key {
            "ArrowUp" => self.zoom[0] = down,
            "ArrowDown" => self.zoom[1] = down,
            _ if !down => return false,
            _ => return self.press(&key.to_ascii_lowercase()),
        }
        true
    }

    fn press(&mut self, key: &str) -> bool {
        match key {
            "x" => {
                // Not in the original: somewhere, for lack of a right button.
                let at = Vec2::new(0.754_88, 0.569_84) * (self.drops + 1) as f32;
                self.drop_at((at.fract() - 0.5) * 1.6);
            }
            "p" => self.playing = !self.playing,
            " " => self.running = !self.running,
            "." => self.single = true,
            "w" => self.wires = !self.wires,
            "h" | "?" => self.shows_help = !self.shows_help,
            "n" => {
                self.water.better = !self.water.better;
                self.water.normals();
            }
            "+" | "=" => self.strength *= 1.25,
            "-" | "_" => self.strength *= 0.8,
            _ => match key.parse() {
                Ok(n @ 0..=9) => {
                    let steps = if n == 0 { 10 } else { n };
                    let day = (self.count as f32 / self.steps as f32) % DAY as f32;
                    self.count = (day * steps as f32) as usize;
                    self.steps = steps;
                }
                _ => return false,
            },
        }
        true
    }

    /// 0x412119, 0x4120b6. A press that stays in place is a ripple, too.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        let at = Vec2::new(x as f32, y as f32);
        if buttons & 2 != 0 && !self.right {
            self.drop_at(self.pick(at));
        }
        self.right = buttons & 2 != 0;
        match (self.mouse, buttons & 1 != 0) {
            (None, true) => {
                self.mouse = Some((at, at));
                self.turn = 0.0;
            }
            (Some((last, first)), true) => {
                // Pixels of the window it opened.
                let by = (at - last) * Vec2::new(1024.0, 768.0) * 0.01;
                let most = std::f32::consts::FRAC_PI_2;
                self.pitch = (self.pitch + by.y).clamp(-most, most);
                self.yaw += by.x;
                self.turn = by.x;
                self.mouse = Some((at, first));
            }
            (Some((_, first)), false) => {
                self.mouse = None;
                self.turn *= 0.01;
                if at.distance(first) < 0.004 {
                    let at = self.pick(at);
                    if at.abs().max_element() < 1.0 {
                        self.drop_at(at);
                    }
                }
            }
            (None, false) => {}
        }
    }

    /// Not in the original.
    pub fn wheel(&mut self, amount: f64) {
        self.zoom_by(amount as f32);
    }

    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }

    pub fn report(&self) -> String {
        let (frame, _, _) = self.hour();
        let moving = self.water.height.iter().fold(0.0f32, |m, h| m.max(h.abs()));
        format!(
            "photograph {frame}, {} drops, {} rings, waves {}, {}",
            self.drops,
            self.water.splashes.len(),
            if moving > 1e-5 { "on" } else { "none" },
            if self.running { "running" } else { "stopped" },
        )
    }

    /// The water's squares, each with what is at its four corners (0x40ffd9).
    fn squares(&self) -> Vec<Vertex> {
        let eye = self.eye();
        let w = &self.water;
        let points: Vec<[f32; 6]> = (0..N * N)
            .map(|i| {
                let (x, z) = (place(i % N), place(i / N));
                let [nx, nz] = w.normal[i];
                let edge = [i % N, i / N].iter().any(|&k| k == 0 || k == N - 1);
                let (mut light, mut hit) = (128.0 / 255.0, Vec2::new(x, z));
                if !edge {
                    let p = Vec3::new(x, w.height[i], z);
                    let v = (p - eye).normalize() - Vec3::new(nx, 1.0, nz) * 0.25;
                    let d = depth(p, v);
                    hit += Vec2::new(v.x, v.z) * d;
                    let high = w.sample(hit * 0.5 + 0.5);
                    light = (high * 7000.0 + 128.0).clamp(51.0, 255.0) / 255.0;
                }
                let uv = hit * 0.25 + 0.5;
                [w.height[i], nx, nz, light, uv.x, uv.y]
            })
            .collect();
        let square = |(j, i): (usize, usize)| {
            let at = j * N + i;
            let c = [at, at + 1, at + N, at + N + 1];
            let of = |k: usize| c.map(|p| points[p][k]);
            Vertex {
                position: of(0),
                color: of(1),
                uv: [of(2), of(3), of(4), of(5)],
            }
        };
        let cells = N - 1;
        (0..cells)
            .flat_map(|j| (0..cells).map(move |i| (j, i)))
            .map(square)
            .collect()
    }

    /// 0x40da4d.
    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        use wgpu::BlendFactor as B;
        self.aspect = w as f32 / h as f32;
        let (now, next, part) = self.hour();
        if self.cubes[0].0 != now && self.cubes[1].0 == now {
            self.cubes.swap(0, 1);
        }
        for (slot, frame) in self.cubes.iter_mut().zip([now, next]) {
            if slot.0 != frame {
                slot.0 = frame;
                gl.replace_cube(gpu, slot.1, &cube(&self.faces, frame));
            }
        }
        let sun = sun((self.count as f32 / self.steps as f32) % DAY as f32);
        let bright = sun.y.max(0.0);
        let eye = self.eye();
        let with = |mut s: Surface, all: Mat4| {
            s.params[..4].copy_from_slice(&all.to_cols_array_2d());
            s.params[4] = eye.extend(0.0).to_array();
            s.params[6] = sun.extend(bright).to_array();
            s
        };
        let near = (self.distance - 2.0).max(0.01);
        let all = self.all(near, 3.0 + self.distance);
        let far = self.all(0.01, 20.0);
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        let mesh = usize::from(self.wires);
        // Lines have one color [I].
        let state = |program: usize| match self.wires {
            true => State::new(WIRES).lines(),
            false => State::new(program),
        };
        let shown = |k: usize, all: Mat4| {
            let alpha = [1.0, part][k];
            let s = with(Surface::default().cube(self.cubes[k].1), all);
            s.param(5, [alpha, 0.0, 0.0, 0.0])
        };
        // Both photographs, one over the other.
        let twice = if part > 0.0 && !self.wires { 2 } else { 1 };
        let over = |k: usize, s: State| match k {
            0 => s,
            _ => s.blend(B::SrcAlpha, B::OneMinusSrcAlpha),
        };
        for k in 0..twice {
            let s = over(k, state(AROUND)).vertex("placed");
            gl.draw_mesh(s, &shown(k, far), self.around[mesh]);
        }
        let solid = Depth::LessEqual(true);
        let surface = with(Surface::textures(&[self.pool, self.bumps]), all);
        gl.draw_mesh(
            state(BASIN).vertex("placed").depth(solid),
            &surface,
            self.basin[mesh],
        );
        if sun.y > 0.1 && !self.wires {
            let s = State::new(TEXTURED)
                .vertex("placed")
                .depth(Depth::LessEqual(false));
            let s = s.blend(B::SrcAlpha, B::OneMinusSrcAlpha);
            gl.draw(
                s,
                &with(Surface::textures(&[self.shadow]), all),
                &shadow(sun),
            );
        }
        let squares = self.squares();
        for k in 0..twice {
            let s = over(k, state(AROUND)).vertex("water");
            let s = s.depth(if k == 0 {
                solid
            } else {
                Depth::LessEqual(false)
            });
            gl.draw_instances(s, &shown(k, all), self.corners[mesh], &squares);
        }
        if !self.wires {
            let s = State::new(BELOW)
                .vertex("water")
                .depth(Depth::LessEqual(false));
            let s = s.blend(B::One, B::SrcAlpha);
            let surface = with(Surface::textures(&[self.pool, self.dirt]), all);
            gl.draw_instances(s, &surface, self.corners[mesh], &squares);
        }
        self.overlays(gl);
    }

    /// 0x40f4b6.
    fn overlays(&self, gl: &mut Gl) {
        use wgpu::BlendFactor as B;
        let blended = State::new(TEXTURED).blend(B::SrcAlpha, B::OneMinusSrcAlpha);
        let mut panel = |state, t, x: [f32; 2], y: [f32; 2], at: [f32; 4]| {
            let v = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]].map(|[u, v]: [f32; 2]| {
                let px = at[0] + (at[2] - at[0]) * u;
                let py = at[1] + (at[3] - at[1]) * v;
                let cx = (2.0 * px - x[0] - x[1]) / (x[1] - x[0]);
                let cy = (2.0 * py - y[0] - y[1]) / (y[1] - y[0]);
                Vertex::new(Vec4::new(cx, cy, 0.0, 1.0), [1.0; 4]).uv(0, u, 1.0 - v)
            });
            gl.quad(state, &Surface::textures(&[t]), v);
        };
        let a = self.aspect;
        panel(
            blended,
            self.logo,
            [-1.95 * a, 1.05],
            [-0.2, 9.8],
            [0.0, 0.0, 1.0, 1.0],
        );
        if self.shows_help {
            let x = [0.5 - a * 0.5, 0.5 + a * 0.5];
            panel(blended, self.help, x, [0.0, 1.0], [0.25, 0.25, 0.75, 0.75]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_run_come_back_and_fade() {
        let mut w = Water::new();
        w.splashes.push(Splash::new(0.0, Vec2::ZERO, 0.01));
        (0..30).for_each(|_| w.step());
        // 5 of phase: 0.125 out, so not yet at half way.
        assert!(w.height[40 * N + 43].abs() > 1e-4);
        assert_eq!(w.height[40 * N + 60], 0.0);
        // The walls are 40 away. Four come back, and two from each of those.
        (30..250).for_each(|_| w.step());
        assert_eq!(w.splashes.len(), 13);
        (250..4000).for_each(|_| w.step());
        assert!(w.splashes.is_empty() && w.height.iter().all(|h| h.abs() < 1e-5));
    }

    #[test]
    fn normals_lean_away_from_what_rises() {
        let mut w = Water::new();
        for i in 0..N * N {
            w.height[i] = place(i % N) * 0.5 - place(i / N) * 0.25;
        }
        for better in [false, true] {
            w.better = better;
            w.normals();
            for n in &w.normal {
                assert!(
                    (n[0] + 0.5).abs() < 1e-4 && (n[1] - 0.25).abs() < 1e-4,
                    "{n:?}"
                );
            }
        }
    }

    #[test]
    fn lines_end_on_the_basin() {
        // Straight down, and toward +x to a wall that leans outward from below.
        assert!((depth(Vec3::ZERO, Vec3::NEG_Y) - 0.3535).abs() < 1e-6);
        let d = depth(Vec3::ZERO, Vec3::new(1.0, -0.1, 0.0));
        assert!((1.0 - d * 1.0 - d * 0.1).abs() < 1e-5, "{d}");
    }

    #[test]
    fn the_sun_rises_and_sets() {
        assert!(sun(55.0).y > 0.8);
        for day in [112.0, 125.0, 139.0] {
            assert!(sun(day).y < 0.3, "{day}");
        }
        for day in 0..140 {
            assert!((sun(day as f32).length() - 1.0).abs() < 1e-5);
        }
    }
}
