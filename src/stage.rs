//! What moves in this demo: water in a cave, and the light it bends.
use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::{
    Error,
    data::Files,
    model::{Array, Fur, Geometry, Model},
    pass::Pass,
    show::{Frame, Show},
    water::{self, SIZE, STEP, Water},
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
    /// Which texture of the stage this is for, if not the picture.
    target: Option<usize>,
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

/// The stage's textures: light bent by the water, and the pool seen through it.
const CAUSTICS: usize = 0;
const BELOW: usize = 1;

const POOL: &str = "TidePoolShape";
const POOL_DEPTH: &str = "TidePoolShapeDepth";
const WATER: &str = "Water_grid_hi_polyShape_SHAPE_0";
const RAYS: &str = "Water_plane_polyShape_SHAPE_0_<1>";
const CAVE: &str = "Cave_<1>";

/// The caustics cover the pool. One number of `ProgramMCB_directionalLightcaustics` [I].
const COVER: f32 = 0.012_046_8;
/// How brightly one layer of the water's triangles lights what is below.
const RAY: [f32; 4] = [0.17, 0.18, 0.15, 1.0];

/// Heights of the basin on the water's grid.
fn basin(model: &Model) -> Option<Vec<f32>> {
    let shape = model.shapes.iter().find(|s| s.name == POOL)?;
    let g = &model.geometries[shape.geometry];
    let points: Vec<Vec3> = g.points().collect();
    let mut out = vec![f32::INFINITY; SIZE * SIZE];
    for t in g.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| points[t[i] as usize]);
        let flat = |p: Vec3| Vec2::new(p.x, p.z);
        let area = (flat(b) - flat(a)).perp_dot(flat(c) - flat(a));
        if area.abs() < 1e-9 {
            continue;
        }
        let cell = water::WIDTH / (SIZE - 1) as f32;
        let to = |v: f32| ((v + water::WIDTH * 0.5) / cell).clamp(0.0, (SIZE - 1) as f32);
        let low = a.min(b).min(c);
        let high = a.max(b).max(c);
        for z in to(low.z).floor() as usize..=to(high.z).ceil() as usize {
            for x in to(low.x).floor() as usize..=to(high.x).ceil() as usize {
                let p = Vec2::new(water::place(x), water::place(z));
                let u = (flat(b) - p).perp_dot(flat(c) - p) / area;
                let v = (flat(c) - p).perp_dot(flat(a) - p) / area;
                let w = 1.0 - u - v;
                if u.min(v).min(w) >= -1e-3 {
                    let slot = &mut out[z * SIZE + x];
                    *slot = slot.min(u * a.y + v * b.y + w * c.y);
                }
            }
        }
    }
    Some(out)
}

/// Makes a pass one of the picture below the water. That is upside down, because the
/// water's program takes its first row for the lowest.
fn below(pass: &mut Pass) {
    pass.offscreen = Some(BELOW);
    pass.state.ccw_front = !pass.state.ccw_front;
}

pub struct Stage {
    /// The mesh of the water.
    mesh: usize,
    water: Water,
    /// Steps done, and touches they knew.
    steps: u64,
    seen: f32,
}

