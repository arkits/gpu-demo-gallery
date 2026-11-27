//! ELSA's S3D Victory demo (`DEMO.EXE`, 1997, DOS/4GW with S3's S3D Toolkit for the
//! ViRGE; addresses are in the image `le.py` makes of it, based at 0x10000). Scripts
//! (`SCRIPT.SCP`, `SCRIPT2.SCP`) place objects (`.VOJ`) and run a small program for each,
//! one step a frame.
// Loops go by axis, as the original's code does.
#![allow(clippy::needless_range_loop)]
use std::collections::HashMap;

use glam::{Vec2, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{Depth, Gl, Gpu, Sampling, State, Surface, TextureId, Vertex};
use crate::image::Image;

/// Every file the scripts name, by its name without directories, in lower case.
pub const FILES: &[&str] = &[
    "script.scp",
    "script2.scp",
    "3.voj",
    "c.voj",
    "cube.voj",
    "d.voj",
    "elsa_a.voj",
    "elsa_bor.voj",
    "elsa_e.voj",
    "elsa_l.voj",
    "elsa_s.voj",
    "i.voj",
    "o.voj",
    "r.voj",
    "t.voj",
    "v.voj",
    "x.voj",
    "y.voj",
    "blue.tex",
    "orange.tex",
    "tex4.tex",
    "tex6.tex",
    "vic_sml.tex",
    "yellow.tex",
    "vic_3dx.bmp",
];
/// Files that are parts of a program: name, address, length.
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["textured", "affine", "plain"];
pub const BUTTONS: &[(&str, &str)] = &[("1", "Script 1"), ("2", "Script 2"), (" ", "Pause")];
const SCRIPTS: [&str; 2] = ["script.scp", "script2.scp"];

const TEXTURED: usize = 0;
const AFFINE: usize = 1;
const PLAIN: usize = 2;

/// The screen (0x11d60): 640 by 480, its middle, and the scale of the projection.
const SCREEN: Vec2 = Vec2::new(640.0, 480.0);
const MIDDLE: Vec2 = Vec2::new(320.0, 240.0);
const FOCUS: f32 = 480.0;
/// Angles are hundredths of a degree; this is what the program takes for π / 18000
/// (0x300ed), a little more.
const DEGREE: f32 = 0.000_174_653_2;
const TURN: i32 = 36_000;
/// Depth and depth speeds are hundredths (0x300e1); objects also grow by 1 / (z 0.005)
/// (0x117bc).
const Z_UNIT: f32 = 0.01;
const GROWTH: f32 = 0.005;
/// One step is a frame of the original, which is taken as 60 a second.
const FRAME: f64 = 1.0 / 60.0;
/// Depths that the port maps to its depth buffer, which the original's z times 100 fills.
const DEPTH_FAR: f32 = 2000.0;

// ---------------------------------------------------------------------------------------
// Text: words between blanks, commas and parentheses (0x1725a).

fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c.is_whitespace() || c == ',' || c == '(' || c == ')' {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            if c == '(' || c == ')' {
                out.push(c.to_string());
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// A number of a script (0x14530): with a point it is taken in hundredths.
fn number(w: &str) -> i32 {
    if w.contains('.') {
        (w.parse::<f64>().unwrap_or(0.0) * 100.0).round() as i32
    } else {
        w.parse::<i32>().unwrap_or(0)
    }
}

fn base_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
}

// ---------------------------------------------------------------------------------------
// Objects (`.VOJ`, 0x12590).

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Triangles,
    Strip,
    Fan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Plane {
    Xy,
    Xz,
    Yz,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Render {
    Gouraud,
    /// Texture coordinates go straight across the screen.
    Flat,
    Perspective,
}

#[derive(Debug, Clone)]
struct List {
    kind: Kind,
    indices: Vec<usize>,
    /// Vertex colors, red green blue alpha, for every vertex of the object.
    colors: Vec<[u8; 4]>,
    render: Render,
    linear: bool,
    texture: Option<String>,
    plane: Option<Plane>,
    stretch: (i32, i32),
    /// Texture coordinates in texels, for every vertex; zero until a script sets them.
    uv: Vec<Vec2>,
}

#[derive(Debug, Clone)]
struct Model {
    vertices: Vec<Vec3>,
    lists: Vec<List>,
    min: Vec3,
    max: Vec3,
}

fn read_model(text: &str) -> Result<Model, Error> {
    let w = words(text);
    let mut i = 0;
    let mut colors: HashMap<String, [u8; 3]> = HashMap::new();
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut vertices = Vec::new();
    let mut lists = Vec::new();
    // The box starts as if around ±100 and only grows, as the program has it.
    let (mut min, mut max) = (Vec3::splat(100.0), Vec3::splat(-100.0));
    let at = |i: usize| w.get(i).map(String::as_str).unwrap_or("");
    while i < w.len() {
        match at(i) {
            "COLORLIST" => {
                i += 1;
                while at(i) != "END" && i < w.len() {
                    let rgb = [1, 2, 3].map(|k| at(i + k).parse::<u8>().unwrap_or(0));
                    colors.insert(at(i).to_string(), rgb);
                    i += 4;
                }
                i += 1;
            }
            "VERTEXLIST" => {
                i += 1;
                while at(i) != "END" && i < w.len() {
                    let p = [1, 2, 3].map(|k| at(i + k).parse::<f32>().unwrap_or(0.0));
                    let p = Vec3::from_array(p);
                    min = min.min(p);
                    max = max.max(p);
                    names.insert(at(i).to_string(), vertices.len());
                    vertices.push(p);
                    i += 4;
                }
                i += 1;
            }
            k @ ("STRIPLIST" | "FANLIST" | "TRILIST") => {
                let kind = match k {
                    "STRIPLIST" => Kind::Strip,
                    "FANLIST" => Kind::Fan,
                    _ => Kind::Triangles,
                };
                i += 1;
                if at(i) == "NAME" {
                    i += 2;
                }
                let mut list = List {
                    kind,
                    indices: Vec::new(),
                    colors: vec![[0; 4]; vertices.len()],
                    render: Render::Flat,
                    linear: false,
                    texture: None,
                    plane: None,
                    stretch: (100, 100),
                    uv: vec![Vec2::ZERO; vertices.len()],
                };
                while at(i) != "END" && i < w.len() {
                    match at(i) {
                        "COLORS" => {
                            i += 1;
                            while at(i) != "END" && i < w.len() {
                                let v = *names
                                    .get(at(i))
                                    .ok_or_else(|| format!("voj: no vertex {}", at(i)))?;
                                let alpha = at(i + 1).parse::<u8>().unwrap_or(255);
                                let (rgb, step) = match colors.get(at(i + 2)) {
                                    Some(c) => (*c, 3),
                                    None => {
                                        let c =
                                            [2, 3, 4].map(|k| at(i + k).parse::<u8>().unwrap_or(0));
                                        (c, 5)
                                    }
                                };
                                list.colors[v] = [rgb[0], rgb[1], rgb[2], alpha];
                                i += step;
                            }
                            i += 1;
                        }
                        "LIST" => {
                            i += 1;
                            while at(i) != "END" && i < w.len() {
                                let v = *names
                                    .get(at(i))
                                    .ok_or_else(|| format!("voj: no vertex {}", at(i)))?;
                                list.indices.push(v);
                                i += 1;
                            }
                            i += 1;
                        }
                        _ => i += 1,
                    }
                }
                i += 1;
                lists.push(list);
            }
            _ => i += 1,
        }
    }
    Ok(Model {
        vertices,
        lists,
        min,
        max,
    })
}

/// Triangles of a list, by vertex.
fn triangles(list: &List) -> Vec<[usize; 3]> {
    let v = &list.indices;
    match list.kind {
        Kind::Triangles => v.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
        Kind::Strip => (2..v.len()).map(|k| [v[k - 2], v[k - 1], v[k]]).collect(),
        Kind::Fan => (2..v.len()).map(|k| [v[0], v[k - 1], v[k]]).collect(),
    }
}

// ---------------------------------------------------------------------------------------
// Scripts (0x13dfc).

#[derive(Debug, Clone)]
enum Command {
    /// A command and the words between its parentheses.
    Call(String, Vec<String>),
    Wait(i32),
    Loop(i32),
    End,
}

#[derive(Debug, Clone)]
struct Program {
    model: String,
    commands: Vec<Command>,
}

#[derive(Debug, Clone, Default)]
struct Script {
    bitmaps: HashMap<String, String>,
    textures: HashMap<String, String>,
    programs: Vec<Program>,
}

fn read_script(text: &str) -> Script {
    let w = words(text);
    let mut s = Script::default();
    let mut i = 0;
    let at = |i: usize| w.get(i).map(String::as_str).unwrap_or("");
    while i < w.len() {
        match at(i) {
            k @ ("BITMAPS" | "TEXTURES") => {
                i += 1;
                while at(i) != "END" && i < w.len() {
                    let (name, file) = (at(i).to_string(), base_name(at(i + 1)));
                    match k {
                        "BITMAPS" => s.bitmaps.insert(name, file),
                        _ => s.textures.insert(name, file),
                    };
                    i += 2;
                }
                i += 1;
            }
            "RELATIONS" => {
                while at(i) != "END" && i < w.len() {
                    i += 1;
                }
                i += 1;
            }
            "BEGIN" => {
                let model = base_name(at(i + 1));
                i += 2;
                let mut commands = Vec::new();
                let mut depth = 0;
                while i < w.len() {
                    match at(i) {
                        "END" => {
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                            commands.push(Command::End);
                        }
                        "LOOP" => {
                            commands.push(Command::Loop(number(at(i + 1))));
                            depth += 1;
                            i += 2;
                        }
                        "WAIT" => {
                            commands.push(Command::Wait(number(at(i + 1))));
                            i += 2;
                        }
                        name if at(i + 1) == "(" => {
                            let mut args = Vec::new();
                            i += 2;
                            while at(i) != ")" && i < w.len() {
                                args.push(at(i).to_string());
                                i += 1;
                            }
                            i += 1;
                            commands.push(Command::Call(name.to_string(), args));
                        }
                        _ => i += 1,
                    }
                }
                s.programs.push(Program { model, commands });
            }
            _ => i += 1,
        }
    }
    s
}

// ---------------------------------------------------------------------------------------
// Running objects (0x103d0).

struct Frame {
    count: i32,
    start: usize,
}

struct Actor {
    program: Program,
    model: Model,
    pc: usize,
    loops: Vec<Frame>,
    wait: i32,
    visible: bool,
    /// Turns about the object itself and about its place, with their speeds.
    own: [i32; 3],
    own_speed: [i32; 3],
    about: [i32; 3],
    about_speed: [i32; 3],
    place: Vec3,
    speed: [i32; 3],
    /// Where the object is from its group's middle (RELATION).
    offset: Vec3,
    /// The point others in its group follow, when it leads one.
    pivot: Vec3,
    leads: Option<String>,
    follows: Option<String>,
    /// Screen x and y in pixels, and depth, for every vertex.
    screen: Vec<Vec3>,
}

fn angle(a: i32) -> f32 {
    a as f32 * DEGREE
}

fn wrap(a: &mut i32) {
    if *a > TURN {
        *a -= TURN;
    }
    if *a < 0 {
        *a += TURN;
    }
}

/// Turns about x, then y, then z as the program writes it out (0x117c7).
fn turn(p: Vec3, a: [i32; 3]) -> Vec3 {
    let [(sx, cx), (sy, cy), (sz, cz)] = a.map(|v| angle(v).sin_cos());
    let b = p.y * sx + p.z * cx;
    let c = p.y * cx - p.z * sx;
    let d = b * sy + p.x * cy;
    let z = b * cy - p.x * sy;
    let y = c * cz - d * sz;
    let x = c * sz + d * cz;
    Vec3::new(x, y, z)
}

impl Actor {
    fn new(program: Program, model: Model) -> Self {
        let n = model.vertices.len();
        Self {
            program,
            model,
            pc: 0,
            loops: Vec::new(),
            wait: 0,
            visible: true,
            own: [0; 3],
            own_speed: [0; 3],
            about: [0; 3],
            about_speed: [0; 3],
            place: Vec3::ZERO,
            speed: [0; 3],
            offset: Vec3::ZERO,
            pivot: Vec3::ZERO,
            leads: None,
            follows: None,
            screen: vec![Vec3::ZERO; n],
        }
    }

    /// Commands until one waits, then a frame of motion. The whole program repeats.
    fn step(&mut self, s: &mut Shared, sizes: &HashMap<String, (u32, u32)>) {
        let mut budget = 10_000;
        while self.wait == 0 && budget > 0 {
            budget -= 1;
            if self.pc >= self.program.commands.len() {
                self.pc = 0;
                self.loops.clear();
            }
            let c = self.program.commands[self.pc].clone();
            self.pc += 1;
            match c {
                Command::Wait(n) => self.wait = n,
                Command::Loop(n) => self.loops.push(Frame {
                    count: n,
                    start: self.pc,
                }),
                Command::End => {
                    if let Some(top) = self.loops.last_mut() {
                        if top.count == 0 {
                            self.loops.pop();
                        } else {
                            self.pc = top.start;
                            if top.count > 0 {
                                top.count -= 1;
                            }
                        }
                    }
                }
                Command::Call(name, args) => self.call(&name, &args, s, sizes),
            }
        }
        if self.wait > 0 {
            self.wait -= 1;
        }
        self.motion(s);
    }

    fn call(
        &mut self,
        name: &str,
        a: &[String],
        s: &mut Shared,
        sizes: &HashMap<String, (u32, u32)>,
    ) {
        let at = |i: usize| a.get(i).map(String::as_str).unwrap_or("");
        let axis = |w: &str| match w {
            "X" => Some(0),
            "Y" => Some(1),
            "Z" => Some(2),
            _ => None,
        };
        match name {
            // Speeds add up; ONCE turns at once instead (0x10519 case 0).
            "TURN" => {
                let mut own = true;
                let (mut use_own, mut use_about) = (true, false);
                let mut temp = [[0; 3]; 2];
                let mut i = 0;
                while i < a.len() {
                    let w = at(i);
                    if let Some(k) = axis(w) {
                        temp[usize::from(own)][k] = number(at(i + 1));
                        i += 2;
                        continue;
                    }
                    match w {
                        "FRAMES" => {
                            self.wait = number(at(i + 1)) & 0xffff;
                            i += 1;
                        }
                        "RELATIV" => {
                            own = true;
                            use_own = true;
                        }
                        "ABSOLUT" => {
                            own = false;
                            use_about = true;
                        }
                        "STOP" => match own {
                            true => self.own_speed = [0; 3],
                            false => self.about_speed = [0; 3],
                        },
                        "ONCE" => {
                            for k in 0..3 {
                                match own {
                                    true => self.own[k] += temp[1][k],
                                    false => self.about[k] += temp[0][k],
                                }
                            }
                            temp = [[0; 3]; 2];
                        }
                        "RESET" => match own {
                            true => self.own = [0; 3],
                            false => self.about = [0; 3],
                        },
                        _ => {}
                    }
                    i += 1;
                }
                for k in 0..3 {
                    if use_own {
                        self.own_speed[k] += temp[1][k];
                    }
                    if use_about {
                        self.about_speed[k] += temp[0][k];
                    }
                }
            }
            // Speeds are set, not added (case 1).
            "MOVE" => {
                let mut speed = [0; 3];
                let mut i = 0;
                while i < a.len() {
                    let w = at(i);
                    if let Some(k) = axis(w) {
                        speed[k] = number(at(i + 1));
                        i += 2;
                        continue;
                    }
                    match w {
                        "FRAMES" => {
                            self.wait = number(at(i + 1)) & 0xffff;
                            i += 1;
                        }
                        "STOP" => speed[0] = 0,
                        _ => {}
                    }
                    i += 1;
                }
                self.speed = speed;
            }
            "SET" => self.set(a, s, sizes),
            // Vertices are scaled for good; the box only grows (case 3).
            "SIZE" => {
                let mut scale = Vec3::splat(100.0);
                let mut i = 0;
                while i < a.len() {
                    if let Some(k) = axis(at(i)) {
                        scale[k] = number(at(i + 1)) as f32;
                        i += 1;
                    }
                    i += 1;
                }
                let scale = scale * 0.1;
                let m = &mut self.model;
                for v in &mut m.vertices {
                    *v *= scale;
                    m.min = m.min.min(*v);
                    m.max = m.max.max(*v);
                }
                for l in 0..m.lists.len() {
                    texture_coordinates(m, l, sizes);
                }
            }
            _ => {}
        }
    }

    fn set(&mut self, a: &[String], s: &mut Shared, sizes: &HashMap<String, (u32, u32)>) {
        let at = |i: usize| a.get(i).map(String::as_str).unwrap_or("");
        let mut i = 1;
        match at(0) {
            "POSITION" => {
                while i < a.len() {
                    let v = number(at(i + 1)) as f32;
                    match at(i) {
                        "X" => self.place.x = v,
                        "Y" => self.place.y = v,
                        "Z" => self.place.z = v * Z_UNIT,
                        _ => {
                            i += 1;
                            continue;
                        }
                    }
                    i += 2;
                }
            }
            "RELATION" => {
                while i < a.len() {
                    let v = number(at(i + 1)) as f32;
                    match at(i) {
                        "X" => self.offset.x = v,
                        "Y" => self.offset.y = v,
                        "Z" => self.offset.z = v,
                        "MASTER" => self.leads = Some(at(i + 1).to_string()),
                        "SLAVE" => self.follows = Some(at(i + 1).to_string()),
                        _ => {
                            i += 1;
                            continue;
                        }
                    }
                    i += 2;
                }
            }
            "OBJECT" => match at(1) {
                "ON" => self.visible = true,
                "OFF" => self.visible = false,
                _ => {}
            },
            "BITMAP" => {
                while i < a.len() {
                    match at(i) {
                        "NAME" => {
                            s.bitmap = Some(at(i + 1).to_string());
                            i += 1;
                        }
                        "ON" => s.background = true,
                        "OFF" => s.background = false,
                        _ => {}
                    }
                    i += 1;
                }
            }
            "TRI_LIST" => {
                let n = number(at(1)).max(1) as usize - 1;
                let Some(list) = self.model.lists.get_mut(n) else {
                    return;
                };
                let mut i = 2;
                let mut again = false;
                while i < a.len() {
                    match at(i) {
                        "TEXTURE" => {
                            list.texture = Some(at(i + 1).to_string());
                            again = true;
                            i += 1;
                        }
                        "RENDER" => {
                            list.render = match at(i + 1) {
                                "GOURAUD" => Render::Gouraud,
                                "UNLIT_PERSPECTIVE" | "LIT_PERSPECTIVE" => Render::Perspective,
                                _ => Render::Flat,
                            };
                            i += 1;
                        }
                        "TEX_UV" => {
                            list.plane = match at(i + 1) {
                                "XY" => Some(Plane::Xy),
                                "XZ" => Some(Plane::Xz),
                                "YZ" => Some(Plane::Yz),
                                _ => None,
                            };
                            list.stretch = (100, 100);
                            again = true;
                            i += 1;
                        }
                        "TEX_STRETCH" => {
                            list.stretch = (number(at(i + 1)), number(at(i + 2)));
                            again = true;
                            i += 2;
                        }
                        "FILTER" => {
                            list.linear = at(i + 1) == "LINEAR";
                            i += 1;
                        }
                        _ => {}
                    }
                    i += 1;
                }
                if again {
                    texture_coordinates(&mut self.model, n, sizes);
                }
            }
            _ => {}
        }
    }

    /// A frame of motion and the projection (0x11546).
    fn motion(&mut self, s: &mut Shared) {
        for k in 0..3 {
            self.own[k] += self.own_speed[k];
            wrap(&mut self.own[k]);
        }
        match self.follows.as_ref().and_then(|g| s.pivots.get(g)) {
            Some(p) => self.place = *p,
            None => {
                self.place.x += self.speed[0] as f32;
                self.place.y += self.speed[1] as f32;
                self.place.z += self.speed[2] as f32 * Z_UNIT;
            }
        }
        for k in 0..3 {
            self.about[k] += self.about_speed[k];
            wrap(&mut self.about[k]);
        }
        let scale = 1.0 / (self.place.z * GROWTH);
        if self.follows.is_none() {
            let o = turn(self.offset, self.about);
            self.pivot = Vec3::new(self.place.x - o.x, self.place.y - o.y, self.place.z + o.z);
            if let Some(g) = &self.leads {
                s.pivots.insert(g.clone(), self.pivot);
            }
        }
        for (v, out) in self.model.vertices.iter().zip(&mut self.screen) {
            let p = (turn(*v, self.own) + self.offset) * scale;
            let q = turn(p, self.about);
            let z = q.z + self.place.z;
            let xy = self.place.truncate() - q.truncate() * (FOCUS / z);
            *out = xy.extend(z);
        }
    }
}

/// Texture coordinates of a list from the box on its plane, in texels (0x10040). With a
/// stretch of one in u the v of planes with z is not stretched, as in the original.
fn texture_coordinates(m: &mut Model, l: usize, sizes: &HashMap<String, (u32, u32)>) {
    let list = &m.lists[l];
    let Some(plane) = list.plane else {
        return;
    };
    let (w, h) = list
        .texture
        .as_ref()
        .and_then(|t| sizes.get(t))
        .map_or((0.0, 0.0), |s| (s.0 as f32, s.1 as f32));
    let (su, sv) = (list.stretch.0 as f32 * 0.01, list.stretch.1 as f32 * 0.01);
    let (min, max) = (m.min, m.max);
    let (au, av) = match plane {
        Plane::Xy => (0, 1),
        Plane::Xz => (0, 2),
        Plane::Yz => (1, 2),
    };
    let ku = (w - 1.0) / (max[au] - min[au]);
    let kv = (h - 1.0) / (max[av] - min[av]);
    let uv: Vec<Vec2> = m
        .vertices
        .iter()
        .map(|p| {
            let u = if su == 1.0 { p[au] } else { p[au] * su };
            let v = match (plane, su == 1.0) {
                (Plane::Xy, _) => {
                    if sv == 1.0 {
                        p[av]
                    } else {
                        p[av] * sv
                    }
                }
                (_, true) => p[av],
                (_, false) => p[av] * sv,
            };
            Vec2::new((max[au] - u) * ku, (max[av] - v) * kv)
        })
        .collect();
    m.lists[l].uv = uv;
}

/// What the objects share: groups' points to follow and the background.
#[derive(Default)]
struct Shared {
    pivots: HashMap<String, Vec3>,
    bitmap: Option<String>,
    background: bool,
}

// ---------------------------------------------------------------------------------------
// Pictures: Windows bitmaps of 8, 16 (5-5-5) or 24 bits, top row first.

fn bitmap(d: &[u8]) -> Result<Image, Error> {
    let h = d.get(..54).ok_or("bitmap cut short")?;
    if &h[..2] != b"BM" {
        return Err("not a bitmap".into());
    }
    let int = |o: usize| u32::from_le_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]);
    let (start, width, height) = (int(10) as usize, int(18) as usize, int(22) as i32);
    let bits = u16::from_le_bytes([h[28], h[29]]);
    let rows = height.unsigned_abs() as usize;
    let stride = (width * usize::from(bits) / 8).div_ceil(4) * 4;
    let palette = d.get(14 + int(14) as usize..start).unwrap_or(&[]);
    let mut rgba = Vec::with_capacity(width * rows * 4);
    for r in 0..rows {
        let r = if height > 0 { rows - 1 - r } else { r };
        let row = d.get(start + r * stride..).ok_or("bitmap cut short")?;
        for x in 0..width {
            let px = match bits {
                8 => {
                    let i = usize::from(*row.get(x).ok_or("bitmap cut short")?) * 4;
                    let c = palette.get(i..i + 3).ok_or("bad palette")?;
                    [c[2], c[1], c[0], 255]
                }
                16 => {
                    let v = u16::from_le_bytes([row[2 * x], row[2 * x + 1]]);
                    let c = |s: u16| ((v >> s & 31) * 255 / 31) as u8;
                    [c(10), c(5), c(0), 255]
                }
                24 => [row[3 * x + 2], row[3 * x + 1], row[3 * x], 255],
                b => return Err(format!("{b} bit bitmaps").into()),
            };
            rgba.extend(px);
        }
    }
    Ok(Image {
        width: width as u32,
        height: rows as u32,
        rgba,
    })
}

// ---------------------------------------------------------------------------------------

/// Nearest and bilinear copies and the size of each texture, by name.
type Textures = HashMap<String, ([TextureId; 2], (u32, u32))>;

struct Show {
    actors: Vec<Actor>,
    shared: Shared,
}

pub struct Demo {
    scripts: [Script; 2],
    models: HashMap<String, Model>,
    /// Nearest and bilinear, by texture name of each script.
    textures: [Textures; 2],
    bitmaps: HashMap<String, (TextureId, [u8; 4])>,
    chapter: usize,
    show: Show,
    frames: u64,
    due: f64,
    paused: bool,
    aspect: f32,
}

const NEAREST: Sampling = Sampling {
    clamp: [false; 2],
    mips: false,
    linear: false,
    anisotropy: 1,
};
const BILINEAR: Sampling = Sampling {
    linear: true,
    ..NEAREST
};

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let text = |n: &str| -> Result<String, Error> {
            Ok(String::from_utf8_lossy(files.get(n)?).into_owned())
        };
        let scripts = SCRIPTS.map(|n| text(n).map(|t| read_script(&t)));
        let [a, b] = scripts;
        let scripts = [a?, b?];
        let mut models = HashMap::new();
        for p in scripts.iter().flat_map(|s| &s.programs) {
            if !models.contains_key(&p.model) {
                models.insert(p.model.clone(), read_model(&text(&p.model)?)?);
            }
        }
        let mut textures: [Textures; 2] = Default::default();
        let mut made: Textures = HashMap::new();
        for (k, s) in scripts.iter().enumerate() {
            for (name, file) in &s.textures {
                if !made.contains_key(file) {
                    let image = bitmap(files.get(file)?).map_err(|e| format!("{file}: {e}"))?;
                    let size = (image.width, image.height);
                    let ids = [
                        gl.texture(gpu, &image, NEAREST),
                        gl.texture(gpu, &image, BILINEAR),
                    ];
                    made.insert(file.clone(), (ids, size));
                }
                textures[k].insert(name.clone(), made[file]);
            }
        }
        let mut bitmaps = HashMap::new();
        for s in &scripts {
            for (name, file) in &s.bitmaps {
                // SCRIPT.SCP names back.bmp, which the demo does not have; it is never shown.
                let Ok(d) = files.get(file) else { continue };
                let image = bitmap(d).map_err(|e| format!("{file}: {e}"))?;
                let corner = image.rgba[..4].try_into().unwrap_or([0; 4]);
                bitmaps.insert(name.clone(), (gl.texture(gpu, &image, NEAREST), corner));
            }
        }
        let mut demo = Self {
            scripts,
            models,
            textures,
            bitmaps,
            chapter: 0,
            show: Show {
                actors: Vec::new(),
                shared: Shared::default(),
            },
            frames: 0,
            due: 0.0,
            paused: false,
            aspect: 4.0 / 3.0,
        };
        demo.start(0);
        Ok(demo)
    }

    fn sizes(&self) -> HashMap<String, (u32, u32)> {
        self.textures[self.chapter]
            .iter()
            .map(|(k, v)| (k.clone(), v.1))
            .collect()
    }

    fn start(&mut self, chapter: usize) {
        self.chapter = chapter;
        self.frames = 0;
        let actors = self.scripts[chapter]
            .programs
            .iter()
            .map(|p| Actor::new(p.clone(), self.models[&p.model].clone()))
            .collect();
        self.show = Show {
            actors,
            shared: Shared::default(),
        };
    }

    fn tick(&mut self) {
        let sizes = self.sizes();
        let Show { actors, shared } = &mut self.show;
        for a in actors.iter_mut() {
            a.step(shared, &sizes);
        }
        self.frames += 1;
    }

    pub fn step(&mut self, dt: f64) {
        if self.paused {
            return;
        }
        self.due += dt;
        while self.due >= FRAME - 1e-9 {
            self.due -= FRAME;
            self.tick();
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.tick());
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key {
            "1" => self.start(0),
            "2" => self.start(1),
            " " => self.paused = !self.paused,
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}

    pub fn wheel(&mut self, _amount: f64) {}

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let shown = self.show.actors.iter().filter(|a| a.visible).count();
        format!(
            "script {}, frame {}, {} of {} objects shown, {}",
            self.chapter + 1,
            self.frames,
            shown,
            self.show.actors.len(),
            if self.paused { "paused" } else { "running" }
        )
    }

    /// The 640 by 480 screen, kept whole and in shape inside the window.
    fn to_clip(&self, p: Vec2) -> Vec2 {
        let n = Vec2::new(p.x / MIDDLE.x - 1.0, 1.0 - p.y / MIDDLE.y);
        let screen = SCREEN.x / SCREEN.y;
        if self.aspect > screen {
            Vec2::new(n.x * screen / self.aspect, n.y)
        } else {
            Vec2::new(n.x, n.y * self.aspect / screen)
        }
    }

    /// The background, then every object's lists (0x11dbc's loop).
    pub fn draw(&mut self, _gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        self.aspect = w.max(1) as f32 / h.max(1) as f32;
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        let shared = &self.show.shared;
        let back = shared
            .background
            .then(|| shared.bitmap.as_ref().and_then(|b| self.bitmaps.get(b)))
            .flatten();
        if let Some((tex, _)) = back {
            let corners = [(0.0, 0.0), (640.0, 0.0), (640.0, 480.0), (0.0, 480.0)];
            let v = corners.map(|(x, y)| {
                let c = self.to_clip(Vec2::new(x, y));
                Vertex::new(Vec4::new(c.x, c.y, 0.0, 1.0), [1.0; 4]).uv(0, x / 640.0, y / 480.0)
            });
            gl.quad(State::new(TEXTURED), &Surface::textures(&[*tex]), v);
        }
        // Lists the scripts never set sample the screen's first pixel with no coordinates.
        let corner = back
            .map_or([0, 0, 0, 255], |b| b.1)
            .map(|c| f32::from(c) / 255.0);
        let textures = &self.textures[self.chapter];
        for a in self.show.actors.iter().filter(|a| a.visible) {
            for list in &a.model.lists {
                let tex = list.texture.as_ref().and_then(|t| textures.get(t));
                let vertex = |v: usize| {
                    let p = a.screen[v];
                    let c = self.to_clip(p.truncate());
                    let z = p.z.max(1e-3);
                    let depth = (p.z / DEPTH_FAR).clamp(0.0, 1.0);
                    let color = match (list.render, tex) {
                        (Render::Gouraud, _) => list.colors[v].map(|c| f32::from(c) / 255.0),
                        (_, None) => corner,
                        _ => [1.0; 4],
                    };
                    let (sw, sh) = tex.map_or((1.0, 1.0), |t| (t.1.0 as f32, t.1.1 as f32));
                    let uv = list.uv[v] / Vec2::new(sw, sh);
                    Vertex::new(Vec4::new(c.x * z, c.y * z, depth * z, z), color).uv(0, uv.x, uv.y)
                };
                let vertices: Vec<Vertex> = triangles(list)
                    .into_iter()
                    .flat_map(|t| t.map(vertex))
                    .collect();
                if vertices.is_empty() {
                    continue;
                }
                let (program, vs, surface) = match (list.render, tex) {
                    (Render::Gouraud, _) | (_, None) => (PLAIN, "vs", Surface::default()),
                    (render, Some(t)) => {
                        let id = t.0[usize::from(list.linear)];
                        match render {
                            Render::Perspective => (TEXTURED, "vs", Surface::textures(&[id])),
                            _ => (AFFINE, "flat", Surface::textures(&[id])),
                        }
                    }
                };
                let state = State::new(program).vertex(vs).depth(Depth::LessEqual(true));
                gl.draw(state, &surface, &vertices);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_with_a_point_are_hundredths() {
        assert_eq!(number("4.00"), 400);
        assert_eq!(number("-9.00"), -900);
        assert_eq!(number("15000"), 15000);
        assert_eq!(number("0.5"), 50);
    }

    #[test]
    fn scripts_read_loops_and_calls() {
        let s = read_script(
            "X\nTEXTURES\n A t\\a.tex\nEND\nBEGIN VOJ\\x.voj\n SET ( POSITION, X, 1 )\n LOOP -1\n  WAIT 3\n END\nEND\n",
        );
        assert_eq!(s.textures["A"], "a.tex");
        let p = &s.programs[0];
        assert_eq!(p.model, "x.voj");
        assert!(matches!(&p.commands[0], Command::Call(n, a) if n == "SET" && a.len() == 3));
        assert!(matches!(p.commands[1], Command::Loop(-1)));
        assert!(matches!(p.commands[2], Command::Wait(3)));
        assert!(matches!(p.commands[3], Command::End));
    }

    #[test]
    fn turns_by_hundredths_of_a_degree() {
        // The original's degree is a little large: 90 degrees come to 1.5719 radians.
        let p = turn(Vec3::X, [0, 0, 9000]);
        assert!((p - Vec3::new(0.0, -1.0, 0.0)).length() < 2e-3, "{p}");
        let mut a = 36_100;
        wrap(&mut a);
        assert_eq!(a, 100);
    }
}
