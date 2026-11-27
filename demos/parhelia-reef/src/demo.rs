//! Matrox Parhelia Reef demo (2002): a coral reef with schools of fish, rays, a shark and a
//! jellyfish, under rippling caustics, on the TDRG engine (Direct3D 8, vertex shaders 1.1,
//! pixel shaders 1.3). Everything is data: the world and shader scripts, DirectX .X models
//! with their skins and animations, TGA pictures with their mipmaps, and the shaders.
use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::Error;
use crate::d3d::{self, Device, Gpu, PVertex, ProgConsts, TextureId};
use crate::data::Files;
use crate::image::Image;
use crate::script::{self, Item};
use crate::shader;
use crate::xfile::{Model, XMesh};

pub const FILES: &[&str] = &[];
pub const REQUIRED: &[&str] = &["reefdemoscene.s", "reefdemoscene.w"];
pub const PARTS: &[(&str, usize, usize)] = &[];
/// The music as shipped (WMA, which neither rodio nor browsers read), and as FLAC or WAV made
/// from it: `scripts/pack.sh` makes the FLAC for the browser version.
pub const SOUNDS: &[&str] = &["music.wma", "music.flac", "music.wav"];
pub const BUTTONS: &[(&str, &str)] = &[(" ", "Pause"), ("r", "Restart"), ("c", "Camera")];
pub const SLIDERS: &[(&str, f32, f32)] = &[];

/// The tour: the camera's animation and the music both last this long (milliseconds in the
/// files).
const LOOP: f64 = 95.0;
/// Bones a draw can carry (the palette of 12 in the shaders).
const PALETTE: usize = 12;

#[derive(Debug, Clone)]
enum Value {
    Unset,
    Literal(f32),
    Variable(u32),
    Wave(u32, [(bool, f32); 5]),
    Position(u32),
    Matrix(u32, u32),
    Palette(u32),
    Bunny(String),
}

#[derive(Debug, Clone)]
struct Const {
    reg: usize,
    comp: usize,
    value: Value,
}

fn constants(item: Option<&Item>) -> Vec<Const> {
    let Some(item) = item else { return Vec::new() };
    item.all("Constant")
        .filter_map(|c| {
            let w = c.value.as_ref()?;
            let n = |i: usize| w.get(i).and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.0);
            let reg = n(0) as usize;
            let comp = n(1) as usize;
            let value = match n(2) as u32 {
                1 => Value::Literal(n(3)),
                2 => Value::Variable(n(3) as u32),
                3 => Value::Wave(
                    n(3) as u32,
                    std::array::from_fn(|k| (n(4 + 2 * k) != 0.0, n(5 + 2 * k))),
                ),
                5 => Value::Position(n(3) as u32),
                6 => Value::Matrix(n(3) as u32, n(4) as u32),
                7 => Value::Palette(n(3) as u32),
                12 => Value::Bunny(w.get(3).cloned().unwrap_or_default()),
                _ => Value::Unset,
            };
            Some(Const { reg, comp, value })
        })
        .collect()
}

#[derive(Debug, Clone)]
struct Layer {
    stage: usize,
    texture: Option<TextureId>,
    volume: bool,
    tss: Vec<(u32, u32)>,
}

#[derive(Debug, Clone)]
struct Pass {
    skip: bool,
    skip_bunny: Option<String>,
    states: Vec<(u32, u32)>,
    vs: Vec<Const>,
    ps: Vec<Const>,
    layers: Vec<Layer>,
    program: usize,
    volumes: [bool; 4],
    weights: usize,
}

#[derive(Debug, Clone)]
struct ShaderDef {
    passes: Vec<Pass>,
}

/// One draw's worth of a mesh: a material's triangles that use at most `PALETTE` bones.
struct Batch {
    vertices: usize,
    indices: usize,
    count: u32,
    /// Skins of the mesh, by palette slot.
    bones: Vec<usize>,
}

struct MeshData {
    frame: Option<usize>,
    /// "frame.Mesh_i." and each material's name, the keys of shader assignments.
    key: String,
    names: Vec<String>,
    /// (material, batches), with the shader picked per material by the actor.
    materials: Vec<Vec<Batch>>,
    /// Each skin's frame and offset.
    skins: Vec<(Option<usize>, Mat4)>,
}

struct ModelData {
    model: Model,
    meshes: Vec<MeshData>,
    /// Per frame: the track that moves it.
    tracks: Vec<Option<usize>>,
    length: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Mesh,
    Logo,
    Camera,
}

struct Actor {
    name: String,
    kind: Kind,
    sort: i32,
    model: Option<String>,
    /// Shaders by "frame.Mesh_i.material" in lower case (the model's name dropped).
    shaders: HashMap<String, String>,
    position: Vec3,
    rotation: Vec3,
    scale: Vec3,
    look_at: Vec3,
    offset: f32,
    render: bool,
    near: f32,
    far: f32,
    fov: f32,
    /// Computed each frame.
    world: Mat4,
    frames: Vec<Mat4>,
    target: Vec3,
}

struct Relation {
    dependant: String,
    position: (String, String),
    look_at: (String, String),
    rotation: (String, String),
}

