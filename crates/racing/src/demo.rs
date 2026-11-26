//! ATB Racing: Indy cars lapping a track. `race.exe` loads `race.bof`
//! (`0x401eb0`), drives each car along a lane it keeps in its own data
//! (`0x401000`, `0x401260`), spins and steers the wheels and places the
//! camera (`0x402120`), and draws the track and two of the cars
//! (`0x402cd0`). See `docs/ENGINE.md`.
use atb::{Bof, Item, View};
use glam::{Mat4, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu};
use crate::pe::Pe;
use crate::render::{self, Camera, Fog, Frame, Light, Pictures, look_at};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("1", "Rear view"),
    ("2", "Chase"),
    ("3", "Follow"),
    ("4", "Track cameras"),
    ("5", "Front"),
    ("-", "Slower"),
    ("+", "Faster"),
    ("f", "Fog"),
    ("w", "Wireframe"),
    ("t", "Textures"),
];

/// `race.exe` is read for the four lanes in its data; nothing else of it.
pub const FILES: &[&str] = &["race.bof", "race.exe"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const CARS: [&str; 4] = ["sgi_indy", "mg_indy", "nin_indy", "vp_indy"];
const RATE: f64 = 60.0;
/// Lane tables: point counts at `0x4648b0`, pointers at `0x4648c0`, each
/// point six floats (x, y, height, then the up vector).
const COUNTS: u32 = 0x4648b0;
const POINTERS: u32 = 0x4648c0;
/// The roadside camera posts mode 4 picks from (`0x464aa8`).
const POSTS: u32 = 0x464aa8;

/// A lane (`0x401000`): points, the length and heading of each step.
struct Lane {
    points: Vec<Vec3>,
    lengths: Vec<f32>,
    headings: Vec<f32>,
    index: usize,
    along: f32,
}

impl Lane {
    fn read(pe: &Pe, i: u32) -> Result<Self, Error> {
        let n = pe.word(COUNTS + 4 * i)? as usize;
        if !(2..=10_000).contains(&n) {
            return Err("race.exe lane has an odd length".into());
        }
        let at = pe.word(POINTERS + 4 * i)?;
        // Lane points are (x, y, height); the track is y-up with z = y.
        let points = (0..n as u32)
            .map(|k| {
                let p = at + k * 24;
                Ok(Vec3::new(pe.float(p)?, pe.float(p + 8)?, pe.float(p + 4)?))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut lengths = Vec::new();
        let mut headings = Vec::new();
        for k in 0..n {
            let a = points[k];
            let b = if k + 1 < n { points[k + 1] } else { a };
            lengths.push(a.distance(b));
            headings.push((b.x - a.x).atan2(b.z - a.z));
        }
        Ok(Self {
            points,
            lengths,
            headings,
            index: 0,
            along: 0.0,
        })
    }

    /// Moves `d` along (`0x401150`).
    fn advance(&mut self, d: f32) {
        self.along += d;
        let mut guard = 0;
        while self.along >= self.lengths[self.index] && guard < self.points.len() {
            self.along -= self.lengths[self.index];
            self.index = (self.index + 1) % self.points.len();
            guard += 1;
        }
    }

    /// Position and heading here (`0x401260`).
    fn place(&self) -> (Vec3, f32) {
        let n = self.points.len();
        let i = self.index;
        let f = if self.lengths[i] > 0.0 {
            self.along / self.lengths[i]
        } else {
            0.0
        };
        let (next, mut h1) = if i + 1 < n {
            (self.points[i + 1], self.headings[i + 1])
        } else {
            (self.points[i], self.headings[0])
        };
        let mut h0 = self.headings[i];
        if (h0 - h1).abs() > std::f32::consts::PI {
            if h0 < 1.0 {
                h0 += std::f32::consts::TAU;
            } else {
                h0 -= std::f32::consts::TAU;
            }
        }
        let _ = &mut h1;
        (self.points[i].lerp(next, f), h0 * (1.0 - f) + h1 * f)
    }
}

struct Car {
    root: u32,
    lane: Lane,
    /// Sideways and upward offset from the lane (car 0's are -4 and 0.3).
    side: f32,
    lift: f32,
    wheels: [Option<u32>; 3],
    matrix: Mat4,
    heading: f32,
}

pub struct Demo {
    bof: Bof,
    track: u32,
    cars: Vec<Car>,
    posts: Vec<Vec3>,
    pictures: Pictures,
    /// 0 rear view, 1 chase, 2 follow, 3 track cameras, 4 front.
    view: usize,
    speed: f32,
    spin: f32,
    fog: bool,
    wireframe: bool,
    textures: bool,
    due: f64,
    frames: u64,
    triangles: usize,
}

impl Demo {
    pub fn new(_gpu: &Gpu, _gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let bof = Bof::parse(files.get("race.bof")?)?;
        let pe = Pe::new(files.get("race.exe")?)?;
        let find = |n: &str| bof.find(n).ok_or_else(|| format!("race.bof has no {n}"));
        let track = find("track_noroad")?;
        // Cars 0 to 2 take lanes 1 to 3 (`(i + 1) & 3`); car 3 a copy of
        // car 0's, which starts two units ahead.
        let mut lanes: Vec<Lane> = Vec::new();
        for i in [1, 2, 3, 1] {
            lanes.push(Lane::read(&pe, i)?);
        }
        lanes[0].advance(2.0);
        let mut cars = Vec::new();
        for (i, (name, lane)) in CARS.iter().zip(lanes).enumerate() {
            let root = find(name)?;
            let named = atb::graph::named(&bof, root);
            let get = |n: &str| named.iter().find(|(m, _)| m == n).map(|&(_, id)| id);
            cars.push(Car {
                root,
                lane,
                side: if i == 0 { -4.0 } else { 0.0 },
                lift: if i == 0 { 0.3 } else { 0.0 },
                wheels: [get("rollR"), get("rollL"), get("rear")],
                matrix: Mat4::IDENTITY,
                heading: 0.0,
            });
        }
        let posts = (0..8)
            .map(|k| {
                let p = POSTS + k * 12;
                Ok(Vec3::new(pe.float(p)?, pe.float(p + 4)?, pe.float(p + 8)?))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut demo = Self {
            bof,
            track,
            cars,
            posts,
            pictures: Pictures::default(),
            view: 1,
            speed: 70.0,
            spin: 0.0,
            fog: false,
            wireframe: false,
            textures: true,
            due: 0.0,
            frames: 0,
            triangles: 0,
        };
        demo.place();
        Ok(demo)
    }

    fn place(&mut self) {
        for car in &mut self.cars {
            let (mut p, h) = car.lane.place();
            let side = h - std::f32::consts::FRAC_PI_2;
            p += Vec3::new(side.sin() * car.side, car.lift, side.cos() * car.side);
            car.matrix = Mat4::from_translation(p) * Mat4::from_rotation_y(h);
            car.heading = h;
        }
    }

    fn tick(&mut self) {
        let d = self.speed / RATE as f32;
        for car in &mut self.cars {
            car.lane.advance(d);
        }
        self.place();
        // The wheels turn 3.373 degrees per unit of speed a frame.
        self.spin = (self.spin + self.speed * 3.373) % 360.0;
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
            return key.len() == 1 && "12345-+=fwtFWT".contains(key);
        }
        match key {
            "1" | "2" | "3" | "4" | "5" => self.view = key.parse::<usize>().unwrap_or(2) - 1,
            // Keypad minus and plus: ten slower or faster, 0 to 70.
            "-" => self.speed = (self.speed - 10.0).max(0.0),
            "+" | "=" => self.speed = (self.speed + 10.0).min(70.0),
            "f" | "F" => self.fog = !self.fog,
            "w" | "W" => self.wireframe = !self.wireframe,
            "t" | "T" => self.textures = !self.textures,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}

    pub fn wheel(&mut self, _amount: f64) {}

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let (p, _) = self.cars[0].lane.place();
        format!(
            "view {} speed {} car {:.0},{:.0},{:.0} tris {} frame {}",
            self.view + 1,
            self.speed,
            p.x,
            p.y,
            p.z,
            self.triangles,
            self.frames
        )
    }

    /// The camera for the view (`0x402120`).
    fn camera(&self) -> Camera {
        let car = self.cars[0].matrix;
        let at = car.w_axis.truncate();
        let mut fov = std::f32::consts::FRAC_PI_3;
        let frame = match self.view {
            0 => {
                car * look_at(
                    Vec3::new(0.0, 1.38, 0.0),
                    Vec3::new(0.0, 1.38, 1.0),
                    Vec3::Y,
                )
            }
            2 => look_at(at, at + Vec3::new(0.0, 3.0, -10.0), Vec3::Y),
            3 => {
                // The nearest post, eight up, zoomed to keep the car in view.
                let post = self
                    .posts
                    .iter()
                    .min_by(|a, b| {
                        let d = |p: &&Vec3| Vec3::new(p.x - at.x, 8.0 - at.y, p.z - at.z).length();
                        d(a).total_cmp(&d(b))
                    })
                    .copied()
                    .unwrap_or(Vec3::ZERO);
                let eye = Vec3::new(post.x, 8.0, post.z);
                let radius = 4.0;
                fov = (radius / eye.distance(at)).atan() * 3.333_333;
                look_at(at, eye, Vec3::Y)
            }
            4 => car * look_at(Vec3::ZERO, Vec3::new(0.0, 3.0, 10.0), Vec3::Y),
            _ => {
                car * look_at(
                    Vec3::new(-2.0, 0.0, 0.0),
                    Vec3::new(-2.0, 3.0, -10.0),
                    Vec3::Y,
                )
            }
        };
        let mut c = Camera::new(frame);
        c.fov = fov;
        c.far = 8000.0;
        c
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let camera = self.camera();
        let eye = camera.eye();
        let mut items: Vec<Item> = Vec::new();
        atb::walk(
            &self.bof,
            self.track,
            &View {
                eye,
                ..View::default()
            },
            &mut items,
        );
        // Front wheels turn on their axle and steer; the rear only turns.
        let spin = self.spin.to_radians();
        let steer = self.cars[0].heading * 0.1;
        let axle = |c: Vec3, steer: f32| {
            Mat4::from_translation(c)
                * Mat4::from_rotation_y(steer)
                * Mat4::from_rotation_x(spin)
                * Mat4::from_translation(-c)
        };
        let poses = [
            axle(Vec3::new(0.901, 0.393, 1.655), steer),
            axle(Vec3::new(-0.901, 0.393, 1.655), steer),
            axle(Vec3::new(0.0, 0.43, -1.479), 0.0),
        ];
        // Only cars 0 and 3 are drawn (`0x402cd0`).
        for car in [&self.cars[0], &self.cars[3]] {
            let wheels = car.wheels;
            let pose = move |id: u32| wheels.iter().position(|&w| w == Some(id)).map(|i| poses[i]);
            let mut part = Vec::new();
            atb::walk(
                &self.bof,
                car.root,
                &View {
                    eye,
                    pose: &pose,
                    ..View::default()
                },
                &mut part,
            );
            for mut i in part {
                i.matrix = car.matrix * i.matrix;
                items.push(i);
            }
        }
        // A dim light from above and behind, and a bright one along +z
        // (`0x401d50`).
        let lights = [
            Light::directional(Vec3::splat(0.6), Vec3::new(0.0, -8.0, -2.0)),
            Light::directional(Vec3::splat(0.9), Vec3::Z),
        ];
        let frame = Frame {
            camera,
            lights: &lights,
            fog: self.fog.then_some(Fog {
                near: 60.0,
                far: 100.0,
                color: [0.3; 3],
            }),
            texture_image: &|_| None,
            extra: &[],
            pass: None,
            wireframe: self.wireframe,
            textures: self.textures,
            bilinear: true,
        };
        // Sky blue, or the fog's grey.
        let clear = if self.fog { [0.3; 3] } else { [0.2, 0.3, 0.5] };
        gl.begin([clear[0], clear[1], clear[2], 1.0]);
        self.triangles = render::draw(
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
