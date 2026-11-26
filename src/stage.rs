//! What moves in this demo: the wolfman, his street and the light on him.
use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4};

use crate::{
    Error,
    data::Files,
    model::{Fur, Model, SHADOW_MAP},
    show::{Frame, Show},
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

/// Tiles of the street, 500 long, that come back at the far end.
const TILES: [&str; 3] = ["Street", "Street_2", "Street_3"];
const ROAD: f32 = 1500.0;
/// Groups drawn first; what is in the last is the wolfman.
const WOLFMAN: u32 = 2;

pub struct Stage {
    /// The node that holds the light's texture matrix, and the light's own.
    cookie: usize,
    lamp: usize,
    /// Half the light's angle, and how far it reaches (`spotLightShape1`).
    cone: f32,
    /// Nodes of each tile.
    tiles: Vec<bool>,
}

impl Stage {
    pub fn new(_files: &Files, models: &mut [Model]) -> Result<Self, Error> {
        let model = &mut models[0];
        for s in model.shapes.iter_mut().filter(|s| s.order == WOLFMAN) {
            // Fur casts the shadow of the skin below it.
            if let Some(p) = s.passes.first() {
                let shadow = p.depths(0);
                s.passes.push(shadow);
            }
        }
        let find = |name: &str| model.find(name).ok_or_else(|| format!("missing {name}"));
        let light = model.lights.iter().find(|l| l.name == "spotLightShape1");
        Ok(Self {
            cookie: find("spotLightShape1_spotlight_transform")?,
            lamp: light.and_then(|l| l.node).ok_or("missing spotlight")?,
            cone: light.map_or(0.45, |l| l.cutoff),
            tiles: model
                .nodes
                .iter()
                .map(|n| TILES.contains(&n.name.as_str()))
                .collect(),
        })
    }

    pub fn show(&self, models: &[&Model]) -> Show {
        let model = models[0];
        let body =
            |s: &crate::model::Shape| ["Torso", "Face", "Leg"].iter().any(|n| s.name.contains(n));
        let (min, max) = model.bounds(body);
        let ground = model.find("groundanim");
        let path = match (ground, model.animations.first()) {
            (Some(node), Some(a)) => (0..3300)
                .map(|i| model.pose([(a, f64::from(i) * 10.0)])[node].w_axis.z)
                .collect(),
            _ => Vec::new(),
        };
        Show::new((min + max) * 0.5, (max - min).length() * 0.5, path)
    }

    pub fn alternates(&self, _: &Model, _: usize, _: usize) -> Vec<Vec<Option<usize>>> {
        Vec::new()
    }

    pub fn update(&mut self, _: usize, _: &mut Model, _: &Frame) -> Vec<usize> {
        Vec::new()
    }

    pub fn instances(
        &self,
        _: usize,
        model: &Model,
        frame: &Frame,
        _: Mat4,
        _: Vec3,
    ) -> Vec<Instance> {
        let Some(a) = model.animations.first() else {
            return Vec::new();
        };
        let mut world = model.pose([(a, frame.ms)]);
        for (i, n) in model.nodes.iter().enumerate() {
            let tile =
                std::iter::successors(Some(i), |&k| model.nodes[k].parent).find(|&k| self.tiles[k]);
            if tile.is_some() && n.parent.is_some() || self.tiles[i] {
                let at = world[tile.unwrap_or(i)].w_axis.z;
                let to = (at + frame.street + ROAD * 0.5).rem_euclid(ROAD) - ROAD * 0.5;
                world[i].w_axis.z += to - at;
            }
        }
        // The light as a camera, and its picture's coordinates with rows from the top.
        let projection =
            glam::camera::rh::proj::opengl::perspective(2.0 * self.cone, 1.0, 10.0, 150.0);
        let view = world[self.lamp].inverse();
        let half = Mat4::from_cols(
            Vec4::new(0.5, 0.0, 0.0, 0.0),
            Vec4::new(0.0, -0.5, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.5, 0.0),
            Vec4::new(0.5, 0.5, 0.5, 1.0),
        );
        world[self.cookie] = half * projection * view * frame.view.inverse();
        let instance = |camera| Instance {
            world: world.clone(),
            values: Values::new(),
            registers: Vec::new(),
            camera,
        };
        vec![instance(Some([projection, view])), instance(None)]
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
        let light = instance.camera.is_some();
        let own = |p: &crate::pass::Pass| p.offscreen.is_some() == light;
        let first = s.passes.iter().position(own);
        let passes = match first {
            Some(i) => i..s.passes.iter().filter(|p| own(p)).count() + i,
            None => 0..0,
        };
        vec![Draw {
            passes,
            shells: if light { 0 } else { frame.layers as usize },
            layer: u32::from(s.fur.is_some()),
            values: s
                .fur
                .as_ref()
                .map(|f| Values::from([("furScale", f.scale)])),
            ..Default::default()
        }]
    }

    pub fn offscreen(&self) -> Vec<Offscreen> {
        vec![Offscreen {
            file: SHADOW_MAP,
            size: 1024,
            depth: true,
            clear: wgpu::Color::WHITE,
        }]
    }

    pub fn effects(&self) -> Vec<Effect> {
        Vec::new()
    }

    pub fn effect_values(&self, _: usize, _: &Frame, _: Mat4, _: Vec3) -> Option<Vec<[f32; 4]>> {
        None
    }

    /// Shells lie further out, the program setting their depth for the scene's
    /// (0x402390), and are brighter there [I].
    pub fn shell(&self, fur: &Fur, shell: usize, frame: &Frame) -> Values {
        let part = (shell + 1) as f32 / fur.textures.len().max(1) as f32;
        Values::from([
            (
                "shellScale",
                [
                    frame.spacing * (shell + 1) as f32,
                    0.55 + 0.45 * part,
                    0.0,
                    0.0,
                ],
            ),
            ("furScale", fur.scale),
        ])
    }
}
