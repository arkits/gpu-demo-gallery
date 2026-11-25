//! Maya animation curves, stored in binary viewer scenes.
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
    /// Milliseconds.
    start: i32,
    stop: i32,
    before: u32,
    after: u32,
    method: u32,
    keys: Vec<Key>,
}

pub struct Channel {
    pub node: String,
    pub field: String,
    pub curves: [Option<Curve>; 3],
}

impl Curve {
    fn read(n: &Node) -> Result<Self, Error> {
        let key = |k: &Node| -> Result<Key, Error> {
            let [in_x, out_x, in_y, out_y] = k.vec("tangents").unwrap_or([0.0; 4]);
            let value = k.float("value");
            Ok(Key {
                time: k.int("time") as i32 as f32 / 1000.0,
                value,
                into: [in_x, in_y],
                out: [out_x, out_y],
            })
        };
        let keys: Vec<Key> = n
            .nodes("keyframes")
            .iter()
            .map(key)
            .collect::<Result<_, _>>()?;
        if keys.is_empty() || !keys.is_sorted_by_key(|k| k.time) {
            return Err("bad keyframes".into());
        }
        let c = Self {
            start: n.int("startTime") as i32,
            stop: n.int("stopTime") as i32,
            before: n.int("preStart"),
            after: n.int("postStop"),
            method: n.int("interpMethod"),
            keys,
        };
        if c.stop < c.start || c.before > 5 || c.after > 5 || c.method > 2 {
            return Err("unsupported curve".into());
        }
        Ok(c)
    }

    /// `None` where the curve says nothing (0x46a8f0).
    pub fn at(&self, ms: f64) -> Option<f32> {
        let (start, stop) = (f64::from(self.start), f64::from(self.stop));
        let length = stop - start;
        if length == 0.0 {
            return (ms == start
                || (ms < start && self.before != 0)
                || (ms > stop && self.after != 0))
                .then(|| self.keys[0].value);
        }
        let local = if ms < start {
            if self.before == 0 {
                return None;
            }
            length - (start - ms) % length
        } else if ms <= stop {
            ms - start
        } else if self.after != 0 {
            (ms - stop) % length
        } else {
            return None;
        };
        Some(self.value((local / 1000.0) as f32))
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
            0 => a.value + (b.value - a.value) * u,
            1 => {
                // Hermite (0x46db10): the tangents count as slopes.
                let slope = |x: f32, y: f32| if x == 0.0 { 5_729_578.0 } else { y / x };
                let (m0, m1) = (
                    slope(a.out[0], a.out[1]) * d,
                    slope(b.into[0], b.into[1]) * d,
                );
                let dv = b.value - a.value;
                ((m0 + m1 - 2.0 * dv) * u + (3.0 * dv - 2.0 * m0 - m1)) * u * u + m0 * u + a.value
            }
            _ => bezier(a, b, u, d),
        }
    }
}

/// A Bezier curve through (time, value), its control points a third of the tangents
/// away (0x46d6c0). Points outside the segment are drawn in, so that time only
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

pub fn read(root: &Node) -> Result<Vec<Channel>, Error> {
    let channel = |n: &Node| -> Result<Channel, Error> {
        let curve = |f| n.node(f).map(Curve::read).transpose();
        Ok(Channel {
            node: n
                .texts("targetNodes")
                .first()
                .cloned()
                .unwrap_or_else(|| n.text("targetNode").to_owned()),
            field: n
                .texts("targetFields")
                .first()
                .cloned()
                .unwrap_or_else(|| n.text("targetField").to_owned()),
            curves: [curve("xCurve")?, curve("yCurve")?, curve("zCurve")?],
        })
    };
    root.nodes("channels")
        .iter()
        .filter(|n| n.kind == "MayaVec3fChannel")
        .map(channel)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(method: u32) -> Curve {
        let key = |time, value, slope: f32| Key {
            time,
            value,
            into: [1.0, slope],
            out: [1.0, slope],
        };
        Curve {
            start: 100,
            stop: 1100,
            before: 0,
            after: 1,
            method,
            keys: vec![key(0.0, 1.0, 0.0), key(1.0, 3.0, 0.0)],
        }
    }

    #[test]
    fn repeats_after_its_end() {
        for m in 0..3 {
            let c = curve(m);
            assert_eq!(c.at(50.0), None);
            assert!((c.at(100.0).unwrap() - 1.0).abs() < 1e-3);
            assert!((c.at(600.0).unwrap() - 2.0).abs() < 1e-3);
            let (a, b) = (c.at(700.0).unwrap(), c.at(1700.0).unwrap());
            assert!((a - b).abs() < 1e-3);
        }
        // Flat tangents ease in: both cubics agree, the line does not.
        let at = |m| curve(m).at(350.0).unwrap();
        assert!((at(1) - at(2)).abs() < 1e-3 && at(0) > at(1) + 0.1);
    }
}
