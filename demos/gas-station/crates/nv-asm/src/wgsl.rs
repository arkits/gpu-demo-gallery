//! Translates a parsed program to a WGSL shader module.
//!
//! Each instruction becomes a block that reads its sources (swizzle, negate, absolute value),
//! converts them to the instruction's precision, computes, saturates, updates the condition
//! code and writes the destination through its write mask and condition. Registers hold
//! `vec4f`; fp16 (H) values are rounded through `pack2x16float`, fixed-point (X) values to
//! steps of 1/1024 between -2 and 2047/1024. Temporaries and the condition code start at 0.
//!
//! The framebuffer is kept upside down as OpenGL has it (row 0 at the bottom): vertex
//! programs turn clip y over and map clip z from -w..w to 0..w, so `f[WPOS]` is OpenGL's
//! window position and rendered textures read with OpenGL's coordinates. Winding turns
//! over with y; the renderer flips its front face to match.
//!
//! Interface, in both stages:
//! - Vertex: `v[n]` at `@location(n)`; `c[]` at group 0 binding 0.
//! - Varyings: `TEX0`–`TEX7` at locations 0–7, `COL0` 8, `COL1` 9, `FOGC` 10. Vertex
//!   programs write all of them, (0, 0, 0, 1) where they leave one unwritten.
//! - Fragment: named parameters (`DECLARE`) at group 1 binding 0, in declaration order;
//!   texture unit u, in increasing order of the units used, at bindings 1 + 2k (texture) and
//!   2 + 2k (sampler).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::parse::{Cond, Dst, Inst, Precision, Program, Reg, Src, Stage, Target};

pub const VARYINGS: &[(&str, u32)] = &[
    ("TEX0", 0),
    ("TEX1", 1),
    ("TEX2", 2),
    ("TEX3", 3),
    ("TEX4", 4),
    ("TEX5", 5),
    ("TEX6", 6),
    ("TEX7", 7),
    ("COL0", 8),
    ("COL1", 9),
    ("FOGC", 10),
];

#[derive(Debug, Clone, PartialEq)]
pub struct TextureUse {
    pub unit: u32,
    pub target: Target,
    /// Sampled with depth comparison (a shadow map).
    pub shadow: bool,
}

/// What a translated program needs from the renderer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Interface {
    /// Vertex attributes read.
    pub attributes: Vec<u32>,
    /// Size of the `c[]` array (vertex) or of the parameter array (fragment).
    pub uniforms: u32,
    /// Fragment parameter names in slot order.
    pub params: Vec<String>,
    pub textures: Vec<TextureUse>,
    /// Varyings the fragment program reads.
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranslateError(pub String);

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TranslateError {}

fn fail<T>(inst: &Inst, msg: impl std::fmt::Display) -> Result<T, TranslateError> {
    Err(TranslateError(format!("line {}: {} {}", inst.line, inst.op, msg)))
}

const HELPERS: &str = r#"
fn q_h(v: vec4f) -> vec4f {
    let c = clamp(v, vec4f(-65504.0), vec4f(65504.0));
    return vec4f(unpack2x16float(pack2x16float(c.xy)), unpack2x16float(pack2x16float(c.zw)));
}
fn q_x(v: vec4f) -> vec4f {
    return clamp(round(v * 1024.0) / 1024.0, vec4f(-2.0), vec4f(2047.0 / 1024.0));
}
fn sat(v: vec4f) -> vec4f {
    return clamp(v, vec4f(0.0), vec4f(1.0));
}
fn nv_pow(a: f32, b: f32) -> f32 {
    if (b == 0.0) { return 1.0; }
    if (a == 0.0) { return select(0.0, 1.0 / a, b < 0.0); }
    return exp2(b * log2(a));
}
fn nv_lit(s: vec4f) -> vec4f {
    let x = max(s.x, 0.0);
    let y = max(s.y, 0.0);
    let w = clamp(s.w, -127.9961, 127.9961);
    return vec4f(1.0, x, select(0.0, nv_pow(y, w), s.x > 0.0), 1.0);
}
"#;

struct Gen<'a> {
    p: &'a Program,
    opts: &'a Options,
    out: String,
    temps: BTreeSet<(char, u32)>,
    outputs: BTreeSet<String>,
    inputs: BTreeSet<String>,
    attrs: BTreeSet<u32>,
    max_const: u32,
    relative: bool,
    textures: BTreeMap<u32, Target>,
    uses_cc: bool,
    uses_a0: bool,
}

