//! Light as spherical harmonics, for objects that carry light transfer (0x45b070).
use glam::{DMat3, DVec3, Mat3, Vec3};
use sushi_formats::transfer::Transfer;

const MOST: usize = 36;

/// Real spherical harmonics with the signs of D3DX, in its order.
pub fn basis(order: usize, d: DVec3) -> [f64; MOST] {
    let mut out = [0.0; MOST];
    // Associated Legendre polynomials divided by sin^m, times the parts of (x + iy)^m.
    let (mut re, mut im) = (1.0, 0.0);
    let mut pmm = 1.0;
    let factorial = |n: usize| (1..=n).map(|k| k as f64).product::<f64>();
    for m in 0..order {
        if m > 0 {
            (re, im) = (re * d.x - im * d.y, re * d.y + im * d.x);
            pmm *= -((2 * m - 1) as f64);
        }
        let (mut before, mut here) = (0.0, pmm);
        for l in m..order {
            if l > m {
                let next = ((2 * l - 1) as f64 * d.z * here - (l + m - 1) as f64 * before)
                    / (l - m) as f64;
                (before, here) = (here, next);
            }
            let k = ((2 * l + 1) as f64 / (4.0 * std::f64::consts::PI) * factorial(l - m)
                / factorial(l + m))
            .sqrt();
            let at = l * (l + 1);
            if m == 0 {
                out[at] = k * here;
            } else {
                let k = k * std::f64::consts::SQRT_2 * here;
                out[at + m] = k * re;
                out[at - m] = k * im;
            }
        }
    }
    out
}

/// Directions spread over the sphere, and what turns values there into coefficients.
pub struct Harmonics {
    order: usize,
    directions: Vec<DVec3>,
    /// `fit[coefficient][direction]`.
    fit: Vec<Vec<f64>>,
}

impl Harmonics {
    pub fn new(order: usize) -> Self {
        let n = order * order;
        let count = n * 2;
        let golden = std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
        let directions: Vec<DVec3> = (0..count)
            .map(|i| {
                let z = 1.0 - (2 * i + 1) as f64 / count as f64;
                let r = (1.0 - z * z).sqrt();
                let a = golden * i as f64;
                DVec3::new(r * a.cos(), r * a.sin(), z)
            })
            .collect();
        let rows: Vec<[f64; MOST]> = directions.iter().map(|d| basis(order, *d)).collect();
        // Least squares: invert the normal matrix by elimination.
        let mut a = vec![vec![0.0; n * 2]; n];
        for i in 0..n {
            for j in 0..n {
                a[i][j] = rows.iter().map(|r| r[i] * r[j]).sum();
            }
            a[i][n + i] = 1.0;
        }
        for i in 0..n {
            let p = a[i][i];
            a[i].iter_mut().for_each(|v| *v /= p);
            let pivot = a[i].clone();
            for (j, row) in a.iter_mut().enumerate() {
                if j != i {
                    let f = row[i];
                    row.iter_mut().zip(&pivot).for_each(|(v, p)| *v -= f * p);
                }
            }
        }
        let fit = (0..n)
            .map(|i| {
                rows.iter()
                    .map(|r| (0..n).map(|j| a[i][n + j] * r[j]).sum())
                    .collect()
            })
            .collect();
        Self {
            order,
            directions,
            fit,
        }
    }

    /// What multiplies coefficients to turn their function by `m`.
    pub fn rotation(&self, m: Mat3) -> Vec<f32> {
        let n = self.order * self.order;
        let back = DMat3::from_cols(
            m.x_axis.as_dvec3(),
            m.y_axis.as_dvec3(),
            m.z_axis.as_dvec3(),
        )
        .transpose();
        let turned: Vec<[f64; MOST]> = self
            .directions
            .iter()
            .map(|d| basis(self.order, (back * *d).normalize()))
            .collect();
        let mut out = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                let v: f64 = self.fit[i].iter().zip(&turned).map(|(f, t)| f * t[j]).sum();
                out.push(v as f32);
            }
        }
        out
    }

    /// `AtiSHEvalDirectionalLight` (0x450230 in the X800 SushiDX.exe): a surface facing
    /// the light shows its color. The normalisation is ATI's table at 0x54b840.
    pub fn directional(&self, d: Vec3, color: Vec3) -> [Vec<f32>; 3] {
        let pi = std::f64::consts::PI;
        let scale = match self.order {
            0 | 1 => 4.0 * pi,
            2 => 4.0 * pi / 3.0,
            3 | 4 => 16.0 * pi / 17.0,
            _ => 32.0 * pi / 31.0,
        };
        let b = basis(self.order, d.as_dvec3());
        let n = self.order * self.order;
        color
            .to_array()
            .map(|c| b[..n].iter().map(|v| (v * scale) as f32 * c).collect())
    }
}

