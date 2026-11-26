//! One rendering pass of a shape: its program, textures, state and constants, as the
//! scene describes them.
use std::collections::HashMap;
use std::fmt::Write;

use glam::{Mat4, Vec3, Vec4};

use crate::combiner::{Fog, Shader};
use crate::tree::Node;
use crate::{Error, demo, vp};

/// The named constant that stands for the ambient color of lights.
pub const AMBIENT: &str = "ambient";
/// The named constants that stand for a light: a point, and a color.
pub const LIGHT: &str = "light point";
pub const LIGHT_COLOR: &str = "light color";
/// Registers of the constant colors that a program sets: the direction toward the light
/// in the shape's space, and its color.
pub const COLORS: usize = 94;
/// Constant registers of a pass.
pub const CONSTANTS: usize = 96;
pub type Constants = Vec<[f32; 4]>;

/// Where an attribute's values come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stream {
    Coords(usize),
    Normals(usize),
    Colors(usize),
    /// A list of the geometry, not a texture unit.
    TexCoords(usize),
    Data(usize),
}

/// The matrices a program can ask for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Matrix {
    Projection,
    View,
    Model,
    Texture(usize),
    Spare(usize),
    ModelView,
    ViewProjection,
    ModelViewProjection,
}

impl Matrix {
    /// The first GeForce 3 viewers have eight spare matrices, later ones sixteen.
    fn read(n: u32, spares: u32) -> Result<Self, Error> {
        Ok(match n {
            0 => Self::Projection,
            1 => Self::View,
            2 => Self::Model,
            3..11 => Self::Texture(n as usize - 3),
            n if n < 11 + spares => Self::Spare(n as usize - 11),
            n if n == 11 + spares => Self::ModelView,
            n if n == 12 + spares => Self::ViewProjection,
            n if n == 13 + spares => Self::ModelViewProjection,
            n => return Err(format!("unsupported matrix {n}").into()),
        })
    }
}

#[derive(Debug, Clone)]
struct Factor {
    matrix: Matrix,
    transpose: bool,
    invert: bool,
    /// The matrix as it is at rest.
    initial: bool,
}

#[derive(Debug, Clone)]
enum Param {
    Matrices(usize, Vec<Factor>, bool, bool),
    Light(usize, Light),
    Material(usize, Material),
    Static(usize, String, Vec<[f32; 4]>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightKind {
    Directional,
    Point,
    Spot,
}

#[derive(Debug, Clone)]
pub struct Light {
    pub name: String,
    pub kind: LightKind,
    /// The node it moves with.
    pub node: Option<usize>,
    pub eye_space: bool,
    pub ambient: Vec4,
    pub diffuse: Vec4,
    pub specular: Vec4,
    pub position: Vec3,
    pub direction: Vec3,
    pub cutoff: f32,
    pub attenuation: Vec3,
}

#[derive(Debug, Clone, Copy)]
pub struct Material {
    pub diffuse: Vec4,
    pub specular: Vec4,
    pub ambient: Vec4,
    pub emissive: Vec4,
    pub shininess: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            diffuse: Vec4::new(0.8, 0.8, 0.8, 1.0),
            specular: Vec4::new(0.0, 0.0, 0.0, 1.0),
            ambient: Vec4::new(0.2, 0.2, 0.2, 1.0),
            emissive: Vec4::new(0.0, 0.0, 0.0, 1.0),
            shininess: 0.0,
        }
    }
}

impl Material {
    pub fn read(n: &Node) -> Self {
        let v = |f| n.vec::<4>(f).map_or(Vec4::ZERO, Vec4::from);
        Self {
            diffuse: v("diffuseColor"),
            specular: v("specularColor"),
            ambient: v("ambientColor"),
            emissive: v("emissiveColor"),
            shininess: n.float("shininess"),
        }
    }
}

impl Light {
    pub fn read(n: &Node, find: impl Fn(&str) -> Option<usize>) -> Option<Self> {
        let kind = match n.kind {
            "DirectionalLight" => LightKind::Directional,
            "PointLight" => LightKind::Point,
            "SpotLight" | "ShadowSpotLight" => LightKind::Spot,
            _ => return None,
        };
        let v3 = |f| n.vec::<3>(f).map_or(Vec3::ZERO, Vec3::from);
        let v4 = |f| n.vec::<4>(f).map_or(Vec4::ZERO, Vec4::from);
        Some(Self {
            name: n.name.clone(),
            kind,
            node: Some(n.text("transform"))
                .filter(|t| !t.is_empty())
                .and_then(find),
            eye_space: n.bool("isEyespace", false),
            ambient: v4("ambient"),
            diffuse: v4("diffuse"),
            specular: v4("specular"),
            position: v3("position"),
            direction: v3("direction"),
            cutoff: n.float("outer"),
            attenuation: v3("attenuation"),
        })
    }

