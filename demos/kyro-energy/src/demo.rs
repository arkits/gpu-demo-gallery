//! Kyro Energy (STMicroelectronics, 2002): a cold bottle on a table, condensation running
//! down it. The drops are drawn each step into a bump map with luminance and a light map
//! (`droplets.rs`); the bottle takes its label, the light map, an environment-bump stage with
//! those drops and an added reflection. The table is half transparent over a mirrored copy of
//! the scene, masked by the stencil. Everything is in `energy.exe` (unpacked from UPX).
use glam::{Mat4, Vec3, Vec4};

use crate::Error;
use crate::d3d::{self, Device, Gpu, Light, Source, TextureId, Vertex};
use crate::data::Files;
use crate::droplets::{Droplets, Sprite};
use crate::kyro::{self, Camera, CameraAnim, CameraSrc, Exe, LightAnim, Mesh, MeshAnim};

pub const FILES: &[&str] = &["energy.exe"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Pause"),
    ("r", "Restart"),
    ("t", "Drier"),
    ("g", "Wetter"),
    ("c", "Credits"),
];
pub const SLIDERS: &[(&str, f32, f32)] = &[];

/// The scene's tables (init 0x403000).
const FPS: f64 = 30.0;
const FRAMES: i32 = 600;
const LIGHTS: (u32, u32, usize) = (0x49c900, 0x4a0268, 3);
const MESHES: (u32, usize) = (0x4e6ba0, 12);
const MATERIALS: (u32, usize) = (0x49c770, 5);
const MESH_ANIMS: u32 = 0x4e72c0;
const CAMERAS: (u32, u32, usize) = (0x4a0298, 0x4ecb08, 1);
/// The drops move on calls of the frame function; the port calls it this often.
const DROP_RATE: f64 = 60.0;

/// Texture coordinate kinds (mesh struct +0xe4) and material map slots (0x421fa0).
const DIFFUSE: usize = 1;
const ENV_BUMP: usize = 4;
const LIGHTMAP: usize = 5;
const BUMP: usize = 8;
const ENV: usize = 9;

/// Modes the materials get in style 0 (0x421fa0): diffuse; diffuse, light map, bump with
/// environment; diffuse and environment.
const DEFAULT: u32 = 0;
const BUMPED: u32 = 0x0f;
const DIFFENV: u32 = 0x27;

struct Part {
    mesh: Mesh,
    anim: MeshAnim,
    index: [Option<u32>; 12],
    vertices: Vec<Vertex>,
    mode: u32,
    /// Lit by its light map instead of D3D (flag 0x100).
    unlit: bool,
    pose: kyro::Pose,
}

#[derive(Debug, Clone, Copy, Default)]
struct FadeState {
    on: bool,
    alpha: f32,
    rate: f32,
    left: f32,
    color: u32,
    texture: Option<TextureId>,
}

pub struct Demo {
    parts: Vec<Part>,
    materials: Vec<kyro::Material>,
    /// Each material's maps by slot.
    textures: Vec<[Option<TextureId>; 12]>,
    lights: Vec<(kyro::LightSrc, LightAnim)>,
    camera: (CameraSrc, CameraAnim),
    drops: Droplets,
    bump: TextureId,
    lightmap: TextureId,
    dirty: bool,
    credits: TextureId,
    fade: FadeState,
    time: f64,
    drop_time: f64,
    paused: bool,
    still: bool,
}

