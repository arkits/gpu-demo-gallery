//! Wizards Tower — rendered from the demo's own ATB assets.
//!
//! Loads every `.BOF` name table, `.3DS` mesh and `.3DF` texture in the folder
//! through `crates/atb`'s strict readers, then draws the real meshes (or a
//! reconstructed frame when the BOF object graph is opaque) textured with the
//! demo's own `.3DF` images. See `docs/ENGINE.md`.
use glam::{Mat4, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{
    Depth, Gl, Gpu, Sampling, State, Surface, TextureId, Vertex, from_opengl, look_at, perspective,
};
use crate::image::Image;

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "lit"];
pub const BUTTONS: &[(&str, &str)] = &[("w", "Wireframe"), ("p", "Pause"), ("t", "Next texture")];

/// All ATB assets the port may load; `data.rs` keeps these from the folder/pack.
pub const FILES: &[&str] = &[
    "wiz.bof",
    "3dfxtile.3df",
    "ffont.3df",
    "spark.3df",
    "w4-port.3df",
    "20_1.3df",
    "20_33.3df",
    "20_57.3df",
    "21_16.3df",
    "21_4.3df",
    "21_63.3df",
    "22_24.3df",
    "22_48.3df",
    "23_23.3df",
    "23_47.3df",
    "25_15.3df",
    "28_2.3df",
    "31_2.3df",
    "35_12.3df",
    "7_2.3df",
    "9_14.3df",
    "0_1.3df",
    "1_22.3df",
    "1_46.3df",
    "24_1.3df",
    "25_19.3df",
    "26_8.3df",
    "33_0.3df",
    "34_8.3df",
    "21_30.3df",
    "22_4.3df",
    "31_18.3df",
    "31_41.3df",
    "32_12.3df",
    "33_8.3df",
    "0_4.3df",
    "1_14.3df",
    "1_4.3df",
    "3_8.3df",
    "4_30.3df",
    "11_0.3df",
    "23_29.3df",
    "26_2.3df",
    "29_14.3df",
    "29_38.3df",
    "2_24.3df",
    "2_48.3df",
    "30_5.3df",
    "34_22.3df",
];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const PLAIN: usize = 0;
const LIT: usize = 2;
/// Clamp + no mips: many ATB textures are non-square / non-power-of-two, which
/// must not use repeat addressing with generated mips (WebGL2 parity).
const TEX: Sampling = Sampling {
    clamp: [true; 2],
    mips: false,
    linear: true,
    anisotropy: 1,
};

struct Tri {
    p: [Vec3; 3],
    uv: [[f32; 2]; 3],
    color: [f32; 4],
    tex: Option<usize>,
}

pub struct Demo {
    textures: Vec<TextureId>,
    tris: Vec<Tri>,
    center: Vec3,
    scale: f32,
    dict_len: usize,
    mesh_count: usize,
    tex_count: usize,
    current: usize,
    time: f64,
    paused: bool,
    wireframe: bool,
    yaw: f32,
    pitch: f32,
    dist: f32,
    held: Option<(f64, f64, u8)>,
    drag: (f32, f32),
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        // `Files` implements atb::Source, so every ATB asset is parsed here.
        let scene = atb::Scene::load(files)?;
        if scene.dict.is_empty() && scene.meshes.is_empty() && scene.textures.is_empty() {
            return Err("no ATB assets found".into());
        }

        // Upload every real .3DF base mip as a texture.
        let mut textures = Vec::new();
        for (_, tex) in &scene.textures {
            let base = tex.base();
            let image = Image {
                width: base.width,
                height: base.height,
                rgba: base.rgba.clone(),
            };
            textures.push(gl.texture(gpu, &image, TEX));
        }
        let ntex = textures.len();

        // Build triangles from all real .3DS meshes.
        let mut tris = Vec::new();
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let mut mesh_count = 0;
        for (_, scene3ds) in &scene.meshes {
            for mesh in &scene3ds.meshes {
                if mesh.faces.is_empty() {
                    continue;
                }
                mesh_count += 1;
                for &[a, b, c] in &mesh.faces {
                    let (ia, ib, ic) = (a as usize, b as usize, c as usize);
                    if ia >= mesh.vertices.len()
                        || ib >= mesh.vertices.len()
                        || ic >= mesh.vertices.len()
                    {
                        continue;
                    }
                    let (pa, pb, pc) = (
                        Vec3::from(mesh.vertices[ia]),
                        Vec3::from(mesh.vertices[ib]),
                        Vec3::from(mesh.vertices[ic]),
                    );
                    for p in [pa, pb, pc] {
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                    let uv = |i: usize| {
                        mesh.texcoords
                            .get(i)
                            .map(|t| [t[0], t[1]])
                            .unwrap_or([0.0, 0.0])
                    };
                    let n = (pb - pa).cross(pc - pa).normalize_or_zero();
                    let light = (n.dot(Vec3::new(0.4, 0.8, 0.3).normalize()).abs() * 0.6 + 0.4)
                        .clamp(0.0, 1.0);
                    tris.push(Tri {
                        p: [pa, pb, pc],
                        uv: [uv(ia), uv(ib), uv(ic)],
                        color: [light, light, light, 1.0],
                        tex: (ntex > 0).then_some(0),
                    });
                }
            }
        }

        // Some ATB demos ship only a .BOF whose object graph is opaque, or only
        // movie frames with no geometry. Draw a reconstructed panel grid derived
        // from the real name table / texture names, each panel showing a demo
        // texture (as documented in ENGINE.md).
        if tris.is_empty() {
            let mut panels: Vec<String> = scene.dict.clone();
            if panels.is_empty() {
                panels = scene.textures.iter().map(|(n, _)| n.clone()).collect();
            }
            if panels.is_empty() {
                panels.push("scene".into());
            }
            panels.truncate(64);
            let n = panels.len();
            let cols = (n as f32).sqrt().ceil().max(1.0);
            let rows = (n as f32 / cols).ceil().max(1.0);
            for (i, _) in panels.iter().enumerate() {
                let x = (i % cols as usize) as f32 - cols * 0.5;
                let y = (i / cols as usize) as f32 - rows * 0.5;
                let o = Vec3::new(x * 1.2, y * 1.2, 0.0);
                let (pa, pb, pc, pd) = (
                    o + Vec3::new(-0.5, -0.5, 0.0),
                    o + Vec3::new(0.5, -0.5, 0.0),
                    o + Vec3::new(0.5, 0.5, 0.0),
                    o + Vec3::new(-0.5, 0.5, 0.0),
                );
                let shade = 0.55 + 0.45 * (i as f32 / n as f32);
                let tex = (ntex > 0).then(|| i % ntex);
                push_quad(
                    &mut tris,
                    pa,
                    pb,
                    pc,
                    pd,
                    [shade, shade * 0.85, 0.7, 1.0],
                    tex,
                );
            }
            lo = Vec3::new(-cols, -rows, -1.0);
            hi = Vec3::new(cols, rows, 1.0);
            mesh_count = n;
        }

        let center = (lo + hi) * 0.5;
        let extent = (hi - lo).max_element().max(1.0);
        let scale = 2.0 / extent;

        Ok(Self {
            textures,
            tris,
            center,
            scale,
            dict_len: scene.dict.len(),
            mesh_count,
            tex_count: scene.textures.len(),
            current: 0,
            time: 0.0,
            paused: false,
            wireframe: false,
            yaw: 0.6,
            pitch: 0.25,
            dist: 3.2,
            held: None,
            drag: (0.0, 0.0),
        })
    }