/// Choices the program text does not settle.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Texture units holding depth textures sampled with comparison.
    pub shadow_units: Vec<u32>,
    /// Shadow units that clamp to a white border (wrap 2): outside the map reads as lit.
    pub border_units: Vec<u32>,
    /// Texture units in NV_texture_expand_normal mode (`remapped`): unsigned texels read
    /// as 2t - 1.
    pub expand_units: Vec<u32>,
    /// OpenGL's alpha test, which WebGPU lacks: compare function (0 NEVER, 1 LESS,
    /// 2 LEQUAL, 3 GREATER, 4 GEQUAL, 5 EQUAL, 6 NOTEQUAL, 7 ALWAYS), reference 0–255, and
    /// whether the target clamps colors (fixed-point targets do, float ones do not).
    pub alpha_test: Option<(u32, u32, bool)>,
}

fn comp(i: u8) -> char {
    b"xyzw"[i as usize] as char
}

fn swz(s: [u8; 4]) -> String {
    s.iter().map(|&c| comp(c)).collect()
}

fn lit(v: [f32; 4]) -> String {
    format!("vec4f({:?}, {:?}, {:?}, {:?})", v[0], v[1], v[2], v[3])
}

fn cond_test(c: Cond, v: &str) -> String {
    match c {
        Cond::Eq => format!("({v} == vec4f(0.0))"),
        Cond::Ne => format!("({v} != vec4f(0.0))"),
        Cond::Lt => format!("({v} < vec4f(0.0))"),
        Cond::Gt => format!("({v} > vec4f(0.0))"),
        Cond::Le => format!("({v} <= vec4f(0.0))"),
        Cond::Ge => format!("({v} >= vec4f(0.0))"),
        Cond::Tr => "vec4<bool>(true)".into(),
        Cond::Fl => "vec4<bool>(false)".into(),
    }
}

