//! Demoroom, rendered from the demo's real ATB assets.
//!
//! This is the ATB pioneer: it exercises `crates/atb` end to end. It reads the
//! `demoroom.bof` name table, the `DEMOLITE.3DS` room geometry, and a `.3DF`
//! texture, then draws the textured room through the shared `gl` layer. The
//! original's projected-light cycling, chromakey multipass and the BOF object
//! graph are still future work (see `docs/ENGINE.md`); this proves the readers
//! and the renderer work together, with no placeholder cube.
use glam::{Mat4, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{
    Depth, Gl, Gpu, REPEAT, State, Surface, TextureId, Vertex, from_opengl, look_at, perspective,
};
use crate::image::Image;

/// Fragment programs: `plain` and `textured` come from `prelude.wgsl`; `lit` is here.
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "lit"];
pub const BUTTONS: &[(&str, &str)] = &[("w", "Wireframe"), ("p", "Pause")];

/// Lower-case names of the demo files the port loads. All exist in the staged
/// `data/`. `data.rs` keeps exactly these (and sounds) when packing/serving.
pub const FILES: &[&str] = &["demoroom.bof", "0000.3df", "demolite.3ds"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const PLAIN: usize = 0;
const LIT: usize = 2;

/// A CPU-side triangle in model space, with its texture coordinates and a
/// baked lighting colour (from the face normal).
struct Tri {
    p: [Vec3; 3],
    uv: [[f32; 2]; 3],
    color: [f32; 4],
}

pub struct Demo {
    texture: TextureId,
    tris: Vec<Tri>,
    center: Vec3,
    scale: f32,
    dict_len: usize,
    mesh_count: usize,
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
        // 1. The name table (proves the BOF reader against the real file).
        let bof = atb::Bof::parse(files.get("demoroom.bof")?)?;
        let dict_len = bof.names().len();

        // 2. A real 3DF texture (proves the tex3df reader), as an Image.
        let tex = atb::read_texture_3df(files.get("0000.3DF")?)?;
        let base = tex.base();
        let image = Image {
            width: base.width,
            height: base.height,
            rgba: base.rgba.clone(),
        };
        let texture = gl.texture(gpu, &image, REPEAT);

        // 3. The room geometry (proves the 3DS reader).
        let scene = atb::read_mesh_3ds(files.get("DEMOLITE.3DS")?)?;
        let mut tris = Vec::new();
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let mut mesh_count = 0;

        for mesh in &scene.meshes {
            if mesh.faces.is_empty() {
                continue; // cameras / lights have no geometry
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
                // Simple directional lighting from the face normal.
                let n = (pb - pa).cross(pc - pa).normalize_or_zero();
                let light =
                    (n.dot(Vec3::new(0.4, 0.8, 0.3).normalize()).abs() * 0.6 + 0.4).clamp(0.0, 1.0);
                tris.push(Tri {
                    p: [pa, pb, pc],
                    uv: [uv(ia), uv(ib), uv(ic)],
                    color: [light, light, light, 1.0],
                });
            }
        }
        if tris.is_empty() {
            return Err("DEMOLITE.3DS had no drawable faces".into());
        }

        let center = (lo + hi) * 0.5;
        let extent = (hi - lo).max_element().max(1.0);
        let scale = 2.0 / extent;

        Ok(Self {
            texture,
            tris,
            center,
            scale,
            dict_len,
            mesh_count,
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
            return matches!(key, "e" | "E" | "w" | "W" | "p" | "P");
        }
        match key {
            "w" | "W" => self.wireframe = !self.wireframe,
            "p" | "P" => self.paused = !self.paused,
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
            "dict {} meshes {} tris {} wire {} t {:.2}",
            self.dict_len,
            self.mesh_count,
            self.tris.len(),
            self.wireframe,
            self.time,
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
        let spin = Mat4::from_rotation_y(self.time as f32 * 0.15);
        // 3D Studio is Z-up; remap so the room's height becomes world Y.
        let zup = Mat4::from_cols(
            glam::Vec4::new(1.0, 0.0, 0.0, 0.0),  // x -> x
            glam::Vec4::new(0.0, 0.0, -1.0, 0.0), // y -> -z
            glam::Vec4::new(0.0, 1.0, 0.0, 0.0),  // z -> y
            glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        // Centre first, then scale, then change basis — order matters.
        let model = spin
            * zup
            * Mat4::from_scale(Vec3::splat(self.scale))
            * Mat4::from_translation(-self.center);
        let vp = proj * view * model;

        gl.begin([0.03, 0.04, 0.06, 1.0]);

        let mut verts = Vec::with_capacity(self.tris.len() * 3);
        for t in &self.tris {
            for i in 0..3 {
                verts.push(
                    Vertex::new(vp * t.p[i].extend(1.0), t.color).uv(0, t.uv[i][0], t.uv[i][1]),
                );
            }
        }
        gl.draw(
            State::new(LIT).depth(Depth::Less(true)),
            &Surface::textures(&[self.texture]),
            &verts,
        );

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
