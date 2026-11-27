//! KyroToon (STMicroelectronics, 2001): a cartoon figure, a chip and balls in a room, drawn
//! with two textures whose coordinates the program makes every frame: the light a vertex
//! gets picks a tone, and how far its normal turns from the eye picks a silhouette shade.
//! Everything is in `toon.exe` (the program, unpacked from UPX); addresses are its own.
use glam::{Mat4, Vec3};

use crate::Error;
use crate::d3d::{self, Device, Gpu, Light, Source, TextureId, Vertex};
use crate::data::Files;
use crate::kyro::{self, Camera, CameraAnim, CameraSrc, Exe, Fade, Mesh, MeshAnim};

pub const FILES: &[&str] = &["toon.exe"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const BUTTONS: &[(&str, &str)] = &[(" ", "Pause"), ("r", "Restart"), ("c", "Credits")];
pub const SLIDERS: &[(&str, f32, f32)] = &[];

/// The scene's tables (init 0x403000).
const FPS: f64 = 30.0;
const FRAMES: i32 = 700;
const LIGHTS: (u32, usize) = (0x49bb90, 3);
const MESHES: (u32, usize) = (0x521ab0, 58);
const MATERIALS: (u32, usize) = (0x49b640, 17);
const MESH_ANIMS: u32 = 0x5d0218;
const CAMERAS: (u32, u32, usize) = (0x49bc80, 0x5d8110, 1);
/// Render style 2, toon (0x402f50): its clear colour (0x415f20).
const CLEAR: u32 = 0x00a6_a0b6;
/// The tone is the lit colour's red, green and blue added, times this (0x41b070).
const TONE: f32 = 0.001_253_132_8;

/// Texture coordinates of the twelve kinds (mesh struct +0xe4): diffuse, tone, silhouette.
const DIFFUSE: usize = 1;
const TONE_KIND: usize = 5;
const SILHOUETTE: usize = 9;

/// Material modes for style 2 (0x421df0): plain, with a diffuse map, with an opacity map.
const PLAIN: u32 = 0x37;
const TEXTURED: u32 = 0x38;
const OPACITY: u32 = 0x36;

struct Part {
    mesh: Mesh,
    anim: MeshAnim,
    /// Texture coordinate index of each kind, or none.
    index: [Option<u32>; 12],
    vertices: Vec<Vertex>,
    mode: u32,
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
    /// The floor's height: the centre of SHADOW_FLOOR's bounds (0x440420).
    floor: f32,
    materials: Vec<kyro::Material>,
    textures: Vec<[Option<TextureId>; 12]>,
    lights: Vec<kyro::LightSrc>,
    camera: (CameraSrc, CameraAnim),
    tone: TextureId,
    silhouette: TextureId,
    fades: Vec<(Fade, Option<TextureId>, bool)>,
    credits: TextureId,
    fade: FadeState,
    time: f64,
    last_frame: i32,
    paused: bool,
    still: bool,
    cues: Vec<String>,
}

impl Demo {
    pub fn new(gpu: &Gpu, dev: &mut Device, files: &Files) -> Result<Self, Error> {
        let exe = Exe::new(files.get("toon.exe")?.to_vec())?;
        let res = exe.resources();
        let mut pictures = std::collections::HashMap::new();
        let mut picture = |dev: &mut Device, name: &str| -> Result<TextureId, Error> {
            let key = name.to_ascii_lowercase();
            if let Some(&t) = pictures.get(&key) {
                return Ok(t);
            }
            let t = dev.texture(gpu, &kyro::picture(&res, name)?, true);
            pictures.insert(key, t);
            Ok(t)
        };
        let materials = kyro::materials(&exe, MATERIALS.0, MATERIALS.1);
        let mut textures = Vec::new();
        for m in &materials {
            let mut t = [None; 12];
            for (k, map) in m.maps.iter().enumerate() {
                if let Some(name) = map {
                    t[k] = Some(picture(dev, name)?);
                }
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
        let ini = res
            .get(&("INIFILE".into(), "TOON.INI".into()))
            .map(|d| String::from_utf8_lossy(d).into_owned())
            .unwrap_or_default();
        let mut fades = Vec::new();
        for f in kyro::fades(&ini) {
            let t = match &f.picture {
                Some(p) => Some(picture(dev, p)?),
                None => None,
            };
            fades.push((f, t, false));
        }
        let camera = kyro::cameras(&exe, CAMERAS.0, CAMERAS.1, CAMERAS.2, FRAMES as usize)
            .into_iter()
            .next()
            .ok_or("no camera")?;
        let floor = parts
            .iter()
            .find(|p: &&Part| p.mesh.name.starts_with("SHADOW_FLOOR"))
            .map_or(0.0, |p| {
                let ys = p.mesh.positions.iter().map(|v| v.y);
                let (lo, hi) = ys.fold((f32::MAX, f32::MIN), |(a, b): (f32, f32), y| {
                    (a.min(y), b.max(y))
                });
                (lo + hi) / 2.0
            });
        Ok(Self {
            floor,
            parts,
            textures,
            lights: kyro::lights(&exe, LIGHTS.0, LIGHTS.1),
            camera,
            tone: picture(dev, "toontone.bmp")?,
            silhouette: picture(dev, "toonsilhouette.bmp")?,
            credits: picture(dev, "credits.bmp")?,
            materials,
            fades,
            fade: FadeState::default(),
            time: 0.0,
            last_frame: -1,
            paused: false,
            still: false,
            cues: Vec::new(),
        })
    }

    fn frame(&self) -> (i32, i32, f32, i32) {
        kyro::frame_at(self.time, FPS, FRAMES)
    }

    /// Triggers of the ini fire once each loop when their frame comes (0x403380, type 6).
    fn triggers(&mut self) {
        let (cur, ..) = self.frame();
        if cur < self.last_frame || self.last_frame < 0 {
            for f in &mut self.fades {
                f.2 = false;
            }
        }
        self.last_frame = cur;
        let mut fire = None;
        for (f, t, fired) in &mut self.fades {
            if !*fired && f.frame <= cur {
                *fired = true;
                fire = Some((f.clone(), *t));
            }
        }
        if let Some((f, t)) = fire {
            self.start_fade(f.frames, f.from, f.to, f.color, t);
        }
    }

    /// 0x43db80.
    fn start_fade(
        &mut self,
        frames: i32,
        from: f32,
        to: f32,
        color: u32,
        texture: Option<TextureId>,
    ) {
        let length = frames as f32 / FPS as f32;
        self.fade = FadeState {
            on: true,
            alpha: from.clamp(0.0, 1.0),
            rate: (to.clamp(0.0, 1.0) - from.clamp(0.0, 1.0)) / length,
            left: length,
            color,
            texture,
        };
    }

    /// 0x43dc90, with the frame's length.
    fn advance_fade(&mut self, dt: f32) {
        let f = &mut self.fade;
        if f.on {
            f.alpha = (f.alpha + dt * f.rate).clamp(0.0, 1.0);
            f.left -= dt;
            if f.left < 0.0 {
                f.on = false;
            }
        }
    }

    pub fn step(&mut self, dt: f64) {
        if self.paused || self.still {
            return;
        }
        self.time += dt;
        self.triggers();
        self.advance_fade(dt as f32);
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.time = 0.0;
        self.last_frame = -1;
        self.fade = FadeState::default();
        // Fades run in steps of a frame, as the original's would at 30 frames a second.
        let dt = 1.0 / FPS;
        while self.time + dt <= seconds {
            self.time += dt;
            self.triggers();
            self.advance_fade(dt as f32);
        }
        self.time = seconds;
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
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
                self.last_frame = -1;
                self.fade = FadeState::default();
            }
            // 0x415180, case 'C': the credits over 90 frames, from 0.8 to nothing.
            "c" => self.start_fade(90, 0.8, 0.0, 0, Some(self.credits)),
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
        let (cur, ..) = self.frame();
        format!("frame {cur} of {FRAMES}, fade {:.2}", self.fade.alpha)
    }

    pub fn draw(&mut self, _gpu: &Gpu, dev: &mut Device) {
        let (cur, next, t, _) = self.frame();
        let (w, h) = dev.size();
        let cam = Camera::at(&self.camera.0, &self.camera.1, cur, next, t);
        dev.clear(7, CLEAR, 1.0, 0);
        dev.set_view(cam.view());
        dev.set_projection(cam.projection(w as f32 / h.max(1) as f32));
        set_states(dev);
        self.set_lights(dev);
        for p in &mut self.parts {
            p.pose = p.anim.pose(&p.mesh, cur, next, t);
        }
        self.draw_meshes(dev, cam.position);
        self.draw_shadows(dev);
        self.draw_fade(dev);
    }

    fn set_lights(&self, dev: &mut Device) {
        for (i, l) in self.lights.iter().enumerate().take(8) {
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
                    ambient: glam::Vec4::ZERO,
                    position: l.position,
                    direction: l.direction,
                    range: l.range,
                    falloff: if l.kind == 1 { 1.0 } else { 0.0 },
                    attenuation: l.attenuation,
                    theta: l.theta,
                    phi: l.phi,
                },
            );
            dev.light_enable(i, true);
        }
    }

    /// 0x423620: by material mode, then material, each mesh with its coordinates made.
    fn draw_meshes(&mut self, dev: &mut Device, eye: Vec3) {
        dev.set_render_state(d3d::LIGHTING, 1);
        let mut order: Vec<usize> = (0..self.parts.len()).collect();
        order.sort_by_key(|&i| {
            let p = &self.parts[i];
            (p.mode, p.mesh.material)
        });
        let mut mode = None;
        let mut material = None;
        for i in order {
            let p = &self.parts[i];
            if !p.mesh.drawn || p.pose.hidden {
                continue;
            }
            if mode != Some(p.mode) {
                if let Some(m) = mode {
                    end(dev, m);
                }
                begin(dev, p.mode);
                mode = Some(p.mode);
                material = None;
            }
            if material != Some(p.mesh.material) {
                let m = &self.materials[p.mesh.material];
                dev.set_render_state(d3d::AMBIENT, m.ambient);
                self.bind(dev, p, true);
                material = Some(p.mesh.material);
            }
            let plain = p.mesh.name.starts_with("NO_SILHOUETTE");
            if plain {
                self.bind(dev, p, false);
            }
            let tl = self.toon_vertices(dev, i, eye);
            let p = &self.parts[i];
            for (stage, kind) in kinds(p.mode).iter().enumerate() {
                let index = p.index[*kind].unwrap_or(0);
                dev.set_texture_stage_state(stage, d3d::TEXCOORDINDEX, index);
            }
            let indices: Vec<u16> = p.mesh.faces.iter().flatten().copied().collect();
            dev.set_render_state(d3d::COLORVERTEX, 1);
            let fvf = d3d::FVF_XYZRHW | d3d::FVF_DIFFUSE;
            dev.draw(d3d::TRIANGLELIST, fvf, Source::Data(&tl), Some(&indices));
            if plain {
                self.bind(dev, &self.parts[i], true);
            }
        }
        if let Some(m) = mode {
            end(dev, m);
        }
        dev.set_render_state(d3d::COLORVERTEX, 0);
        dev.set_world(Mat4::IDENTITY);
    }

    /// The material's textures for the mode (0x438940, 0x438980, 0x439130).
    fn bind(&self, dev: &mut Device, p: &Part, silhouette: bool) {
        let maps = &self.textures[p.mesh.material];
        match p.mode {
            // Meshes named NO_SILHOUETTE get no silhouette picture (0x423620, 0x4338e0).
            PLAIN => {
                dev.set_texture(0, Some(self.tone));
                dev.set_texture(1, Some(self.silhouette).filter(|_| silhouette));
            }
            TEXTURED => {
                dev.set_texture(0, maps[DIFFUSE]);
                dev.set_texture(1, Some(self.tone));
                dev.set_texture(2, Some(self.silhouette).filter(|_| silhouette));
            }
            _ => dev.set_texture(0, maps[DIFFUSE]),
        }
    }

    /// The silhouette coordinates in the mesh (0x41b9a0), then ProcessVertices and the tone
    /// (0x41b1d0, 0x41b070): the transformed vertices that are drawn.
    fn toon_vertices(&mut self, dev: &mut Device, i: usize, eye: Vec3) -> Vec<Vertex> {
        let p = &mut self.parts[i];
        let world = p.pose.world;
        let moved = world != Mat4::IDENTITY;
        let eye = if moved {
            world.inverse().transform_point3(eye)
        } else {
            eye
        };
        if let Some(c) = p.index[SILHOUETTE] {
            for (v, pos) in p.vertices.iter_mut().zip(&p.mesh.positions) {
                let n = Vec3::from(v.normal);
                let to = (eye - *pos).normalize_or_zero();
                let d = to.dot(n);
                v.uv[c as usize] = [d.abs(), (1.0 - (to.y - 2.0 * d * n.y)) * 0.5];
            }
        }
        dev.set_world(world);
        let fvf = d3d::FVF_XYZ | d3d::FVF_NORMAL | d3d::FVF_DIFFUSE;
        let mut tl = dev.process_vertices(&p.vertices, fvf, true);
        dev.set_world(Mat4::IDENTITY);
        let m = &self.materials[p.mesh.material];
        let tone = p.index[TONE_KIND].map(|c| c as usize);
        for (k, v) in tl.iter_mut().enumerate() {
            let c = v.diffuse;
            let b = (((c >> 16) & 0xff) + ((c >> 8) & 0xff) + (c & 0xff)) as f32 * TONE;
            v.diffuse = p.mesh.colors.as_ref().map_or(m.diffuse, |c| c[k]);
            if let Some(t) = tone {
                v.uv[t] = [b, b];
            }
        }
        tl
    }

    /// Flat shadows (Shadow module, 0x43ebc0, 0x43f610, 0x43f830): the meshes named
    /// SHADOW_FLAT are flattened onto the floor from each light that casts shadows, marked in
    /// the stencil, and the marked pixels are darkened by half.
    fn draw_shadows(&self, dev: &mut Device) {
        let rs = |dev: &mut Device, k: u32, v: u32| dev.set_render_state(k, v);
        let y = self.floor + 1.0;
        for l in self.lights.iter().filter(|l| l.words[18] == 1) {
            let mut any = false;
            for p in &self.parts {
                let named = p.mesh.name.starts_with("SHADOW_FLAT")
                    || p.mesh.name.starts_with("DUMMY_SHADOW_FLAT");
                if !named || !p.mesh.shadow || p.pose.hidden {
                    continue;
                }
                let flat: Vec<Vertex> = p
                    .mesh
                    .positions
                    .iter()
                    .map(|v| {
                        let v = p.pose.world.transform_point3(*v);
                        let to = if l.kind == 2 {
                            l.direction
                        } else {
                            l.position - v
                        };
                        let along = if l.kind == 2 {
                            l.direction.y
                        } else {
                            l.position.y - v.y
                        };
                        let t = (self.floor - v.y) / along;
                        Vertex::at(Vec3::new(v.x + to.x * t, y, v.z + to.z * t)).color(0x5f00_0000)
                    })
                    .collect();
                for (k, v) in [
                    (d3d::ZWRITEENABLE, 0),
                    (d3d::STENCILENABLE, 1),
                    (d3d::LIGHTING, 0),
                    (d3d::CULLMODE, 1),
                    (d3d::SHADEMODE, 1),
                    (d3d::STENCILFUNC, 8),
                    (d3d::STENCILZFAIL, 1),
                    (d3d::STENCILFAIL, 1),
                    (d3d::ALPHABLENDENABLE, 1),
                    (d3d::SRCBLEND, 1),
                    (d3d::DESTBLEND, 2),
                    (d3d::STENCILREF, 1),
                    (d3d::STENCILMASK, 1),
                    (d3d::STENCILWRITEMASK, 1),
                    (d3d::STENCILPASS, 3),
                ] {
                    rs(dev, k, v);
                }
                dev.set_world(Mat4::IDENTITY);
                let indices: Vec<u16> = p.mesh.faces.iter().flatten().copied().collect();
                let fvf = d3d::FVF_XYZ | d3d::FVF_DIFFUSE;
                dev.draw(d3d::TRIANGLELIST, fvf, Source::Data(&flat), Some(&indices));
                for (k, v) in [
                    (d3d::ZWRITEENABLE, 1),
                    (d3d::STENCILENABLE, 0),
                    (d3d::ALPHABLENDENABLE, 0),
                    (d3d::SHADEMODE, 2),
                    (d3d::CULLMODE, 3),
                    (d3d::LIGHTING, 1),
                ] {
                    rs(dev, k, v);
                }
                any = true;
            }
            if any {
                for (k, v) in [
                    (d3d::ALPHABLENDENABLE, 1),
                    (d3d::SRCBLEND, 5),
                    (d3d::DESTBLEND, 6),
                    (d3d::STENCILENABLE, 1),
                    (d3d::ZENABLE, 0),
                    (d3d::STENCILREF, 1),
                    (d3d::STENCILFUNC, 4),
                    (d3d::STENCILPASS, 2),
                    (d3d::LIGHTING, 0),
                    (d3d::COLORVERTEX, 1),
                ] {
                    rs(dev, k, v);
                }
                dev.set_texture_stage_state(0, d3d::COLOROP, d3d::SELECTARG1);
                dev.set_texture_stage_state(0, d3d::COLORARG1, d3d::TA_DIFFUSE);
                dev.set_texture_stage_state(0, d3d::ALPHAOP, d3d::SELECTARG1);
                dev.set_texture_stage_state(0, d3d::ALPHAARG1, d3d::TA_DIFFUSE);
                dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
                let [x, y0, w, h, ..] = dev.viewport();
                let c = |x: f32, y: f32| Vertex::screen(x, y, 0.0, 1.0).color(0x8000_0000);
                let quad = [c(x, y0), c(x + w, y0), c(x, y0 + h), c(x + w, y0 + h)];
                let fvf = d3d::FVF_XYZRHW | d3d::FVF_DIFFUSE;
                dev.draw(d3d::TRIANGLESTRIP, fvf, Source::Data(&quad), None);
                for (k, v) in [
                    (d3d::LIGHTING, 1),
                    (d3d::COLORVERTEX, 0),
                    (d3d::ZENABLE, 1),
                    (d3d::STENCILENABLE, 0),
                    (d3d::ALPHABLENDENABLE, 0),
                ] {
                    rs(dev, k, v);
                }
            }
        }
    }

    /// 0x43dd30: a quad over the screen with the fade's alpha, its picture if any.
    fn draw_fade(&self, dev: &mut Device) {
        let f = self.fade;
        if !f.on {
            return;
        }
        dev.set_render_state(d3d::ALPHABLENDENABLE, 1);
        dev.set_render_state(d3d::SRCBLEND, 5);
        dev.set_render_state(d3d::DESTBLEND, 6);
        dev.set_render_state(d3d::ZENABLE, 0);
        let (view, proj, world) = (dev.view(), dev.projection(), dev.world());
        dev.set_world(Mat4::IDENTITY);
        dev.set_view(Mat4::IDENTITY);
        dev.set_projection(Mat4::IDENTITY);
        let color = f.color.wrapping_add(((f.alpha * 255.0) as u32) << 24);
        dev.set_render_state(d3d::LIGHTING, 0);
        dev.set_render_state(d3d::COLORVERTEX, 1);
        match f.texture {
            None => {
                dev.set_texture_stage_state(0, d3d::COLOROP, d3d::SELECTARG1);
                dev.set_texture_stage_state(0, d3d::COLORARG1, d3d::TA_DIFFUSE);
                dev.set_texture_stage_state(0, d3d::ALPHAOP, d3d::SELECTARG1);
                dev.set_texture_stage_state(0, d3d::ALPHAARG1, d3d::TA_DIFFUSE);
            }
            Some(t) => {
                dev.set_texture(0, Some(t));
                dev.set_texture_stage_state(0, d3d::COLOROP, d3d::SELECTARG1);
                dev.set_texture_stage_state(0, d3d::COLORARG1, d3d::TA_TEXTURE);
                dev.set_texture_stage_state(0, d3d::ALPHAOP, d3d::SELECTARG2);
                dev.set_texture_stage_state(0, d3d::ALPHAARG2, d3d::TA_DIFFUSE);
            }
        }
        dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0);
        dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
        // (-1,-1) (-1,1) (1,1) (1,-1), as a fan (0x43d9e0).
        let corner = |x: f32, y: f32, u: f32, v: f32| {
            Vertex::at(Vec3::new(x, y, 0.0)).color(color).uv(0, u, v)
        };
        let quad = [
            corner(-1.0, -1.0, 0.0, 1.0),
            corner(-1.0, 1.0, 0.0, 0.0),
            corner(1.0, 1.0, 1.0, 0.0),
            corner(1.0, -1.0, 1.0, 1.0),
        ];
        let fvf = d3d::FVF_XYZ | d3d::FVF_DIFFUSE;
        dev.draw(d3d::TRIANGLEFAN, fvf, Source::Data(&quad), None);
        dev.set_render_state(d3d::LIGHTING, 1);
        dev.set_render_state(d3d::COLORVERTEX, 0);
        dev.set_world(world);
        dev.set_view(view);
        dev.set_projection(proj);
        dev.set_render_state(d3d::ZENABLE, 1);
        dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
    }
}

