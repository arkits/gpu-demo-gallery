//! Camera paths (`.pth`): a count, then each point's place and rotation (x, y, z, w), and `c`
//! after a point the path cuts from (0x41ee90). A point a second; a cut takes no time. Places
//! and rotations are each a natural cubic spline (Numerical Recipes' `spline` with unit steps)
//! over the points with eight more at each end and on both sides of every cut (0x41f0c0 and
//! 0x41f710): copies of the other end where the path loops, else straight lines (slerps for
//! rotations) out of the last two points. Rotations are splined after mapping each unit
//! quaternion q to (x, y, z, 1 - w) / sqrt(2 (1 - w)), and mapped back after.
use glam::{Quat, Vec3, Vec4};

#[derive(Debug, Clone, Default)]
pub struct Path {
    pub points: Vec<(Vec3, Quat)>,
    pub cuts: Vec<bool>,
    /// Places and their second derivatives, and each point's time (n + 1 of each).
    pos: Vec<Vec3>,
    pos2: Vec<Vec3>,
    rot: Vec<Vec4>,
    rot2: Vec<Vec4>,
    time: Vec<f32>,
}

/// NR `spline` with x spacing 1 and natural ends (y2 zero at both).
fn spline<const N: usize>(y: &[[f32; N]]) -> Vec<[f32; N]> {
    let n = y.len();
    let mut y2 = vec![[0.0; N]; n];
    let mut u = vec![[0.0; N]; n];
    for i in 1..n.saturating_sub(1) {
        for k in 0..N {
            let p = 1.0 / (y2[i - 1][k] * 0.5 + 2.0);
            y2[i][k] = -0.5 * p;
            let d = (y[i + 1][k] - y[i][k]) - (y[i][k] - y[i - 1][k]);
            u[i][k] = (d * 6.0 * 0.5 - u[i - 1][k] * 0.5) * p;
        }
    }
    if n >= 2 {
        y2[n - 1] = [0.0; N];
        for i in (0..n - 1).rev() {
            for k in 0..N {
                y2[i][k] = y2[i][k] * y2[i + 1][k] + u[i][k];
            }
        }
    }
    y2
}

/// D3DXQuaternionSlerp as 0x423750 does it, here also to extrapolate (t above 1).
fn slerp(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    let mut c = a.dot(b);
    let b = if c < 0.0 {
        c = -c;
        -b
    } else {
        b
    };
    let (s0, s1) = if 1.0 - c <= 1e-6 {
        (1.0 - t, t)
    } else {
        let th = c.clamp(-1.0, 1.0).acos();
        let s = th.sin();
        (((1.0 - t) * th).sin() / s, (t * th).sin() / s)
    };
    (b * s1 + a * s0).normalize()
}

/// 0x4236d0: slerp after turning `b` to `a`'s side, normalised.
fn slerp_near(a: Vec4, b: Vec4, t: f32) -> Vec4 {
    let a = if a.dot(b) < 0.0 { -a } else { a };
    slerp(a, b, t).normalize()
}

impl Path {
    pub fn parse(text: &str) -> Option<Self> {
        let mut words = text.split_whitespace();
        let n: usize = words.next()?.parse().ok()?;
        let mut points = Vec::with_capacity(n);
        let mut cuts = Vec::with_capacity(n);
        let mut it = words.peekable();
        for _ in 0..n {
            let mut f = || it.next().and_then(|w| w.parse::<f32>().ok());
            let p = Vec3::new(f()?, f()?, f()?);
            let q = Quat::from_xyzw(f()?, f()?, f()?, f()?);
            let cut = it.peek() == Some(&"c");
            if cut {
                it.next();
            }
            points.push((p, q));
            cuts.push(cut);
        }
        if let Some(c) = cuts.first_mut() {
            *c = false;
        }
        let mut path = Self {
            points,
            cuts,
            ..Default::default()
        };
        path.prepare();
        Some(path)
    }

    /// Timed seconds: points less cuts.
    pub fn length(&self) -> f32 {
        (self.points.len() - self.cuts.iter().filter(|&&c| c).count()) as f32
    }

    pub fn loops(&self) -> bool {
        !self.cuts.last().copied().unwrap_or(false)
    }

