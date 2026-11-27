//! Scene lights, their movement, and how objects choose them (0x419130, 0x41c480).
use glam::{Mat4, Vec3, Vec4};
use sushi_formats::reader::{cstr, f32_at, u32_at};
use sushi_formats::scene;

use crate::object::Times;

/// A light at a moment.
#[derive(Debug, Clone)]
pub struct Light {
    pub name: String,
    spot: bool,
    /// Full angles of the cone in degrees, outer and inner.
    pub cone: f32,
    pub inner: f32,
    pub range: f32,
    /// The range matters.
    limited: bool,
    /// Shines on everything (+0x19d).
    always: bool,
    diffuse: bool,
    specular: bool,
    pub position: Vec3,
    pub x: Vec3,
    pub y: Vec3,
    pub direction: Vec3,
    /// A number beside the first axis, used by some shaders.
    pub extra: f32,
    pub object_ambient: bool,
    pub color: Vec3,
    pub intensity: f32,
}

/// A light and its keys.
#[derive(Debug, Clone)]
pub struct Moving {
    still: Light,
    times: Option<Times>,
    keys: Vec<[f32; 24]>,
    duration: f32,
}

impl Moving {
    pub fn new(l: &scene::Light) -> Self {
        let r = &l.raw;
        let v3 = |o: usize| Vec3::new(f32_at(r, o), f32_at(r, o + 4), f32_at(r, o + 8));
        let still = Light {
            name: cstr(&r[..0x20]),
            spot: u32_at(r, 0x20) == 1,
            cone: f32_at(r, 0x53),
            inner: f32_at(r, 0x5b),
            range: f32_at(r, 0x32),
            limited: u32_at(r, 0x24) != 0,
            always: r[0x19d] == 1,
            diffuse: r[0x19b] != 0,
            specular: r[0x19c] != 0,
            position: v3(0x5f),
            x: v3(0x6f),
            y: v3(0x7f),
            direction: v3(0x8f),
            extra: f32_at(r, 0x7b),
            object_ambient: r[0x2a] == 1,
            color: v3(0x36),
            intensity: f32_at(r, 0x2e),
        };
        let mut keys = Vec::new();
        let (mut times, mut duration) = (None, 0.0);
        // Keys of 0x60 bytes: seconds to the next, color, intensity, range, position,
        // three axes with a number each, cone and inner cone. The last one holds the sum.
        if r[0x46] == 1 && r[0x19e] == 2 && l.table_4b.len() > 2 {
            let n = l.table_4b.len() - 1;
            let read = |b: &Vec<u8>| std::array::from_fn(|i| f32_at(b, i * 4));
            keys = l.table_4b[..n].iter().map(read).collect::<Vec<[f32; 24]>>();
            duration = f32_at(&l.table_4b[n], 8);
            let gaps: Vec<f64> = keys.iter().map(|k| f64::from(k[0])).collect();
            times = Some(Times::new(&gaps));
        }
        Self {
            still,
            times,
            keys,
            duration,
        }
    }

    pub fn at(&self, time: f32) -> Light {
        let mut l = self.still.clone();
        let Some(times) = &self.times else { return l };
        let (i, f) = times.locate(time.max(0.0).rem_euclid(self.duration.max(0.1)));
        let (a, b) = (&self.keys[i], &self.keys[(i + 1).min(self.keys.len() - 1)]);
        let v: [f32; 24] = std::array::from_fn(|k| a[k] * (1.0 - f) + b[k] * f);
        let v3 = |o: usize| Vec3::new(v[o], v[o + 1], v[o + 2]);
        (l.color, l.intensity, l.position) = (v3(1), v[5], v3(7));
        (l.x, l.extra, l.y, l.direction) = (v3(10), v[13], v3(14), v3(18));
        (l.cone, l.inner) = (v[22], v[23]);
        l
    }
}

