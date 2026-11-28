//! Maya animation curves and the channels that drive transform fields.

use nv_scene::{Node, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub time: f32,
    pub value: f32,
    pub in_x: f32,
    pub out_x: f32,
    pub in_y: f32,
    pub out_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub start: f64,
    pub stop: f64,
    pub pre: u32,
    pub post: u32,
    pub method: u32,
    pub keys: Vec<Key>,
}

/// A curve's time mapped into its span (`0x4745c0`): local time, and how many spans of
/// value offset to add (mode 3).
fn local_time(c: &Curve, t: f64) -> (f64, u32) {
    let len = c.stop - c.start;
    // MSVC's _ftol truncates.
    let n_of = |a: f64| (a / len) as i32;
    if t < c.start {
        let a = c.start - t;
        match c.pre {
            2 | 3 => {
                let n = n_of(a);
                let out = len - (a - n as f64 * len);
                (out, if c.pre == 3 { (n + 1) as u32 } else { 0 })
            }
            4 => {
                let n = n_of(a);
                let r = a - n as f64 * len;
                (if n % 2 != 0 { r } else { len - r }, 0)
            }
            _ => (0.0, 0),
        }
    } else if t <= c.stop {
        (t - c.start, 0)
    } else {
        let a = t - c.stop;
        match c.post {
            2 | 3 => {
                let n = n_of(a);
                let out = a - n as f64 * len;
                (out, if c.post == 3 { (n + 1) as u32 } else { 0 })
            }
            4 => {
                let n = n_of(a);
                let r = a - n as f64 * len;
                (if n % 2 == 0 { r } else { len - r }, 0)
            }
            _ => (len, 0),
        }
    }
}

/// One segment (`0x47b2d0`, Hermite `0x47b1b0`). `t` is the double the caller holds; it is
/// compared as a float.
fn segment(k0: &Key, k1: &Key, t: f64, method: u32) -> f32 {
    let tf = t as f32;
    if tf <= k0.time {
        return k0.value;
    }
    if tf >= k1.time {
        return k1.value;
    }
    match method {
        0 => {
            let u = (t - k0.time as f64) / (k1.time as f64 - k0.time as f64);
            (u * k1.value as f64 + (1.0 - u) * k0.value as f64) as f32
        }
        _ => {
            // Tangents as slopes y / x; 5729578 where x is 0. The x87 code keeps extended
            // precision except where it stores floats: the incoming control point, dt, dv and
            // dt times the outgoing slope.
            let third = (1.0f32 / 3.0) as f64;
            let p2x = (k1.time as f64 - third * k1.in_x as f64) as f32;
            let p2y = (k1.value as f64 - third * k1.in_y as f64) as f32;
            let dt = (k1.time as f64 - k0.time as f64) as f32 as f64;
            let dv = (k1.value as f64 - k0.value as f64) as f32 as f64;
            let ox = third * k0.out_x as f64;
            let m0 = if ox == 0.0 { 5_729_578.0 } else { (third * k0.out_y as f64) / ox };
            let ix = k1.time as f64 - p2x as f64;
            let m1 = if ix == 0.0 { 5_729_578.0 } else { (k1.value as f64 - p2y as f64) / ix };
            let mm0 = (m0 * dt) as f32 as f64;
            let mm1 = m1 * dt;
            let inv2 = 1.0 / (dt * dt);
            let u = t - k0.time as f64;
            let a = ((mm1 + mm0) - 2.0 * dv) / dt * inv2;
            let b = (3.0 * dv - 2.0 * mm0 - mm1) * inv2;
            (((a * u + b) * u + m0) * u + k0.value as f64) as f32
        }
    }
}

impl Curve {
    pub fn read(n: &Node) -> Option<Curve> {
        let keys = match n.get("keyframes")? {
            Value::Keys(k) => k
                .iter()
                .map(|w| {
                    let f = |i: usize| f32::from_bits(w[i]);
                    Key { time: f(0), value: f(1), in_x: f(4), out_x: f(5), in_y: f(6), out_y: f(7) }
                })
                .collect(),
            _ => return None,
        };
        Some(Curve {
            // Doubles in the file (`MayaCurve` +0x28, +0x30).
            start: n.double("startTime").unwrap_or(0.0),
            stop: n.double("stopTime").unwrap_or(0.0),
            pre: n.int("preStart").unwrap_or(0),
            post: n.int("postStop").unwrap_or(0),
            method: n.int("interpMethod").unwrap_or(1),
            keys,
        })
    }

    /// The value at time t (`0x476e50`, `0x475200`).
    pub fn at(&self, t: f64) -> f32 {
        if self.keys.is_empty() {
            return 0.0;
        }
        let (local, cycles) = local_time(self, t);
        let lt = local as f32;
        let k = &self.keys;
        let v = if k.len() == 1 {
            k[0].value
        } else {
            // The segment whose end is the first key at or after t.
            let mut i = 0;
            while i + 2 < k.len() && k[i + 1].time < lt {
                i += 1;
            }
            segment(&k[i], &k[i + 1], local, self.method)
        };
        if cycles != 0 {
            (k[k.len() - 1].value - k[0].value) * cycles as f32 + v
        } else {
            v
        }
    }
}

/// What a channel moves.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A vector field (`rotation`, `translation`, `scale`) of a named node.
    Vec3 { node: String, field: String, curves: [Option<Curve>; 3] },
    /// A float field.
    Float { node: String, field: String, curve: Option<Curve> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,
    pub time_scale: f32,
    pub channels: Vec<Target>,
    /// Start time (`+0x40`) set when it starts.
    pub start: f64,
}

impl Animation {
    pub fn read(n: &Node) -> Animation {
        let mut channels = vec![];
        for c in n.many("channels") {
            let node = c.str("targetNode").unwrap_or("").to_string();
            let field = c.str("targetField").unwrap_or("").to_string();
            match c.kind {
                "MayaVec3fChannel" => {
                    let curve = |f: &str| c.one(f).and_then(Curve::read);
                    channels.push(Target::Vec3 { node, field, curves: [curve("xCurve"), curve("yCurve"), curve("zCurve")] });
                }
                "MayaFloatChannel" => {
                    channels.push(Target::Float { node, field, curve: c.one("curve").and_then(Curve::read) });
                }
                _ => {}
            }
        }
        Animation { name: n.name.clone(), time_scale: n.float("timeScale").unwrap_or(1.0), channels, start: 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f32, v: f32) -> Key {
        Key { time: t, value: v, in_x: 1.0, out_x: 1.0, in_y: 0.0, out_y: 0.0 }
    }

    #[test]
    fn hermite_holds_flat_tangents_at_the_keys() {
        let c = Curve { start: 0.0, stop: 2.0, pre: 0, post: 0, method: 1, keys: vec![key(0.0, 1.0), key(2.0, 3.0)] };
        assert_eq!(c.at(0.0), 1.0);
        assert_eq!(c.at(2.0), 3.0);
        assert!((c.at(1.0) - 2.0).abs() < 1e-6);
        // Constant outside.
        assert_eq!(c.at(5.0), 3.0);
        assert_eq!(c.at(-1.0), 1.0);
    }

    #[test]
    fn cycle_with_offset_adds_spans() {
        let c = Curve { start: 0.0, stop: 2.0, pre: 3, post: 3, method: 1, keys: vec![key(0.0, 1.0), key(2.0, 3.0)] };
        // 1 s past the end: local 1, one span of 2 added.
        assert!((c.at(3.0) - 4.0).abs() < 1e-6);
    }
}
