//! What moves in this demo: bugs that walk on six legs, and their shadows.
use std::{collections::HashMap, sync::Arc};

use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};

use crate::{
    Error,
    data::Files,
    demo::DETAIL,
    model::{Fur, Model},
    pass::{LIGHT, LIGHT_COLOR, LIGHT_DIRECTION, State},
    show::{Bug, Frame, Show, Vision},
};

/// Named constants of programs.
pub type Values = HashMap<&'static str, [f32; 4]>;

/// One copy of a model.
pub struct Instance {
    pub world: Vec<Mat4>,
    pub values: Values,
    /// Constants by register, over all others.
    pub registers: Vec<(usize, [f32; 4])>,
    /// Another camera: its projection and view.
    pub camera: Option<[Mat4; 2]>,
}

/// A texture that is drawn before the picture.
pub struct Offscreen {
    /// The texture of the scene that it replaces.
    pub file: &'static str,
    pub size: u32,
    pub depth: bool,
    pub clear: wgpu::Color,
}

/// Something drawn that is not a shape.
pub struct Effect {
    pub source: &'static str,
    pub textures: &'static [&'static str],
    pub repeat: bool,
    pub blend: wgpu::BlendState,
    pub vertices: u32,
    pub instances: u32,
}

/// How a shape is drawn.
#[derive(Default)]
pub struct Draw {
    pub passes: std::ops::Range<usize>,
    /// Other textures than the scene's.
    pub alternate: Option<usize>,
    pub shells: usize,
    /// Other named constants than the instance's.
    pub values: Option<Values>,
    /// Higher layers are drawn later.
    pub layer: u32,
}

const STAGE: usize = 0;
/// The model whose shadows are drawn.
const SHADOWS: usize = 4;
const SHADOW_TEXTURE: &str = "textures/shadow.tga";

/// The shadow camera's full field of view (the scene's GL spotlight has its own cutoff).
const CONE: f32 = 0.5;

/// Lengths used by the leg solver.
const HIP: f32 = 5.99;
const THIGH: f32 = 15.57;
const SHIN: f32 = 35.0;
const REACH: f32 = 29.2;
const HEIGHT: f32 = 21.0;

/// Heights of the stage's ground.
pub(crate) struct Ground {
    heights: Vec<f32>,
    columns: usize,
    rows: usize,
    size: Vec2,
}

impl Ground {
    fn new(model: &Model) -> Option<Self> {
        let g = model.geometries.iter().find(|g| g.name == "ground")?;
        let points: Vec<Vec3> = g.points().collect();
        let (first, last) = (points.first()?, points.last()?);
        let columns = points.iter().take_while(|p| p.z == first.z).count();
        let rows = points.len().checked_div(columns)?;
        (columns > 1 && rows > 1).then(|| Self {
            heights: points.iter().map(|p| p.y).collect(),
            columns,
            rows,
            size: Vec2::new(last.x - first.x, last.z - first.z),
        })
    }

    pub(crate) fn height(&self, at: Vec2) -> f32 {
        let cells = Vec2::new((self.columns - 1) as f32, (self.rows - 1) as f32);
        let p = ((at / self.size + 0.5) * cells).clamp(Vec2::ZERO, cells - 1e-3);
        let (x, z) = (p.x as usize, p.y as usize);
        let h = |i: usize, k: usize| self.heights[(z + k) * self.columns + x + i];
        let (u, v) = (p.x.fract(), p.y.fract());
        // Cells are split along the diagonal that does not go through their first corner.
        if u + v <= 1.0 {
            h(0, 0) + (h(1, 0) - h(0, 0)) * u + (h(0, 1) - h(0, 0)) * v
        } else {
            h(1, 1) + (h(0, 1) - h(1, 1)) * (1.0 - u) + (h(1, 0) - h(1, 1)) * (1.0 - v)
        }
    }

    fn normal(&self, at: Vec2) -> Vec3 {
        let d = 20.0;
        let (x, z) = (Vec2::X * d, Vec2::Y * d);
        let slope = |a: Vec2, b: Vec2| (self.height(a) - self.height(b)) / (2.0 * d);
        Vec3::new(-slope(at + x, at - x), 1.0, -slope(at + z, at - z)).normalize()
    }
}