pub struct Demo {
    shaders: HashMap<String, ShaderDef>,
    models: HashMap<String, ModelData>,
    actors: Vec<Actor>,
    relations: Vec<Relation>,
    world_shader: Option<ShaderDef>,
    bunnies: Option<String>,
    cameras: Vec<usize>,
    camera: usize,
    time: f64,
    paused: bool,
    still: bool,
    cues: Vec<String>,
    started: bool,
    frame_count: u32,
    vc: [[f32; 4]; 96],
    pc: [[f32; 4]; 8],
}

fn texture_name(name: &str) -> String {
    name.trim_end_matches(".tga")
        .trim_end_matches(".TGA")
        .to_string()
}

fn load_texture(
    gpu: &Gpu,
    dev: &mut Device,
    files: &Files,
    name: &str,
    volume: bool,
    slices: usize,
    mips: usize,
) -> Result<TextureId, Error> {
    let stem = texture_name(name);
    if volume {
        let mut images = Vec::new();
        for s in 0..slices.max(1) {
            let f = format!("{stem}_L{s:02}_M00.tga");
            images.push(files.image(&f)?);
        }
        return Ok(dev.volume(gpu, &images));
    }
    let mut levels = Vec::new();
    for m in 0..mips.max(1) {
        let f = format!("{stem}_M{m:02}.tga");
        match files.image(&f) {
            Ok(i) => levels.push(i),
            Err(e) if m == 0 => match files.image(name) {
                Ok(i) => levels.push(i),
                Err(_) => return Err(e),
            },
            Err(_) => break,
        }
    }
    Ok(dev.texture_levels(gpu, &levels))
}

/// TDRG's numbers are D3D's less one for blends and compare functions.
fn pass_states(p: &Item) -> Vec<(u32, u32)> {
    let n = |k: &str| p.num(k) as u32;
    vec![
        (d3d::ZENABLE, n("ZBufferEnabled")),
        (d3d::ZWRITEENABLE, n("ZWriteEnabled")),
        (d3d::ZFUNC, n("ZCompareFunc") + 1),
        (d3d::ZBIAS, n("ZBias")),
        (d3d::STENCILENABLE, n("StencilEnabled")),
        (d3d::STENCILFAIL, n("StencilFail") + 1),
        (d3d::STENCILZFAIL, n("StencilZFail") + 1),
        (d3d::STENCILPASS, n("StencilPass") + 1),
        (d3d::STENCILFUNC, n("StencilFunc") + 1),
        (d3d::STENCILREF, n("StencilRef")),
        // Culling is the exception: 1 is D3D's usual counter-clockwise, as every closed model
        // in the scene shows (inside out with clockwise).
        (d3d::CULLMODE, [1, 3, 2][n("CullMode") as usize % 3]),
        (d3d::ALPHABLENDENABLE, n("AlphaBlendEnabled")),
        (d3d::SRCBLEND, n("SrcBlend") + 1),
        (d3d::DESTBLEND, n("DestBlend") + 1),
        (d3d::ALPHATESTENABLE, n("AlphaTestEnabled")),
        (d3d::ALPHAFUNC, n("AlphaCompareFunc") + 1),
        (d3d::ALPHAREF, n("AlphaTestingThreshold")),
    ]
}

fn layer_tss(l: &Item) -> Vec<(u32, u32)> {
    let n = |k: &str| l.num(k) as u32;
    vec![
        (d3d::ADDRESSU, n("ADDRESSU") + 1),
        (d3d::ADDRESSV, n("ADDRESSV") + 1),
        (d3d::MINFILTER, n("MINFILTER")),
        (d3d::MAGFILTER, n("MAGFILTER")),
        (d3d::MIPFILTER, n("MIPFILTER")),
        (d3d::MAXANISOTROPY, n("MAXANISOTROPY").max(1)),
        (d3d::BUMPENVMAT00, n("BUMPENVMAT00")),
        (d3d::BUMPENVMAT01, n("BUMPENVMAT01")),
        (d3d::BUMPENVMAT10, n("BUMPENVMAT10")),
        (d3d::BUMPENVMAT11, n("BUMPENVMAT11")),
        (d3d::BUMPENVLSCALE, n("BUMPENVLSCALE")),
        (d3d::BUMPENVLOFFSET, n("BUMPENVLOFFSET")),
    ]
}

