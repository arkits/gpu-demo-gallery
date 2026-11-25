//! The laboratory (`lightning000419.exe`; addresses are in it).
use glam::{Mat3, Quat, Vec2, Vec3};
use wgpu::BlendFactor as B;
use wgpu::Face;

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, REPEAT, State, Surface, TextureId, Vertex, WHITE, from_opengl, look_at,
    perspective,
};
use crate::image::Image;

pub const FILES: &[&str] = &[
    "lightning.png",
    "plate.png",
    "plate_bump.png",
    "smallspark.png",
    "spark.png",
    "walls.png",
    "walls_bump.png",
];
pub const SOUNDS: &[&str] = &[
    "background.wav",
    "bounce.wav",
    "clickon.wav",
    "lightning.wav",
    "spark.wav",
];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "lit", "lit_wall", "dark_wall", "bump"];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Lightning"),
    ("b", "Bumps"),
    ("x", "More sparks"),
    ("w", "Wireframe"),
];

const PLAIN: usize = 0;
const TEXTURED: usize = 1;
const LIT: usize = 2;
const LIT_WALL: usize = 3;
const DARK_WALL: usize = 4;
const BUMP: usize = 5;

/// Where the bolt begins, and the plate (0x4055e0).
const TOP: Vec3 = Vec3::new(0.0, 2.0, 0.0);
const PLATE_Y: f32 = -2.0;
const PLATE: f32 = 2.0;
/// Points of heat along each side of the plate.
const HEAT: usize = 32;
/// Half the size of each room, how often the picture repeats from the middle to the side.
const ROOMS: [(f32, f32); 2] = [(6.0, 1.0), (10.0, 2.0)];
/// Parts of the bolt.
const PARTS: usize = 128;
/// Points of the bolt that shine (0x401fa0).
const SHINING: [usize; 3] = [64, 32, 96];
const G: f32 = -10.0;
const STEP: f32 = 1.0 / 60.0;

/// Park and Miller's numbers, as the original's.
struct Random(u64);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 = self.0 * 16807 % 2_147_483_647;
        (self.0 as f64 / 2_147_483_647.0) as f32
    }
}

struct Ball {
    at: Vec3,
    radius: f32,
}

/// Radius 0.1.
struct Rod {
    from: Vec3,
    to: Vec3,
}

const ROD: f32 = 0.1;

/// The frame that stands in the rooms (0x40cd40).
fn frame() -> (Vec<Ball>, Vec<Rod>) {
    let mut balls = vec![Ball {
        at: TOP,
        radius: 0.8,
    }];
    let corners = [(-2.0, -2.0), (2.0, -2.0), (-2.0, 2.0), (2.0, 2.0)];
    for y in [-2.0, 2.0] {
        for (x, z) in corners {
            let at = Vec3::new(x, y, z);
            balls.push(Ball { at, radius: 0.3 });
        }
    }
    let (near, far) = (2.0, ROOMS[1].0);
    let rod = |a: [f32; 3], b: [f32; 3]| Rod {
        from: a.into(),
        to: b.into(),
    };
    let mut rods = Vec::new();
    for (x, z) in corners {
        rods.push(rod([x, -far, z], [x, far, z]));
        // Out from the corners below.
        rods.push(rod([x, -near, z], [x, -near, far * z.signum()]));
        rods.push(rod([x, -near, z], [far * x.signum(), -near, z]));
    }
    // Across above, and the two diagonals.
    for s in [-near, near] {
        rods.push(rod([-far, near, s], [far, near, s]));
        rods.push(rod([s, near, -far], [s, near, far]));
    }
    rods.push(rod([-near, near, -near], [near, near, near]));
    rods.push(rod([near, near, -near], [-near, near, near]));
    (balls, rods)
}

/// From a point along a way: how much of the way to the surface, and its normal there.
type Hit = Option<(f32, Vec3)>;

impl Ball {
    /// 0x411560.
    fn hit(&self, p: Vec3, d: Vec3) -> Hit {
        let o = p - self.at;
        let (a, b, c) = (
            d.length_squared(),
            2.0 * o.dot(d),
            o.length_squared() - self.radius * self.radius,
        );
        let q = b * b - 4.0 * a * c;
        if q < 0.0 || a == 0.0 {
            return None;
        }
        let roots = [(-b - q.sqrt()) / (2.0 * a), (-b + q.sqrt()) / (2.0 * a)];
        let t = roots.into_iter().find(|t| *t >= 0.0)?;
        (t > 0.0 && t < 1.0).then(|| (t, o + d * t))
    }
}

