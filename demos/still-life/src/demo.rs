//! Still Life (STMicroelectronics, 2001): fruit, bottles and a lamp on a table. A window in
//! the middle of the screen shows the same scene as a pencil sketch: six hatching pictures
//! of rising darkness, each with its own mipmaps so that strokes keep their width, blended
//! by how much light each vertex gets, and a silhouette picture for outlines. Outside the
//! window the scene is drawn lit and textured. Everything is in `still.exe` (the program,
//! unpacked from UPX); addresses are its own.
use glam::{Mat4, Vec3};

use crate::Error;
use crate::d3d::{self, Device, Gpu, Light, Source, TextureId, Vertex};
use crate::data::Files;
use crate::image::Image;
use crate::kyro::{self, Camera, CameraAnim, CameraSrc, Exe, LightAnim, Mesh, MeshAnim};

pub const FILES: &[&str] = &["still.exe"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Pause"),
    ("r", "Restart"),
    ("a", "Bounce"),
    ("i", "Larger"),
    ("d", "Smaller"),
    ("c", "Credits"),
];
pub const SLIDERS: &[(&str, f32, f32)] = &[];

/// The scene's tables (init 0x403010).
const FPS: f64 = 30.0;
const FRAMES: i32 = 600;
const LIGHTS: (u32, u32, usize) = (0x49bab0, 0x49f3c8, 2);
const MESHES: (u32, usize) = (0x508f80, 34);
const MATERIALS: (u32, usize) = (0x49b650, 14);
const MESH_ANIMS: u32 = 0x51ec88;
const CAMERAS: (u32, u32, usize) = (0x49f3e8, 0x524b58, 1);

/// Texture coordinate kinds: diffuse and silhouette (mesh struct +0xe4).
const DIFFUSE: usize = 1;
const TONE_KIND: usize = 5;
const SILHOUETTE: usize = 9;

/// The sketch window (0x415190, 0x417170): centre and half size in screen units of -1 to 1;
/// keys move it by this step; the A key bounces it, under gravity, 1/75 of a second a step.
const STEP: f32 = 0.01;
const GRAVITY: f32 = -9.8;
const BOUNCE_DT: f32 = 1.0 / 75.0;

struct Part {
    mesh: Mesh,
    anim: MeshAnim,
    index: [Option<u32>; 12],
    vertices: Vec<Vertex>,
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

#[derive(Debug, Clone, Copy)]
struct Window {
    center: [f32; 2],
    half: [f32; 2],
    velocity: [f32; 2],
    bounce: bool,
}

pub struct Demo {
    parts: Vec<Part>,
    materials: Vec<kyro::Material>,
    textures: Vec<Option<TextureId>>,
    lights: Vec<(kyro::LightSrc, LightAnim)>,
    camera: (CameraSrc, CameraAnim),
    hatch: Vec<TextureId>,
    silhouette: TextureId,
    overlay: TextureId,
    credits: TextureId,
    window: Window,
    fade: FadeState,
    time: f64,
    bounce_time: f64,
    paused: bool,
    still: bool,
}

impl Demo {
    pub fn new(gpu: &Gpu, dev: &mut Device, files: &Files) -> Result<Self, Error> {
        let exe = Exe::new(files.get("still.exe")?.to_vec())?;
        let res = exe.resources();
        let picture = |dev: &mut Device, name: &str| -> Result<TextureId, Error> {
            Ok(dev.texture(gpu, &kyro::picture(&res, name)?, true))
        };
        let materials = kyro::materials(&exe, MATERIALS.0, MATERIALS.1);
        let mut textures = Vec::new();
        for m in &materials {
            textures.push(match &m.maps[DIFFUSE] {
                Some(name) => Some(picture(dev, name)?),
                None => None,
            });
        }
        // "Set Hatching" in hatch.ini (0x4291b0); each "Set Texture ... MipMap" picture
        // comes with its own smaller pictures, NAME_256 to NAME_32 (0x442ee0).
        let ini = res
            .get(&("INIFILE".into(), "HATCH.INI".into()))
            .map(|d| String::from_utf8_lossy(d).into_owned())
            .unwrap_or_default();
        let mut hatch = Vec::new();
        for name in hatching(&ini) {
            hatch.push(dev.texture_levels(gpu, &levels(&res, &name)?));
        }
        if hatch.len() != 6 {
            return Err("hatch.ini does not name six hatching pictures".into());
        }
        let meshes = kyro::meshes(&exe, MESHES.0, MESHES.1);
        let anims = kyro::mesh_anims(&exe, MESH_ANIMS, MESHES.1, FRAMES as usize);
        let parts: Vec<Part> = meshes
            .into_iter()
            .zip(anims)
            .map(|(mesh, anim)| {
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
            hatch,
            silhouette: picture(dev, "toonsilhouette.bmp")?,
            overlay: picture(dev, "overlay.bmp")?,
            credits: picture(dev, "credits.bmp")?,
            window: Window {
                center: [0.0, 0.0],
                half: [0.5, 0.5],
                velocity: [1.0, 2.0],
                bounce: false,
            },
            fade: FadeState::default(),
            time: 0.0,
            bounce_time: 0.0,
            paused: false,
            still: false,
        })
    }

