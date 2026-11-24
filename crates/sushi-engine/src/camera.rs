use glam::{Mat4, Quat, Vec3};
use sushi_formats::path::FlyPath;

/// Values from `Sushi2.ini`.
pub const NEAR: f32 = 1.0;
pub const FAR: f32 = 5000.0;
pub const FOV_DEGREES: f32 = 35.0;
pub const ORIGINAL_ASPECT: f32 = 4.0 / 3.0;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Camera {
    /// Samples the fly path. The engine fits a cubic spline through the 30 Hz
    /// samples (0x4093e0); this interpolates linearly and does not blend across
    /// a frame whose flag marks a cut.
    pub fn on_path(path: &FlyPath, time: f32) -> Self {
        let last = path.frames.len() - 1;
        let f = (time / path.step).clamp(0.0, last as f32);
        let i = (f as usize).min(last.saturating_sub(1));
        let (a, b) = (&path.frames[i], &path.frames[(i + 1).min(last)]);
        let t = f - i as f32;
        let t = if b.flag != 0 && t < 1.0 { 0.0 } else { t };
        let q = |q: [f32; 4]| Quat::from_array(q).normalize();
        Self {
            position: Vec3::from(a.pos).lerp(Vec3::from(b.pos), t),
            rotation: q(a.quat).slerp(q(b.quat), t),
        }
    }

    /// Rotate after translating by -position (0x4123b0, 0x430420).
    pub fn view(&self) -> Mat4 {
        Mat4::from_quat(self.rotation) * Mat4::from_translation(-self.position)
    }

    /// Right-handed, depth 0..1, horizontal field of view (0x42ba80). On screens
    /// wider than the original 4:3 the vertical extent is kept, so nothing is
    /// cropped; narrower screens keep the horizontal one.
    pub fn projection(aspect: f32) -> Mat4 {
        let half = (FOV_DEGREES.to_radians() / 2.0).tan();
        let (x, y) = if aspect > ORIGINAL_ASPECT {
            (half / ORIGINAL_ASPECT * aspect, half / ORIGINAL_ASPECT)
        } else {
            (half, half / aspect)
        };
        let depth = FAR - NEAR;
        Mat4::from_cols_array_2d(&[
            [1.0 / x, 0.0, 0.0, 0.0],
            [0.0, 1.0 / y, 0.0, 0.0],
            [0.0, 0.0, -FAR / depth, -1.0],
            [0.0, 0.0, -NEAR * FAR / depth, 0.0],
        ])
    }
}

/// Interactive camera: the `control` lines of `Sushi2.ini` (0x411fb0, 0x4139c0).
#[derive(Debug, Clone)]
pub struct Orbit {
    target: Vec3,
    distance: (f32, f32),
    pitch: (f32, f32),
    yaw: (f32, f32),
    now: Vec3, // distance, pitch, yaw
}

/// Look-at point, then distance, pitch and yaw ranges in degrees.
type Preset = ([f32; 3], (f32, f32), (f32, f32), (f32, f32));

const PRESETS: [Preset; 4] = [
    (
        [-210.0, 60.0, 25.0],
        (100.0, 225.0),
        (5.0, 40.0),
        (-90.0, 15.0),
    ),
    (
        [-150.0, 60.0, -15.0],
        (100.0, 200.0),
        (4.0, 60.0),
        (-45.0, 30.0),
    ),
    (
        [-16.0, 50.0, -50.0],
        (156.0, 300.0),
        (9.0, 35.0),
        (-12.0, 15.0),
    ),
    (
        [-115.0, 65.0, 80.0],
        (65.0, 175.0),
        (0.0, 45.0),
        (245.0, 320.0),
    ),
];

impl Orbit {
    pub fn preset(i: usize) -> Self {
        let (target, distance, pitch, yaw) = PRESETS[i % PRESETS.len()];
        let mid = |r: (f32, f32)| (r.0 + r.1) / 2.0;
        let now = Vec3::new(distance.1, mid(pitch), mid(yaw));
        Self {
            target: target.into(),
            distance,
            pitch,
            yaw,
            now,
        }
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.now.z = (self.now.z - dx).clamp(self.yaw.0, self.yaw.1);
        self.now.y = (self.now.y + dy).clamp(self.pitch.0, self.pitch.1);
    }

    /// Positive moves closer, as a fraction of the distance.
    pub fn zoom(&mut self, amount: f32) {
        let d = self.now.x * (1.0 - amount);
        self.now.x = d.clamp(self.distance.0, self.distance.1);
    }

    pub fn camera(&self) -> Camera {
        let (pitch, yaw) = (self.now.y.to_radians(), self.now.z.to_radians());
        let away = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        // Camera space looks along -z, so its axes come from the inverse view.
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
        Camera {
            position: self.target + away * self.now.x,
            rotation: rotation.inverse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_looks_at_its_target_and_respects_limits() {
        for i in 0..4 {
            let mut o = Orbit::preset(i);
            for (dx, dy, z) in [(0.0, 0.0, 0.0), (500.0, 500.0, 9.0), (-500.0, -500.0, -9.0)] {
                o.rotate(dx, dy);
                o.zoom(z);
                let c = o.camera();
                let t = c.view().transform_point3(o.target);
                assert!(
                    t.x.abs() < 1e-2 && t.y.abs() < 1e-2 && t.z < 0.0,
                    "{i}: {t}"
                );
                let d = c.position.distance(o.target);
                assert!(d >= o.distance.0 - 0.01 && d <= o.distance.1 + 0.01);
            }
        }
    }
    use sushi_formats::path::PathFrame;

    #[test]
    fn projection_maps_near_and_far_to_unit_depth() {
        let p = Camera::projection(ORIGINAL_ASPECT);
        assert!(p.project_point3(Vec3::new(0.0, 0.0, -NEAR)).z.abs() < 1e-5);
        assert!((p.project_point3(Vec3::new(0.0, 0.0, -FAR)).z - 1.0).abs() < 1e-5);
    }

    #[test]
    fn wide_screens_keep_the_vertical_extent() {
        let point = Vec3::new(0.0, 10.0, -100.0);
        let y = |a| Camera::projection(a).project_point3(point).y;
        assert!((y(ORIGINAL_ASPECT) - y(16.0 / 9.0)).abs() < 1e-6);
    }

    #[test]
    fn path_does_not_blend_across_a_cut() {
        let frame = |x, flag| PathFrame {
            pos: [x, 0.0, 0.0],
            fov: -1.0,
            flag,
            quat: [0.0, 0.0, 0.0, 1.0],
        };
        let path = |flag| FlyPath {
            step: 1.0,
            frames: vec![frame(0.0, 0), frame(10.0, flag)],
        };
        assert_eq!(Camera::on_path(&path(0), 0.5).position.x, 5.0);
        assert_eq!(Camera::on_path(&path(1), 0.5).position.x, 0.0);
        assert_eq!(Camera::on_path(&path(1), 1.0).position.x, 10.0);
    }
}
