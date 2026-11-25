//! Time, camera and input shared by native and web builds.
use crate::demo::{self, DEMO, Demo};
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
    radius: f32,
    yaw: f32,
    pitch: f32,
    distance: f32,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    item: usize,
    started: f64,
    steering: Vec3,
    ripple: bool,
    layers: u32,
    depth_vision: bool,
    forthcoming_voice: Option<(f64, &'static str)>,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub seconds: f32,
    pub local: f32,
    pub view: Mat4,
    pub item: usize,
    pub steering: Vec3,
    pub ripple: bool,
    pub layers: u32,
    pub depth_vision: bool,
    pub logo: bool,
}

impl Show {
    pub fn new(center: Vec3, radius: f32) -> Self {
        let (yaw, pitch, distance) = Self::home(radius);
        Self {
            time: 0.0,
            last_wall: None,
            center,
            radius,
            yaw,
            pitch,
            distance,
            drag: None,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            item: 0,
            started: 0.0,
            steering: Vec3::ZERO,
            ripple: true,
            layers: 8,
            depth_vision: false,
            forthcoming_voice: Some((0.95, "greetings")),
            paused: false,
            logo: true,
        }
    }

    fn home(radius: f32) -> (f32, f32, f32) {
        match DEMO {
            Demo::Bugs => (0.28, 0.32, radius * 2.2),
            Demo::Wolfman => (-1.9, 0.13, radius * 3.45),
            Demo::Squid => (0.52, 0.14, radius * 2.4),
            Demo::Chameleon => (-0.30, 0.23, radius * 2.75),
            Demo::Tidepool => (-0.58, 0.42, radius * 2.5),
            Demo::Zoltar => (0.08, 0.06, radius * 3.4),
        }
    }

    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        if !self.paused {
            self.time += dt;
            if self
                .forthcoming_voice
                .is_some_and(|(time, _)| self.time >= time)
                && let Some((_, name)) = self.forthcoming_voice.take()
            {
                self.cues.push(Cue {
                    name,
                    repeat: false,
                });
            }
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
            seconds: self.time as f32,
            local: (self.time - self.started).max(0.0) as f32,
            view: glam::camera::rh::view::look_at_mat4(eye, self.center, Vec3::Y),
            item: self.item,
            steering: self.steering,
            ripple: self.ripple,
            layers: self.layers,
            depth_vision: self.depth_vision,
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn key(&mut self, key: &str) -> bool {
        match key {
            "p" | "P" => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            "Home" => {
                (self.yaw, self.pitch, self.distance) = Self::home(self.radius);
                self.steering = Vec3::ZERO;
            }
            " " | "s" | "S" if DEMO == Demo::Zoltar => {
                self.item = (self.item + 1) % demo::CLIPS.len().max(1);
                self.started = self.time;
                if let Some(name) = demo::CLIPS
                    .get(self.item)
                    .and_then(|n| n.strip_suffix(".nmb"))
                {
                    let delay = match name {
                        "greetings" => 0.95,
                        "pleasure" => 2.05,
                        "bigman" => 1.9,
                        "getahead" | "approach" => 1.3,
                        "psst" => 2.3,
                        "yawn" => 1.75,
                        _ => 1.5,
                    };
                    self.forthcoming_voice = (name != "bored").then_some((self.time + delay, name));
                }
            }
            " " if DEMO == Demo::Chameleon => self.paused = !self.paused,
            "s" | "S" if DEMO == Demo::Chameleon => self.item = (self.item + 1) % 5,
            "z" | "Z" | "s" | "S" if DEMO == Demo::Tidepool => {
                self.ripple = !self.ripple;
                self.started = self.time;
                self.cues.push(Cue {
                    name: "tpdrip3",
                    repeat: false,
                });
            }
            "1" | "2" | "3" | "4" | "5" | "6" if DEMO == Demo::Wolfman => {
                self.item = (key.as_bytes()[0] - b'1') as usize;
                self.started = self.time;
                if self.item == 2 {
                    self.cues.push(Cue {
                        name: "howling 3",
                        repeat: false,
                    });
                }
            }
            "+" | "=" if DEMO == Demo::Wolfman => self.layers = (self.layers + 1).min(8),
            "-" if DEMO == Demo::Wolfman => self.layers = self.layers.saturating_sub(1),
            "ArrowLeft" if DEMO == Demo::Squid => self.steering.x -= self.radius * 0.035,
            "ArrowRight" if DEMO == Demo::Squid => self.steering.x += self.radius * 0.035,
            "ArrowUp" if DEMO == Demo::Squid => self.steering.z -= self.radius * 0.035,
            "ArrowDown" if DEMO == Demo::Squid => self.steering.z += self.radius * 0.035,
            "s" | "S" if DEMO == Demo::Squid => {
                self.steering.z -= self.radius * 0.1;
                self.cues.push(Cue {
                    name: "speedup",
                    repeat: false,
                });
            }
            "d" | "D" if DEMO == Demo::Bugs => self.depth_vision = !self.depth_vision,
            "s" | "S" if DEMO == Demo::Bugs => {
                self.item ^= 1;
                self.cues.push(Cue {
                    name: "bug_screech1",
                    repeat: false,
                });
            }
            " " => self.paused = !self.paused,
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
            self.key(if DEMO == Demo::Wolfman { "3" } else { "s" });
            self.started = 0.0;
        }
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.yaw += (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(-1.3, 1.42);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance =
            (self.distance * (-amount).exp()).clamp(self.radius * 0.25, self.radius * 9.0);
    }
}