    /// To the camera's space.
    fn matrix(&self, c: &Context<'_>) -> Mat4 {
        match (self.eye_space, self.node) {
            (true, _) => Mat4::IDENTITY,
            (false, Some(n)) => c.view * c.world[n],
            (false, None) => c.view,
        }
    }

    /// Toward the light, or for spot lights where it shines.
    fn direction(&self, c: &Context<'_>) -> Vec3 {
        self.matrix(c)
            .transform_vector3(self.direction)
            .normalize_or_zero()
    }
}

/// What a frame supplies to the constants.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub projection: Mat4,
    pub view: Mat4,
    /// Transforms of the model's nodes, now and at rest.
    pub world: &'a [Mat4],
    pub rest: &'a [Mat4],
    /// Values for named constants, instead of those in the scene.
    pub values: &'a HashMap<&'static str, [f32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct State {
    /// Source and destination factors.
    pub blend: Option<(u32, u32)>,
    /// Comparison and whether depth is written.
    pub depth: Option<(u32, bool)>,
    /// 0 culls front faces, 1 back faces, 3 none.
    pub cull: u32,
    pub ccw_front: bool,
    pub color: bool,
    pub alpha: bool,
}

#[derive(Clone)]
pub struct Pass {
    /// The whole program, in WGSL.
    pub wgsl: String,
    /// What attribute `@location(i)` reads.
    pub streams: Vec<Stream>,
    pub state: State,
    /// Textures of the model, by unit.
    pub textures: [Option<usize>; 4],
    /// The kind of each unit's texture: cube map or plane.
    pub units: [Option<bool>; 4],
    pub node: usize,
    spares: Vec<usize>,
    texture_nodes: [Option<usize>; 4],
    program: Program,
    dynamic: bool,
    /// Drawn before the picture, into this one of the stage's textures.
    pub offscreen: Option<usize>,
}

#[derive(Clone)]
enum Program {
    Vertex(Vec<Param>),
    Fixed(Box<Fixed>),
}

#[derive(Clone)]
struct Fixed {
    lights: Vec<Light>,
    material: Material,
    ambient: Vec4,
    /// How coordinates of each unit are made.
    texgen: [u32; 4],
}

/// What the pass reader needs from the scene.
pub struct Resources<'a> {
    pub root: &'a Node,
    pub spares: u32,
    pub lights: &'a [Light],
    /// Program text by file name.
    pub program: &'a dyn Fn(&str) -> Result<String, Error>,
    pub find: &'a dyn Fn(&str) -> Option<usize>,
    /// Index and kind of a texture.
    pub texture: &'a dyn Fn(&str) -> Option<(usize, bool)>,
}

pub fn named<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Node> {
    let name = name.trim();
    nodes
        .iter()
        .find(|n| n.name.trim().eq_ignore_ascii_case(name))
}

const HEADER: &str = "
struct Draw { c: array<vec4f, 96> }
@group(0) @binding(0) var<uniform> draw: Draw;
struct Varyings {
    @invariant @builtin(position) position: vec4f,
    @location(0) col0: vec4f,
    @location(1) col1: vec4f,
    @location(2) tex0: vec4f,
    @location(3) tex1: vec4f,
    @location(4) tex2: vec4f,
    @location(5) tex3: vec4f,
    @location(6) fog: f32,
}
fn project(t: vec4f) -> vec2f { return t.xy / select(t.w, 1.0, t.w == 0.0); }
";

/// Attributes a vertex program reads: 1 is the first list of coordinates, and so on.
fn stream(register: u32, coordinates: &[u32]) -> Option<Stream> {
    let n = register.checked_sub(1)? as usize;
    let (kind, i) = (n / 8, n % 8);
    Some(match kind {
        0 => Stream::Coords(i),
        1 => Stream::Normals(i),
        2 => Stream::Colors(i),
        3 => Stream::TexCoords(coordinates.get(i).map_or(i, |&x| x as usize)),
        4 => Stream::Data(i),
        _ => return None,
    })
}

