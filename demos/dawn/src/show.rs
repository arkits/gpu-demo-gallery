//! Time, cameras, the face and input, shared by native and web builds.
use glam::{Mat4, Quat, Vec3};

use crate::demo::{self, EXPRESSIONS, SCENES};

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

/// What the camera does (`FUN_00409db0`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// The dance with its own camera.
    Movie,
    /// A free camera and a chosen face.
    Expression,
    /// A free camera.
    Free,
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
    /// `movie` is the animated camera; the free one looks at `root` (`FUN_0040b1b0`).
    pub fn shot(&self, movie: Mat4, root: Vec3) -> Shot {
        let Camera::Free {
            yaw,
            pitch,
            distance,
            pan,
        } = *self
        else {
            return Shot {
                place: movie,
                fov: 36_f32.to_radians(),
            };
        };
        let turn = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        let place = |target: Vec3| target + turn * Vec3::new(0.0, 0.0, distance);
        // From up close it looks at her face, and with a longer lens.
        let rise = |eye: Vec3| (6.0 - (eye.distance(root) - 20.0) * 0.3).clamp(0.0, 6.0);
        let mut target = root + pan;
        // The program uses the last frame's camera; this is where that settles.
        for _ in 0..24 {
            let next = root + pan + Vec3::Y * rise(place(target));
            target = target.lerp(next, 0.5);
        }
        let eye = place(target);
        let far = ((eye.distance(root) - 22.0) / 38.0).clamp(0.0, 1.0);
        Shot {
            place: Mat4::from_rotation_translation(turn, eye),
            fov: (15.0 + 39.0 * far).to_radians(),
        }
    }
}

pub enum Face {
    Animated,
    /// Morph targets and their weights.
    Posed(Vec<(usize, f32)>),
}

pub struct Frame {
    /// Which of `demo::SCENES`.
    pub scene: usize,
    /// Seconds since it started.
    pub local: f64,
    pub camera: Camera,
    pub face: Face,
    pub oil: f32,
    pub bumps: f32,
    /// How white the picture is.
    pub white: f32,
    pub logo: bool,
}

pub struct Show {
    time: f64,
    started: f64,
    last_wall: Option<f64>,
    touched: f64,
    scene: usize,
    mode: Mode,
    expression: usize,
    sliders: [f32; demo::MORPHS],
    oil: f32,
    bumps: f32,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: Vec3,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    pub paused: bool,
    pub logo: bool,
}

