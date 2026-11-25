//! The trees (`grove000419.exe`; addresses are in it).
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu::BlendFactor as B;
use wgpu::Face;

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, MeshId, REPEAT, State, Surface, TextureId, Vertex, WHITE, from_opengl,
    perspective,
};
use crate::image::Image;

pub const FILES: &[&str] = &[
    "bark.png",
    "ground.png",
    "label_balance.png",
    "label_branch_size.png",
    "label_character.png",
    "label_fullness.png",
    "label_leaf_size.png",
    "label_spread.png",
    "label_twist.png",
    "leaf.png",
    "leaf_back.png",
    "money.png",
    "money_back.png",
    "nvidia.png",
    "sky_night.png",
];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "leaf"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("c", "New tree"),
    ("g", "Grove"),
    ("l", "Leaves"),
    ("4", "4 fireflies"),
    ("8", "8 fireflies"),
    ("f", "Free view"),
    ("u", "Sliders"),
    ("w", "Wireframe"),
];

const PLAIN: usize = 0;
const TEXTURED: usize = 1;
const LEAF: usize = 2;

/// Sides of a branch.
const SIDES: usize = 6;
const SKY: f32 = 45.0;
const SKY_HEIGHT: f32 = 20.0;
/// How far the ground is lit, in quads of 1 (0x404b50).
const GROUND: i32 = 10;
const BUMPS: usize = 10;
const STEP: f32 = 1.0 / 60.0;

/// Park and Miller's numbers, as the original's.
#[derive(Clone)]
struct Random(u64);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 = self.0 * 16807 % 2_147_483_647;
        (self.0 as f64 / 2_147_483_647.0) as f32
    }
}

/// What the sliders say: name, least, most, at first (0x40fee0). Fullness becomes the least
/// length of a branch.
const SLIDERS: [(&str, f32, f32, f32); 7] = [
    ("fullness", 0.0, 1.0, 0.4),
    ("leaf_size", 0.5, 5.0, 2.0),
    ("branch_size", 0.01, 0.5, 0.1),
    ("twist", 0.0, 180.0, 60.0),
    ("balance", 0.0, 1.0, 0.7),
    ("spread", 0.0, 1.0, 0.75),
    ("character", 0.0, 1.0, 0.7),
];

#[derive(Clone, Copy)]
struct Shape {
    depth: usize,
    shortest: f32,
    leaf: f32,
    branch: f32,
    twist: f32,
    balance: f32,
    spread: f32,
    character: f32,
}

impl Shape {
    fn new(v: &[f32; 7]) -> Self {
        Self {
            depth: 31,
            shortest: (1.0 - v[0]).powi(4) * 0.47 + 0.03,
            leaf: v[1],
            branch: v[2],
            twist: v[3],
            balance: v[4],
            spread: v[5],
            character: v[6],
        }
    }
}

/// The random numbers of a tree stay when its shape changes. Branches get theirs when they
/// are first grown (0x404590, 0x404630).
struct Seed {
    value: f32,
    parts: [Option<Box<Seed>>; 2],
}

impl Seed {
    fn new(random: &mut Random) -> Self {
        Self {
            value: random.next(),
            parts: [None, None],
        }
    }
    fn part(&mut self, i: usize, random: &mut Random) -> &mut Seed {
        self.parts[i].get_or_insert_with(|| Box::new(Seed::new(random)))
    }
}

/// Triangles of branches and of leaves.
#[derive(Default)]
struct Grown {
    branches: Vec<Vertex>,
    leaves: Vec<Vertex>,
}

type Ring = [Vertex; SIDES + 1];

fn corner(at: Vec3, normal: Vec3, uv: [f32; 2], color: [f32; 4]) -> Vertex {
    Vertex::new(at.extend(1.0), color)
        .uv(0, uv[0], uv[1])
        .uvw(1, normal.extend(0.0).to_array())
}

/// Around the z axis of `m`, `radius` wide at `height`.
fn ring(m: &Mat4, radius: f32, height: f32, v: f32) -> Ring {
    std::array::from_fn(|i| {
        let u = i as f32 / SIDES as f32;
        let (y, x) = (std::f32::consts::TAU * u).sin_cos();
        let at = m.transform_point3(Vec3::new(x * radius, y * radius, height));
        let normal = m.transform_vector3(Vec3::new(x, y, 0.0));
        corner(at, normal, [u, v], [1.0; 4])
    })
}

