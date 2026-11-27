//! What the viewer's main loop does: stages, transitions, rotation and input
//! (`LogoViewer.cpp`, `NvNavigator.cpp`).

const MODELS: usize = 3;
const STAGES: usize = 6;
/// Seconds for each half of a transition (0x40413a).
const FADE: f64 = 2.0;
/// Seconds before the show starts by itself, and between its steps.
const START: f64 = 2.0;
const IDLE: f64 = 10.0;
/// Radians a second: a full turn every 20 seconds.
const SPIN: f64 = std::f64::consts::PI / 10.0;
const NEAR: f32 = 200.0;
const FAR: f32 = 500.0;
/// 1.5 times the radius of the models' bounding sphere (249.6), plus 50 (0x44d893,
/// 0x44d965).
const AWAY: f32 = 424.4;

/// Sounds the host must start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// The music of a model, replacing what plays, without end.
    Music(usize),
    /// The sound of a transition, once.
    Transition,
}

pub struct Show {
    model: usize,
    stage: usize,
    yaw: f64,
    pitch: f64,
    /// Radians a second.
    spin: (f64, f64),
    distance: f32,
    time: f64,
    last_wall: Option<f64>,
    last_step: f64,
    last_input: f64,
    /// When the current half of a transition began, and whether it darkens.
    fade: Option<(f64, bool)>,
    /// Model and stage to show once it is dark.
    target: (usize, usize),
    started: bool,
    captions: bool,
    sounded: bool,
    drag: Option<(f64, f64, f64)>,
    cues: Vec<Cue>,
    pub logo: bool,
    pub paused: bool,
}

impl Default for Show {
    fn default() -> Self {
        Self {
            model: 0,
            stage: 4,
            yaw: 0.0,
            pitch: 0.0,
            spin: (SPIN, 0.0),
            distance: AWAY,
            time: 0.0,
            last_wall: None,
            last_step: 0.0,
            last_input: f64::MIN,
            fade: None,
            target: (0, 0),
            started: false,
            captions: false,
            sounded: false,
            drag: None,
            cues: vec![Cue::Transition],
            logo: true,
            paused: false,
        }
    }
}

impl Show {
    pub fn advance(&mut self, now: f64) {
        let dt = self.last_wall.map_or(0.0, |w| (now - w).clamp(0.0, 0.25));
        self.last_wall = Some(now);
        self.time += dt;
        if self.drag.is_none() && !self.paused {
            self.turn(self.spin.0 * dt, self.spin.1 * dt);
        }
        if !self.started && self.time > START {
            self.go(0, 0);
        }
        let quiet = self.time - self.last_step > IDLE && self.time - self.last_input > IDLE;
        if quiet && self.started && self.fade.is_none() && !self.paused {
            self.next();
        }
        if let Some((since, darkening)) = self.fade
            && self.time - since >= FADE
        {
            if darkening {
                (self.model, self.stage) = self.target;
                self.captions = true;
                // The first change has no sound of its own.
                if std::mem::replace(&mut self.sounded, true) {
                    self.cues.push(Cue::Transition);
                }
                self.cues.push(Cue::Music(self.model));
                self.fade = Some((self.time, false));
            } else {
                self.fade = None;
            }
        }
    }

    fn turn(&mut self, yaw: f64, pitch: f64) {
        let full = std::f64::consts::TAU;
        self.yaw = (self.yaw + yaw).rem_euclid(full);
        // Never below the floor, nor over the top.
        self.pitch = (self.pitch + pitch).clamp(-full / 4.0, 0.0);
    }

    /// Goes to a model through black. Stages of the same model change at once.
    fn go(&mut self, model: usize, stage: usize) {
        self.started = true;
        self.last_step = self.time;
        self.target = (model, stage);
        match self.fade {
            Some((_, true)) => {}
            // Darkens again from where it is.
            Some((since, false)) => {
                self.fade = Some((self.time - (FADE - (self.time - since)).max(0.0), true))
            }
            None => self.fade = Some((self.time, true)),
        }
    }

    fn stage_to(&mut self, stage: usize) {
        self.stage = stage;
        self.started = true;
        self.captions = true;
        self.last_step = self.time;
        if !self.sounded {
            self.sounded = true;
            self.cues.push(Cue::Music(self.model));
        }
    }

    /// One step of the show (0x404a56): the second and third model start with
    /// their textures.
    fn next(&mut self) {
        if self.stage + 1 < STAGES {
            self.stage_to(self.stage + 1);
        } else if self.model + 1 < MODELS {
            self.go(self.model + 1, 3);
        } else {
            self.go(0, 0);
        }
    }

    fn previous(&mut self) {
        if self.stage > 0 {
            self.stage_to(self.stage - 1);
        } else if self.model > 0 {
            self.go(self.model - 1, STAGES - 1);
        }
    }

    pub fn model(&self) -> usize {
        self.model
    }
    pub fn stage(&self) -> usize {
        self.stage
    }

