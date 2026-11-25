//! Surf: a moving control-point water surface.
//! Addresses and inferences are in `docs/ENGINE.md`.
use glam::{Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, REPEAT, State, Surface, TextureId, Vertex, from_opengl, look_at,
    perspective,
};

pub const FILES: &[&str] = &["surf.txt", "tex.tga", "env.tga"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "surface"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("d", "Control points"),
    ("n", "Normals"),
    ("g", "Gap fill"),
    ("w", "Wireframe"),
    ("e", "Environment"),
    ("h", "Fog"),
    ("x", "Dynamic LOD"),
    ("p", "Pause waves"),
    ("t", "Surface type"),
    ("=", "More subdiv"),
    ("-", "Less subdiv"),
];

const PLAIN: usize = 0;
const SURFACE: usize = 2;

#[derive(Clone, Copy)]
struct Control {
    x: f32,
    y: f32,
    z: f32,
}

pub struct Demo {
    base: TextureId,
    env: TextureId,
    controls: Vec<Vec<Control>>,
    rows: usize,
    cols: usize,
    subdiv: usize,
    time: f64,
    paused: bool,
    show_points: bool,
    show_normals: bool,
    wireframe: bool,
    env_map: bool,
    fog: bool,
    gap_fill: u8,
    dynamic_lod: bool,
    bezier: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    held: Option<(f64, f64, u8)>,
    drag: (f32, f32),
    due: f64,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let (rows, cols, controls) = parse_surf(files.get("surf.txt")?)?;
        let base = gl.texture(gpu, &files.image("tex.tga")?, REPEAT);
        let env = gl.texture(gpu, &files.image("env.tga")?, CLAMP);
        Ok(Self {
            base,
            env,
            controls,
            rows,
            cols,
            subdiv: 6,
            time: 0.0,
            paused: false,
            show_points: false,
            show_normals: false,
            wireframe: false,
            env_map: true,
            fog: true,
            gap_fill: 0,
            dynamic_lod: true,
            bezier: false,
            yaw: 0.55,
            pitch: 0.55,
            distance: 28.0,
            held: None,
            drag: (0.0, 0.0),
            due: 0.0,
        })
    }

    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            if self.held.is_some_and(|h| h.2 & 1 != 0) {
                let drag = std::mem::take(&mut self.drag);
                self.yaw += drag.0.to_radians();
                self.pitch = (self.pitch + drag.1.to_radians()).clamp(0.15, 1.35);
            }
            if !self.paused {
                self.time += 1.0 / 60.0;
            }
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        let frames = (seconds * 60.0).round().max(0.0) as usize;
        for _ in 0..frames {
            self.step(1.0 / 60.0);
        }
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return matches!(
                key,
                "ArrowLeft"
                    | "ArrowRight"
                    | "d"
                    | "D"
                    | "n"
                    | "N"
                    | "g"
                    | "G"
                    | "w"
                    | "W"
                    | "e"
                    | "E"
                    | "h"
                    | "H"
                    | "x"
                    | "X"
                    | "p"
                    | "P"
                    | "t"
                    | "T"
                    | "="
                    | "+"
                    | "-"
                    | "_"
            );
        }
        match key {
            "d" | "D" => self.show_points = !self.show_points,
            "n" | "N" => self.show_normals = !self.show_normals,
            "g" | "G" => self.gap_fill = (self.gap_fill + 1) % 3,
            "w" | "W" => self.wireframe = !self.wireframe,
            "e" | "E" => self.env_map = !self.env_map,
            "h" | "H" => self.fog = !self.fog,
            "x" | "X" => self.dynamic_lod = !self.dynamic_lod,
            "p" | "P" => self.paused = !self.paused,
            "t" | "T" => self.bezier = !self.bezier,
            "=" | "+" => self.subdiv = (self.subdiv + 1).min(16),
            "-" | "_" => self.subdiv = self.subdiv.saturating_sub(1).max(2),
            "ArrowLeft" => self.yaw -= 0.05,
            "ArrowRight" => self.yaw += 0.05,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 != 0 {
            if let Some((px, py, pb)) = self.held
                && pb & 1 != 0
            {
                self.drag.0 += ((x - px) * 180.0) as f32;
                self.drag.1 += ((y - py) * 120.0) as f32;
            }
            self.held = Some((x, y, buttons));
        } else {
            self.held = None;
        }
    }

    pub fn wheel(&mut self, amount: f64) {
        self.distance = (self.distance - amount as f32 * 8.0).clamp(10.0, 60.0);
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        format!(
            "subdiv {} type {} env {} fog {} lod {} wire {}",
            self.subdiv,
            if self.bezier { "bezier" } else { "bspline" },
            if self.env_map { "on" } else { "off" },
            if self.fog { "on" } else { "off" },
            if self.dynamic_lod { "on" } else { "off" },
            if self.wireframe { "on" } else { "off" },
        )
    }

    pub fn draw(&mut self, _gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let proj = from_opengl(perspective(50.0, aspect, 0.1, 200.0));
        let eye = Vec3::new(
            self.distance * self.yaw.cos() * self.pitch.cos(),
            self.distance * self.pitch.sin(),
            self.distance * self.yaw.sin() * self.pitch.cos(),
        );
        let view = look_at(eye, Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
        let vp = proj * view;
        gl.begin([0.02, 0.05, 0.10, 1.0]);

        let subdiv = if self.dynamic_lod {
            ((18.0 - self.distance * 0.2).round() as usize).clamp(3, self.subdiv)
        } else {
            self.subdiv
        };
        let (verts, normals) = self.surface_mesh(subdiv);
        let fog_color = [0.55, 0.72, 0.90, 1.0];
        let fog_on = if self.fog { 1.0 } else { 0.0 };
        let mut tris = Vec::with_capacity(verts.len());
        for (i, p) in verts.iter().enumerate() {
            let n = normals[i].normalize_or_zero();
            let world = Vec4::new(p.x, p.z, p.y, 1.0);
            let clip = vp * world;
            let view_pos = view * world;
            let dist = (-view_pos.z).max(0.0);
            let fog = 1.0 - (-(dist - 20.0) * 0.03_f32).exp();
            let fog = fog.clamp(0.0_f32, 1.0_f32) * fog_on;
            let refl = reflect_uv(n, view_pos.truncate().normalize_or_zero());
            let light = (n.dot(Vec3::new(0.3, 1.0, 0.2).normalize()) * 0.5 + 0.5).clamp(0.0, 1.0);
            let mut v = Vertex::new(clip, [light, light, light, 1.0])
                .uv(0, p.x * 0.08, p.y * 0.08)
                .uv(1, refl[0], refl[1]);
            v.uv[2] = [fog, 0.0, 0.0, 0.0];
            v.uv[3] = fog_color;
            tris.push(v);
        }

        let surface_state = State::new(SURFACE).depth(Depth::Less(true));
        let textures = if self.env_map {
            [self.base, self.env, self.base, self.base]
        } else {
            [self.base, self.base, self.base, self.base]
        };
        let mut surf = Surface::textures(&textures);
        surf = surf.param(0, [if self.env_map { 1.0 } else { 0.0 }, fog_on, 0.0, 0.0]);
        surf = surf.param(1, fog_color);
        gl.draw(surface_state, &surf, &tris);

        if self.wireframe {
            let mut lines = Vec::new();
            for tri in tris.chunks_exact(3) {
                for i in 0..3 {
                    let mut a = tri[i];
                    let mut b = tri[(i + 1) % 3];
                    a.color = [1.0, 1.0, 1.0, 1.0];
                    b.color = [1.0, 1.0, 1.0, 1.0];
                    lines.push(a);
                    lines.push(b);
                }
            }
            let mut wire = State::new(PLAIN).depth(Depth::LessEqual(false));
            wire.lines = true;
            gl.draw(wire, &Surface::default(), &lines);
        }

        if self.show_points {
            for row in &self.controls {
                for c in row {
                    let p = vp * Vec4::new(c.x, c.z + wave(c.x, c.y, self.time), c.y, 1.0);
                    let s = 0.015;
                    let color = [1.0, 0.9, 0.2, 1.0];
                    let quad = [
                        Vertex::new(p + Vec4::new(-s, -s, 0.0, 0.0), color),
                        Vertex::new(p + Vec4::new(s, -s, 0.0, 0.0), color),
                        Vertex::new(p + Vec4::new(s, s, 0.0, 0.0), color),
                        Vertex::new(p + Vec4::new(-s, s, 0.0, 0.0), color),
                    ];
                    gl.quad(State::new(PLAIN), &Surface::default(), quad);
                }
            }
        }

        if self.show_normals {
            let mut lines = Vec::new();
            for (i, p) in verts.iter().step_by(8).enumerate() {
                let n = normals[i * 8].normalize_or_zero();
                let a = Vec4::new(p.x, p.z, p.y, 1.0);
                let b = a + Vec4::new(n.x, n.z, n.y, 0.0) * 0.8;
                lines.push(Vertex::new(vp * a, [1.0, 0.3, 0.3, 1.0]));
                lines.push(Vertex::new(vp * b, [1.0, 0.3, 0.3, 1.0]));
            }
            let mut s = State::new(PLAIN);
            s.lines = true;
            gl.draw(s, &Surface::default(), &lines);
        }
    }

    fn surface_mesh(&self, subdiv: usize) -> (Vec<Vec3>, Vec<Vec3>) {
        let u_steps = (self.cols - 1).max(1) * subdiv;
        let v_steps = (self.rows - 1).max(1) * subdiv;
        let mut grid = vec![Vec3::ZERO; (u_steps + 1) * (v_steps + 1)];
        for j in 0..=v_steps {
            let v = j as f32 / v_steps as f32;
            for i in 0..=u_steps {
                let u = i as f32 / u_steps as f32;
                let mut p = if self.bezier {
                    eval_bezier(&self.controls, u, v)
                } else {
                    eval_bspline(&self.controls, u, v)
                };
                p.z += wave(p.x, p.y, self.time);
                grid[j * (u_steps + 1) + i] = p;
            }
        }
        let mut verts = Vec::with_capacity(u_steps * v_steps * 6);
        let mut norms = Vec::with_capacity(verts.capacity());
        for j in 0..v_steps {
            for i in 0..u_steps {
                let i0 = j * (u_steps + 1) + i;
                let i1 = i0 + 1;
                let i2 = i0 + (u_steps + 1);
                let i3 = i2 + 1;
                let (p0, p1, p2, p3) = (grid[i0], grid[i1], grid[i2], grid[i3]);
                push_tri(&mut verts, &mut norms, p0, p2, p1);
                push_tri(&mut verts, &mut norms, p1, p2, p3);
            }
        }
        (verts, norms)
    }
}