impl Grown {
    fn band(&mut self, top: &Ring, bottom: &Ring) {
        for i in 0..SIDES {
            let [a, b, c, d] = [top[i], bottom[i], top[i + 1], bottom[i + 1]];
            self.branches.extend([a, b, c, c, b, d]);
        }
    }

    /// 0x406600.
    fn leaf(&mut self, m: &Mat4, size: f32, random: &mut Random) {
        let normal = m.transform_vector3(Vec3::X);
        let at = [(-1.0, 0.0), (1.0, 0.0), (1.0, 2.0), (-1.0, 2.0)];
        let uv = [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        let v: [Vertex; 4] = std::array::from_fn(|i| {
            let color = std::array::from_fn(|c| match c {
                3 => 1.0,
                _ => (random.next() + 1.0) * 0.5,
            });
            let p = m.transform_point3(Vec3::new(0.0, at[i].0 * size, at[i].1 * size));
            corner(p, normal, uv[i], color)
        });
        self.leaves.extend([v[0], v[1], v[2], v[0], v[2], v[3]]);
    }

    /// One branch, and what grows from its end (0x401200). Returns its lower ring.
    #[allow(clippy::too_many_arguments)]
    fn branch(
        &mut self,
        s: &Shape,
        seed: &mut Seed,
        random: &mut Random,
        m: &Mat4,
        v: f32,
        twist: f32,
        length: f32,
        depth: usize,
    ) -> Ring {
        let twist = twist + s.twist;
        let r = s.branch;
        let parts = [1.0 - s.balance, s.balance];
        let sizes: [f32; 2] = std::array::from_fn(|i| {
            let chance = seed.part(i, random).value;
            let size = parts[i].sqrt() + (chance * 0.3 - 0.1) * s.character;
            size.clamp(0.0, 1.0)
        });
        let below = ring(m, r, r, v);
        let above = ring(m, sizes[0].max(sizes[1]) * r, 1.0 - r, v + 1.0 - 2.0 * r);
        self.band(&above, &below);
        let end = *m * Mat4::from_translation(Vec3::Z);
        let (sin, cos) = twist.to_radians().sin_cos();
        let axis = Vec3::new(sin, cos, 0.0);
        let angles = [
            s.spread * s.balance * 90.0,
            s.spread * (1.0 - s.balance) * -90.0,
        ];
        for i in 0..2 {
            let m = end
                * Mat4::from_axis_angle(axis, angles[i].to_radians())
                * Mat4::from_scale(Vec3::splat(sizes[i]));
            let length = sizes[i] * length;
            if depth + 1 >= s.depth || length < s.shortest {
                self.leaf(&m, s.leaf, random);
                continue;
            }
            let seed = seed.part(i, random);
            let next = self.branch(s, seed, random, &m, v + 1.0, twist, length, depth + 1);
            self.band(&next, &above);
        }
        below
    }
}

/// Leaf colors are random each time, but the same from one build to the next.
fn grow(s: &Shape, seed: &mut Seed, random: &mut Random) -> Grown {
    let mut g = Grown::default();
    let m = Mat4::IDENTITY;
    let below = g.branch(s, seed, random, &m, 0.0, 0.0, 1.0, 0);
    let foot = ring(&m, s.branch * 1.5, -0.5, -s.branch - 0.5);
    g.band(&below, &foot);
    g
}

#[derive(Clone)]
struct Firefly {
    at: Vec3,
    speed: Vec3,
    hue: f32,
}

/// Full brightness, 0.4 saturated (0x41a630).
fn hue(h: f32) -> [f32; 3] {
    let part = |shift: f32| {
        let t = ((h + shift).fract() * 6.0 - 3.0).abs() - 1.0;
        1.0 - 0.4 * (1.0 - t.clamp(0.0, 1.0))
    };
    [part(0.0), part(2.0 / 3.0), part(1.0 / 3.0)]
}

impl Firefly {
    fn new(random: &mut Random) -> Self {
        Self {
            at: Vec3::Z * 3.0,
            speed: Vec3::ZERO,
            hue: random.next(),
        }
    }