    pub fn frame(&self) -> crate::scene::Frame {
        let brightness = match self.fade {
            None => 1.0,
            Some((since, darkening)) => {
                let f = ((self.time - since) / FADE).clamp(0.0, 1.0);
                if darkening { 1.0 - f } else { f }
            }
        };
        crate::scene::Frame {
            model: self.model,
            stage: self.stage,
            yaw: self.yaw as f32,
            pitch: self.pitch as f32,
            distance: self.distance,
            caption: self.captions,
            logo: self.logo,
            brightness: brightness as f32,
        }
    }

    /// Shows one moment for tests: no transition, rotation stopped.
    pub fn freeze(&mut self, model: usize, stage: usize, yaw: f64, pitch: f64) {
        (self.model, self.stage) = (model.min(MODELS - 1), stage.min(STAGES - 1));
        (self.yaw, self.pitch, self.fade) = (yaw, pitch, None);
        self.started = true;
        self.captions = true;
        self.paused = true;
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Returns false for keys that do nothing.
    pub fn key(&mut self, key: &str) -> bool {
        let still = self.fade.is_none();
        match key {
            "ArrowRight" if still => self.next(),
            "ArrowLeft" if still => self.previous(),
            "ArrowDown" if still && self.model + 1 < MODELS => self.go(self.model + 1, self.stage),
            "ArrowUp" if still && self.model > 0 => self.go(self.model - 1, self.stage),
            "Home" if still => self.stage_to(0),
            "End" if still => self.stage_to(STAGES - 1),
            "1" | "2" | "3" if still => self.go(usize::from(key.as_bytes()[0] - b'1'), 0),
            " " => self.paused = !self.paused,
            "l" | "L" => self.logo = !self.logo,
            _ => return false,
        }
        self.last_input = self.time;
        true
    }

    /// Goes to a stage of the current model.
    pub fn select(&mut self, stage: usize) {
        if self.fade.is_none() {
            self.stage_to(stage.min(STAGES - 1));
            self.last_input = self.time;
        }
    }

    /// Positions are fractions of the window, from its top left.
    pub fn press(&mut self, x: f64, y: f64) {
        self.drag = Some((x, y, self.time));
        self.spin = (0.0, 0.0);
        self.last_input = self.time;
    }

    pub fn motion(&mut self, x: f64, y: f64) {
        let Some((x0, y0, then)) = self.drag else {
            return;
        };
        // The original measures in half windows and turns half a circle for each.
        let (yaw, pitch) = (
            (x0 - x) * -2.0 * std::f64::consts::PI,
            (y0 - y) * 2.0 * std::f64::consts::PI,
        );
        self.turn(yaw, pitch);
        let dt = self.time - then;
        if dt > 0.0 {
            self.spin = (yaw / dt, pitch / dt);
        }
        self.drag = Some((x, y, self.time));
        self.last_input = self.time;
    }

    /// It keeps turning as it was when let go.
    pub fn release(&mut self) {
        if let Some((.., then)) = self.drag.take()
            && self.time - then > 0.1
        {
            self.spin = (0.0, 0.0);
        }
        self.last_input = self.time;
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance = (self.distance * (1.0 - amount)).clamp(NEAR, FAR);
        self.last_input = self.time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Show, seconds: f64) {
        let mut now = s.last_wall.unwrap_or(0.0);
        let end = now + seconds;
        while now < end {
            now += 0.1;
            s.advance(now);
        }
    }

    #[test]
    fn the_show_runs_by_itself() {
        let mut s = Show::default();
        assert_eq!((s.model(), s.stage()), (0, 4));
        assert_eq!(s.take_cues(), [Cue::Transition]);
        run(&mut s, 5.0);
        assert_eq!((s.model(), s.stage()), (0, 0));
        assert_eq!(s.take_cues(), [Cue::Music(0)]);
        let mut seen = vec![(0, 0)];
        for _ in 0..1600 {
            run(&mut s, 0.1);
            if *seen.last().unwrap() != (s.model(), s.stage()) {
                seen.push((s.model(), s.stage()));
            }
        }
        let mut want: Vec<_> = (0..6).map(|i| (0, i)).collect();
        want.extend([(1, 3), (1, 4), (1, 5), (2, 3), (2, 4), (2, 5), (0, 0)]);
        assert_eq!(seen[..13], want[..]);
        assert!(s.take_cues().contains(&Cue::Music(2)));
    }

    #[test]
    fn dragging_turns_and_keeps_turning() {
        let mut s = Show::default();
        run(&mut s, 1.0);
        let yaw = s.frame().yaw;
        s.press(0.5, 0.5);
        run(&mut s, 0.1);
        s.motion(0.6, 0.4);
        s.release();
        let after = s.frame().yaw;
        assert!((after - yaw).abs() > 0.3);
        run(&mut s, 0.2);
        assert!(s.frame().yaw != after && s.frame().pitch <= 0.0);
        s.press(0.5, 0.5);
        let held = s.frame().yaw;
        run(&mut s, 1.0);
        assert_eq!(s.frame().yaw, held);
    }
}
