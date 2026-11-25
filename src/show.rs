//! Playback clock, panic and recovery, and the orbit camera.
use glam::{Mat4, Vec3};

use crate::demo;

const PANIC_LENGTH: f64 = 6.0;
const RECOVERY_LENGTH: f64 = 3.0;

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

struct Panic {
    started: f64,
    recovery: Option<f64>,
}

#[derive(Clone, Copy)]
pub(crate) struct Scatter {
    pub elapsed: f32,
    pub travel: f32,
    pub blend: f32,
}

pub struct Show {
    time: f64,
    march: f64,
    last_wall: Option<f64>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    center: Vec3,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    panic: Option<Panic>,
    orbit_origin: Option<f64>,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub view: Mat4,
    pub(crate) march: f32,
    pub(crate) scatter: Option<Scatter>,
    pub logo: bool,
}

pub(crate) fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Show {
    pub fn new(center: Vec3) -> Self {
        let mut show = Self {
            time: 0.0,
            march: 0.0,
            last_wall: None,
            yaw: 0.0,
            pitch: 0.0,
            distance: 0.0,
            center,
            drag: None,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            panic: None,
            orbit_origin: None,
            paused: false,
            logo: true,
        };
        show.home();
        show
    }

    fn home(&mut self) {
        (self.yaw, self.pitch, self.distance) = (0.26, 0.23, 96.0);
        self.orbit_origin = Some(self.time);
    }

    fn recover(&mut self) {
        if let Some(panic) = &mut self.panic
            && panic.recovery.is_none()
        {
            panic.recovery = Some(self.time);
            self.cue("march", true);
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
        if let Some(panic) = &self.panic {
            if let Some(start) = panic.recovery {
                if self.time >= start + RECOVERY_LENGTH {
                    self.panic = None;
                }
            } else if self.time - panic.started >= PANIC_LENGTH {
                self.recover();
            }
        } else {
            self.march += dt;
        }
    }

    fn camera(&self) -> (f32, f32, f32) {
        let t = self
            .orbit_origin
            .map_or(0.0, |start| (self.time - start) as f32);
        (
            self.yaw + t * 0.075,
            self.pitch + (t * 0.28).sin() * 0.085,
            self.distance * (0.90 + 0.10 * (t * 0.18).cos()),
        )
    }

    fn take_camera(&mut self) {
        (self.yaw, self.pitch, self.distance) = self.camera();
        self.orbit_origin = None;
    }

    pub fn frame(&self) -> Frame {
        let (yaw, pitch, distance) = self.camera();
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        let eye = self.center + distance * direction;
        Frame {
            view: glam::camera::rh::view::look_at_mat4(eye, self.center, Vec3::Y),
            march: self.march as f32,
            scatter: self.panic.as_ref().map(|panic| {
                let elapsed = (self.time - panic.started).max(0.0) as f32;
                let run = (panic.recovery.unwrap_or(self.time) - panic.started).max(0.0) as f32;
                let recovery = panic
                    .recovery
                    .map_or(0.0, |t| smooth(((self.time - t) / RECOVERY_LENGTH) as f32));
                Scatter {
                    elapsed,
                    travel: smooth((run - 0.12) / 2.8) * (1.0 - recovery),
                    blend: smooth(run / 0.28) * (1.0 - recovery),
                }
            }),
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
            "Home" => self.home(),
            " " if self.panic.is_none() => {
                self.panic = Some(Panic {
                    started: self.time,
                    recovery: None,
                });
                self.cue("panic", true);
                self.cue("scary", false);
            }
            " " => (),
            "s" | "S" => self.recover(),
            _ => return false,
        }
        true
    }

    /// Set the camera and time for native and web frame checks.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        self.time = seconds.max(0.0);
        self.march = if action { 0.0 } else { self.time };
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(0.04, 1.48);
        self.orbit_origin = None;
        self.paused = true;
        self.panic = action.then_some(Panic {
            started: 0.0,
            recovery: (self.time >= PANIC_LENGTH).then_some(PANIC_LENGTH),
        });
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.take_camera();
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.yaw += (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(0.04, 1.48);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.take_camera();
        self.distance = (self.distance * (-amount).exp()).clamp(22.0, 260.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_does_not_snap_when_the_user_grabs_it() {
        let mut show = Show::new(Vec3::ZERO);
        show.time = 13.0;
        let before = show.frame().view;
        show.press(0.5, 0.5);
        assert_eq!(before, show.frame().view);
    }

    #[test]
    fn panic_starts_from_the_march_and_recovery_starts_from_the_scatter() {
        let mut show = Show::new(Vec3::ZERO);
        show.march = 6.0;
        assert!(show.key(" "));
        let start = show.frame();
        assert_eq!(start.march, 6.0);
        assert_eq!(start.scatter.unwrap().blend, 0.0);
        show.time = 1.1;
        let before = show.frame().scatter.unwrap();
        assert!(show.key("s"));
        let after = show.frame().scatter.unwrap();
        assert_eq!(before.travel, after.travel);
        assert_eq!(before.blend, after.blend);
        show.advance(0.0);
        for i in 1..=64 {
            show.advance(f64::from(i) * 0.05);
        }
        assert!(show.frame().scatter.is_none());
        assert!(show.frame().march > 6.0);
    }
}