/// The kind of each texture coordinate set of a mesh (0x418920), and how many sets its
/// vertices carry. Style 2 always has three: diffuse, tone and silhouette.
fn layout(mesh: &Mesh) -> ([Option<u32>; 12], usize) {
    let mut index = [None; 12];
    let mut n = 0u32;
    for (k, uv) in mesh.uvs.iter().enumerate() {
        if uv.is_some() {
            index[k] = Some(n);
            n += 1;
        }
    }
    let mut channels = n.clamp(1, 7) as usize;
    if mesh.drawn {
        if channels < 3 {
            channels = 3;
        }
        if mesh.uvs[DIFFUSE].is_none() {
            index[DIFFUSE] = Some(0);
        }
        if mesh.uvs[TONE_KIND].is_none() {
            index[TONE_KIND] = Some(if index[DIFFUSE] == Some(1) { 0 } else { 1 });
        }
        if mesh.uvs[SILHOUETTE].is_none() {
            index[SILHOUETTE] = Some(if index[DIFFUSE] == Some(2) { 0 } else { 2 });
        }
    }
    (index, channels)
}

/// The mode style 2 gives a material (end of 0x421df0).
fn mode(m: &kyro::Material) -> u32 {
    if m.maps[8].is_some() {
        0x39
    } else if m.maps[6].is_some() {
        OPACITY
    } else if m.maps[1].is_some() {
        TEXTURED
    } else {
        PLAIN
    }
}