    /// 0x404740.
    fn step(&mut self, random: &mut Random) {
        self.hue += 0.01;
        if self.hue > 1.0 {
            self.hue -= 1.0;
        }
        let mut chance = || (random.next() * 2.0 - 1.0) * 0.08;
        let push = Vec3::new(chance(), chance(), chance());
        self.speed += (Vec3::Z * 4.0 - self.at) * 0.005 + push;
        self.speed *= 0.97;
        self.at += self.speed * 0.5;
        self.at.z = self.at.z.max(0.5);
    }
}

struct Tree {
    values: [f32; 7],
    seed: Seed,
    at: Vec2,
    /// Degrees.
    turned: f32,
    fireflies: Vec<Firefly>,
    leaves: usize,
    /// Branches, leaves, ground.
    meshes: Option<[MeshId; 3]>,
    changed: bool,
}

impl Tree {
    fn new(random: &mut Random, values: [f32; 7]) -> Self {
        Self {
            values,
            seed: Seed::new(random),
            at: Vec2::ZERO,
            turned: 0.0,
            fireflies: (0..8).map(|_| Firefly::new(random)).collect(),
            leaves: 0,
            meshes: None,
            changed: true,
        }
    }
    fn place(&self) -> Mat4 {
        Mat4::from_translation(self.at.extend(0.0))
            * Mat4::from_rotation_z(self.turned.to_radians())
    }
}

/// So that the first row is the bottom one.
fn upright(i: &Image) -> Image {
    let row = i.width as usize * 4;
    Image {
        rgba: i.rgba.chunks_exact(row).rev().flatten().copied().collect(),
        ..i.clone()
    }
}

fn lines(v: &[Vertex]) -> Vec<Vertex> {
    let of = |t: &[Vertex]| [t[0], t[1], t[1], t[2], t[2], t[0]];
    v.chunks_exact(3).flat_map(of).collect()
}

/// A flat disk lit as if it had bumps (0x404e00, 0x405810). It fades out from half way.
fn ground(bumps: &[Vec3], center: Vec2) -> Vec<Vertex> {
    let least = (center - GROUND as f32).to_array().map(|v| v as i32);
    let most = (center + GROUND as f32).to_array().map(|v| v as i32);
    let at = |i: i32, j: i32| {
        let wrap = |k: i32| k.rem_euclid(BUMPS as i32) as usize;
        let part = |k: i32, l: i32| ((k - l) as f32 / GROUND as f32 - 1.0).powi(2);
        let fade = 1.0 - (part(i, least[1]) + part(j, least[0])).sqrt().min(1.0);
        let c = fade.min(0.5);
        let repeat = BUMPS as f32 - 1.0;
        corner(
            Vec3::new(j as f32, i as f32, 0.0),
            bumps[wrap(i) * BUMPS + wrap(j)],
            [j as f32 / repeat, i as f32 / repeat],
            [c, c, c, 1.0],
        )
    };
    let mut out = Vec::new();
    for i in least[1]..most[1] {
        for j in least[0]..most[0] {
            let [a, b, c, d] = [at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)];
            out.extend([a, b, c, a, c, d]);
        }
    }
    out
}

fn bumps(random: &mut Random) -> Vec<Vec3> {
    let h: Vec<f32> = (0..BUMPS * BUMPS).map(|_| random.next() * 0.5).collect();
    let at = |i: usize, j: usize| h[i % BUMPS * BUMPS + j % BUMPS];
    let mut out = Vec::new();
    for i in 0..BUMPS {
        for j in 0..BUMPS {
            let x = at(i, j + 1) - at(i, j + BUMPS - 1);
            let y = at(i + 1, j) - at(i + BUMPS - 1, j);
            out.push(Vec3::new(-2.0 * x, -2.0 * y, 1.0).normalize());
        }
    }
    out
}

pub struct Demo {
    bark: TextureId,
    ground: TextureId,
    sky: TextureId,
    nvidia: TextureId,
    labels: Vec<TextureId>,
    /// Leaves and money, front and back.
    leaves: [[TextureId; 2]; 2],
    kind: usize,
    random: Random,
    bumps: Vec<Vec3>,
    trees: Vec<Tree>,
    /// Trees that are still to be planted.
    waiting: u32,
    fireflies: usize,
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    turn: Vec2,
    speed: f32,
    /// Left, right, up, down.
    keys: [bool; 4],
    pointer: Vec2,
    buttons: u8,
    /// The slider that is held.
    held: Option<usize>,
    free: bool,
    sliders: bool,
    wireframe: bool,
    frame: u64,
    aspect: f32,
    due: f64,
}

