//! Show logic without any windowing: chapters, clock, fades and controls.
use sushi_engine::camera::{Control, Orbit};

pub struct Show {
    durations: Vec<f64>,
    /// `fadeInTime` and `fadeOutTime` in `Sushi2.ini`.
    fades: (f64, f64),
    chapter: usize,
    time: f64,
    last_wall: Option<f64>,
    paused: bool,
    restart: bool,
    orbit: Option<Orbit>,
    /// The `control` cameras of `Sushi2.ini`.
    pub controls: Vec<Control>,
    pub logos: bool,
}

impl Show {
    pub fn new(durations: Vec<f64>, fades: (f64, f64)) -> Self {
        Self {
            durations,
            fades,
            chapter: 0,
            time: 0.0,
            last_wall: None,
            paused: false,
            restart: true,
            orbit: None,
            controls: Vec::new(),
            logos: true,
        }
    }

    fn duration(&self) -> f64 {
        self.durations[self.chapter]
    }

    /// Advances to wall-clock `now`; chapters follow one another and repeat
    /// (`cycleAllModes` in `Sushi.ini`). A free camera stays in its chapter.
    pub fn advance(&mut self, now: f64) {
        let dt = self.last_wall.map_or(0.0, |w| (now - w).clamp(0.0, 1.0));
        self.last_wall = Some(now);
        if self.paused {
            return;
        }
        self.time += dt;
        if self.time >= self.duration() {
            if self.orbit.is_some() {
                self.time %= self.duration();
            } else {
                self.go((self.chapter + 1) % self.durations.len());
            }
        }
    }

    /// Starts a chapter. The music starts over with it.
    pub fn go(&mut self, chapter: usize) {
        self.chapter = chapter.min(self.durations.len() - 1);
        self.time = 0.0;
        self.restart = true;
        // A free camera takes the new chapter's first control.
        if self.orbit.is_some() {
            self.orbit = Some(self.free(None));
        }
    }

    /// The control after `after` that the chapter offers (0x416ea0), from the first, or the
    /// show's own camera turned around when it has none.
    fn free(&self, after: Option<&Control>) -> Orbit {
        let offered: Vec<&Control> = self
            .controls
            .iter()
            .filter(|c| c.in_chapter(self.chapter))
            .collect();
        let at = after.and_then(|a| offered.iter().position(|c| *c == a));
        let next = at.map_or(0, |i| (i + 1) % offered.len().max(1));
        offered
            .get(next)
            .map_or_else(Orbit::default, |c| Orbit::with((*c).clone()))
    }

    /// Moves within the chapter.
    pub fn set_time(&mut self, time: f64) {
        self.time = time.clamp(0.0, (self.duration() - 0.1).max(0.0));
        self.restart = true;
    }

    pub fn chapter(&self) -> usize {
        self.chapter
    }
    pub fn time(&self) -> f32 {
        self.time as f32
    }
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Whether the music must start over, once.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    /// 0 is black, 1 fully visible.
    pub fn brightness(&self) -> f32 {
        if self.orbit.is_some() {
            return 1.0;
        }
        let left = self.duration() - self.time;
        let shown = (self.time / self.fades.0).min(left / self.fades.1);
        shown.clamp(0.0, 1.0) as f32
    }

    pub fn orbit(&self) -> Option<&Orbit> {
        self.orbit.as_ref()
    }

    /// Keys shared by the native and web players. Returns whether it was used.
    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " => self.paused = !self.paused,
            "ArrowLeft" => self.set_time(self.time - 5.0),
            "ArrowRight" => self.set_time(self.time + 5.0),
            "Home" => self.go(0),
            "f" | "F" => {
                self.orbit = match self.orbit {
                    Some(_) => None,
                    None => Some(self.free(None)),
                }
            }
            "c" | "C" => {
                let next = self.free(self.orbit.as_ref().and_then(Orbit::control));
                self.orbit = Some(next);
            }
            "l" | "L" => self.logos = !self.logos,
            _ => match key.parse::<usize>() {
                Ok(n) if (1..=self.durations.len()).contains(&n) => self.go(n - 1),
                _ => return false,
            },
        }
        true
    }

    /// Dragging takes the camera.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        if self.orbit.is_none() {
            self.orbit = Some(self.free(None));
        }
        if let Some(o) = &mut self.orbit {
            o.rotate(dx, dy);
        }
    }

    pub fn zoom(&mut self, amount: f32) {
        if self.orbit.is_none() {
            self.orbit = Some(self.free(None));
        }
        if let Some(o) = &mut self.orbit {
            o.zoom(amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show() -> Show {
        Show::new(vec![10.0, 5.0], (1.0, 1.0))
    }

    #[test]
    fn chapters_follow_and_restart_the_music() {
        let mut s = show();
        assert!(s.take_restart() && !s.take_restart());
        s.set_time(9.5);
        s.advance(0.0);
        s.advance(0.6);
        assert_eq!((s.chapter(), s.time()), (1, 0.0));
        assert!(s.take_restart());
        s.set_time(4.9);
        s.advance(0.8);
        assert_eq!(s.chapter(), 0);
        s.key("2");
        assert_eq!(s.chapter(), 1);
        assert!(!s.key("5") && !s.key("0"));
        assert_eq!(s.chapter(), 1);
    }

    #[test]
    fn fades() {
        let mut s = show();
        assert_eq!(s.brightness(), 0.0);
        s.set_time(5.0);
        assert_eq!(s.brightness(), 1.0);
        s.set_time(9.5);
        assert!((s.brightness() - 0.5).abs() < 1e-3);
        s.key("f");
        assert_eq!(s.brightness(), 1.0);
    }

    #[test]
    fn pause_and_free_camera() {
        let mut s = show();
        s.advance(0.0);
        s.key(" ");
        s.advance(60.0);
        assert_eq!(s.time(), 0.0);
        s.key(" ");
        s.advance(60.1);
        assert!(s.time() < 0.2 && s.orbit().is_none());
        s.drag(10.0, 0.0);
        assert!(s.orbit().is_some());
        s.set_time(9.5);
        s.advance(60.9);
        assert_eq!(s.chapter(), 0);
    }
}