/// Angles of thigh and shin in the plane of a leg, so that its tip is at `target` from
/// the thigh's start. Knees point up.
fn bend(target: Vec2) -> (f32, f32) {
    let d = target
        .length()
        .clamp(SHIN - THIGH + 0.1, SHIN + THIGH - 0.1);
    let corner = |a: f32, b: f32, c: f32| {
        ((a * a + b * b - c * c) / (2.0 * a * b))
            .clamp(-1.0, 1.0)
            .acos()
    };
    let thigh = target.to_angle() + corner(THIGH, d, SHIN);
    let shin = corner(THIGH, SHIN, d) - std::f32::consts::PI;
    (thigh, shin)
}

/// A leg's nodes, and where its root and foot are in the body's space.
struct Leg {
    nodes: [usize; 3],
    root: Vec3,
    /// Around the vertical.
    yaw: f32,
    /// Alternating threes step together.
    group: usize,
}

/// The joints of one model.
struct Body {
    legs: Vec<Leg>,
}

impl Body {
    fn new(model: &Model) -> Result<Self, Error> {
        let rest = model.rest();
        let mut legs = Vec::new();
        for i in 1..=6 {
            let find = |part: &str| {
                let name = format!("Joint_{i}_{part}");
                model.find(&name).ok_or_else(|| format!("missing {name}"))
            };
            let hip = find("hip")?;
            let nodes = [hip, find("thigh")?, find("foot")?];
            let out = rest[hip].x_axis;
            legs.push(Leg {
                nodes,
                root: rest[hip].w_axis.truncate(),
                yaw: (-out.z).atan2(out.x),
                group: 0,
            });
        }
        // Front and back of one side step with the middle of the other.
        for leg in &mut legs {
            let left = leg.root.x < 0.0;
            let middle = leg.root.z.abs() < 1.0;
            leg.group = usize::from(left != middle);
        }
        // The simulation stores the alternating tripods in even and odd slots.
        legs.sort_by_key(|leg| leg.group);
        legs[1..=3].rotate_right(1);
        legs.swap(3, 4);
        Ok(Self { legs })
    }

    fn feet(&self) -> [Vec2; 6] {
        std::array::from_fn(|i| {
            let leg = &self.legs[i];
            let rest = leg.root + (Mat3::from_rotation_y(leg.yaw) * Vec3::X) * REACH;
            Vec2::new(rest.x, rest.z)
        })
    }

    /// Transforms of all nodes for a bug.
    fn pose(
        &self,
        count: usize,
        bug: &Bug,
        index: usize,
        ground: &Ground,
        seconds: f32,
    ) -> Vec<Mat4> {
        let f = index as f32;
        let normal = ground.normal(bug.at);
        let forward = Vec3::new(bug.heading.sin(), 0.0, bug.heading.cos());
        let right = normal.cross(forward).normalize();
        let forward = right.cross(normal);
        let slow = 1.0 + (f * 0.618_034).fract();
        let fast = 5.0 + 10.0 * (f * 0.414_214).fract();
        let sway = (seconds * slow + f).cos() * 6.0 + (seconds * fast + f * 2.0).cos() * 2.0;
        let base = Vec3::new(bug.at.x, ground.height(bug.at), bug.at.y);
        let turn = Mat3::from_cols(right, normal, forward);
        let body = Mat4::from_translation(base + normal * (HEIGHT + sway)) * Mat4::from_mat3(turn);
        let mut out = vec![body; count];
        let inverse = body.inverse();
        for (i, leg) in self.legs.iter().enumerate() {
            let (under, lift) = bug.foot(i);
            let foot = Vec3::new(under.x, ground.height(under) + lift, under.y);
            let local = inverse.transform_point3(foot) - leg.root;
            let yaw = (-local.z).atan2(local.x);
            let flat = Vec2::new(local.x, local.z).length();
            let (thigh, shin) = bend(Vec2::new(flat - HIP, local.y));
            let hip = body * Mat4::from_translation(leg.root) * Mat4::from_rotation_y(yaw);
            let knee = hip * Mat4::from_translation(Vec3::X * HIP) * Mat4::from_rotation_z(thigh);
            let tip = knee * Mat4::from_translation(Vec3::X * THIGH) * Mat4::from_rotation_z(shin);
            for (n, m) in leg.nodes.into_iter().zip([hip, knee, tip]) {
                out[n] = m;
            }
        }
        out
    }
}

pub struct Stage {
    ground: Arc<Ground>,
    bodies: Vec<Body>,
    /// The node that holds the light's texture matrix.
    lamp: Option<usize>,
    fan: Option<usize>,
    occluders: Vec<Option<Vec2>>,
}