impl Rod {
    /// 0x4152b0.
    fn hit(&self, p: Vec3, d: Vec3) -> Hit {
        let axis = (self.to - self.from).normalize();
        let flat = |v: Vec3| v - axis * v.dot(axis);
        let (o, w) = (flat(p - self.from), flat(d));
        let (a, b, c) = (
            w.length_squared(),
            2.0 * o.dot(w),
            o.length_squared() - ROD * ROD,
        );
        let q = b * b - 4.0 * a * c;
        if q < 0.0 || a == 0.0 {
            return None;
        }
        let roots = [(-b - q.sqrt()) / (2.0 * a), (-b + q.sqrt()) / (2.0 * a)];
        let t = roots.into_iter().find(|t| *t >= 0.0)?;
        let at = p + d * t;
        let along = (at - self.from).dot(axis);
        let inside = (0.0..=self.from.distance(self.to)).contains(&along);
        (t > 0.0 && t < 1.0 && inside).then(|| (t, flat(at - self.from)))
    }
}

/// 0x4133b0.
fn plate_hit(p: Vec3, d: Vec3) -> Hit {
    let t = (PLATE_Y - p.y) / d.y;
    let at = p + d * t;
    let on = at.x.abs() <= PLATE && at.z.abs() <= PLATE;
    (t >= 0.0 && on).then_some((t, Vec3::Y))
}

/// The nearest of the six sides of the inner room (0x417bc0).
fn room_hit(p: Vec3, d: Vec3) -> Hit {
    let mut best: Hit = None;
    for n in [Vec3::X, Vec3::Y, Vec3::Z] {
        for n in [n, -n] {
            let t = -(p.dot(n) + ROOMS[0].0) / d.dot(n);
            if t > 0.0 && best.is_none_or(|b| t < b.0) {
                best = Some((t, n));
            }
        }
    }
    best
}

#[derive(Clone, Copy, Default)]
struct Spark {
    at: Vec3,
    speed: Vec3,
    life: f32,
}

impl Spark {
    /// Up, between 18 and 90 degrees (0x41adf0, 0x418510).
    fn launch(random: &mut Random, from: Vec3, speed: f32, dt: f32) -> Self {
        let around = random.next() * std::f32::consts::TAU;
        let up = std::f32::consts::FRAC_PI_2 * (0.2 + 0.8 * random.next());
        let way = Quat::from_rotation_y(around) * Quat::from_rotation_x(up) * Vec3::NEG_Z;
        Self {
            at: from + way * speed * dt,
            speed: way * speed,
            life: 1.0,
        }
    }

    fn move_on(&mut self, dt: f32) {
        self.at += self.speed * dt;
        self.speed.y += G * dt;
    }
}

struct Bolt {
    points: Vec<Vec3>,
    /// How far it strays, as a part of its length; grows to 0.3.
    stray: f32,
}

impl Bolt {
    /// Most parts stay as they are from one frame to the next (0x4014f0).
    fn split(&mut self, random: &mut Random, a: usize, b: usize, anew: bool) {
        if b - a == 1 {
            return;
        }
        let m = (a + b) / 2;
        let anew = anew || random.next() > 0.9;
        if anew {
            let (p, q) = (self.points[a], self.points[b]);
            let far = p.distance(q) * self.stray;
            let off = Vec3::from_array(std::array::from_fn(|_| (random.next() - 0.5) * far));
            self.points[m] = (p + q) * 0.5 + off;
        }
        self.split(random, a, m, anew);
        self.split(random, m, b, anew);
    }

    /// 0x4017f0.
    fn update(&mut self, random: &mut Random, to: Vec3) {
        let moved = self.points[0] != TOP || self.points[PARTS] != to;
        let anew = random.next() > 0.9 || moved;
        if anew {
            self.points[0] = TOP;
            self.points[PARTS] = to;
        }
        self.split(random, 0, PARTS, anew);
        self.stray = (self.stray + 0.01).min(0.3);
    }
}

