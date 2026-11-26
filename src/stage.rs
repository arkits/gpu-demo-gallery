//! What moves in this demo: the fortune teller's head.
use std::collections::HashMap;

use glam::{Mat4, Vec3};

use crate::{
    Error, anim,
    data::Files,
    demo,
    model::{Animation, Array, Fields, Fur, Model},
    pass::EYE_LIGHT,
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

struct Key {
    ms: u32,
    coords: Vec<f32>,
    normals: Vec<f32>,
    tangents: Vec<f32>,
}

/// A clip: whole meshes at moments in time, and joints.
struct Clip {
    item: usize,
    tracks: Vec<(usize, Vec<Key>)>,
    animation: Option<Animation>,
}

impl Clip {
    /// `own` has each mesh's tangents as the scene gives them, for keys without any.
    fn read(
        item: usize,
        bytes: &[u8],
        model: &Model,
        own: &[Option<Vec<f32>>],
    ) -> Result<Self, Error> {
        let root = crate::tree::read_binary(bytes)?;
        let mut tracks = Vec::new();
        let mut animation = None;
        if let Some(node) = root.nodes("animations").first() {
            // The program takes these out (0x403000).
            let mut channels = anim::read(node)?;
            channels.retain(|c| c.field != "scale");
            animation = Some(Animation::new(model, channels));
            for channel in node
                .nodes("channels")
                .iter()
                .filter(|c| c.kind == "MeshChannel")
            {
                let name = channel.text("targetNode");
                let geometry = model
                    .geometries
                    .iter()
                    .position(|g| g.name == name)
                    .ok_or_else(|| format!("clip for unknown mesh {name}"))?;
                let count = model.geometries[geometry].count();
                let mut keys = Vec::new();
                for k in channel.nodes("curves") {
                    let list = |f| k.floats(f).to_vec();
                    let mut key = Key {
                        ms: k.int("time"),
                        coords: list("coords"),
                        normals: list("normals"),
                        tangents: list("tangents"),
                    };
                    // Five of the clips have no tangents for the face; it keeps its own [I].
                    if key.tangents.is_empty()
                        && let Some(t) = &own[geometry]
                    {
                        key.tangents.clone_from(t);
                    }
                    if key.coords.len() != count * 3
                        || key.normals.len() != count * 3
                        || key.tangents.len() != count * 4
                    {
                        return Err(format!("{name}: clip does not fit its mesh").into());
                    }
                    keys.push(key);
                }
                if keys.is_empty() || !keys.is_sorted_by_key(|k| k.ms) {
                    return Err(format!("{name}: bad keys").into());
                }
                tracks.push((geometry, keys));
            }
        }
        Ok(Self {
            item,
            tracks,
            animation,
        })
    }
}

/// The keys around a moment, and how far it is to the second.
fn around(keys: &[Key], ms: f32) -> (usize, usize, f32) {
    let after = keys.partition_point(|k| (k.ms as f32) <= ms);
    let a = after.saturating_sub(1);
    let b = after.min(keys.len() - 1);
    let span = keys[b].ms.saturating_sub(keys[a].ms).max(1) as f32;
    (a, b, ((ms - keys[a].ms as f32) / span).clamp(0.0, 1.0))
}

pub struct Stage {
    clips: Vec<Vec<u8>>,
    /// Each mesh's tangents before any clip replaced them.
    tangents: Vec<Option<Vec<f32>>>,
    clip: Option<Clip>,
    /// Seconds into the clip.
    local: f32,
    /// Nodes as earlier clips left them.
    fields: Vec<Fields>,
    /// Keys now in the meshes.
    loaded: Vec<(usize, usize)>,
    blend: f32,
}

impl Stage {
    pub fn new(files: &Files, models: &mut [Model]) -> Result<Self, Error> {
        let clips = demo::CLIPS
            .iter()
            .map(|c| {
                files
                    .get(&format!("models/{}", c.scene))
                    .map(<[u8]>::to_vec)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tangents = models[0]
            .geometries
            .iter()
            .map(|g| {
                let t = g.tex_coords.get(2).filter(|t| t.width == 4)?;
                (t.values.len() == g.count() * 4).then(|| t.values.clone())
            })
            .collect();
        Ok(Self {
            clips,
            tangents,
            clip: None,
            local: 0.0,
            fields: models[0].still(),
            loaded: Vec::new(),
            blend: 0.0,
        })
    }

    /// Reads every clip, as playing them would.
    pub fn check_clips(&self, model: &Model) -> Result<(), Error> {
        for (item, bytes) in self.clips.iter().enumerate() {
            Clip::read(item, bytes, model, &self.tangents)
                .map_err(|e| format!("{}: {e}", demo::CLIPS[item].scene))?;
        }
        Ok(())
    }

    pub fn show(&self, _: &[&Model]) -> Show {
        Show::default()
    }

    pub fn alternates(&self, _: &Model, _: usize, _: usize) -> Vec<Vec<Option<usize>>> {
        Vec::new()
    }

    /// Moves meshes and returns the geometries that changed.
    pub fn update(&mut self, _actor: usize, model: &mut Model, frame: &Frame) -> Vec<usize> {
        let Some((item, local)) = frame.clip else {
            return Vec::new();
        };
        if self.clip.as_ref().is_none_or(|c| c.item != item) {
            if let Some(a) = self.clip.as_ref().and_then(|c| c.animation.as_ref()) {
                a.apply(&mut self.fields, f64::from(self.local) * 1000.0);
            }
            // Every clip reads (tests/data.rs); this only guards against other files.
            self.clip = Clip::read(item, &self.clips[item], model, &self.tangents)
                .inspect_err(|e| eprintln!("clip {item}: {e}"))
                .ok();
            self.loaded.clear();
        }
        self.local = local;
        let Some(clip) = &self.clip else {
            return Vec::new();
        };
        let mut changed = Vec::new();
        self.loaded
            .resize(clip.tracks.len(), (usize::MAX, usize::MAX));
        for ((geometry, keys), loaded) in clip.tracks.iter().zip(&mut self.loaded) {
            let (a, b, blend) = around(keys, local * 1000.0);
            self.blend = blend;
            if *loaded == (a, b) {
                continue;
            }
            *loaded = (a, b);
            let array = |width, f: fn(&Key) -> &Vec<f32>| {
                [&keys[a], &keys[b]].map(|k| Array {
                    width,
                    values: f(k).clone(),
                })
            };
            let g = &mut model.geometries[*geometry];
            g.coords = array(3, |k| &k.coords).into();
            g.normals = array(3, |k| &k.normals).into();
            g.tex_coords.truncate(2);
            g.tex_coords.extend(array(4, |k| &k.tangents));
            changed.push(*geometry);
        }
        changed
    }

    pub fn instances(
        &self,
        _: usize,
        model: &Model,
        frame: &Frame,
        _: Mat4,
        _: Vec3,
    ) -> Vec<Instance> {
        let animation = self.clip.as_ref().and_then(|c| c.animation.as_ref());
        // Between keys the vertex program blends two meshes.
        let values = HashMap::from([
            (EYE_LIGHT, frame.light.extend(0.0).to_array()),
            ("interpolantA", [1.0 - self.blend; 4]),
            ("interpolantB", [self.blend; 4]),
        ]);
        let mut fields = self.fields.clone();
        if let Some(a) = animation {
            a.apply(&mut fields, f64::from(self.local) * 1000.0);
        }
        vec![Instance {
            world: model.compose(&fields),
            values,
            registers: Vec::new(),
            camera: None,
        }]
    }

    pub fn draws(
        &self,
        _: usize,
        model: &Model,
        shape: usize,
        _: &Frame,
        _: &Instance,
    ) -> Vec<Draw> {
        vec![Draw {
            passes: 0..model.shapes[shape].passes.len(),
            ..Default::default()
        }]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_blend_and_hold() {
        let key = |ms| Key {
            ms,
            coords: vec![],
            normals: vec![],
            tangents: vec![],
        };
        let keys = [key(0), key(100), key(300)];
        assert_eq!(around(&keys, 0.0), (0, 1, 0.0));
        assert_eq!(around(&keys, 50.0), (0, 1, 0.5));
        assert_eq!(around(&keys, 200.0), (1, 2, 0.5));
        assert_eq!(around(&keys, 9000.0).1, 2);
    }
}
