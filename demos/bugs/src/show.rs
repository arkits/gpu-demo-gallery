//! The opening sequence, flock, navigation and sound cues.
use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3};

use crate::{demo, stage::Ground};

#[derive(Clone, Copy)]
pub struct Cue {
    pub name: &'static str,
    pub repeat: bool,
    pub gain: f32,
    pub pan: f32,
}

const STEP: f32 = 1.0 / 60.0;
const HOME: Vec2 = Vec2::new(250.0, 0.0);
const WAYPOINTS: [Vec2; 7] = [
    Vec2::new(-300.0, -100.0),
    Vec2::new(-100.0, -30.0),
    Vec2::new(-100.0, -30.0),
    Vec2::new(-100.0, 30.0),
    Vec2::new(1000.0, 0.0),
    Vec2::new(-1000.0, 300.0),
    Vec2::new(1000.0, 0.0),
];
const OBSTACLES: [Vec2; 7] = [
    Vec2::new(f32::from_bits(0xc2dbbb64), f32::from_bits(0xc39efd0e)),
    Vec2::new(f32::from_bits(0xc3881a1d), f32::from_bits(0xc3a9b312)),
    Vec2::new(f32::from_bits(0xc3805042), f32::from_bits(0x438ca49c)),
    Vec2::new(f32::from_bits(0xc1876a4b), f32::from_bits(0xc10d0f13)),
    Vec2::new(f32::from_bits(0x42d9c106), f32::from_bits(0xc43a2df4)),
    Vec2::new(f32::from_bits(0xc3d5af3b), f32::from_bits(0xc1250069)),
    Vec2::new(f32::from_bits(0xc3328831), f32::from_bits(0xc3d01c6a)),
];

#[derive(Debug, Clone, Copy, Default)]
struct Foot {
    start: Vec2,
    goal: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub struct Bug {
    pub at: Vec2,
    /// Around the vertical. Zero points along +z.
    pub heading: f32,
    speed: f32,
    stamina: f32,
    resting: bool,
    feet: [Foot; 6],
    stepping: usize,
    progress: f32,
}

fn forward(heading: f32) -> Vec2 {
    Vec2::new(heading.sin(), heading.cos())
}

fn random(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(214013).wrapping_add(2531011);
    ((*state >> 16) & 32767) as f32 / 32767.0
}

impl Bug {
    fn spawn(rng: &mut u32, at: Vec2) -> Self {
        Self {
            at,
            heading: std::f32::consts::FRAC_PI_2,
            speed: 0.0,
            stamina: random(rng),
            resting: false,
            feet: [Foot::default(); 6],
            stepping: 1,
            progress: 1.0,
        }
    }

    fn stance(&self, offsets: &[Vec2; 6], i: usize) -> Vec2 {
        self.at + Vec2::from_angle(-self.heading).rotate(offsets[i])
    }

    fn plant(&mut self, offsets: &[Vec2; 6]) {
        for i in 0..6 {
            let at = self.stance(offsets, i);
            self.feet[i] = Foot {
                start: at,
                goal: at,
            };
        }
        self.progress = 1.0;
    }

    fn walk(&mut self, offsets: &[Vec2; 6]) {
        self.at += forward(self.heading) * self.speed * STEP;
        if self.progress < 1.0 {
            self.progress = (self.progress + STEP * (5.0 + self.speed * 0.10)).min(1.0);
            return;
        }
        let next = self.stepping ^ 1;
        if (next..6)
            .step_by(2)
            .any(|i| self.feet[i].goal.distance_squared(self.stance(offsets, i)) > 9.3_f32.powi(2))
        {
            self.stepping = next;
            self.progress = 0.0;
            for i in (next..6).step_by(2) {
                self.feet[i] = Foot {
                    start: self.feet[i].goal,
                    goal: self.stance(offsets, i)
                        + forward(self.heading) * (self.speed * 0.10).min(9.3),
                };
            }
        }
    }

