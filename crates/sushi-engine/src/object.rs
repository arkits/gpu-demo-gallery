//! Animated objects: meshes, bone tracks and parameter tracks.
use std::collections::BTreeMap;

use glam::{Mat4, Vec3};
use sushi_formats::anim::{Animation, Key, ParamTrack, Track};
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

/// Key start times, so playback can use a binary search. Keys at the same time are cuts.
#[derive(Debug, Clone)]
pub struct Times(Vec<f64>);

impl Times {
    pub fn new(gaps: &[f64]) -> Self {
        let mut t = 0.0;
        Self(
            gaps.iter()
                .map(|g| {
                    let s = t;
                    t += g;
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
        // Moments of keys come from sums of rounded steps.
        let time = f64::from(time) + 1e-4;
        let i = self
            .0
            .partition_point(|s| *s <= time)
            .saturating_sub(1)
            .min(n - 2);
        let gap = self.0[i + 1] - self.0[i];
        let f = if gap > 1e-9 {
            (time - 1e-4 - self.0[i]) / gap
        } else {
            1.0
        };
        (i, f.clamp(0.0, 1.0) as f32)
    }
}

#[derive(Debug)]
struct Bone {
    times: Times,
    keys: Vec<Key>,
}

/// Where a bone is, and what `VisibilityFrame` says of it.
#[derive(Debug, Clone, Copy)]
pub struct Place {
    pub matrix: Mat4,
    pub visible: f32,
    pub frame: f32,
    pub frames: f32,
}

impl Bone {
    fn new(track: &Track) -> Self {
        Self {
            times: Times::new(&track.gaps),
            keys: track.keys.clone(),
        }
    }

    /// Neighboring keys are blended component by component (0x46dab0).
    fn at(&self, time: f32) -> Place {
        let (i, f) = self.times.locate(time);
        let (a, b) = (&self.keys[i], &self.keys[(i + 1).min(self.keys.len() - 1)]);
        Place {
            matrix: Mat4::from_cols_array(&std::array::from_fn(|k| {
                a.matrix[k] * (1.0 - f) + b.matrix[k] * f
            })),
            visible: a.visible,
            frame: a.frame * (1.0 - f) + b.frame * f,
            frames: a.frames,
        }
    }
}

#[derive(Debug)]
pub struct Clip {
    duration: f32,
    bones: Vec<Bone>,
    params: Vec<ParamTrack>,
    visibility: Option<ParamTrack>,
    bounds: Vec<[f32; 10]>,
    bound_times: Times,
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
            bound_times: Times::new(&a.bound_gaps),
        }
    }

    pub fn wrap(&self, time: f32) -> f32 {
        time.max(0.0).rem_euclid(self.duration.max(1e-3))
    }

    /// 0x431d50: the covering key's integer value; invisible when zero.
    pub fn visible(&self, time: f32) -> bool {
        let Some(v) = self.visibility.as_ref().filter(|v| v.keys.len() > 2) else {
            return true;
        };
        let (i, _) = locate(v.keys.iter().map(|k| k[0]), v.keys.len(), self.wrap(time));
        v.keys[i][1].to_bits() != 0
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn bones(&self, time: f32) -> Vec<Place> {
        let t = self.wrap(time);
        self.bones.iter().map(|b| b.at(t)).collect()
    }

    pub fn bone(&self, index: usize, time: f32) -> Mat4 {
        self.bones
            .get(index)
            .map_or(Mat4::IDENTITY, |b| b.at(self.wrap(time)).matrix)
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
        let (i, f) = self.bound_times.locate(self.wrap(time));
        let (a, b) = (&self.bounds[i.min(last)], &self.bounds[(i + 1).min(last)]);
        let v = |o: usize| {
            Vec3::new(a[o], a[o + 1], a[o + 2]).lerp(Vec3::new(b[o], b[o + 1], b[o + 2]), f)
        };
        Some((v(0), v(3), v(6), a[9] + (b[9] - a[9]) * f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_walks_gaps_and_clamps() {
        let gaps = [1.0, 2.0, 1.0, -1.0];
        let at = |t| locate(gaps.into_iter().map(|g| g as f32), 4, t);
        assert_eq!(at(0.5), (0, 0.5));
        assert_eq!(at(2.0), (1, 0.5));
        assert_eq!(at(99.0), (2, 1.0));
        assert_eq!(Times::new(&gaps).locate(2.0), (1, 0.5));
        assert_eq!(Times::new(&gaps).locate(99.0), (2, 1.0));
    }

    #[test]
    fn keys_at_the_same_time_are_cuts() {
        let t = Times::new(&[1.0, 0.0, 1.0, -1.0]);
        assert_eq!(t.locate(0.5), (0, 0.5));
        assert_eq!(t.locate(1.0).0, 2);
        assert_eq!(t.locate(1.5), (2, 0.5));
    }
}
