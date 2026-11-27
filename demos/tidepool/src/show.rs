//! Time, camera and input shared by native and web builds.
use crate::demo;
use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

/// Something that touches the water.
#[derive(Debug, Clone, Copy)]
pub struct Touch {
    /// When, in seconds.
    pub time: f32,
    pub at: Vec2,
    pub radius: f32,
    pub strength: f32,
}

pub struct Show {
    time: f64,
    last_wall: Option<f64>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    drag: Option<(f64, f64)>,
    /// How far the pointer went since it was pressed.
    moved: f64,
    cues: Vec<Cue>,
    touches: Vec<Touch>,
    drips: u32,
    z_correct: bool,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub seconds: f32,
    /// All so far, in order.
    pub touches: Vec<Touch>,
    pub z_correct: bool,
    pub view: Mat4,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub logo: bool,
}

const HOME: (f32, f32, f32) = (-0.75, 0.34, 19.0);
const CENTER: Vec3 = Vec3::new(0.0, 1.2, 0.0);
const FOV: f32 = 0.8;
/// Seconds between drips, and what remains to be drawn of old ones.
const DRIPS: f64 = 1.6;

impl Default for Show {
    fn default() -> Self {
        Self {
            time: 0.0,
            last_wall: None,
            yaw: HOME.0,
            pitch: HOME.1,
            distance: HOME.2,
            drag: None,
            moved: 0.0,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            touches: Vec::new(),
            drips: 0,
            z_correct: true,
            paused: false,
            logo: true,
        }
    }
}

impl Show {
    /// Drips from the cave's roof that fell until now.
    fn drip(&mut self, quiet: bool) {
        while f64::from(self.drips) * DRIPS <= self.time {
            let i = self.drips as f32;
            // Golden angles keep them apart.
            let angle = i * 2.399_963;
            let radius = 4.0 + 13.0 * ((i * 0.618_034) % 1.0);
            self.touches.push(Touch {
                time: (f64::from(self.drips) * DRIPS) as f32,
                at: Vec2::from_angle(angle) * radius,
                radius: 0.9,
                strength: 0.16,
            });
            self.drips += 1;
            if !quiet && self.drips.is_multiple_of(5) {
                self.cues.push(Cue {
                    name: demo::DRIP,
                    repeat: false,
                });
            }
        }
    }

    fn touch(&mut self, at: Vec2, radius: f32, strength: f32) {
        self.touches.push(Touch {
            time: self.time as f32,
            at,
            radius,
            strength,
        });
    }

    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        if !self.paused {
            self.time += dt;
            self.drip(false);
        }
        // The water has long had the old ones.
        let old = self.touches.len().saturating_sub(256);
        self.touches.drain(..old);
    }

    fn eye(&self) -> Vec3 {
        let orbit = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        CENTER + self.distance * orbit
    }

    pub fn frame(&self) -> Frame {
        Frame {
            seconds: self.time as f32,
            touches: self.touches.clone(),
            z_correct: self.z_correct,
            view: glam::camera::rh::view::look_at_mat4(self.eye(), CENTER, Vec3::Y),
            fov: FOV,
            near: 0.5,
            far: 300.0,
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
            "z" | "Z" => self.z_correct = !self.z_correct,
            "s" | "S" => {
                let angle = self.time as f32 * 2.1;
                self.touch(Vec2::from_angle(angle) * 8.0, 2.2, 0.5);
                self.cues.push(Cue {
                    name: demo::DRIP,
                    repeat: false,
                });
            }
            _ => return false,
        }
        true
    }

    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        self.time = (seconds - 0.7).max(0.0);
        self.drip(true);
        if action {
            self.key("s");
        }
        self.time = seconds;
        self.drip(true);
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(0.12, 1.42);
        self.paused = true;
    }

    /// Where a point of the window is on the water.
    fn on_water(&self, x: f64, y: f64) -> Option<Vec2> {
        let view = self.frame().view.inverse();
        // The window's shape is not known here, so only near the middle is this right [I].
        let half = (FOV * 0.5).tan();
        let direction = Vec3::new(
            (x as f32 * 2.0 - 1.0) * half * 1.5,
            (1.0 - y as f32 * 2.0) * half,
            -1.0,
        );
        let d = view.transform_vector3(direction);
        let eye = self.eye();
        let t = -eye.y / d.y;
        (t > 0.0).then(|| {
            let p = eye + d * t;
            Vec2::new(p.x, p.z)
        })
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
        self.moved = 0.0;
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.moved += (x - px).abs() + (y - py).abs();
            self.yaw -= (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(0.12, 1.42);
            self.drag = Some((x, y));
        }
    }

    /// A click without a drag touches the water.
    pub fn release(&mut self) {
        if let Some((x, y)) = self.drag.take()
            && self.moved < 0.01
            && let Some(at) = self.on_water(x, y).filter(|p| p.length() < 30.0)
        {
            self.touch(at, 1.3, 0.35);
            self.cues.push(Cue {
                name: demo::DRIP,
                repeat: false,
            });
        }
    }

    pub fn zoom(&mut self, amount: f32) {
        // Inside the cave.
        self.distance = (self.distance * (-amount).exp()).clamp(9.0, 36.0);
    }
}
