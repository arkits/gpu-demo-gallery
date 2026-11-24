//! What the original's `main.cpp` does: input, camera, autopilot and modes.
//! One `step` is one original frame; the port runs 60 a second.
use glam::{Mat4, Vec3};

use crate::sim::{DIM, Inject, Settings};

/// Milliseconds without input before the autopilot and the camera orbit take over.
const IDLE: f64 = 10_000.0;
/// Initial "last input" times: both start idle.
const LONG_AGO: f64 = -1_000_000.0;
pub const STEP_MS: f64 = 1000.0 / 60.0;

/// What the simulation must do besides stepping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Reset,
    /// Resets, then stamps the logo into the smoke.
    Logo,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Other,
}

struct Drag {
    button: Button,
    rotate: bool,
    start: (f32, f32),
    orbit: Vec3,
    /// The first motion only records a position.
    moved: bool,
}

pub struct Controls {
    pub settings: Settings,
    pub inject: Inject,
    pub paused: bool,
    pub supersample: bool,
    /// Azimuth, polar angle, distance.
    orbit: Vec3,
    path: Vec<Vec3>,
    path_time: f32,
    now: f64,
    last_smoke: f64,
    last_camera: f64,
    drag: Option<Drag>,
    last_cell: [i32; 3],
    pending: Vec<Command>,
    single: bool,
}

impl Controls {
    pub fn new(path: Vec<[f32; 3]>) -> Self {
        Self {
            settings: Settings::smoke(),
            inject: Inject {
                active: false,
                color: true,
                push: false,
                pos: Vec3::new(64.0, 0.0, 32.0),
                velocity: Vec3::Y,
            },
            paused: false,
            supersample: false,
            orbit: Vec3::new(0.0, 1.2, 2.0),
            path: path.into_iter().map(Vec3::from).collect(),
            path_time: 0.0,
            now: 0.0,
            last_smoke: LONG_AGO,
            last_camera: LONG_AGO,
            drag: None,
            last_cell: [64, 32, 32],
            pending: Vec::new(),
            single: false,
        }
    }

