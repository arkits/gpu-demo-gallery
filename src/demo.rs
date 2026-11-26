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
    /// Camera: turned about the world's up and its own right axis, and
    /// placed (`0x406c30`, `0x4078f0`).
    yaw: f32,
    pitch: f32,
    eye: Vec3,
    held: Option<(f64, f64, u8)>,
    keys: [bool; 2],
    drag: (f32, f32),
    due: f64,
    /// The four wave sources (`0x4012a0`), the milliseconds clock they
    /// run on, the random state, and the control points' heights.
    sources: [Source; 4],
    ms: f64,
    seed: u32,
    heights: Vec<Vec<f32>>,
}

/// A ripple from a point of the control grid: it grows for half its life
/// and dies away over the other half.
#[derive(Clone, Copy, Default)]
struct Source {
    col: f32,
    row: f32,
    amplitude: f32,
    start: f64,
    life: f64,
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
            env_map: false,
            fog: false,
            gap_fill: 0,
            dynamic_lod: true,
            bezier: false,
            yaw: 0.0,
            pitch: 0.0,
            eye: Vec3::new(0.0, 0.0, 10.0),
            held: None,
            keys: [false; 2],
            drag: (0.0, 0.0),
            due: 0.0,
            sources: [Source::default(); 4],
            ms: 0.0,
            seed: 1,
            heights: vec![vec![0.0; cols]; rows],
        })
    }

    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            if self.held.is_some_and(|h| h.2 & 1 != 0) {
                // Mouse movement turns the camera a tenth of a degree a
                // pixel (`0x4078f0`).
                let drag = std::mem::take(&mut self.drag);
                self.yaw -= drag.0.to_radians();
                self.pitch = (self.pitch - drag.1.to_radians()).clamp(-1.5, 1.5);
            }
            // Arrows move the camera along its right a unit a frame.
            let right = Vec3::new(self.yaw.cos(), self.yaw.sin(), 0.0);
            let side = f32::from(u8::from(self.keys[1])) - f32::from(u8::from(self.keys[0]));
            self.eye += right * side;
            self.time += 1.0 / 60.0;
            if !self.paused {
                self.ms += 1000.0 / 60.0;
                self.waves();
            }
        }
    }

    fn random(&mut self) -> f32 {
        // `rand()`: 15 bits.
        self.seed = self.seed.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.seed >> 16) & 0x7fff) as f32
    }

    /// `0x4012a0`: renew finished sources, then set every control point's
    /// height from the ripples around it.
    fn waves(&mut self) {
        let (cols, rows) = (self.cols as f32, self.rows as f32);
        for i in 0..self.sources.len() {
            let s = self.sources[i];
            if self.ms - s.start > s.life {
                let col = self.random() * cols * 3.051_851e-5;
                let row = self.random() * rows * 3.051_851e-5;
                let life = f64::from(5000.0 + self.random() * 10000.0 * 3.051_851e-5);
                let amplitude = (50.0 + self.random() * 0.003_051_85) * 0.01;
                self.sources[i] = Source {
                    col,
                    row,
                    amplitude,
                    start: self.ms,
                    life,
                };
            }
        }
        let half = ((self.cols / 2) * (self.cols / 2) + (self.rows / 2) * (self.rows / 2)) as f32;
        let phase = ((self.ms as u64 >> 3) as f32) * 0.017_453_3;
        for j in 0..self.rows {
            for i in 0..self.cols {
                let mut z = 0.0;
                for s in &self.sources {
                    let d2 = (j as f32 - s.row).powi(2) + (i as f32 - s.col).powi(2);
                    if d2 > half {
                        continue;
                    }
                    let f = (d2 / half).sqrt();
                    let age = (self.ms - s.life / 2.0 - s.start).abs() as f32;
                    let envelope = 1.0 - age / (s.life as f32 * 0.5);
                    z -= (1.0 - f)
                        * (1.0 - f)
                        * envelope
                        * (f * 31.415_94 - phase).cos()
                        * s.amplitude;
                }
                self.heights[j][i] = z;
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
            match key {
                "ArrowLeft" => self.keys[0] = false,
                "ArrowRight" => self.keys[1] = false,
                _ => {}
            }
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
            "ArrowLeft" => self.keys[0] = true,
            "ArrowRight" => self.keys[1] = true,
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
        let _ = amount;
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
        // 60 degrees, near 1, far 1000 (`0x4078f0`). The original is z-up;
        // here y is up, so its (x, y, z) is drawn at (x, z, y).
        let proj = from_opengl(perspective(60.0, aspect, 1.0, 1000.0));
        let ahead = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            -self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
        );
        let at = self.eye + ahead;
        let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
        let eye = swap(self.eye);
        let view = look_at(eye, swap(at), Vec3::Y);
        let vp = proj * view;
        gl.begin([0.0, 0.0, 0.0, 1.0]);

        // The original picks each patch's subdivision from its distance; the
        // port uses one level for the whole surface. [I]
        let subdiv = self.subdiv;
        let (verts, normals) = self.surface_mesh(subdiv);
        let fog_color = [0.55, 0.72, 0.90, 1.0];
        let fog_on = if self.fog { 1.0 } else { 0.0 };
        let mut tris = Vec::with_capacity(verts.len());
        for (i, p) in verts.iter().enumerate() {
            let n = normals[i].normalize_or_zero();
            // The surface is drawn 20 times larger across, 10 up.
            let world = Vec4::new(p.x * 20.0, p.z * 10.0, p.y * 20.0, 1.0);
            let clip = vp * world;
            let view_pos = view * world;
            let dist = (-view_pos.z).max(0.0);
            let fog = 1.0 - (-(dist - 200.0) * 0.004_f32).exp();
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
            for row in &self.moved() {
                for c in row {
                    let p = vp * Vec4::new(c.x * 20.0, c.z * 10.0, c.y * 20.0, 1.0);
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
                let a = Vec4::new(p.x * 20.0, p.z * 10.0, p.y * 20.0, 1.0);
                let b = a + Vec4::new(n.x, n.z, n.y, 0.0) * 8.0;
                lines.push(Vertex::new(vp * a, [1.0, 0.3, 0.3, 1.0]));
                lines.push(Vertex::new(vp * b, [1.0, 0.3, 0.3, 1.0]));
            }
            let mut s = State::new(PLAIN);
            s.lines = true;
            gl.draw(s, &Surface::default(), &lines);
        }
    }

    /// The control points as the waves have moved them.
    fn moved(&self) -> Vec<Vec<Control>> {
        let mut c = self.controls.clone();
        for (j, row) in c.iter_mut().enumerate() {
            for (i, p) in row.iter_mut().enumerate() {
                p.z += self.heights[j][i];
            }
        }
        c
    }

    fn surface_mesh(&self, subdiv: usize) -> (Vec<Vec3>, Vec<Vec3>) {
        let controls = self.moved();
        let u_steps = (self.cols - 1).max(1) * subdiv;
        let v_steps = (self.rows - 1).max(1) * subdiv;
        let mut grid = vec![Vec3::ZERO; (u_steps + 1) * (v_steps + 1)];
        for j in 0..=v_steps {
            let v = j as f32 / v_steps as f32;
            for i in 0..=u_steps {
                let u = i as f32 / u_steps as f32;
                let p = if self.bezier {
                    eval_bezier(&controls, u, v)
                } else {
                    eval_bspline(&controls, u, v)
                };
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
