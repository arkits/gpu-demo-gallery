//! Particle systems, simulated in fixed steps so that every moment can be reached again.
use std::rc::Rc;

use glam::{EulerRot, Mat4, Quat, Vec3};
use sushi_formats::particles::{Node, System as Source};

use crate::object::Clip;

const STEP: f32 = 1.0 / 60.0;

/// Repeatable random numbers.
#[derive(Debug, Clone)]
struct Random(u32);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1 << 24) as f32
    }
    /// Within -1 to 1.
    fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
    fn direction(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.signed(), self.signed(), self.signed());
            if (1e-4..=1.0).contains(&v.length_squared()) {
                return v;
            }
        }
    }
}

/// A value of the text: fixed, random around a middle, or a curve.
#[derive(Debug, Clone)]
enum Param {
    Fixed([f32; 4]),
    /// Middle and full width.
    Random([f32; 4], [f32; 4]),
    /// Row of the curves, from when to when, and whether it adds.
    Curve {
        row: usize,
        start: f32,
        end: f32,
        add: bool,
    },
    /// Keys of its own.
    Keys(Vec<(f32, f32)>),
}

fn four(v: &[f32]) -> [f32; 4] {
    std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0))
}

impl Param {
    fn read(n: &Node) -> Option<Self> {
        let v = n.numbers();
        match n
            .words
            .iter()
            .find(|w| matches!(w.as_str(), "Fixed" | "Random" | "KeyFrame"))?
            .as_str()
        {
            "Fixed" => Some(Param::Fixed(four(&v))),
            "Random" => Some(Param::Random(four(&v), four(v.get(4..)?))),
            _ => {
                if let Some(c) = n.child("KeyFrameChannel") {
                    let key = |k: &Node| Some((*k.numbers().first()?, *k.numbers().get(1)?));
                    return Some(Param::Keys(c.children.iter().filter_map(key).collect()));
                }
                let [row, start, end] = v.as_slice() else {
                    return None;
                };
                let add = n.words.iter().any(|w| w == "Additive");
                Some(Param::Curve {
                    row: *row as usize,
                    start: *start,
                    end: *end,
                    add,
                })
            }
        }
    }

