use glam::{Mat4, Quat, Vec3, Vec4};
use sushi_formats::path::FlyPath;

pub const ORIGINAL_ASPECT: f32 = 4.0 / 3.0;

/// Values from `Sushi2.ini`.
#[derive(Debug, Clone, Copy)]
pub struct Lens {
    pub near: f32,
    pub far: f32,
    /// Width of the film in millimeters, and largest blur in pixels of 1024.
    pub film: f32,
    pub blur: f32,
}

impl Lens {
    /// Right-handed, depth 0..1, horizontal field of view (0x42ba80), for a viewport of
    /// `aspect` in a window of `window`. Wide windows show more at the sides, as the
    /// original does; narrow ones keep the 4:3 picture's width.
    pub fn projection(&self, fov: f32, aspect: f32, window: f32) -> Mat4 {
        let half = (fov.to_radians() / 2.0).tan();
        let x = half * (window / ORIGINAL_ASPECT).max(1.0);
        let y = x / aspect;
        let depth = self.far - self.near;
        Mat4::from_cols_array_2d(&[
            [1.0 / x, 0.0, 0.0, 0.0],
            [0.0, 1.0 / y, 0.0, 0.0],
            [0.0, 0.0, -self.far / depth, -1.0],
            [0.0, 0.0, -self.near * self.far / depth, 0.0],
        ])
    }

