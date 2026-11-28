//! Time, cameras, the bug crowd, the menu and input, shared by native and web builds.
use glam::{Mat4, Quat, Vec3};

use crate::demo;
use crate::menu::{Menu, MenuDef, Press};

/// What the camera does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// The demo's own camera.
    Movie,
    /// A free camera.
    Free,
}

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

#[derive(Clone, Copy)]
pub enum Camera {
    /// One of the movie cameras by index into the model's list.
    Movie(usize),
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
    /// `movie` places the movie cameras with their filed lenses; the free one
    /// turns around `root`.
    pub fn shot(&self, movies: &[(Mat4, f32)], root: Vec3) -> Shot {
        let Camera::Free {
            yaw,
            pitch,
            distance,
            pan,
        } = *self
        else {
            let &Camera::Movie(i) = self else {
                unreachable!()
            };
            let (place, fov) = movies.get(i).copied().unwrap_or((Mat4::IDENTITY, 0.7));
            return Shot { place, fov };
        };
        let turn = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        let target = root + pan;
        let eye = target + turn * Vec3::new(0.0, 0.0, distance);
        Shot {
            place: Mat4::from_rotation_translation(turn, eye),
            fov: 0.7,
        }
    }
}

/// One bug of the swarm: where it is, where it looks, and its wing phase.
#[derive(Clone, Copy)]
pub struct Bug {
    pub position: Vec3,
    pub yaw: f32,
    pub phase: f32,
}

pub struct Frame {
    /// Seconds since the show started.
    pub time: f64,
    pub camera: Camera,
    /// The swarm.
    pub bugs: Vec<Bug>,
    /// The blur and exposure sliders, 0 to 1.
    pub sliders: [f32; 5],
    /// How white the picture is.
    pub white: f32,
    pub logo: bool,
    pub paused: bool,
    pub sliders_open: bool,
    pub about: bool,
}

pub struct Show {
    time: f64,
    started: f64,
    last_wall: Option<f64>,
    touched: f64,
    movie: usize,
    mode: Mode,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: Vec3,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    pub paused: bool,
    pub logo: bool,
    sliders: [f32; 5],
    about: f64,
    center: Vec3,
    menu_def: MenuDef,
    menu: Menu,
    dragging: Option<usize>,
    pub quit: bool,
}

/// The sliders' starting values from 0x4115b0: output scale, AGC damping,
/// light angle, softening amount, softening radius.
pub const SLIDERS: [f32; 5] = [0.465, 0.35, 0.565, 0.5, 0.5];
const HOME: (f32, f32, f32) = (0.0, 0.0, 420.0);
const FARTHEST: f32 = 1500.0;
const NEAREST: f32 = 2.0;

impl Default for Show {
    fn default() -> Self {
        Self::new(Vec3::ZERO, MenuDef::default())
    }
}

impl Show {
    pub fn new(center: Vec3, menu_def: MenuDef) -> Self {
        let mut show = Self {
            time: 0.0,
            started: 0.0,
            last_wall: None,
            touched: 0.0,
            movie: 0,
            mode: Mode::Movie,
            yaw: 0.0,
            pitch: 0.0,
            distance: 0.0,
            pan: Vec3::ZERO,
            drag: None,
            cues: Vec::new(),
            paused: false,
            logo: true,
            sliders: SLIDERS,
            about: -1.0,
            center,
            menu_def,
            menu: Menu::default(),
            dragging: None,
            quit: false,
        };
        show.home();
        show.begin();
        show
    }

    fn home(&mut self) {
        (self.yaw, self.pitch, self.distance) = HOME;
        self.pan = Vec3::ZERO;
    }

    fn begin(&mut self) {
        self.started = self.time;
        self.cues.push(Cue {
            name: "soundtrack",
            repeat: true,
        });
    }

