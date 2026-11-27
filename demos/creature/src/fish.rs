//! The school of fish (0x40ca00, 0x40ef00).
use glam::{Mat4, Quat, Vec3, Vec4};

/// The C library's `rand`, so that everything starts where it did.
pub struct Rand(u32);

impl Default for Rand {
    fn default() -> Self {
        Self(1)
    }
}

impl Rand {
    /// 0 to 1.
    pub fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.0 >> 16 & 0x7fff) as f32 / 32767.0
    }
    /// The original fills vectors from the last component to the first.
    pub fn vec(&mut self) -> Vec3 {
        let (z, y) = (self.unit(), self.unit());
        Vec3::new(self.unit(), y, z)
    }
}

const SEPARATION: f32 = 1.0;
const ALIGNMENT: f32 = 0.8;
const COHESION: f32 = 1.0;
const HOME: f32 = 0.001;
const PREDATOR: f32 = 10.0;
const ACCELERATION: f32 = 0.7;
const SLOWEST: f32 = 0.5;
/// Radians a second.
const TURN: f32 = 0.6;
const SIGHT: f32 = 8.0;

#[derive(Clone, Copy)]
struct Fish {
    at: Vec3,
    to: Vec3,
    speed: f32,
    fastest: f32,
}

#[derive(Default)]
pub struct School {
    fish: Vec<Fish>,
    pub rand: Rand,
}

impl School {
    pub fn add(&mut self) {
        let r = &mut self.rand;
        let at = r.vec() * 100.0 - 50.0;
        let to = r.vec() * Vec3::new(2.0, 1.0, 2.0) - Vec3::new(1.0, 0.5, 1.0);
        self.fish.push(Fish {
            at,
            to: to.normalize_or(Vec3::Z),
            speed: r.unit(),
            fastest: (r.unit() * 0.3 + 0.85) * 10.0,
        });
    }
    pub fn remove(&mut self) {
        self.fish.pop();
    }
    pub fn len(&self) -> usize {
        self.fish.len()
    }
    pub fn is_empty(&self) -> bool {
        self.fish.is_empty()
    }

    pub fn step(&mut self, dt: f32) {
        // Each sees the others as they are now, some already moved.
        for i in 0..self.fish.len() {
            self.fish[i] = self.steer(i, dt);
        }
    }

    fn steer(&self, i: usize, dt: f32) -> Fish {
        let mut f = self.fish[i];
        let (mut n, mut speeds) = (0.0, 0.0);
        let (mut apart, mut along, mut center) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
        for (j, o) in self.fish.iter().enumerate() {
            let away = f.at - o.at;
            let d2 = away.length_squared();
            if i == j || d2 > SIGHT * SIGHT || d2 == 0.0 {
                continue;
            }
            n += 1.0;
            speeds += o.speed;
            center += o.at;
            along += o.to / d2;
            apart += away / d2;
        }
        let mut force = Vec3::ZERO;
        if n > 0.0 {
            // The center counts as a position, not as a direction from the fish.
            force =
                apart * SEPARATION + along.normalize_or_zero() * ALIGNMENT + center / n * COHESION;
            speeds /= n;
        }
        // Home and the predator (the creature) are both in the middle.
        let d2 = f.at.length_squared().max(1e-6);
        force -= f.at * d2 * HOME;
        force += f.at * (PREDATOR / d2);
        let want = force.normalize_or(f.to);

        let turn = Quat::from_rotation_arc(f.to, want);
        let (axis, angle) = turn.to_axis_angle();
        let most = dt * TURN;
        f.to = match most < angle {
            true => (Quat::from_axis_angle(axis, most) * f.to).normalize(),
            false => want,
        };
        let ahead = f.to.dot(want);
        let own = if ahead <= 0.0 {
            -ACCELERATION
        } else {
            ACCELERATION
        };
        let change = if n > 0.0 {
            let share = ALIGNMENT / (ahead + ALIGNMENT);
            let others = if speeds < f.speed {
                -ACCELERATION
            } else {
                ACCELERATION
            };
            others * share + (1.0 - share) * own * ahead
        } else {
            own * ahead
        };
        f.speed = (f.speed + change * dt).max(SLOWEST).min(f.fastest);
        f.at += f.to * f.speed * dt;
        f
    }

    /// Their transforms: the model looks along +z.
    pub fn places(&self) -> impl Iterator<Item = Mat4> + '_ {
        self.fish.iter().map(|f| {
            let right = (-f.to).cross(Vec3::Y).normalize_or(Vec3::X);
            let up = right.cross(-f.to);
            let w = |v: Vec3| -> Vec4 { v.extend(0.0) };
            Mat4::from_cols(w(right), w(up), w(f.to), f.at.extend(1.0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand_is_microsofts() {
        let mut r = Rand::default();
        let first: Vec<u32> = (0..4)
            .map(|_| (r.unit() * 32767.0).round() as u32)
            .collect();
        assert_eq!(first, [41, 18467, 6334, 26500]);
    }

    #[test]
    fn the_school_stays_near_and_moves() {
        let mut s = School::default();
        (0..200).for_each(|_| s.add());
        let before: Vec<Vec3> = s.fish.iter().map(|f| f.at).collect();
        (0..60 * 60).for_each(|_| s.step(1.0 / 60.0));
        for (f, b) in s.fish.iter().zip(before) {
            assert!(f.at.length() < 120.0, "{:?}", f.at);
            assert!(f.at != b && (f.to.length() - 1.0).abs() < 1e-3);
            assert!((SLOWEST..=11.5).contains(&f.speed));
        }
    }
}
