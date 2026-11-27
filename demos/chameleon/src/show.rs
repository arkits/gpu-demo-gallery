//! Time, camera and input shared by native and web builds.
use crate::demo;
use crate::stage::Skins;
use glam::{Mat4, Vec3};

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

pub struct Show {
    time: f64,
    last_wall: Option<f64>,
    center: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    shown: usize,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub ms: f64,
    pub skins: Skins,
    pub view: Mat4,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub logo: bool,
}

/// Yaw, pitch and distance. The viewer starts 25 away and allows 10 to 80 (0x4029d0).
const HOME: (f32, f32, f32) = (-0.3, 0.23, 25.0);
const DISTANCES: (f32, f32) = (10.0, 80.0);

impl Show {
    pub fn new(center: Vec3) -> Self {
        Self {
            time: 0.0,
            last_wall: None,
            center,
            yaw: HOME.0,
            pitch: HOME.1,
            distance: HOME.2,
            drag: None,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            shown: 0,
            paused: false,
            logo: true,
        }
    }

    fn skins(&self) -> Skins {
        Skins::at(self.time * 1000.0)
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
        let now = self.skins().new;
        if now != self.shown {
            self.shown = now;
            self.cues.push(Cue {
                name: demo::CHANGE,
                repeat: false,
            });
        }
    }

    pub fn frame(&self) -> Frame {
        let orbit = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let eye = self.center + self.distance * orbit;
        Frame {
            ms: self.time * 1000.0,
            skins: self.skins(),
            view: glam::camera::rh::view::look_at_mat4(eye, self.center, Vec3::Y),
            fov: 0.95,
            // Leaves fade by these (0x4029d0).
            near: 1.0,
            far: 100.0,
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " | "p" | "P" => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            "Home" => (self.yaw, self.pitch, self.distance) = HOME,
            // On to the next skin, at its start.
            "s" | "S" => {
                let turn = self.skins().turn;
                self.time = ((turn + 1) as f64 * demo::PERIOD - demo::LEAD) / 1000.0;
            }
            _ => return false,
        }
        true
    }

    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        self.time = seconds;
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(-1.3, 1.42);
        self.paused = true;
        if action {
            self.key("s");
            self.time += 1.5;
        }
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.yaw -= (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(-1.3, 1.42);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount).exp()).clamp(DISTANCES.0, DISTANCES.1);
    }
}