fn compare_alpha(func: u32, reference: f32) -> Result<String, Error> {
    let op = match func {
        0 => return Ok("  discard;\n".into()),
        1 => "<",
        2 => "<=",
        3 => ">",
        4 => ">=",
        5 => "==",
        6 => "!=",
        7 => return Ok(String::new()),
        n => return Err(format!("unsupported alpha test {n}").into()),
    };
    Ok(format!(
        "  if !(color.a {op} {reference:?}) {{ discard; }}\n"
    ))
}

fn params(n: &Node, r: &Resources<'_>) -> Result<Vec<Param>, Error> {
    let mut out = Vec::new();
    for p in n.nodes("constantParams") {
        let at = p.int("startingRegister") as usize;
        let factor = |m: &Node| -> Result<Factor, Error> {
            Ok(Factor {
                matrix: Matrix::read(m.int("matrix"), r.spares)?,
                transpose: m.bool("transpose", false),
                invert: m.bool("invert", false),
                initial: m.bool("initialValue", false),
            })
        };
        let flag = |f| p.bool(f, false);
        out.push(match p.kind {
            "MatrixParam" => Param::Matrices(at, vec![factor(p)?], false, false),
            "EvaluatedMatrixParam" => Param::Matrices(
                at,
                p.nodes("matrices")
                    .iter()
                    .map(factor)
                    .collect::<Result<_, _>>()?,
                flag("transpose"),
                flag("invert"),
            ),
            "LightParam" => {
                let light = r.lights.iter().find(|l| l.name == p.text("light"));
                Param::Light(at, light.ok_or("constants of an unknown light")?.clone())
            }
            "MaterialParam" => {
                let node = named(r.root.nodes("materials"), p.text("material"));
                Param::Material(at, node.map(Material::read).unwrap_or_default())
            }
            "StaticParam" => {
                let Some(value) = p.node("value") else {
                    continue;
                };
                let width = match value.kind {
                    "Vec1f" => 1,
                    "Vec2f" | "Vec2fArray" => 2,
                    "Vec3f" | "Vec3fArray" => 3,
                    _ => 4,
                };
                let rows = value.floats("value").chunks(width);
                let row = |v: &[f32]| std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0));
                Param::Static(at, p.text("paramName").to_owned(), rows.map(row).collect())
            }
            kind => return Err(format!("unsupported constant {kind}").into()),
        });
    }
    Ok(out)
}

