mod chest;
mod geoforms;
mod gpu;
#[cfg(not(target_arch = "wasm32"))]
mod native;
mod sushi;
#[cfg(not(target_arch = "wasm32"))]
pub use native::run_native;
#[cfg(target_arch = "wasm32")]
mod web;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use sushi_formats::{archive::Archive, path::FlyPath, texture::Level};

pub use gpu::Player;
pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Demo {
    Dolphin,
    Nature,
    Rachel,
    Island1,
    Island2,
    Island3,
    TreasureChest,
    GeoForms,
}

impl Demo {
    pub const ALL: [Self; 8] = [
        Self::Dolphin,
        Self::Nature,
        Self::Rachel,
        Self::Island1,
        Self::Island2,
        Self::Island3,
        Self::TreasureChest,
        Self::GeoForms,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Dolphin => "dolphin",
            Self::Nature => "nature",
            Self::Rachel => "rachel",
            Self::Island1 => "island1",
            Self::Island2 => "island2",
            Self::Island3 => "island3",
            Self::TreasureChest => "treasure-chest",
            Self::GeoForms => "geoforms",
        }
    }

    pub fn from_slug(slug: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|demo| demo.slug() == slug)
            .ok_or_else(|| format!("unknown demo {slug:?}").into())
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Dolphin => "ATI Dolphin",
            Self::Nature => "ATI Nature",
            Self::Rachel => "ATI Rachel",
            Self::Island1 => "ATI Island 1",
            Self::Island2 => "ATI Island 2",
            Self::Island3 => "ATI Island 3",
            Self::TreasureChest => "ATI Treasure Chest",
            Self::GeoForms => "GeoForms",
        }
    }

    pub fn scene(self, pack: &[u8]) -> Result<Scene> {
        let files = Archive::open(pack)?;
        match self {
            Self::TreasureChest => chest::load(&files),
            Self::GeoForms => geoforms::load(&files),
            _ => sushi::load(self, &files),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub uv_extra: [f32; 4],
}

impl Vertex {
    fn new(pos: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            pos,
            normal,
            uv,
            color: [1.0; 4],
            uv_extra: [uv[0], uv[1], uv[0], uv[1]],
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Effect {
    Baked,
    Lit,
    Caustics,
    Water,
    Sky,
    Skin,
    Metal,
    Unlit,
    Terrain,
    PortalWater,
    CloudSky,
    StainedGlass,
    Teleporter,
    DistortedSprite,
}

#[derive(Clone, Copy)]
pub(crate) struct Material {
    texture: usize,
    detail: usize,
    third: usize,
    effect: Effect,
    alpha_cut: f32,
    blend: bool,
    strength: f32,
    tint: [f32; 4],
}

impl Default for Material {
    fn default() -> Self {
        Self {
            texture: 0,
            detail: 0,
            third: 0,
            effect: Effect::Lit,
            alpha_cut: 0.0,
            blend: false,
            strength: 0.0,
            tint: [1.0; 4],
        }
    }
}

pub(crate) struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    material: Material,
    motion: Option<usize>,
    showcase: u8,
}

pub(crate) enum Motion {
    Skeletal {
        origin: Mat4,
        start: f32,
        track: sushi::Track,
        skin: Vec<sushi::Skin>,
        pn_level: usize,
    },
    Morph {
        origin: Mat4,
        start: f32,
        track: sushi::Track,
        frames: Vec<std::sync::Arc<[Vertex]>>,
        skin: Vec<sushi::Skin>,
    },
    ChestLid,
    ChestShowpiece,
}

pub struct Scene {
    pub(crate) demo: Demo,
    pub(crate) meshes: Vec<Mesh>,
    pub(crate) textures: Vec<Vec<Level>>,
    pub(crate) motions: Vec<Motion>,
    pub(crate) path: FlyPath,
    pub(crate) chest: Option<chest::Settings>,
    pub(crate) geo: Option<geoforms::Settings>,
    pub(crate) fov: f32,
    pub(crate) far: f32,
    pub(crate) clear: [f64; 3],
    pub(crate) initial_time: f32,
}

impl Scene {
    fn new(demo: Demo) -> Self {
        Self {
            demo,
            meshes: Vec::new(),
            textures: vec![vec![Level {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            }]],
            motions: Vec::new(),
            path: FlyPath {
                step: 1.0,
                frames: Vec::new(),
            },
            geo: None,
            chest: None,
            fov: 65.0,
            far: 100_000.0,
            clear: [0.0; 3],
            initial_time: 0.0,
        }
    }

    pub fn geometry_counts(&self) -> (usize, usize, usize) {
        (
            self.meshes.len(),
            self.meshes.iter().map(|mesh| mesh.indices.len() / 3).sum(),
            self.textures.len() - 1,
        )
    }

    fn time(&self, elapsed: f32) -> f32 {
        if self.path.frames.len() < 2 {
            return elapsed;
        }
        let duration = (self.path.frames.len() - 1) as f32 * self.path.step;
        elapsed.rem_euclid(duration)
    }
}

fn camera_on(path: &FlyPath, time: f32, fov: f32) -> (Vec3, Quat, f32) {
    let frames = &path.frames;
    let period = frames.len().saturating_sub(1).max(1) as f32;
    let phase = (time / path.step).rem_euclid(period);
    let index = phase as usize;
    let a = frames[index];
    let b = frames.get(index + 1).copied().unwrap_or(a);
    let fraction = if a.flag == 0 { phase.fract() } else { 0.0 };
    let rotation = Quat::from_array(a.quat)
        .normalize()
        .slerp(Quat::from_array(b.quat).normalize(), fraction);
    let before = index
        .checked_sub(1)
        .and_then(|i| frames.get(i))
        .filter(|f| f.flag == 0)
        .unwrap_or(&a);
    let after = frames.get(index + 2).filter(|_| b.flag == 0).unwrap_or(&b);
    let [p0, p1, p2, p3] = [before.pos, a.pos, b.pos, after.pos].map(Vec3::from);
    let pos = p1
        + 0.5
            * ((p2 - p0) * fraction
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * fraction.powi(2)
                + (3.0 * p1 - p0 - 3.0 * p2 + p3) * fraction.powi(3));
    let fov_a = if a.fov > 0.0 { a.fov } else { fov };
    let fov_b = if b.fov > 0.0 { b.fov } else { fov };
    (pos, rotation, fov_a + (fov_b - fov_a) * fraction)
}

fn asset<'a>(files: &'a Archive, name: &str) -> Result<&'a [u8]> {
    files
        .get(name)
        .ok_or_else(|| format!("missing asset {name}").into())
}

fn image_texture(scene: &mut Scene, bytes: &[u8], format: image::ImageFormat) -> Result<usize> {
    let decoded = image::load_from_memory_with_format(bytes, format)?.into_rgba8();
    let index = scene.textures.len();
    scene.textures.push(vec![Level {
        width: decoded.width() as usize,
        height: decoded.height() as usize,
        rgba: decoded.into_raw(),
    }]);
    Ok(index)
}