/// Light for `PrtCPCABuffer` sources computed while running (0x45b010 in the X800
/// SushiDX.exe), in the object's space.
#[derive(Debug, Clone, Default)]
pub struct Source {
    /// `Runtime`: the block's named scene lights, as directional lights seen from the centroid.
    pub runtime: bool,
    /// `Cubemap`: the environment the exporter projected into the block.
    pub cubemap: bool,
    pub scale: Option<f32>,
    /// `Rotate x y z`: an axis scaled by an angle in radians.
    pub rotate: Option<Vec3>,
    pub centroid: Option<Vec3>,
}

impl Source {
    /// The words after the vector index: `"Runtime"`, `Scale, 2.0`, `Rotate, 0, -1.9, 0`, ...
    pub fn parse(args: &[String]) -> Option<Self> {
        let mut s = Self::default();
        let number = |i: usize| args.get(i)?.trim_matches('"').parse::<f32>().ok();
        let mut i = 0;
        while i < args.len() {
            match args[i].trim_matches('"').to_ascii_lowercase().as_str() {
                "runtime" => s.runtime = true,
                "cubemap" => s.cubemap = true,
                "presampled" => return None,
                "scale" => {
                    s.scale = number(i + 1);
                    i += 1;
                }
                "rotate" => {
                    let v = Vec3::new(number(i + 1)?, number(i + 2)?, number(i + 3)?);
                    // Further rotations add up (0x45a210).
                    s.rotate = Some(s.rotate.unwrap_or(Vec3::ZERO) + v);
                    i += 3;
                }
                "centroid" => {
                    s.centroid = Some(Vec3::new(number(i + 1)?, number(i + 2)?, number(i + 3)?));
                    i += 3;
                }
                _ => {}
            }
            i += 1;
        }
        (s.runtime || s.cubemap).then_some(s)
    }

    /// Spherical harmonics of the light, by color, in the object's space. `world` turns the
    /// object's directions into the world's; `centroid` is the default place to look from.
    pub fn light<'a>(
        &self,
        t: &Transfer,
        h: &Harmonics,
        lights: impl Iterator<Item = &'a crate::light::Light> + Clone,
        world: Mat3,
        centroid: Vec3,
    ) -> [Vec<f32>; 3] {
        let n = t.order * t.order;
        let mut out: [Vec<f32>; 3] = std::array::from_fn(|_| vec![0.0; n]);
        let add = |out: &mut [Vec<f32>; 3], v: &[Vec<f32>; 3]| {
            for (o, v) in out.iter_mut().zip(v) {
                o.iter_mut().zip(v).for_each(|(o, v)| *o += v);
            }
        };
        let to_object = world.inverse();
        if self.runtime {
            let from = self.centroid.unwrap_or(centroid);
            for name in &t.lights {
                let Some(l) = lights.clone().find(|l| l.name.eq_ignore_ascii_case(name)) else {
                    continue;
                };
                let d = (to_object * (l.position - from).normalize_or_zero()).normalize_or_zero();
                add(&mut out, &h.directional(d, l.color * l.intensity));
            }
        }
        if self.cubemap && !t.projected_lights.is_empty() {
            let rotation = h.rotation(to_object);
            for p in &t.projected_lights {
                let p: [Vec<f32>; 3] = std::array::from_fn(|c| turn(&rotation, &p[c][..n]));
                add(&mut out, &p);
            }
        }
        if let Some(k) = self.scale {
            out.iter_mut().flatten().for_each(|v| *v *= k);
        }
        // The axis-angle matrix (0x44d2c0) is transposed before AtiSHRotate, so light from
        // d ends up at the axis turned by minus the angle.
        if let Some(v) = self.rotate.filter(|v| v.length() > 1e-3) {
            let rotation = h.rotation(Mat3::from_axis_angle(v.normalize(), -v.length()));
            out = std::array::from_fn(|c| turn(&rotation, &out[c]));
        }
        out
    }
}

fn turn(rotation: &[f32], v: &[f32]) -> Vec<f32> {
    let n = v.len();
    rotation
        .chunks_exact(n)
        .take(n)
        .map(|r| r.iter().zip(v).map(|(a, b)| a * b).sum())
        .collect()
}