impl Demo {
    pub fn new(gpu: &Gpu, dev: &mut Device, files: &Files) -> Result<Self, Error> {
        let exe = Exe::new(files.get("energy.exe")?.to_vec())?;
        let res = exe.resources();
        let picture = |name: &str| kyro::picture(&res, name);
        let drops = Droplets::new(
            (1..=4)
                .map(|i| picture(&format!("drop{i}_16x16.bmp")).map(|p| Sprite::height(&p)))
                .collect::<Result<_, _>>()?,
            (1..=4)
                .map(|i| picture(&format!("lightmap{i}_16x16.bmp")).map(|p| Sprite::rgb565(&p)))
                .collect::<Result<_, _>>()?,
            Sprite::height(&picture("staticDrop.bmp")?),
            Sprite::rgb565(&picture("staticDropLightmap.bmp")?),
        );
        let bump = dev.texture(gpu, &drops.bump_map(), false);
        let lightmap = dev.texture(gpu, &drops.light_map(), false);
        let materials = kyro::materials(&exe, MATERIALS.0, MATERIALS.1);
        let mut textures = Vec::new();
        for m in &materials {
            let mut t = [None; 12];
            for (slot, map) in m.maps.iter().enumerate() {
                let Some(name) = map else { continue };
                // "proc" names are the procedural maps (0x421fa0): the bump map, and the
                // light map made with it.
                t[slot] = Some(if name.starts_with("proc") {
                    if slot == BUMP { bump } else { lightmap }
                } else {
                    dev.texture(gpu, &picture(name)?, true)
                });
            }
            textures.push(t);
        }
        let meshes = kyro::meshes(&exe, MESHES.0, MESHES.1);
        let anims = kyro::mesh_anims(&exe, MESH_ANIMS, MESHES.1, FRAMES as usize);
        let parts: Vec<Part> = meshes
            .into_iter()
            .zip(anims)
            .map(|(mesh, anim)| {
                let mat = &materials[mesh.material.min(materials.len() - 1)];
                let (index, channels) = layout(&mesh);
                let vertices = (0..mesh.positions.len())
                    .map(|i| {
                        let mut v = Vertex::at(mesh.positions[i]).normal(mesh.normals[i]);
                        v.diffuse = mesh.colors.as_ref().map_or(0xffff_ffff, |c| c[i]);
                        let mut c = 0;
                        for uv in mesh.uvs.iter().flatten() {
                            if c < channels {
                                v.uv[c] = uv[i];
                                c += 1;
                            }
                        }
                        v
                    })
                    .collect();
                Part {
                    mode: mode(mat),
                    unlit: mesh.uvs[BUMP].is_some() && mat.maps[LIGHTMAP].is_some(),
                    pose: kyro::Pose {
                        world: Mat4::IDENTITY,
                        hidden: mesh.hidden,
                    },
                    mesh,
                    anim,
                    index,
                    vertices,
                }
            })
            .collect();
        let sources = kyro::lights(&exe, LIGHTS.0, LIGHTS.2);
        let light_anims = kyro::light_anims(&exe, LIGHTS.1, LIGHTS.2, FRAMES as usize);
        let camera = kyro::cameras(&exe, CAMERAS.0, CAMERAS.1, CAMERAS.2, FRAMES as usize)
            .into_iter()
            .next()
            .ok_or("no camera")?;
        Ok(Self {
            parts,
            materials,
            textures,
            lights: sources.into_iter().zip(light_anims).collect(),
            camera,
            drops,
            bump,
            lightmap,
            dirty: false,
            credits: dev.texture(gpu, &picture("credits.bmp")?, true),
            fade: FadeState::default(),
            time: 0.0,
            drop_time: 0.0,
            paused: false,
            still: false,
        })
    }

    fn frame(&self) -> (i32, i32, f32, i32) {
        kyro::frame_at(self.time, FPS, FRAMES)
    }

    fn start_fade(&mut self, frames: i32, from: f32, to: f32, texture: Option<TextureId>) {
        let length = frames as f32 / FPS as f32;
        self.fade = FadeState {
            on: true,
            alpha: from,
            rate: (to - from) / length,
            left: length,
            color: 0,
            texture,
        };
    }

    fn advance(&mut self, dt: f64) {
        let f = &mut self.fade;
        if f.on {
            f.alpha = (f.alpha + dt as f32 * f.rate).clamp(0.0, 1.0);
            f.left -= dt as f32;
            if f.left < 0.0 {
                f.on = false;
            }
        }
        self.drop_time += dt;
        while self.drop_time >= 1.0 / DROP_RATE {
            self.drop_time -= 1.0 / DROP_RATE;
            self.drops.step();
            self.dirty = true;
        }
    }

