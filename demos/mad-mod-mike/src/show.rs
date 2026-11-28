//! Time, takes, cameras, the face and input, shared by native and web builds.
use glam::{Mat4, Quat, Vec3};

use crate::demo::{self, EXPRESSIONS, TAKES};

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

/// What the camera does (`keys.txt`: `c` cycles scripted, free and helmet).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// A take with its scripted camera.
    Movie,
    /// A free camera.
    Free,
    /// From Mike's helmet.
    Helmet,
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
    Helmet,
}

/// A scripted camera preset from the shipped `vistas.txt` and friends.
#[derive(Clone, Copy, Default)]
pub struct Preset {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub look: Vec3,
    /// The animation the shipped file saved this preset for, if any.
    pub animation: Option<usize>,
}

pub struct Shot {
    /// The camera to the world.
    pub place: Mat4,
    pub fov: f32,
}

impl Shot {
    pub fn orbit(target: Vec3, yaw: f32, pitch: f32, distance: f32, fov: f32) -> Self {
        let turn = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        let eye = target + turn * Vec3::new(0.0, 0.0, distance);
        Self {
            place: Mat4::from_rotation_translation(turn, eye),
            fov,
        }
    }
}

pub enum Face {
    Animated,
    /// Blend-shape targets and their weights, plus the jaw angle.
    Posed(Vec<(usize, f32)>),
}

pub struct Frame {
    /// Which of `demo::TAKES`.
    pub take: usize,
    /// Seconds since it started.
    pub local: f64,
    pub camera: Camera,
    pub face: Face,
    pub jaw: f32,
    /// How white the picture is.
    pub white: f32,
    pub menu: bool,
}

pub struct Show {
    time: f64,
    started: f64,
    last_wall: Option<f64>,
    touched: f64,
    take: usize,
    mode: Mode,
    expression: usize,
    sliders: [f32; 5],
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: Vec3,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    pub paused: bool,
    pub menu: bool,
}

