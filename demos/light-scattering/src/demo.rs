//! Rendering Outdoor Light Scattering in Real Time (`App.exe`, ATI and Westwood, GDC 2002;
//! addresses are in it). A height field under a sky dome, both lit by the scattering vertex
//! program the demo ships as `ScatterVS10.vsh`, and a movie of 64 keys that moves the camera,
//! the sun and the scattering settings.
use glam::{Mat4, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{Depth, Gl, Gpu, MeshId, REPEAT, State, Surface, TextureId, Vertex};
use crate::image::Image;

pub const FILES: &[&str] = &[
    "hf129_3.raw",
    "movie.dat",
    "terrain3.tga",
    "clouds.bmp",
    "pnh3.dds",
    "nnh3.dds",
];
/// Files that are parts of a program: name, address, length.
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["sky", "terrain"];
pub const BUTTONS: &[(&str, &str)] = &[(" ", "Play"), ("r", "Restart")];

const SKY: usize = 0;
const TERRAIN: usize = 1;

/// The original's sliders (0x402ff0): name, lowest, highest. The demo's dialog shows each
/// as a number from 0 to 100.
pub const SLIDERS: &[(&str, f32, f32)] = &[
    ("Sun", 0.0, 1.0),
    ("Sun intensity", 0.0, 100.0),
    ("Rayleigh", 0.0, 1.0),
    ("Mie", 0.0, 1.0),
    ("Henyey-Greenstein g", 0.0, 1.0),
    ("Inscattering", 0.0, 1.0),
];

/// The height field (0x404330): 129 by 129 bytes, 20 units apart.
const SIDE: usize = 129;
const CELL: f32 = 20.0;
/// The sky dome (0x404da0, 0x4050b0): radii across and up, slices and rings, and how far
/// down from the top it reaches. It follows the camera, this far below it (0x406dc0).
const DOME: (f32, f32) = (2580.0, 2000.0);
const SLICES: usize = 128;
const RINGS: usize = 64;
const DOME_ANGLE: f32 = std::f32::consts::TAU / 3.0;
const DOME_DROP: f32 = -200.0;
/// The camera (0x403580): vertical field of view in radians for a view 2.2 times as wide
/// as high, near, far, step and turn for each frame.
const FOV: f32 = 0.78;
const ASPECT: f32 = 2.2;
const NEAR: f32 = 1.0;
const FAR: f32 = 50_000.0;
const STEP: f32 = 5.0;
const TURN: f32 = 0.005;
/// What of the light the ground sends back (0x406dc0, constant 13).
const REFLECTANCE: [f32; 4] = [0.0138, 0.0113, 0.008, 0.0];
/// Texture coordinates for each unit (0x406c60): one picture over the whole field.
const SPAN: f32 = 1.0 / 2560.0;
/// The clouds move this far each frame and start half way (0x406dc0).
const CLOUD_STEP: f32 = 0.0025;
const CLOUD_START: f32 = 0.5;
/// One step is a frame of the original, which is taken as 60 a second.
const FRAME: f64 = 1.0 / 60.0;
/// Wavelengths of red, green and blue in metres and micrometres (0x403200, 0x4063a0).
const WAVES: [f32; 3] = [650e-9, 570e-9, 475e-9];
/// Turbidity of the air the sun shines through (0x4064e0).
const TURBIDITY: f32 = 2.0;

/// Everything a key of the movie holds, in the order of `Movie.dat` (0x405ed0): the camera's
/// place, direction and up; Rayleigh and Mie multipliers, inscattering and extinction
/// multipliers, Henyey-Greenstein g (0x403440); the sun's intensity, angle from the zenith
/// and turn about it (0x4065e0).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look([f32; 17]);

impl Look {
    fn at(&self) -> Vec3 {
        Vec3::from_slice(&self.0[0..3])
    }
    fn ahead(&self) -> Vec3 {
        Vec3::from_slice(&self.0[3..6])
    }
    fn up(&self) -> Vec3 {
        Vec3::from_slice(&self.0[6..9])
    }
    fn rayleigh(&self) -> f32 {
        self.0[9]
    }
    fn mie(&self) -> f32 {
        self.0[10]
    }
    fn inscattering(&self) -> f32 {
        self.0[11]
    }
    fn g(&self) -> f32 {
        self.0[13]
    }
    fn intensity(&self) -> f32 {
        self.0[14]
    }
    fn sun_angles(&self) -> (f32, f32) {
        (self.0[15], self.0[16])
    }

    /// 0x406340.
    fn sun(&self) -> Vec3 {
        let (theta, phi) = self.sun_angles();
        Vec3::new(
            phi.cos() * theta.sin(),
            theta.cos(),
            phi.sin() * theta.sin(),
        )
    }

    /// The one slider for the sun (0x402fc0): 0 rises in -z, 0.5 is noon, 1 sets in +z.
    fn sun_slider(&self) -> f32 {
        let (theta, phi) = self.sun_angles();
        match phi >= std::f32::consts::PI {
            true => 0.5 - theta / std::f32::consts::PI,
            false => theta / std::f32::consts::PI + 0.5,
        }
    }
    fn set_sun_slider(&mut self, s: f32) {
        use std::f32::consts::{FRAC_PI_2, PI};
        let (theta, phi) = match s < 0.5 {
            true => ((0.5 - s) * PI, 3.0 * FRAC_PI_2),
            false => ((s - 0.5) * PI, FRAC_PI_2),
        };
        (self.0[15], self.0[16]) = (theta, phi);
    }

    fn slider(&self, i: usize) -> f32 {
        match i {
            0 => self.sun_slider(),
            1 => self.intensity(),
            2 => self.rayleigh(),
            3 => self.mie(),
            4 => self.g(),
            _ => self.inscattering(),
        }
    }
    fn set_slider(&mut self, i: usize, v: f32) {
        let (_, low, high) = SLIDERS[i.min(SLIDERS.len() - 1)];
        let v = v.clamp(low, high);
        match i {
            0 => self.set_sun_slider(v),
            1 => self.0[14] = v,
            2 => self.0[9] = v,
            3 => self.0[10] = v,
            4 => self.0[13] = v,
            _ => self.0[11] = v,
        }
    }
}

/// `Movie.dat` (0x4049b0): tracks, then for each its keys and a flag, then a line for each
/// key: the frame it is reached on and the 17 numbers.
struct Movie {
    frames: Vec<f32>,
    keys: Vec<Look>,
}

impl Movie {
    fn read(text: &str) -> Result<Self, Error> {
        let mut words = text.split_ascii_whitespace();
        let mut number = || -> Result<f32, Error> {
            let w = words.next().ok_or("Movie.dat: cut short")?;
            Ok(w.parse::<f32>()
                .map_err(|e| format!("Movie.dat: {w}: {e}"))?)
        };
        let tracks = number()? as usize;
        if tracks == 0 {
            return Err("Movie.dat: no track".into());
        }
        let count = number()? as usize;
        let _flag = number()?;
        let (mut frames, mut keys) = (Vec::new(), Vec::new());
        for _ in 0..count {
            frames.push(number()?.trunc());
            let mut look = [0.0; 17];
            for v in &mut look {
                *v = number()?;
            }
            keys.push(Look(look));
        }
        if keys.len() < 4 {
            return Err("Movie.dat: fewer than four keys".into());
        }
        Ok(Self { frames, keys })
    }

    fn length(&self) -> f32 {
        *self.frames.last().expect("four keys")
    }

    /// Where `frame` falls among the keys, as a share of them (0x401240).
    fn share(&self, frame: f32) -> f32 {
        let n = self.frames.len();
        for i in 1..n {
            if frame < self.frames[i] {
                let part = (frame - self.frames[i - 1]) / (self.frames[i] - self.frames[i - 1]);
                return (part + (i - 1) as f32) / (n - 1) as f32;
            }
        }
        1.0
    }

    /// A Catmull-Rom spline through four keys (0x4055e0). The share is spread over the
    /// spline's n - 3 pieces, not the n - 1 gaps between keys, as in the original.
    fn at(&self, frame: f32) -> Look {
        let n = self.keys.len();
        let s = self.share(frame).clamp(0.0, 1.0) * (n - 3) as f32;
        let piece = (s.floor() as usize).min(n - 4);
        let t = s - piece as f32;
        let [p0, p1, p2, p3] = std::array::from_fn(|k| &self.keys[piece + k].0);
        let (t2, t3) = (t * t, t * t * t);
        Look(std::array::from_fn(|i| {
            let (a, b, c, d) = (p0[i], p1[i], p2[i], p3[i]);
            (-0.5 * a + 1.5 * b - 1.5 * c + 0.5 * d) * t3
                + (a - 2.5 * b + 2.0 * c - 0.5 * d) * t2
                + (-0.5 * a + 0.5 * c) * t
                + b
        }))
    }
}

/// The sun's color after the air between it and the ground (0x4063a0): Preetham's
/// Rayleigh and aerosol transmittance for the air mass at its angle from the zenith.
fn sun_color(theta: f32) -> Vec3 {
    let beta = 0.046_083_66 * TURBIDITY - 0.045_860_26;
    let degrees = theta / std::f32::consts::PI * 180.0;
    let mass = 1.0 / (theta.cos() + 0.15 * (93.885 - degrees).powf(-1.253));
    Vec3::from_array(WAVES.map(|w| {
        let um = w * 1e6;
        let rayleigh = (-mass * 0.008_735 * um.powf(-4.08)).exp();
        let aerosol = (-mass * beta * um.powf(-1.3)).exp();
        rayleigh * aerosol
    }))
}

/// Scattering coefficients for each wavelength (0x403200): Rayleigh total and angular,
/// Mie total and angular, before the sliders' multipliers.
struct Betas {
    rayleigh: Vec3,
    rayleigh_angular: Vec3,
    mie: Vec3,
    mie_angular: Vec3,
}

fn betas() -> Betas {
    let inv = WAVES.map(|w| 1.0 / w);
    let [a, b, c] = inv.map(|i| i * i);
    let squared = Vec3::new(a, b, c);
    let fourth = squared * squared;
    Betas {
        rayleigh: fourth * 1.244_461_4e-28,
        rayleigh_angular: fourth * 7.427_331_5e-30,
        mie: squared * Vec3::new(0.685, 0.679, 0.67) * 3.540_733_5e-15,
        mie_angular: squared * 5.635_252_5e-16,
    }
}

/// `D3DXMatrixLookAtLH` (0x40cd59).
fn look_at_lh(eye: Vec3, at: Vec3, up: Vec3) -> Mat4 {
    let z = (at - eye).normalize();
    let x = up.cross(z).normalize();
    let y = z.cross(x);
    Mat4::from_cols(
        Vec4::new(x.x, y.x, z.x, 0.0),
        Vec4::new(x.y, y.y, z.y, 0.0),
        Vec4::new(x.z, y.z, z.z, 0.0),
        Vec4::new(-x.dot(eye), -y.dot(eye), -z.dot(eye), 1.0),
    )
}

/// `D3DXMatrixPerspectiveFovLH` (0x40cf21): depth from 0 at `near` to 1 at `far`.
fn perspective_lh(fov: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let y = 1.0 / (fov * 0.5).tan();
    let q = far / (far - near);
    Mat4::from_cols(
        Vec4::new(y / aspect, 0.0, 0.0, 0.0),
        Vec4::new(0.0, y, 0.0, 0.0),
        Vec4::new(0.0, 0.0, q, 1.0),
        Vec4::new(0.0, 0.0, -near * q, 0.0),
    )
}

fn point(p: Vec3) -> Vertex {
    Vertex::new(p.extend(1.0), [1.0; 4])
}

/// Two triangles for each cell of the field (0x4044d0 draws it as one strip).
fn terrain(heights: &[u8]) -> Result<Vec<Vertex>, Error> {
    if heights.len() < SIDE * SIDE {
        return Err("hf129_3.raw: cut short".into());
    }
    let at = |i: usize, j: usize| {
        let h = f32::from(heights[j * SIDE + i]);
        point(Vec3::new(i as f32 * CELL, h, j as f32 * CELL))
    };
    let mut v = Vec::with_capacity((SIDE - 1) * (SIDE - 1) * 6);
    for j in 0..SIDE - 1 {
        for i in 0..SIDE - 1 {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            v.extend([a, c, b, b, c, d]);
        }
    }
    Ok(v)
}

/// The dome, slice by slice from its top (0x4050b0).
fn dome() -> Vec<Vertex> {
    let at = |slice: usize, ring: usize| {
        let phi = slice as f32 / SLICES as f32 * std::f32::consts::TAU;
        let theta = ring as f32 / RINGS as f32 * DOME_ANGLE;
        let p = Vec3::new(
            DOME.0 * phi.cos() * theta.sin(),
            DOME.1 * theta.cos(),
            DOME.0 * phi.sin() * theta.sin(),
        );
        point(p)
    };
    let mut v = Vec::with_capacity(SLICES * RINGS * 6);
    for s in 0..SLICES {
        for r in 0..RINGS {
            let (a, b, c, d) = (at(s, r), at(s + 1, r), at(s, r + 1), at(s + 1, r + 1));
            v.extend([a, c, b, b, c, d]);
        }
    }
    v
}

pub struct Demo {
    movie: Movie,
    betas: Betas,
    /// What is shown: the movie's moment, or the camera and sliders as last left.
    look: Look,
    frame: f32,
    playing: bool,
    cloud: f32,
    due: f64,
    held: Vec<String>,
    /// Steps of the camera, which F5 and F6 change.
    step: f32,
    mouse: Option<(f32, f32)>,
    sky: MeshId,
    ground: MeshId,
    color: TextureId,
    clouds: TextureId,
    /// Normals and horizons for a sun in +z, and in -z (0x4069a0).
    horizons: [TextureId; 2],
    aspect: f32,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let movie = Movie::read(&String::from_utf8_lossy(files.get("movie.dat")?))?;
        let mut texture = |name: &str, flip: bool| -> Result<TextureId, Error> {
            let image: Image = files.image(name)?;
            let image = if flip { image.flipped() } else { image };
            Ok(gl.texture(gpu, &image, REPEAT))
        };
        // Targa pictures start from their bottom row; Direct3D's textures from the top.
        let color = texture("terrain3.tga", true)?;
        let clouds = texture("clouds.bmp", false)?;
        let horizons = [texture("pnh3.dds", false)?, texture("nnh3.dds", false)?];
        let ground = gl.mesh(gpu, &terrain(files.get("hf129_3.raw")?)?);
        let sky = gl.mesh(gpu, &dome());
        let look = movie.at(0.0);
        Ok(Self {
            movie,
            betas: betas(),
            look,
            frame: 0.0,
            playing: true,
            cloud: CLOUD_START,
            due: 0.0,
            held: Vec::new(),
            step: STEP,
            mouse: None,
            sky,
            ground,
            color,
            clouds,
            horizons,
            aspect: ASPECT,
        })
    }

    pub fn step(&mut self, dt: f64) {
        self.due += dt;
        while self.due >= FRAME - 1e-9 {
            self.due -= FRAME;
            self.tick();
        }
    }

    /// One frame: the movie goes on a frame (0x401320) and the clouds move (0x406dc0).
    fn tick(&mut self) {
        self.cloud += CLOUD_STEP;
        if self.cloud > 8.0 {
            self.cloud -= 8.0;
        }
        if self.playing {
            self.look = self.movie.at(self.frame);
            self.frame += 1.0;
            // The original stops at the end; this one starts again.
            if self.frame >= self.movie.length() {
                self.frame = 0.0;
            }
        }
        let held = self.held.clone();
        for k in &held {
            self.fly(k);
        }
    }

    /// The original's keys for the camera (Readme.txt): each frame one is held.
    fn fly(&mut self, key: &str) {
        let ahead = self.look.ahead().normalize_or(Vec3::Z);
        let up = self.look.up().normalize_or(Vec3::Y);
        let side = up.cross(ahead).normalize_or(Vec3::X);
        let turn =
            |about: Vec3, by: f32, v: Vec3| Mat4::from_axis_angle(about, by).transform_vector3(v);
        let (mut at, mut ahead2, mut up2) = (self.look.at(), ahead, up);
        match key {
            "ArrowUp" => at += ahead * self.step,
            "ArrowDown" => at -= ahead * self.step,
            "ArrowLeft" => ahead2 = turn(up, -TURN, ahead),
            "ArrowRight" => ahead2 = turn(up, TURN, ahead),
            "PageUp" => (ahead2, up2) = (turn(side, -TURN, ahead), turn(side, -TURN, up)),
            "PageDown" => (ahead2, up2) = (turn(side, TURN, ahead), turn(side, TURN, up)),
            "Home" => at += up * self.step,
            "End" => at -= up * self.step,
            "F9" => up2 = turn(ahead, TURN, up),
            "F11" => up2 = turn(ahead, -TURN, up),
            _ => return,
        }
        self.look.0[0..3].copy_from_slice(&at.to_array());
        self.look.0[3..6].copy_from_slice(&ahead2.to_array());
        self.look.0[6..9].copy_from_slice(&up2.to_array());
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.tick());
    }

    const FLY: &[&str] = &[
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "PageUp",
        "PageDown",
        "Home",
        "End",
        "F9",
        "F11",
    ];

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if Self::FLY.contains(&key) {
            self.held.retain(|k| k != key);
            if down {
                // The camera is the user's until the movie plays again.
                self.playing = false;
                self.held.push(key.to_string());
            }
            return true;
        }
        if !down {
            return false;
        }
        match key {
            " " | "p" => self.playing = !self.playing,
            "r" => {
                self.frame = 0.0;
                self.look = self.movie.at(0.0);
                self.playing = true;
            }
            "F5" => self.step = (self.step * 0.5).max(0.25),
            "F6" => self.step = (self.step * 2.0).min(320.0),
            // Not in the original, which moves the sun with a slider.
            "," | "." => {
                let by = if key == "," { -0.01 } else { 0.01 };
                self.playing = false;
                self.set_slider(0, self.slider(0) + by);
            }
            _ => return false,
        }
        true
    }

    /// Not in the original: dragging turns the camera.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        let at = (x as f32, y as f32);
        match (self.mouse, buttons & 1 != 0) {
            (Some(last), true) => {
                let (dx, dy) = (at.0 - last.0, at.1 - last.1);
                let ahead = self.look.ahead().normalize_or(Vec3::Z);
                let up = self.look.up().normalize_or(Vec3::Y);
                let side = up.cross(ahead).normalize_or(Vec3::X);
                let turn = Mat4::from_axis_angle(Vec3::Y, dx * 2.0)
                    * Mat4::from_axis_angle(side, dy * 1.5);
                let (a, u) = (turn.transform_vector3(ahead), turn.transform_vector3(up));
                self.look.0[3..6].copy_from_slice(&a.to_array());
                self.look.0[6..9].copy_from_slice(&u.to_array());
                self.playing = false;
                self.mouse = Some(at);
            }
            (None, true) => self.mouse = Some(at),
            (_, false) => self.mouse = None,
        }
    }

    /// Not in the original: forward and back.
    pub fn wheel(&mut self, amount: f64) {
        let at = self.look.at() + self.look.ahead().normalize_or(Vec3::Z) * amount as f32 * 40.0;
        self.look.0[0..3].copy_from_slice(&at.to_array());
        self.playing = false;
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn slider(&self, i: usize) -> f32 {
        self.look.slider(i)
    }

    /// Moving a slider stops the movie, which would set it again.
    pub fn set_slider(&mut self, i: usize, v: f32) {
        self.playing = false;
        self.look.set_slider(i, v);
    }

    pub fn report(&self) -> String {
        let p = self.look.at();
        format!(
            "frame {} of {}, {}, sun {:.3}, camera {:.0} {:.0} {:.0}",
            self.frame as u32,
            self.movie.length() as u32,
            if self.playing { "playing" } else { "paused" },
            self.look.sun_slider(),
            p.x,
            p.y,
            p.z,
        )
    }

    /// The vertex program's numbers (0x406dc0), in its registers, and the pixel program's
    /// sun in slot 14, which the vertex program does not use (0x406a30).
    fn constants(&self, world: Mat4, view: Mat4, projection: Mat4) -> Surface {
        let l = &self.look;
        let b = &self.betas;
        let rayleigh = b.rayleigh * l.rayleigh();
        let mie = b.mie * l.mie();
        let total = rayleigh + mie;
        let sun = l.sun();
        let color = sun_color(l.sun_angles().0);
        let g = l.g();
        let mut s = Surface::default();
        let put = |s: &mut Surface, at: usize, m: Mat4| {
            s.params[at..at + 4].copy_from_slice(&m.to_cols_array_2d());
        };
        put(&mut s, 0, projection * view * world);
        put(&mut s, 4, view * world);
        put(&mut s, 8, world);
        s.params[12] = sun.extend(1.0).to_array();
        s.params[13] = REFLECTANCE;
        // The pixel program's c1: the sun as the normal map holds directions (x, z, y), and
        // how high it stands over the side its horizons were taken for.
        s.params[14] = [sun.x, sun.z, sun.y, sun.z.abs()];
        s.params[15] = color.extend(l.intensity()).to_array();
        s.params[16] = [SPAN, SPAN, 0.0, 0.0];
        s.params[18] = [SPAN, SPAN, self.cloud, 0.0];
        s.params[22] = l.at().extend(1.0).to_array();
        s.params[23] = total.recip().extend(0.0).to_array();
        s.params[24] = total.extend(0.0).to_array();
        s.params[25] = [l.0[12], l.inscattering(), 0.0, 0.0];
        s.params[26] = (b.mie_angular * l.mie()).extend(0.0).to_array();
        s.params[27] = (b.rayleigh_angular * l.rayleigh()).extend(0.0).to_array();
        // As the original has it: 1 + g where the formula has 1 + g².
        s.params[28] = [1.0 - g * g, 1.0 + g, 2.0 * g, 0.0];
        s.params[29] = mie.extend(0.0).to_array();
        s.params[30] = rayleigh.extend(0.0).to_array();
        s.params[31] = [1.0, std::f32::consts::LOG2_E, 0.5, 0.0];
        s
    }

    /// Keeps the original's view where the window is narrower, and its height where wider.
    fn projection(&self) -> Mat4 {
        let fov = match self.aspect < ASPECT {
            true => 2.0 * ((FOV * 0.5).tan() * ASPECT / self.aspect).atan(),
            false => FOV,
        };
        perspective_lh(fov, self.aspect, NEAR, FAR)
    }

    /// 0x404ec0: the sky without writing depth, then the ground; nothing is culled.
    pub fn draw(&mut self, _gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        self.aspect = w.max(1) as f32 / h.max(1) as f32;
        let l = &self.look;
        let view = look_at_lh(l.at(), l.at() + l.ahead(), l.up());
        let projection = self.projection();
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        let under = Mat4::from_translation(Vec3::new(l.at().x, DOME_DROP, l.at().z));
        let sky = self.constants(under, view, projection);
        let sky_state = State::new(SKY)
            .vertex("scatter")
            .depth(Depth::LessEqual(false));
        gl.draw_mesh(sky_state, &sky, self.sky);
        let mut ground = self.constants(Mat4::IDENTITY, view, projection);
        let side = usize::from(self.look.sun().z <= 0.0);
        ground.textures = [
            self.horizons[side],
            self.color,
            self.clouds,
            crate::gl::WHITE,
        ];
        let ground_state = State::new(TERRAIN)
            .vertex("scatter")
            .depth(Depth::LessEqual(true));
        gl.draw_mesh(ground_state, &ground, self.ground);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_passes_through_keys() {
        let keys: Vec<Look> = (0..6).map(|i| Look([i as f32; 17])).collect();
        let movie = Movie {
            frames: (0..6).map(|i| i as f32 * 40.0).collect(),
            keys,
        };
        // Straight lines stay straight, though the share is spread over three pieces.
        let mid = movie.at(100.0).0[0];
        assert!((mid - (1.0 + 2.5 / 5.0 * 3.0)).abs() < 1e-4, "{mid}");
        assert!((movie.at(0.0).0[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn noon_sun_is_white_ish_and_low_sun_is_red() {
        let noon = sun_color(0.0);
        let low = sun_color(1.5);
        assert!(noon.min_element() > 0.6, "{noon}");
        assert!(low.x > low.z * 2.0, "{low}");
    }

    #[test]
    fn sun_slider_goes_both_ways() {
        let mut l = Look([0.0; 17]);
        for s in [0.1, 0.3, 0.5, 0.7, 0.95] {
            l.set_sun_slider(s);
            assert!((l.sun_slider() - s).abs() < 1e-5);
        }
    }
}