    pub fn step(&mut self, dt: f64) {
        if self.paused || self.still {
            return;
        }
        self.time += dt;
        self.advance(dt);
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.time = 0.0;
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        let dt = 1.0 / DROP_RATE;
        while self.time + dt <= seconds {
            self.time += dt;
            self.advance(dt);
        }
        self.time = seconds;
        self.still = true;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key {
            " " => self.paused = !self.paused,
            "r" => {
                self.time = 0.0;
                self.fade = FadeState::default();
            }
            // T and G: luminance of the dry glass, 10 a press (0x432a30).
            "t" => self.drops.dry = (self.drops.dry + 10).min(0xff),
            "g" => self.drops.dry = (self.drops.dry - 10).max(0),
            "c" => self.start_fade(90, 0.8, 0.0, Some(self.credits)),
            _ => return false,
        }
        self.dirty = true;
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}
    pub fn wheel(&mut self, _amount: f64) {}
    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }
    pub fn slider(&self, _i: usize) -> f32 {
        0.0
    }
    pub fn set_slider(&mut self, _i: usize, _v: f32) {}

    pub fn report(&self) -> String {
        let (cur, ..) = self.frame();
        format!("frame {cur} of {FRAMES}, dry {}", self.drops.dry)
    }

    pub fn draw(&mut self, gpu: &Gpu, dev: &mut Device) {
        if self.dirty {
            dev.replace(gpu, self.bump, &self.drops.bump_map());
            dev.replace(gpu, self.lightmap, &self.drops.light_map());
            self.dirty = false;
        }
        let (cur, next, t, _) = self.frame();
        let (w, h) = dev.size();
        let cam = Camera::at(&self.camera.0, &self.camera.1, cur, next, t);
        dev.clear(7, 0, 1.0, 0);
        dev.set_view(cam.view());
        dev.set_projection(cam.projection(w as f32 / h.max(1) as f32));
        set_states(dev);
        dev.set_material(d3d::Material {
            diffuse: Vec4::ONE,
            ambient: Vec4::ONE,
            specular: Vec4::ONE,
            emissive: Vec4::ZERO,
            power: 0.0,
        });
        dev.set_render_state(d3d::SPECULARMATERIALSOURCE, 0);
        self.set_lights(dev, cur, next, t);
        for i in 0..self.parts.len() {
            let p = &mut self.parts[i];
            p.pose = p.anim.pose(&p.mesh, cur, next, t);
            if p.mesh.drawn && !p.pose.hidden {
                environment(p, cam.position);
            }
        }
        // 0x415fb0: the table into the stencil, the mirrored meshes where it is, then the rest.
        self.stencil_table(dev);
        stencil_equal(dev);
        self.draw_meshes(dev, true);
        dev.set_render_state(d3d::STENCILENABLE, 0);
        self.draw_meshes(dev, false);
        self.draw_fade(dev);
    }

    /// The three lights; in style 0 Direct01 is on and ANIM_Direct02 off (0x4167b0).
    fn set_lights(&self, dev: &mut Device, cur: i32, next: i32, t: f32) {
        for (i, (src, anim)) in self.lights.iter().enumerate().take(8) {
            let (l, on) = src.at(anim, cur, next, t);
            let c = l.color.extend(1.0);
            dev.set_light(
                i,
                Light {
                    kind: match l.kind {
                        0 => d3d::LIGHT_POINT,
                        1 => d3d::LIGHT_SPOT,
                        _ => d3d::LIGHT_DIRECTIONAL,
                    },
                    diffuse: c,
                    specular: c,
                    ambient: Vec4::ZERO,
                    position: l.position,
                    direction: l.direction,
                    range: l.range,
                    falloff: if l.kind == 1 { 1.0 } else { 0.0 },
                    attenuation: l.attenuation,
                    theta: l.theta,
                    phi: l.phi,
                },
            );
            let on = on && f32::from_bits(src.words[19]) == 1.0 && src.name != "ANIM_Direct02";
            dev.light_enable(i, on);
        }
    }

    /// 0x440980: the STENCIL_ mesh writes 1 into the stencil, colour from its vertices,
    /// no depth.
    fn stencil_table(&self, dev: &mut Device) {
        let Some(p) = self
            .parts
            .iter()
            .find(|p| p.mesh.name.starts_with("STENCIL"))
        else {
            return;
        };
        if p.pose.hidden {
            return;
        }
        for (k, v) in [
            (d3d::ZWRITEENABLE, 0),
            (d3d::STENCILENABLE, 1),
            (d3d::SHADEMODE, 1),
            (d3d::LIGHTING, 0),
            (d3d::STENCILFUNC, 8),
            (d3d::STENCILZFAIL, 1),
            (d3d::STENCILFAIL, 1),
            (d3d::STENCILREF, 1),
            (d3d::STENCILMASK, 1),
            (d3d::STENCILWRITEMASK, 1),
            (d3d::STENCILPASS, 3),
            (d3d::COLORVERTEX, 1),
        ] {
            dev.set_render_state(k, v);
        }
        dev.set_texture_stage_state(0, d3d::COLOROP, d3d::DISABLE);
        dev.set_world(p.pose.world);
        let indices: Vec<u16> = p.mesh.faces.iter().flatten().copied().collect();
        let fvf = d3d::FVF_XYZ | d3d::FVF_NORMAL | d3d::FVF_DIFFUSE;
        dev.draw(
            d3d::TRIANGLELIST,
            fvf,
            Source::Data(&p.vertices),
            Some(&indices),
        );
        dev.set_world(Mat4::IDENTITY);
        for (k, v) in [
            (d3d::ZWRITEENABLE, 1),
            (d3d::STENCILENABLE, 0),
            (d3d::SHADEMODE, 2),
            (d3d::LIGHTING, 1),
            (d3d::COLORVERTEX, 0),
        ] {
            dev.set_render_state(k, v);
        }
    }

    /// 0x4237d0: by mode and material, the meshes named REFLECTION or the others.
    fn draw_meshes(&self, dev: &mut Device, mirrored: bool) {
        dev.set_render_state(d3d::LIGHTING, 1);
        let mut order: Vec<usize> = (0..self.parts.len()).collect();
        order.sort_by_key(|&i| (self.parts[i].mode, self.parts[i].mesh.material));
        let mut mode = None;
        let mut material = None;
        for i in order {
            let p = &self.parts[i];
            if !p.mesh.drawn || p.pose.hidden || p.mesh.name.starts_with("REFLECTION") != mirrored {
                continue;
            }
            if mode != Some(p.mode) {
                begin(dev, p.mode);
                mode = Some(p.mode);
                material = None;
                // The bottle's label is not mipmapped; the rest are, linearly (0x4237d0).
                let label = self.materials[p.mesh.material].name == "Material #2";
                dev.set_texture_stage_state(0, d3d::MIPFILTER, if label { 1 } else { 3 });
            }
            if material != Some(p.mesh.material) {
                let label = self.materials[p.mesh.material].name == "Material #2";
                dev.set_texture_stage_state(0, d3d::MIPFILTER, if label { 1 } else { 3 });
                dev.set_render_state(d3d::AMBIENT, self.materials[p.mesh.material].ambient);
                let maps = &self.textures[p.mesh.material];
                for (stage, slot) in slots(p.mode).iter().enumerate() {
                    dev.set_texture(stage, maps[*slot]);
                }
                material = Some(p.mesh.material);
            }
            for (stage, kind) in kinds(p.mode).iter().enumerate() {
                dev.set_texture_stage_state(stage, d3d::TEXCOORDINDEX, p.index[*kind].unwrap_or(0));
            }
            // 0x433fb0: vertex colours mean blending by their alpha; light-mapped meshes
            // are not lit.
            let coloured = p.mesh.colors.is_some();
            dev.set_render_state(d3d::ALPHABLENDENABLE, u32::from(coloured));
            dev.set_render_state(d3d::SRCBLEND, 5);
            dev.set_render_state(d3d::DESTBLEND, 6);
            dev.set_render_state(d3d::COLORVERTEX, u32::from(coloured));
            dev.set_render_state(d3d::LIGHTING, u32::from(!p.unlit));
            dev.set_world(p.pose.world);
            let indices: Vec<u16> = p.mesh.faces.iter().flatten().copied().collect();
            let fvf = d3d::FVF_XYZ | d3d::FVF_NORMAL | d3d::FVF_DIFFUSE;
            dev.draw(
                d3d::TRIANGLELIST,
                fvf,
                Source::Data(&p.vertices),
                Some(&indices),
            );
        }
        dev.set_world(Mat4::IDENTITY);
        for (k, v) in [
            (d3d::ALPHABLENDENABLE, 0),
            (d3d::COLORVERTEX, 0),
            (d3d::LIGHTING, 1),
        ] {
            dev.set_render_state(k, v);
        }
        for s in 0..8 {
            dev.set_texture(s, None);
        }
        dev.set_texture_stage_state(0, d3d::ADDRESS, 1);
        dev.set_texture_stage_state(3, d3d::ADDRESS, 1);
    }

    fn draw_fade(&self, dev: &mut Device) {
        let f = self.fade;
        if !f.on {
            return;
        }
        for (k, v) in [
            (d3d::ALPHABLENDENABLE, 1),
            (d3d::SRCBLEND, 5),
            (d3d::DESTBLEND, 6),
            (d3d::ZENABLE, 0),
            (d3d::LIGHTING, 0),
            (d3d::COLORVERTEX, 1),
        ] {
            dev.set_render_state(k, v);
        }
        let saved = (dev.world(), dev.view(), dev.projection());
        dev.set_world(Mat4::IDENTITY);
        dev.set_view(Mat4::IDENTITY);
        dev.set_projection(Mat4::IDENTITY);
        let color = f.color.wrapping_add(((f.alpha * 255.0) as u32) << 24);
        dev.set_texture(0, f.texture);
        for (k, v) in [
            (d3d::COLOROP, d3d::SELECTARG1),
            (d3d::COLORARG1, d3d::TA_TEXTURE),
            (d3d::ALPHAOP, d3d::SELECTARG2),
            (d3d::ALPHAARG2, d3d::TA_DIFFUSE),
            (d3d::TEXCOORDINDEX, 0),
        ] {
            dev.set_texture_stage_state(0, k, v);
        }
        dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
        let corner = |x: f32, y: f32, u: f32, v: f32| {
            Vertex::at(Vec3::new(x, y, 0.0)).color(color).uv(0, u, v)
        };
        let quad = [
            corner(-1.0, -1.0, 0.0, 1.0),
            corner(-1.0, 1.0, 0.0, 0.0),
            corner(1.0, 1.0, 1.0, 0.0),
            corner(1.0, -1.0, 1.0, 1.0),
        ];
        dev.draw(
            d3d::TRIANGLEFAN,
            d3d::FVF_XYZ | d3d::FVF_DIFFUSE,
            Source::Data(&quad),
            None,
        );
        dev.set_world(saved.0);
        dev.set_view(saved.1);
        dev.set_projection(saved.2);
        for (k, v) in [
            (d3d::ALPHABLENDENABLE, 0),
            (d3d::ZENABLE, 1),
            (d3d::LIGHTING, 1),
            (d3d::COLORVERTEX, 0),
        ] {
            dev.set_render_state(k, v);
        }
    }
}