impl Stage {
    pub fn new(_files: &Files, models: &mut [Model]) -> Result<Self, Error> {
        let original = &models[STAGE];
        let ground = Arc::new(Ground::new(original).ok_or("stage without ground")?);
        let rest = original.rest();
        let occluders = original
            .shapes
            .iter()
            .map(|shape| {
                if original.geometries[shape.geometry].name == "ground" {
                    return None;
                }
                let world = rest[shape.passes[0].node];
                let mut min = Vec3::splat(f32::INFINITY);
                let mut max = Vec3::splat(f32::NEG_INFINITY);
                for p in original.geometries[shape.geometry].points() {
                    let p = world.transform_point3(p);
                    min = min.min(p);
                    max = max.max(p);
                }
                Some(Vec2::new((min.x + max.x) * 0.5, (min.z + max.z) * 0.5))
            })
            .collect();
        let bodies = models[1..]
            .iter()
            .map(Body::new)
            .collect::<Result<_, _>>()?;
        for (i, model) in models.iter_mut().enumerate() {
            for shape in &mut model.shapes {
                let base = shape.passes.first().ok_or("shape without a pass")?;
                // Depth Vision counts how often each pixel is drawn (`DepthShader`).
                let flat = base.flat(
                    [0.1, 0.2, 0.3, 0.0],
                    State {
                        blend: Some((1, 1)),
                        depth: None,
                        ..base.state
                    },
                );
                let mut dark = base.flat(
                    [0.2, 0.2, 0.2, 1.0],
                    State {
                        blend: None,
                        depth: None,
                        cull: 3,
                        ..base.state
                    },
                );
                dark.offscreen = Some(0);
                if i == SHADOWS {
                    shape.passes = vec![dark];
                } else {
                    // 0x408820 switches the constants when the bug groups start drawing.
                    let colors = if i == STAGE {
                        [[0.1; 4], [1.0; 4]]
                    } else {
                        [[0.4, 1.0, 0.4, 0.0], [1.0, 0.0, 0.0, 0.0]]
                    };
                    let night = base.night(colors);
                    shape.passes.extend([flat, night]);
                    if i == STAGE {
                        shape.passes.push(dark);
                    }
                }
            }
        }
        Ok(Self {
            ground,
            bodies,
            lamp: models[STAGE].find("shadowTransform"),
            fan: models[STAGE].find("Fan_blade"),
            occluders,
        })
    }

    pub fn show(&self, _: &[&Model]) -> Show {
        Show::new(self.ground.clone(), self.bodies[0].feet())
    }

    pub fn alternates(&self, _: &Model, _: usize, _: usize) -> Vec<Vec<Option<usize>>> {
        Vec::new()
    }

    pub fn update(&mut self, _: usize, _: &mut Model, _: &Frame) -> Vec<usize> {
        Vec::new()
    }

    pub fn instances(
        &self,
        actor: usize,
        model: &Model,
        frame: &Frame,
        _: Mat4,
        eye: Vec3,
    ) -> Vec<Instance> {
        let direction = (frame.target - frame.lamp).normalize_or(-Vec3::Y);
        let camera = [
            glam::camera::rh::proj::opengl::perspective(CONE, 1.0, 0.1, 10000.0),
            glam::camera::rh::view::look_at_mat4(frame.lamp, frame.target, Vec3::Z),
        ];
        let light = Values::from([
            (LIGHT, frame.lamp.extend(1.0).to_array()),
            (LIGHT_DIRECTION, (-direction).extend(0.0).to_array()),
            (LIGHT_COLOR, [1.0, 1.0, 1.0, 0.3]),
        ]);
        // From the light's picture to coordinates, rows from the top.
        let half = Mat4::from_cols(
            Vec4::new(0.5, 0.0, 0.0, 0.0),
            Vec4::new(0.0, -0.5, 0.0, 0.0),
            Vec4::ZERO,
            Vec4::new(0.5, 0.5, 0.0, 1.0),
        );
        let cookie = half * camera[0] * camera[1];
        if actor == STAGE {
            let mut world = model.rest();
            if let Some(i) = self.lamp {
                world[i] = cookie;
            }
            if let Some(i) = self.fan {
                model.swing(&mut world, i, 0.7 * (frame.seconds * 0.31).sin());
            }
            let mut instances = vec![Instance {
                world: world.clone(),
                values: light.clone(),
                registers: Vec::new(),
                camera: None,
            }];
            if frame.vision == Vision::Normal {
                instances.push(Instance {
                    world,
                    values: light,
                    registers: Vec::new(),
                    camera: Some(camera),
                });
            }
            return instances;
        }
        if actor == SHADOWS && frame.vision != Vision::Normal {
            return Vec::new();
        }
        let body = &self.bodies[actor - 1];
        let mut out = Vec::new();
        for (i, bug) in frame.bugs.iter().enumerate() {
            let at = Vec3::new(bug.at.x, self.ground.height(bug.at) + HEIGHT, bug.at.y);
            let detail = 1 + DETAIL.iter().filter(|d| at.distance(eye) > **d).count();
            if actor != SHADOWS && actor != detail {
                continue;
            }
            if actor == SHADOWS
                && Vec2::new(frame.target.x, frame.target.z).distance_squared(bug.at) > 200_000.0
            {
                continue;
            }
            if actor != SHADOWS && at.distance_squared(eye) > 3500.0_f32.powi(2) {
                continue;
            }
            out.push(Instance {
                world: body.pose(model.nodes.len(), bug, i, &self.ground, frame.seconds),
                values: light.clone(),
                registers: Vec::new(),
                camera: (actor == SHADOWS).then_some(camera),
            });
        }
        out
    }

