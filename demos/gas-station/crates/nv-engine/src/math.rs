//! Matrices as the viewers keep them: 16 floats, row-major, for column vectors
//! (`m[row * 4 + col]`, translation in the last column). They reach Cg row by row
//! (`cgGLSetMatrixParameterfr`), so row i of a matrix parameter is register c[n + i].

use std::ops::Mul;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4(pub [f32; 16]);

pub type Vec3 = [f32; 3];
pub type Vec4 = [f32; 4];

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]);

    pub fn at(&self, r: usize, c: usize) -> f32 {
        self.0[r * 4 + c]
    }

    pub fn row(&self, r: usize) -> Vec4 {
        [self.0[r * 4], self.0[r * 4 + 1], self.0[r * 4 + 2], self.0[r * 4 + 3]]
    }

    pub fn translation(t: Vec3) -> Mat4 {
        let mut m = Self::IDENTITY;
        m.0[3] = t[0];
        m.0[7] = t[1];
        m.0[11] = t[2];
        m
    }

    pub fn scale(s: Vec3) -> Mat4 {
        let mut m = Self::IDENTITY;
        m.0[0] = s[0];
        m.0[5] = s[1];
        m.0[10] = s[2];
        m
    }

    pub fn transpose(&self) -> Mat4 {
        let mut o = [0.0; 16];
        for r in 0..4 {
            for c in 0..4 {
                o[c * 4 + r] = self.0[r * 4 + c];
            }
        }
        Mat4(o)
    }

    /// The general inverse (cofactors); a singular matrix gives the identity.
    pub fn inverse(&self) -> Mat4 {
        let m: [f64; 16] = std::array::from_fn(|i| self.0[i] as f64);
        let mut inv = [0f64; 16];
        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];
        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det == 0.0 {
            return Self::IDENTITY;
        }
        Mat4(std::array::from_fn(|i| (inv[i] / det) as f32))
    }

    /// M·(p, 1), divided by w (`0x43d190`).
    pub fn point(&self, p: Vec3) -> Vec3 {
        let r = |i: usize| self.0[i * 4] * p[0] + self.0[i * 4 + 1] * p[1] + self.0[i * 4 + 2] * p[2] + self.0[i * 4 + 3];
        let w = r(3);
        [r(0) / w, r(1) / w, r(2) / w]
    }

    /// The upper 3×3 times v (`0x439e20`).
    pub fn vector(&self, v: Vec3) -> Vec3 {
        let r = |i: usize| self.0[i * 4] * v[0] + self.0[i * 4 + 1] * v[1] + self.0[i * 4 + 2] * v[2];
        [r(0), r(1), r(2)]
    }

    /// The rotation of a quaternion (w, x, y, z) (`0x42e6d0`).
    pub fn from_quat(q: Vec4) -> Mat4 {
        let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
        let mut m = Self::IDENTITY;
        m.0[0] = 1.0 - 2.0 * (z * z + y * y);
        m.0[4] = 2.0 * (x * y + z * w);
        m.0[8] = 2.0 * (x * z - y * w);
        m.0[1] = 2.0 * (x * y - z * w);
        m.0[5] = 1.0 - 2.0 * (z * z + x * x);
        m.0[9] = 2.0 * (x * w + z * y);
        m.0[2] = 2.0 * (x * z + y * w);
        m.0[6] = 2.0 * (z * y - x * w);
        m.0[10] = 1.0 - 2.0 * (y * y + x * x);
        m
    }

    /// Euler angles in radians (`0x42fbb0`); order 0 (Maya xyz) is Rz·Ry·Rx.
    pub fn euler(order: u32, r: Vec3) -> Mat4 {
        let rx = Self::axis(0, r[0]);
        let ry = Self::axis(1, r[1]);
        let rz = Self::axis(2, r[2]);
        // Maya's orders name the first rotation applied.
        match order {
            0 => rz * ry * rx,
            1 => rx * rz * ry,
            2 => ry * rx * rz,
            3 => ry * rz * rx,
            4 => rz * rx * ry,
            _ => rx * ry * rz,
        }
    }

    fn axis(axis: usize, a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut m = Self::IDENTITY;
        let (i, j) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        m.0[i * 4 + i] = c;
        m.0[i * 4 + j] = -s;
        m.0[j * 4 + i] = s;
        m.0[j * 4 + j] = c;
        m
    }

    /// gluPerspective with the full vertical angle in radians (`0x41ab80`).
    ///
    /// Backgrounds with `fovScale` 0 draw with a lens of 0: 1/tan(0) is infinite on the x87,
    /// NV vertex programs take 0 times infinity as 0, and the cube's faces cover the window,
    /// which is how the demos draw full-window passes. WebGPU drops infinite positions, so
    /// the port uses a focal length large enough to cover any window instead.
    pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let tan = (fov * 0.5).tan();
        let f = if tan == 0.0 { 1.0e7 } else { 1.0 / tan };
        let d = 1.0 / (near - far);
        Mat4([
            f / aspect, 0., 0., 0., //
            0., f, 0., 0., //
            0., 0., (near + far) * d, 2.0 * near * far * d, //
            0., 0., -1., 0.,
        ])
    }

    pub fn ortho(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> Mat4 {
        Mat4([
            2.0 / (r - l), 0., 0., -(r + l) / (r - l), //
            0., 2.0 / (t - b), 0., -(t + b) / (t - b), //
            0., 0., -2.0 / (f - n), -(f + n) / (f - n), //
            0., 0., 0., 1.,
        ])
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, b: Mat4) -> Mat4 {
        let a = &self.0;
        let b = &b.0;
        Mat4(std::array::from_fn(|i| {
            let (r, c) = (i / 4, i % 4);
            a[r * 4] * b[c] + a[r * 4 + 1] * b[4 + c] + a[r * 4 + 2] * b[8 + c] + a[r * 4 + 3] * b[12 + c]
        }))
    }
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

pub fn normalize(a: Vec3) -> Vec3 {
    let l = length(a);
    if l == 0.0 { a } else { [a[0] / l, a[1] / l, a[2] / l] }
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_xyz_matches_the_viewer() {
        // 0x42fbb0's fast path for order 0: m00 = cz cy, m10 = sz cy, m20 = -sy.
        let (x, y, z) = (0.3f32, -0.7f32, 1.1f32);
        let m = Mat4::euler(0, [x, y, z]);
        assert!((m.at(0, 0) - z.cos() * y.cos()).abs() < 1e-6);
        assert!((m.at(1, 0) - z.sin() * y.cos()).abs() < 1e-6);
        assert!((m.at(2, 0) + y.sin()).abs() < 1e-6);
        assert!((m.at(0, 1) - (-z.sin() * x.cos() + z.cos() * y.sin() * x.sin())).abs() < 1e-6);
    }

    #[test]
    fn inverse_round_trips() {
        let m = Mat4::translation([1., 2., 3.]) * Mat4::euler(0, [0.2, 0.4, 0.6]) * Mat4::scale([2., 3., 4.]);
        let i = m * m.inverse();
        for k in 0..16 {
            assert!((i.0[k] - Mat4::IDENTITY.0[k]).abs() < 1e-5);
        }
    }
}