/// Normals from heights, with `depth` (0x424250). Rows begin at the bottom.
fn normals(height: &Image, depth: f32) -> Image {
    let (w, h) = (height.width as i32, height.height as i32);
    let at = |x: i32, y: i32| {
        let i = (y.rem_euclid(h) * w + x.rem_euclid(w)) as usize * 4;
        f32::from(height.rgba[i]) / 255.0
    };
    let mut rgba = Vec::with_capacity(height.rgba.len());
    for y in 0..h {
        for x in 0..w {
            let n = Vec3::new(
                (at(x - 1, y) - at(x + 1, y)) * depth,
                (at(x, y - 1) - at(x, y + 1)) * depth,
                1.0,
            );
            let n = n.normalize() * 127.0 + 128.0;
            rgba.extend([n.x as u8, n.y as u8, n.z as u8, 255]);
        }
    }
    Image {
        rgba,
        ..height.clone()
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

/// A corner of something that does not move: place, normal, picture.
#[derive(Clone, Copy)]
struct Corner(Vec3, Vec3, [f32; 2]);

fn balls_mesh(balls: &[Ball]) -> Vec<Corner> {
    let mut faces: Vec<[Vec3; 3]> = Vec::new();
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                let f = [Vec3::X * x, Vec3::Y * y, Vec3::Z * z];
                faces.push(if x * y * z > 0.0 {
                    f
                } else {
                    [f[0], f[2], f[1]]
                });
            }
        }
    }
    for _ in 0..4 {
        let cut = |[a, b, c]: [Vec3; 3]| {
            let m = |a: Vec3, b: Vec3| (a + b).normalize();
            let (p, q, r) = (m(a, b), m(b, c), m(c, a));
            [[p, q, r], [a, p, r], [b, q, p], [c, r, q]]
        };
        faces = faces.into_iter().flat_map(cut).collect();
    }
    let mut out = Vec::new();
    for b in balls {
        let corner = |&n: &Vec3| Corner(b.at + n * b.radius, n, [0.0; 2]);
        out.extend(faces.iter().flatten().map(corner));
    }
    out
}

fn rods_mesh(rods: &[Rod]) -> Vec<Corner> {
    const SIDES: usize = 12;
    let mut out = Vec::new();
    for r in rods {
        let axis = (r.to - r.from).normalize();
        let (u, v) = axis.any_orthonormal_pair();
        let corner = |k: usize, end: Vec3| {
            let a = std::f32::consts::TAU * k as f32 / SIDES as f32;
            let n = u * a.cos() + v * a.sin();
            Corner(end + n * ROD, n, [0.0; 2])
        };
        for k in 0..SIDES {
            let [a, b, c, d] = [
                corner(k, r.from),
                corner(k + 1, r.from),
                corner(k + 1, r.to),
                corner(k, r.to),
            ];
            out.extend([a, b, c, a, c, d]);
        }
    }
    out
}

/// Turns the side at -z to each of the six (0x415f10).
fn sides() -> [Mat3; 6] {
    let q = std::f32::consts::FRAC_PI_2;
    [
        Mat3::IDENTITY,
        Mat3::from_rotation_y(q),
        Mat3::from_rotation_y(2.0 * q),
        Mat3::from_rotation_y(3.0 * q),
        Mat3::from_rotation_x(q),
        Mat3::from_rotation_x(-q),
    ]
}

struct Pictures {
    bolt: TextureId,
    plate: TextureId,
    plate_bump: TextureId,
    small: TextureId,
    spark: TextureId,
    walls: TextureId,
    walls_bump: TextureId,
}