    pub fn menu(&self) -> (&MenuDef, &Menu) {
        (&self.menu_def, &self.menu)
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn camera(&self) -> usize {
        self.movie
    }

    pub fn sliders(&self) -> [f32; 5] {
        self.sliders
    }

    pub fn slide(&mut self, i: usize, value: f32) {
        self.touched = self.time;
        if i < self.sliders.len() {
            self.sliders[i] = value.clamp(0.0, 1.0);
        }
    }

    /// The swarm crosses the set: each bug loops around the character [I].
    pub fn bugs(&self, time: f64) -> Vec<Bug> {
        let t = time as f32;
        (0..demo::BUGS)
            .map(|i| {
                let f = i as f32;
                let speed = 0.25 + 0.05 * (f * 0.37).sin();
                let a = t * speed + f * 2.39996;
                let r = 110.0 + 40.0 * (f * 0.73).sin();
                let position = self.center
                    + Vec3::new(
                        r * a.cos() + 18.0 * (t * 0.9 + f).sin(),
                        26.0 + 15.0 * (t * 0.7 + f * 1.7).sin(),
                        r * a.sin() * 0.6 + 18.0 * (t * 1.1 + f * 0.9).cos(),
                    );
                // Face along the flight direction [I].
                let b = a + 0.05;
                let next = Vec3::new(r * b.cos(), position.y, r * b.sin() * 0.6);
                let yaw = (next - position).y.atan2((next - position).x);
                Bug {
                    position,
                    yaw,
                    phase: (t * 15.0 + f * 1.3) % 1.0,
                }
            })
            .collect()
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
        // The show restarts with its music after 63 seconds (0x41c840).
        if self.time - self.started > crate::scene::SHOW_LENGTH {
            self.begin();
        }
        if self.mode != Mode::Movie && self.time - self.touched > demo::IDLE {
            self.mode = Mode::Movie;
        }
    }

    pub fn level(&self) -> f32 {
        1.0
    }

    pub fn frame(&self) -> Frame {
        Frame {
            time: self.time - self.started,
            camera: match self.mode {
                Mode::Movie => Camera::Movie(self.movie),
                Mode::Free => Camera::Free {
                    yaw: self.yaw,
                    pitch: self.pitch,
                    distance: self.distance,
                    pan: self.pan,
                },
            },
            bugs: self.bugs(self.time - self.started),
            sliders: self.sliders,
            white: 0.0,
            logo: self.logo,
            paused: self.paused,
            sliders_open: self.menu.sliders,
            about: self.time < self.menu.about_until,
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
            "1" => {
                self.mode = Mode::Movie;
                self.movie = 0;
                self.paused = false;
            }
            "2" => {
                self.mode = Mode::Movie;
                self.movie = 1;
                self.paused = false;
            }
            "f" | "F" => self.mode = Mode::Free,
            "Home" => self.home(),
            _ => return false,
        }
        true
    }

    fn action(&mut self, callback: &str) {
        self.touched = self.time;
        match callback {
            "Camera1CB" => {
                self.mode = Mode::Movie;
                self.movie = 0;
                self.paused = false;
            }
            "Camera2CB" => {
                self.mode = Mode::Movie;
                self.movie = 1;
                self.paused = false;
            }
            "SlidersCB" => self.menu.sliders = !self.menu.sliders,
            "ResetCB" => {
                self.home();
                self.mode = Mode::Movie;
                self.movie = 0;
            }
            "PauseCB" => self.paused = !self.paused,
            "RestartCB" => self.begin(),
            "AboutCB" => self.menu.about_until = self.time + 5.0,
            "QuitCB" => self.quit = true,
            _ => {}
        }
    }

    /// Stops at a moment. A key sets the rest up; an empty one leaves the movie.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, key: &str) {
        if !key.is_empty() {
            self.key(key);
        }
        self.started = self.time - seconds;
        self.turn(yaw as f32, pitch as f32);
        self.paused = true;
    }

    fn turn(&mut self, yaw: f32, pitch: f32) {
        self.yaw += yaw;
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
        if let Some(hit) = self.menu.press(
            &self.menu_def,
            x as f32,
            y as f32,
            self.time,
            self.sliders.len(),
        ) {
            match hit {
                Press::Button(callback) => self.action(callback),
                Press::Slider(i, v) => {
                    self.dragging = Some(i);
                    self.slide(i, v);
                }
            }
            self.drag = None;
            return;
        }
        self.dragging = None;
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some(i) = self.dragging
            && let Some((_, v)) =
                self.menu
                    .drag(&self.menu_def, x as f32, y as f32, self.sliders.len())
        {
            self.slide(i, v);
            self.drag = Some((x, y));
            return;
        }
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
        self.dragging = None;
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

    pub fn about(&self) -> f64 {
        self.about
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn the_swarm_loops_and_faces_forward() {
        let s = Show::new(glam::Vec3::ZERO, crate::menu::MenuDef::default());
        let (a, b) = (s.bugs(0.0), s.bugs(0.37));
        assert_eq!((a.len(), b.len()), (demo::BUGS, demo::BUGS));
        let mut moved = 0;
        for (x, y) in a.iter().zip(&b) {
            assert!(x.position.is_finite() && y.position.is_finite());
            assert!((x.yaw - y.yaw).abs() > 1e-3 || (x.phase - y.phase).abs() > 1e-3);
            moved += usize::from((x.position - y.position).length() > 1e-3);
        }
        assert!(moved > demo::BUGS / 2);
        // Facing along the flight direction.
        for bug in &a {
            assert!(bug.yaw.is_finite() && (0.0..1.0).contains(&bug.phase));
        }
    }

    #[test]
    fn cameras_switch_and_free_takes_hold() {
        let mut s = Show::new(glam::Vec3::ZERO, crate::menu::MenuDef::default());
        s.key("2");
        assert_eq!((s.mode(), s.camera()), (Mode::Movie, 1));
        s.key("f");
        assert_eq!(s.mode(), Mode::Free);
        s.press(0.5, 0.5);
        s.motion(0.75, 0.5);
        match s.frame().camera {
            Camera::Free { yaw, .. } => assert!((yaw + std::f32::consts::PI / 2.0).abs() < 0.01),
            _ => panic!("free camera"),
        }
    }

    #[test]
    fn menu_buttons_and_sliders_act() {
        use crate::menu::{Menu, MenuDef};
        let mut def = MenuDef::default();
        def.buttons.push(crate::menu::Button {
            callback: "Camera2CB",
            image: String::new(),
            position: Vec2::new(0.0, 0.8),
            size: Vec2::new(0.2, 0.05),
        });
        def.sliders.push(crate::menu::Slider {
            index: 0,
            position: Vec2::new(0.0, 0.0),
            size: Vec2::new(0.4, 0.02),
        });
        let mut show = Show::new(Vec3::ZERO, def);
        show.press(0.1, 0.17);
        assert_eq!((show.mode(), show.camera()), (Mode::Movie, 1));
        show.menu.sliders = true;
        show.press(0.1, 0.99);
        show.motion(0.3, 0.99);
        assert!((show.sliders()[0] - 0.75).abs() < 1e-6);
        show.release();
        let menu = Menu::default();
        assert!(!menu.sliders);
    }
}