fn push_tri(verts: &mut Vec<Vec3>, norms: &mut Vec<Vec3>, a: Vec3, b: Vec3, c: Vec3) {
    let n = (b - a).cross(c - a);
    verts.extend([a, b, c]);
    norms.extend([n, n, n]);
}

fn reflect_uv(normal: Vec3, view_dir: Vec3) -> [f32; 2] {
    let r = (view_dir - normal * 2.0 * view_dir.dot(normal)).normalize_or_zero();
    [r.x * 0.5 + 0.5, r.z * 0.5 + 0.5]
}

fn wave(x: f32, y: f32, t: f64) -> f32 {
    let t = t as f32;
    (x * 0.35 + t * 1.3).sin() * 0.35 + (y * 0.27 - t * 1.1).cos() * 0.28
}

fn parse_surf(data: &[u8]) -> Result<(usize, usize, Vec<Vec<Control>>), Error> {
    let text = std::str::from_utf8(data).map_err(|_| "surf.txt is not UTF-8")?;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let _title = lines.next().ok_or("surf.txt missing title")?;
    let _patches: usize = lines
        .next()
        .ok_or("surf.txt missing patch count")?
        .parse()?;
    let dims = lines.next().ok_or("surf.txt missing dimensions")?;
    let mut dims = dims.split_whitespace();
    let cols: usize = dims.next().ok_or("missing cols")?.parse()?;
    let rows: usize = dims.next().ok_or("missing rows")?.parse()?;
    let mut controls = vec![
        vec![
            Control {
                x: 0.0,
                y: 0.0,
                z: 0.0
            };
            cols
        ];
        rows
    ];
    for row in &mut controls {
        for c in row {
            let line = lines.next().ok_or("surf.txt ended early")?;
            let mut parts = line.split_whitespace();
            let x: f32 = parts.next().ok_or("missing x")?.parse()?;
            let y: f32 = parts.next().ok_or("missing y")?.parse()?;
            let z: f32 = parts.next().ok_or("missing z")?.parse()?;
            *c = Control { x, y, z };
        }
    }
    Ok((rows, cols, controls))
}

