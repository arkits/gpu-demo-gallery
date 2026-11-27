//! What moves in this demo: the squid, and the sea around it.
use std::collections::HashMap;

use glam::{Mat4, Vec3};

use crate::{
    Error,
    data::Files,
    demo::{BONES, BUBBLES, FISH, FOG_DISTANCE, PLANTS, PLANTS_PER_TILE, Plant, SILT},
    model::{Array, Fur, Geometry, Model},
    pass::AMBIENT,
    show::{Frame, SKINS, Show},
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
    /// The only shape to draw, if not all.
    shape: Option<usize>,
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

const SQUID: usize = 0;
const KELP: usize = 1;
const FISHES: usize = 2;
/// The last actors are plants.
const FIRST_PLANT: usize = 3;

/// Pictures of the caustics, 30 of them a second (0x4014e0).
const CAUSTICS: usize = 32;
/// Kelp and plants are in square tiles this wide [I].
const TILE: f32 = 100.0;
/// The bottom. It lies in fog and is not drawn.
const FLOOR: f32 = -22.0;
/// Kelp closer than this to the eye would hide everything.
const CLEAR: f32 = 45.0;
/// Tiles drawn each way from the eye. Beyond them there is only fog.
const KELP_REACH: i32 = 3;
const PLANT_REACH: i32 = 2;
/// How high the water's surface is over the eye (0x5330d8).
const SURFACE: f32 = 140.0;

/// A number from 0 to 1 that always belongs to these.
fn chance(a: i32, b: i32, c: i32) -> f32 {
    let mut x = (a as u32)
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add((b as u32).wrapping_mul(0x85eb_ca77))
        .wrapping_add((c as u32).wrapping_mul(0xc2b2_ae3d));
    x = (x ^ (x >> 15)).wrapping_mul(0x2c1b_3c6d);
    x = (x ^ (x >> 12)).wrapping_mul(0x297a_2d39);
    (x ^ (x >> 15)) as f32 / u32::MAX as f32
}

/// Two crossed quads for each plant of a tile. Programs sway the upper ends.
fn plants(g: &mut Geometry, kind: i32, plant: &Plant) {
    let array = |width| Array {
        width,
        values: Vec::new(),
    };
    let (mut coords, mut uv, mut sway) = (array(3), array(2), array(4));
    g.indices.clear();
    let count = PLANTS_PER_TILE / PLANTS.len();
    for i in 0..count as i32 {
        let (x, z) = (chance(kind, i, 1) * TILE, chance(kind, i, 2) * TILE);
        let angle = chance(kind, i, 3) * std::f32::consts::PI;
        let scale = 0.7 + 0.5 * chance(kind, i, 4);
        for turn in 0..2 {
            let (sin, cos) = (angle + turn as f32 * std::f32::consts::FRAC_PI_2).sin_cos();
            let first = (coords.values.len() / 3) as u32;
            for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let side = (u - 0.5) * plant.width * scale;
                let p = Vec3::new(x + side * cos, v * plant.height * scale, z + side * sin);
                coords.values.extend(p.to_array());
                // Pictures have their first row at the top.
                uv.values.extend([u, v]);
                for [a, fx, fy, fz, _, c] in plant.sway {
                    let (sin, cos) = (fx * p.x + fy * p.y + fz * p.z + c).sin_cos();
                    sway.values.extend([a * cos, a * sin]);
                }
            }
            g.indices.extend([0, 1, 2, 2, 1, 3].map(|k: u32| first + k));
        }
    }
    g.coords = vec![coords];
    g.tex_coords = vec![uv];
    g.data = vec![sway];
}

/// Which two bones a height belongs to, and how much to each.
fn bones(g: &mut Geometry) -> f32 {
    let height = g.points().fold(1.0_f32, |h, p| h.max(p.y));
    let mut values = Vec::new();
    for p in g.points() {
        let at = (p.y / height).clamp(0.0, 1.0) * (BONES - 1) as f32;
        let low = (at as usize).min(BONES - 2);
        let part = at - low as f32;
        let register = |bone: usize| (FIRST_BONE + 4 * bone) as f32;
        values.extend([register(low), register(low + 1), 1.0 - part, part]);
    }
    g.data = vec![Array { width: 4, values }];
    height
}

/// The register where the kelp program expects matrices.
const FIRST_BONE: usize = 12;

pub struct Stage {
    kelp_heights: Vec<f32>,
    /// The node of the water's surface.
    surface: Option<usize>,
    /// Nodes of the squid, which move with it.
    body: Vec<bool>,
    /// The long tentacles: meshes that are drawn, and the curves they follow.
    ropes: Vec<(usize, Vec<Vec3>)>,
}