impl Pass {
    /// `inherited` is the node of the group that holds the shape.
    pub fn read(n: &Node, inherited: usize, r: &Resources<'_>) -> Result<Self, Error> {
        let render = r.root.nodes("renderParams");
        let setting = |field| named(render, n.text(field));
        let node = (r.find)(n.text("modelTransform")).unwrap_or(inherited);
        let mut textures = [None; 4];
        let mut units = [None; 4];
        for (i, name) in n.texts("textures").iter().take(4).enumerate() {
            if let Some((index, cube)) = (r.texture)(name) {
                (textures[i], units[i]) = (Some(index), Some(cube));
            }
        }
        let mut texture_nodes = [None; 4];
        for (i, name) in n.texts("textureTransforms").iter().take(4).enumerate() {
            texture_nodes[i] = (r.find)(name);
        }
        let spares = n
            .texts("spareTransforms")
            .iter()
            .map(|s| (r.find)(s).unwrap_or(0))
            .collect();

        let misc = setting("miscParams");
        let depth = setting("depthTestParams");
        let blend =
            setting("blendParams").map_or((1, 0), |b| (b.int("srcBlend"), b.int("destBlend")));
        let state = State {
            blend: (blend != (1, 0)).then_some(blend),
            depth: match depth {
                Some(d) if d.bool("depthTestEnabled", true) => {
                    Some((d.int("depthTestFunc"), d.bool("depthMask", true)))
                }
                // Without a test nothing is written either.
                Some(_) => None,
                None => Some((1, true)),
            },
            cull: setting("cullParams").map_or(1, |c| c.int("cullFace")),
            ccw_front: setting("cullParams").is_none_or(|c| c.bool("ccwIsFront", true)),
            color: misc.is_none_or(|m| m.bool("redMask", true)),
            alpha: misc.is_none_or(|m| m.bool("alphaMask", true)),
        };

        let coordinates = n.ints("texcoordIndices");
        let mut wgsl = String::from(HEADER);
        for (i, cube) in units.iter().enumerate() {
            let ty = if *cube == Some(true) {
                "texture_cube<f32>"
            } else {
                "texture_2d<f32>"
            };
            let _ = writeln!(wgsl, "@group(1) @binding({i}) var map{i}: {ty};");
            let _ = writeln!(
                wgsl,
                "@group(1) @binding({}) var sampler{i}: sampler;",
                i + 4
            );
        }
        let fog = setting("fogParams")
            .filter(|f| f.bool("fogEnable", false))
            .map(|f| Fog {
                color: f.vec::<4>("fogColor").unwrap_or([0.0; 4]),
                start: f.float("fogStart"),
                end: f.float("fogEnd"),
            });

        let mut streams = Vec::new();
        let program = match n.text("vertexProgram") {
            "" => {
                let fixed = Fixed::read(n, misc, r);
                wgsl.push_str(&fixed.wgsl(misc, coordinates, &units, &mut streams));
                Program::Fixed(Box::new(fixed))
            }
            name => {
                let p = named(r.root.nodes("vertexPrograms"), name)
                    .ok_or_else(|| format!("missing program {name}"))?;
                let file = p.text("filename");
                let body =
                    vp::translate(&(r.program)(file)?).map_err(|e| format!("{file}: {e}"))?;
                let mut registers = [None; 16];
                for (i, slot) in registers.iter_mut().enumerate() {
                    let field = REGISTERS[i];
                    *slot = stream(p.int(field), coordinates);
                }
                wgsl.push_str(&vertex_program(&body, &registers, &mut streams));
                Program::Vertex(params(p, r)?)
            }
        };
        let name = n.text("pixelShader");
        let dynamic = demo::DYNAMIC.contains(&name);
        let shader = match name {
            "" => None,
            name => named(r.root.nodes("pixelShaders"), name)
                .map(|n| Shader::read(n, dynamic))
                .transpose()?,
        };
        let shader = shader.unwrap_or_default();
        wgsl.push_str(&shader.wgsl(units, fog)?);
        let mut test = String::new();
        if let Some(a) = setting("alphaTestParams").filter(|a| a.bool("alphaTestEnabled", false)) {
            let reference = a.int("alphaTestRef") as f32 / 255.0;
            test = compare_alpha(a.int("alphaTestFunc"), reference)?;
        }
        let (output, result) = match shader.writes_depth() {
            true => ("Shaded", "Shaded(color, depth)"),
            false => ("@location(0) vec4f", "color"),
        };
        let _ = write!(
            wgsl,
            "struct Shaded {{ @location(0) color: vec4f, @builtin(frag_depth) depth: f32 }}\n\
             @fragment fn fs(s: Varyings) -> {output} {{\n  let color = shade(s);\n{test}  return {result};\n}}\n"
        );
        Ok(Self {
            wgsl,
            streams,
            state,
            textures,
            units,
            node,
            spares,
            texture_nodes,
            program,
            dynamic,
            offscreen: None,
        })
    }

    /// Lights of a pass without a program.
    pub fn lights_mut(&mut self) -> &mut [Light] {
        match &mut self.program {
            Program::Fixed(f) => &mut f.lights,
            Program::Vertex(_) => &mut [],
        }
    }

    /// The same shape in one color.
    pub fn flat(&self, color: [f32; 4], state: State) -> Self {
        let [r, g, b, a] = color;
        let color = format!("let color = vec4f({r:?}, {g:?}, {b:?}, {a:?});");
        Self {
            wgsl: self.wgsl.replace("let color = shade(s);", &color),
            state,
            ..self.clone()
        }
    }

    fn factor(&self, f: &Factor, c: &Context<'_>) -> Mat4 {
        let world = if f.initial { c.rest } else { c.world };
        let model = || world[self.node];
        let m = match f.matrix {
            Matrix::Projection => c.projection,
            Matrix::View => c.view,
            Matrix::Model => model(),
            Matrix::Texture(i) => self
                .texture_nodes
                .get(i)
                .copied()
                .flatten()
                .map_or(Mat4::IDENTITY, |n| world[n]),
            Matrix::Spare(i) => self.spares.get(i).map_or(Mat4::IDENTITY, |&n| world[n]),
            Matrix::ModelView => c.view * model(),
            Matrix::ViewProjection => c.projection * c.view,
            Matrix::ModelViewProjection => c.projection * c.view * model(),
        };
        change(m, f.transpose, f.invert)
    }

