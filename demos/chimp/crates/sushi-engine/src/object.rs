//! Animated objects: meshes, bone tracks and parameter tracks.
use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};
use sushi_formats::anim::{Animation, ParamTrack, Track};
use sushi_formats::reader::{f32_at, u32_at};

/// Finds the key covering `time` and the fraction into it (0x438d10).
/// `gaps` yields the seconds from each key to the next.
fn locate(gaps: impl Iterator<Item = f32>, count: usize, time: f32) -> (usize, f32) {
    let mut start = 0.0;
    let last = count.saturating_sub(2);
    for (i, gap) in gaps.enumerate() {
        if i >= last || time < start + gap {
            let f = if gap > 0.0 { (time - start) / gap } else { 0.0 };
            return (i, f.clamp(0.0, 1.0));
        }
        start += gap;
    }
    (0, 0.0)
}

/// Key start times, so playback can use a binary search.
#[derive(Debug, Clone)]
pub struct Times(Vec<f32>);

impl Times {
    pub fn new(gaps: &[f32]) -> Self {
        let mut t = 0.0f64;
        Self(
            gaps.iter()
                .map(|g| {
                    let s = t as f32;
                    t += f64::from(*g);
                    s
                })
                .collect(),
        )
    }
    pub fn locate(&self, time: f32) -> (usize, f32) {
        let n = self.0.len();
        if n < 2 {
            return (0, 0.0);
        }
        let i = self
            .0
            .partition_point(|s| *s <= time)
            .saturating_sub(1)
            .min(n - 2);
        let f = (time - self.0[i]) / (self.0[i + 1] - self.0[i]);
        (i, f.clamp(0.0, 1.0))
    }
}

#[derive(Debug)]
struct Bone {
    times: Times,
    /// Row-vector matrices as stored, i.e. column-major for column vectors.
    keys: Vec<Mat4>,
}

impl Bone {
    fn new(track: &Track) -> Self {
        let affine = |m: &[f32], t: &[f32]| {
            Mat4::from_cols_array(&[
                m[0], m[1], m[2], 0.0, m[3], m[4], m[5], 0.0, m[6], m[7], m[8], 0.0, t[0], t[1],
                t[2], 1.0,
            ])
        };
        let quat = |q: &[f32], t: &[f32]| {
            let q = Quat::from_xyzw(q[0], q[1], q[2], q[3]);
            Mat4::from_rotation_translation(q, Vec3::new(t[0], t[1], t[2]))
        };
        let keys = match track {
            Track::Matrix(k) => k
                .iter()
                .map(|k| Mat4::from_cols_array(&std::array::from_fn(|i| k[i + 1])))
                .collect(),
            Track::Affine(k) => k.iter().map(|k| affine(&k[1..10], &k[10..13])).collect(),
            Track::RotationFixedPos(k) => k[1..]
                .iter()
                .map(|r| affine(&r[1..10], &k[0][1..4]))
                .collect(),
            Track::TranslationFixedRot(k) => {
                let m: Vec<f32> = k[..3].iter().flat_map(|r| r[1..4].to_vec()).collect();
                k[3..].iter().map(|r| affine(&m, &r[1..4])).collect()
            }
            Track::QuatPos(k) => k.iter().map(|k| quat(&k[1..5], &k[5..8])).collect(),
            Track::QuatFixedPos(k) => k[1..].iter().map(|r| quat(&r[1..5], &k[0][1..4])).collect(),
        };
        Self {
            times: Times::new(&track.dts()),
            keys,
        }
    }

    /// Neighboring keys are blended component by component (0x439bb0).
    fn at(&self, time: f32) -> Mat4 {
        let (i, f) = self.times.locate(time);
        let (a, b) = (&self.keys[i], &self.keys[(i + 1).min(self.keys.len() - 1)]);
        *a * (1.0 - f) + *b * f
    }
}

/// Seconds between the two positions used for ball motion blur. The original
/// used its own frame time; a fixed step keeps the look frame-rate independent.
pub const BLUR_STEP: f32 = 1.0 / 60.0;

#[derive(Debug)]
pub struct Clip {
    duration: f32,
    bones: Vec<Bone>,
    params: Vec<ParamTrack>,
    visibility: Option<ParamTrack>,
    bounds: Vec<[f32; 10]>,
    total: Option<[f32; 10]>,
}