    pub fn draws(
        &self,
        actor: usize,
        model: &Model,
        shape: usize,
        frame: &Frame,
        instance: &Instance,
    ) -> Vec<Draw> {
        let last = model.shapes[shape].passes.len() - 1;
        let passes = if actor == SHADOWS {
            0..1
        } else if actor == STAGE && instance.camera.is_some() {
            let target = Vec2::new(frame.target.x, frame.target.z);
            let close =
                self.occluders[shape].is_some_and(|p| p.distance_squared(target) < 200_000.0);
            if !close && !model.shapes[shape].name.starts_with("Junk_Shape8") {
                return Vec::new();
            }
            last..last + 1
        } else {
            let night = last - usize::from(actor == STAGE);
            match frame.vision {
                Vision::Normal => 0..night - 1,
                Vision::Night => night..night + 1,
                Vision::Depth => night - 1..night,
            }
        };
        vec![Draw {
            passes,
            ..Default::default()
        }]
    }

    pub fn offscreen(&self) -> Vec<Offscreen> {
        vec![Offscreen {
            file: SHADOW_TEXTURE,
            size: 512,
            depth: false,
            clear: wgpu::Color::WHITE,
        }]
    }

    pub fn effects(&self) -> Vec<Effect> {
        let blend = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let blend = wgpu::BlendState {
            color: blend,
            alpha: blend,
        };
        vec![
            Effect {
                source: include_str!("../shaders/dust.wgsl"),
                textures: &["textures/dust.dds"],
                repeat: false,
                blend,
                vertices: 6,
                instances: 100,
            },
            Effect {
                source: include_str!("../shaders/flare.wgsl"),
                textures: &["textures/lens_flare.tga"],
                repeat: false,
                blend,
                vertices: 6,
                instances: 2,
            },
        ]
    }

    pub fn effect_values(&self, index: usize, frame: &Frame, clip: Mat4) -> Option<Vec<[f32; 4]>> {
        if frame.vision != Vision::Normal || (index == 1 && frame.light_control) {
            return None;
        }
        let camera = frame.view.inverse();
        let mut values: Vec<_> = (0..4).map(|i| clip.row(i).to_array()).collect();
        values.extend([
            frame.lamp.extend(frame.seconds).to_array(),
            frame.target.extend(1.0).to_array(),
            camera.x_axis.to_array(),
            camera.y_axis.to_array(),
        ]);
        Some(values)
    }

    pub fn shell(&self, _: &Fur, _: usize, _: &Frame) -> Values {
        Values::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legs_reach_their_targets() {
        for target in [
            Vec2::new(29.0, -21.0),
            Vec2::new(20.0, -25.0),
            Vec2::new(38.0, -10.0),
        ] {
            let (thigh, shin) = bend(target);
            let knee = Vec2::from_angle(thigh) * THIGH;
            let tip = knee + Vec2::from_angle(thigh + shin) * SHIN;
            assert!(tip.distance(target) < 0.01, "{tip}");
            // The knee is over the line to the foot.
            assert!(target.perp_dot(knee) > 0.0);
        }
    }
}