    pub fn step(&mut self, dt: f64) {
        if !self.paused {
            self.time += dt;
        }
        if let Some((_, _, pb)) = self.held
            && pb & 1 != 0
        {
            let drag = std::mem::take(&mut self.drag);
            self.yaw += drag.0.to_radians();
            self.pitch = (self.pitch + drag.1.to_radians()).clamp(-1.2, 1.2);
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        self.time = seconds;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return matches!(key, "w" | "W" | "p" | "P" | "t" | "T");
        }
        match key {
            "w" | "W" => self.wireframe = !self.wireframe,
            "p" | "P" => self.paused = !self.paused,
            "t" | "T" if !self.textures.is_empty() => {
                self.current = (self.current + 1) % self.textures.len()
            }
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, py, pb)) = self.held
                && pb & 1 != 0
            {
                self.drag.0 += ((x - px) * 160.0) as f32;
                self.drag.1 += ((y - py) * 120.0) as f32;
            }
            self.held = Some((x, y, buttons));
        } else {
            self.held = None;
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.dist = (self.dist - amount as f32 * 1.5).clamp(1.5, 8.0);
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "dict {} meshes {} tex {} frame {} wire {} t {:.2}",
            self.dict_len, self.mesh_count, self.tex_count, self.current, self.wireframe, self.time,
        )
    }

    pub fn draw(&mut self, _gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let proj = from_opengl(perspective(55.0, aspect, 0.05, 50.0));
        let eye = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * self.dist;
        let view = look_at(eye, Vec3::ZERO, Vec3::Y);
        let zup = Mat4::from_cols(
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, -1.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        let spin = Mat4::from_rotation_y(if self.paused {
            0.0
        } else {
            self.time as f32 * 0.15
        });
        let model = spin
            * zup
            * Mat4::from_scale(Vec3::splat(self.scale))
            * Mat4::from_translation(-self.center);
        let vp = proj * view * model;

        gl.begin([0.03, 0.04, 0.06, 1.0]);

        let groups = self.textures.len().max(1);
        for g in 0..groups {
            let mut verts = Vec::new();
            for t in &self.tris {
                let base = t.tex.unwrap_or(0);
                let idx = (base + self.current) % groups;
                if idx != g {
                    continue;
                }
                for i in 0..3 {
                    verts.push(
                        Vertex::new(vp * t.p[i].extend(1.0), t.color).uv(0, t.uv[i][0], t.uv[i][1]),
                    );
                }
            }
            if verts.is_empty() {
                continue;
            }
            let tex = self.textures.get(g).copied().unwrap_or(0);
            gl.draw(
                State::new(LIT).depth(Depth::Less(true)),
                &Surface::textures(&[tex]),
                &verts,
            );
        }

        if self.wireframe {
            let mut lines = Vec::with_capacity(self.tris.len() * 6);
            for t in &self.tris {
                let q: Vec<Vertex> = (0..3)
                    .map(|i| Vertex::new(vp * t.p[i].extend(1.0), [1.0, 1.0, 1.0, 1.0]))
                    .collect();
                lines.extend([q[0], q[1], q[1], q[2], q[2], q[0]]);
            }
            gl.draw(
                State::new(PLAIN).depth(Depth::LessEqual(false)).lines(),
                &Surface::default(),
                &lines,
            );
        }
    }
}

fn push_quad(
    tris: &mut Vec<Tri>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    d: Vec3,
    color: [f32; 4],
    tex: Option<usize>,
) {
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    tris.push(Tri {
        p: [a, b, c],
        uv: [uv[0], uv[1], uv[2]],
        color,
        tex,
    });
    tris.push(Tri {
        p: [a, c, d],
        uv: [uv[0], uv[2], uv[3]],
        color,
        tex,
    });
}