    /// The constants for a frame, no more than are used.
    pub fn constants(&self, c: &Context<'_>) -> Constants {
        let mut out = Constants::new();
        match &self.program {
            Program::Vertex(params) => {
                for p in params {
                    match p {
                        Param::Matrices(at, factors, transpose, invert) => {
                            let m = factors
                                .iter()
                                .fold(Mat4::IDENTITY, |m, f| m * self.factor(f, c));
                            rows(&mut out, *at, change(m, *transpose, *invert));
                        }
                        Param::Light(at, l) => {
                            let d = l.direction(c).extend(0.0).to_array();
                            let ambient = c.values.get(AMBIENT).copied();
                            put(
                                &mut out,
                                *at,
                                &[
                                    ambient.unwrap_or(l.ambient.to_array()),
                                    l.diffuse.to_array(),
                                    l.specular.to_array(),
                                    d,
                                ],
                            );
                        }
                        Param::Material(at, m) => put(
                            &mut out,
                            *at,
                            &[
                                m.ambient.to_array(),
                                m.diffuse.to_array(),
                                m.specular.to_array(),
                                m.emissive.to_array(),
                            ],
                        ),
                        Param::Static(at, name, value) => match c.values.get(name.as_str()) {
                            Some(v) => put(&mut out, *at, &[*v]),
                            None => put(&mut out, *at, value),
                        },
                    }
                }
            }
            Program::Fixed(f) => {
                let view = c.view * c.world[self.node];
                rows(&mut out, 0, c.projection * view);
                rows(&mut out, 4, view);
                for i in 0..4 {
                    let m = self.texture_nodes[i].map_or(Mat4::IDENTITY, |n| c.world[n]);
                    let g = match f.texgen[i] {
                        2..=4 => Mat4::from_mat3(glam::Mat3::from_mat4(view).transpose()),
                        8 => c.world[self.node],
                        9..=11 => Mat4::from_mat3(glam::Mat3::from_mat4(c.view).transpose()),
                        _ => Mat4::IDENTITY,
                    };
                    rows(&mut out, 8 + 4 * i, m * g);
                }
                let m = &f.material;
                let base = m.emissive + f.ambient * m.ambient;
                put(
                    &mut out,
                    24,
                    &[
                        base.truncate().extend(m.diffuse.w).to_array(),
                        [m.shininess, f.lights.len() as f32, 0.0, 0.0],
                    ],
                );
                for (i, l) in f.lights.iter().enumerate() {
                    let direction = l.direction(c);
                    let position = match l.kind {
                        LightKind::Directional => direction.extend(0.0),
                        _ => l.matrix(c).transform_point3(l.position).extend(1.0),
                    };
                    let cutoff = match l.kind {
                        LightKind::Spot => l.cutoff.cos(),
                        _ => -1.0,
                    };
                    put(
                        &mut out,
                        26 + 6 * i,
                        &[
                            position.to_array(),
                            (l.ambient * m.ambient).truncate().extend(cutoff).to_array(),
                            (l.diffuse * m.diffuse).truncate().extend(0.0).to_array(),
                            (l.specular * m.specular).truncate().extend(0.0).to_array(),
                            direction.extend(0.0).to_array(),
                            l.attenuation.extend(0.0).to_array(),
                        ],
                    );
                }
            }
        }
        if self.dynamic {
            let get = |name| c.values.get(name).copied();
            let [x, y, z, _] = get(LIGHT).unwrap_or([0.0, 1e6, 0.0, 1.0]);
            let inverse = c.world[self.node].inverse();
            let toward = inverse
                .transform_point3(Vec3::new(x, y, z))
                .normalize_or(Vec3::Y);
            put(
                &mut out,
                COLORS,
                &[
                    (toward * 0.5 + 0.5).extend(0.0).to_array(),
                    get(LIGHT_COLOR).unwrap_or([1.0, 1.0, 1.0, 0.1]),
                ],
            );
        }
        out
    }
}

