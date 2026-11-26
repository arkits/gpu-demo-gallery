//! Time, camera and input shared by native and web builds.
use glam::{Mat4, Vec3};

use crate::demo;

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

/// Radius of the ball, which is around the origin.
const BALL: f32 = 10.0;
/// Yaw, pitch and distance, outside and inside the ball.
const OUTSIDE: (f32, f32, f32) = (0.35, 0.10, 44.0);
const INSIDE: (f32, f32, f32) = (0.5, 0.12, 7.2);
/// Seconds that the way in or out takes [I].
const WAY: f64 = 1.6;

pub struct Show {
    time: f64,
    last_wall: Option<f64>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    castle: usize,
    /// When the castles began to change. The glass clouds for a second, then clears for
    /// another (0x4048d0).
    changing: Option<f64>,
    /// When the way to another distance began, and from where to where.
    way: Option<(f64, f32, f32)>,
    music: &'static str,
    snow_started: f64,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub view: Mat4,
    pub castle: usize,
    /// The eye is in the ball.
    pub inside: bool,
    /// How clear the glass is.
    pub clear: f32,
    pub snow_time: f32,
    pub logo: bool,
}

impl Default for Show {
    fn default() -> Self {
        Self {
            time: 0.0,
            last_wall: None,
            yaw: OUTSIDE.0,
            pitch: OUTSIDE.1,
            distance: OUTSIDE.2,
            drag: None,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            castle: 0,
            changing: None,
            way: None,
            music: demo::INITIAL_MUSIC,
            snow_started: 0.0,
            paused: false,
            logo: true,
        }
    }
}

impl Show {
    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        if !self.paused {
            self.run(dt);
        }
    }

    fn run(&mut self, dt: f64) {
        let before = self.time;
        self.time += dt;
        if let Some(start) = self.changing {
            if before - start < 1.0 && self.time - start >= 1.0 {
                self.castle ^= 1;
                self.play(["light_castle", "dark_castle"][self.castle]);
            }
            if self.time - start >= 2.0 {
                self.changing = None;
            }
        }
        if let Some((start, from, to)) = self.way {
            let t = (((self.time - start) / WAY) as f32).min(1.0);
            self.distance = from + (to - from) * t * t * (3.0 - 2.0 * t);
            if t >= 1.0 {
                self.way = None;
            }
        }
    }

    /// Changes the music.
    fn play(&mut self, name: &'static str) {
        if self.music != name {
            self.music = name;
            self.cue(name, true);
        }
    }

    fn cue(&mut self, name: &'static str, repeat: bool) {
        self.cues.push(Cue { name, repeat });
    }

    fn inside(&self) -> bool {
        self.distance < BALL
    }

    pub fn frame(&self) -> Frame {
        let orbit = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        // Outside, the stand belongs to the picture.
        let center = Vec3::Y * if self.inside() { -0.5 } else { -3.2 };
        let eye = self.distance * orbit;
        let clear = self.changing.map_or(1.0, |start| {
            ((self.time - start) as f32 - 1.0).abs().clamp(0.0, 1.0)
        });
        Frame {
            view: glam::camera::rh::view::look_at_mat4(eye, center, Vec3::Y),
            castle: self.castle,
            inside: self.inside(),
            clear,
            snow_time: (self.time - self.snow_started).max(0.0) as f32,
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// In or out of the ball.
    fn enter(&mut self, inside: bool) {
        let home = if inside { INSIDE } else { OUTSIDE };
        self.way = Some((self.time, self.distance, home.2));
        self.cue("transition", false);
    }

    pub fn key(&mut self, key: &str) -> bool {
        match key {
            "p" | "P" => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            "Home" => {
                (self.yaw, self.pitch, self.distance) = OUTSIDE;
                self.way = None;
            }
            " " => {
                let inside = self.way.map_or(self.inside(), |(_, _, to)| to < BALL);
                self.enter(!inside);
            }
            "s" | "S" if self.changing.is_none() => {
                self.changing = Some(self.time);
                self.cue(["thunder", "transition"][self.castle], false);
            }
            "s" | "S" => {}
            "r" | "R" => self.snow_started = self.time,
            _ => return false,
        }
        true
    }

    /// Sets the moment and the camera exactly, for checking frames.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        if action {
            self.castle = 1;
            (self.yaw, self.pitch, self.distance) = INSIDE;
        }
        self.time = seconds;
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(-0.5, 1.48);
        self.paused = true;
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.yaw -= (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(-0.5, 1.48);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.way = None;
        self.distance = (self.distance * (-amount).exp()).clamp(4.5, 90.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn castles_change_behind_clouded_glass() {
        let mut s = Show::default();
        s.key("s");
        for step in 0..130 {
            s.run(1.0 / 60.0);
            if step == 58 {
                assert!(s.frame().castle == 0 && s.frame().clear < 0.05);
            }
        }
        assert_eq!((s.castle, s.frame().clear), (1, 1.0));
        assert_eq!(s.music, "dark_castle");
    }

    #[test]
    fn goes_in_and_out() {
        let mut s = Show::default();
        for inside in [true, false] {
            s.key(" ");
            (0..120).for_each(|_| s.run(1.0 / 60.0));
            assert_eq!(s.frame().inside, inside);
        }
    }
}