/// Texture coordinate kinds of each stage (draw functions 0x434430, 0x434480, 0x4385f0).
fn kinds(mode: u32) -> &'static [usize] {
    match mode {
        PLAIN => &[TONE_KIND, SILHOUETTE],
        TEXTURED => &[DIFFUSE, TONE_KIND, SILHOUETTE],
        _ => &[DIFFUSE],
    }
}

/// Render and filter states the program sets once (0x4165e0), for style 2.
fn set_states(dev: &mut Device) {
    dev.set_render_state(d3d::CULLMODE, 3);
    dev.set_render_state(d3d::ZENABLE, 1);
    dev.set_render_state(d3d::LIGHTING, 1);
    for s in 0..6 {
        dev.set_texture_stage_state(s, d3d::MIPFILTER, 1);
        dev.set_texture_stage_state(s, d3d::MINFILTER, 1);
        dev.set_texture_stage_state(s, d3d::MAGFILTER, 1);
    }
}

/// The begin function of a mode (0x43a760, 0x43a870, 0x43d300).
fn begin(dev: &mut Device, mode: u32) {
    let t = |dev: &mut Device, s: usize, k: u32, v: u32| dev.set_texture_stage_state(s, k, v);
    match mode {
        PLAIN => {
            for (s, k, v) in [
                (0, 1, 4),
                (0, 2, 0),
                (0, 3, 2),
                (0, 4, 2),
                (0, 5, 0),
                (0, 6, 2),
                (1, 1, 4),
                (1, 2, 2),
                (1, 3, 1),
                (1, 4, 2),
                (1, 5, 0),
                (1, 6, 2),
                (2, 1, 1),
            ] {
                t(dev, s, k, v);
            }
        }
        TEXTURED => {
            for (s, k, v) in [
                (0, 0x11, 2),
                (0, 0x10, 2),
                (0, 0x12, 3),
                (0, 1, 3),
                (0, 2, 0),
                (0, 3, 2),
                (0, 4, 2),
                (0, 5, 0),
                (0, 6, 2),
                (1, 1, 4),
                (1, 2, 1),
                (1, 3, 2),
                (1, 4, 2),
                (1, 5, 0),
                (1, 6, 2),
                (2, 1, 4),
                (2, 2, 1),
                (2, 3, 2),
                (2, 4, 2),
                (2, 5, 0),
                (2, 6, 2),
                (3, 1, 1),
            ] {
                t(dev, s, k, v);
            }
        }
        _ => {
            dev.set_render_state(d3d::ALPHATESTENABLE, 1);
            dev.set_render_state(d3d::ALPHAREF, 0x20);
            dev.set_render_state(d3d::ALPHAFUNC, 5);
            dev.set_render_state(d3d::CULLMODE, 1);
            for (s, k, v) in [
                (0, 1, 2),
                (0, 2, 2),
                (0, 3, 1),
                (0, 4, 2),
                (0, 5, 2),
                (0, 6, 1),
                (1, 1, 1),
            ] {
                t(dev, s, k, v);
            }
        }
    }
}

/// The end function of a mode (0x43d6c0, 0x43d6e0, 0x43d8d0).
fn end(dev: &mut Device, mode: u32) {
    match mode {
        PLAIN => {}
        TEXTURED => {
            dev.set_texture_stage_state(0, d3d::MINFILTER, 1);
            dev.set_texture_stage_state(0, d3d::MAGFILTER, 1);
        }
        _ => {
            dev.set_render_state(d3d::ALPHATESTENABLE, 0);
            dev.set_render_state(d3d::CULLMODE, 3);
        }
    }
}
