//! Show logic without any windowing: clock, fades, cameras and controls.
use sushi_engine::camera::{Camera, Orbit};
use sushi_engine::demo::START_TIME;

/// `fadeInTime` and `fadeOutTime` in `Sushi2.ini`.
const FADE_IN: f64 = 2.0;
const FADE_OUT: f64 = 2.0;
/// Beyond this the clock jumps to the audio instead of easing toward it.
const SNAP: f64 = 0.15;

pub struct Show {
    /// Scene seconds; the music starts at `START_TIME`.
    time: f64,
    end: f64,
    last_wall: Option<f64>,
    paused: bool,
    seek: Option<f64>,
    orbit: Option<Orbit>,
    pub logos: bool,
}

impl Show {
    pub fn new(end: f32) -> Self {
        Self {
            time: f64::from(START_TIME),
            end: f64::from(end),
            last_wall: None,
            paused: false,
            seek: Some(0.0),
            orbit: None,
            logos: true,
        }
    }

    /// Advances to wall-clock `now`, following the music when it is playing.
    pub fn advance(&mut self, now: f64, audio: Option<f64>) {
        let dt = self.last_wall.map_or(0.0, |w| (now - w).clamp(0.0, 1.0));
        self.last_wall = Some(now);
        if self.paused {
            return;
        }
        self.time += dt;
        if let Some(a) = audio {
            let error = a + f64::from(START_TIME) - self.time;
            // Audio positions arrive in steps, so ease toward them.
            self.time += if error.abs() > SNAP {
                error
            } else {
                error * 0.05
            };
        }
        if self.time >= self.end {
            self.jump(0.0);
        }
    }

    fn jump(&mut self, music: f64) {
        let length = self.end - f64::from(START_TIME);
        let m = music.clamp(0.0, (length - 0.1).max(0.0));
        self.time = f64::from(START_TIME) + m;
        self.seek = Some(m);
    }

    pub fn time(&self) -> f32 {
        self.time as f32
    }

    pub fn set_time(&mut self, time: f32) {
        self.jump(f64::from(time - START_TIME));
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Music position the host must move its audio to, once.
    pub fn take_seek(&mut self) -> Option<f64> {
        self.seek.take()
    }

    /// 0 is black, 1 fully visible.
    pub fn brightness(&self) -> f32 {
        let start = (self.time - f64::from(START_TIME)) / FADE_IN;
        let end = (self.end - self.time) / FADE_OUT;
        start.min(end).clamp(0.0, 1.0) as f32
    }

    pub fn camera(&self) -> Option<Camera> {
        self.orbit.as_ref().map(Orbit::camera)
    }

    pub fn interactive(&self) -> bool {
        self.orbit.is_some()
    }

    /// Keys shared by the native and web players. Returns whether it was used.
    pub fn key(&mut self, key: &str) -> bool {
        match key {
            " " => self.paused = !self.paused,
            "ArrowLeft" => self.jump(self.time - f64::from(START_TIME) - 5.0),
            "ArrowRight" => self.jump(self.time - f64::from(START_TIME) + 5.0),
            "Home" => self.jump(0.0),
            "f" | "F" => {
                self.orbit = if self.orbit.is_some() {
                    None
                } else {
                    Some(Orbit::preset(0))
                }
            }
            "1" | "2" | "3" | "4" => {
                self.orbit = Some(Orbit::preset(usize::from(key.as_bytes()[0] - b'1')));
            }
            "l" | "L" => self.logos = !self.logos,
            _ => return false,
        }
        true
    }

    pub fn drag(&mut self, dx: f32, dy: f32) {
        if let Some(o) = &mut self.orbit {
            o.rotate(dx, dy);
        }
    }

    pub fn zoom(&mut self, amount: f32) {
        if let Some(o) = &mut self.orbit {
            o.zoom(amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_starts_at_scene_time_ten() {
        let mut s = Show::new(214.93);
        assert_eq!(s.take_seek(), Some(0.0));
        assert_eq!(s.take_seek(), None);
        s.advance(100.0, Some(0.0));
        assert_eq!(s.time(), 10.0);
        s.advance(100.5, Some(0.5));
        assert!((s.time() - 10.5).abs() < 1e-3);
    }

    #[test]
    fn clock_eases_small_errors_and_snaps_large_ones() {
        let mut s = Show::new(214.93);
        s.advance(0.0, None);
        s.advance(0.016, Some(0.05));
        assert!(s.time() > 10.016 && s.time() < 10.03);
        s.advance(0.032, Some(3.0));
        assert!((s.time() - 13.0).abs() < 1e-3);
    }

    #[test]
    fn pause_holds_time_and_does_not_jump_on_resume() {
        let mut s = Show::new(214.93);
        s.advance(0.0, None);
        s.key(" ");
        s.advance(60.0, None);
        assert_eq!(s.time(), 10.0);
        s.key(" ");
        s.advance(60.016, None);
        assert!(s.time() < 10.1);
    }

    #[test]
    fn fades_and_restart() {
        let mut s = Show::new(20.0);
        assert_eq!(s.brightness(), 0.0);
        s.set_time(15.0);
        assert_eq!(s.brightness(), 1.0);
        s.set_time(19.0);
        assert!((s.brightness() - 0.5).abs() < 1e-3);
        s.take_seek();
        s.advance(0.0, None);
        s.advance(1.0, None);
        assert_eq!(s.take_seek(), Some(0.0));
        assert_eq!(s.time(), 10.0);
    }

    #[test]
    fn seeking_is_clamped() {
        let mut s = Show::new(20.0);
        s.key("ArrowLeft");
        assert_eq!(s.take_seek(), Some(0.0));
        for _ in 0..9 {
            s.key("ArrowRight");
        }
        assert!(s.time() < 20.0);
    }
}