impl Demo {
    pub fn new(gpu: &Gpu, dev: &mut Device, files: &Files) -> Result<Self, Error> {
        let world = script::parse(&script::decode(
            files.get("reefdemoscene.w")?,
            script::WORLD_KEY,
        ))?;
        let shaders_item = script::parse(&script::decode(
            files.get("reefdemoscene.s")?,
            script::SHADER_KEY,
        ))?;
        let mut textures: HashMap<String, TextureId> = HashMap::new();
        let mut programs: HashMap<(String, String, [bool; 4]), usize> = HashMap::new();
        let mut shaders = HashMap::new();
        let mut world_shader = None;
        for s in shaders_item.all("Shader") {
            let name = s.text("Name");
            let mut passes = Vec::new();
            for p in s.all("Pass") {
                let mut layers = Vec::new();
                let mut volumes = [false; 4];
                for l in p.all("Layer") {
                    let stage = l.num("Stage") as usize;
                    let file = l.text("TextureFileName");
                    let volume = l.num("TextureType") as u32 == 1;
                    if stage < 4 {
                        volumes[stage] = volume;
                    }
                    let texture = if file.is_empty() {
                        None
                    } else {
                        let key = format!("{file}#{volume}");
                        match textures.get(&key) {
                            Some(&t) => Some(t),
                            None => match load_texture(
                                gpu,
                                dev,
                                files,
                                &file,
                                volume,
                                l.num("NumSlices") as usize,
                                l.num("NumMipmapLevels") as usize,
                            ) {
                                Ok(t) => {
                                    textures.insert(key, t);
                                    Some(t)
                                }
                                Err(_) => None,
                            },
                        }
                    };
                    layers.push(Layer {
                        stage,
                        texture,
                        volume,
                        tss: layer_tss(l),
                    });
                }
                let vs_name = p.text("VertexShader").to_ascii_lowercase();
                let ps_name = p.text("PixelShader").to_ascii_lowercase();
                let vs_src = String::from_utf8_lossy(
                    files.get(&vs_name).unwrap_or(b"vs.1.1\nmov oPos, v0\n"),
                )
                .into_owned();
                let ps_src =
                    String::from_utf8_lossy(files.get(&ps_name).unwrap_or(b"ps.1.1\nmov r0, v0\n"))
                        .into_owned();
                let key = (vs_name.clone(), ps_name.clone(), volumes);
                let program = match programs.get(&key) {
                    Some(&p) => p,
                    None => {
                        let w = shader::program(&vs_src, &ps_src, volumes)
                            .map_err(|e| format!("{vs_name} {ps_name}: {e}"))?;
                        let id = dev.program(gpu, &w);
                        programs.insert(key, id);
                        id
                    }
                };
                let weights = ["v1.w", "v1.z", "v1.y", "v1.x"]
                    .iter()
                    .position(|w| vs_src.contains(w))
                    .map_or(0, |i| 4 - i);
                passes.push(Pass {
                    skip: p.num("SkipPass") != 0.0,
                    skip_bunny: p
                        .get("DataBunnyAssignments")
                        .map(|d| d.text("SkipPass"))
                        .filter(|s| !s.is_empty()),
                    states: pass_states(p),
                    vs: constants(p.get("VertexShaderConstants")),
                    ps: constants(p.get("PixelShaderConstants")),
                    layers,
                    program,
                    volumes,
                    weights,
                });
            }
            let def = ShaderDef { passes };
            if name == "WorldShader" {
                world_shader = Some(def.clone());
            }
            shaders.insert(name, def);
        }
        let mut actors = Vec::new();
        let mut relations = Vec::new();
        let mut bunnies = None;
        for item in &world.children {
            let kind = match item.name.as_str() {
                "TDRG_MeshActor" => Kind::Mesh,
                "TDRG_LogoActor" => Kind::Logo,
                "TDRG_CameraActor" => Kind::Camera,
                "TDRG_RelationshipObject" => {
                    let pair = |a: &str, b: &str| (item.text(a), item.text(b));
                    relations.push(Relation {
                        dependant: item.text("DependantActorName"),
                        position: pair("PositionTargetActorName", "PositionTargetBoneName"),
                        look_at: pair("LookAtTargetActorName", "LookAtTargetBoneName"),
                        rotation: pair("RotationTargetActorName", "RotationTargetBoneName"),
                    });
                    continue;
                }
                "DataBunnyManager" => {
                    bunnies = Some(item.text("xfile"));
                    continue;
                }
                _ => continue,
            };
            let name = item.text("Name");
            let sort = name
                .strip_prefix("[Sort:")
                .and_then(|r| r.get(..2))
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let v3 = |k: &str| {
                let n = item.nums(k);
                Vec3::new(
                    n.first().copied().unwrap_or(0.0),
                    n.get(1).copied().unwrap_or(0.0),
                    n.get(2).copied().unwrap_or(0.0),
                )
            };
            let mut shaders_of = HashMap::new();
            if let Some(sa) = item.get("ShaderAssignment") {
                for c in &sa.children {
                    let key = c
                        .name
                        .split_once('.')
                        .map_or(c.name.as_str(), |k| k.1)
                        .to_ascii_lowercase();
                    shaders_of.insert(
                        key,
                        c.value.as_ref().map(|v| v.join(" ")).unwrap_or_default(),
                    );
                }
            }
            let model =
                Some(item.text("MeshFilename").to_ascii_lowercase()).filter(|m| !m.is_empty());
            actors.push(Actor {
                name,
                kind,
                sort,
                model,
                shaders: shaders_of,
                position: v3("Position"),
                rotation: v3("Rotation"),
                scale: if item.get("Scale").is_some() {
                    v3("Scale")
                } else {
                    Vec3::ONE
                },
                look_at: v3("LookAt"),
                offset: item.num("AnimationOffset"),
                render: item.get("Render").is_none_or(|_| item.num("Render") != 0.0),
                near: item.num("Near"),
                far: item.num("Far"),
                fov: item.num("FOV"),
                world: Mat4::IDENTITY,
                frames: Vec::new(),
                target: Vec3::ZERO,
            });
        }
        // Drawn in the order of their [Sort:nn] names, logos (screen overlays drawn without
        // depth, the Parhelia logo and the fade) over everything.
        actors.sort_by_key(|a| (a.kind == Kind::Logo, a.sort));
        let mut models = HashMap::new();
        let mut names: Vec<String> = actors.iter().filter_map(|a| a.model.clone()).collect();
        if let Some(b) = &bunnies {
            names.push(b.to_ascii_lowercase());
        }
        names.sort();
        names.dedup();
        for name in names {
            let Ok(d) = files.get(&name) else { continue };
            let model = Model::read(d).map_err(|e| format!("{name}: {e}"))?;
            models.insert(name, prepare(gpu, dev, model, &actors, &shaders));
        }
        let cameras: Vec<usize> = actors
            .iter()
            .enumerate()
            .filter(|a| a.1.kind == Kind::Camera)
            .map(|a| a.0)
            .collect();
        Ok(Self {
            shaders,
            models,
            actors,
            relations,
            world_shader,
            bunnies: bunnies.map(|b| b.to_ascii_lowercase()),
            camera: 0,
            cameras,
            time: 0.0,
            paused: false,
            still: false,
            cues: Vec::new(),
            started: false,
            frame_count: 0,
            vc: [[0.0; 4]; 96],
            pc: [[0.0; 4]; 8],
        })
    }

