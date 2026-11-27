//! Time, camera, fish and laser (`CreatureViewer.cpp`, `NvNavigator.cpp`).
use glam::{Mat4, Quat, Vec3};

use crate::fish::School;

pub const FISH: usize = 500;
/// More would slow the school down too much.
pub const MOST_FISH: usize = 1000;
/// Distance at which fog hides everything, and the size of a cell of plankton.
pub const FAR: f32 = 80.0;
/// `setup_fullscreen.txt`: the water's color looking up; it is black looking down.
const WATER: Vec3 = Vec3::new(0.05, 0.6, 0.4);
/// Seconds the laser stays on.
const LASER: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// The music, without end.
    Music,
    Laser,
}

pub struct Show {
    time: f64,
    last_wall: Option<f64>,
    yaw: f64,
    pitch: f64,
    spin: (f64, f64),
    start: f32,
    distance: f32,
    school: School,
    laser: Option<f64>,
    drag: Option<(f64, f64, f64)>,
    cues: Vec<Cue>,
    pub plankton: Vec<[f32; 4]>,
    pub logo: bool,
    pub paused: bool,
    still_fish: bool,
}

pub struct Frame {
    pub ms: f64,
    /// World to camera.
    pub view: Mat4,
    pub eye: Vec3,
    pub fog: Vec3,
    /// Direction to the light, in the world.
    pub light: Vec3,
    pub fish: Vec<Mat4>,
    pub laser: bool,
    pub logo: bool,
}

impl Show {
    /// `radius` is the scene's: the camera starts one and a half times as far.
    pub fn new(radius: f32) -> Self {
        let mut school = School::default();
        (0..FISH).for_each(|_| school.add());
        // Positions in a cell, and how far to follow either of two currents.
        let plankton = (0..2000)
            .map(|_| {
                let p = school.rand.vec() * FAR;
                [p.x, p.y, p.z, school.rand.unit()]
            })
            .collect();
        Self {
            time: 0.0,
            last_wall: None,
            yaw: 0.0,
            pitch: 0.0,
            spin: (0.0, 0.0),
            start: radius * 1.5,
            distance: radius * 1.5,
            school,
            laser: None,
            drag: None,
            cues: vec![Cue::Music],
            plankton,
            logo: true,
            paused: false,
            still_fish: false,
        }
    }

    pub fn advance(&mut self, now: f64) {
        let dt = self.last_wall.map_or(0.0, |w| (now - w).clamp(0.0, 0.1));
        self.last_wall = Some(now);
        if self.drag.is_none() {
            self.turn(self.spin.0 * dt, self.spin.1 * dt);
        }
        if !self.paused {
            self.step(dt);
        }
    }

    fn step(&mut self, dt: f64) {
        self.time += dt;
        if !self.still_fish {
            self.school.step(dt as f32);
        }
        if self.laser.is_some_and(|since| self.time - since > LASER) {
            self.laser = None;
        }
    }

    /// Goes to a time in steps of a sixtieth, and stops there. For checking frames.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, laser: bool) {
        for _ in 0..(seconds * 60.0).round() as usize {
            self.step(1.0 / 60.0);
        }
        (self.yaw, self.pitch, self.paused) = (yaw, pitch, true);
        self.laser = laser.then_some(self.time);
    }

    fn turn(&mut self, yaw: f64, pitch: f64) {
        let full = std::f64::consts::TAU;
        self.yaw = (self.yaw + yaw).rem_euclid(full);
        // Never from below, nor over the top.
        self.pitch = (self.pitch + pitch).clamp(-full / 4.0, 0.0);
    }

    pub fn frame(&self) -> Frame {
        let ms = self.time * 1000.0;
        let turn =
            Quat::from_rotation_x(-self.pitch as f32) * Quat::from_rotation_y(self.yaw as f32);
        let eye = turn.inverse() * Vec3::new(0.0, 0.0, self.distance);
        // How far up the camera looks, 0 to 1 (0x402850).
        let up = (turn * Vec3::Y).z.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
        // The light is above the camera and sways (0x403cc0).
        let (a, b) = (
            (ms * 0.003).cos() as f32 * 0.15,
            (ms * 0.0029).cos() as f32 * 0.15,
        );
        let sway = Quat::from_xyzw(
            b.cos() * a.sin(),
            -b.sin() * a.sin(),
            b.sin() * a.cos(),
            b.cos() * a.cos(),
        );
        Frame {
            ms,
            view: Mat4::from_translation(Vec3::new(0.0, 0.0, -self.distance))
                * Mat4::from_quat(turn),
            eye,
            fog: WATER * up,
            light: turn.inverse() * (sway.inverse() * Vec3::Y),
            fish: self.school.places().collect(),
            laser: self.laser.is_some(),
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Returns false for keys that do nothing.
    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " => {
                self.laser = Some(self.time);
                self.cues.push(Cue::Laser);
            }
            "+" | "=" if self.school.len() < MOST_FISH => self.school.add(),
            "-" => self.school.remove(),
            "u" | "U" => self.still_fish = !self.still_fish,
            "l" | "L" => self.logo = !self.logo,
            "p" | "P" => self.paused = !self.paused,
            "Home" => {
                (self.yaw, self.pitch, self.spin, self.distance) =
                    (0.0, 0.0, (0.0, 0.0), self.start)
            }
            _ => return false,
        }
        true
    }

    pub fn fish(&self) -> usize {
        self.school.len()
    }

    /// Positions are fractions of the window, from its top left.
    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y, self.last_wall.unwrap_or(0.0)));
        self.spin = (0.0, 0.0);
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        let Some((x0, y0, then)) = self.drag else {
            return;
        };
        // The original measures in half windows and turns half a circle for each.
        let full = std::f64::consts::TAU;
        let (yaw, pitch) = ((x - x0) * full, (y0 - y) * full);
        self.turn(yaw, pitch);
        let now = self.last_wall.unwrap_or(0.0);
        if now > then {
            self.spin = (yaw / (now - then), pitch / (now - then));
        }
        self.drag = Some((x, y, now));
    }

    /// It keeps turning as it was when let go.
    pub fn release(&mut self) {
        if let Some((.., then)) = self.drag.take()
            && self.last_wall.unwrap_or(0.0) - then > 0.1
        {
            self.spin = (0.0, 0.0);
        }
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (1.0 - amount)).clamp(3.0, FAR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_laser_ends_by_itself() {
        let mut s = Show::new(20.0);
        assert_eq!(s.take_cues(), [Cue::Music]);
        s.key(" ");
        assert!(s.frame().laser);
        assert_eq!(s.take_cues(), [Cue::Laser]);
        let mut now = 0.0;
        while now < 6.0 {
            s.advance(now);
            now += 0.05;
        }
        assert!(!s.frame().laser);
    }

    #[test]
    fn water_is_bright_above_and_dark_below() {
        let mut s = Show::new(20.0);
        let level = s.frame().fog.y;
        assert!((level - 0.3).abs() < 1e-3);
        // The camera goes up and looks down.
        s.press(0.5, 0.5);
        s.motion(0.5, 0.7);
        let f = s.frame();
        assert!(f.eye.y > 1.0 && f.fog.y < level);
        // The light stays above the camera.
        assert!((f.view.transform_vector3(f.light)).y > 0.9);
    }
}
