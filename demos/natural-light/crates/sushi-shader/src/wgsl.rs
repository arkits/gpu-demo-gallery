//! Emits one WGSL module per pass from its vertex and pixel programs.
//!
//! Bindings: group 0 holds vertex constants (0) and pixel constants (1) as
//! `array<vec4<f32>>`; group 1 holds, for sampler `s`, texture `2s` and
//! sampler `2s + 1`.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::asm::{self, Bank, Instruction, Operand, Program, Stage as AsmStage};
use crate::{Compare, Pass, Result, Stage};

/// How a vertex element is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementType {
    Float1,
    Float2,
    Float3,
    Float4,
    /// BGRA bytes.
    Color,
    Ubyte4,
}

/// A vertex attribute the shader reads, at WGSL `location` = its position here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Input {
    pub semantic: String,
    pub index: u32,
    pub ty: ElementType,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub source: String,
    pub inputs: Vec<Input>,
    pub vs_consts: u32,
    pub ps_consts: u32,
    /// Sampler stage and whether it is a cube map.
    pub samplers: BTreeMap<u32, bool>,
    /// Number of color targets written to.
    pub outputs: u32,
}

/// Constants a fixed-function pass expects: WVP rows, then the texture factor.
pub const FIXED_WVP: u32 = 0;
pub const FIXED_FACTOR: u32 = 0;

const C: [&str; 4] = ["x", "y", "z", "w"];
/// Stands in for D3D's infinity from `rcp`/`rsq` of zero.
const HELPERS: &str = "\
fn rcp1(x: f32) -> f32 { if (x == 0.0) { return 3.0e38; } return 1.0 / x; }
fn rsq1(x: f32) -> f32 { let a = abs(x); if (a == 0.0) { return 1.0e19; } return inverseSqrt(a); }
fn pow1(x: f32, y: f32) -> f32 { let a = abs(x); if (a == 0.0) { if (y == 0.0) { return 1.0; } return 0.0; } return pow(a, y); }
";