    fn frame(&self) -> (i32, i32, f32, i32) {
        kyro::frame_at(self.time, FPS, FRAMES)
    }

    /// 0x43df70.
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
            alpha: from,
            rate: (to - from) / length,
            left: length,
            color,
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
        self.bounce_time += dt;
        while self.bounce_time >= f64::from(BOUNCE_DT) {
            self.bounce_time -= f64::from(BOUNCE_DT);
            self.bounce();
        }
    }

    /// 0x417170: the window falls and bounces off the screen's edges.
    fn bounce(&mut self) {
        let w = &mut self.window;
        if !w.bounce {
            return;
        }
        let mut hit = false;
        if 1.0 - w.half[0] <= w.center[0] || w.center[0] <= w.half[0] - 1.0 {
            hit = true;
            w.velocity[0] = -w.velocity[0];
        }
        if w.center[1] < 1.0 - w.half[1] {
            if w.center[1] <= w.half[1] - 1.0 {
                w.velocity[1] = -w.velocity[1];
            } else if !hit {
                w.velocity[1] += BOUNCE_DT * GRAVITY;
            }
        } else {
            w.velocity[1] = -w.velocity[1];
        }
        w.center[1] += BOUNCE_DT * w.velocity[1];
        w.center[0] += BOUNCE_DT * w.velocity[0];
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
        let dt = 1.0 / FPS;
        while self.time + dt <= seconds {
            self.time += dt;
            self.advance(dt);
        }
        self.time = seconds;
        self.still = true;
    }

