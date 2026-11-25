//! Playback time, orbit controls, and the demo-specific cues.
use glam::{Mat4, Vec3};

use crate::demo;

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

pub struct Show {
    time: f64,
    last_wall: Option<f64>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    radius: f32,
    center: Vec3,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    action_time: Option<f64>,
    castle: usize,
    transition: Option<Transition>,
    snow_started: f64,
    inside: bool,
    show_camera: bool,
    pub paused: bool,
    pub logo: bool,
}

struct Transition {
    started: f64,
    inside: bool,
    castle: usize,
}

pub struct Frame {
    pub seconds: f32,
    pub view: Mat4,
    pub action: Option<f32>,
    pub castle: usize,
    pub inside: bool,
    pub exposure: f32,
    pub snow_time: f32,
    pub logo: bool,
}

impl Show {
    pub fn new(center: Vec3, radius: f32) -> Self {
        let (yaw, pitch, distance) = Self::home(radius);
        Self {
            time: 0.0,
            last_wall: None,
            yaw,
            pitch,
            distance,
            center,
            radius,
            drag: None,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            action_time: None,
            castle: 0,
            transition: None,
            snow_started: 0.0,
            inside: false,
            show_camera: !demo::CRYSTAL,
            paused: false,
            logo: true,
        }
    }

    fn home(radius: f32) -> (f32, f32, f32) {
        if demo::CRYSTAL {
            (0.35, 0.10, radius * 2.9)
        } else {
            (0.26, 0.23, 96.0)
        }
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
        if let Some(transition) = &self.transition {
            let elapsed = self.time - transition.started;
            if elapsed >= 1.0
                && (self.inside, self.castle) != (transition.inside, transition.castle)
            {
                self.inside = transition.inside;
                self.castle = transition.castle;
                self.cue(self.music(), true);
            }
            if elapsed >= 2.0 {
                self.transition = None;
            }
        }
        if !demo::CRYSTAL
            && self
                .action_time
                .is_some_and(|start| self.time - start > 14.0)
        {
            self.action_time = None;
            self.cue("march", true);
        }
    }

    pub fn frame(&self) -> Frame {
        let action = self
            .action_time
            .map(|start| (self.time - start).max(0.0) as f32);
        let (mut yaw, mut pitch, mut distance) = (self.yaw, self.pitch, self.distance);
        if self.show_camera {
            yaw += self.time as f32 * 0.085;
            pitch += (self.time as f32 * 0.28).sin() * 0.11;
            distance *= 0.88 + 0.12 * (self.time as f32 * 0.18).cos();
        }
        let orbit = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        let eye = self.center + distance * orbit;
        let exposure = if demo::CRYSTAL {
            self.transition.as_ref().map_or(1.0, |transition| {
                ((self.time - transition.started) as f32 - 1.0)
                    .abs()
                    .clamp(0.06, 1.0)
            })
        } else {
            1.0
        };
        Frame {
            seconds: self.time as f32,
            view: glam::camera::rh::view::look_at_mat4(eye, self.center, Vec3::Y),
            action,
            castle: self.castle,
            inside: self.inside,
            exposure,
            snow_time: (self.time - self.snow_started).max(0.0) as f32,
            logo: self.logo,
        }
    }

    fn cue(&mut self, name: &'static str, repeat: bool) {
        self.cues.push(Cue { name, repeat });
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn key(&mut self, key: &str) -> bool {
        match key {
            "p" | "P" => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            "Home" => (self.yaw, self.pitch, self.distance) = Self::home(self.radius),
            " " if demo::CRYSTAL => self.change(!self.inside, self.castle),
            "s" | "S" if demo::CRYSTAL => self.change(self.inside, self.castle ^ 1),
            "r" | "R" if demo::CRYSTAL => self.snow_started = self.time,
            " " => {
                self.action_time = Some(self.time);
                self.cue("panic", true);
                self.cue("scary", false);
            }
            "s" | "S" => {
                self.action_time = None;
                self.cue("march", true);
            }
            _ => return false,
        }
        true
    }

    fn change(&mut self, inside: bool, castle: usize) {
        self.transition = Some(Transition {
            started: self.time,
            inside,
            castle,
        });
        self.cue(
            if castle == 1 && self.castle != castle {
                "thunder"
            } else {
                "transition"
            },
            false,
        );
    }

    fn music(&self) -> &'static str {
        if !self.inside {
            "crystal_ball"
        } else if self.castle == 0 {
            "light_castle"
        } else {
            "dark_castle"
        }
    }

    /// Set the camera and time exactly for the native and web frame checks.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        self.time = seconds;
        self.yaw += yaw as f32;
        self.pitch += pitch as f32;
        self.show_camera = false;
        self.paused = true;
        if action {
            if demo::CRYSTAL {
                self.inside = true;
            } else {
                self.action_time = Some(0.0);
            }
        }
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
        self.show_camera = false;
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((previous_x, previous_y)) = self.drag {
            self.yaw += (x - previous_x) as f32 * std::f32::consts::TAU;
            self.pitch =
                (self.pitch + (y - previous_y) as f32 * std::f32::consts::PI).clamp(-0.04, 1.48);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance =
            (self.distance * (-amount).exp()).clamp(self.radius * 0.11, self.radius * 9.0);
        self.show_camera = false;
    }
}