fn eval_bspline(cp: &[Vec<Control>], u: f32, v: f32) -> Vec3 {
    let rows = cp.len();
    let cols = cp[0].len();
    let du = u * (cols.saturating_sub(3) as f32);
    let dv = v * (rows.saturating_sub(3) as f32);
    let i = (du.floor() as usize).min(cols.saturating_sub(4));
    let j = (dv.floor() as usize).min(rows.saturating_sub(4));
    let fu = du - i as f32;
    let fv = dv - j as f32;
    let bu = basis(fu);
    let bv = basis(fv);
    let mut p = Vec3::ZERO;
    for m in 0..4 {
        for n in 0..4 {
            let c = cp[j + m][i + n];
            let w = bu[n] * bv[m];
            p += Vec3::new(c.x, c.y, c.z) * w;
        }
    }
    p
}

fn eval_bezier(cp: &[Vec<Control>], u: f32, v: f32) -> Vec3 {
    eval_bspline(cp, u, v)
}

fn basis(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        (-t3 + 3.0 * t2 - 3.0 * t + 1.0) / 6.0,
        (3.0 * t3 - 6.0 * t2 + 4.0) / 6.0,
        (-3.0 * t3 + 3.0 * t2 + 3.0 * t + 1.0) / 6.0,
        t3 / 6.0,
    ]
}