    /// Keys of the original (0x415190).
    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        let w = &mut self.window;
        match key {
            " " => self.paused = !self.paused,
            "r" => {
                self.time = 0.0;
                self.fade = FadeState::default();
            }
            "ArrowLeft" if w.half[0] - 1.0 < w.center[0] => w.center[0] -= STEP,
            "ArrowRight" if w.center[0] < 1.0 - w.half[0] => w.center[0] += STEP,
            "ArrowUp" if w.center[1] < 1.0 - w.half[1] => w.center[1] += STEP,
            "ArrowDown" if w.half[1] - 1.0 < w.center[1] => w.center[1] -= STEP,
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {}
            "a" => {
                w.bounce = !w.bounce;
                w.velocity = [1.0, 2.0];
            }
            "i" => {
                for h in &mut w.half {
                    if *h < 1.0 {
                        *h += STEP;
                    }
                }
            }
            "d" => {
                for h in &mut w.half {
                    if STEP < *h {
                        *h -= STEP;
                    }
                }
            }
            "c" => self.start_fade(90, 0.8, 0.0, 0, Some(self.credits)),
            _ => return false,
        }
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
        let w = self.window;
        format!(
            "frame {cur} of {FRAMES}, window {:.2} {:.2} size {:.2}{}",
            w.center[0],
            w.center[1],
            w.half[0],
            if w.bounce { ", bouncing" } else { "" }
        )
    }

    pub fn draw(&mut self, _gpu: &Gpu, dev: &mut Device) {
        let (cur, next, t, _) = self.frame();
        let (w, h) = dev.size();
        let cam = Camera::at(&self.camera.0, &self.camera.1, cur, next, t);
        // Render style 0 clears to black (0x416100); the stencil holds the window.
        dev.clear(7, 0, 1.0, 0);
        dev.set_view(cam.view());
        dev.set_projection(cam.projection(w as f32 / h.max(1) as f32));
        set_states(dev);
        // The one material of the framework (0x421b90): white, no emissive, power 0, specular
        // from the material.
        dev.set_material(d3d::Material {
            diffuse: glam::Vec4::ONE,
            ambient: glam::Vec4::ONE,
            specular: glam::Vec4::ONE,
            emissive: glam::Vec4::ZERO,
            power: 0.0,
        });
        dev.set_render_state(d3d::SPECULARMATERIALSOURCE, 0);
        self.set_lights(dev, cur, next, t);
        for p in &mut self.parts {
            p.pose = p.anim.pose(&p.mesh, cur, next, t);
        }
        self.window_stencil(dev);
        self.draw_meshes(dev, cam.position);
        dev.set_render_state(d3d::STENCILENABLE, 0);
        self.window_overlay(dev);
        self.draw_fade(dev);
    }

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
            dev.light_enable(i, on);
        }
    }

    /// The window's rectangle as a strip, at depth 0 with identity transforms.
    fn rectangle(&self) -> [Vertex; 4] {
        let w = self.window;
        let (x0, x1) = (w.center[0] - w.half[0], w.center[0] + w.half[0]);
        let (y0, y1) = (w.center[1] - w.half[1], w.center[1] + w.half[1]);
        let v = |x: f32, y: f32, u: f32, t: f32| Vertex::at(Vec3::new(x, y, 0.0)).uv(0, u, t);
        [
            v(x0, y1, 0.0, 0.0),
            v(x1, y1, 1.0, 0.0),
            v(x0, y0, 0.0, 1.0),
            v(x1, y0, 1.0, 1.0),
        ]
    }

    fn identity(dev: &mut Device) -> (Mat4, Mat4, Mat4) {
        let saved = (dev.world(), dev.view(), dev.projection());
        dev.set_world(Mat4::IDENTITY);
        dev.set_view(Mat4::IDENTITY);
        dev.set_projection(Mat4::IDENTITY);
        saved
    }

    fn restore(dev: &mut Device, saved: (Mat4, Mat4, Mat4)) {
        dev.set_world(saved.0);
        dev.set_view(saved.1);
        dev.set_projection(saved.2);
    }

    /// 0x440bc0: the window's rectangle into the stencil as 1, white where nothing is drawn.
    fn window_stencil(&self, dev: &mut Device) {
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
        dev.set_texture_stage_state(0, d3d::COLOROP, d3d::SELECTARG1);
        dev.set_texture_stage_state(0, d3d::COLORARG1, d3d::TA_DIFFUSE);
        dev.set_texture_stage_state(0, d3d::ALPHAOP, d3d::SELECTARG1);
        dev.set_texture_stage_state(0, d3d::ALPHAARG1, d3d::TA_DIFFUSE);
        dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
        let saved = Self::identity(dev);
        let quad = self.rectangle();
        dev.draw(d3d::TRIANGLESTRIP, d3d::FVF_XYZ, Source::Data(&quad), None);
        Self::restore(dev, saved);
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

    /// 0x440f00: the overlay picture, a ragged edge of paper, over the window.
    fn window_overlay(&self, dev: &mut Device) {
        for (k, v) in [
            (d3d::ALPHABLENDENABLE, 1),
            (d3d::SRCBLEND, 5),
            (d3d::DESTBLEND, 6),
            (d3d::LIGHTING, 0),
            (d3d::COLORVERTEX, 1),
        ] {
            dev.set_render_state(k, v);
        }
        dev.set_texture(0, Some(self.overlay));
        for (k, v) in [
            (d3d::COLOROP, d3d::SELECTARG2),
            (d3d::COLORARG1, d3d::TA_DIFFUSE),
            (d3d::COLORARG2, d3d::TA_TEXTURE),
            (d3d::TEXCOORDINDEX, 0),
            (d3d::ALPHAOP, d3d::SELECTARG2),
            (d3d::ALPHAARG1, d3d::TA_DIFFUSE),
            (d3d::ALPHAARG2, d3d::TA_TEXTURE),
        ] {
            dev.set_texture_stage_state(0, k, v);
        }
        dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
        let saved = Self::identity(dev);
        let quad = self.rectangle();
        dev.draw(d3d::TRIANGLESTRIP, d3d::FVF_XYZ, Source::Data(&quad), None);
        Self::restore(dev, saved);
        dev.set_render_state(d3d::LIGHTING, 1);
        dev.set_render_state(d3d::COLORVERTEX, 0);
        dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
    }

    /// 0x4241d0 with modes "the material's" and 0x34: each mesh outside the window as its
    /// material says, then inside it hatched.
    fn draw_meshes(&mut self, dev: &mut Device, eye: Vec3) {
        dev.set_render_state(d3d::LIGHTING, 1);
        for i in 0..self.parts.len() {
            let p = &self.parts[i];
            if !p.mesh.drawn || p.pose.hidden {
                continue;
            }
            self.silhouette_coordinates(i, eye);
            let p = &self.parts[i];
            let m = &self.materials[p.mesh.material];
            dev.set_render_state(d3d::AMBIENT, m.ambient);
            // Outside: stencil not 1 (0x4413d0), mode 0 (0x439550, 0x438a20, 0x434230).
            stencil(dev, 6);
            for (k, v) in [
                (d3d::COLOROP, d3d::MODULATE),
                (d3d::COLORARG1, d3d::TA_TEXTURE),
                (d3d::COLORARG2, d3d::TA_DIFFUSE),
                (d3d::ALPHAOP, d3d::SELECTARG1),
                (d3d::ALPHAARG1, d3d::TA_TEXTURE),
                (d3d::ALPHAARG2, d3d::TA_DIFFUSE),
            ] {
                dev.set_texture_stage_state(0, k, v);
            }
            dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
            dev.set_texture(0, self.textures[p.mesh.material]);
            let tci = p.index[DIFFUSE].unwrap_or(0);
            dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, tci);
            dev.set_render_state(d3d::COLORVERTEX, u32::from(p.mesh.colors.is_some()));
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
            dev.set_render_state(d3d::COLORVERTEX, 0);
            // Inside: stencil 1 (0x441330), mode 0x34 on the lit vertices (0x41b470).
            stencil(dev, 3);
            let tl = self.hatch_vertices(dev, i);
            let p = &self.parts[i];
            hatching_stages(dev);
            for (s, t) in self.hatch.iter().enumerate() {
                dev.set_texture(s, Some(*t));
                dev.set_texture_stage_state(s, d3d::TEXCOORDINDEX, tci);
            }
            let plain = p.mesh.name.starts_with("NO_SILHOUETTE");
            dev.set_texture(6, Some(self.silhouette).filter(|_| !plain));
            let sil = p.index[SILHOUETTE].unwrap_or(0);
            dev.set_texture_stage_state(6, d3d::TEXCOORDINDEX, sil);
            let fvf = d3d::FVF_XYZRHW | d3d::FVF_DIFFUSE;
            dev.draw(d3d::TRIANGLELIST, fvf, Source::Data(&tl), Some(&indices));
            for s in 0..8 {
                dev.set_texture(s, None);
            }
        }
        dev.set_texture_stage_state(1, d3d::COLOROP, d3d::DISABLE);
    }

    /// 0x41be60: silhouette coordinates, every frame for every mesh shown.
    fn silhouette_coordinates(&mut self, i: usize, eye: Vec3) {
        let p = &mut self.parts[i];
        let world = p.pose.world;
        let eye = if world != Mat4::IDENTITY {
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
    }

    /// ProcessVertices, then the darkness as alpha and black as colour (0x41b470).
    fn hatch_vertices(&self, dev: &mut Device, i: usize) -> Vec<Vertex> {
        let p = &self.parts[i];
        dev.set_world(p.pose.world);
        let fvf = d3d::FVF_XYZ | d3d::FVF_NORMAL | d3d::FVF_DIFFUSE;
        let mut tl = dev.process_vertices(&p.vertices, fvf, true);
        dev.set_world(Mat4::IDENTITY);
        for v in &mut tl {
            let c = v.diffuse;
            let sum = ((c >> 16) & 0xff) + ((c >> 8) & 0xff) + (c & 0xff);
            let dark = (255.0 - sum as f32 * (1.0 / 3.0)) as u32;
            v.diffuse = dark << 24;
        }
        tl
    }

    /// 0x43e120: a quad over the screen with the fade's alpha and picture.
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
        let saved = Self::identity(dev);
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
        let fvf = d3d::FVF_XYZ | d3d::FVF_DIFFUSE;
        dev.draw(d3d::TRIANGLEFAN, fvf, Source::Data(&quad), None);
        Self::restore(dev, saved);
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

/// Stencil against 1 with `func`, writing nothing (0x4413d0, 0x441330).
fn stencil(dev: &mut Device, func: u32) {
    for (k, v) in [
        (d3d::STENCILENABLE, 1),
        (d3d::STENCILREF, 1),
        (d3d::STENCILMASK, 1),
        (d3d::STENCILFUNC, func),
        (d3d::STENCILPASS, 1),
        (d3d::STENCILZFAIL, 1),
        (d3d::STENCILFAIL, 1),
        (d3d::STENCILWRITEMASK, 0),
    ] {
        dev.set_render_state(k, v);
    }
}

/// Mode 0x34 (0x43b170): each of six stages blends its hatching picture over what is there
/// by the alpha the stage before made, starting from white; each stage's alpha is the
/// darkness plus the picture's own alpha, signed and doubled, so a picture comes in when the
/// darkness passes its level. The seventh multiplies by the silhouette.
fn hatching_stages(dev: &mut Device) {
    dev.set_render_state(d3d::TEXTUREFACTOR, 0xffff_ffff);
    for s in 0..6 {
        let arg2 = if s == 0 {
            d3d::TA_TFACTOR
        } else {
            d3d::TA_CURRENT
        };
        for (k, v) in [
            (d3d::COLOROP, 16),
            (d3d::COLORARG1, d3d::TA_TEXTURE),
            (d3d::COLORARG2, arg2),
            (d3d::ALPHAOP, 9),
            (d3d::ALPHAARG1, d3d::TA_DIFFUSE),
            (d3d::ALPHAARG2, d3d::TA_TEXTURE),
        ] {
            dev.set_texture_stage_state(s, k, v);
        }
    }
    for (k, v) in [
        (d3d::COLOROP, d3d::MODULATE),
        (d3d::COLORARG1, d3d::TA_TEXTURE),
        (d3d::COLORARG2, d3d::TA_CURRENT),
        (d3d::ALPHAOP, d3d::SELECTARG2),
        (d3d::ALPHAARG1, d3d::TA_DIFFUSE),
        (d3d::ALPHAARG2, d3d::TA_TEXTURE),
    ] {
        dev.set_texture_stage_state(6, k, v);
    }
    dev.set_texture_stage_state(7, d3d::COLOROP, d3d::DISABLE);
}

/// The texture coordinate sets of a mesh (0x418e70): at least three; diffuse, tone and
/// silhouette get sets 0, 1 and 2 when the mesh has none of its own.
fn layout(mesh: &Mesh) -> ([Option<u32>; 12], usize) {
    let mut index = [None; 12];
    let mut n = 0u32;
    for (k, uv) in mesh.uvs.iter().enumerate() {
        if uv.is_some() {
            index[k] = Some(n);
            n += 1;
        }
    }
    let channels = (n.clamp(1, 7) as usize).max(3);
    if mesh.drawn {
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

/// The names under "Set Hatching" in the ini.
fn hatching(ini: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in ini.lines().map(str::trim) {
        if line.starts_with("Set Hatching") {
            inside = true;
        } else if inside && line == "}" {
            break;
        } else if inside && line.starts_with('"') {
            out.push(line.trim_matches('"').to_string());
        }
    }
    out
}

/// A hatching picture and its own smaller ones: "Tam_250_512.bmp", then _256 down to _32.
fn levels(
    res: &std::collections::HashMap<(String, String), &[u8]>,
    name: &str,
) -> Result<Vec<Image>, Error> {
    let stem = name.trim_end_matches(".bmp").trim_end_matches("_512");
    let mut out = vec![kyro::picture(res, name)?];
    for size in [256, 128, 64, 32] {
        let n = format!("{stem}_{size}.bmp");
        match kyro::picture(res, &n) {
            Ok(i) => out.push(i),
            Err(_) => break,
        }
    }
    Ok(out)
}

/// Render and filter states of style 0 (0x416970): linear filtering with the nearest
/// mipmap on stages 0 to 5.
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