/// Reflection coordinates for the environment kinds (0x41b810): the eye in the mesh's space,
/// R = V − 2(V·N)N for the unit vector V to the eye; u = (R.x + 1) / 2, v = (1 − R.z) / 2.
fn environment(p: &mut Part, eye: Vec3) {
    let world = p.pose.world;
    let eye = if world != Mat4::IDENTITY {
        world.inverse().transform_point3(eye)
    } else {
        eye
    };
    for kind in [ENV_BUMP, ENV] {
        let Some(c) = p.index[kind] else { continue };
        for (v, pos) in p.vertices.iter_mut().zip(&p.mesh.positions) {
            let n = Vec3::from(v.normal);
            let to = (eye - *pos).normalize_or_zero();
            let d = to.dot(n);
            let r = (to - 2.0 * d * n).normalize_or_zero();
            v.uv[c as usize] = [(r.x + 1.0) * 0.5, (1.0 - r.z) * 0.5];
        }
    }
}

/// Texture coordinate sets in style 0 (0x418ad0): one for each kind the mesh has, the
/// diffuse kind at 0 if it has none.
fn layout(mesh: &Mesh) -> ([Option<u32>; 12], usize) {
    let mut index = [None; 12];
    let mut n = 0u32;
    for (k, uv) in mesh.uvs.iter().enumerate() {
        if uv.is_some() {
            index[k] = Some(n);
            n += 1;
        }
    }
    if mesh.drawn && mesh.uvs[DIFFUSE].is_none() {
        index[DIFFUSE] = Some(0);
    }
    (index, n.clamp(1, 7) as usize)
}