impl Gen<'_> {
    fn reg_value(&mut self, inst: &Inst, r: &Reg) -> Result<String, TranslateError> {
        Ok(match r {
            Reg::Temp(p, n) => {
                let c = if *p == Precision::H { 'H' } else { 'R' };
                self.temps.insert((c, *n));
                format!("{c}{n}")
            }
            Reg::Attr(n) => {
                self.attrs.insert(*n);
                format!("vin.v{n}")
            }
            Reg::Const(n) => {
                self.max_const = self.max_const.max(*n + 1);
                format!("c[{n}]")
            }
            Reg::ConstRel(n, a, off) => {
                self.relative = true;
                self.uses_a0 = true;
                format!("crel(a{n}.{} + {off})", comp(*a))
            }
            Reg::Addr(_) => return fail(inst, "reads an address register as a value"),
            Reg::Out(o) => {
                // Programs may read back what they wrote (vertex program 2 can).
                self.outputs.insert(o.clone());
                format!("o_{o}")
            }
            Reg::In(i) => {
                if i == "WPOS" {
                    "fin.wpos".into()
                } else {
                    if !VARYINGS.iter().any(|v| v.0 == i) {
                        return fail(inst, format!("reads unknown input f[{i}]"));
                    }
                    self.inputs.insert(i.clone());
                    format!("fin.{i}")
                }
            }
            Reg::Named(n) => {
                if let Some(v) = self.p.defines.get(n) {
                    lit(*v)
                } else if let Some(i) = self.p.declares.iter().position(|d| &d.0 == n) {
                    format!("p[{i}]")
                } else {
                    return fail(inst, format!("unknown name {n}"));
                }
            }
            Reg::Literal(v) => lit(*v),
        })
    }

    fn src(&mut self, inst: &Inst, s: &Src, prec: Precision) -> Result<String, TranslateError> {
        let mut e = self.reg_value(inst, &s.reg)?;
        if s.swizzle != [0, 1, 2, 3] {
            e = format!("({e}).{}", swz(s.swizzle));
        }
        if s.abs {
            e = format!("abs({e})");
        }
        if s.neg {
            e = format!("(-{e})");
        }
        // Operands are taken at the instruction's precision.
        Ok(match prec {
            Precision::R => e,
            Precision::H => format!("q_h({e})"),
            Precision::X => format!("q_x({e})"),
        })
    }

    fn texture(&mut self, inst: &Inst, srcs: &[String]) -> Result<String, TranslateError> {
        let Some((unit, target)) = inst.texture else { return fail(inst, "without texture") };
        if let Some(t) = self.textures.insert(unit, target) {
            if t != target {
                return fail(inst, format!("uses texture unit {unit} as two targets"));
            }
        }
        let shadow = self.opts.shadow_units.contains(&unit);
        let (t, s) = (format!("t{unit}"), format!("s{unit}"));
        let a = &srcs[0];
        let coord = match inst.op.as_str() {
            "TXP" => match target {
                Target::Cube => format!("({a}).xyz"),
                Target::T3D => format!("(({a}).xyz / ({a}).w)"),
                _ => format!("(({a}).xy / ({a}).w)"),
            },
            _ => match target {
                Target::Cube | Target::T3D => format!("({a}).xyz"),
                Target::T1D => format!("vec2f(({a}).x, 0.5)"),
                _ => format!("({a}).xy"),
            },
        };
        let coord = if target == Target::Rect {
            format!("({coord} / vec2f(textureDimensions({t})))")
        } else {
            coord
        };
        if shadow {
            if target != Target::T2D && target != Target::Rect {
                return fail(inst, "compares depth in a texture that is not 2D");
            }
            // r (or r/q) against the stored depth; luminance mode gives (d, d, d, 1).
            let depth = match inst.op.as_str() {
                "TXP" => format!("(({a}).z / ({a}).w)"),
                "TEX" => format!("({a}).z"),
                _ => return fail(inst, "with derivatives on a shadow map"),
            };
            let cmp = format!("textureSampleCompareLevel({t}, {s}, {coord}, {depth})");
            if self.opts.border_units.contains(&unit) {
                // `_TextureShadow` clamps to a white border: outside the map is lit.
                return Ok(format!(
                    "vec4f(vec3f(select({cmp}, 1.0, any({coord} < vec2f(0.0)) || any({coord} > vec2f(1.0)))), 1.0)"
                ));
            }
            return Ok(format!("vec4f(vec3f({cmp}), 1.0)"));
        }
        let expand = self.opts.expand_units.contains(&unit);
        let fetch = match inst.op.as_str() {
            "TXD" => {
                let (dx, dy) = (&srcs[1], &srcs[2]);
                let (dx, dy) = match target {
                    Target::Cube | Target::T3D => (format!("({dx}).xyz"), format!("({dy}).xyz")),
                    Target::Rect => (
                        format!("(({dx}).xy / vec2f(textureDimensions({t})))"),
                        format!("(({dy}).xy / vec2f(textureDimensions({t})))"),
                    ),
                    Target::T1D => (format!("vec2f(({dx}).x, 0.0)"), format!("vec2f(({dy}).x, 0.0)")),
                    Target::T2D => (format!("({dx}).xy"), format!("({dy}).xy")),
                };
                format!("textureSampleGrad({t}, {s}, {coord}, {dx}, {dy})")
            }
            _ => format!("textureSample({t}, {s}, {coord})"),
        };
        Ok(if expand { format!("({fetch} * 2.0 - 1.0)") } else { fetch })
    }

    fn dst_store(&mut self, inst: &Inst, d: &Dst) -> Result<(String, Option<Precision>), TranslateError> {
        Ok(match &d.reg {
            Reg::Temp(p, n) => {
                let c = if *p == Precision::H { 'H' } else { 'R' };
                self.temps.insert((c, *n));
                (format!("{c}{n}"), Some(*p))
            }
            Reg::Out(o) => {
                let o = match (self.p.stage, o.as_str()) {
                    // One color output; COLH holds half floats.
                    (Stage::Fragment, "COLR" | "COLH") => "COL".to_string(),
                    _ => o.clone(),
                };
                self.outputs.insert(o.clone());
                let p = (o == "COL" && matches!(&d.reg, Reg::Out(x) if x == "COLH")).then_some(Precision::H);
                (format!("o_{o}"), p)
            }
            Reg::Addr(n) => {
                self.uses_a0 = true;
                (format!("a{n}"), None)
            }
            _ => return fail(inst, "writes a register that cannot be written"),
        })
    }

    fn inst(&mut self, inst: &Inst) -> Result<(), TranslateError> {
        let prec = inst.precision.unwrap_or(Precision::R);
        let mut srcs = vec![];
        for s in &inst.srcs {
            srcs.push(self.src(inst, s, prec)?);
        }
        let s = |i: usize| srcs[i].clone();
        let scalar = |e: String| format!("vec4f({e})");
        let r = match inst.op.as_str() {
            "MOV" => s(0),
            "ADD" => format!("{} + {}", s(0), s(1)),
            "MUL" => format!("{} * {}", s(0), s(1)),
            "MAD" => format!("{} * {} + {}", s(0), s(1), s(2)),
            "DP3" => scalar(format!("dot(({}).xyz, ({}).xyz)", s(0), s(1))),
            "DP4" => scalar(format!("dot({}, {})", s(0), s(1))),
            "DPH" => scalar(format!("dot(({}).xyz, ({}).xyz) + ({}).w", s(0), s(1), s(1))),
            "DST" => format!("vec4f(1.0, ({0}).y * ({1}).y, ({0}).z, ({1}).w)", s(0), s(1)),
            "MIN" => format!("min({}, {})", s(0), s(1)),
            "MAX" => format!("max({}, {})", s(0), s(1)),
            "SLT" | "SGE" | "SGT" | "SLE" | "SEQ" | "SNE" => {
                let op = match inst.op.as_str() {
                    "SLT" => "<",
                    "SGE" => ">=",
                    "SGT" => ">",
                    "SLE" => "<=",
                    "SEQ" => "==",
                    _ => "!=",
                };
                format!("select(vec4f(0.0), vec4f(1.0), {} {op} {})", s(0), s(1))
            }
            "STR" => "vec4f(1.0)".into(),
            "SFL" => "vec4f(0.0)".into(),
            "RCP" => scalar(format!("1.0 / ({}).x", s(0))),
            "RCC" => scalar(format!(
                "sign(1.0 / ({0}).x) * clamp(abs(1.0 / ({0}).x), 1.884467e-19, 5.42101e19)",
                s(0)
            )),
            "RSQ" => scalar(format!("inverseSqrt(abs(({}).x))", s(0))),
            "EX2" => scalar(format!("exp2(({}).x)", s(0))),
            "LG2" => scalar(format!("log2(({}).x)", s(0))),
            "EXP" => format!(
                "vec4f(exp2(floor(({0}).x)), ({0}).x - floor(({0}).x), exp2(({0}).x), 1.0)",
                s(0)
            ),
            "LOG" => format!(
                "vec4f(floor(log2(abs(({0}).x))), abs(({0}).x) / exp2(floor(log2(abs(({0}).x)))), log2(abs(({0}).x)), 1.0)",
                s(0)
            ),
            "POW" => scalar(format!("nv_pow(({}).x, ({}).x)", s(0), s(1))),
            "LIT" => format!("nv_lit({})", s(0)),
            "FLR" => format!("floor({})", s(0)),
            "FRC" => format!("fract({})", s(0)),
            "LRP" => format!("{0} * {1} + (vec4f(1.0) - {0}) * {2}", s(0), s(1), s(2)),
            "SIN" => scalar(format!("sin(({}).x)", s(0))),
            "COS" => scalar(format!("cos(({}).x)", s(0))),
            "SSG" => format!("sign({})", s(0)),
            "ABS" => format!("abs({})", s(0)),
            "DDX" => format!("dpdx({})", s(0)),
            "DDY" => format!("dpdy({})", s(0)),
            "X2D" => format!(
                "vec4f(({0}).x + ({1}).x * ({2}).x + ({1}).y * ({2}).y, ({0}).y + ({1}).x * ({2}).z + ({1}).y * ({2}).w, ({0}).x + ({1}).x * ({2}).x + ({1}).y * ({2}).y, ({0}).y + ({1}).x * ({2}).z + ({1}).y * ({2}).w)",
                s(0), s(1), s(2)
            ),
            "TEX" | "TXP" | "TXD" => self.texture(inst, &srcs)?,
            "ARL" => format!("floor({})", s(0)),
            "ARR" => format!("round({})", s(0)),
            _ => return fail(inst, "is not supported"),
        };
        let Some(d) = &inst.dst else { return fail(inst, "without destination") };
        let (target, store_prec) = self.dst_store(inst, d)?;
        let o = &mut self.out;
        writeln!(o, "    {{ // line {}", inst.line).unwrap();
        writeln!(o, "        var r = {r};").unwrap();
        if inst.op != "ARL" && inst.op != "ARR" {
            match prec {
                Precision::R => {}
                Precision::H => writeln!(o, "        r = q_h(r);").unwrap(),
                Precision::X => writeln!(o, "        r = q_x(r);").unwrap(),
            }
        }
        if inst.sat {
            writeln!(o, "        r = sat(r);").unwrap();
        }
        let mask = format!(
            "vec4<bool>({}, {}, {}, {})",
            d.mask[0], d.mask[1], d.mask[2], d.mask[3]
        );
        if inst.set_cc {
            self.uses_cc = true;
            writeln!(o, "        cc = select(cc, r, {mask});").unwrap();
        }
        let write_mask = match d.cond {
            Some((c, sw)) => {
                self.uses_cc = true;
                format!("({mask} & {})", cond_test(c, &format!("cc.{}", swz(sw))))
            }
            None => mask,
        };
        let o = &mut self.out;
        if target == "a0" || target == "a1" {
            writeln!(o, "        {target} = select({target}, vec4<i32>(r), {write_mask});").unwrap();
        } else {
            let value = match store_prec {
                Some(Precision::H) => "q_h(r)",
                _ => "r",
            };
            writeln!(o, "        {target} = select({target}, {value}, {write_mask});").unwrap();
        }
        writeln!(o, "    }}").unwrap();
        Ok(())
    }

    fn body(&mut self) -> Result<String, TranslateError> {
        let p = self.p;
        // Forward branches only (as Cg writes if/else). Block k runs when pc == k; a taken
        // branch sets pc to its block and skips the rest of the one it is in.
        let mut starts: Vec<usize> = p.labels.values().copied().filter(|&s| s < p.insts.len()).collect();
        starts.push(0);
        starts.sort();
        starts.dedup();
        let branched = p.insts.iter().any(|i| i.op == "BRA");
        let block_of = |i: usize| starts.iter().rposition(|&s| s <= i).unwrap();
        let mut open = 0;
        for (i, inst) in p.insts.iter().enumerate() {
            if branched && starts.contains(&i) {
                for _ in 0..open {
                    writeln!(self.out, "    }}").unwrap();
                }
                let b = block_of(i);
                writeln!(self.out, "    if (pc == {b}) {{").unwrap();
                writeln!(self.out, "    pc = {}; skip = false;", b + 1).unwrap();
                open = 1;
            }
            match inst.op.as_str() {
                "BRA" => {
                    let (label, c, sw) = inst.branch.as_ref().unwrap();
                    let Some(&to) = p.labels.get(label) else {
                        return fail(inst, format!("to unknown label {label}"));
                    };
                    if to <= i {
                        return fail(inst, "backwards");
                    }
                    let tb = if to >= p.insts.len() { starts.len() } else { block_of(to) };
                    self.uses_cc |= *c != Cond::Tr;
                    let test = cond_test(*c, &format!("cc.{}", swz(*sw)));
                    writeln!(self.out, "    if (any({test})) {{ pc = {tb}; skip = true; }}").unwrap();
                    writeln!(self.out, "    if (!skip) {{").unwrap();
                    open += 1;
                }
                "KIL" => {
                    let (_, c, sw) = inst.branch.as_ref().unwrap();
                    self.uses_cc = true;
                    let test = cond_test(*c, &format!("cc.{}", swz(*sw)));
                    writeln!(self.out, "    if (any({test})) {{ discard; }}").unwrap();
                }
                _ => self.inst(inst)?,
            }
        }
        for _ in 0..open {
            writeln!(self.out, "    }}").unwrap();
        }
        Ok(std::mem::take(&mut self.out))
    }
}