impl Clip {
    pub fn new(a: &Animation) -> Self {
        let (visibility, params) = a
            .params
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|p| p.name == "SOLVisibility");
        let bounds = |b: &Vec<u8>| std::array::from_fn(|i| f32_at(b, i * 4));
        Self {
            duration: a.duration,
            bones: a.bones.iter().map(Bone::new).collect(),
            params,
            visibility: visibility.into_iter().next(),
            bounds: a.bounds.iter().map(bounds).collect(),
            total: (a.total_bounds.len() >= 40).then(|| bounds(&a.total_bounds)),
        }
    }

    pub fn wrap(&self, time: f32) -> f32 {
        time.max(0.0).rem_euclid(self.duration)
    }

    /// 0x431d50: the covering key's integer value; invisible when zero.
    pub fn visible(&self, time: f32) -> bool {
        let Some(v) = self.visibility.as_ref().filter(|v| v.keys.len() > 2) else {
            return true;
        };
        let (i, _) = locate(v.keys.iter().map(|k| k[0]), v.keys.len(), self.wrap(time));
        v.keys[i][1].to_bits() != 0
    }

    pub fn bones(&self, time: f32) -> Vec<Mat4> {
        let t = self.wrap(time);
        self.bones.iter().map(|b| b.at(t)).collect()
    }

    pub fn bone(&self, index: usize, time: f32) -> Mat4 {
        self.bones
            .get(index)
            .map_or(Mat4::IDENTITY, |b| b.at(self.wrap(time)))
    }

    /// Animated shader parameters by name (0x43db70).
    pub fn params(&self, time: f32) -> BTreeMap<String, [f32; 4]> {
        let t = self.wrap(time);
        let mut out = BTreeMap::new();
        for p in &self.params {
            let n = p.keys.len();
            if n == 0 {
                continue;
            }
            let (i, f) = locate(p.keys.iter().map(|k| k[0]), n, t);
            let (a, b) = (&p.keys[i], &p.keys[(i + 1).min(n - 1)]);
            // Kind at +0x50: 1 int, 2 float, 3 vector.
            let v = match u32_at(&p.raw, 0x50) {
                1 => [a[1].to_bits() as f32, 0.0, 0.0, 0.0],
                _ => std::array::from_fn(|k| a[k + 1] * (1.0 - f) + b[k + 1] * f),
            };
            out.insert(p.name.clone(), v);
        }
        out
    }

    /// Bounding box and sphere, interpolated between samples (0x431b00): min,
    /// max, center, radius.
    pub fn bounds(&self, time: f32) -> Option<(Vec3, Vec3, Vec3, f32)> {
        let last = self.bounds.len().checked_sub(1)?;
        let f = self.wrap(time) / self.duration * last as f32;
        let i = (f as usize).min(last);
        let (a, b) = (&self.bounds[i], &self.bounds[(i + 1).min(last)]);
        let f = f - i as f32;
        let v = |o: usize| {
            Vec3::new(a[o], a[o + 1], a[o + 2]).lerp(Vec3::new(b[o], b[o + 1], b[o + 2]), f)
        };
        Some((v(0), v(3), v(6), a[9] + (b[9] - a[9]) * f))
    }

    /// The box and sphere around every frame.
    pub fn total_bounds(&self) -> Option<(Vec3, Vec3, Vec3, f32)> {
        let a = self.total?;
        let v = |o: usize| Vec3::new(a[o], a[o + 1], a[o + 2]);
        Some((v(0), v(3), v(6), a[9]))
    }

    /// `SOLVelocityX` and `SOLOrientationX` for a center moving with bone 0.
    pub fn motion(&self, center: Vec3, time: f32) -> ([f32; 4], Mat4) {
        let at = |t: f32| self.bone(0, t).transform_point3(center);
        let move_ = at(time) - at((time - BLUR_STEP).max(0.0));
        let speed = move_.length();
        if !(1e-6..=80.0).contains(&speed) {
            return ([0.0; 4], Mat4::IDENTITY);
        }
        let dir = move_ / speed;
        let rotation = if dir.z < 0.99999 {
            let axis = Vec3::Z.cross(dir).normalize_or(Vec3::X);
            Mat4::from_axis_angle(axis, dir.z.clamp(-1.0, 1.0).acos())
        } else {
            Mat4::IDENTITY
        };
        ([dir.x, dir.y, dir.z, speed], rotation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_walks_gaps_and_clamps() {
        let gaps = [1.0, 2.0, 1.0, -1.0];
        let at = |t| locate(gaps.into_iter(), 4, t);
        assert_eq!(at(0.5), (0, 0.5));
        assert_eq!(at(2.0), (1, 0.5));
        assert_eq!(at(99.0), (2, 1.0));
        assert_eq!(Times::new(&gaps).locate(2.0), (1, 0.5));
        assert_eq!(Times::new(&gaps).locate(99.0), (2, 1.0));
    }

    #[test]
    fn motion_turns_z_toward_the_velocity() {
        let keys = vec![[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]; 2];
        let mut keys: Vec<[f32; 8]> = keys
            .into_iter()
            .map(|_| [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0])
            .collect();
        keys[1][5] = 6.0; // moves +x at 6 units/s
        keys[1][0] = -1.0;
        let clip = Clip {
            duration: 1.0,
            bones: vec![Bone::new(&Track::QuatPos(keys))],
            params: vec![],
            visibility: None,
            bounds: vec![],
            total: None,
        };
        let (v, r) = clip.motion(Vec3::ZERO, 0.5);
        assert!((v[3] - 0.1).abs() < 1e-4, "{v:?}");
        assert!((r.transform_vector3(Vec3::Z) - Vec3::X).length() < 1e-4);
    }
}