/// Units of the original's 480 high overlay, from the bottom left. Raised, so that buttons
/// fit below.
const PANEL: f32 = 100.0;
const PANEL_WIDTH: f32 = 150.0;

fn slider(i: usize) -> f32 {
    PANEL + 240.0 - 40.0 * i as f32
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let mut load = |n: &str, s| Ok::<_, Error>(gl.texture(gpu, &upright(&files.image(n)?), s));
        let mut random = Random(1);
        let bumps = bumps(&mut random);
        let mut labels = Vec::new();
        for s in SLIDERS {
            labels.push(load(&format!("label_{}.png", s.0), CLAMP)?);
        }
        let first = Tree::new(&mut random, SLIDERS.map(|s| s.3));
        Ok(Self {
            bark: load("bark.png", REPEAT)?,
            ground: load("ground.png", REPEAT)?,
            sky: load("sky_night.png", REPEAT)?,
            nvidia: load("nvidia.png", CLAMP)?,
            labels,
            leaves: [
                [load("leaf.png", CLAMP)?, load("leaf_back.png", CLAMP)?],
                [load("money.png", CLAMP)?, load("money_back.png", CLAMP)?],
            ],
            kind: 0,
            random,
            bumps,
            trees: vec![first],
            waiting: 0,
            fireflies: 4,
            eye: Vec3::new(0.0, 1.0, 7.0),
            yaw: 0.0,
            pitch: 17.0,
            turn: Vec2::ZERO,
            speed: 0.0,
            keys: [false; 4],
            pointer: Vec2::ZERO,
            buttons: 0,
            held: None,
            free: false,
            sliders: true,
            wireframe: false,
            frame: 0,
            aspect: 4.0 / 3.0,
            due: 0.0,
        })
    }

    /// The original does this once a frame, which is taken as 60 a second (0x411fa0).
    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.frame += 1;
        if self.waiting > 0 {
            self.plant();
            self.waiting -= 1;
        }
        for t in &mut self.trees {
            for f in &mut t.fireflies[..self.fireflies] {
                f.step(&mut self.random);
            }
        }
        self.speed *= 0.9;
        let (ahead, back) = (self.buttons & 1 != 0, self.buttons & 2 != 0);
        if (ahead || back) && self.held.is_none() {
            let part = |on, v| if on { v } else { 0.0 };
            self.speed += part(ahead, 0.1) + part(back, -0.1);
            self.turn = (self.pointer - 0.5) * Vec2::new(640.0, -480.0);
        } else {
            self.turn.y *= 0.8;
            if !self.free {
                // To the middle.
                let to = (-self.eye.x).atan2(self.eye.z).to_degrees() - self.yaw;
                self.turn.x = ((to + 180.0).rem_euclid(360.0) - 180.0) * 20.0;
            }
        }
        self.yaw += self.turn.x * STEP * 0.05;
        self.pitch = (self.pitch + self.turn.y * STEP * 0.05).clamp(-80.0, 80.0);
        let (sin, cos) = self.yaw.to_radians().sin_cos();
        let (up, level) = self.pitch.to_radians().sin_cos();
        self.eye += Vec3::new(level * sin, up, -level * cos) * self.speed * STEP;
        let [left, right, rise, sink] = self.keys.map(|k| if k { STEP } else { 0.0 });
        self.eye += Vec3::new(cos, 0.0, sin) * (right - left);
        self.eye.y = (self.eye.y + rise - sink).max(0.5);
    }

    /// Another tree in view, unless it would be too close to one (0x4135e0).
    fn plant(&mut self) -> bool {
        let far = SKY * 0.7 * self.random.next() * self.random.next();
        let way = (self.random.next() * 120.0 - 60.0 + self.yaw).to_radians();
        let at = Vec2::new(way.sin(), way.cos()) * far;
        if self.trees.iter().any(|t| t.at.distance(at) < 2.0) {
            return false;
        }
        let mut tree = Tree::new(&mut self.random, self.trees[0].values);
        tree.at = at;
        tree.turned = self.random.next() * 360.0;
        self.trees.push(tree);
        true
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
            self.keys[i] = down;
            return true;
        }
        if !down {
            return false;
        }
        match key.to_ascii_lowercase().as_str() {
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => {
                self.fireflies = usize::from(key.as_bytes()[0] - b'0');
            }
            "c" => {
                self.trees[0].seed = Seed::new(&mut self.random);
                self.trees[0].changed = true;
            }
            "f" => self.free = !self.free,
            // As many as the original plants with one key; more would not be drawn in time.
            "g" if self.trees.len() < 60 => self.waiting += 15,
            "l" => self.kind ^= 1,
            "u" => self.sliders = !self.sliders,
            "w" => {
                self.wireframe = !self.wireframe;
                self.trees.iter_mut().for_each(|t| t.changed = true);
            }
            "+" | "=" if self.trees.len() < 60 => {
                let _ = (0..100).any(|_| self.plant());
            }
            "-" if self.trees.len() > 1 => {
                self.trees.pop();
            }
            _ => return false,
        }
        true
    }

    /// Moves toward the pointer, or a slider that is under it.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        self.pointer = Vec2::new(x as f32, y as f32);
        let at = Vec2::new(self.pointer.x * self.aspect, 1.0 - self.pointer.y) * 480.0;
        if buttons & 1 != 0 && self.buttons & 1 == 0 && self.sliders {
            let on = |i: &usize| (slider(*i)..slider(*i) + 20.0).contains(&at.y);
            self.held = (0..SLIDERS.len()).find(on).filter(|_| at.x < PANEL_WIDTH);
        }
        if buttons & 1 == 0 {
            self.held = None;
        }
        if let Some(i) = self.held {
            let (_, least, most, _) = SLIDERS[i];
            let part = ((at.x - 10.0) / (PANEL_WIDTH - 20.0)).clamp(0.0, 1.0);
            let value = least + part * (most - least);
            let tree = &mut self.trees[0];
            if tree.values[i] != value {
                tree.values[i] = value;
                tree.changed = true;
            }
        }
        self.buttons = buttons;
    }

    /// Not in the original.
    pub fn wheel(&mut self, amount: f64) {
        self.speed += amount as f32 * 4.0;
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let leaves: usize = self.trees.iter().map(|t| t.leaves).sum();
        let kind = ["leaves", "bills"][self.kind];
        format!(
            "trees {}, {leaves} {kind}, fireflies {}, fullness {:.2}, eye {:.1}",
            self.trees.len(),
            self.fireflies,
            self.trees[0].values[0],
            self.eye
        )
    }

    /// Makes meshes of what changed.
    fn build(&mut self, gpu: &Gpu, gl: &mut Gl) {
        for (i, t) in self.trees.iter_mut().enumerate().filter(|t| t.1.changed) {
            t.changed = false;
            let mut random = Random(i as u64 * 1009 + 1);
            let grown = grow(&Shape::new(&t.values), &mut t.seed, &mut random);
            t.leaves = grown.leaves.len() / 6;
            let mut parts = [grown.branches, grown.leaves, ground(&self.bumps, t.at)];
            if self.wireframe {
                parts = parts.map(|p| lines(&p));
            }
            let Some(old) = t.meshes else {
                t.meshes = Some(parts.map(|p| gl.mesh(gpu, &p)));
                continue;
            };
            for (id, p) in old.into_iter().zip(parts) {
                gl.replace_mesh(gpu, id, &p);
            }
        }
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        self.build(gpu, gl);
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        self.aspect = w as f32 / h as f32;
        // Half as high as far.
        let fov = 2.0 * 0.5f32.atan().to_degrees();
        let all = from_opengl(perspective(fov, self.aspect, 0.1, 1000.0))
            * Mat4::from_rotation_x(-self.pitch.to_radians())
            * Mat4::from_rotation_y(self.yaw.to_radians())
            * Mat4::from_translation(-self.eye)
            * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let count = self.fireflies;
        let surface = |m: Mat4, t: TextureId, sign: f32, lights: Option<(&Tree, Mat4)>| {
            let mut s = Surface::textures(&[t]);
            for (i, c) in m.to_cols_array_2d().into_iter().enumerate() {
                s = s.param(i, c);
            }
            s = s.param(4, [sign, count as f32, 0.0, 0.0]);
            if let Some((tree, place)) = lights {
                for (i, f) in tree.fireflies[..count].iter().enumerate() {
                    let [r, g, b] = hue(f.hue);
                    s = s.param(8 + i, place.transform_point3(f.at).extend(1.0).to_array());
                    s = s.param(16 + i, [r, g, b, 0.0]);
                }
            }
            s
        };
        let state = |program: usize, depth: Depth, cull: Option<Face>| {
            let s = State::new(program).vertex("grown").depth(depth);
            State {
                lines: self.wireframe,
                cull: cull.filter(|_| !self.wireframe),
                ..s
            }
        };
        // The fireflies of each tree light the ground around it.
        let over = state(TEXTURED, Depth::Always, None).blend(B::One, B::One);
        for t in &self.trees {
            let s = surface(all, self.ground, 1.0, Some((t, t.place())));
            gl.draw_mesh(over, &s, t.meshes.expect("built")[2]);
        }
        let solid = Depth::Less(true);
        for t in &self.trees {
            let m = all * t.place();
            let [branches, leaves, _] = t.meshes.expect("built");
            let here = Some((t, Mat4::IDENTITY));
            let [front, back] = self.leaves[self.kind];
            let s = surface(m, front, 1.0, here);
            gl.draw_mesh(state(LEAF, solid, Some(Face::Back)), &s, leaves);
            let s = surface(m, back, -1.0, here);
            gl.draw_mesh(state(LEAF, solid, Some(Face::Front)), &s, leaves);
            let s = surface(m, self.bark, 1.0, here);
            gl.draw_mesh(state(TEXTURED, solid, Some(Face::Back)), &s, branches);
        }
        // The sky, around and above (0x40d030).
        let half = [0.5, 0.5, 0.5, 1.0];
        let at = |i: usize, top: f32, color| {
            let part = i as f32 / 24.0;
            let (y, x) = (part * std::f32::consts::TAU).sin_cos();
            let p = Vec3::new(x * SKY, y * SKY, top * SKY_HEIGHT);
            Vertex::new(all * p.extend(1.0), color).uv(0, part * 4.0, top * 0.99)
        };
        let mut sides = Vec::new();
        let mut above = Vec::new();
        let dark = [12.0 / 255.0, 30.0 / 255.0, 52.0 / 255.0, 1.0];
        for i in 0..24 {
            let [a, b, c, d] = [
                at(i, 0.0, half),
                at(i + 1, 0.0, half),
                at(i + 1, 1.0, half),
                at(i, 1.0, half),
            ];
            sides.extend([a, b, c, a, c, d]);
            if i >= 2 {
                above.extend([at(0, 1.0, dark), at(i - 1, 1.0, dark), at(i, 1.0, dark)]);
            }
        }
        above.extend([at(0, 1.0, dark), at(22, 1.0, dark), at(23, 1.0, dark)]);
        let sky = State::new(TEXTURED).depth(solid);
        gl.draw(sky, &Surface::textures(&[self.sky]), &sides);
        gl.draw(sky, &Surface::textures(&[WHITE]), &above);

        // Fireflies flicker (0x404950).
        let mut random = Random(self.frame * 7919 % 2_147_483_646 + 1);
        (0..3).for_each(|_| _ = random.next());
        let mut glow = Vec::new();
        for t in &self.trees {
            let m = all * t.place();
            for f in &t.fireflies[..count] {
                let [r, g, b] = hue(f.hue);
                let middle = Vertex::new(m * f.at.extend(1.0), [r, g, b, 1.0]);
                let edge: [Vertex; 10] = std::array::from_fn(|_| {
                    let off = Vec3::from_array(std::array::from_fn(|_| random.next() - 0.5));
                    Vertex::new(m * (f.at + off * 0.2).extend(1.0), [r, g, b, 0.0])
                });
                for e in edge.windows(2) {
                    glow.extend([middle, e[0], e[1]]);
                }
            }
        }
        let faint = State::new(PLAIN).blend(B::SrcAlpha, B::OneMinusSrcAlpha);
        gl.draw(faint.depth(Depth::Less(false)), &Surface::default(), &glow);
        if self.sliders {
            self.panel(gl, faint);
        }
    }

    fn panel(&self, gl: &mut Gl, plain: State) {
        let wide = self.aspect * 480.0;
        let show = |x: [f32; 2], y: [f32; 2], color: [f32; 4]| {
            let v = |i: usize, j: usize| {
                let p = Vec4::new(x[i] / wide * 2.0 - 1.0, y[j] / 240.0 - 1.0, 0.0, 1.0);
                Vertex::new(p, color).uv(0, i as f32, j as f32)
            };
            [v(0, 0), v(1, 0), v(1, 1), v(0, 1)]
        };
        let picture = State {
            program: TEXTURED,
            ..plain
        };
        let none = Surface::default();
        for (i, (_, least, most, _)) in SLIDERS.into_iter().enumerate() {
            let y = slider(i);
            let label = show([0.0, PANEL_WIDTH], [y + 20.0, y + 35.0], [1.0; 4]);
            gl.quad(picture, &Surface::textures(&[self.labels[i]]), label);
            let groove = show(
                [10.0, PANEL_WIDTH - 10.0],
                [y + 9.0, y + 11.0],
                [1.0, 1.0, 1.0, 0.5],
            );
            gl.quad(plain, &none, groove);
            let part = (self.trees[0].values[i] - least) / (most - least);
            let x = 10.0 + part * (PANEL_WIDTH - 20.0);
            let light = if self.held == Some(i) { 1.0 } else { 0.8 };
            let knob = show(
                [x - 4.0, x + 4.0],
                [y + 3.0, y + 17.0],
                [light, light, light, 0.9],
            );
            gl.quad(plain, &none, knob);
        }
        let logo = show([wide - 100.0, wide], [PANEL, PANEL + 30.0], [1.0; 4]);
        gl.quad(picture, &Surface::textures(&[self.nvidia]), logo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trees_are_whole_and_stay_the_same() {
        let shape = Shape::new(&SLIDERS.map(|s| s.3));
        let mut seed = Seed::new(&mut Random(5));
        let mut random = Random(9);
        let a = grow(&shape, &mut seed, &mut random);
        // Two parts for each branch: leaves are one more than branches.
        let bands = a.branches.len() / (SIDES * 6);
        assert_eq!(bands, 2 * (a.leaves.len() / 6 - 1));
        let b = grow(&shape, &mut seed, &mut Random(9));
        assert_eq!(a.branches.len(), b.branches.len());
        assert!(
            a.branches
                .iter()
                .zip(&b.branches)
                .all(|(a, b)| a.position == b.position)
        );
        // The trunk faces out.
        for t in a.branches[..SIDES * 6].chunks_exact(3) {
            let p = |i: usize| Vec4::from(t[i].position).truncate();
            let n = (p(1) - p(0)).cross(p(2) - p(0));
            assert!(n.dot(Vec4::from(t[0].uv[1]).truncate()) > 0.0);
        }
    }

    #[test]
    fn the_fullest_tree_ends() {
        let mut shape = Shape::new(&SLIDERS.map(|s| s.3));
        for (balance, character) in [(0.5, 1.0), (1.0, 1.0), (0.0, 0.0), (0.7, 1.0)] {
            shape = Shape {
                balance,
                character,
                shortest: 0.03,
                ..shape
            };
            for seed in 1..6 {
                let mut seed = Seed::new(&mut Random(seed));
                let g = grow(&shape, &mut seed, &mut Random(3));
                assert!(g.leaves.len() / 6 < 200_000, "{}", g.leaves.len() / 6);
            }
        }
    }

    #[test]
    fn fireflies_stay_near() {
        let mut random = Random(1);
        let mut f = Firefly::new(&mut random);
        for _ in 0..10_000 {
            f.step(&mut random);
            assert!(f.at.length() < 20.0 && f.at.z >= 0.5);
        }
    }

    #[test]
    fn hues_go_around() {
        assert_eq!(hue(0.0), [1.0, 0.6, 0.6]);
        let [r, g, b] = hue(1.0 / 3.0);
        assert!(g > 0.999 && (r - 0.6).abs() < 1e-3 && (b - 0.6).abs() < 1e-3);
    }
}