const HOME: (f32, f32, f32) = (-2.3, -0.38, 240.0);
const FARTHEST: f32 = 800.0;
const NEAREST: f32 = 8.0;

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
            take: 0,
            mode: Mode::Movie,
            expression: demo::FIRST,
            sliders: EXPRESSIONS[demo::FIRST].weights,
            yaw: 0.0,
            pitch: 0.0,
            distance: 0.0,
            pan: Vec3::ZERO,
            drag: None,
            cues: Vec::new(),
            paused: false,
            menu: false,
        };
        show.home();
        show.begin(0);
        show
    }

    fn home(&mut self) {
        (self.yaw, self.pitch, self.distance) = HOME;
        if TAKES[self.take].set == demo::Set::Workshop {
            self.yaw = 1.55;
        }
        self.pan = Vec3::ZERO;
    }

    pub fn begin(&mut self, take: usize) {
        self.take = take % TAKES.len();
        self.started = self.time;
        self.cues.push(Cue {
            name: "soundtrack",
            repeat: true,
        });
    }

    /// Changes what the camera does, and the take if that differs.
    fn set(&mut self, mode: Mode, take: usize) {
        if self.take != take % TAKES.len() {
            self.begin(take);
        }
        if mode == Mode::Free && self.mode != Mode::Free {
            self.home();
        }
        self.mode = mode;
        self.paused = false;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn take(&self) -> usize {
        self.take
    }

    pub fn expression(&self) -> usize {
        self.expression
    }

    pub fn sliders(&self) -> [f32; 5] {
        self.sliders
    }

    pub fn express(&mut self, i: usize) {
        let i = i % EXPRESSIONS.len();
        self.touched = self.time;
        self.expression = i;
        self.sliders = EXPRESSIONS[i].weights;
        self.set(self.mode, self.take);
    }

    pub fn slide(&mut self, i: usize, value: f32) {
        self.touched = self.time;
        if i < 5 {
            self.sliders[i] = value.clamp(0.0, 1.0);
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
        if self.time - self.started > TAKES[self.take].stop {
            self.begin(self.take);
        }
    }

    /// How far the take has come out of the white.
    pub fn level(&self) -> f32 {
        let local = self.time - self.started;
        ((local / demo::FADE).min(1.0)) as f32
    }

    pub fn frame(&self) -> Frame {
        let jaw = EXPRESSIONS[self.expression].jaw;
        Frame {
            take: self.take,
            local: self.time - self.started,
            camera: match self.mode {
                Mode::Movie => Camera::Movie,
                Mode::Helmet => Camera::Helmet,
                Mode::Free => Camera::Free {
                    yaw: self.yaw,
                    pitch: self.pitch,
                    distance: self.distance,
                    pan: self.pan,
                },
            },
            face: match self.mode {
                Mode::Free if self.expression != demo::FIRST => Face::Posed(
                    EXPRESSIONS[self.expression]
                        .targets
                        .into_iter()
                        .zip(self.sliders)
                        .collect(),
                ),
                _ => Face::Animated,
            },
            jaw,
            white: 1.0 - self.level(),
            menu: self.menu,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn key(&mut self, key: &str) -> bool {
        self.touched = self.time;
        match key {
            " " | "p" | "P" => self.paused = !self.paused,
            "m" | "M" => self.menu = !self.menu,
            "c" | "C" => self.set(
                match self.mode {
                    Mode::Movie => Mode::Free,
                    Mode::Free => Mode::Helmet,
                    Mode::Helmet => Mode::Movie,
                },
                self.take,
            ),
            "Home" => {
                self.home();
                self.pan = Vec3::ZERO;
            }
            "ArrowUp" => self.express(self.expression + EXPRESSIONS.len() - 1),
            "ArrowDown" => self.express(self.expression + 1),
            "ArrowRight" => self.begin(self.take + 1),
            "ArrowLeft" => self.begin(self.take + TAKES.len() - 1),
            _ => match key.parse::<usize>() {
                Ok(n @ 1..=9) => {
                    self.begin(n - 1);
                    self.set(Mode::Movie, n - 1);
                }
                Ok(0) => {
                    self.begin(9);
                    self.set(Mode::Movie, 9);
                }
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
        // Limits from the original's camera clamp.
        let (low, high) = (-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_4);
        self.pitch = (self.pitch + pitch).clamp(low, high);
    }

    /// The camera is free from here.
    fn take_hold(&mut self) {
        self.touched = self.time;
        if self.mode == Mode::Movie {
            self.home();
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
        self.pan.y = (self.pan.y + amount * self.distance * 0.05).clamp(-80.0, 80.0);
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
    fn takes_loop_and_restart() {
        let mut s = Show::new();
        assert_eq!(s.level(), 0.0);
        let mut wall = 0.0;
        let mut until = |s: &mut Show, t: f64| {
            while wall < t {
                s.advance(wall);
                wall += 0.05;
            }
        };
        until(&mut s, 1.0);
        assert!((s.level() - 0.5).abs() < 0.06);
        until(&mut s, 5.0);
        assert_eq!(s.level(), 1.0);
        assert_eq!(s.take_cues()[0].name, "soundtrack");
        until(&mut s, TAKES[0].stop + 0.5);
        assert_eq!(s.take(), 0);
    }

    #[test]
    fn idle_returns_to_the_movie() {
        let mut s = Show::new();
        s.key("c");
        assert_eq!(s.mode(), Mode::Free);
        for i in 0..2500 {
            s.advance(f64::from(i) * 0.05);
        }
        assert_eq!((s.mode(), s.take()), (Mode::Movie, 0));
    }

    #[test]
    fn digits_and_arrows_change_takes() {
        let mut s = Show::new();
        s.key("3");
        assert_eq!(s.take(), 2);
        s.key("ArrowRight");
        assert_eq!(s.take(), 3);
        s.key("ArrowDown");
        assert_eq!(s.expression(), 1);
    }
}