    /// Every drawn actor's meshes and the shader each material gets, for checking.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        for a in &self.actors {
            let Some(m) = a.model.as_ref() else { continue };
            let Some(md) = self.models.get(m) else {
                out += &format!("{} {m}: no model\n", a.name);
                continue;
            };
            for mesh in &md.meshes {
                for (j, n) in mesh.names.iter().enumerate() {
                    let k = format!("{}{n}", mesh.key);
                    let s = a.shaders.get(&k).map_or("-", |s| s.as_str());
                    let ok = self.shaders.contains_key(s);
                    out += &format!(
                        "{} {m} {k} -> {s}{} batches {}\n",
                        a.name,
                        if ok { "" } else { " (none)" },
                        mesh.materials[j].len()
                    );
                }
            }
            for k in a.shaders.keys() {
                if !md
                    .meshes
                    .iter()
                    .any(|mesh| mesh.names.iter().any(|n| format!("{}{n}", mesh.key) == *k))
                {
                    out += &format!("{} {m}: unused {k}\n", a.name);
                }
            }
        }
        out
    }

    pub fn step(&mut self, dt: f64) {
        if !self.started {
            self.started = true;
            self.cues.push("music".into());
        }
        if self.paused || self.still {
            return;
        }
        self.time += dt;
        if self.time >= LOOP {
            self.time -= LOOP;
            self.cues.push("music".into());
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.time = seconds % LOOP;
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        self.still = true;
        self.started = true;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key {
            " " => self.paused = !self.paused,
            "r" => {
                self.time = 0.0;
                self.cues.push("music".into());
            }
            // Cycles through the cameras (cApplicationCamera::SwitchCamera, 0x4013b0).
            "c" => self.camera = (self.camera + 1) % self.cameras.len().max(1),
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}
    pub fn wheel(&mut self, _amount: f64) {}
    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }
    pub fn slider(&self, _i: usize) -> f32 {
        0.0
    }
    pub fn set_slider(&mut self, _i: usize, _v: f32) {}

    pub fn report(&self) -> String {
        let cam = self
            .cameras
            .get(self.camera)
            .map_or("none", |&i| self.actors[i].name.as_str());
        let at = self
            .cameras
            .get(self.camera)
            .map(|&i| (self.actors[i].position, self.actors[i].target));
        format!("time {:.2} camera {cam} {at:?}", self.time)
    }

    /// A frame's world matrix in an actor, by name.
    fn bone(&self, actor: &str, bone: &str) -> Option<Mat4> {
        let a = self.actors.iter().find(|a| a.name == actor)?;
        let m = self.models.get(a.model.as_ref()?)?;
        let i = m.model.frames.iter().position(|f| f.name == bone)?;
        // Bones are taken in the model's own space: helpers carry leftover places and turns
        // that the tour does not use (their orientation mode, 5, takes them from the file).
        Some(a.frames[i])
    }

    fn animate(&mut self) {
        let t = self.time as f32;
        for a in &mut self.actors {
            // S, then Z, Y and X turns in degrees, then the place (0x46f9c0).
            let r = a.rotation * std::f32::consts::PI / 180.0;
            a.world = Mat4::from_translation(a.position)
                * Mat4::from_rotation_x(r.x)
                * Mat4::from_rotation_y(r.y)
                * Mat4::from_rotation_z(r.z)
                * Mat4::from_scale(a.scale);
            a.target = a.look_at;
            let Some(m) = a.model.as_ref().and_then(|n| self.models.get(n)) else {
                continue;
            };
            let ms = if m.length > 0.0 {
                ((t + a.offset) * 1000.0).rem_euclid(m.length)
            } else {
                0.0
            };
            a.frames = frames_at(m, ms);
        }
        // Relationships: cameras and fish that ride bones of animated helpers.
        let mut updates = Vec::new();
        for r in &self.relations {
            let p = self.bone(&r.position.0, &r.position.1);
            let l = self.bone(&r.look_at.0, &r.look_at.1);
            let q = self.bone(&r.rotation.0, &r.rotation.1);
            updates.push((r.dependant.clone(), p, l, q));
        }
        for (name, p, l, q) in updates {
            let Some(a) = self.actors.iter_mut().find(|a| a.name == name) else {
                continue;
            };
            if a.kind == Kind::Camera {
                if let Some(p) = p {
                    a.position = p.w_axis.truncate();
                }
                if let Some(l) = l {
                    a.target = l.w_axis.truncate();
                }
            } else {
                let mut w = q.unwrap_or(Mat4::IDENTITY);
                if let Some(p) = p {
                    w.w_axis = p.w_axis;
                }
                a.world = w * Mat4::from_scale(a.scale);
            }
        }
    }

    fn bunny(&self, name: &str) -> f32 {
        // "Anim_Fade.x_Fade_Translation_float_x": the frame DataBunny_Fade_float_1, its x.
        let Some(file) = &self.bunnies else {
            return 0.0;
        };
        let Some(m) = self.models.get(file) else {
            return 0.0;
        };
        let rest = name.split_once(".x_").map_or(name, |r| r.1);
        let mut parts = rest.split('_');
        let base = parts.next().unwrap_or("");
        let ty = rest.split('_').nth(2).unwrap_or("float");
        let frame = format!("DataBunny_{base}_{ty}_1");
        let Some(i) = m.model.frames.iter().position(|f| f.name == frame) else {
            return 0.0;
        };
        let ms = ((self.time as f32) * 1000.0).rem_euclid(m.length.max(1.0));
        frames_at(m, ms)[i].w_axis.x
    }

    fn evaluate(&self, list: &[Const], file: &mut [[f32; 4]], ctx: &Ctx) {
        for c in list {
            let mut set = |r: usize, comp: usize, v: f32| {
                if let Some(x) = file.get_mut(r).and_then(|x| x.get_mut(comp)) {
                    *x = v;
                }
            };
            match &c.value {
                Value::Unset => {}
                Value::Literal(v) => set(c.reg, c.comp, *v),
                Value::Variable(k) => set(c.reg, c.comp, ctx.var(*k)),
                Value::Wave(f, p) => {
                    let g = |i: usize| {
                        if p[i].0 {
                            ctx.var(p[i].1 as u32)
                        } else {
                            p[i].1
                        }
                    };
                    let a = g(0);
                    let mut x = g(1);
                    x *= g(2);
                    x += g(3);
                    let off = g(4);
                    // 0x49a290. A/x with x zero and ln of x not above zero leave the register.
                    let v = match f {
                        0 => x.sin() * a + off,
                        1 => x.cos() * a + off,
                        2 if x != 0.0 => a / x + off,
                        3 => x.exp() * a + off,
                        4 if x > 0.0 => x.ln() * a + off,
                        5 => x.abs() * a + off,
                        // The C library's fmod (its table at 0x569560 is named so): the scroll
                        // of the caustics and the logo's shimmer wrap at A.
                        6 if a == 0.0 => 0.0,
                        6 => x % a + off,
                        7 => x * a + off,
                        8 => x * x * a + off,
                        _ => continue,
                    };
                    set(c.reg, c.comp, v);
                }
                Value::Position(k) => {
                    let p = match k {
                        0 => ctx.eye.extend(1.0),
                        1 => ctx.world.w_axis,
                        2 => ctx.target.extend(1.0),
                        _ => Vec4::ZERO,
                    };
                    if let Some(x) = file.get_mut(c.reg) {
                        *x = p.to_array();
                    }
                }
                Value::Matrix(k, n) => {
                    let m = match k {
                        0 => ctx.world,
                        1 => ctx.view,
                        2 => ctx.proj,
                        3 => ctx.view * ctx.world,
                        4 => ctx.proj * ctx.view,
                        5 => ctx.proj * ctx.view * ctx.world,
                        6 => ctx
                            .frames
                            .get(*n as usize)
                            .map_or(Mat4::IDENTITY, |f| ctx.world * *f),
                        _ => Mat4::IDENTITY,
                    };
                    // Rows of the D3D matrix are the shader's columns: our matrix's rows.
                    for r in 0..4 {
                        if let Some(x) = file.get_mut(c.reg + r) {
                            *x = m.row(r).to_array();
                        }
                    }
                }
                Value::Palette(n) => {
                    for (b, m) in ctx.palette.iter().enumerate().take(*n as usize) {
                        for r in 0..4 {
                            if let Some(x) = file.get_mut(c.reg + b * 4 + r) {
                                *x = m.row(r).to_array();
                            }
                        }
                    }
                }
                Value::Bunny(name) => set(c.reg, c.comp, self.bunny(name)),
            }
        }
    }

    pub fn draw(&mut self, _gpu: &Gpu, dev: &mut Device) {
        self.animate();
        self.frame_count += 1;
        let (w, h) = dev.size();
        let cam_i = self.cameras.get(self.camera).copied();
        let (eye, target, fov, near, far) = cam_i.map_or(
            (Vec3::new(0.0, 50.0, -100.0), Vec3::ZERO, 45.0, 0.5, 2000.0),
            |i| {
                let c = &self.actors[i];
                (c.position, c.target, c.fov, c.near, c.far)
            },
        );
        let view = d3d::look_at_lh(eye, target, Vec3::Y);
        let proj = d3d::perspective_lh(
            fov.to_radians(),
            w as f32 / h.max(1) as f32,
            near.max(0.01),
            far.max(1.0),
        );
        dev.clear(7, 0, 1.0, 0);
        dev.set_view(view);
        dev.set_projection(proj);
        let base = Ctx {
            time: self.time as f32,
            frame: self.frame_count,
            eye,
            target,
            view,
            proj,
            world: Mat4::IDENTITY,
            frames: Vec::new(),
            palette: Vec::new(),
            size: (w as f32, h as f32),
        };
        // The world shader's constants are set first and stay unless a pass sets its own.
        let mut vc = [[0.0f32; 4]; 96];
        let mut pc = [[0.0f32; 4]; 8];
        if let Some(ws) = &self.world_shader {
            for p in &ws.passes {
                self.evaluate(&p.vs, &mut vc, &base);
                self.evaluate(&p.ps, &mut pc, &base);
            }
        }
        for ai in 0..self.actors.len() {
            let a = &self.actors[ai];
            if a.kind == Kind::Camera || !a.render {
                continue;
            }
            let Some(m) = a.model.as_ref().and_then(|n| self.models.get(n)) else {
                continue;
            };
            for mesh in &m.meshes {
                let world = match mesh.frame {
                    Some(f) if mesh.skins.is_empty() => a.world * a.frames[f],
                    _ => a.world,
                };
                for (mat, batches) in mesh.materials.iter().enumerate() {
                    let Some(sname) = a.shaders.get(&format!("{}{}", mesh.key, mesh.names[mat]))
                    else {
                        continue;
                    };
                    let Some(def) = self.shaders.get(sname) else {
                        continue;
                    };
                    for pass in &def.passes {
                        let skip = match &pass.skip_bunny {
                            Some(b) => self.bunny(b) != 0.0,
                            None => pass.skip,
                        };
                        if skip {
                            continue;
                        }
                        for (k, v) in &pass.states {
                            dev.set_render_state(*k, *v);
                        }
                        dev.set_render_state(d3d::ALPHATESTENABLE, 0);
                        for s in 0..4 {
                            dev.set_texture(s, None);
                        }
                        let mut bump = [[0.0f32; 4]; 4];
                        let mut bumpl = [[0.0f32; 4]; 4];
                        for l in &pass.layers {
                            if l.stage >= 4 {
                                continue;
                            }
                            dev.set_texture(l.stage, l.texture);
                            for (k, v) in &l.tss {
                                dev.set_texture_stage_state(l.stage, *k, *v);
                            }
                            let f = |k: u32| {
                                f32::from_bits(l.tss.iter().find(|x| x.0 == k).map_or(0, |x| x.1))
                            };
                            bump[l.stage] = [
                                f(d3d::BUMPENVMAT00),
                                f(d3d::BUMPENVMAT01),
                                f(d3d::BUMPENVMAT10),
                                f(d3d::BUMPENVMAT11),
                            ];
                            bumpl[l.stage] =
                                [f(d3d::BUMPENVLSCALE), f(d3d::BUMPENVLOFFSET), 0.0, 0.0];
                            let _ = l.volume;
                        }
                        let mut ctx = Ctx {
                            world,
                            frames: a.frames.clone(),
                            ..base.clone()
                        };
                        for b in batches {
                            ctx.palette = b
                                .bones
                                .iter()
                                .map(|&s| {
                                    let (f, offset) = mesh.skins[s];
                                    let bone = f.map_or(Mat4::IDENTITY, |f| a.frames[f]);
                                    a.world * bone * offset
                                })
                                .collect();
                            let mut vcf = vc;
                            let mut pcf = pc;
                            self.evaluate(&pass.vs, &mut vcf, &ctx);
                            self.evaluate(&pass.ps, &mut pcf, &ctx);
                            let consts = ProgConsts {
                                c: vcf,
                                pc: pcf,
                                bump,
                                bumpl,
                                alpha: [0.0; 4],
                                misc: [0.0; 4],
                            };
                            dev.draw_program(
                                pass.program,
                                b.vertices,
                                b.indices,
                                0..b.count,
                                pass.volumes,
                                &consts,
                            );
                        }
                        let _ = pass.weights;
                    }
                }
            }
        }
        self.vc = vc;
        self.pc = pc;
        for s in 0..4 {
            dev.set_texture(s, None);
        }
    }
}

