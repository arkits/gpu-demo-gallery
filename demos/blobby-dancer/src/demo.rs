//! The dancer (`BlobbyDancer32.exe`; addresses are in it). See `docs/ENGINE.md`.
use glam::{Mat4, Vec2, Vec3};

use crate::Error;
use crate::blobs::{Blobs, Physics};
use crate::data::Files;
use crate::fp30;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, MeshId, REPEAT, State, Surface, TextureId, Vertex, from_opengl,
    perspective,
};
use crate::vab::{self, Node};

/// Files that are parts of a program: name, address, length.
pub const PARTS: &[(&str, usize, usize)] = &[];
/// Sounds made into FLAC for the web; the music stays as it is, in `FILES`.
pub const SOUNDS: &[&str] = &[];
pub const FILES: &[&str] = &[
    "dancer_11_to_28_103v.vab",
    "floor2.tga",
    "sunol_right.tga",
    "sunol_left.tga",
    "sunol_top.tga",
    "sunol_bottom.tga",
    "sunol_front.tga",
    "sunol_back.tga",
    "normal_01.tga",
    "normal_02.tga",
    "normal_03.tga",
    "normal_04.tga",
    "normal_05.tga",
    "normal_06.tga",
    "normal_07.tga",
    "normal_08.tga",
    "floor2.fp30.asm",
    "fresnel_00.fp30.asm",
    "fresnel_01.fp30.asm",
    "fresnel_02.fp30.asm",
    "fresnel_03.fp30.asm",
    "fresnel_04.fp30.asm",
    "fresnel_05.fp30.asm",
    "fresnel_06.fp30.asm",
    "fresnel_07.fp30.asm",
    "fresnel_08.fp30.asm",
    "fresnel_09.fp30.asm",
    "fresnel_10.fp30.asm",
    "fresnel_11.fp30.asm",
    "fresnel_12.fp30.asm",
    "fresnel_13.fp30.asm",
    "fresnel_14.fp30.asm",
    "fresnel_15.fp30.asm",
    "fresnel_16.fp30.asm",
    "fresnel_17.fp30.asm",
    "fresnel_18.fp30.asm",
    "fresnel_19.fp30.asm",
    "fresnel_20.fp30.asm",
    "music000.mp3",
];
const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &[
    "shown", "floor2", "look_00", "look_01", "look_02", "look_03", "look_04", "look_05", "look_06",
    "look_07", "look_08", "look_09", "look_10", "look_11", "look_12", "look_13", "look_14",
    "look_15", "look_16", "look_17", "look_18", "look_19", "look_20",
];
pub const BUTTONS: &[(&str, &str)] = &[
    ("n", "Next look"),
    ("c", "Next move"),
    ("r", "Turn"),
    (" ", "Pause"),
];

const SHOWN: usize = 0;
const FLOOR: usize = 1;
const LOOK: usize = 2;
const LOOKS: usize = 21;
const NORMALS: usize = 8;

/// The program's frames: what turns and fades does so a frame (0x40c640).
const RATE: f64 = 60.0;
/// The floor and the mirror at it (0x409ec0).
const FLOOR_Y: f32 = -0.99;
const FLOOR_R: f32 = std::f32::consts::SQRT_2;
/// A clip eases in and out over this many milliseconds (0x4119e0).
const EASE: f32 = 1500.0;
const PAUSE: f32 = 650.0;
/// The move that blends into the next clip instead of breaking up (0x270d).
const MORPH: i32 = 12;
const NONE: i32 = -1;
/// Where the dancer stands (0x4119e0, before the tree).
const ROOT: Mat4 = Mat4::from_cols_array(&[
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -0.115, 0.0, 1.0,
]);

/// The shader: the port's programs and the demo's own, translated.
pub fn shader(files: &Files) -> Result<String, Error> {
    let mut s = SHADER.to_string();
    let floor = std::str::from_utf8(files.get("floor2.fp30.asm")?)?;
    s.push_str(&fp30::translate("floor2", floor)?);
    for k in 0..LOOKS {
        let listing = files.get(&format!("fresnel_{k:02}.fp30.asm"))?;
        s.push_str(&fp30::translate(
            &format!("look_{k:02}"),
            std::str::from_utf8(listing)?,
        )?);
    }
    Ok(s)
}

