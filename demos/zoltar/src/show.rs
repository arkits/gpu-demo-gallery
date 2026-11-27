//! Time, camera, light and input shared by native and web builds. The numbers are the
//! program's own unless marked [I]; `docs/ENGINE.md` says where they are.
use crate::demo::{self, FORTUNES, GREETING, IDLE};
use glam::{Mat4, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Starts a sound that repeats, this loud.
    Repeat(f32),
    /// Starts a sound, this loud.
    Once(f32),
    Volume(f32),
    Stop,
    /// Holds or goes on with all sounds.
    Pause(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub name: &'static str,
    pub action: Action,
}

/// The sound of the gem.
const GEM: &str = "oneffect";
/// Seconds before the greeting, and before he does something by himself.
const WAIT: f64 = 5.0;
const PATIENCE: f64 = 15.0;
/// How often that is an idle clip, not a fortune.
const IDLENESS: f64 = 0.8;
/// Music while he speaks, and how long it takes to get there.
const SOFT: f32 = 0.5;
const FADE: f64 = 2.0;

const TARGET: Vec3 = Vec3::new(0.0, 12.0, 0.0);
const DISTANCE: f32 = 75.0;
/// Toward the light, as the camera sees it.
pub const LIGHT: Vec3 = Vec3::new(0.62, -0.38, 0.67);
/// Straight up would leave no direction for the camera's sides.
const STEEPEST: f32 = FRAC_PI_2 - 0.01;
/// Holding still for this long ends a spin.
const STILL: f64 = 0.1;

struct Clip {
    item: usize,
    started: f64,
    voiced: bool,
}

pub struct Show {
    time: f64,
    last_wall: Option<f64>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    light: Vec3,
    /// Drags move the light instead of the camera.
    pub moving_light: bool,
    drag: Option<(f64, f64)>,
    /// What drags added since the last frame, when they last did, and what goes on.
    turned: (f32, f32),
    moved: f64,
    spin: (f32, f32),
    cues: Vec<Cue>,
    clip: Option<Clip>,
    /// The last clip and where it ended.
    held: Option<(usize, f32)>,
    ended: f64,
    greeted: bool,
    fortune: usize,
    idle: usize,
    music: f32,
    random: u32,
    pub paused: bool,
    pub logo: bool,
    /// Of the window, width by height.
    pub aspect: f32,
}

pub struct Frame {
    pub view: Mat4,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    /// The clip now or last shown, and seconds into it.
    pub clip: Option<(usize, f32)>,
    pub light: Vec3,
    /// He speaks, and the gem is pressed.
    pub busy: bool,
    pub logo: bool,
}

/// The gem and the logo as parts of the window: left, top, width and height. They keep
/// the shape they have in a 4:3 window.
pub fn gem(aspect: f32) -> [f32; 4] {
    let (w, h) = corner(aspect);
    [1.0 - w, 0.0, w, h]
}

pub fn logo(aspect: f32) -> [f32; 4] {
    let (w, h) = corner(aspect);
    [1.0 - w, 1.0 - h * 0.35, w, h * 0.35]
}

fn corner(aspect: f32) -> (f32, f32) {
    // Narrow windows get a bigger one, for fingers.
    let w = (0.2 * 4.0 / 3.0 / aspect).min(0.36);
    (w, w * aspect * 0.75)
}

impl Default for Show {
    fn default() -> Self {
        Self {
            time: 0.0,
            last_wall: None,
            yaw: 0.0,
            pitch: 0.0,
            distance: DISTANCE,
            light: LIGHT,
            moving_light: false,
            drag: None,
            turned: (0.0, 0.0),
            moved: 0.0,
            spin: (0.0, 0.0),
            cues: vec![Cue {
                name: demo::MUSIC,
                action: Action::Repeat(1.0),
            }],
            clip: None,
            held: None,
            ended: 0.0,
            greeted: false,
            fortune: 0,
            idle: 0,
            music: 1.0,
            random: 1,
            paused: false,
            logo: true,
            aspect: 4.0 / 3.0,
        }
    }
}

impl Show {
    fn home(&mut self) {
        (self.yaw, self.pitch, self.distance) = (0.0, 0.0, DISTANCE);
        self.spin = (0.0, 0.0);
    }

    fn cue(&mut self, name: &'static str, action: Action) {
        self.cues.push(Cue { name, action });
    }

    /// Starts a clip unless one is on.
    fn say(&mut self, item: usize) -> bool {
        if self.clip.is_some() {
            return false;
        }
        self.clip = Some(Clip {
            item,
            started: self.time,
            voiced: demo::CLIPS[item].voice.is_none(),
        });
        true
    }

    /// The fortune asked for, or the next.
    fn tell(&mut self, which: Option<usize>) {
        let n = which.unwrap_or(self.fortune);
        if self.say(FORTUNES.start + n) {
            self.fortune = (n + 1) % FORTUNES.len();
            self.cue(GEM, Action::Once(0.2));
        }
    }

    fn greet(&mut self) {
        self.greeted |= self.say(GREETING);
    }

    fn wait(&mut self) {
        if self.say(IDLE.start + self.idle) {
            self.idle = (self.idle + 1) % IDLE.len();
        }
    }

    fn end(&mut self) {
        if let Some(c) = self.clip.take() {
            let length = demo::CLIPS[c.item].length;
            self.held = Some((c.item, ((self.time - c.started) as f32).min(length)));
            self.ended = self.time;
        }
    }

    /// The C library's generator, which the program never seeds: 0 to 1.
    fn chance(&mut self) -> f64 {
        self.random = self.random.wrapping_mul(214_013).wrapping_add(2_531_011);
        f64::from(self.random >> 16 & 0x7fff) / 32767.0
    }

    pub fn advance(&mut self, wall: f64) {
        let dt = self
            .last_wall
            .replace(wall)
            .map_or(0.0, |last| (wall - last).clamp(0.0, 0.1));
        self.turn(dt, wall);
        if !self.paused {
            self.time += dt;
            self.direct();
        }
    }

    /// A drag goes on when it is let go.
    fn turn(&mut self, dt: f64, wall: f64) {
        let turned = std::mem::take(&mut self.turned);
        if self.drag.is_some() {
            if turned != (0.0, 0.0) && dt > 0.0 {
                self.spin = (turned.0 / dt as f32, turned.1 / dt as f32);
                self.moved = wall;
            } else if wall - self.moved > STILL {
                self.spin = (0.0, 0.0);
            }
        } else {
            let dt = dt as f32;
            self.rotate(self.spin.0 * dt, self.spin.1 * dt);
        }
    }

    fn rotate(&mut self, yaw: f32, pitch: f32) {
        self.yaw = (self.yaw + yaw) % (2.0 * PI);
        self.pitch = (self.pitch + pitch).clamp(0.0, STEEPEST);
    }

    /// What the program does every frame (0x402220).
    fn direct(&mut self) {
        let now = self.time;
        if let Some(c) = &mut self.clip {
            let clip = &demo::CLIPS[c.item];
            let local = now - c.started;
            if local >= f64::from(clip.length) {
                self.end();
            } else if let Some((voice, delay)) = clip.voice
                && !c.voiced
                && local > delay
            {
                c.voiced = true;
                self.cue(GEM, Action::Stop);
                self.cue(voice, Action::Once(1.0));
            }
        } else if !self.greeted {
            if now - self.ended > WAIT {
                self.greet();
            }
        } else if now - self.ended > PATIENCE {
            match self.chance() <= IDLENESS {
                true => self.wait(),
                false => self.tell(None),
            }
        }
        let since = |t: f64| ((now - t) / FADE).clamp(0.0, 1.0) as f32;
        let music = match &self.clip {
            Some(c) => 1.0 - (1.0 - SOFT) * since(c.started),
            // Nothing was ever softer before the first clip.
            None if self.held.is_none() => 1.0,
            None => SOFT + (1.0 - SOFT) * since(self.ended),
        };
        if music != self.music {
            self.music = music;
            self.cue(demo::MUSIC, Action::Volume(music));
        }
    }

    pub fn frame(&self) -> Frame {
        let orbit = Vec3::new(
            -self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let eye = TARGET + self.distance * orbit;
        let now = |c: &Clip| (c.item, (self.time - c.started).max(0.0) as f32);
        Frame {
            view: glam::camera::rh::view::look_at_mat4(eye, TARGET, Vec3::Y),
            fov: 0.7,
            near: 4.0,
            far: 400.0,
            clip: self.clip.as_ref().map(now).or(self.held),
            light: self.light,
            busy: self.clip.is_some(),
            logo: self.logo,
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Whether the key does anything.
    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " => self.tell(None),
            "0" | "g" | "G" => self.greet(),
            "i" | "I" => self.wait(),
            "x" | "X" => {
                if let Some(c) = &self.clip {
                    if let Some((voice, _)) = demo::CLIPS[c.item].voice {
                        self.cue(voice, Action::Stop);
                    }
                    self.end();
                }
            }
            "l" | "L" => self.light = LIGHT,
            "n" | "N" => self.moving_light = !self.moving_light,
            "v" | "V" => self.logo = !self.logo,
            "p" | "P" => {
                self.paused = !self.paused;
                self.cue("", Action::Pause(self.paused));
            }
            "Home" => self.home(),
            _ => match key.parse::<usize>() {
                Ok(n) if key.len() == 1 && n <= FORTUNES.len() => self.tell(Some(n - 1)),
                _ => return false,
            },
        }
        true
    }

    /// Shows one moment, for checking frames: so far into a clip, counted from one, or
    /// at rest for zero.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, clip: usize) {
        self.time = seconds;
        self.paused = true;
        self.rotate(yaw as f32, pitch as f32);
        if let Some(item) = clip.checked_sub(1).filter(|&i| i < demo::CLIPS.len()) {
            self.say(item);
            if let Some(c) = &mut self.clip {
                c.started = 0.0;
            }
        }
        self.cues.clear();
    }

    /// Positions are parts of the window from its top left.
    pub fn press(&mut self, x: f64, y: f64) {
        let [left, top, w, h] = gem(self.aspect).map(f64::from);
        if x > left && x < left + w && y > top && y < top + h {
            self.tell(None);
            return;
        }
        self.drag = Some((x, y));
        self.spin = (0.0, 0.0);
        self.turned = (0.0, 0.0);
        if self.moving_light {
            self.shine(x, y);
        }
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        let Some((px, py)) = self.drag.replace((x, y)) else {
            self.drag = None;
            return;
        };
        if self.moving_light {
            return self.shine(x, y);
        }
        let (yaw, pitch) = ((x - px) as f32 * PI, (y - py) as f32 * PI);
        self.rotate(yaw, pitch);
        self.turned.0 += yaw;
        self.turned.1 += pitch;
    }

    /// The window is a ball that the light is over (0x41d3c0).
    fn shine(&mut self, x: f64, y: f64) {
        let (dx, dy) = (x as f32 - 0.5, (y as f32 - 0.5) / self.aspect);
        let length = dx.hypot(dy);
        if length > 0.0 {
            let (sin, cos) = (length.min(0.5) * PI).sin_cos();
            self.light = Vec3::new(dx / length * sin, -dy / length * sin, cos);
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (-amount).exp()).clamp(40.0, 200.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs to a time in steps and returns the cues and their times.
    fn run(show: &mut Show, from: f64, to: f64) -> Vec<(f64, Cue)> {
        let mut out = Vec::new();
        let steps = ((to - from) * 100.0).round() as usize;
        for i in 1..=steps {
            let t = from + i as f64 / 100.0;
            show.advance(t);
            out.extend(show.take_cues().into_iter().map(|c| (t, c)));
        }
        out
    }

    fn started(cues: &[(f64, Cue)], name: &str) -> Option<f64> {
        cues.iter()
            .find(|(_, c)| c.name == name && matches!(c.action, Action::Once(_)))
            .map(|c| c.0)
    }

    fn begin() -> Show {
        let mut show = Show::default();
        show.advance(0.0);
        show
    }

    #[test]
    fn greets_after_five_seconds_and_softens_the_music() {
        let mut show = begin();
        assert!(show.frame().clip.is_none());
        let cues = run(&mut show, 0.0, 10.0);
        let t = started(&cues, "greetings").unwrap();
        assert!((t - 5.95).abs() < 0.05, "{t}");
        assert_eq!(show.music, SOFT);
        run(&mut show, 10.0, 18.0);
        assert_eq!(show.music, 1.0);
        assert!(!show.frame().busy);
        // He stays as the clip left him.
        assert_eq!(show.frame().clip, Some((GREETING, 10.0)));
    }

    #[test]
    fn fortunes_come_in_order_and_are_not_interrupted() {
        let mut show = begin();
        for n in 0..FORTUNES.len() + 1 {
            show.key(" ");
            let item = FORTUNES.start + n % FORTUNES.len();
            assert_eq!(show.frame().clip.unwrap().0, item);
            let cues = show.take_cues();
            assert!(started(&cues.iter().map(|c| (0.0, *c)).collect::<Vec<_>>(), GEM).is_some());
            show.key(" ");
            show.key("g");
            show.key("i");
            assert!(show.take_cues().is_empty());
            assert_eq!(show.frame().clip.unwrap().0, item);
            show.key("x");
            assert!(!show.frame().busy);
        }
        show.key("5");
        assert_eq!(show.frame().clip.unwrap().0, FORTUNES.start + 4);
        show.key("x");
        show.key(" ");
        assert_eq!(show.frame().clip.unwrap().0, FORTUNES.start + 5);
        assert!(!show.key("8") && !show.key("9") && !show.key("+1"));
    }

    #[test]
    fn grows_bored() {
        let mut show = begin();
        show.key("g");
        run(&mut show, 0.0, 24.9);
        assert!(!show.frame().busy);
        run(&mut show, 24.9, 25.2);
        assert_eq!(show.frame().clip.unwrap().0, IDLE.start);
    }

    #[test]
    fn the_gem_is_a_button() {
        let mut show = begin();
        show.press(0.5, 0.1);
        show.release();
        assert!(!show.frame().busy);
        show.press(0.9, 0.1);
        assert!(show.frame().busy && show.drag.is_none());
        assert_eq!(
            gem(4.0 / 3.0).map(|v| (v * 100.0).round()),
            [80.0, 0.0, 20.0, 20.0]
        );
        assert_eq!(
            logo(4.0 / 3.0).map(|v| (v * 100.0).round()),
            [80.0, 93.0, 20.0, 7.0]
        );
    }

    #[test]
    fn drags_spin_until_pressed() {
        let mut show = begin();
        show.press(0.5, 0.5);
        show.motion(0.51, 0.5);
        show.advance(0.02);
        show.release();
        let yaw = show.yaw;
        show.advance(0.04);
        assert!(show.yaw > yaw);
        show.press(0.5, 0.5);
        assert_eq!(show.spin, (0.0, 0.0));
        show.motion(0.5, -5.0);
        assert_eq!(show.pitch, 0.0, "never from below");
        show.motion(0.51, -5.0);
        show.advance(0.06);
        for i in 0..10 {
            show.advance(0.08 + f64::from(i) * 0.02);
        }
        assert_eq!(show.spin, (0.0, 0.0), "held still");
    }

    #[test]
    fn the_light_follows_the_pointer() {
        let mut show = begin();
        show.key("n");
        show.press(1.0, 0.5);
        assert!((show.light - Vec3::X).length() < 1e-5);
        show.motion(0.5, 0.75);
        assert!(show.light.y < -0.5 && show.light.z > 0.0);
        assert_eq!(show.yaw, 0.0);
        show.key("l");
        assert_eq!(show.light, LIGHT);
    }
}