/// A vertex program to a module with entry point `main`.
pub fn vertex(p: &Program, opts: &Options) -> Result<(String, Interface), TranslateError> {
    assert_eq!(p.stage, Stage::Vertex);
    let mut g = Gen::new(p, opts);
    let body = g.body()?;
    let n = if g.relative { 256 } else { g.max_const.max(1) };
    let mut m = String::new();
    m.push_str(HELPERS);
    writeln!(m, "@group(0) @binding(0) var<uniform> c: array<vec4f, {n}>;").unwrap();
    if g.relative {
        writeln!(m, "fn crel(i: i32) -> vec4f {{ if (i < 0 || i > 255) {{ return vec4f(0.0); }} return c[i]; }}").unwrap();
    }
    m.push_str("struct VIn {\n");
    for a in &g.attrs {
        writeln!(m, "    @location({a}) v{a}: vec4f,").unwrap();
    }
    if g.attrs.is_empty() {
        m.push_str("    @builtin(vertex_index) unused: u32,\n");
    }
    m.push_str("}\nstruct VOut {\n    @builtin(position) pos: vec4f,\n");
    for (v, l) in VARYINGS {
        writeln!(m, "    @location({l}) {v}: vec4f,").unwrap();
    }
    m.push_str("}\n@vertex\nfn main(vin: VIn) -> VOut {\n");
    decls(&mut m, &g);
    let mut outs: BTreeSet<String> = g.outputs.clone();
    outs.insert("HPOS".into());
    for o in &outs {
        if o != "HPOS" && !VARYINGS.iter().any(|v| v.0 == o) {
            return Err(TranslateError(format!("writes unsupported output o[{o}]")));
        }
    }
    for (v, _) in VARYINGS {
        writeln!(m, "    var o_{v} = vec4f(0.0, 0.0, 0.0, 1.0);").unwrap();
    }
    m.push_str("    var o_HPOS = vec4f(0.0, 0.0, 0.0, 1.0);\n");
    m.push_str(&body);
    m.push_str("    var out: VOut;\n");
    m.push_str("    out.pos = vec4f(o_HPOS.x, -o_HPOS.y, (o_HPOS.z + o_HPOS.w) * 0.5, o_HPOS.w);\n");
    for (v, _) in VARYINGS {
        // Colors are clamped before they are interpolated.
        if v.starts_with("COL") {
            writeln!(m, "    out.{v} = sat(o_{v});").unwrap();
        } else {
            writeln!(m, "    out.{v} = o_{v};").unwrap();
        }
    }
    m.push_str("    return out;\n}\n");
    let iface = Interface {
        attributes: g.attrs.iter().copied().collect(),
        uniforms: n,
        ..Default::default()
    };
    Ok((m, iface))
}