fn change(m: Mat4, transpose: bool, invert: bool) -> Mat4 {
    let m = if invert { m.inverse() } else { m };
    if transpose { m.transpose() } else { m }
}

pub fn put(out: &mut Constants, at: usize, values: &[[f32; 4]]) {
    let end = (at + values.len()).min(CONSTANTS);
    if out.len() < end {
        out.resize(end, [0.0; 4]);
    }
    for (slot, v) in out[at.min(end)..end].iter_mut().zip(values) {
        *slot = *v;
    }
}

fn rows(out: &mut Constants, at: usize, m: Mat4) {
    put(out, at, &[0, 1, 2, 3].map(|i| m.row(i).to_array()));
}

const REGISTERS: [&str; 16] = [
    "vertexRegister0",
    "vertexRegister1",
    "vertexRegister2",
    "vertexRegister3",
    "vertexRegister4",
    "vertexRegister5",
    "vertexRegister6",
    "vertexRegister7",
    "vertexRegister8",
    "vertexRegister9",
    "vertexRegister10",
    "vertexRegister11",
    "vertexRegister12",
    "vertexRegister13",
    "vertexRegister14",
    "vertexRegister15",
];

const FINISH: &str = "
    var out: Varyings;
    out.position = vec4f(hpos.xy, (hpos.z + hpos.w) * 0.5, hpos.w);
    out.col0 = saturate(col0);
    out.col1 = saturate(col1);
    out.tex0 = tex0;
    out.tex1 = tex1;
    out.tex2 = tex2;
    out.tex3 = tex3;
    out.fog = fogc.x;
    return out;
}
";

fn attribute(streams: &mut Vec<Stream>, s: Stream) -> usize {
    streams.iter().position(|x| *x == s).unwrap_or_else(|| {
        streams.push(s);
        streams.len() - 1
    })
}

fn inputs(streams: &[Stream]) -> String {
    let mut s = String::from("struct Input {\n");
    for i in 0..streams.len() {
        let _ = writeln!(s, "    @location({i}) a{i}: vec4f,");
    }
    s.push_str("}\n@vertex fn vs(v: Input) -> Varyings {\n");
    s
}

fn vertex_program(
    body: &str,
    registers: &[Option<Stream>; 16],
    streams: &mut Vec<Stream>,
) -> String {
    let at = registers.map(|r| r.map(|r| attribute(streams, r)));
    if streams.is_empty() {
        streams.push(Stream::Coords(0));
    }
    let mut s = String::from(vp::PRELUDE);
    s.push_str(&inputs(streams));
    for (i, a) in at.iter().enumerate() {
        let value = match a {
            Some(a) => format!("v.a{a}"),
            None => "vec4f(0.0, 0.0, 0.0, 1.0)".to_owned(),
        };
        let _ = writeln!(s, "    let v{i} = {value};");
    }
    for i in 0..12 {
        let _ = writeln!(s, "    var r{i} = vec4f(0.0);");
    }
    s.push_str("    var a0 = 0;\n    var hpos = vec4f(0.0, 0.0, 0.0, 1.0);\n");
    for name in ["col0", "col1", "fogc", "tex0", "tex1", "tex2", "tex3"] {
        let _ = writeln!(s, "    var {name} = vec4f(0.0, 0.0, 0.0, 1.0);");
    }
    s.push_str(body);
    s.push_str(FINISH);
    s
}

impl Fixed {
    fn read(n: &Node, misc: Option<&Node>, r: &Resources<'_>) -> Self {
        let material = named(r.root.nodes("materials"), n.text("material"));
        let mut texgen = [0; 4];
        for (slot, g) in texgen.iter_mut().zip(n.ints("texGenFuncs")) {
            *slot = *g;
        }
        let lit = misc.is_some_and(|m| m.bool("lightingEnable", false));
        let wanted = n.texts("lights");
        Self {
            lights: match lit {
                true => r
                    .lights
                    .iter()
                    .filter(|l| wanted.contains(&l.name))
                    .cloned()
                    .collect(),
                false => Vec::new(),
            },
            material: material.map(Material::read).unwrap_or_default(),
            ambient: misc
                .and_then(|m| m.vec::<4>("ambientColor"))
                .map_or(Vec4::ZERO, Vec4::from),
            texgen,
        }
    }