/// `rand` of the C library the program uses.
struct Rand(u32);

impl Rand {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.0 >> 16) & 0x7fff
    }
    fn unit(&mut self) -> f32 {
        self.next() as f32 / 32767.0
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    (3.0 - 2.0 * x) * x * x
}

/// The numbers each look's program gets (0x406350).
fn timefunc(look: usize, t: f32) -> [f32; 4] {
    let three = |a: f32, b: f32, c: f32| [a.sin(), b.sin(), c.sin()];
    let scaled =
        |v: [f32; 3], m: f32, add: f32, w: f32| [v[0] * m + add, v[1] * m + add, v[2] * m + add, w];
    let pow3 = |v: [f32; 3], w: f32| [3f32.powf(v[0]), 3f32.powf(v[1]), 3f32.powf(v[2]), w];
    match look {
        6 | 7 => scaled(three(1.72 * t, 1.31 * t, 1.56 * t), 0.4, 0.6, 1.0),
        9 => scaled(
            three(0.461 * t + 3.0, 0.349 * t + 5.0, 0.412 * t + 2.0),
            0.5,
            1.0,
            1.0,
        ),
        16 | 19 => pow3(
            three(0.2855 * t + 2.0, 0.2145 * t + 4.0, 0.1495 * t + 6.0),
            ((0.0838 * t + 5.0).sin() + 1.0) / 2.0,
        ),
        17 => scaled(
            three(0.571 * t + 2.0, 0.429 * t + 4.0, 0.299 * t + 6.0),
            0.85,
            1.15,
            1.0,
        ),
        18 => pow3(
            three(0.2855 * t + 2.0, 0.2145 * t + 4.0, 0.1495 * t + 6.0),
            1.0,
        ),
        20 => scaled(
            three(0.2305 * t + 3.0, 0.1745 * t + 5.0, 0.206 * t + 2.0),
            0.4,
            0.6,
            1.0,
        ),
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// The dance: which clip, since when, and what happens between clips (0x4119e0, 0x4139f0).
struct Dance {
    clip: usize,
    next: usize,
    /// Seconds the clip started at.
    start: f64,
    kind: i32,
    chosen: bool,
    /// `rand() % 100` with the move (app + 0xa0).
    a: u32,
}

/// What a frame of the dance asks of the rest: weights of the camera's pull and of the
/// wobble (app + 0xa8, + 0xac).
#[derive(Default)]
struct Pull {
    camera: f32,
    wobble: f32,
}

pub struct Demo {
    dancer: Node,
    lengths: Vec<f32>,
    blobs: Blobs,
    targets: Vec<Vec3>,
    physics: Physics,
    radius: f32,
    dance: Dance,
    rand: Rand,
    pull: Pull,
    /// Looks: now, next, the fade's start and end in seconds (+0x25c, +0x260).
    looks: [usize; 2],
    fade: (f64, f64),
    normal: usize,
    normals: [TextureId; NORMALS],
    sky: TextureId,
    floor_map: TextureId,
    floor: Vec<Vertex>,
    body: MeshId,
    scenes: Option<((u32, u32), [TextureId; 2])>,
    frame: usize,
    /// Seconds of the dance.
    now: f64,
    due: f64,
    running: bool,
    turning: bool,
    pitch: f32,
    yaw: f32,
    zoom: f32,
    mouse: Option<Vec2>,
    aspect: f32,
    triangles: usize,
    cues: Vec<String>,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let dancer = vab::read(files.get("dancer_11_to_28_103v.vab")?)?;
        let lengths = (0..dancer.clips())
            .map(|c| dancer.length(c))
            .collect::<Vec<_>>();
        if lengths.is_empty() {
            return Err("the dancer has no clips".into());
        }
        let face = |n: &str| files.image(&format!("sunol_{n}.tga"));
        let faces = [
            face("right")?,
            face("left")?,
            face("top")?,
            face("bottom")?,
            face("front")?,
            face("back")?,
        ];
        let sky = gl.cube(gpu, &faces);
        let mut normals = [0; NORMALS];
        for (k, n) in normals.iter_mut().enumerate() {
            *n = gl.texture(
                gpu,
                &files.image(&format!("normal_{:02}.tga", k + 1))?,
                REPEAT,
            );
        }
        let floor_map = gl.texture(gpu, &files.image("floor2.tga")?, CLAMP);
        let mut rand = Rand(1);
        let phases = [rand.unit(), rand.unit(), rand.unit()];
        let mut demo = Self {
            dancer,
            lengths,
            blobs: Blobs::new(phases),
            targets: Vec::new(),
            physics: Physics::default(),
            radius: 0.1,
            dance: Dance {
                clip: 0,
                next: 1,
                start: 0.0,
                kind: NONE,
                chosen: false,
                a: 0,
            },
            looks: [0, 1],
            fade: (0.0, 6.4),
            normal: 0,
            normals,
            sky,
            floor_map,
            floor: floor(),
            body: gl.mesh(gpu, &[Vertex::new(glam::Vec4::ZERO, [0.0; 4]); 3]),
            scenes: None,
            frame: 0,
            now: 0.0,
            due: 0.0,
            running: true,
            turning: true,
            pitch: 0.0,
            yaw: 0.0,
            zoom: 2.0,
            mouse: None,
            aspect: 4.0 / 3.0,
            triangles: 0,
            rand,
            pull: Pull::default(),
            cues: vec!["music000*".into()],
        };
        demo.dance.next = demo.pick_clip(0);
        demo.looks[1] = demo.pick_look(0);
        demo.fade.1 = demo.look_time();
        demo.pose();
        demo.blobs.reset(&demo.targets);
        Ok(demo)
    }

    fn pick_clip(&mut self, not: usize) -> usize {
        let n = self.lengths.len();
        let c = self.rand.next() as usize % n;
        if c == not { (c + 1) % n } else { c }
    }

    fn pick_look(&mut self, not: usize) -> usize {
        let c = self.rand.next() as usize % LOOKS;
        if c == not { (c + 1) % LOOKS } else { c }
    }

    /// When the next look is due (0x4072f0).
    fn look_time(&mut self) -> f64 {
        self.now + 6.4 + f64::from(self.rand.unit()) * 3.8
    }

    pub fn step(&mut self, dt: f64) {
        self.due += dt * RATE;
        while self.due >= 1.0 - 1e-6 {
            self.due -= 1.0;
            self.tick();
        }
    }

    /// One frame of the program.
    fn tick(&mut self) {
        if self.turning && self.mouse.is_none() {
            self.yaw -= 1f32.to_radians();
        }
        if !self.running {
            return;
        }
        self.now += 1.0 / RATE;
        self.frame += 1;
        if self.now >= self.fade.1 {
            self.looks[0] = self.looks[1];
            self.looks[1] = self.pick_look(self.looks[0]);
            self.normal = self.rand.next() as usize % NORMALS;
            self.fade = (self.now, self.look_time());
        }
        self.pose();
        let time = self.now as f32;
        self.blobs.step(
            &self.targets,
            &self.physics,
            (1.0 / RATE) as f32,
            time,
            self.pull.wobble,
        );
    }

    /// The clip's time in milliseconds, unclamped.
    fn clip_ms(&self) -> f32 {
        ((self.now - self.dance.start) * 1000.0) as f32
    }

    /// Where the blobs are to be this frame, and the forces on them (0x4119e0).
    fn pose(&mut self) {
        let raw = self.clip_ms();
        let d = &self.dance;
        let length = self.lengths[d.clip];
        // On to the next clip.
        let over = match d.kind {
            MORPH => raw > length,
            _ => raw > length + PAUSE,
        };
        if over {
            let morph = self.dance.kind == MORPH;
            self.dance.clip = self.dance.next;
            self.dance.next = self.pick_clip(self.dance.clip);
            self.dance.start = match morph {
                true => self.now - f64::from(EASE) / 1000.0 - 0.001,
                false => self.now + f64::from(PAUSE) / 1000.0,
            };
            self.dance.chosen = false;
            if morph {
                self.dance.kind = NONE;
            }
        }
        let d = &self.dance;
        let length = self.lengths[d.clip];
        let t = self.clip_ms().clamp(0.0, length);
        let extra = if d.kind == MORPH { 0.0 } else { PAUSE };
        let (e, p) = if t < EASE {
            let e = (EASE - t) / (extra + EASE);
            (e, 1.0 - e / 2.0)
        } else if t > length - EASE {
            let e = (t - (length - EASE)) / (extra + EASE);
            (e, e / 2.0)
        } else {
            if !d.chosen {
                let kind = match self.rand.next() % 100 < 68 {
                    true => (self.rand.next() % 12) as i32,
                    false => MORPH,
                };
                self.dance.kind = kind;
                self.dance.a = self.rand.next() % 100;
                self.dance.chosen = true;
            }
            (0.0, 0.0)
        };
        let e = smooth(e);
        let mut points = Vec::new();
        let root = ROOT;
        self.dancer.place(self.dance.clip, t, root, &mut points);
        self.physics = Physics::default();
        self.radius = 0.1;
        self.pull = Pull::default();
        let kind = if e > 0.0 { self.dance.kind } else { NONE };
        if kind != NONE {
            self.effect(kind, e, p, t, length, &mut points);
        }
        self.targets = points;
    }

    /// The moves between clips (0x4119e0's cases).
    fn effect(&mut self, kind: i32, e: f32, p: f32, t: f32, length: f32, points: &mut [Vec3]) {
        let a = self.dance.a;
        let time = self.now as f32;
        let n = points.len();
        let f = if p < 0.36 {
            (3.0 - p * 5.5556) * p * p * 7.716
        } else if p >= 0.8 {
            let x = 1.0 - (p - 0.8) * 5.0;
            x - x * (p - 0.8) * 5.0
        } else {
            1.0
        };
        let s = 0.35 * e + 0.65 * f;
        if kind != MORPH {
            self.pull = Pull {
                camera: 0.67 * s,
                wobble: 0.4 * s,
            };
        }
        let group = [2, 3, 4, 24][a as usize % 4];
        // Each group goes its own way.
        let wander = |i: usize| {
            let g = (i / group) as f32;
            let seeds = [9464.4, 7590.8, 1366.4];
            let speeds = [0.31, 0.47, 0.72];
            let w = |k: usize| {
                (0..3)
                    .map(|j| (time * speeds[j] + g * seeds[k] * (1.0 + j as f32 * 0.37)).sin())
                    .sum::<f32>()
            };
            Vec3::new(w(0) * 0.23, w(1) * 0.333, w(2) * 0.23)
        };
        let saved = self.physics.mass;
        match kind {
            0..=2 => {
                for (i, q) in points.iter_mut().enumerate() {
                    *q = *q * (1.0 - e) + wander(i) * e;
                }
            }
            3 => {
                for (i, q) in points.iter_mut().enumerate() {
                    let w = wander(i);
                    q.x = q.x * (1.0 - e) + w.x * e;
                    q.z = q.z * (1.0 - e) + w.z * e;
                    q.y = (q.y - 2.4 * e).max(-1.0);
                }
                self.radius *= 1.0 - 0.4 * e;
            }
            4 | 5 => {
                if p < 0.5 {
                    for q in points.iter_mut() {
                        q.y = 0.1 - 1.0;
                    }
                }
                self.physics.mass = if p >= 0.1 { 135.0 } else { 1.9 };
                self.physics.gravity_dir = 1;
                self.physics.gravity = (a % 3 + 1) as f32;
                if p > 0.6 {
                    let x = ((p - 0.6) * 2.5).min(1.0);
                    self.physics.mass = self.physics.mass * (1.0 - x) + saved * x;
                }
                self.pull.wobble *= 0.75;
            }
            6 => {
                self.physics.mass = if p < 0.6 {
                    400.0
                } else {
                    let x = 1.0 - (p - 0.6) * 2.5;
                    x * x * 10.0 + (1.0 - x * x) * saved
                };
            }
            7 | 8 => {
                self.physics.stiffness = 0.01;
                if p < 0.75 {
                    self.physics.mass = (a % 3 + 4) as f32;
                    self.physics.gravity_dir = 1;
                    self.physics.gravity = (a % 7 + 4) as f32;
                    for q in points.iter_mut() {
                        q.y = FLOOR_Y;
                    }
                } else {
                    self.physics = Physics::default();
                }
            }
            9..=11 => {
                for (i, q) in points.iter_mut().enumerate() {
                    let order = match kind {
                        9 => i,
                        10 => n - 1 - i,
                        _ => i * 37 % n.max(1),
                    } as f32
                        / n.max(1) as f32;
                    let angle = i as f32 * 2.4 + time * 2.0;
                    let r = 0.5 + 0.1 * (i as f32 * 1.7).sin();
                    let to = Vec3::new(angle.cos() * r, -0.9 + 1.8 * order, angle.sin() * r);
                    *q = *q * (1.0 - e) + to * e;
                }
            }
            MORPH => {
                // Toward the start of the next clip.
                if t > length - EASE {
                    let mut next = Vec::new();
                    let at = t - (length - EASE);
                    self.dancer.place(self.dance.next, at, ROOT, &mut next);
                    let w = smooth(p * 3.0 - 0.25);
                    for (q, to) in points.iter_mut().zip(next) {
                        *q = *q * (1.0 - w) + to * w;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * RATE).round() as usize).for_each(|_| self.tick());
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key.to_ascii_lowercase().as_str() {
            "n" => self.fade.1 = self.now,
            "c" => {
                // To where the clip begins to end.
                let length = self.lengths[self.dance.clip];
                let t = (length - EASE).max(0.0);
                if self.clip_ms() < t {
                    self.dance.start = self.now - f64::from(t) / 1000.0;
                }
            }
            "r" => self.turning = !self.turning,
            " " => self.running = !self.running,
            "arrowup" => self.zoom_by(0.1),
            "arrowdown" => self.zoom_by(-0.1),
            _ => return false,
        }
        true
    }

    fn zoom_by(&mut self, amount: f32) {
        self.zoom = (self.zoom - amount).clamp(0.5, 5.0);
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        let at = Vec2::new(x as f32, y as f32);
        match (self.mouse, buttons & 1 != 0) {
            (Some(last), true) => {
                let by = (at - last) * 3.0;
                self.yaw += by.x;
                self.pitch = (self.pitch + by.y).clamp(-1.4, 1.4);
                self.mouse = Some(at);
            }
            (None, true) => self.mouse = Some(at),
            (_, false) => self.mouse = None,
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.zoom_by(amount as f32);
    }

    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }

    pub fn report(&self) -> String {
        let kind = match self.dance.kind {
            NONE => "none".to_string(),
            MORPH => "morph".to_string(),
            k => k.to_string(),
        };
        format!(
            "clip {} ({:.0} ms of {:.0}), move {kind}, looks {} > {}, {} blobs, {} triangles{}",
            self.dance.clip,
            self.clip_ms().max(0.0),
            self.lengths[self.dance.clip],
            self.looks[0],
            self.looks[1],
            self.blobs.pos.len(),
            self.triangles,
            if self.running { "" } else { ", paused" },
        )
    }

    /// 0x4023d0.
    fn view(&self) -> Mat4 {
        let a = self.pull.camera;
        let distance = a * 2.3 + (1.0 - a) * self.zoom;
        Mat4::from_translation(Vec3::NEG_Z * distance)
            * Mat4::from_rotation_x(self.pitch)
            * Mat4::from_rotation_y(self.yaw)
            * Mat4::from_translation(Vec3::Y * 0.08)
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        use wgpu::BlendFactor as B;
        self.aspect = w as f32 / h as f32;
        let size = (w.max(1), h.max(1));
        if self.scenes.is_none_or(|s| s.0 != size) {
            let made = [0, 1].map(|_| gl.target(gpu, size, CLAMP));
            self.scenes = Some((size, made));
        }
        let (_, scenes) = self.scenes.expect("made");
        let (to, before) = (scenes[self.frame % 2], scenes[1 - self.frame % 2]);

        let tris = self.blobs.surface(self.radius, 0.8);
        self.triangles = tris.len() / 3;
        let body: Vec<Vertex> = tris
            .iter()
            .map(|[p, n]| Vertex::new(p.extend(1.0), [1.0; 4]).uvw(0, n.extend(0.0).to_array()))
            .collect();
        gl.replace_mesh(gpu, self.body, &body);

        let projection = from_opengl(perspective(60.0, self.aspect, 0.1, 10.0));
        let view = self.view();
        let mirror = Mat4::from_translation(Vec3::Y * FLOOR_Y)
            * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
            * Mat4::from_translation(Vec3::Y * -FLOOR_Y);
        let time = self.now as f32;
        let m = (self.now - self.fade.0) / (self.fade.1 - self.fade.0).max(1e-3);
        let mix = smooth(smooth(smooth(m as f32)));
        let placed = |s: Surface, model_view: Mat4| {
            let mut s = s;
            s.params[..4].copy_from_slice(&(projection * model_view).to_cols_array_2d());
            s.params[4..8].copy_from_slice(&model_view.to_cols_array_2d());
            s.params[fp30::RECT_SIZE] = [size.0 as f32, size.1 as f32, 0.0, 0.0];
            s.params[21] = [time; 4];
            s.params[27] = [time; 4];
            s
        };
        let look = |k: usize, amount: f32| {
            Surface::textures(&[self.normals[self.normal], before])
                .cube(self.sky)
                .param(20, timefunc(k, time))
                .param(22, [amount; 4])
        };

        gl.begin([0.0, 0.0, 0.0, 1.0]);
        gl.to(Some(to), Some([0.0, 0.0, 0.0, 1.0]));
        for (model_view, clip) in [(view * mirror, [1.0, FLOOR_R, 0.0, 0.0]), (view, [0.0; 4])] {
            let body = State::new(0).vertex("body").depth(Depth::LessEqual(true));
            let [now, next] = self.looks;
            let a = placed(look(now, 1.0 - mix), model_view).param(28, clip);
            gl.draw_mesh(
                State {
                    program: LOOK + now,
                    ..body
                },
                &a,
                self.body,
            );
            if mix > 0.001 {
                let b = placed(look(next, mix), model_view).param(28, clip);
                let blended = body.blend(B::SrcAlpha, B::OneMinusSrcAlpha);
                gl.draw_mesh(
                    State {
                        program: LOOK + next,
                        ..blended
                    },
                    &b,
                    self.body,
                );
            }
            if clip[0] > 0.0 {
                // The floor over the reflection (0x4067c0).
                let reflect =
                    (((time * 0.37).sin() * 0.3 + (time * 0.23).sin() * 0.7) * 0.05) + 0.5;
                let s = placed(Surface::textures(&[self.floor_map]), view)
                    .param(24, [1.0; 4])
                    .param(25, [reflect; 4])
                    .param(26, [0.5; 4]);
                let state = State::new(FLOOR)
                    .vertex("ground")
                    .depth(Depth::LessEqual(true))
                    .blend(B::One, B::SrcAlpha);
                gl.draw(state, &s, &self.floor);
            }
        }
        gl.to(None, None);
        let corner = |x: f32, y: f32| {
            Vertex::new(
                glam::Vec4::new(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0),
                [1.0; 4],
            )
            .uv(0, x, y)
        };
        let all = [
            corner(0.0, 0.0),
            corner(1.0, 0.0),
            corner(1.0, 1.0),
            corner(0.0, 1.0),
        ];
        gl.quad(State::new(SHOWN), &Surface::textures(&[to]), all);
    }
}

/// The floor: a disc of 128 pieces (0x4067c0).
fn floor() -> Vec<Vertex> {
    let at = |k: usize| {
        let a = k as f32 / 128.0 * std::f32::consts::TAU;
        Vec3::new(a.cos() * FLOOR_R, FLOOR_Y, a.sin() * FLOOR_R)
    };
    let v = |p: Vec3| Vertex::new(p.extend(1.0), [1.0; 4]).uvw(0, [0.0, 1.0, 0.0, 0.0]);
    (0..128)
        .flat_map(|k| [v(Vec3::Y * FLOOR_Y), v(at(k + 1)), v(at(k))])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rand_of_the_c_library() {
        let mut r = Rand(1);
        assert_eq!([r.next(), r.next(), r.next()], [41, 18467, 6334]);
    }

    #[test]
    fn looks_have_their_numbers() {
        assert_eq!(timefunc(0, 3.0), [0.0, 0.0, 0.0, 1.0]);
        let v = timefunc(18, 0.0);
        assert!((v[0] - 3f32.powf(2f32.sin())).abs() < 1e-5);
    }
}