/// A fragment program to a module with entry point `main`.
pub fn fragment(p: &Program, opts: &Options) -> Result<(String, Interface), TranslateError> {
    assert_eq!(p.stage, Stage::Fragment);
    if let Some(f) = &p.fp20 {
        return fragment20(p, f, opts);
    }
    let mut g = Gen::new(p, opts);
    let body = g.body()?;
    let mut m = String::new();
    m.push_str(HELPERS);
    let np = p.declares.len().max(1);
    writeln!(m, "@group(1) @binding(0) var<uniform> p: array<vec4f, {np}>;").unwrap();
    let mut textures = vec![];
    for (k, (unit, target)) in g.textures.iter().enumerate() {
        let shadow = opts.shadow_units.contains(unit);
        let ty = match (target, shadow) {
            (_, true) => "texture_depth_2d",
            (Target::Cube, _) => "texture_cube<f32>",
            (Target::T3D, _) => "texture_3d<f32>",
            _ => "texture_2d<f32>",
        };
        let st = if shadow { "sampler_comparison" } else { "sampler" };
        writeln!(m, "@group(1) @binding({}) var t{unit}: {ty};", 1 + 2 * k).unwrap();
        writeln!(m, "@group(1) @binding({}) var s{unit}: {st};", 2 + 2 * k).unwrap();
        textures.push(TextureUse { unit: *unit, target: *target, shadow });
    }
    m.push_str("struct FIn {\n    @builtin(position) wpos: vec4f,\n");
    for (v, l) in VARYINGS {
        if g.inputs.contains(*v) {
            writeln!(m, "    @location({l}) {v}: vec4f,").unwrap();
        }
    }
    m.push_str("}\n@fragment\nfn main(fin: FIn) -> @location(0) vec4f {\n");
    decls(&mut m, &g);
    for o in &g.outputs {
        if o != "COL" {
            return Err(TranslateError(format!("writes unsupported output o[{o}]")));
        }
    }
    m.push_str("    var o_COL = vec4f(0.0);\n");
    m.push_str(&body);
    if let Some((func, reference, clamp)) = opts.alpha_test {
        let a = if clamp { "clamp(o_COL.a, 0.0, 1.0)" } else { "o_COL.a" };
        let r = format!("{:?}", reference.min(255) as f32 / 255.0);
        let pass = match func {
            0 => "false".to_string(),
            1 => format!("{a} < {r}"),
            2 => format!("{a} <= {r}"),
            3 => format!("{a} > {r}"),
            4 => format!("{a} >= {r}"),
            5 => format!("{a} == {r}"),
            6 => format!("{a} != {r}"),
            _ => "true".to_string(),
        };
        writeln!(m, "    if (!({pass})) {{ discard; }}").unwrap();
    }
    m.push_str("    return o_COL;\n}\n");
    let iface = Interface {
        uniforms: np as u32,
        params: p.declares.iter().map(|d| d.0.clone()).collect(),
        textures,
        inputs: VARYINGS.iter().filter(|v| g.inputs.contains(v.0)).map(|v| v.0.to_string()).collect(),
        ..Default::default()
    };
    Ok((m, iface))
}