    /// The vertex stage of OpenGL 1.2: lighting, and coordinates made or transformed.
    fn wgsl(
        &self,
        misc: Option<&Node>,
        coordinates: &[u32],
        units: &[Option<bool>; 4],
        streams: &mut Vec<Stream>,
    ) -> String {
        attribute(streams, Stream::Coords(0));
        attribute(streams, Stream::Normals(0));
        let mut tex = String::new();
        for (i, unit) in units.iter().enumerate() {
            let value = match (unit, self.texgen[i]) {
                (None, _) => "vec4f(0.0, 0.0, 0.0, 1.0)".to_owned(),
                (_, 0) => {
                    let list = coordinates.get(i).map_or(i, |&x| x as usize);
                    format!("v.a{}", attribute(streams, Stream::TexCoords(list)))
                }
                (_, 1 | 8) => "object".to_owned(),
                (_, 12) => "eye".to_owned(),
                (_, 2 | 5 | 9 | 13) => "vec4f(normal, 1.0)".to_owned(),
                (_, 4 | 7 | 11 | 15) => "sphere(normalize(eye.xyz), normal)".to_owned(),
                _ => "vec4f(reflect(normalize(eye.xyz), normal), 1.0)".to_owned(),
            };
            let _ = writeln!(tex, "    let coord{i} = {value};");
            let row = |k: usize| format!("dot(draw.c[{}], coord{i})", 8 + 4 * i + k);
            let _ = writeln!(
                tex,
                "    let tex{i} = vec4f({}, {}, {}, {});",
                row(0),
                row(1),
                row(2),
                row(3)
            );
        }
        let lit = misc.is_some_and(|m| m.bool("lightingEnable", false));
        let mut s = String::from(LIGHTING);
        s.push_str(&inputs(streams));
        s.push_str(
            "    let object = vec4f(v.a0.xyz, 1.0);
    let hpos = vec4f(dot(draw.c[0], object), dot(draw.c[1], object), dot(draw.c[2], object), dot(draw.c[3], object));
    let eye = vec4f(dot(draw.c[4], object), dot(draw.c[5], object), dot(draw.c[6], object), 1.0);
    let normal = normalize(vec3f(dot(draw.c[4].xyz, v.a1.xyz), dot(draw.c[5].xyz, v.a1.xyz), dot(draw.c[6].xyz, v.a1.xyz)));
    let fogc = vec4f(eye.z);
",
        );
        s.push_str(match lit {
            true => "    let lighting = lights(eye.xyz, normal);\n    let col0 = vec4f(lighting.diffuse, draw.c[24].a);\n    let col1 = vec4f(lighting.specular, 0.0);\n",
            false => "    let col0 = vec4f(1.0);\n    let col1 = vec4f(0.0);\n",
        });
        s.push_str(&tex);
        s.push_str(FINISH);
        s
    }
}

const LIGHTING: &str = "
struct Lit { diffuse: vec3f, specular: vec3f }
fn lights(eye: vec3f, n: vec3f) -> Lit {
    var out = Lit(draw.c[24].rgb, vec3f(0.0));
    let shininess = draw.c[25].x;
    for (var i = 0; i < i32(draw.c[25].y); i++) {
        let at = 26 + 6 * i;
        let position = draw.c[at];
        let offset = position.xyz - eye * position.w;
        let distance = length(offset);
        let l = offset / max(distance, 1e-6);
        var strength = 1.0;
        if position.w > 0.0 {
            let k = draw.c[at + 5].xyz;
            strength = 1.0 / max(k.x + distance * (k.y + distance * k.z), 1e-6);
            if dot(-l, draw.c[at + 4].xyz) < draw.c[at + 1].w { strength = 0.0; }
        }
        let facing = max(dot(n, l), 0.0);
        out.diffuse += strength * (draw.c[at + 1].rgb + facing * draw.c[at + 2].rgb);
        // The viewer counts as far away, as by default.
        let h = normalize(l + vec3f(0.0, 0.0, 1.0));
        let glint = select(0.0, pow(max(dot(n, h), 1e-6), max(shininess, 1e-3)), facing > 0.0);
        out.specular += strength * glint * draw.c[at + 3].rgb;
    }
    return out;
}
fn sphere(u: vec3f, n: vec3f) -> vec4f {
    let r = reflect(u, n);
    let m = 2.0 * length(r + vec3f(0.0, 0.0, 1.0));
    return vec4f(r.xy / m + 0.5, 0.0, 1.0);
}
";
