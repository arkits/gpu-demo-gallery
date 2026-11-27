//! Maya animation curves (`0x4c3dc0` registers `Animation`; curves under `0x4ae940`).
//!
//! Interpolation differs from Dawn: 1 is linear, 2 is hermite (`poses.nma`
//! writes `linear`/`hermite` for those values), 0 is Bezier.
use crate::Error;
use crate::tree::Node;

#[derive(Debug, Clone)]
struct Key {
    time: f32,
    value: f32,
    /// Tangents, seconds and value.
    into: [f32; 2],
    out: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct Curve {
    /// Seconds.
    start: f32,
    stop: f32,
    before: u32,
    after: u32,
    method: u32,
    keys: Vec<Key>,
}

/// Moves one vector of a transform.
pub struct Channel {
    pub node: String,
    pub field: String,
    pub curves: [Option<Curve>; 3],
}

/// Moves named numbers: the weights of morph targets.
pub struct Number {
    pub nodes: Vec<String>,
    pub curve: Curve,
}

pub struct Animation {
    /// Seconds of curves per second of time.
    pub scale: f32,
    pub channels: Vec<Channel>,
    pub numbers: Vec<Number>,
}

impl Curve {
    fn read(n: &Node) -> Result<Self, Error> {
        let key = |k: &[f32]| Key {
            time: k[0],
            value: k[1],
            into: [k[2], k[4]],
            out: [k[3], k[5]],
        };
        let keys: Vec<Key> = n.floats("keyframes").chunks_exact(6).map(key).collect();
        if keys.is_empty() || !keys.is_sorted_by_key(|k| k.time) {
            return Err("bad keyframes".into());
        }
        let c = Self {
            start: n.float("startTime"),
            stop: n.float("stopTime"),
            before: n.int("preStart"),
            after: n.int("postStop"),
            method: n.int("interpMethod"),
            keys,
        };
        let known = |mode| mode <= 3;
        // A stop before the start means "no end": single-key constants use it.
        if !known(c.before) || !known(c.after) || c.method > 2 {
            return Err("unsupported curve".into());
        }
        Ok(c)
    }

    /// Outside its span a curve holds still (0), extrapolates linearly (1),
    /// repeats (2), or repeats from where it ended (3).
    pub fn at(&self, seconds: f64) -> f32 {
        if self.keys.len() == 1 {
            return self.keys[0].value;
        }
        let length = (f64::from(self.stop) - f64::from(self.start)).max(0.0);
        let local = seconds - f64::from(self.start);
        let mode = if local < 0.0 {
            self.before
        } else if local > length {
            self.after
        } else {
            return self.value(local as f32);
        };
        if length == 0.0 || mode == 0 {
            return self.value(local.clamp(0.0, length) as f32);
        }
        if mode == 1 {
            // Linear extrapolation along the edge segment.
            let (c, t) = if local < 0.0 {
                (&self.keys[0], local)
            } else {
                let c = &self.keys[self.keys.len() - 1];
                (c, local - length)
            };
            let slope = if local < 0.0 {
                let b = &self.keys[1.min(self.keys.len() - 1)];
                (b.value - self.keys[0].value) / (b.time - self.keys[0].time).max(1e-6)
            } else {
                let n = self.keys.len();
                let a = &self.keys[n - 2.min(n - 1)];
                (self.keys[n - 1].value - a.value) / (self.keys[n - 1].time - a.time).max(1e-6)
            };
            return c.value + slope * t as f32;
        }
        let value = self.value(local.rem_euclid(length) as f32);
        let turns = (local / length).floor() as f32;
        match mode {
            3 => value + turns * (self.value(length as f32) - self.value(0.0)),
            _ => value,
        }
    }