/// The mode of a material (0x421fa0) for the maps these have.
fn mode(m: &kyro::Material) -> u32 {
    let has = |slot: usize| m.maps[slot].is_some();
    if has(DIFFUSE) && has(BUMP) && has(ENV_BUMP) && has(LIGHTMAP) {
        BUMPED
    } else if has(DIFFUSE) && has(ENV) {
        DIFFENV
    } else {
        DEFAULT
    }
}

/// Map slots bound to stages (mat functions 0x438a20, 0x438b60, 0x439340).
fn slots(mode: u32) -> &'static [usize] {
    match mode {
        BUMPED => &[DIFFUSE, LIGHTMAP, BUMP, ENV_BUMP],
        DIFFENV => &[DIFFUSE, ENV],
        _ => &[DIFFUSE],
    }
}

/// Texture coordinate kinds of each stage (draw functions 0x434230, 0x434390, 0x437520).
fn kinds(mode: u32) -> &'static [usize] {
    match mode {
        BUMPED => &[DIFFUSE, LIGHTMAP, BUMP, ENV_BUMP],
        DIFFENV => &[DIFFUSE, ENV],
        _ => &[DIFFUSE],
    }
}

/// Stencil equal to 1, writing nothing (0x441300).
fn stencil_equal(dev: &mut Device) {
    for (k, v) in [
        (d3d::STENCILENABLE, 1),
        (d3d::STENCILREF, 1),
        (d3d::STENCILMASK, 1),
        (d3d::STENCILFUNC, 3),
        (d3d::STENCILPASS, 1),
        (d3d::STENCILZFAIL, 1),
        (d3d::STENCILFAIL, 1),
        (d3d::STENCILWRITEMASK, 0),
    ] {
        dev.set_render_state(k, v);
    }
}