    pub fn eye(&self) -> Vec3 {
        let (theta, phi) = (self.orbit.x, self.orbit.y);
        let r = if self.orbit.z < 1.0 {
            1.05
        } else {
            self.orbit.z
        };
        r * Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin())
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), Vec3::ZERO, Vec3::Y)
    }

    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.pending)
    }

    /// Whether the simulation advances in this step.
    pub fn runs(&mut self) -> bool {
        std::mem::take(&mut self.single) || !self.paused
    }

    /// Advances one frame. The original simulates first, so positions set here
    /// take effect in the next step.
    pub fn step(&mut self) {
        self.now += STEP_MS;
        let idle = ((self.now - self.last_camera) / IDLE - 1.0).min(1.0);
        if idle > 0.0 {
            self.orbit.x += idle as f32 * 0.003;
        }
        if self.now - self.last_smoke > IDLE && self.path.len() > 1 {
            self.inject.active = true;
            self.inject.color = true;
            let t = self.path_time * 1.3;
            let (i, f) = (t.floor() as usize % self.path.len(), t.fract());
            let (a, b) = (self.path[i], self.path[(i + 1) % self.path.len()]);
            let at = a.lerp(b, f) * 128.0 + DIM.as_vec3() * 0.5;
            self.inject.pos = at.trunc();
            self.inject.velocity = (b - a) * 128.0;
        }
        self.path_time += self.settings.timestep;
    }

    /// Returns false for keys the demo does not use.
    pub fn key(&mut self, key: &str) -> bool {
        let s = &mut self.settings;
        match key {
            " " => self.paused = !self.paused,
            "`" => self.single = true,
            "a" | "A" => self.supersample = !self.supersample,
            "s" | "S" => {
                *s = Settings::smoke();
                self.inject.active = false;
                self.inject.color = true;
                self.last_smoke = LONG_AGO;
                self.pending.push(Command::Reset);
            }
            "n" | "N" => {
                *s = Settings::logo();
                self.inject.active = true;
                self.inject.color = false;
                self.last_smoke = self.now;
                self.pending.push(Command::Logo);
            }
            "w" | "W" => {
                *s = Settings::water(s);
                self.inject.color = false;
                self.pending.push(Command::Reset);
            }
            "b" | "B" => {
                s.bfecc = !s.bfecc;
                (s.confinement, s.fade) = if s.bfecc { (0.1, 0.985) } else { (0.19, 0.99) };
            }
            "(" => s.saturation = (s.saturation - 0.01).max(0.0),
            ")" => s.saturation = (s.saturation + 0.01).min(1.0),
            "{" => s.fade = (s.fade - 0.001).max(0.0),
            "}" => s.fade = (s.fade + 0.001).min(1.0),
            "[" => s.size += 0.01,
            "]" => s.size = (s.size - 0.01).max(0.0),
            _ => return false,
        }
        true
    }

    /// Positions are fractions of the window, from its top left.
    pub fn press(&mut self, button: Button, rotate: bool, at: (f32, f32)) {
        if self.settings.water {
            self.inject.color = false;
        }
        self.drag = Some(Drag {
            button,
            rotate,
            start: at,
            orbit: self.orbit,
            moved: false,
        });
        self.touch(rotate);
    }

    pub fn release(&mut self) {
        let rotate = self.drag.take().is_some_and(|d| d.rotate);
        self.inject.push = false;
        self.touch(rotate);
    }

    fn touch(&mut self, rotate: bool) {
        if rotate {
            self.last_camera = self.now;
        } else {
            self.last_smoke = self.now;
        }
    }

    pub fn motion(&mut self, (x, y): (f32, f32)) {
        let Some(d) = &mut self.drag else { return };
        if d.rotate {
            if d.button == Button::Left {
                self.orbit.x = d.orbit.x + 2.0 * (x - d.start.0);
                let polar = d.orbit.y + 2.0 * (d.start.1 - y);
                // The original's limits: 1.56 above pi/2 is never reached from below.
                self.orbit.y = polar.clamp(0.1, 1.56);
            } else {
                self.orbit.z = (d.orbit.z + 2.0 * (d.start.1 - y)).clamp(1.05, 8.0);
            }
            self.last_camera = self.now;
            return;
        }
        if d.moved {
            self.inject.push = true;
        }
        let (gx, gy) = ((x * 128.0) as i32, (y * 64.0) as i32);
        let cell = match d.button {
            Button::Left => [gx, 63 - gy, self.last_cell[2]],
            Button::Other => [gx, self.last_cell[1], gy],
        };
        let inside = (0..128).contains(&cell[0]) && (0..64).contains(&cell[1]);
        if d.moved && inside {
            let delta = Vec3::from_array(std::array::from_fn(|i| {
                (cell[i] - self.last_cell[i]) as f32
            }));
            if delta.length() > 0.01 {
                self.inject.pos = Vec3::from_array(cell.map(|c| c as f32));
                self.inject.velocity = delta * 0.5;
            }
        }
        self.last_cell = cell;
        d.moved = true;
        self.last_smoke = self.now;
    }

    /// Not in the original, which zooms with Ctrl and the right button.
    pub fn zoom(&mut self, amount: f32) {
        self.orbit.z = (self.orbit.z - amount).clamp(1.05, 8.0);
        self.last_camera = self.now;
    }

    /// Not in the original: two-finger rotation on touch screens.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.orbit.x += 2.0 * dx;
        self.orbit.y = (self.orbit.y - 2.0 * dy).clamp(0.1, 1.56);
        self.last_camera = self.now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls() -> Controls {
        Controls::new(vec![[0.0; 3], [0.1, 0.0, 0.0], [0.1, 0.1, 0.0]])
    }

    #[test]
    fn autopilot_starts_at_once_and_yields_to_a_drag() {
        let mut c = controls();
        c.step();
        assert!(c.inject.active);
        assert_eq!(c.inject.pos, Vec3::new(64.0, 32.0, 32.0));
        c.press(Button::Left, false, (0.5, 0.5));
        c.motion((0.5, 0.5));
        c.motion((0.75, 0.25));
        assert!(c.inject.push);
        assert_eq!(c.inject.pos, Vec3::new(96.0, 47.0, 32.0));
        c.step();
        assert_eq!(c.inject.pos, Vec3::new(96.0, 47.0, 32.0));
        c.release();
        assert!(!c.inject.push);
        for _ in 0..601 {
            c.step();
        }
        assert_ne!(c.inject.pos, Vec3::new(96.0, 47.0, 32.0));
    }

    #[test]
    fn modes() {
        let mut c = controls();
        assert!(c.key("w") && c.settings.water && c.settings.timestep == 4.0);
        assert!(c.key("n") && !c.settings.water && !c.settings.bfecc);
        assert_eq!(c.take_commands(), [Command::Reset, Command::Logo]);
        assert!(c.key("s") && c.settings == Settings::smoke());
        assert!(!c.key("q"));
        c.key(" ");
        assert!(!c.runs());
        c.key("`");
        assert!(c.runs() && !c.runs());
    }

    #[test]
    fn camera_stays_above_the_floor() {
        let mut c = controls();
        c.press(Button::Left, true, (0.5, 0.5));
        c.motion((0.5, -5.0));
        assert!(c.eye().y > 0.0);
        c.motion((0.5, 5.0));
        assert!((c.eye().length() - 2.0).abs() < 1e-5);
    }
}