    fn at(&self, time: f32, curves: &[Vec<[f32; 4]>], random: &mut Random) -> [f32; 4] {
        match self {
            Param::Fixed(v) => *v,
            Param::Random(m, w) => std::array::from_fn(|i| m[i] + (random.next() - 0.5) * w[i]),
            Param::Curve {
                row, start, end, ..
            } => {
                let Some(c) = curves.get(*row).filter(|c| !c.is_empty()) else {
                    return [0.0; 4];
                };
                let f = ((time - start) / (end - start).max(1e-6)).clamp(0.0, 1.0)
                    * (c.len() - 1) as f32;
                let i = (f as usize).min(c.len() - 1);
                let (a, b, f) = (&c[i], &c[(i + 1).min(c.len() - 1)], f.fract());
                std::array::from_fn(|k| a[k] * (1.0 - f) + b[k] * f)
            }
            Param::Keys(k) => {
                let i = k.iter().rposition(|k| k.0 <= time).unwrap_or(0);
                let (Some(a), b) = (k.get(i), k.get(i + 1)) else {
                    return [0.0; 4];
                };
                let v = match b {
                    Some(b) if b.0 > a.0 => {
                        a.1 + (b.1 - a.1) * ((time - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
                    }
                    _ => a.1,
                };
                [v; 4]
            }
        }
    }
}

/// Where emitters and actions are.
struct Transform {
    local: Mat4,
    clip: Option<(Rc<Clip>, f32)>,
}

impl Transform {
    fn read(n: &Node, clip: &dyn Fn(&str) -> Option<Rc<Clip>>) -> Self {
        let v = |name, or| n.vector(name).map_or(or, Vec3::from);
        let r = v("Rotation", Vec3::ZERO);
        let q = Quat::from_euler(
            EulerRot::ZYX,
            r.z.to_radians(),
            r.y.to_radians(),
            r.x.to_radians(),
        );
        let local = Mat4::from_scale_rotation_translation(
            v("Scale", Vec3::ONE),
            q,
            v("Translation", Vec3::ZERO),
        );
        let clip = n.child("Animation").and_then(|a| {
            let file = a
                .words
                .iter()
                .find(|w| w.to_ascii_lowercase().ends_with(".sa2"))?;
            Some((clip(file)?, a.numbers().last().copied().unwrap_or(0.0)))
        });
        Self { local, clip }
    }
    fn at(&self, time: f32) -> Mat4 {
        match &self.clip {
            Some((c, offset)) => c.bone(0, time.max(0.0) + offset) * self.local,
            None => self.local,
        }
    }
}

enum Shape {
    Ellipsoid(Vec3, Vec3),
    Box(Vec3, Vec3),
    /// Center, axis, length, radius.
    Cylinder(Vec3, Vec3, f32, f32),
    Torus(Vec3, [f32; 3]),
    Plane(Vec3, Vec3),
}

impl Shape {
    fn read(n: &Node) -> Option<Self> {
        let center = n.vector("Center").map_or(Vec3::ZERO, Vec3::from);
        let v = |name| n.vector(name).map_or(Vec3::Y, Vec3::from);
        Some(match n.words.get(1)?.as_str() {
            "Ellipsoid" => Shape::Ellipsoid(center, v("Dim") * 0.5),
            "Box" => Shape::Box(center, v("Dim") * 0.5),
            "Cylinder" => {
                Shape::Cylinder(center, v("Axis"), n.number("Length")?, n.number("Radius")?)
            }
            "Torus" => Shape::Torus(
                center,
                ["Radius1", "Radius2", "Radius3"].map(|r| n.number(r).unwrap_or(0.0)),
            ),
            "Plane" => Shape::Plane(center, v("Normal")),
            _ => return None,
        })
    }
    fn point(&self, r: &mut Random) -> Vec3 {
        match self {
            Shape::Ellipsoid(c, d) => *c + r.direction() * *d,
            Shape::Box(c, d) => *c + Vec3::new(r.signed(), r.signed(), r.signed()) * *d,
            Shape::Cylinder(c, axis, length, radius) => {
                let axis = axis.normalize_or(Vec3::Y);
                let (x, y) = axis.any_orthonormal_pair();
                let (a, s) = (r.next() * std::f32::consts::TAU, r.next().sqrt() * radius);
                *c + (x * a.cos() + y * a.sin()) * s + axis * (r.signed() * 0.5 * length)
            }
            Shape::Torus(c, [ring, tube, _]) => {
                let (a, b) = (
                    r.next() * std::f32::consts::TAU,
                    r.next() * std::f32::consts::TAU,
                );
                let s = r.next().sqrt() * tube;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                *c + out * (ring + s * b.cos()) + Vec3::Y * (s * b.sin())
            }
            Shape::Plane(c, _) => *c,
        }
    }
}

struct Emitter {
    rate: Param,
    /// Share of the emitter's own speed that particles get.
    inherit: [f32; 4],
    transform: Transform,
    shape: Shape,
    velocity: Param,
    rotation: Param,
    color: Param,
    life: Param,
    size: Param,
    tex_frame: Param,
}

enum Action {
    Attributes(Vec<(&'static str, Param)>),
    Accelerate(Param),
    Damp(f32, f32),
    Wander(Param),
    Kill(Transform, Shape),
    Advect(Transform),
}

#[derive(Debug, Clone)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub life: f32,
    pub color: [f32; 4],
    pub size: f32,
    pub tex_frame: f32,
    first_frame: f32,
    pub rotation: [f32; 4],
    pub id: u32,
}

/// One system of a file.
pub struct System {
    /// In seconds of the show.
    pub start: f32,
    pub end: f32,
    pub enabled: bool,
    pub sort: bool,
    pub tile: [f32; 2],
    most: usize,
    warm_up: f32,
    emitters: Vec<Emitter>,
    actions: Vec<Action>,
    curves: Vec<Vec<[f32; 4]>>,
    /// Shader parameters by name.
    pub params: Vec<(String, f32)>,
    moving: Vec<(String, Param)>,
    seed: u32,
    random: Random,
    owed: Vec<f32>,
    pub particles: Vec<Particle>,
    made: u32,
    /// Steps done since the start.
    steps: i64,
    ready: bool,
}

impl System {
    pub fn new(
        s: &Source,
        start: f32,
        end: f32,
        seed: u32,
        clip: &dyn Fn(&str) -> Option<Rc<Clip>>,
    ) -> Result<Self, String> {
        let t = &s.text;
        let mut emitters = Vec::new();
        for e in t.all("Emitter").filter(|e| e.is("Enabled")) {
            let param = |name: &str| e.child(name).and_then(Param::read);
            let inherit = match e.child("AddEmitterVelocity") {
                Some(n) if n.words.get(1).is_some_and(|w| w == "TRUE") => [1.0; 4],
                Some(n) => four(&n.numbers()),
                None => [0.0; 4],
            };
            let missing = |what: &str| format!("emitter without {what}");
            let zero = Param::Fixed([0.0; 4]);
            emitters.push(Emitter {
                rate: param("EmissionRate").ok_or_else(|| missing("rate"))?,
                inherit,
                transform: Transform::read(
                    e.child("Transform").ok_or_else(|| missing("transform"))?,
                    clip,
                ),
                shape: e
                    .child("Shape")
                    .and_then(Shape::read)
                    .ok_or_else(|| missing("shape"))?,
                velocity: param("Velocity").unwrap_or(zero.clone()),
                rotation: param("Rotation").unwrap_or(zero.clone()),
                color: param("Color").unwrap_or(Param::Fixed([1.0; 4])),
                life: param("Lifetime").unwrap_or(Param::Fixed([1.0; 4])),
                size: param("Size").unwrap_or(Param::Fixed([1.0; 4])),
                tex_frame: param("TexFrame").unwrap_or(zero),
            });
        }
        let mut actions = Vec::new();
        for a in t.all("Action").filter(|a| a.is("Enabled")) {
            let param = |name: &str| a.child(name).and_then(Param::read);
            let transform = || a.child("Transform").map(|t| Transform::read(t, clip));
            let made = match a.words.get(1).map_or("", String::as_str) {
                "AttribAnim" => {
                    let names = ["Color", "Size", "TexFrame", "Rotation"];
                    Some(Action::Attributes(
                        names.iter().filter_map(|n| Some((*n, param(n)?))).collect(),
                    ))
                }
                "ConstAccel" => param("Accel").map(Action::Accelerate),
                "Damping" => Some(Action::Damp(
                    a.number("Coefficient").unwrap_or(0.0),
                    a.number("Threshold").unwrap_or(0.0),
                )),
                "Wander" => param("Magnitude").map(Action::Wander),
                "Kill" => transform()
                    .zip(a.child("Shape").and_then(Shape::read))
                    .map(|(t, s)| Action::Kill(t, s)),
                "ObjectBasedAdvect" => transform().map(Action::Advect),
                a => return Err(format!("unknown particle action {a}")),
            };
            actions.extend(made);
        }
        let mut moving = Vec::new();
        for p in t
            .all("ShaderParameters")
            .flat_map(|p| p.all("EditableParam"))
        {
            if let (Some(name), Some(v)) = (p.words.get(1), Param::read(p)) {
                moving.push((name.clone(), v));
            }
        }
        let tile = t
            .child("TextureTile")
            .map(Node::numbers)
            .unwrap_or_default();
        Ok(Self {
            start,
            end,
            enabled: t.is("Enabled"),
            sort: t.is("Sort"),
            tile: [0, 1].map(|i| tile.get(i).copied().unwrap_or(1.0)),
            most: t.number("MaxParticles").unwrap_or(100.0) as usize,
            warm_up: t.number("WarmUp").unwrap_or(0.0),
            owed: vec![0.0; emitters.len()],
            emitters,
            actions,
            curves: s.curves.clone(),
            params: Vec::new(),
            moving,
            seed,
            random: Random(seed),
            particles: Vec::new(),
            made: 0,
            steps: 0,
            ready: false,
        })
    }

    /// Brings the particles to a moment of the show; `false` outside of the system's time.
    pub fn go(&mut self, time: f32) -> bool {
        if !self.enabled || time < self.start || time >= self.end {
            self.ready = false;
            return false;
        }
        let want = ((time - self.start) / STEP) as i64;
        if !self.ready || want < self.steps || want > self.steps + 240 {
            // Older particles are gone, so there is no need to begin at the start.
            let before = (self.warm_up / STEP) as i64;
            let longest = self.emitters.iter().map(|e| match e.life {
                Param::Fixed(v) | Param::Random(v, _) => v[0],
                _ => 5.0,
            });
            let longest = (longest.fold(1.0, f32::max) * 1.5 / STEP) as i64;
            self.particles.clear();
            self.random = Random(self.seed);
            self.owed.fill(0.0);
            self.made = 0;
            self.steps = (-before).max(want - longest);
            self.ready = true;
        }
        while self.steps < want {
            self.steps += 1;
            self.step(self.steps as f32 * STEP);
        }
        let at = time - self.start;
        self.params.clear();
        for (n, p) in &self.moving {
            self.params
                .push((n.clone(), p.at(at, &self.curves, &mut self.random)[0]));
        }
        true
    }

    fn step(&mut self, time: f32) {
        let r = &mut self.random;
        for p in &mut self.particles {
            p.age += STEP;
        }
        self.particles.retain(|p| p.age < p.life);
        for (e, owed) in self.emitters.iter().zip(&mut self.owed) {
            *owed += e.rate.at(time, &self.curves, r)[0].max(0.0) * STEP;
            let (now, before) = (e.transform.at(time), e.transform.at(time - STEP));
            while *owed >= 1.0 {
                *owed -= 1.0;
                if self.particles.len() >= self.most {
                    continue;
                }
                // Spread over the step.
                let f = r.next();
                let inside = e.shape.point(r);
                let position = now
                    .transform_point3(inside)
                    .lerp(before.transform_point3(inside), f);
                let v = e.velocity.at(time, &self.curves, r);
                let mut velocity = now.transform_vector3(Vec3::new(v[0], v[1], v[2]));
                let own = (now.w_axis - before.w_axis).truncate() / STEP;
                velocity += own * Vec3::new(e.inherit[0], e.inherit[1], e.inherit[2]);
                let tex_frame = e.tex_frame.at(time, &self.curves, r)[0];
                self.made += 1;
                self.particles.push(Particle {
                    position,
                    velocity,
                    age: f * STEP,
                    life: e.life.at(time, &self.curves, r)[0],
                    color: e.color.at(time, &self.curves, r),
                    size: e.size.at(time, &self.curves, r)[0],
                    tex_frame,
                    first_frame: tex_frame,
                    rotation: e.rotation.at(time, &self.curves, r),
                    id: self.made,
                });
            }
        }
        for a in &self.actions {
            match a {
                Action::Attributes(list) => {
                    for p in &mut self.particles {
                        for (name, param) in list {
                            let v = param.at(p.age, &self.curves, r);
                            let add = matches!(param, Param::Curve { add: true, .. });
                            match *name {
                                "Color" => p.color = v,
                                "Size" => p.size = v[0],
                                "TexFrame" => {
                                    p.tex_frame = v[0] + if add { p.first_frame } else { 0.0 }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Action::Accelerate(v) => {
                    let v = v.at(time, &self.curves, r);
                    let v = Vec3::new(v[0], v[1], v[2]) * STEP;
                    self.particles.iter_mut().for_each(|p| p.velocity += v);
                }
                Action::Damp(c, threshold) => {
                    for p in self
                        .particles
                        .iter_mut()
                        .filter(|p| p.velocity.length() > *threshold)
                    {
                        p.velocity *= (1.0 - c * STEP).max(0.0);
                    }
                }
                Action::Wander(v) => {
                    for p in &mut self.particles {
                        let m = v.at(time, &self.curves, r)[0];
                        p.velocity += r.direction() * (m * STEP);
                    }
                }
                Action::Kill(t, Shape::Plane(c, n)) => {
                    let m = t.at(time);
                    let (c, n) = (m.transform_point3(*c), m.transform_vector3(*n));
                    self.particles.retain(|p| (p.position - c).dot(n) >= 0.0);
                }
                Action::Kill(..) => {}
                Action::Advect(t) => {
                    let m = t.at(time) * t.at(time - STEP).inverse();
                    self.particles
                        .iter_mut()
                        .for_each(|p| p.position = m.transform_point3(p.position));
                }
            }
        }
        for p in &mut self.particles {
            p.position += p.velocity * STEP;
        }
    }
}
