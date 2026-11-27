//! Time, cameras and input, shared by native and web builds.
use glam::{Mat4, Quat, Vec3};

use crate::demo;

/// A sound to start.
#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

#[derive(Clone, Copy)]
pub enum Camera {
    Movie,
    Free {
        yaw: f32,
        pitch: f32,
        distance: f32,
        pan: Vec3,
    },
}

pub struct Shot {
    /// The camera to the world.
    pub place: Mat4,
    pub fov: f32,
}

impl Camera {
    pub fn shot(&self, root: Vec3) -> Shot {
        let Camera::Free {
            yaw,
            pitch,
            distance,
            pan,
        } = *self
        else {
            // The demo's camera: eases around the boss [I].
            return Shot {
                place: Mat4::IDENTITY,
                fov: 36_f32.to_radians(),
            };
        };
        let turn = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        let eye = root + pan + turn * Vec3::new(0.0, 0.0, distance);
        Shot {
            place: Mat4::from_rotation_translation(turn, eye),
            fov: 36_f32.to_radians(),
        }
    }
}

pub struct Frame {
    /// Seconds since the start.
    pub local: f64,
    pub camera: Camera,
    /// How white the picture is.
    pub white: f32,
    pub logo: bool,
}

pub struct Show {
    time: f64,
    started: f64,
    last_wall: Option<f64>,
    touched: f64,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: Vec3,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    pub paused: bool,
    pub logo: bool,
}

const HOME: (f32, f32, f32) = (0.35, 0.05, 650.0);
const FARTHEST: f32 = 2000.0;
const NEAREST: f32 = 40.0;
/// Where the movie looks: the boss, 200 units up [I].
pub const ROOT: Vec3 = Vec3::new(0.0, 70.0, 0.0);

impl Default for Show {
    fn default() -> Self {
        Self::new()
    }
}

impl Show {
    pub fn new() -> Self {
        let mut show = Self {
            time: 0.0,
            started: 0.0,
            last_wall: None,
            touched: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            distance: 0.0,
            pan: Vec3::ZERO,
            drag: None,
            cues: Vec::new(),
            paused: false,
            logo: true,
        };
        show.home();
        show.cues.push(Cue {
            name: demo::SOUND,
            repeat: true,
        });
        show
    }

    fn home(&mut self) {
        (self.yaw, self.pitch, self.distance) = HOME;
        self.pan = Vec3::ZERO;
    }

    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        if self.paused {
            return;
        }
        self.time += dt;
        if self.time - self.touched > demo::IDLE {
            self.home();
        }
    }

    pub fn frame(&self) -> Frame {
        Frame {
            local: self.time - self.started,
            camera: Camera::Free {
                yaw: self.yaw + (self.time * 0.05) as f32,
                pitch: self.pitch,
                distance: self.distance,
                pan: self.pan,
            },
            white: 0.0,
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn key(&mut self, key: &str) -> bool {
        self.touched = self.time;
        match key {
            " " | "p" | "P" => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            "Home" => self.home(),
            _ => return false,
        }
        true
    }

    /// Stops at a moment. A key sets the rest up; an empty one leaves the movie.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, key: &str) {
        if !key.is_empty() {
            self.key(key);
        }
        self.time = self.started + seconds;
        self.turn(yaw as f32, pitch as f32);
        self.paused = true;
    }

    fn turn(&mut self, yaw: f32, pitch: f32) {
        self.yaw += yaw;
        let (low, high) = (-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_4);
        self.pitch = (self.pitch + pitch).clamp(low, high);
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.turn(
                -(x - px) as f32 * std::f32::consts::TAU,
                -(y - py) as f32 * std::f32::consts::PI,
            );
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn pan(&mut self, amount: f32) {
        self.pan.y = (self.pan.y + amount * self.distance * 0.002).clamp(-300.0, 300.0);
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount).exp()).clamp(NEAREST, FARTHEST);
    }
}
