//! What moves in this demo: a chameleon that walks along a vine and changes its skin.
use std::collections::HashMap;

use glam::{Mat4, Vec3};

use crate::{
    Error,
    data::Files,
    demo::{FADE, LEAD, PERIOD, SKINS},
    model::{Fur, Model},
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

/// Units a branch is long, and moves in a period.
const BRANCH: f32 = 64.0;
/// The skin that has holes.
const CUTOUT: usize = SKINS - 1;

/// Which skins show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skins {
    pub old: usize,
    pub new: usize,
    /// From 0 to 1 while one replaces the other.
    pub fade: Option<f32>,
    /// How many skins there were before.
    pub turn: usize,
}

impl Skins {
    /// As 0x4023ba does it.
    pub fn at(ms: f64) -> Self {
        let turns = (ms + LEAD) / PERIOD;
        let turn = turns as usize;
        let since = ms - (turn as f64 * PERIOD - LEAD);
        let fade = (since * FADE) as f32;
        let new = turn % SKINS;
        Self {
            old: (new + SKINS - 1) % SKINS,
            new,
            fade: (turn > 0 && fade < 1.0).then_some(fade),
            turn,
        }
    }
}

pub struct Stage {
    /// The branch each node belongs to, if any.
    branches: Vec<Option<usize>>,
}

/// Passes that are added to a skin's own: where it is already drawn, then over it.
const OVER_EQUAL: usize = 1;
const FADE_IN: usize = 2;
const FADE_OUT: usize = 3;

impl Stage {
    pub fn new(_files: &Files, models: &mut [Model]) -> Result<Self, Error> {
        let model = &mut models[0];
        let mut branches: Vec<Option<usize>> = Vec::new();
        let mut count = 0;
        for n in &model.nodes {
            let own = n.name.starts_with("Branch_group_1").then(|| {
                count += 1;
                count - 1
            });
            branches.push(own.or(n.parent.and_then(|p| branches[p])));
        }
        if count != SKINS {
            return Err(format!("{count} branches").into());
        }
        for shape in model.shapes.iter_mut().filter(|s| s.variant.is_some()) {
            let base = shape.passes.first().ok_or("skin without a pass")?.clone();
            // A less-than test would fail where the first skin was drawn.
            for (blend, func) in [((5, 4), 5), ((4, 5), 2), ((5, 4), 2)] {
                let mut p = base.clone();
                p.state.blend = Some(blend);
                p.state.depth = Some((func, false));
                shape.passes.push(p);
            }
            shape.passes.truncate(4);
        }
        Ok(Self { branches })
    }

    pub fn show(&self, models: &[&Model]) -> Show {
        let (min, max) = models[0].bounds(|s| s.variant.is_some());
        Show::new((min + max) * 0.5)
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
        let mut world = model.pose(model.animations.iter().map(|a| (a, frame.ms)));
        // Branches take turns; their animations go on and on.
        for (m, branch) in world.iter_mut().zip(&self.branches) {
            if let Some(b) = branch {
                let turn = frame.skins.turn as i64;
                *m = Mat4::from_translation(Vec3::X * -BRANCH * slot(*b, turn) as f32) * *m;
            }
        }
        let fade = frame.skins.fade.unwrap_or(1.0);
        vec![Instance {
            world,
            values: HashMap::from([("chameleonBlend", [fade; 4])]),
            registers: Vec::new(),
            camera: None,
        }]
    }

    pub fn draws(
        &self,
        _: usize,
        model: &Model,
        shape: usize,
        frame: &Frame,
        _: &Instance,
    ) -> Vec<Draw> {
        let s = &model.shapes[shape];
        let one = |pass: usize, layer| Draw {
            passes: pass..pass + 1,
            layer,
            ..Default::default()
        };
        let whole = || Draw {
            values: Some(HashMap::from([("chameleonBlend", [1.0; 4])])),
            ..one(0, 0)
        };
        let skins = frame.skins;
        let Some(variant) = s.variant else {
            let branch = s.passes.first().and_then(|p| self.branches[p.node]);
            let turn = skins.turn as i64;
            // The last one, this and the next (0x402483).
            let near = branch.is_none_or(|b| slot(b, turn) <= turn + 1);
            return if near { vec![one(0, 0)] } else { Vec::new() };
        };
        let cutout = skins.old == CUTOUT || skins.new == CUTOUT;
        match (skins.fade, variant) {
            (None, v) if v == skins.new => vec![whole()],
            (None, _) => Vec::new(),
            // The skin with holes shows what was drawn before, so it comes first and is
            // whole. The original blends by equal depth here too, which hides the
            // animal at first [I].
            (Some(_), CUTOUT) if cutout => vec![whole()],
            (Some(_), v) if cutout && v == skins.new => vec![one(FADE_IN, 1)],
            (Some(_), v) if cutout && v == skins.old => vec![one(FADE_OUT, 1)],
            (Some(_), v) if v == skins.new => vec![one(0, 0)],
            (Some(_), v) if v == skins.old => vec![one(OVER_EQUAL, 1)],
            _ => Vec::new(),
        }
    }

    pub fn offscreen(&self) -> Vec<Offscreen> {
        Vec::new()
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

/// The turn a branch is for: the one before, this one, or one of the next five.
fn slot(branch: usize, turn: i64) -> i64 {
    let n = SKINS as i64;
    turn - 1 + (branch as i64 - (turn - 1)).rem_euclid(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skins_take_turns() {
        assert_eq!(Skins::at(0.0).new, 0);
        assert_eq!(Skins::at(0.0).fade, None);
        let s = Skins::at(PERIOD - LEAD + 1000.0);
        assert_eq!((s.old, s.new, s.turn), (0, 1, 1));
        assert!((s.fade.unwrap() - 0.3).abs() < 1e-4);
        assert_eq!(Skins::at(PERIOD - LEAD + 4000.0).fade, None);
        assert_eq!(Skins::at(7.0 * PERIOD).new, 0);
    }

    #[test]
    fn branches_follow() {
        assert_eq!(
            (0..SKINS).map(|b| slot(b, 0)).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, -1]
        );
        assert_eq!(
            (slot(0, 1), slot(1, 1), slot(2, 1), slot(6, 1)),
            (0, 1, 2, 6)
        );
        assert_eq!((slot(0, 8), slot(1, 8)), (7, 8));
    }
}