impl Stage {
    pub fn new(_files: &Files, models: &mut [Model]) -> Result<Self, Error> {
        let kelp = &mut models[KELP];
        let mut kelp_heights = Vec::new();
        for i in 0..kelp.shapes.len() {
            let g = kelp.shapes[i].geometry;
            kelp_heights.push(bones(&mut kelp.geometries[g]));
        }
        for (i, plant) in PLANTS.iter().enumerate() {
            let model = &mut models[FIRST_PLANT + i];
            let g = model
                .shapes
                .first()
                .ok_or("plant without a shape")?
                .geometry;
            plants(&mut model.geometries[g], i as i32, plant);
        }
        let squid = &mut models[SQUID];
        let mut ropes = Vec::new();
        for (mesh, path, side) in [
            (
                "curveShape1_CURVE",
                "curve_long_tentacle_rightShape_CURVE",
                1.0,
            ),
            (
                "curveShape2_CURVE",
                "curve_long_tentacle_leftShape_CURVE",
                -1.0,
            ),
        ] {
            let find = |name: &str| {
                let found = squid.geometries.iter().position(|g| g.name == name);
                found.ok_or_else(|| format!("missing {name}"))
            };
            let mesh = find(mesh)?;
            let path = squid.geometries[find(path)?].tube.as_ref();
            let path = path.ok_or("tentacle without a curve")?.points.iter();
            ropes.push((mesh, path.map(|p| *p * Vec3::new(side, 1.0, 1.0)).collect()));
        }
        // The paths are there for the simulation only.
        squid
            .shapes
            .retain(|s| !s.name.starts_with("curve_long_tentacle"));
        let surface = squid.find("pPlane2");
        let mut body: Vec<bool> = Vec::new();
        for n in &squid.nodes {
            let own = n.name == "Squid_body" || n.name.starts_with("curve");
            body.push(own || n.parent.is_some_and(|p| body[p]));
        }
        Ok(Self {
            kelp_heights,
            surface,
            body,
            ropes,
        })
    }

    pub fn show(&self, _: &[&Model]) -> Show {
        Show::default()
    }

    /// Every picture of the caustics for passes that use them.
    pub fn alternates(&self, model: &Model, shape: usize, pass: usize) -> Vec<Vec<Option<usize>>> {
        let textures = &model.shapes[shape].passes[pass].textures;
        let caustic = |i: usize| {
            let name = format!("caustics_{}", i + 1);
            model.textures.iter().position(|t| t.name == name)
        };
        let first = caustic(0);
        let Some(unit) = textures.iter().position(|t| t.is_some() && *t == first) else {
            return Vec::new();
        };
        (0..CAUSTICS)
            .map(|i| {
                let mut own = textures.to_vec();
                own[unit] = caustic(i).or(own[unit]);
                own
            })
            .collect()
    }

    pub fn update(&mut self, actor: usize, model: &mut Model, frame: &Frame) -> Vec<usize> {
        match actor {
            SQUID => {
                let mut changed = model.bend(f64::from(frame.seconds) * 1000.0);
                for (i, (mesh, path)) in self.ropes.iter().enumerate() {
                    model.geometries[*mesh].follow(rope(path, i as f32, frame.seconds));
                    changed.push(*mesh);
                }
                changed
            }
            _ => Vec::new(),
        }
    }

    pub fn instances(
        &self,
        actor: usize,
        model: &Model,
        frame: &Frame,
        projection: Mat4,
        eye: Vec3,
    ) -> Vec<Instance> {
        let fog = [1.0 / FOG_DISTANCE, 0.583_8, 0.705_8, 0.002];
        let eye4 = eye.extend(1.0).to_array();
        match actor {
            SQUID => vec![self.squid(model, frame, eye)],
            KELP => self.kelp(model, frame, eye, fog),
            FISHES => (0..FISH as i32)
                .map(|i| {
                    let (world, bend) = fish(i, frame);
                    let nodes = model.rest().iter().map(|m| world * *m).collect();
                    Instance {
                        world: nodes,
                        values: Values::from([("fogdist", fog)]),
                        registers: vec![(14, eye4), (16, [0.0, 0.0, 0.0, bend])],
                        camera: None,
                        shape: None,
                    }
                })
                .collect(),
            _ => {
                let (sin, cos) = frame.seconds.sin_cos();
                let (x, z) = ((eye.x / TILE).floor() as i32, (eye.z / TILE).floor() as i32);
                let tiles = -PLANT_REACH..=PLANT_REACH;
                tiles
                    .clone()
                    .flat_map(|i| tiles.clone().map(move |k| (x + i, z + k)))
                    .map(|(i, k)| {
                        // The program has no model matrix, so the eye moves instead.
                        let corner = Vec3::new(i as f32 * TILE, FLOOR, k as f32 * TILE);
                        Instance {
                            world: vec![Mat4::IDENTITY; model.nodes.len()],
                            camera: Some([projection, frame.view * Mat4::from_translation(corner)]),
                            values: Values::from([
                                ("timeX", [sin, cos, 0.0, 1.0]),
                                ("timeZ", [sin, cos, 0.0, 1.0]),
                                ("fogdist", fog),
                            ]),
                            registers: vec![(15, (eye - corner).extend(1.0).to_array())],
                            shape: None,
                        }
                    })
                    .collect()
            }
        }
    }