/// The begin functions (0x439550, 0x439df0, 0x43cc40).
fn begin(dev: &mut Device, mode: u32) {
    let t = |dev: &mut Device, s: usize, k: u32, v: u32| dev.set_texture_stage_state(s, k, v);
    let list: &[(usize, u32, u32)] = match mode {
        BUMPED => {
            dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
            &[
                (0, 1, 4),
                (0, 2, 2),
                (0, 3, 0),
                (0, 4, 2),
                (0, 5, 2),
                (0, 6, 0),
                (1, 1, 4),
                (1, 2, 2),
                (1, 3, 1),
                (1, 4, 2),
                (1, 5, 2),
                (1, 6, 0),
                (2, 1, 0x17),
                (2, 2, 2),
                (2, 3, 1),
                (2, 4, 3),
                (2, 5, 2),
                (2, 6, 1),
                (2, 7, 0x3f00_0000),
                (2, 8, 0),
                (2, 9, 0),
                (2, 10, 0x3f00_0000),
                (2, 0x16, 0x3f33_3333),
                (2, 0x17, 0),
                (3, 1, 7),
                (3, 2, 2),
                (3, 3, 1),
                (3, 4, 2),
                (3, 5, 1),
                (3, 6, 0),
                (3, 0xc, 3),
                (4, 1, 1),
            ]
        }
        DIFFENV => &[
            (0, 1, 4),
            (0, 2, 2),
            (0, 3, 0),
            (0, 4, 2),
            (0, 5, 2),
            (0, 6, 0),
            (1, 1, 7),
            (1, 2, 1),
            (1, 3, 2),
            (1, 4, 4),
            (1, 5, 2),
            (1, 6, 0),
            (2, 1, 1),
        ],
        _ => &[
            (0, 1, 4),
            (0, 2, 2),
            (0, 3, 0),
            (0, 4, 2),
            (0, 5, 2),
            (0, 6, 0),
            (1, 1, 1),
        ],
    };
    for &(s, k, v) in list {
        t(dev, s, k, v);
    }
}

/// Filters of style 0 (0x4167b0): linear, nearest mipmap, on stages 0 to 5.
fn set_states(dev: &mut Device) {
    dev.set_render_state(d3d::CULLMODE, 3);
    dev.set_render_state(d3d::ZENABLE, 1);
    dev.set_render_state(d3d::LIGHTING, 1);
    for s in 0..6 {
        dev.set_texture_stage_state(s, d3d::MIPFILTER, 2);
        dev.set_texture_stage_state(s, d3d::MINFILTER, 2);
        dev.set_texture_stage_state(s, d3d::MAGFILTER, 2);
    }
}