/// The light around an object at a moment, in its own space.
#[derive(Debug, Clone, Default)]
pub struct Around {
    pub position: Vec3,
    pub colors: [Vec<f32>; 3],
    /// Change along x, y and z, by color.
    pub slopes: Option<[[Vec<f32>; 3]; 3]>,
}

pub fn around(t: &Transfer, time: f64, h: &Harmonics, world: Mat3) -> Around {
    let Some(last) = t.samples.len().checked_sub(1) else {
        return Around {
            colors: std::array::from_fn(|_| vec![0.0; t.order * t.order]),
            ..Default::default()
        };
    };
    let i = t
        .samples
        .partition_point(|s| s.time <= time)
        .saturating_sub(1);
    let (a, b) = (&t.samples[i], &t.samples[(i + 1).min(last)]);
    let gap = b.time - a.time;
    let f = if gap > 1e-6 {
        ((time - a.time) / gap).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let mix = |a: &[f32], b: &[f32]| -> Vec<f32> {
        a.iter()
            .zip(b)
            .map(|(a, b)| a * (1.0 - f) + b * f)
            .collect()
    };
    let n = t.count;
    let rotation = h.rotation(world.inverse());
    let light = mix(&a.light, &b.light);
    let slopes = (!a.slopes.is_empty()).then(|| {
        let all = mix(&a.slopes, &b.slopes);
        std::array::from_fn(|axis| {
            std::array::from_fn(|c| {
                let v: Vec<f32> = (0..n).map(|k| all[(c * n + k) * 3 + axis]).collect();
                turn(&rotation, &v)
            })
        })
    });
    Around {
        position: Vec3::from(a.position).lerp(Vec3::from(b.position), f),
        colors: std::array::from_fn(|c| turn(&rotation, &light[c * n..(c + 1) * n])),
        slopes,
    }
}

/// `PrtCPCABuffer`: light times each cluster's mean and principal components. Every
/// cluster has the mean's color, then the components' red, green and blue parts.
pub fn cluster_buffer(t: &Transfer, vectors: usize, light: &Around, scale: f32) -> Vec<[f32; 4]> {
    cluster_buffer_of(t, vectors, &light.colors, scale)
}

/// [`cluster_buffer`] for light given as coefficients by color.
pub fn cluster_buffer_of(
    t: &Transfer,
    vectors: usize,
    colors: &[Vec<f32>; 3],
    scale: f32,
) -> Vec<[f32; 4]> {
    let n = t.order * t.order;
    let stride = n * 3 * (1 + vectors * 4);
    let mut out = Vec::new();
    for c in t.clusters.chunks_exact(stride.max(1)) {
        let dot = |block: usize, color: usize| -> f32 {
            let v = &c[(block * 3 + color) * n..][..n];
            v.iter()
                .zip(&colors[color])
                .map(|(a, b)| a * b)
                .sum::<f32>()
                * scale
        };
        out.push([dot(0, 0), dot(0, 1), dot(0, 2), 0.0]);
        for color in 0..3 {
            for v in 0..vectors {
                out.push(std::array::from_fn(|k| dot(1 + v * 4 + k, color)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_has_the_signs_of_d3dx() {
        let d = DVec3::new(0.36, 0.48, 0.8);
        let b = basis(3, d);
        let near = |a: f64, e: f64| assert!((a - e).abs() < 1e-4, "{a} {e}");
        near(b[0], 0.28209);
        near(b[1], -0.48860 * d.y);
        near(b[2], 0.48860 * d.z);
        near(b[3], -0.48860 * d.x);
        near(b[4], 1.09255 * d.x * d.y);
        near(b[5], -1.09255 * d.y * d.z);
        near(b[6], 0.31539 * (3.0 * d.z * d.z - 1.0));
        near(b[8], 0.54627 * (d.x * d.x - d.y * d.y));
    }

    #[test]
    fn rotation_turns_the_function() {
        let h = Harmonics::new(6);
        let m = Mat3::from_rotation_y(0.7) * Mat3::from_rotation_x(-1.1);
        let d = Vec3::new(0.6, 0.0, 0.8);
        let b = |d: Vec3| basis(6, d.as_dvec3()).map(|v| v as f32).to_vec();
        let turned = turn(&h.rotation(m), &b(d));
        for (a, e) in turned.iter().zip(b(m * d)) {
            assert!((a - e).abs() < 1e-3, "{a} {e}");
        }
    }
}
