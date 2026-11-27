//! Time, camera and input, and the squid's own decisions.
use crate::demo;
use glam::{EulerRot, Mat4, Vec3};

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
}

/// The program moves once a frame; here 60 times a second [I].
const STEP: f32 = 1.0 / 60.0;
/// Radians per second that the squid turns (0x5330e8).
const TURN: f32 = 0.105;
/// Where the squid keeps itself (0x5330dc, 0x5330e0).
const HEIGHTS: (f32, f32) = (10.0, 50.0);
/// Speed after a stroke, and what is left of it a frame later (0x5330ec, 0x5330f0).
const STROKE: f32 = -4.0;
const DRAG: f32 = 0.98;
/// Seconds of a swimming stroke.
const PERIOD: f32 = 3.75;
/// Seconds that a steering key counts for.
const HOLD: f32 = 0.4;
/// Seconds without a key after which the squid steers itself again.
const IDLE: f32 = 8.0;

pub const SKINS: usize = 3;

/// A skin gives way to the next from -1 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Morph {
    pub skin: usize,
    pub blend: Option<f32>,
}

impl Morph {
    /// After 0x40f210.
    fn step(&mut self) {
        if let Some(b) = &mut self.blend {
            *b += match *b {
                b if b < 0.0 => 0.1,
                b if b < 0.7 => 0.05,
                _ => 0.01,
            };
            if *b >= 1.0 {
                self.skin = (self.skin + 1) % SKINS;
                self.blend = None;
            }
        }
    }
}

struct Squid {
    position: Vec3,
    pitch: f32,
    yaw: f32,
    bank: f32,
    speed: f32,
    /// How firmly it returns to its heights.
    correction: f32,
    time: f32,
    next_stroke: f32,
    morph: Morph,
    /// Seconds since the last changes of course and skin, and the last blink.
    across: f32,
    along: f32,
    changed: f32,
    blinked: f32,
    next_blink: f32,
    blink: f32,
    /// Its own idea: left or right, up or down.
    course: (f32, f32),
    random: u32,
}

impl Squid {
    fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 30.0, 0.0),
            pitch: 0.0,
            yaw: 0.0,
            bank: 0.0,
            speed: 0.0,
            correction: 0.0,
            time: 0.0,
            next_stroke: 0.0,
            morph: Morph {
                skin: 0,
                blend: None,
            },
            across: 0.0,
            along: 4.0 / 3.0,
            changed: 0.0,
            blinked: 0.0,
            next_blink: 5.0,
            blink: -10.0,
            course: (0.0, 0.0),
            random: 1,
        }
    }

    /// The C library's generator, 0 to 32767.
    fn rand(&mut self) -> u32 {
        self.random = self.random.wrapping_mul(214_013).wrapping_add(2_531_011);
        self.random >> 16 & 0x7fff
    }

    fn pick(&mut self) -> f32 {
        let (a, b) = (self.rand() & 1, self.rand() & 1);
        a as f32 - b as f32
    }

    /// After 0x410e00: changes its mind from time to time.
    fn decide(&mut self) {
        self.across += STEP;
        self.along += STEP;
        self.changed += STEP;
        if self.across > 6.0 {
            self.course.0 = self.pick();
            self.across = 0.0;
        }
        if self.along > 8.0 {
            self.course.1 = self.pick();
            self.along = 0.0;
        }
    }

    /// After 0x410bb0.
    fn steer(&mut self, (across, up): (f32, f32)) {
        let turn = TURN * STEP;
        if across != 0.0 {
            self.yaw += turn * across;
            if self.bank * across < 0.7 {
                self.bank += 2.0 * turn * across;
            }
        } else {
            self.bank -= STEP * self.bank * 0.5;
        }
        let y = self.position.y;
        if (HEIGHTS.0..=HEIGHTS.1).contains(&y) {
            if up == 0.0 {
                self.pitch -= STEP * self.pitch * 0.5;
            } else {
                self.pitch += turn * up;
                self.correction = 0.0;
            }
            return;
        }
        let back = if y > HEIGHTS.1 { -1.0 } else { 1.0 };
        self.pitch -= STEP * self.pitch * 0.5 * self.correction;
        self.pitch += turn * back * if up == back { 1.0 } else { self.correction };
        self.correction = (self.correction + STEP * 0.5).min(1.0);
    }

    fn matrix(&self) -> Mat4 {
        Mat4::from_translation(self.position)
            * Mat4::from_euler(EulerRot::YXZ, self.yaw, self.pitch, self.bank)
    }

    /// Returns the sounds to start.
    fn step(&mut self, player: Option<(f32, f32)>) -> Vec<&'static str> {
        let mut sounds = Vec::new();
        self.time += STEP;
        if self.time >= self.next_stroke {
            if self.next_stroke > 0.0 {
                sounds.push(demo::THRUST);
            }
            self.next_stroke += PERIOD;
            self.speed = STROKE;
        }
        match player {
            Some(p) => self.steer(p),
            None => {
                self.decide();
                self.steer(self.course);
            }
        }
        if self.changed > 20.0 && self.morph.blend.is_none() {
            self.changed = 0.0;
            self.morph.blend = Some(-1.0);
            sounds.push(demo::MORPH);
        }
        self.blinked += STEP;
        if self.blinked > self.next_blink {
            self.blinked = 0.0;
            self.blink = self.time;
            self.next_blink = self.rand() as f32 * 0.000_122_074_04 + 3.0;
            sounds.push(demo::BLINK);
        }
        self.morph.step();
        let forward = self.matrix().z_axis.truncate();
        self.position += forward * self.speed * STEP * 20.0;
        self.speed *= DRAG;
        sounds
    }
}