pub struct Demo {
    pictures: Pictures,
    balls: Vec<Ball>,
    rods: Vec<Rod>,
    mesh: Vec<Corner>,
    random: Random,
    bolt: Bolt,
    heat: Vec<f32>,
    sparks: Vec<Spark>,
    more: Vec<Spark>,
    small: Vec<Spark>,
    /// The next of each to launch.
    next: [usize; 3],
    yaw: f32,
    yaw_speed: f32,
    distance: f32,
    distance_speed: f32,
    /// Left, right, up, down.
    keys: [bool; 4],
    mouse: Vec2,
    pressed: bool,
    /// Without the pointer.
    steady: bool,
    aspect: f32,
    hit: Option<Vec3>,
    bumps: bool,
    more_sparks: bool,
    wireframe: bool,
    cues: Vec<String>,
    due: f64,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let image = |n: &str| Ok::<_, Error>(upright(&files.image(n)?));
        let (balls, rods) = frame();
        let mut mesh = balls_mesh(&balls);
        mesh.extend(rods_mesh(&rods));
        let plate_bump = normals(&image("plate_bump.png")?, 6.0);
        let walls_bump = normals(&image("walls_bump.png")?, 4.0);
        Ok(Self {
            pictures: Pictures {
                bolt: gl.texture(gpu, &image("lightning.png")?, CLAMP),
                plate: gl.texture(gpu, &image("plate.png")?, REPEAT),
                plate_bump: gl.texture(gpu, &plate_bump, REPEAT),
                small: gl.texture(gpu, &image("smallspark.png")?, CLAMP),
                spark: gl.texture(gpu, &image("spark.png")?, CLAMP),
                walls: gl.texture(gpu, &image("walls.png")?, REPEAT),
                walls_bump: gl.texture(gpu, &walls_bump, REPEAT),
            },
            balls,
            rods,
            mesh,
            random: Random(1),
            bolt: Bolt {
                points: vec![Vec3::ZERO; PARTS + 1],
                stray: 0.0,
            },
            heat: vec![0.0; HEAT * HEAT],
            sparks: vec![Spark::default(); 4],
            more: vec![Spark::default(); 8],
            small: vec![Spark::default(); 50],
            next: [0; 3],
            yaw: std::f32::consts::PI,
            yaw_speed: 0.0,
            distance: ROOMS[0].0,
            distance_speed: 0.0,
            keys: [false; 4],
            mouse: Vec2::ZERO,
            pressed: false,
            steady: false,
            aspect: 4.0 / 3.0,
            hit: None,
            bumps: true,
            more_sparks: false,
            wireframe: false,
            cues: vec!["background*".into()],
            due: 0.0,
        })
    }

    fn eye(&self) -> Vec3 {
        Quat::from_rotation_y(self.yaw) * Vec3::NEG_Z * self.distance
    }

    /// The original moves once a frame, which is taken as 60 a second (0x407830).
    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            self.tick();
        }
    }

    fn tick(&mut self) {
        let [left, right, up, down] = self.keys;
        if up {
            self.distance_speed -= 0.05;
        }
        if down {
            self.distance_speed += 0.05;
        }
        self.distance_speed *= 0.94;
        self.distance += self.distance_speed * STEP;
        let range = 0.01..=ROOMS[0].0;
        if !range.contains(&self.distance) {
            self.distance = self.distance.clamp(0.01, ROOMS[0].0);
            self.distance_speed = 0.0;
        }
        self.yaw_speed += match (right, left) {
            (true, _) => -0.02,
            (_, true) => 0.02,
            _ => 0.001,
        };
        self.yaw_speed *= 0.94;
        self.yaw += self.yaw_speed * STEP;

        self.cool();
        self.fly();
        let was = self.hit.is_some();
        self.hit = self.aim();
        if let Some(at) = self.hit {
            self.strike(at);
            if !was {
                self.bolt.stray = 0.0;
                self.cues.extend(["clickon".into(), "lightning*".into()]);
            }
            self.bolt.update(&mut self.random, at);
        } else if was {
            self.cues.push("-lightning".into());
        }
    }

    /// The heat spreads and is lost (0x413610).
    fn cool(&mut self) {
        let old = self.heat.clone();
        // Beyond the sides it is cold (0x4135d0).
        let at = |i: i32, j: i32| {
            let inside = (0..HEAT as i32).contains(&i) && (0..HEAT as i32).contains(&j);
            if inside {
                old[i as usize * HEAT + j as usize]
            } else {
                0.0
            }
        };
        for i in 0..HEAT as i32 {
            for j in 0..HEAT as i32 {
                let around = at(i, j - 1) + at(i + 1, j) + at(i - 1, j) + at(i, j + 1);
                self.heat[i as usize * HEAT + j as usize] =
                    around * 0.005 + at(i, j) * (1.0 - 0.005 * 4.0 - 0.0002);
            }
        }
    }

    /// 0x41b750, 0x418d40, 0x41ab60, 0x418290.
    fn fly(&mut self) {
        let shapes = |p: Vec3, d: Vec3, which: usize| match which {
            0 => {
                let ball = self.balls.iter().find_map(|b| b.hit(p, d));
                ball.or_else(|| self.rods.iter().find_map(|r| r.hit(p, d)))
            }
            1 => plate_hit(p, d),
            _ => room_hit(p, d),
        };
        let mut bounced = false;
        let mut bounce = |s: &mut Spark, small: bool| {
            for which in 0..3 {
                let d = s.speed * STEP;
                let hit = shapes(s.at, d, which).filter(|h| h.0 > 0.0 && h.0 < 1.0);
                if let Some((t, n)) = hit {
                    if !small {
                        bounced |= s.life > 0.2;
                        s.at += d * t;
                    }
                    let n = n.normalize();
                    s.speed = (s.speed - n * 2.0 * n.dot(s.speed)) * 0.8;
                }
            }
        };
        let mut sparks = std::mem::take(&mut self.sparks);
        let mut more = std::mem::take(&mut self.more);
        let mut small = std::mem::take(&mut self.small);
        for s in sparks.iter_mut().chain(&mut more).filter(|s| s.life > 0.0) {
            bounce(s, false);
            s.life *= 0.99;
            if s.life < 0.1 {
                s.life = 0.0;
            }
            s.move_on(STEP);
        }
        for s in small.iter_mut().filter(|s| s.life > 0.004) {
            bounce(s, true);
            s.life *= 0.95;
            s.move_on(STEP * 0.5);
        }
        if bounced {
            self.cues.push("bounce".into());
        }
        (self.sparks, self.more, self.small) = (sparks, more, small);
    }

    /// Where on the plate the pointer is, if it is pressed there.
    fn aim(&self) -> Option<Vec3> {
        if self.steady {
            return Some(Vec3::new(0.7, PLATE_Y, 0.4));
        }
        if !self.pressed {
            return None;
        }
        let eye = self.eye();
        let turn = look_at(eye, Vec3::ZERO, Vec3::Y).inverse();
        let way = Vec3::new(
            self.mouse.x * self.aspect * self.half(),
            self.mouse.y * self.half(),
            -1.0,
        );
        let way = turn.transform_vector3(way);
        plate_hit(eye, way).map(|(t, _)| eye + way * t)
    }

    /// Heats the plate and throws sparks (0x4134f0, 0x41b660, 0x418ce0).
    fn strike(&mut self, at: Vec3) {
        let cell = |v: f32| (((v / PLATE + 1.0) * 0.5 * HEAT as f32) as usize).min(HEAT - 1);
        let i = cell(at.z) * HEAT + cell(at.x);
        self.heat[i] += (1.0 - self.heat[i]) * 0.1;
        let groups = [true, self.more_sparks];
        for (k, _) in groups.into_iter().enumerate().filter(|g| g.1) {
            if self.random.next() < 0.07 {
                let fast = self.random.next() * self.random.next();
                let speed = 15.0 * 0.2 + 15.0 * fast * 0.8;
                let spark = Spark::launch(&mut self.random, at, speed, STEP);
                let group = [&mut self.sparks, &mut self.more]
                    .into_iter()
                    .nth(k)
                    .expect("two");
                group[self.next[k]] = spark;
                self.next[k] = (self.next[k] + 1) % group.len();
                self.cues.push("spark".into());
            }
        }
        for _ in 0..5 {
            let speed = 2.5 + 2.5 * self.random.next();
            self.small[self.next[2]] = Spark::launch(&mut self.random, at, speed, STEP * 0.5);
            self.next[2] = (self.next[2] + 1) % self.small.len();
        }
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
        match key {
            " " => self.steady = !self.steady,
            "b" | "B" => self.bumps = !self.bumps,
            "x" | "X" => self.more_sparks = !self.more_sparks,
            "w" | "W" => self.wireframe = !self.wireframe,
            _ => return false,
        }
        true
    }

    /// Lightning goes to where the plate is pressed.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        self.mouse = Vec2::new(x as f32 * 2.0 - 1.0, 1.0 - y as f32 * 2.0);
        self.pressed = buttons & 1 != 0;
    }

    pub fn wheel(&mut self, amount: f64) {
        self.distance_speed -= amount as f32 * 4.0;
    }

    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }

    pub fn report(&self) -> String {
        let on = |b: bool| if b { "on" } else { "off" };
        let hot = self.heat.iter().copied().fold(0.0, f32::max);
        let flying = self
            .sparks
            .iter()
            .chain(&self.more)
            .filter(|s| s.life > 0.0)
            .count();
        format!(
            "lightning {}, bumps {}, heat {hot:.2}, sparks {flying}, distance {:.1}",
            on(self.hit.is_some()),
            on(self.bumps),
            self.distance
        )
    }

    /// Half the height of the window, 1 away. More in windows higher than wide.
    fn half(&self) -> f32 {
        0.8 * (1.0 / self.aspect).clamp(1.0, 1.4)
    }

    pub fn draw(&mut self, _: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        self.aspect = w as f32 / h as f32;
        let eye = self.eye();
        let fov = 2.0 * self.half().atan().to_degrees();
        // From 1 away to 100 in the original, which cuts off what the eye comes close to.
        let all = from_opengl(perspective(fov, self.aspect, 0.1, 100.0))
            * look_at(eye, Vec3::ZERO, Vec3::Y);
        let place = |p: Vec3| all * p.extend(1.0);
        let put = |gl: &mut Gl, state: State, surface: &Surface, v: &[Vertex]| {
            if !self.wireframe {
                return gl.draw(state, surface, v);
            }
            let mut lines = Vec::with_capacity(v.len() * 2);
            for t in v.chunks_exact(3) {
                lines.extend([t[0], t[1], t[1], t[2], t[2], t[0]]);
            }
            gl.draw(
                State {
                    cull: None,
                    ..state
                }
                .lines(),
                surface,
                &lines,
            );
        };
        let corner = |c: &Corner, diffuse: [f32; 3], highlight: f32| {
            let [r, g, b] = diffuse;
            Vertex::new(place(c.0), [0.0; 4])
                .uv(0, c.2[0], c.2[1])
                .uvw(1, c.0.extend(1.0).to_array())
                .uvw(2, c.1.extend(0.0).to_array())
                .uvw(3, [r, g, b, highlight])
        };
        let lights = |t: &[TextureId], bolt: bool| {
            let count = if bolt { 3.0 } else { 0.0 };
            let mut s = Surface::textures(t).param(0, eye.extend(count).to_array());
            for (i, k) in SHINING.into_iter().enumerate() {
                s = s.param(i + 1, self.bolt.points[k].extend(1.0).to_array());
            }
            for (i, k) in self.sparks.iter().enumerate() {
                s = s.param(i + 4, k.at.extend(k.life).to_array());
            }
            s
        };
        let on = self.hit.is_some();
        let solid = Depth::Less(true);
        let pictures = &self.pictures;
        let p = |i: usize| self.bolt.points[i];

        for (half, repeats) in ROOMS {
            if on && self.bumps {
                let light = p(64) * 1.2 + p(32) + p(96);
                let wall = |m: &Mat3, f: &dyn Fn(Vec3, [f32; 2]) -> Vertex| {
                    grid(4, |x, y| {
                        f(
                            *m * Vec3::new(x * half, y * half, -half),
                            [x * repeats, y * repeats],
                        )
                    })
                };
                for m in sides() {
                    let v = wall(&m, &|at, uv| {
                        Vertex::new(place(at), [1.0; 4]).uv(0, uv[0], uv[1])
                    });
                    let state = State::new(DARK_WALL).depth(solid).cull(Face::Back);
                    put(gl, state, &Surface::textures(&[pictures.walls]), &v);
                }
                for m in sides() {
                    let v = wall(&m, &|at, uv| {
                        let h = m.transpose() * between(light, eye, at);
                        Vertex::new(place(at), [1.0; 4])
                            .uv(0, uv[0], uv[1])
                            .uvw(1, h.extend(0.0).to_array())
                    });
                    let state = State::new(BUMP).depth(Depth::Equal).cull(Face::Back);
                    let state = state.blend(B::DstAlpha, B::One);
                    put(gl, state, &Surface::textures(&[pictures.walls_bump]), &v);
                }
            } else {
                for m in sides() {
                    let n = m * Vec3::Z;
                    let v = grid(1, |x, y| {
                        let at = m * Vec3::new(x * half, y * half, -half);
                        corner(&Corner(at, n, [x * repeats, y * repeats]), [0.3; 3], 1.2)
                    });
                    let state = State::new(LIT_WALL).depth(solid).cull(Face::Back);
                    put(gl, state, &lights(&[pictures.walls], on), &v);
                }
            }
        }

        let v: Vec<Vertex> = self
            .mesh
            .iter()
            .map(|c| corner(c, [0.45, 0.45, 0.5], 1.0))
            .collect();
        let state = State::new(LIT).depth(solid).cull(Face::Back);
        put(gl, state, &lights(&[WHITE], on), &v);

        // The plate. It glows where it is hot (0x412070).
        let n = HEAT as f32 - 1.0;
        let hot = |i: usize, j: usize| {
            let t = self.heat[i * HEAT + j];
            [t, t * t, t * t * t, 0.0]
        };
        let at = |i: usize| (i as f32 / n * 2.0 - 1.0) * PLATE;
        let top = |i: usize, j: usize| {
            let c = Corner(
                Vec3::new(at(j), PLATE_Y, at(i)),
                Vec3::Y,
                [j as f32 / n, 1.0 - i as f32 / n],
            );
            let mut v = corner(&c, [0.8; 3], 1.0);
            v.color = hot(i, j);
            v
        };
        let mut v = Vec::with_capacity((HEAT - 1) * (HEAT - 1) * 6);
        for i in 0..HEAT - 1 {
            for j in 0..HEAT - 1 {
                let [a, b, c, d] = [top(i, j), top(i + 1, j), top(i + 1, j + 1), top(i, j + 1)];
                v.extend([a, b, c, a, c, d]);
            }
        }
        // With the bumps on, the bolt does not light it otherwise.
        let bolt = on && !self.bumps;
        put(gl, state, &lights(&[pictures.plate], bolt), &v);
        let thick = Vec3::Y * (PLATE / n);
        let mut v = Vec::new();
        for m in &sides()[..4] {
            let normal = *m * Vec3::NEG_Z;
            let edge = |x: f32| *m * Vec3::new(x * PLATE, 0.0, -PLATE) + Vec3::Y * PLATE_Y;
            let [a, b] = [edge(-1.0), edge(1.0)];
            let at = [a, b, b - thick, a, b - thick, a - thick];
            v.extend(at.map(|p| corner(&Corner(p, normal, [0.0; 2]), [0.8; 3], 1.0)));
        }
        put(gl, state, &lights(&[WHITE], bolt), &v);
        if self.bumps {
            // Without lightning, the light is far above.
            let light = if on { p(96) } else { Vec3::Y * 100.0 };
            let turn = Mat3::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            let v = grid(5, |x, y| {
                let at = turn * Vec3::new(x * PLATE, y * PLATE, PLATE_Y * 0.999);
                let h = turn.transpose() * between(light, eye, at);
                Vertex::new(place(at), [1.0; 4])
                    .uv(0, x * 0.5 + 0.5, y * 0.5 + 0.5)
                    .uvw(1, h.extend(0.0).to_array())
            });
            let state = State::new(BUMP)
                .depth(Depth::LessEqual(false))
                .cull(Face::Back);
            let state = state.blend(B::One, B::One);
            put(gl, state, &Surface::textures(&[pictures.plate_bump]), &v);
        }

        let seen = Depth::Less(false);
        let mut draw_sparks = |sparks: &[Spark], size: f32, t: TextureId| {
            let mut v = Vec::new();
            for s in sparks.iter().filter(|s| s.life > 0.004) {
                let color = [1.0, 0.9, 0.8, s.life];
                // A kite that points where the spark came from (0x41bc90).
                let e = (eye - s.at).normalize();
                let across = e.cross(-s.speed);
                let along = across.cross(e);
                let long = if along.length() > 1.0 {
                    along
                } else {
                    along.normalize_or_zero()
                };
                let (a, b) = (long * size, across.normalize_or_zero() * size);
                let k = |p: Vec3, u: f32, t: f32| Vertex::new(place(p), color).uv(0, u, t);
                let q = [
                    k(s.at + a, 1.0, 0.0),
                    k(s.at + b, 1.0, 1.0),
                    k(s.at - a, 0.0, 1.0),
                    k(s.at - b, 0.0, 0.0),
                ];
                v.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
            }
            let state = State::new(TEXTURED).depth(seen).blend(B::SrcAlpha, B::One);
            put(gl, state, &Surface::textures(&[t]), &v);
        };
        draw_sparks(&self.sparks, 0.1, pictures.spark);
        draw_sparks(&self.small, 0.08, pictures.small);
        if self.more_sparks {
            draw_sparks(&self.more, 0.1, pictures.spark);
        }
        if on {
            let v = ribbon(&self.bolt.points, eye);
            let v: Vec<Vertex> = v
                .into_iter()
                .map(|(at, uv)| Vertex::new(place(at), [1.0; 4]).uv(0, uv[0], uv[1]))
                .collect();
            let state = State::new(TEXTURED).depth(seen);
            let state = state.blend(B::SrcAlpha, B::OneMinusSrcAlpha);
            put(gl, state, &Surface::textures(&[pictures.bolt]), &v);
        } else if self.wireframe {
            gl.draw(State::new(PLAIN).lines(), &Surface::default(), &[]);
        }
    }
}

