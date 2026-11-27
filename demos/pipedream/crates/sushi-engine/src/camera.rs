use glam::{Mat4, Quat, Vec3, Vec4};
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
