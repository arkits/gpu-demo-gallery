//! Waves on the pool. The program solved a fluid on two grids; this is the wave equation
//! on one [I].
use glam::{Vec2, Vec3};

/// Points across.
pub const SIZE: usize = 128;
/// Width of the grid, a little more than the pool.
pub const WIDTH: f32 = 85.0;
const CELL: f32 = WIDTH / (SIZE - 1) as f32;
/// Units per second that waves travel.
const SPEED: f32 = 9.0;
/// What is left of a wave after a step.
const DAMPING: f32 = 0.9965;
/// Seconds in a step.
pub const STEP: f32 = 1.0 / 120.0;

pub struct Water {
    now: Vec<f32>,
    before: Vec<f32>,
    /// Where there is water.
    wet: Vec<bool>,
}

fn index(x: usize, z: usize) -> usize {
    z * SIZE + x
}

pub fn place(i: usize) -> f32 {
    i as f32 * CELL - WIDTH * 0.5
}

impl Water {
    pub fn new(wet: Vec<bool>) -> Self {
        Self {
            now: vec![0.0; SIZE * SIZE],
            before: vec![0.0; SIZE * SIZE],
            wet,
        }
    }

    /// Pushes the water down around a point, and up in a ring around that.
    pub fn disturb(&mut self, at: Vec2, radius: f32, strength: f32) {
        for z in 1..SIZE - 1 {
            for x in 1..SIZE - 1 {
                let d = (Vec2::new(place(x), place(z)) - at).length() / radius;
                if d < 3.0 && self.wet[index(x, z)] {
                    self.now[index(x, z)] += strength * (d * d - 1.0) * (-d * d).exp();
                }
            }
        }
    }

    pub fn step(&mut self) {
        let c = (SPEED * STEP / CELL).powi(2);
        let mut next = vec![0.0; SIZE * SIZE];
        for z in 1..SIZE - 1 {
            for x in 1..SIZE - 1 {
                let i = index(x, z);
                if !self.wet[i] {
                    continue;
                }
                let h = self.now[i];
                // Dry neighbors reflect.
                let at = |j: usize| if self.wet[j] { self.now[j] } else { h };
                let sum = at(i - 1) + at(i + 1) + at(i - SIZE) + at(i + SIZE);
                let v = (h - self.before[i]) * DAMPING;
                next[i] = (h + v + c * (sum - 4.0 * h)).clamp(-0.6, 0.6);
            }
        }
        self.before = std::mem::replace(&mut self.now, next);
    }

    pub fn height(&self, x: usize, z: usize) -> f32 {
        self.now[index(x, z)]
    }

    pub fn normal(&self, x: usize, z: usize) -> Vec3 {
        let at = |x: usize, z: usize| self.now[index(x.min(SIZE - 1), z.min(SIZE - 1))];
        let (w, e) = (at(x.saturating_sub(1), z), at(x + 1, z));
        let (n, s) = (at(x, z.saturating_sub(1)), at(x, z + 1));
        Vec3::new(w - e, 2.0 * CELL, n - s).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_spread_and_die() {
        let mut w = Water::new(vec![true; SIZE * SIZE]);
        w.disturb(Vec2::ZERO, 2.0, 0.3);
        let energy = |w: &Water| w.now.iter().map(|h| h * h).sum::<f32>();
        let first = energy(&w);
        for _ in 0..240 {
            w.step();
        }
        // Two seconds later it has gone some 18 units.
        let at = (18.0 / CELL) as usize;
        let ring = (at - 4..at + 4).map(|d| w.height(SIZE / 2 + d, SIZE / 2).abs());
        assert!(ring.fold(0.0, f32::max) > 0.002);
        for _ in 0..120 * 60 {
            w.step();
        }
        assert!(energy(&w) < first * 0.02);
        assert!(w.now.iter().all(|h| h.is_finite()));
    }

    #[test]
    fn dry_places_stay_flat() {
        let wet = (0..SIZE * SIZE).map(|i| i % SIZE < SIZE / 2).collect();
        let mut w = Water::new(wet);
        w.disturb(Vec2::new(-5.0, 0.0), 3.0, 0.4);
        for _ in 0..600 {
            w.step();
        }
        assert!((SIZE / 2..SIZE).all(|x| w.height(x, 60) == 0.0));
    }
}