impl Light {
    /// How strongly the light reaches a sphere: 0 when out of range or cone.
    fn reach(&self, center: Vec3, radius: f32) -> f32 {
        if (self.color.length_squared() * self.intensity).abs() < 1e-6 {
            return 0.0;
        }
        let to = center - self.position;
        let distance = to.length();
        let left = distance - radius;
        let mut strength = 1.0;
        if self.spot {
            if left < 0.0 {
                return 1.0;
            }
            let angle = (to / distance)
                .dot(self.direction)
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            let half = self.cone * 0.5;
            let spread = (radius / left.max(1e-6))
                .clamp(0.0, 1.0)
                .asin()
                .to_degrees();
            if half + spread < angle {
                return 0.0;
            }
            strength = 1.0 - ((angle - spread) / half).clamp(0.0, 1.0);
        }
        match self.limited && left >= self.range {
            true => 0.0,
            false => strength,
        }
    }

    pub fn shines_on(&self, center: Vec3, radius: f32) -> bool {
        self.object_ambient && (self.always || self.reach(center, radius) > 0.0)
    }

    /// `ObjectAmbientPos`
    pub fn place(&self) -> [f32; 4] {
        self.position.extend(1.0 / self.range).to_array()
    }
    /// `ObjectAmbientVec`
    pub fn vector(&self) -> [f32; 4] {
        self.direction.extend(self.extra).to_array()
    }
    fn cosine(angle: f32) -> f32 {
        (angle * 0.5).to_radians().cos()
    }
    /// `ObjectAmbientParams`
    pub fn params(&self) -> [f32; 4] {
        [
            self.cone,
            self.inner,
            Self::cosine(self.cone),
            Self::cosine(self.inner),
        ]
    }
    fn flag(on: bool) -> f32 {
        f32::from(u8::from(on))
    }
    /// `ObjectAmbientLightComponents`
    pub fn components(&self) -> [f32; 4] {
        let [.., outer, inner] = self.params();
        [
            Self::flag(self.diffuse),
            Self::flag(self.specular),
            outer,
            inner,
        ]
    }
    /// `ObjectAmbientColor`
    pub fn shade(&self) -> [f32; 4] {
        (self.color * self.intensity * 0.5).extend(0.0).to_array()
    }
    /// `ObjectAmbientDiffuseColor`
    pub fn diffuse_color(&self) -> [f32; 4] {
        let c = self.color * (self.intensity * 0.5 * Self::flag(self.diffuse));
        c.extend(Self::cosine(self.cone)).to_array()
    }
    /// `ObjectAmbientSpecularColor`
    pub fn specular_color(&self) -> [f32; 4] {
        let c = self.color * (self.intensity * 0.5 * Self::flag(self.specular));
        c.extend(Self::cosine(self.inner)).to_array()
    }

    /// World to the light, which looks along -z.
    pub fn view(&self) -> Mat4 {
        let (x, y, z) = (self.x, self.y, -self.direction);
        let rows = Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), Vec4::W);
        rows.transpose() * Mat4::from_translation(-self.position)
    }

    /// Square picture from the light, as `SetSpotLightCamera` sees it (0x416650).
    pub fn projection(&self, near: f32, far: f32, scale: f32) -> Mat4 {
        let s = 1.0 / (self.cone * scale * 0.5).to_radians().tan();
        let depth = far - near;
        Mat4::from_cols_array_2d(&[
            [s, 0.0, 0.0, 0.0],
            [0.0, s, 0.0, 0.0],
            [0.0, 0.0, -far / depth, -1.0],
            [0.0, 0.0, -near * far / depth, 0.0],
        ])
    }

    /// Clip space of a projection to texture coordinates.
    pub fn texturing(m: Mat4) -> Mat4 {
        Mat4::from_cols_array_2d(&[
            [0.5, 0.0, 0.0, 0.0],
            [0.0, -0.5, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.5, 0.5, 0.0, 1.0],
        ]) * m
    }

    /// `ObjectAmbientViewProjMatrix` (0x435600, case 0x4e): depth from 1 to a million.
    pub fn texture_matrix(&self) -> Mat4 {
        Self::texturing(self.projection(1.0, 1.0e6, 1.0) * self.view())
    }
}