#[derive(Clone)]
struct Ctx {
    time: f32,
    frame: u32,
    eye: Vec3,
    target: Vec3,
    view: Mat4,
    proj: Mat4,
    world: Mat4,
    frames: Vec<Mat4>,
    palette: Vec<Mat4>,
    size: (f32, f32),
}

impl Ctx {
    /// 0x49a160: 0 time in seconds, 1 frames drawn, 3 the back buffer's height (+0x254) and
    /// 4 its width (+0x250).
    fn var(&self, k: u32) -> f32 {
        match k {
            0 => self.time,
            1 => self.frame as f32,
            3 => self.size.1,
            4 => self.size.0,
            _ => 0.0,
        }
    }
}

/// The world matrix of every frame of a model at `ms` milliseconds, as offsets from the
/// model's root: each frame's local matrix (its track's keys, or its own) times its parent's.
fn frames_at(m: &ModelData, ms: f32) -> Vec<Mat4> {
    let mut out: Vec<Mat4> = Vec::with_capacity(m.model.frames.len());
    for (i, f) in m.model.frames.iter().enumerate() {
        let local = match m.tracks[i] {
            Some(t) => track_matrix(&m.model.tracks[t], f.matrix, ms),
            None => f.matrix,
        };
        let parent = f.parent.map_or(Mat4::IDENTITY, |p| out[p]);
        out.push(parent * local);
    }
    out
}

