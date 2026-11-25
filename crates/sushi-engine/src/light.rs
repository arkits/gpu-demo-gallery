//! Scene lights and how objects choose them (0x414c40, 0x415140, 0x4170c0).
use glam::{Vec3, Vec4};
use sushi_formats::reader::{count_at, f32_at, u32_at};
use sushi_formats::scene;

#[derive(Debug, Clone)]
pub struct Light {
    spot: bool,
    /// Full cone angle in degrees.
    cone: f32,
    range: f32,
    position: Vec3,
    direction: Vec3,
    planes: [Vec4; 6],
    pub fast_rt: bool,
    pub object_ambient: bool,
    pub casts_shadows: bool,
    pub ambient_color: Vec3,
    pub matrix: [[f32; 4]; 4],
    pub texture: Option<usize>,
    color: Vec3,
    keys: Option<(f32, Vec<Vec3>)>,
}

impl Light {
    pub fn new(l: &scene::Light) -> Self {
        let r = &l.raw;
        let v3 = |o: usize| Vec3::new(f32_at(r, o), f32_at(r, o + 4), f32_at(r, o + 8));
        let v4 = |o: usize| v3(o).extend(f32_at(r, o + 12));
        let keys = (r[0x46] == 1 && count_at(r, 0x4b) > 0).then(|| {
            let k = l.table_4b.iter();
            let k = k.map(|b| Vec3::new(f32_at(b, 0), f32_at(b, 4), f32_at(b, 8)));
            (f32_at(r, 0x47), k.collect())
        });
        Self {
            spot: u32_at(r, 0x20) == 1,
            cone: f32_at(r, 0x53),
            range: f32_at(r, 0x32),
            position: v3(0x5f),
            direction: v3(0x8f),
            planes: std::array::from_fn(|i| v4(0x9f + i * 16)),
            fast_rt: l.is_fast_rt(),
            object_ambient: l.is_object_ambient(),
            casts_shadows: r[0x28] & 2 != 0,
            ambient_color: Vec3::from(l.color()) * f32_at(r, 0x2e),
            matrix: l.fast_rt_matrix(),
            texture: l.texture(),
            color: Vec3::from(l.color()),
            keys,
        }
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Animated color at `time` (0x427210).
    pub fn color(&self, time: f32) -> Vec3 {
        let Some((step, keys)) = &self.keys else {
            return self.color;
        };
        let f = time.max(0.0).rem_euclid(step * keys.len() as f32) / step;
        let i = (f as usize).min(keys.len() - 1);
        keys[i].lerp(keys[(i + 1).min(keys.len() - 1)], f - i as f32)
    }

    /// `FastRTColor`: rgb and luminance.
    pub fn fast_rt_color(&self, time: f32) -> [f32; 4] {
        let c = self.color(time);
        [c.x, c.y, c.z, c.dot(Vec3::new(0.3, 0.59, 0.11))]
    }

    /// How strongly the light reaches a sphere: 0 when out of range or cone.
    pub fn reach(&self, center: Vec3, radius: f32) -> f32 {
        let to = center - self.position;
        let distance = to.length();
        if (distance - radius).max(0.0) >= self.range {
            return 0.0;
        }
        if !self.spot || distance <= radius {
            return 1.0;
        }
        let angle = (to / distance)
            .dot(self.direction)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees();
        let half = self.cone * 0.5;
        let spread = (radius / distance).clamp(0.0, 1.0).asin().to_degrees();
        if half + spread < angle {
            return 0.0;
        }
        1.0 - ((angle - spread) / half).clamp(0.0, 1.0)
    }

    /// Whether the light's volume touches a sphere and box.
    pub fn encloses(&self, center: Vec3, radius: f32, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|p| {
            let n = p.truncate();
            let far = Vec3::select(n.cmpge(Vec3::ZERO), max, min);
            n.dot(center) + p.w >= -radius && n.dot(far) + p.w >= 0.0
        })
    }
}