/// Half way between the directions to the light and to the eye.
fn between(light: Vec3, eye: Vec3, at: Vec3) -> Vec3 {
    ((light - at).normalize() + (eye - at).normalize()).normalize()
}

/// `n` by `n` quads from -1 to 1.
fn grid(n: usize, f: impl Fn(f32, f32) -> Vertex) -> Vec<Vertex> {
    let at = |i: usize| i as f32 / n as f32 * 2.0 - 1.0;
    let mut out = Vec::with_capacity(n * n * 6);
    for i in 0..n {
        for j in 0..n {
            let [a, b, c, d] =
                [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)].map(|(x, y)| f(at(x), at(y)));
            out.extend([a, b, c, a, c, d]);
        }
    }
    out
}

/// A band 0.2 wide along points that faces the eye (0x402040).
fn ribbon(points: &[Vec3], eye: Vec3) -> Vec<(Vec3, [f32; 2])> {
    let last = points.len() - 1;
    let sides: Vec<Vec3> = (0..=last)
        .map(|i| {
            let to = eye - points[i];
            let before = if i > 0 {
                (points[i - 1] - points[i]).cross(to)
            } else {
                Vec3::ZERO
            };
            let after = if i < last {
                (points[i] - points[i + 1]).cross(to)
            } else {
                Vec3::ZERO
            };
            (before + after).normalize_or_zero() * 0.1
        })
        .collect();
    let mut out = Vec::with_capacity(last * 6);
    for i in 0..last {
        let edge = |k: usize, s: f32| {
            let u = (1.0 - s) * 0.5;
            (points[k] + sides[k] * s, [u, k as f32 / last as f32])
        };
        let [a, b, c, d] = [
            edge(i, 1.0),
            edge(i, -1.0),
            edge(i + 1, -1.0),
            edge(i + 1, 1.0),
        ];
        out.extend([a, b, c, a, c, d]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_face_out() {
        let (balls, rods) = frame();
        for t in balls_mesh(&balls)
            .chunks_exact(3)
            .chain(rods_mesh(&rods).chunks_exact(3))
        {
            let n = (t[1].0 - t[0].0).cross(t[2].0 - t[0].0);
            assert!(n.dot(t[0].1) > 0.0);
        }
        for m in sides() {
            let middle = m * Vec3::new(0.0, 0.0, -1.0);
            assert!((m * Vec3::Z).dot(middle) < 0.0);
        }
    }

    #[test]
    fn ways_meet_surfaces() {
        let ball = Ball {
            at: Vec3::ZERO,
            radius: 1.0,
        };
        let (t, n) = ball.hit(Vec3::X * 2.0, Vec3::X * -2.0).unwrap();
        assert!((t - 0.5).abs() < 1e-5 && n.normalize().distance(Vec3::X) < 1e-5);
        let rod = Rod {
            from: Vec3::ZERO,
            to: Vec3::Y * 2.0,
        };
        assert!(rod.hit(Vec3::new(1.0, 1.0, 0.0), Vec3::X * -1.0).is_some());
        assert!(rod.hit(Vec3::new(1.0, 3.0, 0.0), Vec3::X * -1.0).is_none());
        let (t, n) = room_hit(Vec3::ZERO, Vec3::X * 12.0).unwrap();
        assert!((t - 0.5).abs() < 1e-5 && n == Vec3::NEG_X);
        assert!(plate_hit(Vec3::ZERO, Vec3::NEG_Y).is_some());
        assert!(plate_hit(Vec3::X * 5.0, Vec3::NEG_Y).is_none());
    }

    #[test]
    fn sparks_stay_in_the_room() {
        let mut random = Random(7);
        for _ in 0..200 {
            let s = Spark::launch(&mut random, Vec3::ZERO, 2.0, 0.0);
            assert!((s.speed.length() - 2.0).abs() < 1e-4 && s.speed.y > 0.5);
        }
    }

    #[test]
    fn bolts_reach_their_ends() {
        let mut random = Random(1);
        let mut bolt = Bolt {
            points: vec![Vec3::ZERO; PARTS + 1],
            stray: 0.0,
        };
        let to = Vec3::new(1.0, -2.0, 0.0);
        (0..100).for_each(|_| bolt.update(&mut random, to));
        assert_eq!((bolt.points[0], bolt.points[PARTS]), (TOP, to));
        let longest = bolt
            .points
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .fold(0.0, f32::max);
        assert!(longest < 0.3, "{longest}");
    }
}