    fn squid(&self, model: &Model, frame: &Frame, eye: Vec3) -> Instance {
        let ms = f64::from(frame.seconds) * 1000.0;
        let blink = f64::from(frame.blink) * 1000.0;
        let times = [ms, blink];
        let mut world = model.pose(model.animations.iter().zip(times));
        for (m, own) in world.iter_mut().zip(&self.body) {
            if *own {
                *m = frame.squid * *m;
            }
        }
        if let Some(i) = self.surface {
            world[i].w_axis = Vec3::new(eye.x, eye.y + SURFACE, eye.z).extend(1.0);
        }
        Instance {
            world,
            values: Values::from([skin(1.0), glimmer(frame)]),
            registers: Vec::new(),
            camera: None,
            shape: None,
        }
    }

    fn kelp(&self, model: &Model, frame: &Frame, eye: Vec3, fog: [f32; 4]) -> Vec<Instance> {
        let (x, z) = ((eye.x / TILE).floor() as i32, (eye.z / TILE).floor() as i32);
        let tiles = -KELP_REACH..=KELP_REACH;
        let mut out = Vec::new();
        for (i, k) in tiles
            .clone()
            .flat_map(|i| tiles.clone().map(move |k| (i, k)))
        {
            let (i, k) = (x + i, z + k);
            let base = Vec3::new(
                (i as f32 + chance(i, k, 11)) * TILE,
                FLOOR,
                (k as f32 + chance(i, k, 12)) * TILE,
            );
            if (base - eye).with_y(0.0).length() < CLEAR {
                continue;
            }
            let shape = (chance(i, k, 13) * 2.999) as usize;
            let height = self.kelp_heights[shape];
            let turn = Mat4::from_rotation_y(chance(i, k, 14) * std::f32::consts::TAU);
            let mut registers = vec![(2, eye.extend(1.0).to_array())];
            let mut at = base;
            let step = height / (BONES - 1) as f32;
            for bone in 0..BONES {
                // The stalk leans more the higher it is. Once this was simulated [I].
                let up = bone as f32 / (BONES - 1) as f32;
                let phase = frame.seconds * 0.6 - up * 2.5 + chance(i, k, 15) * 6.0;
                let lean = Vec3::new(phase.sin(), 0.0, (phase * 0.83 + 1.0).sin()) * 0.35 * up;
                let axis = (Vec3::Y + lean).normalize();
                let tilt = Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::Y, axis));
                let m = Mat4::from_translation(at)
                    * tilt
                    * turn
                    * Mat4::from_translation(Vec3::Y * -(bone as f32 * step));
                for row in 0..4 {
                    registers.push((FIRST_BONE + 4 * bone + row, m.row(row).to_array()));
                }
                at += axis * step;
            }
            out.push(Instance {
                world: vec![Mat4::IDENTITY; model.nodes.len()],
                values: Values::from([
                    ("fogdist", fog),
                    // Its last number also moves normals into colors, as a half.
                    ("light", [0.0, -0.8, 0.0, 0.5]),
                ]),
                registers,
                camera: None,
                shape: Some(shape),
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
        if instance.shape.is_some_and(|s| s != shape) {
            return Vec::new();
        }
        let s = &model.shapes[shape];
        let alternate = Some((frame.seconds * 30.0) as usize % CAUSTICS);
        let draw = |passes: std::ops::Range<usize>| Draw {
            passes,
            alternate,
            ..Default::default()
        };
        if actor != SQUID || s.passes.len() != SKINS {
            return vec![draw(0..s.passes.len())];
        }
        let morph = frame.morph;
        let next = (morph.skin + 1) % SKINS;
        if s.passes[1].state.blend.is_none() {
            // These have no mask, so they change at once.
            let late = morph.blend.is_some_and(|b| b > 0.0);
            let i = if late { next } else { morph.skin };
            return vec![draw(i..i + 1)];
        }
        // Skins but the first lie over it, as far as their masks allow.
        let over = |which: usize, blend: f32| Draw {
            values: Some(Values::from([skin(blend), glimmer(frame)])),
            ..draw(which..which + 1)
        };
        let mut out = vec![draw(0..1)];
        let leaving = morph.blend.filter(|_| next == 0);
        if morph.skin > 0 {
            out.push(over(morph.skin, leaving.map_or(1.0, |b| -b)));
        }
        if let Some(b) = morph.blend.filter(|_| next > 0) {
            out.push(over(next, b));
        }
        out
    }

    pub fn offscreen(&self) -> Vec<Offscreen> {
        Vec::new()
    }

    pub fn effects(&self) -> Vec<Effect> {
        let blend = wgpu::BlendState::ALPHA_BLENDING;
        vec![
            Effect {
                source: include_str!("../shaders/silt.wgsl"),
                textures: &[],
                repeat: false,
                blend,
                vertices: 6,
                instances: SILT,
            },
            Effect {
                source: include_str!("../shaders/bubbles.wgsl"),
                textures: &["textures/bubble.dds", "textures/bubble_colors.tga"],
                repeat: false,
                blend,
                vertices: 6,
                instances: BUBBLES,
            },
        ]
    }

    pub fn effect_values(
        &self,
        effect: usize,
        frame: &Frame,
        view_projection: Mat4,
        eye: Vec3,
    ) -> Option<Vec<[f32; 4]>> {
        let mut out: Vec<_> = (0..4).map(|i| view_projection.row(i).to_array()).collect();
        let box_size = if effect == 0 { 220.0 } else { 260.0 };
        // Sizes of a unit across and up, at a distance of one.
        let scale = |row: usize| view_projection.row(row).truncate().length();
        out.push(eye.extend(frame.seconds).to_array());
        out.push([box_size, scale(1) / scale(0), 0.0, 0.0]);
        out.push([scale(0), scale(1), 0.0, 0.0]);
        Some(out)
    }

    pub fn shell(&self, _: &Fur, _: usize, _: &Frame) -> Values {
        Values::new()
    }
}

fn skin(blend: f32) -> (&'static str, [f32; 4]) {
    ("skinblend", [blend, 0.0, 0.0, 0.0])
}

/// The light's ambient color turns, which the programs add to its direction (0x410fa0).
fn glimmer(frame: &Frame) -> (&'static str, [f32; 4]) {
    let (sin, cos) = (frame.seconds * 2.5).sin_cos();
    let v = Vec3::new(sin, 1.0, cos).normalize();
    (AMBIENT, v.extend(0.0).to_array())
}

/// A long tentacle that trails behind. The program simulated ten joints of it [I].
fn rope(path: &[Vec3], which: f32, seconds: f32) -> Vec<Vec3> {
    let last = (path.len() - 1) as f32;
    let point = |(i, p): (usize, &Vec3)| {
        let far = (i as f32 / last).powi(2);
        let phase = seconds * 1.1 - i as f32 * 0.5 + which * 1.9;
        let sway = Vec3::new(
            phase.sin() * 9.0,
            (phase * 0.9 + 1.0).sin() * 7.0 + 4.0,
            0.0,
        );
        *p + sway * far
    };
    path.iter().enumerate().map(point).collect()
}

/// Where a fish is, and how its tail bends. They circle the squid [I].
fn fish(i: i32, frame: &Frame) -> (Mat4, f32) {
    let t = frame.seconds;
    let school = i % 3;
    let way = if school == 1 { -1.0 } else { 1.0 };
    let angle = way * t * (0.16 + 0.03 * school as f32)
        + school as f32 * 2.1
        + (chance(i, 1, 5) - 0.5) * 1.1;
    let radius = 34.0 + 20.0 * school as f32 + chance(i, 2, 5) * 16.0;
    let height =
        (chance(i, 3, 5) - 0.5) * 18.0 + (t * 0.7 + i as f32).sin() * 1.5 + school as f32 * 5.0;
    let at = frame.center + Vec3::new(angle.cos() * radius, height, angle.sin() * radius);
    let heading = Vec3::new(-angle.sin(), 0.0, angle.cos()) * way;
    let world = Mat4::from_translation(at)
        * Mat4::from_rotation_y(heading.x.atan2(heading.z) + std::f32::consts::PI)
        * Mat4::from_scale(Vec3::splat(1.2));
    (world, 0.12 * (t * 9.0 + i as f32 * 1.7).sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plants_are_indexed() {
        let mut g = Geometry::default();
        plants(&mut g, 0, &PLANTS[0]);
        let n = g.count() as u32;
        assert!(n > 0 && g.indices.iter().all(|&i| i < n));
        assert_eq!(g.data[0].values.len(), 4 * n as usize);
        assert_eq!(g.tex_coords[0].values.len(), 2 * n as usize);
    }

    #[test]
    fn chances_spread() {
        let mean = (0..1000).map(|i| chance(i, 2, 3)).sum::<f32>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05);
    }
}