const HOME: (f32, f32, f32) = (0.0, 0.0, 20.0);
const FARTHEST: f32 = 50.0;
const NEAREST: f32 = 4.0;

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
            scene: 0,
            mode: Mode::Movie,
            expression: demo::FIRST,
            sliders: EXPRESSIONS[demo::FIRST].weights,
            oil: 1.0,
            bumps: 1.0,
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
        show.begin(0);
        show
    }

    fn home(&mut self) {
        (self.yaw, self.pitch, self.distance) = HOME;
        self.pan = Vec3::ZERO;
    }

    fn begin(&mut self, scene: usize) {
        self.scene = scene;
        self.started = self.time;
        let name = SCENES[scene].sound;
        self.cues.push(Cue {
            name,
            repeat: false,
        });
    }

    /// Changes what the camera does, and the scene if that differs.
    fn set(&mut self, mode: Mode, scene: usize) {
        if self.scene != scene {
            self.begin(scene);
        }
        self.mode = mode;
        self.paused = false;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn scene(&self) -> usize {
        self.scene
    }

    pub fn expression(&self) -> usize {
        self.expression
    }

    pub fn sliders(&self) -> [f32; demo::MORPHS] {
        self.sliders
    }

    pub fn express(&mut self, i: usize) {
        let i = i % EXPRESSIONS.len();
        self.touched = self.time;
        self.expression = i;
        self.sliders = EXPRESSIONS[i].weights;
        self.set(Mode::Expression, self.scene);
    }

    /// Sliders run from 0 to 1. After those of the face come oil and goose bumps.
    pub fn slide(&mut self, i: usize, value: f32) {
        self.touched = self.time;
        let value = value.clamp(0.0, 1.0);
        match i {
            0..demo::MORPHS => {
                self.set(Mode::Expression, self.scene);
                self.sliders[i] = value;
            }
            // `FUN_00405520` and `FUN_004055d0`.
            demo::MORPHS => self.oil = value * 10.0,
            _ => self.bumps = value * 6.0 - 3.0,
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
        if self.mode != Mode::Movie && self.time - self.touched > demo::IDLE {
            self.set(Mode::Movie, 0);
        }
        if self.time - self.started > SCENES[self.scene].stop {
            self.begin(self.scene);
        }
    }

    /// How far the scene has come out of the white. Sound follows it.
    pub fn level(&self) -> f32 {
        let s = &SCENES[self.scene];
        let local = self.time - self.started;
        let (rise, fall) = (
            (local - s.start) / demo::FADE,
            (s.stop - local) / demo::FADE,
        );
        rise.min(fall).clamp(0.0, 1.0) as f32
    }

    pub fn frame(&self) -> Frame {
        let posed = || {
            let targets = EXPRESSIONS[self.expression].targets;
            Face::Posed(targets.into_iter().zip(self.sliders).collect())
        };
        Frame {
            scene: self.scene,
            local: self.time - self.started,
            camera: match self.mode {
                Mode::Movie => Camera::Movie,
                _ => Camera::Free {
                    yaw: self.yaw,
                    pitch: self.pitch,
                    distance: self.distance,
                    pan: self.pan,
                },
            },
            face: match self.mode {
                Mode::Expression => posed(),
                _ => Face::Animated,
            },
            oil: self.oil,
            bumps: self.bumps,
            white: 1.0 - self.level(),
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
            "d" | "D" => self.set(Mode::Movie, 0),
            // The sunning has no camera of its own.
            "s" | "S" => self.set(Mode::Expression, 1),
            "f" | "F" => self.set(Mode::Free, self.scene),
            "Home" => self.home(),
            "+" | "=" | "ArrowRight" => self.express(self.expression + 1),
            "-" | "ArrowLeft" => self.express(self.expression + EXPRESSIONS.len() - 1),
            _ => match key.parse::<usize>() {
                Ok(n @ 1..=8) => self.express(n - 1),
                _ => return false,
            },
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
        // Limits from `FUN_0042fea0`.
        let (low, high) = (-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_4);
        self.pitch = (self.pitch + pitch).clamp(low, high);
    }

    /// The camera is free from here, as after choosing that in the menu.
    fn take_hold(&mut self) {
        self.touched = self.time;
        if self.mode == Mode::Movie {
            self.mode = Mode::Free;
        }
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.take_hold();
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

    /// Moves what the camera looks at up.
    pub fn pan(&mut self, amount: f32) {
        self.take_hold();
        self.pan.y = (self.pan.y + amount * self.distance * 0.6).clamp(-12.0, 12.0);
    }

    pub fn zoom(&mut self, amount: f32) {
        self.take_hold();
        self.distance = (self.distance * (-amount).exp()).clamp(NEAREST, FARTHEST);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_fade_and_start_again() {
        let mut s = Show::new();
        assert_eq!(s.level(), 0.0);
        let mut wall = 0.0;
        let mut until = |s: &mut Show, t: f64| {
            while wall < t {
                s.advance(wall);
                wall += 0.05;
            }
        };
        until(&mut s, 2.6);
        assert!((s.level() - 0.5).abs() < 0.05);
        until(&mut s, 20.0);
        assert_eq!(s.level(), 1.0);
        assert_eq!(s.take_cues().len(), 1);
        until(&mut s, 43.5);
        assert_eq!(s.level(), 0.0);
        assert_eq!(s.take_cues()[0].name, "flirt");
    }

    #[test]
    fn idle_returns_to_the_movie() {
        let mut s = Show::new();
        s.key("s");
        assert_eq!((s.mode(), s.scene()), (Mode::Expression, 1));
        for i in 0..2500 {
            s.advance(f64::from(i) * 0.05);
        }
        assert_eq!((s.mode(), s.scene()), (Mode::Movie, 0));
    }

    #[test]
    fn close_shots_look_at_the_face() {
        let shot = |distance| {
            let c = Camera::Free {
                yaw: 0.0,
                pitch: 0.0,
                distance,
                pan: Vec3::ZERO,
            };
            c.shot(Mat4::IDENTITY, Vec3::ZERO)
        };
        assert!((shot(10.0).place.w_axis.y - 6.0).abs() < 0.01);
        assert!(shot(45.0).place.w_axis.y.abs() < 0.1);
        assert!(shot(10.0).fov < shot(45.0).fov);
    }
}