fn sample<T: Copy>(keys: &[(f32, T)], ms: f32, mix: impl Fn(T, T, f32) -> T) -> Option<T> {
    let first = keys.first()?;
    if ms <= first.0 || keys.len() == 1 {
        return Some(first.1);
    }
    let i = keys.partition_point(|k| k.0 <= ms);
    if i >= keys.len() {
        return keys.last().map(|k| k.1);
    }
    let (a, b) = (keys[i - 1], keys[i]);
    let t = (ms - a.0) / (b.0 - a.0).max(1e-6);
    Some(mix(a.1, b.1, t))
}

fn track_matrix(t: &crate::xfile::Track, base: Mat4, ms: f32) -> Mat4 {
    if let Some(m) = sample(&t.matrix, ms, |a, _, _| a) {
        return m;
    }
    let (bs, br, bt) = base.to_scale_rotation_translation();
    let s = sample(&t.scale, ms, |a: Vec3, b, k| a.lerp(b, k)).unwrap_or(bs);
    // D3DXMatrixRotationQuaternion of q, in D3D's row layout, is glam's matrix of q.
    let r = sample(&t.rotation, ms, |a: Quat, b, k| a.slerp(b, k)).unwrap_or(br);
    let p = sample(&t.position, ms, |a: Vec3, b, k| a.lerp(b, k)).unwrap_or(bt);
    Mat4::from_scale_rotation_translation(s, r, p)
}