impl Stage {
    pub fn new(_files: &Files, models: &mut [Model]) -> Result<Self, Error> {
        let model = &mut models[0];
        // The rest is for the program to fill: steam, a lens flare, guides. The low pool
        // and the plain water are for slower cards.
        let kept = [POOL, POOL_DEPTH, WATER, RAYS, CAVE];
        model
            .shapes
            .retain(|s| kept.contains(&s.name.as_str()) || s.name.starts_with("Rocks_polys"));
        if model.environment.is_none() {
            model.environment = model
                .textures
                .iter()
                .position(|t| t.name == "reflectionTexture");
        }
        let depths = basin(model).ok_or("missing pool")?;
        let find = |name: &str| {
            let found = model.shapes.iter().position(|s| s.name == name);
            found.ok_or_else(|| format!("missing {name}"))
        };
        let (surface, rays, pool) = (find(WATER)?, find(RAYS)?, find(POOL)?);
        // One grid for the whole pool.
        let mesh = model.shapes[surface].geometry;
        model.geometries[mesh] = surface_mesh(&depths);
        model.shapes[rays].geometry = mesh;
        let bent = &mut model.shapes[rays].passes[0];
        let mut flat = bent.flat(RAY, bent.state);
        flat.offscreen = Some(CAUSTICS);
        flat.state.depth = None;
        model.shapes[rays].passes = vec![flat];
        // The pool has caustics where it is seen through the water.
        let shape = &mut model.shapes[pool];
        let pass = shape.passes.last_mut().ok_or("pool without a pass")?;
        below(pass);
        for s in &mut model.shapes {
            if s.name.starts_with("Rocks_polys") {
                let mut pass = s.passes[0].clone();
                below(&mut pass);
                s.passes.push(pass);
            }
        }
        // Seen from outside, the roof would hide everything.
        if let Some(cave) = model.shapes.iter_mut().find(|s| s.name == CAVE) {
            cave.passes[0].state.cull = 1;
        }
        Ok(Self {
            mesh,
            water: Water::new(depths.iter().map(|h| *h < 0.0).collect()),
            steps: 0,
            seen: -1.0,
        })
    }

    pub fn show(&self, _: &[&Model]) -> Show {
        Show::default()
    }

    pub fn alternates(&self, _: &Model, _: usize, _: usize) -> Vec<Vec<Option<usize>>> {
        Vec::new()
    }

    /// Catches the water up with the time.
    fn run(&mut self, frame: &Frame) {
        let goal = (f64::from(frame.seconds) / f64::from(STEP)) as u64;
        // After a jump, only the last seconds matter.
        let reach = (6.0 / STEP) as u64;
        if goal > self.steps + reach {
            self.steps = goal - reach;
            self.seen = self.steps as f32 * STEP - 1e-4;
        }
        while self.steps < goal {
            self.steps += 1;
            let now = self.steps as f32 * STEP;
            for t in &frame.touches {
                if t.time > self.seen && t.time <= now {
                    self.water.disturb(t.at, t.radius, t.strength);
                }
            }
            self.seen = now;
            self.water.step();
        }
    }

    pub fn update(&mut self, _: usize, model: &mut Model, frame: &Frame) -> Vec<usize> {
        self.run(frame);
        let g = &mut model.geometries[self.mesh];
        let t = frame.seconds;
        for z in 0..SIZE {
            for x in 0..SIZE {
                let i = z * SIZE + x;
                let (px, pz) = (water::place(x), water::place(z));
                // A slow swell keeps still water alive [I].
                let a = px * 0.55 + pz * 0.21 + t * 1.3;
                let b = pz * 0.47 - px * 0.31 - t * 1.1;
                let swell = 0.035 * (a.sin() + b.sin());
                let slope = Vec3::new(
                    -0.035 * (0.55 * a.cos() - 0.31 * b.cos()),
                    0.0,
                    -0.035 * (0.21 * a.cos() + 0.47 * b.cos()),
                );
                g.coords[0].values[i * 3 + 1] = self.water.height(x, z) + swell;
                let n = (self.water.normal(x, z) + slope).normalize();
                g.normals[0].values[i * 3..i * 3 + 3].copy_from_slice(&n.to_array());
            }
        }
        vec![self.mesh]
    }

