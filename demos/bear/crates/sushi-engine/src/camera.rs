use glam::{Mat4, Quat, Vec3, Vec4};
use sushi_formats::path::FlyPath;

pub const ORIGINAL_ASPECT: f32 = 4.0 / 3.0;

/// Values from `Sushi2.ini`.
#[derive(Debug, Clone, Copy)]
pub struct Lens {
    pub near: f32,
    pub far: f32,
    /// Horizontal, in degrees.
    pub fov: f32,
}

impl Lens {
    /// Right-handed, depth 0..1, horizontal field of view (0x42ba80), for a viewport of
    /// `aspect`. Windows of other shapes than the original 4:3 show less of the picture, never
    /// more: the forest is a flat picture that ends there.
    pub fn projection(&self, aspect: f32, window: f32) -> Mat4 {
        let half = (self.fov.to_radians() / 2.0).tan();
        let x = half * (window / ORIGINAL_ASPECT).min(1.0);
        let y = x / aspect;
        let depth = self.far - self.near;
        Mat4::from_cols_array_2d(&[
            [1.0 / x, 0.0, 0.0, 0.0],
            [0.0, 1.0 / y, 0.0, 0.0],
            [0.0, 0.0, -self.far / depth, -1.0],
            [0.0, 0.0, -self.near * self.far / depth, 0.0],
        ])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Camera {
    /// Samples the fly path with a spline through the 30 Hz samples, as the engine
    /// does (0x4093e0, 0x40a1c0). A flagged sample is the last of its shot: the
    /// camera holds it until the next shot begins.
    pub fn on_path(path: &FlyPath, time: f32) -> Self {
        let frames = &path.frames;
        let last = frames.len() - 1;
        let f = (time / path.step).clamp(0.0, last as f32);
        let i = f as usize;
        let t = if i == last || frames[i].flag != 0 {
            0.0
        } else {
            f - i as f32
        };
        // Neighbors beyond a cut repeat the end sample.
        let before = if i == 0 || frames[i - 1].flag != 0 {
            i
        } else {
            i - 1
        };
        let next = if t == 0.0 { i } else { i + 1 };
        let after = if next == last || frames[next].flag != 0 {
            next
        } else {
            next + 1
        };
        let pos = [before, i, next, after].map(|k| Vec3::from(frames[k].pos));
        let here = Quat::from_array(frames[i].quat);
        let quat = [before, i, next, after].map(|k| {
            let q = Quat::from_array(frames[k].quat);
            Vec4::from(if q.dot(here) < 0.0 { -q } else { q })
        });
        Self {
            position: spline(pos, t),
            rotation: Quat::from_vec4(spline(quat, t)).normalize(),
        }
    }

    /// Rotate after translating by -position (0x4123b0, 0x430420).
    pub fn view(&self) -> Mat4 {
        Mat4::from_quat(self.rotation) * Mat4::from_translation(-self.position)
    }
}

/// Catmull-Rom through `p[1]` and `p[2]`.
fn spline<V>(p: [V; 4], t: f32) -> V
where
    V: Copy
        + std::ops::Add<Output = V>
        + std::ops::Sub<Output = V>
        + std::ops::Mul<f32, Output = V>,
{
    let [a, b, c, d] = p;
    let (t2, t3) = (t * t, t * t * t);
    b + ((c - a) * t + (a * 2.0 - b * 5.0 + c * 4.0 - d) * t2 + (b * 3.0 - a - c * 3.0 + d) * t3)
        * 0.5
}

/// `projection * view` whose near plane is `plane` (world space, keeping its positive
/// side), so that nothing on the other side is drawn. Depth is only good for
/// comparing draws that use the same plane.
pub fn clipped(projection: Mat4, view: Mat4, plane: Vec4) -> Mat4 {
    let p = view.inverse().transpose() * plane;
    // The far corner of the view volume opposite the plane (Lengyel 2005).
    let corner = projection.inverse() * Vec4::new(p.x.signum(), p.y.signum(), 1.0, 1.0);
    let row = p / p.dot(corner);
    let mut m = projection.transpose();
    m.z_axis = row;
    m.transpose() * view
}

/// Interactive camera around the middle. Not in the original.
#[derive(Debug, Clone)]
pub struct Orbit(Vec3);

const TARGET: Vec3 = Vec3::new(2.0, -17.0, -4.0);
/// Distance, pitch and yaw.
const START: Vec3 = Vec3::new(130.0, -7.0, 41.0);
const DISTANCE: (f32, f32) = (70.0, 134.0);
const PITCH: (f32, f32) = (-11.0, 2.0);
/// The forest ends beyond.
const YAW: (f32, f32) = (33.0, 52.0);

impl Default for Orbit {
    fn default() -> Self {
        Self(START)
    }
}

impl Orbit {
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.0.z = (self.0.z - dx).clamp(YAW.0, YAW.1);
        self.0.y = (self.0.y + dy).clamp(PITCH.0, PITCH.1);
    }

    /// Positive moves closer, as a fraction of the distance.
    pub fn zoom(&mut self, amount: f32) {
        self.0.x = (self.0.x * (1.0 - amount)).clamp(DISTANCE.0, DISTANCE.1);
    }

    pub fn camera(&self) -> Camera {
        let (pitch, yaw) = (self.0.y.to_radians(), self.0.z.to_radians());
        let away = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        // Camera space looks along -z, so its axes come from the inverse view.
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
        Camera {
            position: TARGET + away * self.0.x,
            rotation: rotation.inverse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LENS: Lens = Lens {
        near: 1.0,
        far: 99999.0,
        fov: 50.0,
    };

    #[test]
    fn orbit_looks_at_its_target() {
        let mut o = Orbit::default();
        for (dx, dy, z) in [(0.0, 0.0, 0.0), (500.0, 500.0, 9.0), (-500.0, -500.0, -9.0)] {
            o.rotate(dx, dy);
            o.zoom(z);
            let t = o.camera().view().transform_point3(TARGET);
            assert!(t.x.abs() < 1e-2 && t.y.abs() < 1e-2 && t.z < 0.0, "{t}");
            assert!((DISTANCE.0 - 0.1..=DISTANCE.1 + 0.1).contains(&-t.z), "{t}");
        }
    }

    #[test]
    fn clipping_keeps_positions_and_cuts_at_the_plane() {
        let o = Orbit(Vec3::new(420.0, 14.0, 35.0)).camera();
        let proj = LENS.projection(ORIGINAL_ASPECT, ORIGINAL_ASPECT);
        // Below the plane is kept; the camera is above it.
        let m = clipped(
            proj,
            o.view(),
            Vec4::new(0.0, -1.0, 0.0, TARGET.y - 45.0 - 11.0),
        );
        let plain = proj * o.view();
        for (p, inside) in [
            (Vec3::new(20.0, -12.0, 5.0), true),
            (Vec3::new(20.0, -60.0, -50.0), true),
            (Vec3::new(20.0, -10.0, 5.0), false),
        ] {
            let p = p + TARGET - Vec3::Y * 45.0;
            let (a, b) = (m * p.extend(1.0), plain * p.extend(1.0));
            assert!(
                (a.x - b.x).abs() < 1e-2 && (a.y - b.y).abs() < 1e-2 && (a.w - b.w).abs() < 1e-2
            );
            assert_eq!(a.z >= 0.0, inside, "{p} {a}");
            assert!(a.z <= a.w, "{p} {a}");
        }
    }
    use sushi_formats::path::PathFrame;

    #[test]
    fn projection_maps_near_and_far_to_unit_depth() {
        let p = LENS.projection(ORIGINAL_ASPECT, ORIGINAL_ASPECT);
        let z = |z: f32| p.project_point3(Vec3::new(0.0, 0.0, z)).z;
        assert!(z(-LENS.near).abs() < 1e-5 && (z(-LENS.far) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn other_shapes_show_less() {
        let point = Vec3::new(10.0, 10.0, -100.0);
        let at = |a| LENS.projection(a, a).project_point3(point);
        let (normal, wide, narrow) = (at(ORIGINAL_ASPECT), at(16.0 / 9.0), at(1.0));
        assert!((normal.x - wide.x).abs() < 1e-6 && wide.y > normal.y);
        assert!((normal.y - narrow.y).abs() < 1e-6 && narrow.x > normal.x);
    }

    fn frame(x: f32, flag: i32) -> PathFrame {
        PathFrame {
            pos: [x, 0.0, 0.0],
            fov: -1.0,
            flag,
            quat: [0.0, 0.0, 0.0, 1.0],
        }
    }

    #[test]
    fn path_holds_the_last_frame_of_a_shot_then_cuts() {
        let frames = vec![frame(0.0, 0), frame(1.0, 1), frame(50.0, 0), frame(51.0, 0)];
        let path = FlyPath { step: 1.0, frames };
        let x = |t| Camera::on_path(&path, t).position.x;
        assert_eq!(x(1.0), 1.0);
        assert_eq!(x(1.9), 1.0);
        assert_eq!(x(2.0), 50.0);
        // Nothing overshoots because of the other shot.
        assert!((0.0..=1.0).contains(&x(0.5)) && (50.0..=51.0).contains(&x(2.5)));
        assert_eq!(x(99.0), 51.0);
    }

    #[test]
    fn path_is_exact_at_samples_and_straight_on_lines() {
        let frames = (0..5).map(|i| frame(i as f32, 0)).collect();
        let path = FlyPath { step: 1.0, frames };
        for t in [0.0, 1.0, 1.5, 2.25, 3.0] {
            assert!(
                (Camera::on_path(&path, t).position.x - t).abs() < 1e-5,
                "{t}"
            );
        }
    }
}