/// Splits a model's meshes by material and into palettes of bones, with the vertex layout the
/// shaders read, and puts them on the GPU.
fn prepare(
    gpu: &Gpu,
    dev: &mut Device,
    mut model: Model,
    actors: &[Actor],
    shaders: &HashMap<String, ShaderDef>,
) -> ModelData {
    // Tracks by frame, merged (an Animation per kind of key).
    let mut merged: Vec<crate::xfile::Track> = Vec::new();
    for t in std::mem::take(&mut model.tracks) {
        match merged.iter_mut().find(|m| m.frame == t.frame) {
            Some(m) => {
                m.rotation.extend(t.rotation);
                m.position.extend(t.position);
                m.scale.extend(t.scale);
                m.matrix.extend(t.matrix);
            }
            None => merged.push(t),
        }
    }
    let length = merged
        .iter()
        .flat_map(|t| {
            t.rotation
                .iter()
                .map(|k| k.0)
                .chain(t.position.iter().map(|k| k.0))
                .chain(t.scale.iter().map(|k| k.0))
        })
        .fold(0.0f32, f32::max);
    model.tracks = merged;
    let tracks = model
        .frames
        .iter()
        .map(|f| model.tracks.iter().position(|t| t.frame == f.name))
        .collect();
    let weights_wanted = |key: &str| -> usize {
        // The most blend weights any shader on this mesh reads.
        let mut w = 0;
        for a in actors {
            for (k, s) in &a.shaders {
                if k.starts_with(key)
                    && let Some(d) = shaders.get(s)
                {
                    w = w.max(d.passes.iter().map(|p| p.weights).max().unwrap_or(0));
                }
            }
        }
        w.max(1)
    };
    let frame_index = |name: &str| model.frames.iter().position(|f| f.name == name);
    let mut meshes = Vec::new();
    for (mi, x) in model.meshes.iter().enumerate() {
        let frame = frame_index(&x.frame);
        let fname = if x.frame.is_empty() {
            "Frame_0"
        } else {
            &x.frame
        };
        let key = format!("{fname}.Mesh_{mi}.").to_ascii_lowercase();
        let names = (0..x.materials.len().max(1))
            .map(|j| match x.materials.get(j).map(|m| m.name.as_str()) {
                Some(n) if !n.is_empty() => n.to_ascii_lowercase(),
                _ => format!("shader_{j}"),
            })
            .collect();
        let skins: Vec<(Option<usize>, Mat4)> = x
            .skins
            .iter()
            .map(|s| (frame_index(&s.bone), s.offset))
            .collect();
        let materials = build_batches(gpu, dev, x, weights_wanted(&key));
        meshes.push(MeshData {
            frame,
            key,
            names,
            materials,
            skins,
        });
    }
    ModelData {
        model,
        meshes,
        tracks,
        length,
    }
}

/// Per-vertex tangents from the texture coordinates, as D3DXComputeTangent makes them.
fn tangents(x: &XMesh) -> Vec<Vec3> {
    let mut t = vec![Vec3::ZERO; x.positions.len()];
    if x.uvs.len() != x.positions.len() {
        return vec![Vec3::X; x.positions.len()];
    }
    for (tri, _) in &x.triangles {
        let [a, b, c] = tri.map(|i| i as usize);
        if a.max(b).max(c) >= x.positions.len() {
            continue;
        }
        let (p0, p1, p2) = (x.positions[a], x.positions[b], x.positions[c]);
        let (u0, u1, u2) = (x.uvs[a], x.uvs[b], x.uvs[c]);
        let (e1, e2) = (p1 - p0, p2 - p0);
        let (du1, dv1, du2, dv2) = (u1[0] - u0[0], u1[1] - u0[1], u2[0] - u0[0], u2[1] - u0[1]);
        let r = du1 * dv2 - du2 * dv1;
        if r.abs() < 1e-12 {
            continue;
        }
        let tan = (e1 * dv2 - e2 * dv1) / r;
        for i in [a, b, c] {
            t[i] += tan;
        }
    }
    t.into_iter()
        .zip(&x.normals)
        .map(|(t, n)| {
            let t = (t - *n * n.dot(t)).normalize_or_zero();
            if t == Vec3::ZERO {
                n.any_orthonormal_vector()
            } else {
                t
            }
        })
        .collect()
}