    fn value(&self, t: f32) -> f32 {
        let i = self.keys.partition_point(|k| k.time <= t);
        let (Some(a), Some(b)) = (i.checked_sub(1).map(|i| &self.keys[i]), self.keys.get(i)) else {
            // Before the first or after the last key.
            return self.keys[if i == 0 { 0 } else { i - 1 }].value;
        };
        let d = b.time - a.time;
        let u = (t - a.time) / d;
        match self.method {
            // 1 is linear and 2 is hermite (`poses.nma` writes those words for
            // these values); 0 is Bezier.
            1 => a.value + (b.value - a.value) * u,
            2 => {
                // Hermite: the tangents count as slopes.
                let slope = |x: f32, y: f32| if x == 0.0 { 5_729_578.0 } else { y / x };
                let (m0, m1) = (
                    slope(a.out[0], a.out[1]) * d,
                    slope(b.into[0], b.into[1]) * d,
                );
                let dv = b.value - a.value;
                ((m0 + m1 - 2.0 * dv) * u + (3.0 * dv - 2.0 * m0 - m1)) * u * u + m0 * u + a.value
            }
            _ => bezier(a, b, u, d),
            // 0 is Bezier; 1 is linear and 2 is hermite (see module docs).
        }
    }
}

/// A Bezier curve through (time, value), its control points a third of the tangents
/// away. Points outside the segment are drawn in, so that time only
/// goes forward.
fn bezier(a: &Key, b: &Key, u: f32, d: f32) -> f32 {
    let (x1, x2) = (a.out[0] / 3.0 / d, 1.0 - b.into[0] / 3.0 / d);
    let (mut y1, mut y2) = (a.value + a.out[1] / 3.0, b.value - b.into[1] / 3.0);
    let (c1, mut c2) = (x1.max(0.0), x2.min(1.0));
    let mut c1 = c1;
    if c1 > 1.0 || c2 < -1.0 / 3.0 {
        let mut f = (1.0 - c2).max(0.0);
        let g = c1 - 2.0;
        if (c1 > 1.0 || f > 1.0) && (f - 2.0) * f + (g + f) * c1 + 1.0 > 0.0 {
            if c1 >= 4.0 / 3.0 {
                (c1, f) = (4.0 / 3.0, 1.0 / 3.0);
            } else {
                let r = (g * g - 4.0 * (c1 - 1.0) * (c1 - 1.0)).max(0.0).sqrt();
                let (low, high) = ((-g - r) / 2.0, (r - g) / 2.0);
                f = f.clamp(low, high.max(low));
            }
        }
        c2 = 1.0 - f;
    }
    if c1 != x1 && x1 != 0.0 {
        y1 = a.value + (y1 - a.value) * c1 / x1;
    }
    if c2 != x2 && x2 != 1.0 {
        y2 = b.value - (1.0 - c2) * (b.value - y2) / (1.0 - x2);
    }
    let cubic = |p: [f32; 4], s: f32| {
        let q = 1.0 - s;
        q * q * q * p[0] + 3.0 * q * q * s * p[1] + 3.0 * q * s * s * p[2] + s * s * s * p[3]
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (low + high) / 2.0;
        if cubic([0.0, c1, c2, 1.0], mid) < u {
            low = mid
        } else {
            high = mid
        }
    }
    cubic([a.value, y1, y2, b.value], (low + high) / 2.0)
}

impl Animation {
    pub fn read(root: &Node) -> Result<Self, Error> {
        let channel = |n: &Node| -> Result<Channel, Error> {
            let curve = |f| n.node(f).map(Curve::read).transpose();
            Ok(Channel {
                node: n.text("targetNode").to_owned(),
                field: n.text("targetField").to_owned(),
                curves: [curve("xCurve")?, curve("yCurve")?, curve("zCurve")?],
            })
        };
        let number = |n: &Node| -> Result<Number, Error> {
            Ok(Number {
                nodes: n.texts("targetNodes").to_vec(),
                curve: Curve::read(n.node("curve").ok_or("channel without curve")?)?,
            })
        };
        let of = |kind| {
            root.nodes("channels")
                .iter()
                .filter(move |n| n.kind == kind)
        };
        Ok(Self {
            scale: root.float("timeScale"),
            channels: of("MayaVec3fChannel")
                .map(channel)
                .collect::<Result<_, _>>()?,
            numbers: of("MayaFloatChannel")
                .map(number)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(method: u32, before: u32, after: u32) -> Curve {
        let key = |time, value, slope: f32| Key {
            time,
            value,
            into: [1.0, slope],
            out: [1.0, slope],
        };
        Curve {
            start: 0.1,
            stop: 1.1,
            before,
            after,
            method,
            keys: vec![key(0.0, 1.0, 0.0), key(1.0, 3.0, 0.0)],
        }
    }

    #[test]
    fn repeats_or_holds() {
        for m in 0..3 {
            let near = |c: &Curve, t: f64, v: f32| assert!((c.at(t) - v).abs() < 1e-2, "{t}");
            let hold = curve(m, 0, 0);
            for (ms, v) in [(0.05, 1.0), (0.1, 1.0), (0.6, 2.0), (1.1, 3.0), (9e9, 3.0)] {
                near(&hold, ms, v);
            }
            near(&curve(m, 2, 2), 1.6, 2.0);
            near(&curve(m, 2, 2), -0.4, 2.0);
            near(&curve(m, 3, 3), 1.6, 4.0);
            near(&curve(m, 3, 3), 2.6, 6.0);
            near(&curve(m, 3, 3), -0.4, 0.0);
        }
        // Flat tangents ease in: both cubics agree, the line does not.
        // Here 0 is Bezier, 1 is linear and 2 is hermite.
        let at = |m| curve(m, 0, 0).at(0.35);
        assert!((at(0) - at(2)).abs() < 1e-3 && at(1) > at(0) + 0.1);
    }
}