    /// `DOFParams` (0x435600, case 0x62): where the picture begins to blur before and
    /// behind the distance in focus, for a blur of `blur` pixels of 1024.
    pub fn focus(&self, camera: &Camera) -> [f32; 4] {
        let [distance, stop, length] = camera.focus;
        if stop == 0.0 {
            return [-1.0e10, 0.0, 1.0e10, 1.0];
        }
        // In centimeters.
        let a = length / 10.0;
        let circle = self.film / 10.0 / 1024.0 * self.blur * 2.0;
        let h = a * a / (circle * stop);
        let beyond = distance - a;
        let near = h * distance / (beyond + h);
        let h9 = a * a / (circle * stop * 0.9);
        let mut far = (h9 * distance / (h9 - beyond) - distance) / 0.9 + distance;
        let mut most = 1.0;
        if h <= beyond {
            let largest = a * a / (beyond * stop);
            let h = a * a / (largest * stop * 0.9);
            far = (h * distance / (h - beyond) - distance) * (circle / (largest * 0.9)) + distance;
            most = largest / circle;
        }
        [near, distance, far, most]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub rotation: Quat,
    /// Horizontal, in degrees.
    pub fov: f32,
    /// Distance in focus, f-stop, focal length.
    pub focus: [f32; 3],
}

impl Camera {
    /// Samples the fly path with a spline through the 30 Hz samples, as the engine
    /// does (0x4093e0, 0x40a1c0). The sample after a flagged one is at the same moment,
    /// in the next shot.
    pub fn on_path(path: &FlyPath, time: f32, fov: f32) -> Self {
        let frames = &path.frames;
        let last = frames.len() - 1;
        let f = (time.max(0.0) / path.step).min(path.length() / path.step);
        // Number the samples by moments.
        let i = path.sample(f as usize);
        let t = if i == last || frames[i].flag != 0 {
            0.0
        } else {
            f - f.floor()
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
        let all = [before, i, next, after];
        let pos = all.map(|k| Vec3::from(frames[k].pos));
        let here = Quat::from_array(frames[i].quat);
        let quat = all.map(|k| {
            let q = Quat::from_array(frames[k].quat);
            Vec4::from(if q.dot(here) < 0.0 { -q } else { q })
        });
        let mix = |v: &dyn Fn(usize) -> f32| v(i) * (1.0 - t) + v(next) * t;
        Self {
            position: spline(pos, t),
            rotation: Quat::from_vec4(spline(quat, t)).normalize(),
            fov: match frames[i].fov > 0.0 {
                true => mix(&|k| frames[k].fov),
                false => fov,
            },
            focus: std::array::from_fn(|c| mix(&|k| frames[k].focus[c])),
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

/// A `control` line of `Sushi2.ini` (parsed at 0x426cdb): a camera that turns around a
/// point within limits. Ranges are lowest, highest and where to start, which is the middle
/// unless given (0x413850..0x413890).
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub target: Vec3,
    pub distance: [f32; 3],
    /// Degrees.
    pub pitch: [f32; 3],
    pub yaw: [f32; 3],
    pub fov: Option<f32>,
    /// Focal distance, f-stop and focal length.
    pub focus: [Option<f32>; 3],
    /// The script (chapter) it belongs to; any when `None` (`Seq` is 1 for the first).
    pub sequence: Option<usize>,
}

impl Control {
    /// The words after `control =`, such as `LAt(0,700,0)` or `Dist(325, 325)`.
    pub fn parse(words: &[String]) -> Option<Self> {
        let mut c = Self {
            target: Vec3::ZERO,
            distance: [0.0; 3],
            pitch: [0.0; 3],
            yaw: [0.0; 3],
            fov: None,
            focus: [None; 3],
            sequence: None,
        };
        let range = |v: &[f32]| -> Option<[f32; 3]> {
            let (low, high) = (*v.first()?, *v.get(1)?);
            Some([
                low,
                high,
                v.get(2).copied().unwrap_or((high - low) / 2.0 + low),
            ])
        };
        for w in words {
            let (name, rest) = w.split_once('(')?;
            let v: Vec<f32> = rest
                .trim_end_matches(')')
                .split(',')
                .filter_map(|n| n.trim().parse().ok())
                .collect();
            match name.trim().to_ascii_lowercase().as_str() {
                "lookat" | "lat" => c.target = Vec3::new(*v.first()?, *v.get(1)?, *v.get(2)?),
                "dist" | "d" => c.distance = range(&v)?,
                "pitch" | "p" => c.pitch = range(&v)?,
                "yaw" | "y" => c.yaw = range(&v)?,
                "fov" => c.fov = v.first().copied(),
                "focaldistance" | "fdist" | "fdst" => c.focus[0] = v.first().copied(),
                "fstop" => c.focus[1] = v.first().copied(),
                "focallength" | "flength" | "fln" => c.focus[2] = v.first().copied(),
                "sequence" | "seq" => c.sequence = (*v.first()? as usize).checked_sub(1),
                "frame" | "fr" => {}
                _ => return None,
            }
        }
        Some(c)
    }

    /// Whether it is offered in chapter `chapter` (0x416ea0).
    pub fn in_chapter(&self, chapter: usize) -> bool {
        self.sequence.is_none_or(|s| s == chapter)
    }
}

/// Interactive camera. With a [`Control`] it is the original's: around the control's point
/// within its limits. Otherwise it turns the show's own camera around what it looks at,
/// which the original does not do.
#[derive(Debug, Clone, Default)]
pub struct Orbit {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    control: Option<Control>,
}

impl Orbit {
    pub fn with(control: Control) -> Self {
        Self {
            yaw: control.yaw[2],
            pitch: control.pitch[2],
            zoom: control.distance[2],
            control: Some(control),
        }
    }

    pub fn control(&self) -> Option<&Control> {
        self.control.as_ref()
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        if let Some(c) = &self.control {
            // A full turn of limits wraps around.
            self.yaw -= dx;
            if c.yaw[1] - c.yaw[0] >= 360.0 {
                self.yaw = (self.yaw - c.yaw[0]).rem_euclid(360.0) + c.yaw[0];
            } else {
                self.yaw = self.yaw.clamp(c.yaw[0], c.yaw[1]);
            }
            self.pitch = (self.pitch + dy).clamp(c.pitch[0], c.pitch[1]);
            return;
        }
        self.yaw -= dx;
        self.pitch = (self.pitch + dy).clamp(-80.0, 80.0);
    }

    /// Positive moves closer, as a fraction of the distance.
    pub fn zoom(&mut self, amount: f32) {
        if let Some(c) = &self.control {
            self.zoom = (self.zoom * (1.0 - amount)).clamp(c.distance[0], c.distance[1]);
            return;
        }
        self.zoom = (self.zoom + amount).clamp(-2.0, 0.9);
    }

    pub fn camera(&self, show: &Camera) -> Camera {
        if let Some(c) = &self.control {
            // 0x415520 and 0x4138b0: the camera's back axis, scaled by the distance.
            let (pitch, yaw) = (self.pitch.to_radians(), self.yaw.to_radians());
            let turn = Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(-pitch);
            let focus = [0, 1, 2].map(|i| c.focus[i].unwrap_or(show.focus[i]));
            return Camera {
                position: c.target + turn * Vec3::Z * self.zoom,
                rotation: turn.inverse(),
                fov: c.fov.unwrap_or(show.fov),
                focus,
            };
        }
        let back = show.rotation.inverse();
        let distance = if show.focus[0] > 1.0 {
            show.focus[0]
        } else {
            150.0
        }
        .min(2000.0);
        let target = show.position + back * Vec3::new(0.0, 0.0, -distance);
        let turn = Quat::from_rotation_y(self.yaw.to_radians());
        let tilt = Quat::from_axis_angle(back * Vec3::X, -self.pitch.to_radians());
        let away = turn * (tilt * (show.position - target)) * (1.0 - self.zoom);
        Camera {
            position: target + away,
            rotation: (turn * tilt * back).inverse().normalize(),
            ..*show
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sushi_formats::path::PathFrame;

    const LENS: Lens = Lens {
        near: 1.0,
        far: 99999.0,
        film: 35.0,
        blur: 8.0,
    };

    fn still() -> Camera {
        Camera {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            fov: 50.0,
            focus: [100.0, 5.6, 35.0],
        }
    }

    #[test]
    fn orbit_keeps_its_target() {
        let mut o = Orbit::default();
        assert!((o.camera(&still()).position).length() < 1e-4);
        o.rotate(50.0, 20.0);
        let t = o
            .camera(&still())
            .view()
            .transform_point3(Vec3::new(0.0, 0.0, -100.0));
        assert!(
            t.x.abs() < 1e-2 && t.y.abs() < 1e-2 && (t.z + 100.0).abs() < 1e-2,
            "{t}"
        );
    }

    #[test]
    fn projection_maps_near_and_far_to_unit_depth() {
        let p = LENS.projection(50.0, ORIGINAL_ASPECT, ORIGINAL_ASPECT);
        let z = |z: f32| p.project_point3(Vec3::new(0.0, 0.0, z)).z;
        assert!(z(-LENS.near).abs() < 1e-5 && (z(-LENS.far) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn blur_begins_around_the_focus() {
        let [near, focus, far, most] = LENS.focus(&still());
        assert!(
            near < focus && focus < far && most > 0.0 && most <= 1.0,
            "{near} {focus} {far} {most}"
        );
    }

    fn frame(x: f32, flag: i32) -> PathFrame {
        PathFrame {
            pos: [x, 0.0, 0.0],
            fov: -1.0,
            flag,
            focus: [0.0; 3],
            quat: [0.0, 0.0, 0.0, 1.0],
        }
    }

    #[test]
    fn cuts_take_no_time() {
        let frames = vec![
            frame(0.0, 0),
            frame(1.0, 0),
            frame(2.0, 1),
            frame(50.0, 0),
            frame(51.0, 0),
        ];
        let path = FlyPath { step: 1.0, frames };
        let x = |t| Camera::on_path(&path, t, 50.0).position.x;
        assert_eq!(path.length(), 4.0);
        assert_eq!(x(1.0), 1.0);
        assert!((1.0..2.0).contains(&x(1.5)));
        assert_eq!(x(2.0), 50.0);
        assert!((50.0..=51.0).contains(&x(2.5)));
        assert_eq!(x(99.0), 51.0);
    }
}