    pub(crate) fn foot(&self, i: usize) -> (Vec2, f32) {
        let foot = self.feet[i];
        if i % 2 == self.stepping && self.progress < 1.0 {
            let t = self.progress;
            (
                foot.start.lerp(foot.goal, t),
                9.3 * (std::f32::consts::PI * t).sin(),
            )
        } else {
            (foot.goal, 0.0)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Vision {
    Normal,
    Night,
    Depth,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DrawStyle {
    Solid,
    Lines,
    Points,
}

pub struct Show {
    last_wall: Option<f64>,
    rest: f64,
    ticks: u32,
    phase: usize,
    phase_start: u32,
    bugs: Vec<Bug>,
    previous: Vec<Bug>,
    count: usize,
    rng: u32,
    ground: Arc<Ground>,
    feet: [Vec2; 6],
    camera: Vec2,
    yaw: f32,
    pitch: f32,
    elevation: f32,
    drag: Option<(f64, f64)>,
    movement: u8,
    light_control: bool,
    light_offset: Vec2,
    light_height: f32,
    cues: Vec<Cue>,
    vision: Vision,
    style: DrawStyle,
    alias: bool,
    menu: bool,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    pub seconds: f32,
    pub bugs: Vec<Bug>,
    pub vision: Vision,
    pub style: DrawStyle,
    pub alias: bool,
    pub menu: bool,
    pub paused: bool,
    pub light_control: bool,
    pub view: Mat4,
    pub lamp: Vec3,
    pub target: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub logo: bool,
}

pub(crate) struct MenuButton {
    pub key: &'static str,
    icon: &'static str,
}

impl MenuButton {
    pub fn texture(&self, pressed: bool) -> String {
        let state = if pressed { "down" } else { "up" };
        format!("userinterface/button_{}_{state}_small.dds", self.icon)
    }
}

pub(crate) const MENU: &[MenuButton] = &[
    MenuButton {
        key: "1",
        icon: "normal",
    },
    MenuButton {
        key: "2",
        icon: "nightvision",
    },
    MenuButton {
        key: "3",
        icon: "depthvision",
    },
    MenuButton {
        key: "s",
        icon: "solid",
    },
    MenuButton {
        key: "w",
        icon: "lines",
    },
    MenuButton {
        key: "p",
        icon: "points",
    },
    MenuButton {
        key: "n",
        icon: "walk",
    },
    MenuButton {
        key: "l",
        icon: "light",
    },
    MenuButton {
        key: "a",
        icon: "alias_on",
    },
];

pub(crate) fn menu_rect(index: usize) -> [f32; 4] {
    [
        0.025 + (index % 3) as f32 * 0.162,
        0.025 + (index / 3) as f32 * 0.064,
        0.155,
        0.057,
    ]
}

impl Frame {
    pub fn selected(&self, key: &str) -> bool {
        match key {
            "1" => self.vision == Vision::Normal,
            "2" => self.vision == Vision::Night,
            "3" => self.vision == Vision::Depth,
            "s" => self.style == DrawStyle::Solid,
            "w" => self.style == DrawStyle::Lines,
            "p" => self.style == DrawStyle::Points,
            "l" => self.light_control,
            "n" => !self.light_control,
            "a" => self.alias,
            "Tab" => self.menu,
            " " => self.paused,
            _ => false,
        }
    }
}

impl Show {
    pub(crate) fn new(ground: Arc<Ground>, feet: [Vec2; 6]) -> Self {
        Self {
            last_wall: None,
            rest: 0.0,
            ticks: 0,
            phase: 1,
            phase_start: 0,
            bugs: Vec::with_capacity(demo::BUGS),
            previous: Vec::with_capacity(demo::BUGS),
            count: demo::BUGS,
            rng: 1,
            ground,
            feet,
            camera: HOME,
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            elevation: 20.0,
            drag: None,
            movement: 0,
            light_control: false,
            light_offset: Vec2::ZERO,
            light_height: 0.0,
            cues: vec![Cue {
                name: demo::INITIAL_MUSIC,
                repeat: true,
                gain: 0.5,
                pan: 0.0,
            }],
            vision: Vision::Normal,
            style: DrawStyle::Solid,
            alias: false,
            menu: false,
            paused: false,
            logo: true,
        }
    }

    fn spawn(&mut self, x: f32, z: Option<f32>) {
        let at = Vec2::new(x, z.unwrap_or_else(|| random(&mut self.rng) * 100.0 - 50.0));
        let mut bug = Bug::spawn(&mut self.rng, at);
        bug.plant(&self.feet);
        self.bugs.push(bug);
    }

    fn set_phase(&mut self, phase: usize) {
        self.phase = phase;
        self.phase_start = self.ticks;
        match phase {
            2 => self.spawn(-1800.0, Some(-250.0)),
            4 | 6 => {
                let b = &mut self.bugs[0];
                let waypoint = WAYPOINTS[phase - 2];
                if b.at.distance(waypoint) > 100.0 {
                    b.at = waypoint;
                    b.speed = 0.0;
                    b.plant(&self.feet);
                }
            }
            _ => {}
        }
    }

    fn step(&mut self) {
        self.ticks += 1;
        let elapsed = (self.ticks - self.phase_start) as f32 * STEP;
        let phase = self.phase;
        let waypoint = phase.checked_sub(2).map(|i| WAYPOINTS[i]);
        let finish = match phase {
            1 => elapsed >= 25.0,
            2 | 3 | 5 | 7 => self.bugs[0].at.distance(waypoint.unwrap()) < 50.0 || elapsed >= 10.0,
            4 => elapsed >= 5.0,
            6 => elapsed >= 20.0,
            _ => false,
        };
        if finish {
            self.set_phase(phase + 1);
        }
        if self.phase == 8 {
            let elapsed = (self.ticks - self.phase_start) as f32 * STEP;
            let wanted = (1 + (elapsed * self.count as f32 / 23.0) as usize).min(self.count);
            while self.bugs.len() < wanted {
                self.spawn(-2000.0, None);
            }
        }
        self.previous.clone_from(&self.bugs);
        for (i, bug) in self.bugs.iter_mut().enumerate() {
            if bug.at.x > 500.0 {
                let lane = random(&mut self.rng) * 100.0 - 50.0;
                *bug = Bug::spawn(&mut self.rng, Vec2::new(-2000.0, lane));
                bug.plant(&self.feet);
            }
            if bug.stamina <= 0.0 && !bug.resting && self.phase == 8 {
                bug.resting = true;
                let offset = bug.at - self.camera;
                if offset.length() < 750.0 {
                    let sound = (random(&mut self.rng) * demo::VOICES.len() as f32) as usize;
                    self.cues.push(Cue {
                        name: demo::VOICES[sound.min(demo::VOICES.len() - 1)],
                        repeat: false,
                        gain: attenuation(offset.length(), 140.0),
                        pan: pan(self.yaw, offset),
                    });
                }
            }
            if bug.resting {
                bug.stamina = (bug.stamina + 0.5 * STEP).min(1.0);
                bug.speed = (bug.speed - 100.0 * STEP).max(0.0);
                bug.resting = bug.stamina < 1.0;
            } else if !matches!(self.phase, 4 | 6) {
                let target = WAYPOINTS[self.phase - 2];
                let mut separation = Vec2::ZERO;
                let mut alignment = Vec2::ZERO;
                let mut center = Vec2::ZERO;
                let mut neighbors = 0;
                for (j, other) in self.previous.iter().enumerate() {
                    let delta = bug.at - other.at;
                    let squared = delta.length_squared();
                    if i != j && (0.01..=40_000.0).contains(&squared) {
                        separation += delta / squared;
                        alignment += forward(other.heading) / squared;
                        center += other.at;
                        neighbors += 1;
                    }
                }
                let mut wish = (target - bug.at).normalize_or(Vec2::X);
                if neighbors != 0 {
                    wish += 100.0 * separation
                        + alignment.normalize_or_zero() * 0.5
                        + (bug.at - center / neighbors as f32).normalize_or_zero() * 0.1;
                }
                for obstacle in OBSTACLES {
                    let away = obstacle - bug.at;
                    let distance = away.length().max(1.0);
                    wish += away / distance * (-10.0 / distance - 10_000.0 / distance.powi(2));
                }
                let current = forward(bug.heading);
                let wish = wish.normalize_or(current);
                let turn = current.perp_dot(wish).atan2(current.dot(wish));
                bug.heading -= turn.clamp(-0.5 * STEP, 0.5 * STEP);
                let max_speed = if matches!(self.phase, 3 | 5) {
                    30.0
                } else if self.phase == 8 {
                    100.0
                } else {
                    200.0
                };
                bug.speed = (bug.speed + current.dot(wish) * 100.0 * STEP).clamp(5.0, max_speed);
                if self.phase == 8 {
                    bug.stamina -= 0.001 * bug.speed * STEP;
                }
            } else {
                bug.speed = (bug.speed - 100.0 * STEP).max(0.0);
            }
            bug.walk(&self.feet);
        }
    }

    fn run(&mut self, seconds: f64) {
        self.rest += seconds;
        while self.rest >= f64::from(STEP) {
            self.rest -= f64::from(STEP);
            self.step();
        }
    }

    pub fn advance(&mut self, now: f64) {
        let dt = self
            .last_wall
            .replace(now)
            .map_or(0.0, |last| (now - last).clamp(0.0, 0.25));
        if !self.paused {
            self.run(dt);
        }
        let motion = |positive, negative| {
            f32::from(self.movement & positive != 0) - f32::from(self.movement & negative != 0)
        };
        let step = forward(self.yaw) * motion(1, 2)
            + forward(self.yaw - std::f32::consts::FRAC_PI_2) * motion(8, 4);
        self.move_by(step * (dt as f32 * 140.0));
    }

    pub fn frame(&self) -> Frame {
        let seconds = self.ticks as f32 * STEP;
        let at = self.point(self.camera) + Vec3::Y * self.elevation;
        let front = Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos()) * self.pitch.cos()
            + Vec3::Y * self.pitch.sin();
        let (lamp, beam) = self.lamp(seconds);
        Frame {
            seconds,
            bugs: self.bugs.clone(),
            vision: self.vision,
            style: self.style,
            alias: self.alias,
            menu: self.menu,
            paused: self.paused,
            light_control: self.light_control,
            view: glam::camera::rh::view::look_at_mat4(at, at + front, Vec3::Y),
            lamp,
            target: beam,
            fov: 0.7,
            near: 4.0,
            far: 8000.0,
            logo: self.logo,
        }
    }

    fn lamp(&self, seconds: f32) -> (Vec3, Vec3) {
        let target = if (2..8).contains(&self.phase) {
            self.bugs[0].at
        } else {
            self.camera + forward(self.yaw) * 350.0 + Vec2::from_angle(seconds * 0.4) * 200.0
        };
        let target = (target + self.light_offset).clamp(Vec2::splat(-1900.0), Vec2::splat(1900.0));
        let beam = self.point(target);
        let base =
            self.camera + forward(self.yaw) * 130.75 + Vec2::from_angle(seconds * 0.5) * 300.0;
        let lamp = Vec3::new(
            base.x + self.light_offset.x,
            (700.0 + 300.0 * (seconds * 0.013).cos() + self.light_height).clamp(400.0, 1000.0),
            base.y + self.light_offset.y,
        );
        (lamp, beam)
    }

    fn point(&self, at: Vec2) -> Vec3 {
        Vec3::new(at.x, self.ground.height(at), at.y)
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        let mut cues = std::mem::take(&mut self.cues);
        let (lamp, _) = self.lamp(self.ticks as f32 * STEP);
        let offset = Vec2::new(lamp.x, lamp.z) - self.camera;
        let distance = (lamp - (self.point(self.camera) + Vec3::Y * self.elevation)).length();
        cues.push(Cue {
            name: "heli",
            repeat: true,
            gain: 0.7 * attenuation(distance, 500.0),
            pan: pan(self.yaw, offset),
        });
        if self.phase == 8 {
            let nearest = self
                .bugs
                .iter()
                .map(|b| b.at.distance(self.camera))
                .fold(f32::INFINITY, f32::min);
            let gain = ((self.ticks - self.phase_start) as f32 * STEP / 15.0).min(1.0);
            cues.push(Cue {
                name: "horde_rumble",
                repeat: true,
                gain: gain * attenuation(nearest, 800.0),
                pan: 0.0,
            });
        }
        cues
    }

    pub fn key(&mut self, key: &str) -> bool {
        if let Some(bit) = movement(key) {
            self.movement |= bit;
            return true;
        }
        match key {
            " " => self.paused = !self.paused,
            "Home" => {
                self.camera = HOME;
                self.yaw = -std::f32::consts::FRAC_PI_2;
                self.pitch = 0.0;
                self.elevation = 20.0;
                self.light_offset = Vec2::ZERO;
                self.light_height = 0.0;
            }
            "1" => self.vision = Vision::Normal,
            "2" => self.vision = Vision::Night,
            "3" => self.vision = Vision::Depth,
            "s" | "S" => self.style = DrawStyle::Solid,
            "w" | "W" => self.style = DrawStyle::Lines,
            "p" | "P" => self.style = DrawStyle::Points,
            "l" | "L" => self.light_control = !self.light_control,
            "n" | "N" => self.light_control = false,
            "a" | "A" => self.alias = !self.alias,
            "Tab" | "Escape" => self.menu = !self.menu,
            "+" | "=" => {
                self.count = (self.count + 1).min(200);
                if self.phase == 8 && self.bugs.len() < self.count {
                    self.spawn(-2000.0, None);
                }
            }
            "-" | "_" => {
                self.count = self.count.saturating_sub(1);
                if self.phase == 8 {
                    self.bugs.pop();
                }
            }
            _ => return false,
        }
        true
    }

    pub fn key_up(&mut self, key: &str) {
        if let Some(bit) = movement(key) {
            self.movement &= !bit;
        }
    }

    pub fn clear_input(&mut self) {
        self.movement = 0;
        self.drag = None;
    }

    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, action: bool) {
        self.run(seconds + 1e-4);
        self.rest = 0.0;
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(-1.42, 1.42);
        self.paused = true;
        if action {
            self.vision = Vision::Depth;
        }
    }

    pub fn press(&mut self, x: f64, y: f64) {
        if self.menu {
            for (i, button) in MENU.iter().enumerate() {
                let [left, top, width, height] = menu_rect(i);
                if (left..left + width).contains(&(x as f32))
                    && (top..top + height).contains(&(y as f32))
                {
                    if button.key == "l" {
                        self.light_control = true;
                    } else {
                        self.key(button.key);
                    }
                    return;
                }
            }
        }
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            if self.light_control {
                self.light_offset += Vec2::new((x - px) as f32, (y - py) as f32) * 900.0;
            } else {
                self.yaw -= (x - px) as f32 * std::f32::consts::TAU;
                self.pitch =
                    (self.pitch - (y - py) as f32 * std::f32::consts::PI).clamp(-1.42, 1.42);
            }
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        if self.light_control {
            self.light_height = (self.light_height + 240.0 * amount).clamp(-600.0, 600.0);
        } else {
            self.move_by(forward(self.yaw) * amount * 140.0);
        }
    }

    fn move_by(&mut self, step: Vec2) {
        if self.light_control {
            self.light_offset =
                (self.light_offset + step).clamp(Vec2::splat(-1800.0), Vec2::splat(1800.0));
        } else {
            self.camera = (self.camera + step).clamp(Vec2::splat(-1900.0), Vec2::splat(1900.0));
        }
    }
}

fn movement(key: &str) -> Option<u8> {
    Some(match key {
        "ArrowUp" => 1,
        "ArrowDown" => 2,
        "ArrowLeft" => 4,
        "ArrowRight" => 8,
        _ => return None,
    })
}

fn attenuation(distance: f32, reference: f32) -> f32 {
    1.0 / (1.0 + (distance / reference).powi(2))
}

fn pan(yaw: f32, offset: Vec2) -> f32 {
    forward(yaw - std::f32::consts::FRAC_PI_2).dot(offset.normalize_or_zero())
}