fn build_batches(gpu: &Gpu, dev: &mut Device, x: &XMesh, weights: usize) -> Vec<Vec<Batch>> {
    let n = x.positions.len();
    // Each vertex's strongest influences, as (skin, weight).
    let mut influences: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
    for (s, skin) in x.skins.iter().enumerate() {
        for (&v, &w) in skin.vertices.iter().zip(&skin.weights) {
            if let Some(list) = influences.get_mut(v as usize)
                && w > 0.0
            {
                list.push((s, w));
            }
        }
    }
    let keep = weights.clamp(1, 4);
    for list in &mut influences {
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.truncate(keep);
        let sum: f32 = list.iter().map(|l| l.1).sum();
        if sum > 0.0 {
            for l in list.iter_mut() {
                l.1 /= sum;
            }
        }
    }
    let tangents = tangents(x);
    let count = x.materials.len().max(1);
    let mut out: Vec<Vec<Batch>> = (0..count).map(|_| Vec::new()).collect();
    for (mat, batches) in out.iter_mut().enumerate() {
        let tris: Vec<&[u32; 3]> = x
            .triangles
            .iter()
            .filter(|(_, f)| {
                x.face_materials.get(*f).copied().unwrap_or(0) as usize == mat
                    || x.face_materials.is_empty() && mat == 0
            })
            .map(|(t, _)| t)
            .collect();
        if tris.is_empty() {
            continue;
        }
        // Greedy palettes of at most PALETTE bones, as D3DX's indexed skinning splits.
        let mut groups: Vec<(Vec<usize>, Vec<[u32; 3]>)> = Vec::new();
        for t in tris {
            let mut need: Vec<usize> = t
                .iter()
                .flat_map(|&v| {
                    influences
                        .get(v as usize)
                        .into_iter()
                        .flatten()
                        .map(|i| i.0)
                })
                .collect();
            need.sort_unstable();
            need.dedup();
            let fits = |g: &(Vec<usize>, Vec<[u32; 3]>)| {
                let extra = need.iter().filter(|b| !g.0.contains(b)).count();
                g.0.len() + extra <= PALETTE
            };
            let g = match groups.iter().position(fits) {
                Some(i) => &mut groups[i],
                None => {
                    groups.push((Vec::new(), Vec::new()));
                    groups.last_mut().expect("pushed")
                }
            };
            for b in need {
                if !g.0.contains(&b) {
                    g.0.push(b);
                }
            }
            g.1.push(*t);
        }
        for (bones, tris) in groups {
            let mut map: HashMap<u32, u32> = HashMap::new();
            let mut verts = Vec::new();
            let mut idx = Vec::new();
            for t in &tris {
                for &v in t {
                    let id = *map.entry(v).or_insert_with(|| {
                        let vi = v as usize;
                        let p = x.positions.get(vi).copied().unwrap_or(Vec3::ZERO);
                        let nn = x.normals.get(vi).copied().unwrap_or(Vec3::Y);
                        let set = |k: usize| x.uv_sets.get(k).and_then(|s| s.get(vi)).copied();
                        let uv = set(0).unwrap_or([0.0; 2]);
                        let uv1 = set(1).unwrap_or(uv);
                        let uv2 = set(2).unwrap_or([0.0; 2]);
                        let uv3 = set(3).unwrap_or([0.0; 2]);
                        let col = x.colors.get(vi).copied().unwrap_or([1.0; 4]);
                        let tan = tangents.get(vi).copied().unwrap_or(Vec3::X);
                        let bin = nn.cross(tan);
                        let mut w = [0.0f32; 4];
                        let mut ix = [0.0f32; 4];
                        for (k, (s, wt)) in influences[vi].iter().enumerate() {
                            w[k] = *wt;
                            ix[k] = bones.iter().position(|b| b == s).unwrap_or(0) as f32;
                        }
                        if influences[vi].is_empty() {
                            w[0] = 1.0;
                        }
                        let id = verts.len() as u32;
                        verts.push(PVertex {
                            r: [
                                [p.x, p.y, p.z, 1.0],
                                w,
                                ix,
                                [nn.x, nn.y, nn.z, 0.0],
                                col,
                                [0.0, 0.0, 0.0, 0.0],
                                [uv[0], uv[1], 0.0, 1.0],
                                [uv1[0], uv1[1], 0.0, 1.0],
                                [tan.x, tan.y, tan.z, 1.0],
                                [bin.x, bin.y, bin.z, 1.0],
                                [uv2[0], 0.0, 0.0, 1.0],
                                [uv3[0], 0.0, 0.0, 1.0],
                            ],
                        });
                        id
                    });
                    idx.push(id);
                }
            }
            let vertices = dev.program_vertices(gpu, &verts);
            let indices = dev.program_indices(gpu, &idx);
            batches.push(Batch {
                vertices,
                indices,
                count: idx.len() as u32,
                bones,
            });
        }
    }
    out
}

#[allow(dead_code)]
fn unused(_: Image) {}