    fn prepare(&mut self) {
        let n = self.points.len();
        if n <= 10 {
            return;
        }
        let p = |i: usize| self.points[i].0;
        let q = |i: usize| Vec4::from(self.points[i].1.to_array());
        let lerp = |a: Vec3, b: Vec3, t: f32| a + (b - a) * t;
        let last_cut = self.cuts[n - 1];
        let mut ps: Vec<Vec3> = Vec::new();
        let mut qs: Vec<Vec4> = Vec::new();
        if last_cut {
            for k in 0..8 {
                let t = 9.0 - k as f32;
                ps.push(lerp(p(1), p(0), t));
                qs.push(slerp_near(q(1), q(0), t));
            }
        } else {
            for k in 0..8 {
                ps.push(p(n - 8 + k));
                qs.push(q(n - 8 + k));
            }
        }
        // Where each point lands in the padded lists.
        let mut at = Vec::with_capacity(n + 1);
        for i in 0..n {
            at.push(ps.len());
            ps.push(p(i));
            qs.push(q(i));
            if self.cuts[i] {
                for k in 0..8 {
                    let t = k as f32 + 2.0;
                    ps.push(lerp(p(i - 1), p(i), t));
                    qs.push(slerp_near(q(i - 1), q(i), t));
                }
                if i + 2 == n {
                    for k in 0..8 {
                        let t = 9.0 - k as f32;
                        ps.push(lerp(p(0), p(i + 1), t));
                        qs.push(slerp_near(q(0), q(i + 1), t));
                    }
                } else if i + 2 < n {
                    for k in 0..8 {
                        let t = 9.0 - k as f32;
                        ps.push(lerp(p(i + 2), p(i + 1), t));
                        qs.push(slerp_near(q(i + 2), q(i + 1), t));
                    }
                }
            }
        }
        at.push(ps.len());
        if !last_cut {
            for k in 0..8 {
                ps.push(p(k));
                qs.push(q(k));
            }
        }
        // Rotations on one side of each other, then mapped for the spline.
        for j in 0..qs.len() - 1 {
            if qs[j].dot(qs[j + 1]) < 0.0 {
                qs[j + 1] = -qs[j + 1];
            }
        }
        let mapped: Vec<[f32; 4]> = qs
            .iter()
            .map(|q| {
                let f = 1.0 / ((1.0 - q.w) * 2.0).max(1e-12).sqrt();
                [q.x * f, q.y * f, q.z * f, (1.0 - q.w) * f]
            })
            .collect();
        let pa: Vec<[f32; 3]> = ps.iter().map(|v| v.to_array()).collect();
        let p2 = spline(&pa);
        let q2 = spline(&mapped);
        let get = |v: &[[f32; 3]], i: usize| v.get(i).map_or(Vec3::ZERO, |a| Vec3::from(*a));
        let get4 = |v: &[[f32; 4]], i: usize| v.get(i).map_or(Vec4::ZERO, |a| Vec4::from(*a));
        let mut t = 0.0;
        for (i, &k) in at.iter().enumerate() {
            self.pos.push(get(&pa, k));
            self.pos2.push(get(&p2, k));
            self.rot.push(get4(&mapped, k));
            self.rot2.push(get4(&q2, k));
            self.time.push(t);
            if i + 1 < n && self.cuts[i] {
                t -= 1.0;
            }
            t += 1.0;
        }
    }

    /// The place and rotation `t` seconds in (0x41fe20, 0x420000); a looping path wraps.
    pub fn at(&self, t: f32) -> Option<(Vec3, Quat)> {
        if self.time.len() < 2 {
            return None;
        }
        let total = self.length();
        let t = if self.loops() {
            t.rem_euclid(total)
        } else {
            t.clamp(0.0, total)
        };
        let (mut lo, mut hi) = (0, self.time.len() - 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.time[mid] <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let h = (self.time[hi] - self.time[lo]).max(1e-6);
        let a = (self.time[hi] - t) / h;
        let b = (t - self.time[lo]) / h;
        let (ca, cb) = ((a * a * a - a) * h * h / 6.0, (b * b * b - b) * h * h / 6.0);
        let pos = self.pos[lo] * a + self.pos[hi] * b + self.pos2[lo] * ca + self.pos2[hi] * cb;
        let r = self.rot[lo] * a + self.rot[hi] * b + self.rot2[lo] * ca + self.rot2[hi] * cb;
        let s = 1.0 / r.length_squared().max(1e-12);
        let v = r.truncate();
        let q = Vec4::new(
            2.0 * r.w * v.x * s,
            2.0 * r.w * v.y * s,
            2.0 * r.w * v.z * s,
            (v.length_squared() - r.w * r.w) * s,
        );
        let q = q.normalize_or(Vec4::W);
        Some((pos, Quat::from_xyzw(q.x, q.y, q.z, q.w)))
    }
}