/// Builds the module for `pass`. `element` reports the stored type of a vertex
/// semantic, and `cube` whether a sampler stage holds a cube map.
pub fn translate(
    pass: &Pass,
    element: &dyn Fn(&str, u32) -> Option<ElementType>,
    cube: &dyn Fn(u32) -> bool,
) -> Result<Module> {
    let vs = pass.vs.as_deref().map(asm::parse).transpose()?;
    let ps = pass.ps.as_deref().map(asm::parse).transpose()?;
    let mut m = Module {
        source: HELPERS.into(),
        inputs: Vec::new(),
        vs_consts: 4,
        ps_consts: 1,
        samplers: BTreeMap::new(),
        outputs: 1,
    };
    let biases: BTreeMap<u32, f32> = pass
        .samplers
        .iter()
        .filter(|s| s.1.lod_bias != 0.0)
        .map(|(k, s)| (*k, s.lod_bias))
        .collect();
    match &vs {
        Some(p) => {
            m.vs_consts = consts(p).max(1);
            // Relative addressing reaches the whole 40-matrix palette.
            if p.code.iter().flat_map(|i| &i.src).any(|o| o.relative) {
                m.vs_consts = m.vs_consts.max(30 + 40 * 4);
            }
            for (semantic, index, _) in &p.inputs {
                let ty =
                    element(semantic, *index).ok_or(format!("mesh lacks {semantic}{index}"))?;
                m.inputs.push(Input {
                    semantic: semantic.clone(),
                    index: *index,
                    ty,
                });
            }
        }
        None => {
            for (semantic, index) in [("position", 0), ("color", 0), ("texcoord", 0)] {
                if let Some(ty) = element(semantic, index) {
                    m.inputs.push(Input {
                        semantic: semantic.into(),
                        index,
                        ty,
                    });
                }
            }
        }
    }
    if let Some(p) = &ps {
        m.ps_consts = consts(p).max(1);
        let written = p.code.iter().filter(|i| i.dest.bank == Bank::OutColor);
        m.outputs = written.map(|i| i.dest.index + 1).max().unwrap_or(1);
        for s in sampled(p) {
            m.samplers.insert(s, cube(s) || p.cubes.contains(&s));
        }
    } else if matches!(
        pass.stage,
        Some(Stage::Texture | Stage::TextureTimesDiffuse2 { .. })
    ) {
        m.samplers.insert(0, false);
    }

    // Interpolators: everything either side mentions.
    let mut tex = BTreeSet::new();
    let mut col = BTreeSet::new();
    for i in vs.iter().flat_map(|p| &p.code) {
        match i.dest.bank {
            Bank::OutTexcoord => drop(tex.insert(i.dest.index)),
            Bank::OutColor => drop(col.insert(i.dest.index)),
            _ => {}
        }
    }
    for o in ps.iter().flat_map(|p| &p.code).flat_map(|i| &i.src) {
        match o.bank {
            Bank::Texcoord => drop(tex.insert(o.index)),
            Bank::Input => drop(col.insert(o.index)),
            _ => {}
        }
    }
    if vs.is_none() {
        tex.insert(0);
        col.insert(0);
    }
    if ps.is_none() {
        match pass.stage {
            Some(Stage::Texture) => drop(tex.insert(0)),
            Some(Stage::TextureTimesDiffuse2 { .. }) => {
                tex.insert(0);
                col.insert(0);
            }
            Some(Stage::Factor) => {}
            Some(Stage::DiffuseTimesFactor | Stage::Diffuse) | None => drop(col.insert(0)),
        }
    }

    let s = &mut m.source;
    let _ = writeln!(
        s,
        "struct VsConst {{ c: array<vec4<f32>, {}> }};",
        m.vs_consts
    );
    let _ = writeln!(
        s,
        "struct PsConst {{ c: array<vec4<f32>, {}> }};",
        m.ps_consts
    );
    s.push_str("@group(0) @binding(0) var<uniform> vc: VsConst;\n");
    s.push_str("@group(0) @binding(1) var<uniform> pc: PsConst;\n");
    for (stage, cube) in &m.samplers {
        let ty = if *cube {
            "texture_cube<f32>"
        } else {
            "texture_2d<f32>"
        };
        let _ = writeln!(s, "@group(1) @binding({}) var t{stage}: {ty};", stage * 2);
        let _ = writeln!(
            s,
            "@group(1) @binding({}) var s{stage}: sampler;",
            stage * 2 + 1
        );
    }
    s.push_str("struct In {\n");
    for (loc, i) in m.inputs.iter().enumerate() {
        let ty = match i.ty {
            ElementType::Float1 => "f32",
            ElementType::Float2 => "vec2<f32>",
            ElementType::Float3 => "vec3<f32>",
            ElementType::Float4 | ElementType::Color => "vec4<f32>",
            ElementType::Ubyte4 => "vec4<u32>",
        };
        let _ = writeln!(s, "  @location({loc}) a{loc}: {ty},");
    }
    s.push_str("};\nstruct V2F {\n  @builtin(position) pos: vec4<f32>,\n");
    for t in &tex {
        let _ = writeln!(s, "  @location({t}) t{t}: vec4<f32>,");
    }
    for c in &col {
        // On the original hardware colors never extrapolate past the triangle,
        // which shaders such as oMarimbaGlow.ssh rely on.
        let _ = writeln!(
            s,
            "  @location({}) @interpolate(perspective, centroid) d{c}: vec4<f32>,",
            8 + c
        );
    }
    s.push_str("};\n@vertex\nfn vs(i: In) -> V2F {\n  var o: V2F;\n");
    let wide: Vec<String> = m
        .inputs
        .iter()
        .enumerate()
        .map(|(l, i)| match i.ty {
            ElementType::Float1 => format!("vec4<f32>(i.a{l}, 0.0, 0.0, 1.0)"),
            ElementType::Float2 => format!("vec4<f32>(i.a{l}, 0.0, 1.0)"),
            ElementType::Float3 => format!("vec4<f32>(i.a{l}, 1.0)"),
            ElementType::Float4 => format!("i.a{l}"),
            ElementType::Color => format!("i.a{l}.bgra"),
            ElementType::Ubyte4 => format!("vec4<f32>(i.a{l})"),
        })
        .collect();
    match &vs {
        Some(p) => {
            for ((_, _, reg), w) in p.inputs.iter().zip(&wide) {
                let _ = writeln!(s, "  let v{reg} = {w};");
            }
            s.push_str("  var oPos = vec4<f32>();\n  var a0 = 0;\n");
            for t in &tex {
                let _ = writeln!(s, "  var oT{t} = vec4<f32>(0.0, 0.0, 0.0, 1.0);");
            }
            for c in &col {
                let _ = writeln!(s, "  var oD{c} = vec4<f32>(1.0);");
            }
            body(s, p, &m.samplers, &biases)?;
            s.push_str("  o.pos = oPos;\n");
            for t in &tex {
                let _ = writeln!(s, "  o.t{t} = oT{t};");
            }
            for c in &col {
                // Color interpolators are clamped.
                let _ = writeln!(s, "  o.d{c} = clamp(oD{c}, vec4<f32>(), vec4<f32>(1.0));");
            }
        }
        None => {
            let find = |sem: &str| m.inputs.iter().position(|i| i.semantic == sem);
            let get = |sem: &str, d: &str| find(sem).map_or(d.to_string(), |l| wide[l].clone());
            let _ = writeln!(
                s,
                "  let p = {};",
                get("position", "vec4<f32>(0.0, 0.0, 0.0, 1.0)")
            );
            s.push_str(
                "  o.pos = vec4<f32>(dot(p, vc.c[0]), dot(p, vc.c[1]), dot(p, vc.c[2]), dot(p, vc.c[3]));\n",
            );
            let _ = writeln!(
                s,
                "  o.t0 = {};",
                get("texcoord", "vec4<f32>(0.0, 0.0, 0.0, 1.0)")
            );
            let _ = writeln!(s, "  o.d0 = {};", get("color", "vec4<f32>(1.0)"));
        }
    }
    s.push_str("  return o;\n}\nstruct Shown {\n");
    for n in 0..m.outputs {
        let _ = writeln!(s, "  @location({n}) c{n}: vec4<f32>,");
    }
    s.push_str("};\nfn shade(i: V2F) -> Shown {\n");
    match &ps {
        Some(p) => {
            for t in &tex {
                let _ = writeln!(s, "  let t{t}_ = i.t{t};");
            }
            for c in &col {
                let _ = writeln!(s, "  let v{c} = i.d{c};");
            }
            for n in 0..m.outputs {
                let _ = writeln!(s, "  var oC{n} = vec4<f32>();");
            }
            body(s, p, &m.samplers, &biases)?;
            if p.stage == AsmStage::Pixel14 {
                s.push_str("  oC0 = r0;\n");
            }
            let all: Vec<String> = (0..m.outputs).map(|n| format!("oC{n}")).collect();
            let _ = writeln!(s, "  return Shown({});", all.join(", "));
        }
        None => s.push_str(match pass.stage {
            Some(Stage::Texture) => "  return Shown(textureSample(t0, s0, i.t0.xy));\n",
            Some(Stage::TextureTimesDiffuse2 { texture_alpha: true }) => {
                "  let t = textureSample(t0, s0, i.t0.xy);\n  return Shown(vec4<f32>(2.0 * t.rgb * i.d0.rgb, t.a));\n"
            }
            Some(Stage::TextureTimesDiffuse2 { texture_alpha: false }) => {
                "  let t = textureSample(t0, s0, i.t0.xy);\n  return Shown(vec4<f32>(2.0 * t.rgb * i.d0.rgb, i.d0.a));\n"
            }
            Some(Stage::Factor) => "  return Shown(pc.c[0]);\n",
            Some(Stage::DiffuseTimesFactor) => {
                "  return Shown(vec4<f32>(i.d0.rgb * pc.c[0].rgb, i.d0.a));\n"
            }
            Some(Stage::Diffuse) | None => "  return Shown(i.d0);\n",
        }),
    }
    // Buffers of 16 bits per channel hold 0 to 1 as well, but are floats here.
    s.push_str("}\n@fragment\nfn fs(i: V2F) -> Shown {\n  var c = shade(i);\n");
    for n in 0..m.outputs {
        let _ = writeln!(s, "  c.c{n} = clamp(c.c{n}, vec4<f32>(), vec4<f32>(1.0));");
    }
    if let Some((test, value)) = pass.alpha_test {
        let op = match test {
            Compare::Less => "<",
            Compare::LessEqual => "<=",
            Compare::Greater => ">",
            Compare::GreaterEqual => ">=",
            Compare::Equal => "==",
            Compare::NotEqual => "!=",
            t @ (Compare::Never | Compare::Always) => {
                return Err(format!("alpha test {t:?}"));
            }
        };
        let _ = writeln!(s, "  if (!(c.c0.a {op} {value:?})) {{ discard; }}");
    }
    s.push_str("  return c;\n}\n");
    Ok(m)
}