pub struct Show {
    last_wall: Option<f64>,
    /// Time not yet stepped.
    rest: f64,
    squid: Squid,
    yaw: f32,
    pitch: f32,
    distance: f32,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    /// Steering by keys, and until when it counts.
    wish: (f32, f32),
    until: f32,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub seconds: f32,
    pub squid: Mat4,
    pub center: Vec3,
    pub morph: Morph,
    /// Seconds since the last blink began.
    pub blink: f32,
    pub view: Mat4,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub logo: bool,
}

const HOME: (f32, f32, f32) = (0.85, 0.16, 58.0);

impl Default for Show {
    fn default() -> Self {
        Self {
            last_wall: None,
            rest: 0.0,
            squid: Squid::new(),
            yaw: HOME.0,
            pitch: HOME.1,
            distance: HOME.2,
            drag: None,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
            }],
            wish: (0.0, 0.0),
            until: -IDLE,
            paused: false,
            logo: true,
        }
    }
}

impl Show {
    fn run(&mut self, seconds: f64) {
        self.rest += seconds;
        while self.rest >= f64::from(STEP) {
            self.rest -= f64::from(STEP);
            let now = self.squid.time;
            if now > self.until {
                self.wish = (0.0, 0.0);
            }
            let player = (now < self.until + IDLE).then_some(self.wish);
            for name in self.squid.step(player) {
                self.cues.push(Cue {
                    name,
                    repeat: false,
                });
            }
        }
    }

    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        if !self.paused {
            self.run(dt);
        }
    }

    pub fn frame(&self) -> Frame {
        let orbit = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        // Between two steps.
        let part = self.rest as f32;
        let squid = self.squid.matrix();
        let drift = squid.z_axis.truncate() * self.squid.speed * part * 20.0;
        let squid = Mat4::from_translation(drift) * squid;
        // The middle of the body.
        let center = squid.transform_point3(Vec3::new(0.0, 0.0, -4.0));
        let mut eye = center + self.distance * orbit;
        // Above the plants.
        eye.y = eye.y.max(12.0);
        Frame {
            seconds: self.squid.time + part,
            squid,
            center,
            morph: self.squid.morph,
            blink: self.squid.time + part - self.squid.blink,
            view: glam::camera::rh::view::look_at_mat4(eye, center, Vec3::Y),
            fov: 0.85,
            near: 1.0,
            far: 900.0,
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    fn wish(&mut self, across: f32, up: f32) {
        self.wish = (across, up);
        self.until = self.squid.time + HOLD;
    }

    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " | "p" | "P" => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            "Home" => (self.yaw, self.pitch, self.distance) = HOME,
            "ArrowLeft" => self.wish(1.0, 0.0),
            "ArrowRight" => self.wish(-1.0, 0.0),
            "ArrowUp" => self.wish(0.0, 1.0),
            "ArrowDown" => self.wish(0.0, -1.0),
            "s" | "S" => self.squid.changed = 21.0,
            _ => return false,
        }
        true
    }

    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        self.run(seconds + 1e-4);
        self.rest = 0.0;
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(-1.3, 1.42);
        self.paused = true;
        if action {
            self.key("s");
            self.run(0.45);
            self.rest = 0.0;
        }
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.yaw -= (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(-1.3, 1.42);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount).exp()).clamp(30.0, 200.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swims_and_stays_at_its_heights() {
        let mut s = Squid::new();
        for _ in 0..60 * 300 {
            s.step(None);
            assert!((-30.0..90.0).contains(&s.position.y), "{}", s.position);
            assert!(s.bank.abs() < 0.8);
        }
        assert!(s.position.length() > 100.0);
    }

    #[test]
    fn morphs_once_every_twenty_seconds() {
        let mut s = Squid::new();
        (0..60 * 19).for_each(|_| drop(s.step(None)));
        assert_eq!(s.morph.blend, None);
        (0..60 * 3).for_each(|_| drop(s.step(None)));
        assert_eq!((s.morph.skin, s.morph.blend), (1, None));
    }
}
