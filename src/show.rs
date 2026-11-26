//! Time, camera and input, and what the wolfman does.
use std::collections::VecDeque;

use crate::demo::{
    self, BREATH_TO_WALK, BREATHE, HOWL, KEYS, RUN, RUN_TO_WALK, SCRATCH, SCRIPT, SEGMENTS,
    SIDEWAYS, WALK, WALK_TO_BREATH, WALK_TO_RUN,
};
use glam::{Mat4, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Starts a sound that repeats.
    Repeat,
    /// Starts a sound once.
    Once,
    Stop,
    /// How fast sounds play, 1 being their own speed.
    Rate(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub name: &'static str,
    pub action: Action,
}

/// How fast slow motion is (0x402870, key O).
const SLOW: f64 = 0.3;

pub struct Show {
    /// Seconds into the current segment.
    since: f64,
    segment: usize,
    /// What comes next.
    queue: VecDeque<usize>,
    /// The script fills the queue again until a key is pressed (0x401760).
    script: bool,
    /// The first key drops the rest of the script.
    first_key: bool,
    /// How far the street had gone when the segment began.
    origin: f32,
    last_wall: Option<f64>,
    center: Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,
    distance: f32,
    drag: Option<(f64, f64)>,
    cues: Vec<Cue>,
    /// Shells of fur, and how long the outermost is with all eight (0x402390).
    layers: u32,
    length: f32,
    /// How far the street goes by animation, per ten milliseconds, as the scene has it.
    ground: Vec<f32>,
    pub slow: bool,
    pub paused: bool,
    pub logo: bool,
}

pub struct Frame {
    /// Milliseconds into the scene's animation.
    pub ms: f64,
    /// How far the street has gone.
    pub street: f32,
    /// Shells of fur, and how far apart they are.
    pub layers: u32,
    pub spacing: f32,
    pub view: Mat4,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub logo: bool,
}

fn key_time(frame: u32) -> f64 {
    f64::from(frame) * 1000.0 / 30.0
}

fn seconds(segment: usize) -> f64 {
    let (a, b) = SEGMENTS[segment].frames;
    f64::from(b - a) / 30.0
}

/// What to queue to get from the last segment to an action, as 0x401930 does.
fn route(last: usize, target: usize) -> Vec<usize> {
    let mut route = Vec::new();
    match target {
        BREATHE | SCRATCH | HOWL => {
            match last {
                WALK | SIDEWAYS => route.push(WALK_TO_BREATH),
                RUN => route.extend([RUN_TO_WALK, WALK_TO_BREATH]),
                _ => {}
            }
            route.extend([target, BREATHE]);
        }
        WALK | SIDEWAYS => {
            match last {
                BREATHE | SCRATCH | HOWL => route.push(BREATH_TO_WALK),
                RUN => route.push(RUN_TO_WALK),
                _ => {}
            }
            route.extend([target, WALK]);
        }
        RUN => {
            match last {
                BREATHE | SCRATCH | HOWL => route.extend([BREATH_TO_WALK, WALK_TO_RUN]),
                WALK | SIDEWAYS => route.push(WALK_TO_RUN),
                _ => {}
            }
            route.push(RUN);
        }
        _ => {}
    }
    route
}

const HOME: (f32, f32, f32) = (-1.9, 0.13, 2.6);

impl Show {
    /// `ground` has the street's place every ten milliseconds of the animation.
    pub fn new(center: Vec3, radius: f32, ground: Vec<f32>) -> Self {
        let mut show = Self {
            since: 0.0,
            segment: SCRIPT[0],
            queue: SCRIPT[1..].iter().copied().collect(),
            script: true,
            first_key: true,
            origin: 0.0,
            last_wall: None,
            center,
            radius,
            yaw: HOME.0,
            pitch: HOME.1,
            distance: HOME.2 * radius,
            drag: None,
            cues: vec![Cue {
                name: demo::MUSIC,
                action: Action::Repeat,
            }],
            layers: 8,
            length: 0.7,
            ground,
            slow: false,
            paused: false,
            logo: true,
        };
        show.sound();
        show
    }

    fn ground(&self, ms: f64) -> f32 {
        let at = (ms / 10.0).max(0.0);
        let get = |i: usize| {
            self.ground
                .get(i)
                .or(self.ground.last())
                .copied()
                .unwrap_or(0.0)
        };
        let i = at as usize;
        get(i) + (get(i + 1) - get(i)) * at.fract() as f32
    }

    /// How far the street goes in part of a segment.
    fn travel(&self, segment: usize, seconds: f64) -> f32 {
        let start = key_time(SEGMENTS[segment].frames.0);
        self.ground(start + seconds * 1000.0) - self.ground(start)
    }

    /// Starts the segment's sound, after stopping the last one's (0x4196e0).
    fn sound(&mut self) {
        let s = &SEGMENTS[self.segment];
        let action = match s.repeats {
            true => Action::Repeat,
            false => Action::Once,
        };
        self.cues.push(Cue {
            name: s.sound,
            action,
        });
        if self.slow {
            self.cues.push(Cue {
                name: s.sound,
                action: Action::Rate(SLOW as f32),
            });
        }
    }

    fn stop_sound(&mut self) {
        self.cues.push(Cue {
            name: SEGMENTS[self.segment].sound,
            action: Action::Stop,
        });
    }

    /// Goes on by some seconds of animation, segment after segment (0x419960).
    fn step(&mut self, mut dt: f64) {
        while self.since + dt >= seconds(self.segment) {
            dt -= seconds(self.segment) - self.since;
            self.origin += self.travel(self.segment, seconds(self.segment));
            if self.queue.is_empty() && self.script {
                self.queue.extend(SCRIPT);
            }
            // Without more, a segment that loops goes on.
            let next = self.queue.pop_front().unwrap_or(self.segment);
            self.stop_sound();
            self.segment = next;
            self.since = 0.0;
            self.sound();
        }
        self.since += dt;
    }

    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        if !self.paused {
            self.step(dt * if self.slow { SLOW } else { 1.0 });
        }
    }

    /// Asks for an action: it follows what is queued, or what plays if it is the first.
    fn act(&mut self, target: usize) {
        if std::mem::take(&mut self.first_key) {
            self.queue.clear();
        }
        self.script = false;
        let last = self.queue.back().copied().unwrap_or(self.segment);
        self.queue.extend(route(last, target));
    }

    pub fn frame(&self) -> Frame {
        let orbit = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let eye = self.center + self.distance * orbit;
        let start = key_time(SEGMENTS[self.segment].frames.0);
        Frame {
            ms: start + self.since * 1000.0,
            street: self.origin + self.travel(self.segment, self.since),
            layers: self.layers,
            spacing: 0.125 * self.length,
            view: glam::camera::rh::view::look_at_mat4(eye, self.center, Vec3::Y),
            fov: 0.69,
            near: 1.0,
            far: 2000.0,
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    fn music(&mut self, on: bool) {
        self.cues.push(Cue {
            name: demo::MUSIC,
            action: if on { Action::Repeat } else { Action::Stop },
        });
    }

    pub fn key(&mut self, key: &str) -> bool {
        match key {
            // The segment's sound stops while it is held, and stays stopped (0x4196b0).
            " " | "p" | "P" => {
                self.paused = !self.paused;
                if self.paused {
                    self.stop_sound();
                }
            }
            "l" | "L" => self.logo = !self.logo,
            // Slow motion, without the music (0x402870).
            "o" | "O" => {
                self.slow = !self.slow;
                self.music(!self.slow);
                let rate = if self.slow { SLOW as f32 } else { 1.0 };
                self.cues.push(Cue {
                    name: SEGMENTS[self.segment].sound,
                    action: Action::Rate(rate),
                });
            }
            "Home" => {
                (self.yaw, self.pitch) = (HOME.0, HOME.1);
                self.distance = HOME.2 * self.radius;
            }
            "1" | "2" | "3" | "4" | "5" | "6" => {
                self.act(KEYS[usize::from(key.as_bytes()[0] - b'1')]);
            }
            "+" | "=" => self.layers = (self.layers + 1).min(8),
            "-" => self.layers = self.layers.saturating_sub(1),
            "[" => self.length = (self.length - 0.1).max(0.1),
            "]" => self.length = (self.length + 0.1).min(2.0),
            _ => return false,
        }
        true
    }

    /// Holds a moment: what key 1 to 6 was pressed at the start, if any, and the
    /// camera turned from where it starts.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, key: usize) {
        if (1..=6).contains(&key) {
            self.act(KEYS[key - 1]);
        }
        self.step(seconds);
        self.cues.clear();
        self.yaw += yaw as f32;
        self.pitch = (self.pitch + pitch as f32).clamp(0.02, 1.42);
        self.paused = true;
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y));
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        if let Some((px, py)) = self.drag {
            self.yaw -= (x - px) as f32 * std::f32::consts::TAU;
            self.pitch = (self.pitch + (y - py) as f32 * std::f32::consts::PI).clamp(0.02, 1.42);
            self.drag = Some((x, y));
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance =
            (self.distance * (-amount).exp()).clamp(self.radius * 0.5, self.radius * 8.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show() -> Show {
        // A street that goes one unit a millisecond.
        Show::new(
            Vec3::ZERO,
            1.0,
            (0..4000).map(|i| i as f32 * 10.0).collect(),
        )
    }

    fn sounds(s: &mut Show) -> Vec<(&'static str, Action)> {
        s.take_cues()
            .into_iter()
            .map(|c| (c.name, c.action))
            .collect()
    }

    #[test]
    fn the_script_plays_and_comes_again() {
        let mut s = show();
        assert_eq!(
            sounds(&mut s),
            [
                ("wolf 47b lr", Action::Repeat),
                ("breathloop8", Action::Repeat)
            ]
        );
        let all: f64 = SCRIPT.iter().map(|&i| seconds(i)).sum();
        s.step(seconds(BREATHE) * 2.0 + 0.1);
        assert_eq!(s.segment, SCRATCH);
        s.step(all - seconds(BREATHE) * 2.0);
        assert_eq!(s.segment, BREATHE);
        assert!((s.since - 0.1).abs() < 1e-6);
    }

    #[test]
    fn a_first_key_drops_the_script_and_others_queue() {
        let mut s = show();
        s.step(0.5);
        s.key("6");
        assert_eq!(
            Vec::from(s.queue.clone()),
            [BREATH_TO_WALK, WALK_TO_RUN, RUN]
        );
        s.key("3");
        assert_eq!(
            Vec::from(s.queue.clone()),
            [
                BREATH_TO_WALK,
                WALK_TO_RUN,
                RUN,
                RUN_TO_WALK,
                WALK_TO_BREATH,
                HOWL,
                BREATHE
            ]
        );
        // The breath in progress ends first, and the run repeats until the howl.
        s.step(seconds(BREATHE) - 0.5 + 0.01);
        assert_eq!(s.segment, BREATH_TO_WALK);
    }

    #[test]
    fn an_action_loops_where_it_ends() {
        let mut s = show();
        s.key("4");
        s.step(60.0);
        assert_eq!(s.segment, WALK);
        assert!(s.queue.is_empty());
    }

    #[test]
    fn the_street_moves_on_with_the_segments() {
        let mut s = show();
        s.key("4");
        s.step(seconds(BREATHE) + seconds(BREATH_TO_WALK) + 0.5);
        let ms = |seconds: f64| (seconds * 1000.0) as f32;
        let expected = ms(seconds(BREATHE) + seconds(BREATH_TO_WALK) + 0.5);
        assert!((s.frame().street - expected).abs() < 1.0);
    }

    #[test]
    fn segments_meet_end_to_start() {
        for w in SEGMENTS.windows(2) {
            assert_eq!(w[0].frames.1, w[1].frames.0);
        }
    }
}