/// Number of constant registers the program touches.
fn consts(p: &Program) -> u32 {
    let mut n = 0;
    for i in &p.code {
        for (k, o) in i.src.iter().enumerate() {
            if o.bank == Bank::Const && !o.relative {
                let rows = match i.op.as_str() {
                    "m4x4" if k == 1 => 4,
                    "m3x3" if k == 1 => 3,
                    _ => 1,
                };
                n = n.max(o.index + rows);
            }
        }
    }
    n
}

fn sampled(p: &Program) -> BTreeSet<u32> {
    let texld = p.code.iter().filter(|i| i.op.starts_with("texld"));
    texld
        .map(|i| match p.stage {
            AsmStage::Pixel14 => i.dest.index,
            _ => i.src[1].index,
        })
        .collect()
}

fn register(p: &Program, o: &Operand, row: u32) -> String {
    let n = o.index + row;
    match o.bank {
        Bank::Temp => format!("r{n}"),
        Bank::Input => format!("v{n}"),
        Bank::Texcoord => format!("t{n}_"),
        Bank::Const if o.relative => format!("vc.c[a0 + {n}]"),
        Bank::Const => match p.defs.iter().find(|d| d.0 == n) {
            Some((_, v)) => format!("vec4<f32>({:?}, {:?}, {:?}, {:?})", v[0], v[1], v[2], v[3]),
            None if p.stage == AsmStage::Vertex => format!("vc.c[{n}]"),
            None => format!("pc.c[{n}]"),
        },
        _ => "vec4<f32>()".into(),
    }
}