    pub fn instances(
        &self,
        _: usize,
        model: &Model,
        frame: &Frame,
        projection: Mat4,
        _: Vec3,
    ) -> Vec<Instance> {
        let world = model.rest();
        let t = frame.seconds;
        // Straight down on the pool, rows from the top as textures have them.
        let down = Mat4::from_cols(
            Vec4::new(2.0 * COVER, 0.0, 0.0, 0.0),
            Vec4::ZERO,
            Vec4::new(0.0, 2.0 * COVER, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        let flip = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        let values = Values::from([
            ("planar_proj", [COVER, -COVER, 0.5, 0.0]),
            // How deep the bent light goes before it shows.
            ("caustics_proj", [1.0, 0.0, -3.4, 1.0]),
            // The bump map drifts over the water.
            ("reg92", [t * 0.012, t * 0.007, 1.0, 0.5]),
        ]);
        let instance = |camera, target| Instance {
            world: world.clone(),
            values: values.clone(),
            registers: Vec::new(),
            camera,
            target,
        };
        vec![
            instance(Some([down, Mat4::IDENTITY]), Some(CAUSTICS)),
            instance(Some([flip * projection, frame.view]), Some(BELOW)),
            instance(None, None),
        ]
    }

    pub fn draws(
        &self,
        _: usize,
        model: &Model,
        shape: usize,
        frame: &Frame,
        instance: &Instance,
    ) -> Vec<Draw> {
        let s = &model.shapes[shape];
        let name = s.name.as_str();
        if name == POOL_DEPTH && !frame.z_correct {
            return Vec::new();
        }
        let draw = |i: usize, layer| Draw {
            passes: i..i + 1,
            layer,
            ..Default::default()
        };
        let mine = s
            .passes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.offscreen == instance.target);
        let layer = match name {
            WATER => 2,
            POOL_DEPTH => 1,
            _ => 0,
        };
        mine.map(|(i, _)| draw(i, layer)).collect()
    }

    pub fn offscreen(&self) -> Vec<Offscreen> {
        let dark = wgpu::Color {
            r: 0.02,
            g: 0.05,
            b: 0.05,
            a: 1.0,
        };
        vec![
            Offscreen {
                file: "textures/target.tga",
                size: 1024,
                depth: false,
                clear: wgpu::Color::BLACK,
            },
            Offscreen {
                file: "textures/targetb.tga",
                size: 1024,
                depth: true,
                clear: dark,
            },
        ]
    }

    pub fn effects(&self) -> Vec<Effect> {
        Vec::new()
    }

    pub fn effect_values(&self, _: usize, _: &Frame, _: Mat4, _: Vec3) -> Option<Vec<[f32; 4]>> {
        None
    }

    pub fn shell(&self, _: &Fur, _: usize, _: &Frame) -> Values {
        Values::new()
    }
}

/// The water's grid. Colors say how much it bends light, and how clear it is.
fn surface_mesh(basin: &[f32]) -> Geometry {
    let array = |width| Array {
        width,
        values: Vec::new(),
    };
    let mut g = Geometry {
        name: "water".to_owned(),
        coords: vec![array(3)],
        normals: vec![array(3)],
        colors: vec![array(4)],
        tex_coords: vec![array(2), array(2), array(4)],
        ..Default::default()
    };
    for z in 0..SIZE {
        for x in 0..SIZE {
            let (px, pz) = (water::place(x), water::place(z));
            let depth = -basin[z * SIZE + x];
            let deep = (depth / 0.8).clamp(0.0, 1.0);
            g.coords[0].values.extend([px, 0.0, pz]);
            g.normals[0].values.extend([0.0, 1.0, 0.0]);
            g.colors[0]
                .values
                .extend([deep, deep, deep, 1.0 - 0.15 * deep]);
            let uv = [px / 12.0, pz / 12.0];
            g.tex_coords[0].values.extend(uv);
            g.tex_coords[1].values.extend(uv);
            // Along the second coordinate.
            g.tex_coords[2].values.extend([0.0, 0.0, 1.0, 1.0]);
            let wet = |i: usize, k: usize| basin[k * SIZE + i] < 0.7;
            if x + 1 < SIZE
                && z + 1 < SIZE
                && [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)]
                    .iter()
                    .any(|&(i, k)| wet(i, k))
            {
                let a = (z * SIZE + x) as u32;
                let c = a + SIZE as u32;
                g.indices.extend([a, c, a + 1, a + 1, c, c + 1]);
            }
        }
    }
    g
}