fn decls(m: &mut String, g: &Gen) {
    for (c, n) in &g.temps {
        writeln!(m, "    var {c}{n} = vec4f(0.0);").unwrap();
    }
    if g.uses_cc {
        m.push_str("    var cc = vec4f(0.0);\n");
    }
    if g.uses_a0 {
        m.push_str("    var a0 = vec4<i32>(0);\n    var a1 = vec4<i32>(0);\n");
    }
    if g.p.insts.iter().any(|i| i.op == "BRA") {
        m.push_str("    var pc = 0;\n    var skip = false;\n");
    }
}

impl<'a> Gen<'a> {
    fn new(p: &'a Program, opts: &'a Options) -> Self {
        Gen {
            p,
            opts,
            out: String::new(),
            temps: BTreeSet::new(),
            outputs: BTreeSet::new(),
            inputs: BTreeSet::new(),
            attrs: BTreeSet::new(),
            max_const: 0,
            relative: false,
            textures: BTreeMap::new(),
            uses_cc: false,
            uses_a0: false,
        }
    }
}

/// An fp20 program (texture shader stages and register combiners).
fn fragment20(p: &Program, f: &crate::fp20::Program, opts: &Options) -> Result<(String, Interface), TranslateError> {
    use crate::fp20::{Expr, Input, Stage as TS};
    let mut m = String::new();
    m.push_str(HELPERS);
    let np = p.declares.len().max(1);
    writeln!(m, "@group(1) @binding(0) var<uniform> p: array<vec4f, {np}>;").unwrap();
    let mut textures = vec![];
    let mut inputs: BTreeSet<String> = BTreeSet::new();
    let mut k = 0;
    for (i, st) in f.stages.iter().enumerate() {
        let target = match st {
            TS::Texture(t) => *t,
            TS::PassThrough => {
                inputs.insert(format!("TEX{i}"));
                continue;
            }
            TS::Nop => continue,
        };
        let shadow = opts.shadow_units.contains(&(i as u32));
        let ty = match (target, shadow) {
            (_, true) => "texture_depth_2d",
            (Target::Cube, _) => "texture_cube<f32>",
            (Target::T3D, _) => "texture_3d<f32>",
            _ => "texture_2d<f32>",
        };
        writeln!(m, "@group(1) @binding({}) var t{i}: {ty};", 1 + 2 * k).unwrap();
        writeln!(m, "@group(1) @binding({}) var s{i}: {};", 2 + 2 * k, if shadow { "sampler_comparison" } else { "sampler" }).unwrap();
        textures.push(TextureUse { unit: i as u32, target, shadow });
        inputs.insert(format!("TEX{i}"));
        k += 1;
    }
    // Which colours the combiners read.
    let mut reads = vec![];
    let mut add_input = |x: &Input| reads.push(x.reg.clone());
    for c in &f.combiners {
        for a in c.rgb.iter().chain(c.alpha.iter()) {
            match &a.expr {
                Expr::Product(x, y) | Expr::Dot(x, y) => {
                    add_input(x);
                    add_input(y);
                }
                Expr::SumOfProducts(x, y, z, w) => {
                    add_input(x);
                    add_input(y);
                    add_input(z);
                    add_input(w);
                }
                Expr::Copy(x) => add_input(x),
            }
        }
    }
    if let Some(x) = &f.out_rgb {
        reads.push(x.reg.clone());
    }
    if let Some(x) = &f.out_a {
        reads.push(x.reg.clone());
    }
    for r in &reads {
        if r == "col0" {
            inputs.insert("COL0".into());
        }
        if r == "col1" {
            inputs.insert("COL1".into());
        }
    }
    m.push_str("struct FIn {\n    @builtin(position) wpos: vec4f,\n");
    for (v, l) in VARYINGS {
        if inputs.contains(*v) {
            writeln!(m, "    @location({l}) {v}: vec4f,").unwrap();
        }
    }
    m.push_str("}\n@fragment\nfn main(fin: FIn) -> @location(0) vec4f {\n");
    m.push_str("    let zero = vec4f(0.0);\n");
    for i in 0..4 {
        let v = match f.stages.get(i) {
            Some(TS::Texture(t)) => {
                let c = match t {
                    Target::Cube => format!("fin.TEX{i}.xyz"),
                    Target::T3D => format!("(fin.TEX{i}.xyz / fin.TEX{i}.w)"),
                    Target::Rect => format!("((fin.TEX{i}.xy / fin.TEX{i}.w) / vec2f(textureDimensions(t{i})))"),
                    _ => format!("(fin.TEX{i}.xy / fin.TEX{i}.w)"),
                };
                let s = format!("textureSample(t{i}, s{i}, {c})");
                if opts.expand_units.contains(&(i as u32)) { format!("({s} * 2.0 - 1.0)") } else { s }
            }
            Some(TS::PassThrough) => format!("clamp(fin.TEX{i}, vec4f(0.0), vec4f(1.0))"),
            _ => "vec4f(0.0)".into(),
        };
        writeln!(m, "    var tex{i} = {v};").unwrap();
    }
    let col = |n: &str| if inputs.contains(n) { format!("fin.{n}") } else { "vec4f(0.0)".into() };
    writeln!(m, "    var col0 = {};", col("COL0")).unwrap();
    writeln!(m, "    var col1 = {};", col("COL1")).unwrap();
    // spare0's alpha starts as texture 0's (NV_register_combiners).
    m.push_str("    var spare0 = vec4f(0.0, 0.0, 0.0, tex0.a);\n    var spare1 = vec4f(0.0);\n");
    m.push_str("    let fog = vec4f(0.0);\n");
    // Constants: named parameters, by register; per-stage ones as `[stage]`.
    let slot = |reg: &str, stage: usize| -> Option<usize> {
        p.declares.iter().position(|(n, _)| {
            p.vars.iter().any(|v| {
                &v.name == n
                    && (v.resource == format!("COMBINER_{}", reg.to_ascii_uppercase())
                        || v.resource == format!("COMBINER_STAGE_{}[{stage}]", reg.to_ascii_uppercase()))
            })
        })
    };
    let konst = |reg: &str, stage: usize, local: &[(usize, [f32; 4])]| -> String {
        let n: usize = reg.trim_start_matches("const").parse().unwrap_or(0);
        if let Some((_, v)) = local.iter().chain(f.global_consts.iter()).find(|c| c.0 == n) {
            return lit(*v);
        }
        match slot(reg, stage) {
            Some(i) => format!("p[{i}]"),
            None => "vec4f(0.0)".into(),
        }
    };
    let value = |x: &Input, stage: usize, local: &[(usize, [f32; 4])], rgb: bool| -> String {
        let base = if x.reg.starts_with("const") { konst(&x.reg, stage, local) } else { x.reg.clone() };
        let v = match x.part.as_str() {
            "a" => format!("vec4f({base}.a)"),
            "b" => format!("vec4f({base}.b)"),
            _ => format!("{base}"),
        };
        let _ = rgb;
        let v = match x.map.as_str() {
            "unsigned" => format!("max({v}, vec4f(0.0))"),
            "unsigned_invert" => format!("(vec4f(1.0) - clamp({v}, vec4f(0.0), vec4f(1.0)))"),
            "expand" => format!("(2.0 * max({v}, vec4f(0.0)) - 1.0)"),
            "half_bias" => format!("(max({v}, vec4f(0.0)) - 0.5)"),
            _ => v,
        };
        if x.neg { format!("(-{v})") } else { v }
    };
    for (si, c) in f.combiners.iter().enumerate() {
        writeln!(m, "    {{ // stage {si}").unwrap();
        let mut writes = vec![];
        for (part, list) in [("rgb", &c.rgb), ("a", &c.alpha)] {
            for a in list.iter() {
                let rgb = part == "rgb";
                let e = match &a.expr {
                    Expr::Product(x, y) => format!("{} * {}", value(x, si, &c.consts, rgb), value(y, si, &c.consts, rgb)),
                    Expr::SumOfProducts(x, y, z, w) => format!(
                        "{} * {} + {} * {}",
                        value(x, si, &c.consts, rgb),
                        value(y, si, &c.consts, rgb),
                        value(z, si, &c.consts, rgb),
                        value(w, si, &c.consts, rgb)
                    ),
                    Expr::Dot(x, y) => format!("vec4f(dot(({}).xyz, ({}).xyz))", value(x, si, &c.consts, rgb), value(y, si, &c.consts, rgb)),
                    Expr::Copy(x) => value(x, si, &c.consts, rgb),
                };
                let v = format!("clamp(({e} + vec4f({:?})) * {:?}, vec4f(-1.0), vec4f(1.0))", c.bias, c.scale);
                let dst = a.dst.split('.').next().unwrap_or("discard").to_string();
                writes.push((dst, part, v));
            }
        }
        // Every read of a stage comes before its writes.
        for (n, (dst, part, v)) in writes.iter().enumerate() {
            writeln!(m, "        let w{n} = {v};").unwrap();
            let _ = (dst, part);
        }
        for (n, (dst, part, _)) in writes.iter().enumerate() {
            if dst == "discard" {
                continue;
            }
            if *part == "rgb" {
                writeln!(m, "        {dst} = vec4f(w{n}.xyz, {dst}.w);").unwrap();
            } else {
                writeln!(m, "        {dst} = vec4f({dst}.xyz, w{n}.w);").unwrap();
            }
        }
        m.push_str("    }\n");
    }
    let last = f.combiners.len();
    let fin = |x: &Option<Input>| -> String {
        match x {
            Some(x) => format!("clamp({}, vec4f(0.0), vec4f(1.0))", value(x, last, &[], true)),
            None => "vec4f(0.0)".into(),
        }
    };
    writeln!(m, "    let rgb = {};", fin(&f.out_rgb)).unwrap();
    writeln!(m, "    let a = {};", fin(&f.out_a)).unwrap();
    m.push_str("    var o_COL = vec4f(rgb.xyz, a.w);\n");
    if let Some((func, reference, clamp)) = opts.alpha_test {
        let a = if clamp { "clamp(o_COL.a, 0.0, 1.0)" } else { "o_COL.a" };
        let r = format!("{:?}", reference.min(255) as f32 / 255.0);
        let pass = match func {
            0 => "false".to_string(),
            1 => format!("{a} < {r}"),
            2 => format!("{a} <= {r}"),
            3 => format!("{a} > {r}"),
            4 => format!("{a} >= {r}"),
            5 => format!("{a} == {r}"),
            6 => format!("{a} != {r}"),
            _ => "true".to_string(),
        };
        writeln!(m, "    if (!({pass})) {{ discard; }}").unwrap();
    }
    m.push_str("    return o_COL;\n}\n");
    let iface = Interface {
        uniforms: np as u32,
        params: p.declares.iter().map(|d| d.0.clone()).collect(),
        textures,
        inputs: VARYINGS.iter().filter(|v| inputs.contains(v.0)).map(|v| v.0.to_string()).collect(),
        ..Default::default()
    };
    Ok((m, iface))
}