/// Source operand as a vec4 expression.
fn source(p: &Program, o: &Operand, row: u32) -> String {
    let mut e = register(p, o, row);
    if !o.comps.is_empty() {
        let swz: String = (0..4)
            .map(|i| C[o.comps[i.min(o.comps.len() - 1)] as usize])
            .collect();
        e = format!("{e}.{swz}");
    }
    if o.complement {
        e = format!("(vec4<f32>(1.0) - {e})");
    }
    if o.negate { format!("(-{e})") } else { e }
}

fn body(
    s: &mut String,
    p: &Program,
    samplers: &BTreeMap<u32, bool>,
    biases: &BTreeMap<u32, f32>,
) -> Result<()> {
    for r in 0..12 {
        let _ = writeln!(s, "  var r{r} = vec4<f32>();");
    }
    for i in &p.code {
        instruction(s, p, i, samplers, biases).map_err(|e| format!("{}: {e}", i.op))?;
    }
    Ok(())
}

fn instruction(
    s: &mut String,
    p: &Program,
    i: &Instruction,
    samplers: &BTreeMap<u32, bool>,
    biases: &BTreeMap<u32, f32>,
) -> Result<()> {
    let arity = |n: usize| {
        if i.src.len() == n {
            Ok(())
        } else {
            Err(format!("expected {n} sources"))
        }
    };
    let a = |k: usize| source(p, &i.src[k], 0);
    let dot3 = |x: &str, y: &str| format!("vec4<f32>(dot({x}.xyz, {y}.xyz))");
    let scalar = |k: usize| format!("{}.x", a(k));
    let mut e = match i.op.as_str() {
        "mov" => {
            arity(1)?;
            a(0)
        }
        "add" | "sub" | "mul" => {
            arity(2)?;
            let op = match i.op.as_str() {
                "add" => "+",
                "sub" => "-",
                _ => "*",
            };
            format!("({} {op} {})", a(0), a(1))
        }
        "mad" => {
            arity(3)?;
            format!("({} * {} + {})", a(0), a(1), a(2))
        }
        "lrp" => {
            arity(3)?;
            format!("mix({}, {}, {})", a(2), a(1), a(0))
        }
        "max" | "min" => {
            arity(2)?;
            format!("{}({}, {})", i.op, a(0), a(1))
        }
        "slt" => {
            arity(2)?;
            format!("select(vec4<f32>(), vec4<f32>(1.0), {} < {})", a(0), a(1))
        }
        "cmp" => {
            arity(3)?;
            let zero = "vec4<f32>()";
            format!("select({}, {}, {} >= {zero})", a(2), a(1), a(0))
        }
        "dp3" => {
            arity(2)?;
            dot3(&a(0), &a(1))
        }
        "dp2add" => {
            arity(3)?;
            format!("vec4<f32>(dot({}.xy, {}.xy) + {})", a(0), a(1), scalar(2))
        }
        "abs" | "frc" => {
            arity(1)?;
            let f = if i.op == "abs" { "abs" } else { "fract" };
            format!("{f}({})", a(0))
        }
        "dp4" => {
            arity(2)?;
            format!("vec4<f32>(dot({}, {}))", a(0), a(1))
        }
        "rcp" | "rsq" => {
            arity(1)?;
            format!("vec4<f32>({}1({}))", i.op, scalar(0))
        }
        "pow" => {
            arity(2)?;
            format!("vec4<f32>(pow1({}, {}))", scalar(0), scalar(1))
        }
        "nrm" => {
            arity(1)?;
            format!("({0} * rsq1(dot({0}.xyz, {0}.xyz)))", a(0))
        }
        "m4x4" | "m3x3" => {
            arity(2)?;
            let rows = if i.op == "m4x4" { 4 } else { 3 };
            let v = a(0);
            let mut parts: Vec<String> = (0..rows)
                .map(|r| {
                    let m = source(p, &i.src[1], r);
                    if rows == 4 {
                        format!("dot({v}, {m})")
                    } else {
                        format!("dot({v}.xyz, {m}.xyz)")
                    }
                })
                .collect();
            parts.resize(4, "0.0".into());
            format!("vec4<f32>({})", parts.join(", "))
        }
        "texcrd" => {
            arity(1)?;
            let reg = register(p, &i.src[0], 0);
            if i.src[0].divide_w {
                format!("({reg} * vec4<f32>(rcp1({reg}.w)))")
            } else {
                a(0)
            }
        }
        "texld" | "texldb" | "texldp" => {
            let stage = match p.stage {
                AsmStage::Pixel14 => i.dest.index,
                _ => i.src.get(1).ok_or("missing sampler")?.index,
            };
            let cube = samplers.get(&stage).copied().unwrap_or(false);
            let coord = if cube { "xyz" } else { "xy" };
            let c = a(0);
            if i.op == "texldp" {
                format!("textureSample(t{stage}, s{stage}, {c}.{coord} * rcp1({c}.w))")
            } else if i.op == "texldb" {
                format!("textureSampleBias(t{stage}, s{stage}, {c}.{coord}, {c}.w)")
            } else if let Some(b) = biases.get(&stage) {
                format!("textureSampleBias(t{stage}, s{stage}, {c}.{coord}, {b:?})")
            } else {
                format!("textureSample(t{stage}, s{stage}, {c}.{coord})")
            }
        }
        op => return Err(format!("unsupported instruction {op}")),
    };
    if i.scale != 1.0 {
        e = format!("({e} * {:?})", i.scale);
    }
    if i.saturate {
        e = format!("clamp({e}, vec4<f32>(), vec4<f32>(1.0))");
    }
    let d = &i.dest;
    let name = match d.bank {
        Bank::Temp => format!("r{}", d.index),
        Bank::OutPosition => "oPos".into(),
        Bank::OutTexcoord => format!("oT{}", d.index),
        Bank::OutColor if p.stage == AsmStage::Vertex => format!("oD{}", d.index),
        Bank::OutColor => format!("oC{}", d.index),
        Bank::Address => {
            // vs.1.1 address moves round down.
            let _ = writeln!(s, "  a0 = i32(floor({e}.x));");
            return Ok(());
        }
        _ => return Err("bad destination".into()),
    };
    let mask: Vec<u8> = match (d.comps.is_empty(), i.op.as_str()) {
        (true, "m3x3") => vec![0, 1, 2],
        (true, _) => vec![0, 1, 2, 3],
        (false, _) => d.comps.clone(),
    };
    if mask.len() == 4 {
        let _ = writeln!(s, "  {name} = {e};");
    } else {
        let _ = writeln!(s, "  {{ let t = {e};");
        for c in mask {
            let c = C[c as usize];
            let _ = writeln!(s, "    {name}.{c} = t.{c};");
        }
        s.push_str("  }\n");
    }
    Ok(())
}
