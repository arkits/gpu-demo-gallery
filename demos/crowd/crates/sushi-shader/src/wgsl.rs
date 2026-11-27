//! Emits one WGSL module per pass from its vertex and pixel programs.
//!
//! Bindings: group 0 holds vertex constants (0) and pixel constants (1) as
//! `array<vec4<f32>>`; group 1 holds, for the k-th sampler, texture `2k` and sampler
//! `2k + 1`.
//!
//! HLSL matrices are kept with rows as WGSL columns, so that `m[i]` means the same and
//! `mul(a, b)` becomes `b * a`.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use crate::hlsl::{self, Direction, Expr, Field, Function, Program, Scalar, Stmt, Ty};
use crate::{Compare, Pass, Result, Sampler};

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
    /// Four signed 16 bit numbers, passed at their stored magnitude.
    Short4,
    /// Four signed 16 bit numbers within -1 to 1.
    Short4n,
}

/// A vertex attribute the shader reads, at WGSL `location` = its position here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Input {
    pub semantic: String,
    pub index: u32,
    pub ty: ElementType,
}

/// A constant that the program reads, in registers from `at`.
#[derive(Debug, Clone, PartialEq)]
pub struct Uniform {
    pub name: String,
    pub at: u32,
    /// Registers per element, and elements if it is an array.
    pub registers: u32,
    pub count: Option<u32>,
    pub default: Vec<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub source: String,
    pub inputs: Vec<Input>,
    pub vs: Vec<Uniform>,
    pub ps: Vec<Uniform>,
    pub vs_consts: u32,
    pub ps_consts: u32,
    /// Texture, and whether it is a cube map and cannot be filtered.
    pub samplers: Vec<(String, bool, bool)>,
    /// Number of color targets written to.
    pub outputs: u32,
}

const HELPERS: &str = "\
fn pow1_(x: f32, y: f32) -> f32 { let a = abs(x); if (a < 1e-30) { return select(0.0, 1.0, y == 0.0); } return pow(a, y); }
fn pow2_(x: vec2<f32>, y: vec2<f32>) -> vec2<f32> { return vec2<f32>(pow1_(x.x, y.x), pow1_(x.y, y.y)); }
fn pow3_(x: vec3<f32>, y: vec3<f32>) -> vec3<f32> { return vec3<f32>(pow1_(x.x, y.x), pow1_(x.y, y.y), pow1_(x.z, y.z)); }
fn pow4_(x: vec4<f32>, y: vec4<f32>) -> vec4<f32> { return vec4<f32>(pow1_(x.x, y.x), pow1_(x.y, y.y), pow1_(x.z, y.z), pow1_(x.w, y.w)); }
fn rcp_(x: f32) -> f32 { return select(1.0 / x, 3.0e38, x == 0.0); }
fn nrm2_(v: vec2<f32>) -> vec2<f32> { return v * inverseSqrt(max(dot(v, v), 1e-30)); }
fn nrm3_(v: vec3<f32>) -> vec3<f32> { return v * inverseSqrt(max(dot(v, v), 1e-30)); }
fn nrm4_(v: vec4<f32>) -> vec4<f32> { return v * inverseSqrt(max(dot(v, v), 1e-30)); }
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Vertex,
    Pixel,
}

/// Where a struct field of the entry point comes from or goes: name and number.
fn semantic(s: &str) -> (String, u32, bool) {
    let mut s = s.to_ascii_lowercase();
    let centroid = s.ends_with("_centroid");
    if centroid {
        s.truncate(s.len() - "_centroid".len());
    }
    let name = s.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
    let n = s[name.len()..].parse().unwrap_or(0);
    (name, n, centroid)
}

fn name_of(ty: &Ty) -> String {
    let scalar = |s: &Scalar| match s {
        Scalar::Bool => "bool",
        Scalar::Int => "i32",
        Scalar::Float => "f32",
    };
    match ty {
        Ty::Void => "void".into(),
        Ty::Vec(s, 1) => scalar(s).into(),
        Ty::Vec(s, n) => format!("vec{n}<{}>", scalar(s)),
        // A matrix of one row or column is a vector here.
        Ty::Mat(r, c) if *r == 1 || *c == 1 => name_of(&Ty::float(r * c)),
        Ty::Mat(r, c) => format!("mat{r}x{c}<f32>"),
        Ty::Sampler => "sampler".into(),
        Ty::Struct(n) => format!("S_{n}"),
        Ty::Array(t, n) => format!("array<{}, {n}>", name_of(t)),
    }
}

struct Local {
    wgsl: String,
    ty: Ty,
    /// Set for sampler parameters: the texture they stand for.
    sampler: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Typed {
    s: String,
    ty: Ty,
}

impl Typed {
    fn new(s: impl Into<String>, ty: Ty) -> Self {
        Self { s: s.into(), ty }
    }
}

/// Translates one program.
struct Emitter<'a> {
    program: &'a Program,
    stage: Stage,
    scopes: Vec<HashMap<String, Local>>,
    /// Statements to run before the one being translated.
    lines: Vec<String>,
    next: u32,
    uniforms: Vec<Uniform>,
    registers: u32,
    samplers: BTreeSet<(String, bool)>,
    statics: BTreeMap<String, (String, Ty)>,
    /// Finished functions, and the names of those made or being made.
    functions: Vec<String>,
    made: HashMap<String, Ty>,
    /// Parameters to copy back when the current function returns.
    copies: Vec<(String, String)>,
    result: Ty,
    biases: &'a dyn Fn(&str) -> f32,
    /// Inside a loop or branch, where levels cannot be chosen by hardware everywhere.
    structs: BTreeSet<String>,
    /// Constants that the program changes for itself.
    written: BTreeSet<String>,
}

/// The variables that statements assign to.
fn written(s: &Stmt, out: &mut BTreeSet<String>) {
    fn expr(e: &Expr, out: &mut BTreeSet<String>) {
        fn root(e: &Expr) -> Option<&String> {
            match e {
                Expr::Var(n) => Some(n),
                Expr::Member(e, _) | Expr::Index(e, _) => root(e),
                _ => None,
            }
        }
        match e {
            Expr::Assign(_, t, v) => {
                out.extend(root(t).cloned());
                expr(v, out);
            }
            Expr::Step(_, _, t) => out.extend(root(t).cloned()),
            Expr::Binary(_, a, b) => {
                expr(a, out);
                expr(b, out);
            }
            Expr::Call(_, v) | Expr::Make(_, v) => v.iter().for_each(|e| expr(e, out)),
            _ => {}
        }
    }
    match s {
        Stmt::Expr(e) | Stmt::Return(Some(e)) | Stmt::Declare(_, _, Some(e)) => expr(e, out),
        Stmt::If(_, a, b) => {
            written(a, out);
            b.iter().for_each(|b| written(b, out));
        }
        Stmt::For(a, _, _, b) => {
            a.iter().for_each(|s| written(s, out));
            written(b, out);
        }
        Stmt::While(_, b) | Stmt::DoWhile(b, _) => written(b, out),
        Stmt::Block(v) => v.iter().for_each(|s| written(s, out)),
        _ => {}
    }
}

fn unsupported<T>(what: impl std::fmt::Display) -> Result<T> {
    Err(format!("unsupported {what}"))
}

fn swizzle(name: &str) -> Option<Vec<usize>> {
    if name.is_empty() || name.len() > 4 {
        return None;
    }
    let at = |set: &str| {
        name.chars()
            .map(|c| set.find(c))
            .collect::<Option<Vec<_>>>()
    };
    at("xyzw").or_else(|| at("rgba"))
}

const C: [&str; 4] = ["x", "y", "z", "w"];

fn letters(v: &[usize]) -> String {
    v.iter().map(|i| C[*i]).collect()
}

/// Elements of `_m01` and `_12` names.
fn elements(name: &str) -> Option<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    for part in name.strip_prefix('_')?.split('_') {
        let (digits, base) = match part.strip_prefix('m') {
            Some(d) => (d, 0),
            None => (part, 1),
        };
        let d: Vec<usize> = digits
            .chars()
            .map(|c| c.to_digit(10).map(|d| d as usize))
            .collect::<Option<_>>()?;
        let [r, c] = d.as_slice() else { return None };
        out.push((r.checked_sub(base)?, c.checked_sub(base)?));
    }
    Some(out)
}

fn number(v: f64) -> String {
    let s = format!("{:?}", v as f32);
    if s.contains("inf") || s.contains("NaN") {
        return "3.0e38".into();
    }
    match s.contains(['.', 'e']) {
        true => s,
        false => s + ".0",
    }
}

impl<'a> Emitter<'a> {
    fn new(program: &'a Program, stage: Stage, biases: &'a dyn Fn(&str) -> f32) -> Self {
        Self {
            program,
            stage,
            scopes: Vec::new(),
            lines: Vec::new(),
            next: 0,
            uniforms: Vec::new(),
            registers: 0,
            samplers: BTreeSet::new(),
            statics: BTreeMap::new(),
            functions: Vec::new(),
            made: HashMap::new(),
            copies: Vec::new(),
            result: Ty::Void,
            biases,
            structs: BTreeSet::new(),
            written: {
                let mut w = BTreeSet::new();
                for f in &program.functions {
                    f.body.iter().for_each(|s| written(s, &mut w));
                }
                w
            },
        }
    }

    fn prefix(&self) -> &'static str {
        match self.stage {
            Stage::Vertex => "vs",
            Stage::Pixel => "ps",
        }
    }

    fn unique(&mut self, name: &str) -> String {
        self.next += 1;
        format!("{name}_{}", self.next)
    }

    fn fresh(&mut self, name: &str, ty: &Ty) -> String {
        let wgsl = self.unique(name);
        let local = Local {
            wgsl: wgsl.clone(),
            ty: ty.clone(),
            sampler: None,
        };
        self.scopes
            .last_mut()
            .expect("scope")
            .insert(name.into(), local);
        wgsl
    }

    fn local(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// Gives a value a name, so that it can be used several times.
    fn hold(&mut self, e: &Typed) -> String {
        if e.s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return e.s.clone();
        }
        let n = self.unique("h");
        self.lines.push(format!("let {n} = {};", e.s));
        n
    }

    fn struct_(&mut self, name: &str) -> Result<()> {
        if self.structs.contains(name) {
            return Ok(());
        }
        let fields = self.program.structs.get(name).ok_or("missing struct")?;
        for f in fields {
            if let Ty::Struct(inner) = &f.ty {
                self.struct_(inner)?;
            }
        }
        self.structs.insert(name.into());
        Ok(())
    }

    fn convert(&mut self, e: Typed, to: &Ty) -> Result<Typed> {
        if e.ty == *to {
            return Ok(e);
        }
        let fail = || unsupported(format!("conversion of {:?} to {to:?} in {}", e.ty, e.s));
        let s = match (&e.ty, to) {
            (Ty::Mat(r, c), _) if *r == 1 || *c == 1 => {
                let v = Typed::new(e.s.clone(), Ty::float(r * c));
                return self.convert(v, to);
            }
            (_, Ty::Mat(r, c)) if (*r == 1 || *c == 1) => {
                let mut v = self.convert(e, &Ty::float(r * c))?;
                v.ty = to.clone();
                return Ok(v);
            }
            (Ty::Vec(a, n), Ty::Vec(b, m)) => {
                let s = match (n, m) {
                    (n, m) if n == m => e.s.clone(),
                    (1, m) => {
                        let one = self.convert(e.clone(), &Ty::Vec(*b, 1))?;
                        return Ok(Typed::new(
                            format!("{}({})", name_of(&Ty::Vec(*b, *m)), one.s),
                            to.clone(),
                        ));
                    }
                    (n, m) if n > m => {
                        format!("{}.{}", e.s, letters(&(0..*m as usize).collect::<Vec<_>>()))
                    }
                    _ => return fail(),
                };
                if a == b {
                    s
                } else {
                    let zero = |t: Scalar| match t {
                        Scalar::Int => "0",
                        _ => "0.0",
                    };
                    let name = name_of(&Ty::Vec(*b, *m));
                    match b {
                        Scalar::Bool => {
                            format!("({s} != {}({}))", name_of(&Ty::Vec(*a, *m)), zero(*a))
                        }
                        _ => format!("{name}({s})"),
                    }
                }
            }
            (Ty::Vec(_, 1), Ty::Mat(r, c)) => {
                let row = self.convert(e, &Ty::float(*c))?;
                let row = self.hold(&row);
                let rows = vec![row; *r as usize].join(", ");
                format!("{}({rows})", name_of(to))
            }
            (Ty::Mat(r, c), Ty::Mat(r2, c2)) if r >= r2 && c >= c2 => {
                let m = self.hold(&e);
                let cut = letters(&(0..*c2 as usize).collect::<Vec<_>>());
                let rows: Vec<String> = (0..*r2)
                    .map(|i| match c == c2 {
                        true => format!("{m}[{i}]"),
                        false => format!("{m}[{i}].{cut}"),
                    })
                    .collect();
                format!("{}({})", name_of(to), rows.join(", "))
            }
            (Ty::Mat(_, c), Ty::Vec(..)) => {
                // Row by row.
                let m = self.hold(&e);
                let Ty::Vec(_, n) = to else { return fail() };
                let parts: Vec<String> = (0..*n)
                    .map(|k| format!("{m}[{}][{}]", k / c, k % c))
                    .collect();
                let v = Typed::new(format!("vec{n}<f32>({})", parts.join(", ")), Ty::float(*n));
                let v = match n {
                    1 => Typed::new(parts[0].clone(), Ty::FLOAT),
                    _ => v,
                };
                return self.convert(v, to);
            }
            (Ty::Vec(_, 1), Ty::Struct(_) | Ty::Array(..)) => format!("{}()", name_of(to)),
            (Ty::Array(a, n), Ty::Array(b, m)) if n == m => {
                let held = self.hold(&e);
                let mut parts = Vec::new();
                for i in 0..*n {
                    parts.push(
                        self.convert(Typed::new(format!("{held}[{i}]"), (**a).clone()), b)?
                            .s,
                    );
                }
                format!("{}({})", name_of(to), parts.join(", "))
            }
            _ => return fail(),
        };
        if let Ty::Struct(n) = to {
            self.struct_(n)?;
        }
        Ok(Typed::new(s, to.clone()))
    }

    fn uniform(&mut self, name: &str, ty: &Ty) -> Result<Typed> {
        let (element, count) = match ty {
            Ty::Array(t, n) => ((**t).clone(), Some(*n)),
            t => (t.clone(), None),
        };
        let at = match self.uniforms.iter().find(|u| u.name == name) {
            Some(u) => u.at,
            None => {
                let registers = match &element {
                    Ty::Mat(_, c) => *c,
                    Ty::Vec(..) => 1,
                    t => return unsupported(format!("constant {name} of {t:?}")),
                };
                let mut default = Vec::new();
                if let Some(g) = self.program.globals.get(name)
                    && let Some(v) = &g.value
                {
                    flatten(v, &mut default);
                }
                let at = self.registers;
                self.registers += registers * count.unwrap_or(1);
                let u = Uniform {
                    name: name.into(),
                    at,
                    registers,
                    count,
                    default,
                };
                self.uniforms.push(u);
                at
            }
        };
        match count {
            // Read when indexed.
            Some(_) => Ok(Typed::new(format!("@{at}"), ty.clone())),
            None => Ok(Typed::new(self.load(&at.to_string(), &element), element)),
        }
    }

    /// Reads a constant from its registers.
    fn load(&self, at: &str, ty: &Ty) -> String {
        let c = match self.stage {
            Stage::Vertex => "vc.c",
            Stage::Pixel => "pc.c",
        };
        let cut = |n: u32| match n {
            4 => String::new(),
            n => format!(".{}", letters(&(0..n as usize).collect::<Vec<_>>())),
        };
        match ty {
            Ty::Vec(Scalar::Float, n) => format!("{c}[{at}]{}", cut(*n)),
            Ty::Vec(Scalar::Int, n) => format!("{}({c}[{at}]{})", name_of(ty), cut(*n)),
            Ty::Vec(Scalar::Bool, n) => {
                format!("({c}[{at}]{} != {}())", cut(*n), name_of(&Ty::float(*n)))
            }
            // Registers hold columns.
            Ty::Mat(r, n) => {
                let columns: Vec<String> = (0..*n)
                    .map(|k| format!("{c}[{at} + {k}]{}", cut(*r)))
                    .collect();
                match n {
                    1 => columns[0].clone(),
                    _ => format!("transpose(mat{n}x{r}<f32>({}))", columns.join(", ")),
                }
            }
            t => format!("bad {t:?}"),
        }
    }

    fn variable(&mut self, name: &str) -> Result<Typed> {
        if let Some(l) = self.local(name) {
            return Ok(Typed::new(l.wgsl.clone(), l.ty.clone()));
        }
        if let Some((w, t)) = self.statics.get(name) {
            return Ok(Typed::new(w.clone(), t.clone()));
        }
        let program = self.program;
        let g = program
            .globals
            .get(name)
            .ok_or_else(|| format!("unknown name {name}"))?;
        if g.ty == Ty::Sampler {
            return Ok(Typed::new(name, Ty::Sampler));
        }
        let written = self.written.contains(name);
        if g.fixed || written {
            // Statements for it would have no place to go.
            let before = std::mem::take(&mut self.lines);
            let v = match &g.value {
                Some(value) if g.fixed => self.initial(value, &g.ty)?,
                _ => {
                    let v = self.uniform(name, &g.ty)?;
                    self.whole(&v)?
                }
            };
            let extra = std::mem::replace(&mut self.lines, before);
            let w = format!("{}k_{name}", self.prefix());
            let mut text = String::new();
            let _ = writeln!(text, "fn {w}_() -> {} {{", name_of(&g.ty));
            for l in extra {
                let _ = writeln!(text, "  {l}");
            }
            let _ = writeln!(text, "  return {};\n}}", v.s);
            let _ = writeln!(text, "var<private> {w}: {};", name_of(&g.ty));
            self.functions.push(text);
            self.statics.insert(name.into(), (w.clone(), g.ty.clone()));
            return Ok(Typed::new(w, g.ty.clone()));
        }
        self.uniform(name, &g.ty)
    }

    fn initial(&mut self, e: &Expr, ty: &Ty) -> Result<Typed> {
        let Expr::List(list) = e else {
            let v = self.expr(e)?;
            return self.convert(v, ty);
        };
        let (count, element) = match ty {
            Ty::Array(t, n) => (*n, (**t).clone()),
            Ty::Vec(s, n) => (*n, Ty::Vec(*s, 1)),
            Ty::Mat(r, c) => (r * c, Ty::FLOAT),
            Ty::Struct(_) if list.is_empty() => (0, Ty::FLOAT),
            t => return unsupported(format!("list for {t:?}")),
        };
        if list.len() == count as usize && !matches!(ty, Ty::Mat(..)) {
            let parts: Vec<String> = list
                .iter()
                .map(|e| Ok(self.initial(e, &element)?.s))
                .collect::<Result<_>>()?;
            return Ok(Typed::new(
                format!("{}({})", name_of(ty), parts.join(", ")),
                ty.clone(),
            ));
        }
        // All components in order.
        let mut scalars = Vec::new();
        let mut todo: Vec<&Expr> = list.iter().rev().collect();
        while let Some(e) = todo.pop() {
            if let Expr::List(v) = e {
                todo.extend(v.iter().rev());
                continue;
            }
            let v = self.expr(e)?;
            self.components(v, &mut scalars)?;
        }
        fn build(ty: &Ty, from: &mut std::vec::IntoIter<String>) -> Result<String> {
            let parts: Vec<String> = match ty {
                Ty::Array(t, n) => (0..*n).map(|_| build(t, from)).collect::<Result<_>>()?,
                Ty::Vec(_, 1) => return from.next().ok_or_else(|| "list too short".into()),
                Ty::Vec(s, n) => (0..*n)
                    .map(|_| build(&Ty::Vec(*s, 1), from))
                    .collect::<Result<_>>()?,
                Ty::Mat(r, c) => (0..*r)
                    .map(|_| build(&Ty::float(*c), from))
                    .collect::<Result<_>>()?,
                t => return unsupported(format!("list for {t:?}")),
            };
            Ok(format!("{}({})", name_of(ty), parts.join(", ")))
        }
        let s = build(ty, &mut scalars.into_iter())?;
        Ok(Typed::new(s, ty.clone()))
    }

    /// Appends the components of a value as floats.
    fn components(&mut self, v: Typed, out: &mut Vec<String>) -> Result<()> {
        match v.ty.clone() {
            Ty::Vec(s, n) => {
                let f = self.convert(v, &Ty::float(n))?;
                let _ = s;
                if n == 1 {
                    out.push(f.s);
                } else {
                    let h = self.hold(&f);
                    out.extend((0..n as usize).map(|i| format!("{h}.{}", C[i])));
                }
            }
            Ty::Mat(r, c) => {
                let h = self.hold(&v);
                out.extend(
                    (0..r)
                        .flat_map(|i| (0..c).map(move |j| (i, j)))
                        .map(|(i, j)| format!("{h}[{i}][{j}]")),
                );
            }
            t => return unsupported(format!("components of {t:?}")),
        }
        Ok(())
    }

    /// The common type that two operands become.
    fn common(a: &Ty, b: &Ty) -> Result<Ty> {
        let rank = |s: &Scalar| match s {
            Scalar::Bool => 0,
            Scalar::Int => 1,
            Scalar::Float => 2,
        };
        Ok(match (a, b) {
            (Ty::Vec(s, n), Ty::Vec(t, m)) => {
                let s = if rank(s) >= rank(t) { *s } else { *t };
                let n = match (*n, *m) {
                    (1, m) => m,
                    (n, 1) => n,
                    (n, m) => n.min(m),
                };
                Ty::Vec(s, n)
            }
            (Ty::Mat(r, c), Ty::Mat(r2, c2)) => Ty::Mat(*r.min(r2), *c.min(c2)),
            (Ty::Mat(r, c), Ty::Vec(_, 1)) | (Ty::Vec(_, 1), Ty::Mat(r, c)) => Ty::Mat(*r, *c),
            (Ty::Mat(r, c), Ty::Vec(_, n)) | (Ty::Vec(_, n), Ty::Mat(r, c))
                if *r == 1 || *c == 1 =>
            {
                Ty::float((*n).min(r * c))
            }
            _ => return unsupported(format!("operands {a:?} and {b:?}")),
        })
    }

    fn arithmetic(&mut self, op: &str, a: Typed, b: Typed) -> Result<Typed> {
        let mut ty = Self::common(&a.ty, &b.ty)?;
        if let Ty::Vec(Scalar::Bool, n) = ty {
            ty = Ty::Vec(Scalar::Int, n);
        }
        if let Ty::Mat(r, c) = ty {
            if r == 1 || c == 1 {
                ty = Ty::float(r * c);
            } else if matches!(op, "*" | "/" | "%") {
                // By component; WGSL has only products with scalars.
                let scalar = |t: &Typed| matches!(t.ty, Ty::Vec(_, 1));
                if op == "*" && (scalar(&a) || scalar(&b)) {
                    let one = |me: &mut Self, t: Typed| match t.ty {
                        Ty::Vec(..) => me.convert(t, &Ty::FLOAT),
                        _ => Ok(t),
                    };
                    let (a, b) = (one(self, a)?, one(self, b)?);
                    return Ok(Typed::new(format!("({} * {})", a.s, b.s), ty));
                }
                let (a, b) = (self.convert(a, &ty)?, self.convert(b, &ty)?);
                let (a, b) = (self.hold(&a), self.hold(&b));
                let rows: Vec<String> = (0..r).map(|i| format!("{a}[{i}] {op} {b}[{i}]")).collect();
                return Ok(Typed::new(
                    format!("{}({})", name_of(&ty), rows.join(", ")),
                    ty,
                ));
            }
        }
        let (a, b) = (self.convert(a, &ty)?, self.convert(b, &ty)?);
        if op == "/" && ty == Ty::FLOAT && b.s.parse::<f32>().is_err() {
            return Ok(Typed::new(format!("({} * rcp_({}))", a.s, b.s), ty));
        }
        Ok(Typed::new(format!("({} {op} {})", a.s, b.s), ty))
    }

    fn binary(&mut self, op: &str, a: &Expr, b: &Expr) -> Result<Typed> {
        let (a, b) = (self.expr(a)?, self.expr(b)?);
        match op {
            "+" | "-" | "*" | "/" | "%" => self.arithmetic(op, a, b),
            "<" | ">" | "<=" | ">=" | "==" | "!=" => {
                let mut ty = Self::common(&a.ty, &b.ty)?;
                if let Ty::Vec(Scalar::Bool, n) = ty
                    && !matches!(op, "==" | "!=")
                {
                    ty = Ty::Vec(Scalar::Int, n);
                }
                let Ty::Vec(_, n) = ty else {
                    return unsupported("comparison of matrices");
                };
                let (a, b) = (self.convert(a, &ty)?, self.convert(b, &ty)?);
                Ok(Typed::new(
                    format!("({} {op} {})", a.s, b.s),
                    Ty::Vec(Scalar::Bool, n),
                ))
            }
            "&&" | "||" => {
                let Ty::Vec(_, n) = Self::common(&a.ty, &b.ty)? else {
                    return unsupported("logic on matrices");
                };
                let ty = Ty::Vec(Scalar::Bool, n);
                let (a, b) = (self.convert(a, &ty)?, self.convert(b, &ty)?);
                // Both sides are evaluated.
                Ok(Typed::new(format!("({} {} {})", a.s, &op[..1], b.s), ty))
            }
            op => unsupported(format!("operator {op}")),
        }
    }

    fn member(&mut self, e: Typed, name: &str) -> Result<Typed> {
        match &e.ty {
            Ty::Struct(s) => {
                let program = self.program;
                let f = program.structs[s].iter().find(|f| f.name == name);
                let f = f.ok_or_else(|| format!("no member {name}"))?;
                Ok(Typed::new(format!("{}.m_{name}", e.s), f.ty.clone()))
            }
            Ty::Vec(s, n) => {
                let v = swizzle(name).filter(|v| v.iter().all(|i| *i < *n as usize));
                let v = v.ok_or_else(|| format!("bad swizzle {name} of {n}"))?;
                let ty = Ty::Vec(*s, v.len() as u32);
                Ok(match (n, v.len()) {
                    (1, 1) => e,
                    (1, _) => Typed::new(format!("{}({})", name_of(&ty), e.s), ty),
                    _ => Typed::new(format!("{}.{}", e.s, letters(&v)), ty),
                })
            }
            Ty::Mat(r, c) => {
                let v = elements(name).ok_or_else(|| format!("no element {name}"))?;
                if *r == 1 || *c == 1 {
                    return unsupported("elements of a thin matrix");
                }
                let m = if v.len() > 1 {
                    self.hold(&e)
                } else {
                    e.s.clone()
                };
                let parts: Vec<String> = v.iter().map(|(r, c)| format!("{m}[{r}][{c}]")).collect();
                Ok(match v.len() {
                    1 => Typed::new(parts[0].clone(), Ty::FLOAT),
                    n => Typed::new(
                        format!("vec{n}<f32>({})", parts.join(", ")),
                        Ty::float(n as u32),
                    ),
                })
            }
            t => unsupported(format!("member {name} of {t:?}")),
        }
    }

    fn index(&mut self, e: Typed, i: Typed) -> Result<Typed> {
        let i = self.convert(i, &Ty::INT)?;
        let ty = match &e.ty {
            Ty::Array(t, _) => (**t).clone(),
            Ty::Vec(s, _) => Ty::Vec(*s, 1),
            Ty::Mat(_, c) => Ty::float(*c),
            t => return unsupported(format!("index of {t:?}")),
        };
        if matches!(e.ty, Ty::Vec(_, 1)) {
            return Ok(e);
        }
        if let Some(at) = e.s.strip_prefix('@') {
            let registers = match &ty {
                Ty::Mat(_, c) => *c,
                _ => 1,
            };
            let at = match registers {
                1 => format!("{at} + {}", i.s),
                n => format!("{at} + {} * {n}", i.s),
            };
            return Ok(Typed::new(self.load(&at, &ty), ty));
        }
        Ok(Typed::new(format!("{}[{}]", e.s, i.s), ty))
    }

    /// Assigns to what `target` names.
    fn assign(&mut self, target: &Expr, value: Typed) -> Result<()> {
        if let Expr::Member(inner, name) = target {
            let place = self.expr(inner)?;
            match &place.ty {
                Ty::Vec(s, n) if *n > 1 => {
                    let v = swizzle(name).ok_or_else(|| format!("bad swizzle {name}"))?;
                    let value = self.convert(value, &Ty::Vec(*s, v.len() as u32))?;
                    if v.len() == 1 {
                        self.lines
                            .push(format!("{}.{} = {};", place.s, C[v[0]], value.s));
                        return Ok(());
                    }
                    let t = self.unique("t");
                    self.lines.push(format!("let {t} = {};", value.s));
                    for (k, i) in v.iter().enumerate() {
                        self.lines
                            .push(format!("{}.{} = {t}.{};", place.s, C[*i], C[k]));
                    }
                    return Ok(());
                }
                Ty::Mat(r, c) if *r > 1 && *c > 1 => {
                    let v = elements(name).ok_or_else(|| format!("no element {name}"))?;
                    let value = self.convert(value, &Ty::float(v.len() as u32))?;
                    let t = self.unique("t");
                    self.lines.push(format!("let {t} = {};", value.s));
                    for (k, (r, c)) in v.iter().enumerate() {
                        let part = if v.len() == 1 {
                            t.clone()
                        } else {
                            format!("{t}.{}", C[k])
                        };
                        self.lines.push(format!("{}[{r}][{c}] = {part};", place.s));
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        let place = self.expr(target)?;
        let value = self.convert(value, &place.ty)?;
        self.lines.push(format!("{} = {};", place.s, value.s));
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> Result<Typed> {
        match e {
            Expr::Float(v) => Ok(Typed::new(number(*v), Ty::FLOAT)),
            Expr::Int(v) => Ok(Typed::new(v.to_string(), Ty::INT)),
            Expr::Bool(v) => Ok(Typed::new(v.to_string(), Ty::BOOL)),
            Expr::Var(n) => self.variable(n),
            Expr::Call(n, args) => self.call(n, args),
            Expr::Make(ty, args) => self.make(ty, args),
            Expr::Cast(ty, e) => {
                let v = self.expr(e)?;
                self.convert(v, ty)
            }
            Expr::Member(e, n) => {
                let v = self.expr(e)?;
                self.member(v, n)
            }
            Expr::Index(e, i) => {
                let (e, i) = (self.expr(e)?, self.expr(i)?);
                self.index(e, i)
            }
            Expr::Unary(op, e) => {
                let v = self.expr(e)?;
                match *op {
                    "-" => {
                        let v = match v.ty {
                            Ty::Vec(Scalar::Bool, n) => {
                                self.convert(v, &Ty::Vec(Scalar::Int, n))?
                            }
                            _ => v,
                        };
                        Ok(Typed::new(format!("(-{})", v.s), v.ty))
                    }
                    "!" => {
                        let Ty::Vec(_, n) = v.ty else {
                            return unsupported("! of a matrix");
                        };
                        let v = self.convert(v, &Ty::Vec(Scalar::Bool, n))?;
                        Ok(Typed::new(format!("(!{})", v.s), v.ty))
                    }
                    op => unsupported(format!("operator {op}")),
                }
            }
            Expr::Step(op, before, target) => {
                let old = self.expr(target)?;
                let t = self.unique("t");
                if !before {
                    self.lines.push(format!("let {t} = {};", old.s));
                }
                let new = self.arithmetic(op, old.clone(), Typed::new("1", Ty::INT))?;
                self.assign(target, new)?;
                match before {
                    true => self.expr(target),
                    false => Ok(Typed::new(t, old.ty)),
                }
            }
            Expr::Binary(op, a, b) => self.binary(op, a, b),
            Expr::Assign(op, target, value) => {
                let v = match &**value {
                    list @ Expr::List(_) => {
                        let ty = self.expr(target)?.ty;
                        self.initial(list, &ty)?
                    }
                    value => self.expr(value)?,
                };
                let v = match op.is_empty() {
                    true => v,
                    false => {
                        let old = self.expr(target)?;
                        self.arithmetic(op, old, v)?
                    }
                };
                self.assign(target, v)?;
                self.expr(target)
            }
            Expr::Select(c, a, b) => {
                let (c, a, b) = (self.expr(c)?, self.expr(a)?, self.expr(b)?);
                let mut ty = Self::common(&a.ty, &b.ty)?;
                if let (Ty::Vec(s, 1), Ty::Vec(_, n)) = (&ty, &c.ty) {
                    ty = Ty::Vec(*s, *n);
                }
                let (a, b) = (self.convert(a, &ty)?, self.convert(b, &ty)?);
                if let Ty::Mat(r, _) = ty {
                    let c = self.convert(c, &Ty::BOOL)?;
                    let (a, b, c) = (self.hold(&a), self.hold(&b), self.hold(&c));
                    let rows: Vec<String> = (0..r)
                        .map(|i| format!("select({b}[{i}], {a}[{i}], {c})"))
                        .collect();
                    return Ok(Typed::new(
                        format!("{}({})", name_of(&ty), rows.join(", ")),
                        ty,
                    ));
                }
                let Ty::Vec(_, n) = ty else {
                    return unsupported("choice");
                };
                let c = match c.ty {
                    Ty::Vec(_, 1) => self.convert(c, &Ty::BOOL)?,
                    _ => self.convert(c, &Ty::Vec(Scalar::Bool, n))?,
                };
                Ok(Typed::new(format!("select({}, {}, {})", b.s, a.s, c.s), ty))
            }
            Expr::List(_) => unsupported("list here"),
        }
    }

    fn make(&mut self, ty: &Ty, args: &[Expr]) -> Result<Typed> {
        let mut values = Vec::new();
        for a in args {
            values.push(self.expr(a)?);
        }
        if let [one] = values.as_slice() {
            return self.convert(one.clone(), ty);
        }
        match ty {
            Ty::Vec(s, n) => {
                let mut parts = Vec::new();
                let mut left = *n;
                for v in values {
                    let Ty::Vec(_, k) = v.ty else {
                        let mut c = Vec::new();
                        self.components(v, &mut c)?;
                        left = left.saturating_sub(c.len() as u32);
                        parts.extend(c);
                        continue;
                    };
                    if left == 0 {
                        break;
                    }
                    let k = k.min(left);
                    left -= k;
                    parts.push(self.convert(v, &Ty::Vec(*s, k))?.s);
                }
                if left > 0 {
                    return unsupported(format!("{ty:?} with too few components"));
                }
                Ok(Typed::new(
                    format!("{}({})", name_of(ty), parts.join(", ")),
                    ty.clone(),
                ))
            }
            Ty::Mat(r, c) => {
                let mut scalars = Vec::new();
                for v in values {
                    self.components(v, &mut scalars)?;
                }
                if scalars.len() < (r * c) as usize {
                    return unsupported("matrix with too few components");
                }
                let rows: Vec<String> = scalars
                    .chunks(*c as usize)
                    .take(*r as usize)
                    .map(|row| format!("{}({})", name_of(&Ty::float(*c)), row.join(", ")))
                    .collect();
                Ok(Typed::new(
                    format!("{}({})", name_of(ty), rows.join(", ")),
                    ty.clone(),
                ))
            }
            t => unsupported(format!("constructor of {t:?}")),
        }
    }

    /// Converts all arguments to floats of a common size.
    fn floats(&mut self, args: Vec<Typed>) -> Result<(Vec<String>, u32)> {
        let mut n = 1;
        for a in &args {
            match a.ty {
                Ty::Vec(_, 1) => {}
                Ty::Vec(_, k) => n = if n == 1 { k } else { n.min(k) },
                Ty::Mat(r, c) if r == 1 || c == 1 => n = r * c,
                _ => return unsupported(format!("argument {:?}", a.ty)),
            }
        }
        let made = args
            .into_iter()
            .map(|a| Ok(self.convert(a, &Ty::float(n))?.s))
            .collect::<Result<_>>()?;
        Ok((made, n))
    }

    fn texture_name(&self, e: &Expr) -> Result<String> {
        let Expr::Var(n) = e else {
            return unsupported("computed sampler");
        };
        if let Some(l) = self.local(n) {
            return l
                .sampler
                .clone()
                .ok_or_else(|| format!("{n} is not a sampler"));
        }
        Ok(n.clone())
    }

    fn sample(&mut self, name: &str, args: &[Expr]) -> Result<Typed> {
        let sampler = args.first().ok_or("missing sampler")?;
        if let Expr::Index(array, index) = sampler {
            if name != "tex2D" || args.len() != 2 {
                return unsupported("sampler array lookup other than tex2D");
            }
            return self.sample_array(array, index, &args[1]);
        }
        let texture = self.texture_name(sampler)?;
        let cube = name.starts_with("texCUBE");
        self.samplers.insert((texture.clone(), cube));
        let n = if cube { 3 } else { 2 };
        let coord = self.expr(args.get(1).ok_or("missing coordinates")?)?;
        let bias = (self.biases)(&texture);
        let (t, s) = (format!("t_{texture}"), format!("s_{texture}"));
        let kind = name
            .trim_start_matches("tex2D")
            .trim_start_matches("texCUBE");
        let made = match kind {
            "" if args.len() == 4 => {
                let (dx, dy) = (self.expr(&args[2])?, self.expr(&args[3])?);
                let c = self.convert(coord, &Ty::float(n))?;
                let (dx, dy) = (
                    self.convert(dx, &Ty::float(n))?,
                    self.convert(dy, &Ty::float(n))?,
                );
                format!("textureSampleGrad({t}, {s}, {}, {}, {})", c.s, dx.s, dy.s)
            }
            "" => {
                let c = self.convert(coord, &Ty::float(n))?;
                match (self.stage, bias == 0.0) {
                    (Stage::Vertex, _) => format!("textureSampleLevel({t}, {s}, {}, 0.0)", c.s),
                    (_, true) => format!("textureSample({t}, {s}, {})", c.s),
                    _ => format!(
                        "textureSampleBias({t}, {s}, {}, {})",
                        c.s,
                        number(bias.into())
                    ),
                }
            }
            "lod" => {
                let all = self.convert(coord, &Ty::float(4))?;
                let h = self.hold(&all);
                format!(
                    "textureSampleLevel({t}, {s}, {h}.{}, {h}.w)",
                    &"xyz"[..n as usize]
                )
            }
            "bias" => {
                let all = self.convert(coord, &Ty::float(4))?;
                let h = self.hold(&all);
                format!(
                    "textureSampleBias({t}, {s}, {h}.{}, {h}.w)",
                    &"xyz"[..n as usize]
                )
            }
            "proj" => {
                let all = self.convert(coord, &Ty::float(4))?;
                let h = self.hold(&all);
                format!(
                    "textureSample({t}, {s}, {h}.{} * rcp_({h}.w))",
                    &"xyz"[..n as usize]
                )
            }
            k => return unsupported(format!("tex2D{k}")),
        };
        Ok(Typed::new(made, Ty::float(4)))
    }

    /// Sushi names consecutive D3D sampler slots with a numeric suffix.
    fn sample_array(&mut self, array: &Expr, index: &Expr, coord: &Expr) -> Result<Typed> {
        let Expr::Var(name) = array else {
            return unsupported("computed sampler array");
        };
        let Some(Ty::Array(element, count)) = self.program.globals.get(name).map(|g| &g.ty) else {
            return unsupported("sampler array must be global");
        };
        if **element != Ty::Sampler || *count == 0 || *count > 16 {
            return unsupported("invalid sampler array");
        }
        let count = *count;
        let index = self.expr(index)?;
        let index = self.convert(index, &Ty::INT)?;
        let coords = self.expr(coord)?;
        let coords = self.convert(coords, &Ty::float(2))?;
        let uv = self.hold(&coords);
        let gradients = (self.stage == Stage::Pixel).then(|| {
            let dx = self.hold(&Typed::new(format!("dpdx({uv})"), Ty::float(2)));
            let dy = self.hold(&Typed::new(format!("dpdy({uv})"), Ty::float(2)));
            (dx, dy)
        });
        let out = self.unique("sample");
        self.lines.push(format!("var {out} = vec4<f32>(0.0);"));
        self.lines.push(format!("switch {} {{", index.s));
        for i in 0..count {
            let tex = format!("{name}{i}");
            self.samplers.insert((tex.clone(), false));
            let sample = if let Some((dx, dy)) = &gradients {
                let scale = number(f64::from((self.biases)(&tex).exp2()));
                format!("textureSampleGrad(t_{tex}, s_{tex}, {uv}, {dx} * {scale}, {dy} * {scale})")
            } else {
                format!("textureSampleLevel(t_{tex}, s_{tex}, {uv}, 0.0)")
            };
            self.lines
                .push(format!("case {i}: {{ {out} = {sample}; }}"));
        }
        self.lines.push("default: {}".into());
        self.lines.push("}".into());
        Ok(Typed::new(out, Ty::float(4)))
    }

    fn mul(&mut self, a: Typed, b: Typed) -> Result<Typed> {
        match (a.ty.clone(), b.ty.clone()) {
            (Ty::Mat(r, c), Ty::Mat(r2, c2)) => {
                let k = c.min(r2);
                let a = self.convert(a, &Ty::Mat(r, k))?;
                let b = self.convert(b, &Ty::Mat(k, c2))?;
                Ok(Typed::new(format!("({} * {})", b.s, a.s), Ty::Mat(r, c2)))
            }
            (Ty::Vec(_, n), Ty::Mat(r, c)) if n > 1 => {
                let k = n.min(r);
                let v = self.convert(a, &Ty::float(k))?;
                let m = self.convert(b, &Ty::Mat(k, c))?;
                Ok(Typed::new(format!("({} * {})", m.s, v.s), Ty::float(c)))
            }
            (Ty::Mat(r, c), Ty::Vec(_, n)) if n > 1 => {
                let k = n.min(c);
                let m = self.convert(a, &Ty::Mat(r, k))?;
                let v = self.convert(b, &Ty::float(k))?;
                Ok(Typed::new(format!("({} * {})", v.s, m.s), Ty::float(r)))
            }
            (Ty::Vec(_, n), Ty::Vec(_, m)) if n > 1 && m > 1 => {
                let (v, _) = self.floats(vec![a, b])?;
                Ok(Typed::new(format!("dot({}, {})", v[0], v[1]), Ty::FLOAT))
            }
            _ => self.arithmetic("*", a, b),
        }
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Result<Typed> {
        if self.program.functions.iter().any(|f| f.name == name) {
            return self.user(name, args);
        }
        if name.starts_with("tex2D") || name.starts_with("texCUBE") {
            return self.sample(name, args);
        }
        if name == "sincos" {
            let a = self.expr(&args[0])?;
            let (v, n) = self.floats(vec![a])?;
            let h = self.hold(&Typed::new(v[0].clone(), Ty::float(n)));
            self.assign(&args[1], Typed::new(format!("sin({h})"), Ty::float(n)))?;
            self.assign(&args[2], Typed::new(format!("cos({h})"), Ty::float(n)))?;
            return Ok(Typed::new("0.0", Ty::FLOAT));
        }
        let mut v = Vec::new();
        for a in args {
            v.push(self.expr(a)?);
        }
        let want = match name {
            "mul" | "dot" | "pow" | "min" | "max" | "fmod" | "atan2" | "step" | "reflect"
            | "cross" | "distance" | "ldexp" | "modf" => 2,
            "lerp" | "clamp" | "smoothstep" | "refract" => 3,
            _ => 1,
        };
        if v.len() != want {
            return unsupported(format!("{name} of {} arguments", v.len()));
        }
        // Passes matrices through by rows.
        if let [
            Typed {
                ty: Ty::Mat(r, c), ..
            },
        ] = v.as_slice()
            && name == "transpose"
        {
            let ty = Ty::Mat(*c, *r);
            return Ok(Typed::new(format!("transpose({})", v[0].s), ty));
        }
        match name {
            "mul" => {
                let b = v.pop().expect("two");
                let a = v.pop().expect("one");
                return self.mul(a, b);
            }
            "determinant" => return Ok(Typed::new(format!("determinant({})", v[0].s), Ty::FLOAT)),
            "cross" => {
                let b = self.convert(v.pop().expect("two"), &Ty::float(3))?;
                let a = self.convert(v.pop().expect("one"), &Ty::float(3))?;
                return Ok(Typed::new(format!("cross({}, {})", a.s, b.s), Ty::float(3)));
            }
            "refract" => {
                let eta = self.convert(v.pop().expect("three"), &Ty::FLOAT)?;
                let (a, n) = self.floats(v)?;
                return Ok(Typed::new(
                    format!("refract({}, {}, {})", a[0], a[1], eta.s),
                    Ty::float(n),
                ));
            }
            "any" | "all" => {
                let Ty::Vec(_, n) = v[0].ty else {
                    return unsupported(name);
                };
                let b = self.convert(v.remove(0), &Ty::Vec(Scalar::Bool, n))?;
                return Ok(Typed::new(
                    if n == 1 {
                        b.s
                    } else {
                        format!("{name}({})", b.s)
                    },
                    Ty::BOOL,
                ));
            }
            "clip" => {
                let (a, n) = self.floats(v)?;
                let test = match n {
                    1 => format!("{} < 0.0", a[0]),
                    n => format!("any({} < vec{n}<f32>())", a[0]),
                };
                self.lines.push(format!("if ({test}) {{ discard; }}"));
                return Ok(Typed::new("0.0", Ty::FLOAT));
            }
            "abs" | "sign" | "min" | "max" | "clamp" => {
                // Whole numbers stay whole.
                let mut ty = v[0].ty.clone();
                for x in &v[1..] {
                    ty = Self::common(&ty, &x.ty)?;
                }
                if let Ty::Vec(Scalar::Int, n) = ty
                    && name != "sign"
                {
                    let made: Vec<String> = v
                        .into_iter()
                        .map(|x| Ok(self.convert(x, &Ty::Vec(Scalar::Int, n))?.s))
                        .collect::<Result<_>>()?;
                    return Ok(Typed::new(format!("{name}({})", made.join(", ")), ty));
                }
            }
            _ => {}
        }
        let (a, n) = self.floats(v)?;
        let same = |s: String| Ok(Typed::new(s, Ty::float(n)));
        let one = |s: String| Ok(Typed::new(s, Ty::FLOAT));
        let all = a.join(", ");
        match name {
            "dot" if n == 1 => one(format!("({} * {})", a[0], a[1])),
            "dot" => one(format!("dot({all})")),
            "length" | "distance" => one(format!("{name}({all})")),
            "normalize" if n == 1 => same(format!("sign({})", a[0])),
            "normalize" => same(format!("nrm{n}_({})", a[0])),
            "pow" => same(format!("pow{n}_({all})")),
            "saturate" => same(format!(
                "clamp({}, {}, {})",
                a[0],
                splat("0.0", n),
                splat("1.0", n)
            )),
            "lerp" => same(format!("mix({all})")),
            "frac" => same(format!("fract({all})")),
            "fmod" => same(format!("({} % {})", a[0], a[1])),
            "ddx" => same(format!("dpdx({all})")),
            "ddy" => same(format!("dpdy({all})")),
            "sqrt" => same(format!("sqrt(abs({all}))")),
            "rsqrt" => same(format!(
                "inverseSqrt(max(abs({all}), {}))",
                splat("1e-30", n)
            )),
            "log2" | "log" => same(format!("{name}(max(abs({all}), {}))", splat("1e-30", n))),
            "radians" | "degrees" | "sin" | "cos" | "tan" | "atan" | "exp" | "exp2" | "floor"
            | "ceil" | "round" | "trunc" | "abs" | "sign" | "min" | "max" | "clamp" | "atan2"
            | "step" | "reflect" | "fwidth" => same(format!("{name}({all})")),
            "smoothstep" => {
                let z = self.hold(&Typed::new(
                    format!(
                        "clamp(({2} - {0}) / ({1} - {0}), {3}, {4})",
                        a[0],
                        a[1],
                        a[2],
                        splat("0.0", n),
                        splat("1.0", n)
                    ),
                    Ty::float(n),
                ));
                same(format!("({z} * {z} * ({} - 2.0 * {z}))", splat("3.0", n)))
            }
            "acos" | "asin" => same(format!(
                "{name}(clamp({all}, {}, {}))",
                splat("-1.0", n),
                splat("1.0", n)
            )),
            "ldexp" => same(format!("({} * exp2({}))", a[0], a[1])),
            n => unsupported(format!("function {n}")),
        }
    }

    fn user(&mut self, name: &str, args: &[Expr]) -> Result<Typed> {
        let program = self.program;
        // What each argument is, without keeping what that takes.
        let mut types = Vec::new();
        for a in args {
            if let Expr::Var(_) = a
                && self.texture_name(a).is_ok_and(|n| {
                    self.local(&n).is_none() && program.globals.get(&n).is_some_and(|g| g.ty == Ty::Sampler)
                        || matches!(a, Expr::Var(v) if self.local(v).is_some_and(|l| l.sampler.is_some()))
                })
            {
                types.push(Ty::Sampler);
                continue;
            }
            let before = self.lines.len();
            types.push(self.expr(a)?.ty);
            self.lines.truncate(before);
        }
        let cost = |f: &Function| -> Option<u32> {
            if f.params.len() < types.len()
                || f.params[types.len()..].iter().any(|p| p.default.is_none())
            {
                return None;
            }
            let mut sum = 0;
            for (p, t) in f.params.iter().zip(&types) {
                sum += match (t, &p.ty) {
                    (a, b) if a == b => 0,
                    (Ty::Sampler, _) | (_, Ty::Sampler) => return None,
                    (Ty::Vec(a, n), Ty::Vec(b, m)) => {
                        let scalar = u32::from(a != b);
                        match (n, m) {
                            (n, m) if n == m => scalar,
                            (1, _) => 4 + scalar,
                            (n, m) if n > m => 8 + (n - m) * 2 + scalar,
                            _ => return None,
                        }
                    }
                    (Ty::Vec(_, 1), Ty::Mat(..)) => 12,
                    (Ty::Mat(r, c), Ty::Mat(r2, c2)) if r >= r2 && c >= c2 => 10,
                    (Ty::Mat(..), Ty::Vec(..)) => 30,
                    (Ty::Array(..), Ty::Array(..)) => 1,
                    _ => return None,
                };
            }
            Some(sum)
        };
        // Later definitions replace earlier ones of the same kind.
        let found = program
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| f.name == name)
            .filter_map(|(i, f)| Some((cost(f)?, std::cmp::Reverse(i), f)))
            .min_by_key(|(c, i, _)| (*c, *i));
        let Some((_, std::cmp::Reverse(number), f)) = found else {
            return unsupported(format!("call of {name} with {types:?}"));
        };
        let mut samplers = Vec::new();
        let mut values = Vec::new();
        let mut back = Vec::new();
        let defaults = f.params[args.len()..]
            .iter()
            .filter_map(|p| p.default.as_ref());
        for (p, a) in f.params.iter().zip(args.iter().chain(defaults)) {
            if p.ty == Ty::Sampler {
                samplers.push(self.texture_name(a)?);
                continue;
            }
            match p.direction {
                Direction::In => {
                    let v = self.expr(a)?;
                    let v = match v.s.strip_prefix('@') {
                        // Whole arrays of constants are copied.
                        Some(_) => self.whole(&v)?,
                        None => v,
                    };
                    values.push(self.convert(v, &p.ty)?.s);
                }
                d => {
                    let t = self.unique("out");
                    let first = match d {
                        Direction::InOut => {
                            let v = self.expr(a)?;
                            format!(" = {}", self.convert(v, &p.ty)?.s)
                        }
                        _ => String::new(),
                    };
                    self.lines
                        .push(format!("var {t}: {}{first};", name_of(&p.ty)));
                    values.push(format!("&{t}"));
                    back.push((a, t, p.ty.clone()));
                }
            }
        }
        let mut w = format!("{}f_{name}_{number}", self.prefix());
        for s in &samplers {
            w.push('_');
            w.push_str(s);
        }
        if !self.made.contains_key(&w) {
            self.made.insert(w.clone(), f.result.clone());
            self.function(&w, f, &samplers)?;
        }
        let made = format!("{w}({})", values.join(", "));
        if f.result == Ty::Void {
            self.lines.push(format!("{made};"));
            for (a, t, ty) in back {
                self.assign(a, Typed::new(t, ty))?;
            }
            return Ok(Typed::new("0.0", Ty::FLOAT));
        }
        if back.is_empty() {
            return Ok(Typed::new(made, f.result.clone()));
        }
        let h = self.unique("h");
        self.lines.push(format!("let {h} = {made};"));
        for (a, t, ty) in back {
            self.assign(a, Typed::new(t, ty))?;
        }
        Ok(Typed::new(h, f.result.clone()))
    }

    fn whole(&mut self, v: &Typed) -> Result<Typed> {
        let Ty::Array(_, n) = &v.ty else {
            return Ok(v.clone());
        };
        let mut parts = Vec::new();
        for i in 0..*n {
            parts.push(self.index(v.clone(), Typed::new(i.to_string(), Ty::INT))?.s);
        }
        Ok(Typed::new(
            format!("{}({})", name_of(&v.ty), parts.join(", ")),
            v.ty.clone(),
        ))
    }

    fn function(&mut self, wgsl: &str, f: &Function, samplers: &[String]) -> Result<()> {
        let scopes = std::mem::replace(&mut self.scopes, vec![HashMap::new()]);
        let lines = std::mem::take(&mut self.lines);
        let copies = std::mem::take(&mut self.copies);
        let result = std::mem::replace(&mut self.result, f.result.clone());
        let mut params = Vec::new();
        let mut start = Vec::new();
        let mut textures = samplers.iter();
        for p in &f.params {
            if p.ty == Ty::Sampler {
                let local = Local {
                    wgsl: String::new(),
                    ty: Ty::Sampler,
                    sampler: textures.next().cloned(),
                };
                self.scopes[0].insert(p.name.clone(), local);
                continue;
            }
            if let Ty::Struct(s) = &p.ty {
                self.struct_(s)?;
            }
            let v = self.fresh(&p.name, &p.ty);
            match p.direction {
                Direction::In => {
                    params.push(format!("{v}_: {}", name_of(&p.ty)));
                    start.push(format!("var {v} = {v}_;"));
                }
                d => {
                    params.push(format!("{v}_: ptr<function, {}>", name_of(&p.ty)));
                    start.push(match d {
                        Direction::Out => format!("var {v}: {};", name_of(&p.ty)),
                        _ => format!("var {v} = *{v}_;"),
                    });
                    self.copies.push((format!("*{v}_"), v));
                }
            }
        }
        if let Ty::Struct(s) = &f.result {
            self.struct_(s)?;
        }
        for s in &f.body {
            self.statement(s)?;
        }
        if f.result == Ty::Void {
            self.leave();
        } else if !matches!(f.body.last(), Some(Stmt::Return(_))) {
            // Paths without a value do not happen, but have to look right.
            self.lines.push(format!("return {}();", name_of(&f.result)));
        }
        let mut text = match f.result {
            Ty::Void => format!("fn {wgsl}({}) {{\n", params.join(", ")),
            _ => format!(
                "fn {wgsl}({}) -> {} {{\n",
                params.join(", "),
                name_of(&f.result)
            ),
        };
        for l in start.iter().chain(&self.lines) {
            let _ = writeln!(text, "  {l}");
        }
        text.push_str("}\n");
        self.functions.push(text);
        self.scopes = scopes;
        self.lines = lines;
        self.copies = copies;
        self.result = result;
        Ok(())
    }

    fn leave(&mut self) {
        for (to, from) in self.copies.clone() {
            self.lines.push(format!("{to} = {from};"));
        }
    }

    fn scoped(&mut self, body: &Stmt) -> Result<()> {
        self.scopes.push(HashMap::new());
        match body {
            Stmt::Block(v) => {
                for s in v {
                    self.statement(s)?;
                }
            }
            s => self.statement(s)?,
        }
        self.scopes.pop();
        Ok(())
    }

    fn statement(&mut self, s: &Stmt) -> Result<()> {
        match s {
            Stmt::Declare(ty, name, value) => {
                if let Ty::Struct(n) = ty {
                    self.struct_(n)?;
                }
                let value = match value {
                    Some(v) => Some(self.initial(v, ty)?),
                    None => None,
                };
                let w = self.fresh(name, ty);
                self.lines.push(match value {
                    Some(v) => format!("var {w}: {} = {};", name_of(ty), v.s),
                    None => format!("var {w}: {};", name_of(ty)),
                });
            }
            Stmt::Expr(e) => {
                let e = match e {
                    // The value is not needed.
                    Expr::Step(op, _, t) => Expr::Step(op, true, t.clone()),
                    e => e.clone(),
                };
                let made = self.expr(&e)?;
                if matches!(e, Expr::Call(..)) && made.s.contains("f_") && made.s.ends_with(')') {
                    self.lines.push(format!("_ = {};", made.s));
                }
            }
            Stmt::If(c, yes, no) => {
                let c = self.expr(c)?;
                let c = match c.ty {
                    Ty::Vec(_, n) if n > 1 => {
                        let b = self.convert(c, &Ty::Vec(Scalar::Bool, n))?;
                        Typed::new(format!("all({})", b.s), Ty::BOOL)
                    }
                    _ => self.convert(c, &Ty::BOOL)?,
                };
                self.lines.push(format!("if ({}) {{", c.s));
                self.scoped(yes)?;
                if let Some(no) = no {
                    self.lines.push("} else {".into());
                    self.scoped(no)?;
                }
                self.lines.push("}".into());
            }
            Stmt::For(first, cond, step, body) => {
                // Variables live on after the loop in old HLSL.
                for s in first {
                    self.statement(s)?;
                }
                self.lines.push("loop {".into());
                if let Some(c) = cond {
                    let c = self.expr(c)?;
                    let c = self.convert(c, &Ty::BOOL)?;
                    self.lines.push(format!("if (!({})) {{ break; }}", c.s));
                }
                self.scoped(body)?;
                self.lines.push("continuing {".into());
                for e in step {
                    self.statement(&Stmt::Expr(e.clone()))?;
                }
                self.lines.push("} }".into());
            }
            Stmt::While(c, body) => {
                self.lines.push("loop {".into());
                let c = self.expr(c)?;
                let c = self.convert(c, &Ty::BOOL)?;
                self.lines.push(format!("if (!({})) {{ break; }}", c.s));
                self.scoped(body)?;
                self.lines.push("}".into());
            }
            Stmt::DoWhile(body, c) => {
                self.lines.push("loop {".into());
                self.scoped(body)?;
                let c = self.expr(c)?;
                let c = self.convert(c, &Ty::BOOL)?;
                self.lines.push(format!("if (!({})) {{ break; }} }}", c.s));
            }
            Stmt::Return(v) => {
                let value = match v {
                    Some(v) => {
                        let v = self.expr(v)?;
                        let ty = self.result.clone();
                        Some(self.convert(v, &ty)?)
                    }
                    None => None,
                };
                self.leave();
                self.lines.push(match value {
                    Some(v) => format!("return {};", v.s),
                    None => "return;".into(),
                });
            }
            Stmt::Break => self.lines.push("break;".into()),
            Stmt::Continue => self.lines.push("continue;".into()),
            Stmt::Discard => self.lines.push("discard;".into()),
            Stmt::Block(_) => {
                self.lines.push("{".into());
                self.scoped(s)?;
                self.lines.push("}".into());
            }
        }
        Ok(())
    }
}

fn splat(v: &str, n: u32) -> String {
    match n {
        1 => v.into(),
        n => format!("vec{n}<f32>({v})"),
    }
}

/// Numbers of a constant expression in order.
fn flatten(e: &Expr, out: &mut Vec<f32>) {
    match e {
        Expr::Float(v) => out.push(*v as f32),
        Expr::Int(v) => out.push(*v as f32),
        Expr::Bool(v) => out.push(f32::from(u8::from(*v))),
        Expr::List(v) | Expr::Make(_, v) => v.iter().for_each(|e| flatten(e, out)),
        Expr::Cast(_, e) => flatten(e, out),
        _ => {}
    }
}

/// Leaves of an entry point's parameters or result: access path, type and semantic.
struct Leaf {
    path: String,
    ty: Ty,
    semantic: (String, u32, bool),
}

fn leaves(p: &Program, f: &Field, path: &str, out: &mut Vec<Leaf>) -> Result<()> {
    match &f.ty {
        Ty::Struct(s) => {
            for x in &p.structs[s] {
                leaves(p, x, &format!("{path}.m_{}", x.name), out)?;
            }
        }
        Ty::Array(t, n) => {
            let (name, first, centroid) =
                semantic(f.semantic.as_deref().ok_or("array without semantic")?);
            for i in 0..*n {
                out.push(Leaf {
                    path: format!("{path}[{i}]"),
                    ty: (**t).clone(),
                    semantic: (name.clone(), first + i, centroid),
                });
            }
        }
        Ty::Mat(r, c) => {
            let (name, first, centroid) =
                semantic(f.semantic.as_deref().ok_or("matrix without semantic")?);
            for i in 0..*r {
                out.push(Leaf {
                    path: format!("{path}[{i}]"),
                    ty: Ty::float(*c),
                    semantic: (name.clone(), first + i, centroid),
                });
            }
        }
        ty => {
            let Some(s) = &f.semantic else {
                return Err(format!("{} has no semantic", f.name));
            };
            out.push(Leaf {
                path: path.into(),
                ty: ty.clone(),
                semantic: semantic(s),
            });
        }
    }
    Ok(())
}

/// Widens a value to the four floats of a register.
fn widen(s: &str, ty: &Ty) -> Result<String> {
    Ok(match ty {
        Ty::Vec(Scalar::Float, 4) => s.into(),
        Ty::Vec(Scalar::Float, 3) => format!("vec4<f32>({s}, 1.0)"),
        Ty::Vec(Scalar::Float, 2) => format!("vec4<f32>({s}, 0.0, 1.0)"),
        Ty::Vec(Scalar::Float, 1) => format!("vec4<f32>({s}, 0.0, 0.0, 1.0)"),
        Ty::Vec(_, n) => widen(&format!("{}({s})", name_of(&Ty::float(*n))), &Ty::float(*n))?,
        t => return unsupported(format!("output {t:?}")),
    })
}

/// What a pass needs to know about where it is used.
pub struct Setting<'a> {
    /// The stored type of a vertex semantic.
    pub element: &'a dyn Fn(&str, u32) -> Option<ElementType>,
    /// Textures that cannot be filtered.
    pub unfiltered: &'a dyn Fn(&str) -> bool,
    pub sampler: &'a dyn Fn(&str) -> Option<Sampler>,
    /// Writes depth instead of color.
    pub depth_only: bool,
}

/// Interpolators are numbered by semantic.
fn slot(semantic: &(String, u32, bool)) -> Option<u32> {
    match semantic.0.as_str() {
        "texcoord" => Some(semantic.1),
        "color" => Some(8 + semantic.1),
        _ => None,
    }
}

fn main_of(p: &Program) -> Result<&Function> {
    p.functions
        .iter()
        .rev()
        .find(|f| f.name == "main")
        .ok_or_else(|| "no main function".into())
}

fn declare_structs(s: &mut String, e: &Emitter) -> Result<()> {
    // Inner structs first.
    let mut done: BTreeSet<&str> = BTreeSet::new();
    fn one<'p>(
        p: &'p Program,
        name: &'p str,
        s: &mut String,
        done: &mut BTreeSet<&'p str>,
        prefix: &str,
    ) {
        if !done.insert(name) {
            return;
        }
        for f in &p.structs[name] {
            if let Ty::Struct(n) = &f.ty {
                one(p, n, s, done, prefix);
            }
        }
        let _ = writeln!(s, "struct S_{name} {{");
        for f in &p.structs[name] {
            let _ = writeln!(s, "  m_{}: {},", f.name, name_of(&f.ty));
        }
        if p.structs[name].is_empty() {
            s.push_str("  none: f32,\n");
        }
        s.push_str("};\n");
    }
    for n in &e.structs {
        one(e.program, n, s, &mut done, e.prefix());
    }
    Ok(())
}

/// Builds the module for `pass`.
pub fn translate(pass: &Pass, setting: &Setting) -> Result<Module> {
    let vs = hlsl::parse(pass.vs.as_deref().ok_or("no vertex shader")?)
        .map_err(|e| format!("vertex: {e}"))?;
    let ps = hlsl::parse(pass.ps.as_deref().ok_or("no pixel shader")?)
        .map_err(|e| format!("pixel: {e}"))?;
    let bias = |t: &str| {
        let s = pass
            .samplers
            .get(t)
            .cloned()
            .or_else(|| (setting.sampler)(t));
        s.map_or(0.0, |s| s.lod_bias)
    };
    let mut ve = Emitter::new(&vs, Stage::Vertex, &bias);
    let mut pe = Emitter::new(&ps, Stage::Pixel, &bias);
    let (vm, pm) = (main_of(&vs)?, main_of(&ps)?);
    ve.scopes.push(HashMap::new());
    pe.scopes.push(HashMap::new());
    ve.function("vsmain_", vm, &[])
        .map_err(|e| format!("vertex: {e}"))?;
    let ps_samplers: Vec<String> = pm
        .params
        .iter()
        .filter(|p| p.ty == Ty::Sampler)
        .map(|p| p.name.clone())
        .collect();
    pe.function("psmain_", pm, &ps_samplers)
        .map_err(|e| format!("pixel: {e}"))?;

    let mut inputs = Vec::new();
    for p in &vm.params {
        leaves(&vs, p, &format!("a_{}", p.name), &mut inputs)?;
    }
    let result = |f: &Function| Field {
        ty: f.result.clone(),
        name: "o".into(),
        semantic: f.semantic.clone(),
        direction: Direction::Out,
        default: None,
    };
    let mut outputs = Vec::new();
    leaves(&vs, &result(vm), "o", &mut outputs)?;
    let mut taken = Vec::new();
    for p in pm.params.iter().filter(|p| p.ty != Ty::Sampler) {
        leaves(&ps, p, &format!("a_{}", p.name), &mut taken)?;
    }
    let mut shown = Vec::new();
    leaves(&ps, &result(pm), "o", &mut shown)?;

    let mut m = Module {
        vs: ve.uniforms.clone(),
        ps: pe.uniforms.clone(),
        vs_consts: ve.registers.max(1),
        ps_consts: pe.registers.max(1),
        ..Module::default()
    };
    let used: BTreeSet<&(String, bool)> = ve.samplers.iter().chain(&pe.samplers).collect();
    for (name, cube) in used {
        if m.samplers.iter().any(|s| s.0 == *name) {
            continue;
        }
        m.samplers
            .push((name.clone(), *cube, (setting.unfiltered)(name)));
    }
    if ve.samplers.iter().any(|s| pe.samplers.contains(s)) {
        return unsupported("texture in both programs");
    }

    let mut s = String::from("diagnostic(off, derivative_uniformity);\n");
    s.push_str(HELPERS);
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
    s.push_str("@group(0) @binding(0) var<uniform> vc: VsConst;\n@group(0) @binding(1) var<uniform> pc: PsConst;\n");
    for (k, (name, cube, _)) in m.samplers.iter().enumerate() {
        let ty = if *cube {
            "texture_cube<f32>"
        } else {
            "texture_2d<f32>"
        };
        let _ = writeln!(s, "@group(1) @binding({}) var t_{name}: {ty};", k * 2);
        let _ = writeln!(
            s,
            "@group(1) @binding({}) var s_{name}: sampler;",
            k * 2 + 1
        );
    }
    // The programs' structs may share names.
    let mut vtext = String::new();
    declare_structs(&mut vtext, &ve)?;
    ve.functions.iter().for_each(|f| vtext.push_str(f));
    let mut ptext = String::new();
    declare_structs(&mut ptext, &pe)?;
    pe.functions.iter().for_each(|f| ptext.push_str(f));
    s.push_str(&vtext.replace("S_", "Sv_"));
    s.push_str(&ptext.replace("S_", "Sp_"));

    // Interpolators: what either side mentions.
    let mut slots: BTreeMap<u32, bool> = BTreeMap::new();
    for l in outputs.iter().chain(&taken) {
        if let Some(k) = slot(&l.semantic) {
            *slots.entry(k).or_default() |= l.semantic.2;
        }
    }
    s.push_str("struct In {\n");
    let mut reads = Vec::new();
    for l in &inputs {
        let (name, index, _) = &l.semantic;
        let found = match m
            .inputs
            .iter()
            .position(|i| i.semantic == *name && i.index == *index)
        {
            Some(k) => Some(k),
            None => (setting.element)(name, *index).map(|ty| {
                m.inputs.push(Input {
                    semantic: name.clone(),
                    index: *index,
                    ty,
                });
                m.inputs.len() - 1
            }),
        };
        let wide = match found {
            None => Typed::new("vec4<f32>(0.0, 0.0, 0.0, 1.0)", Ty::float(4)),
            Some(k) => match m.inputs[k].ty {
                ElementType::Float1 => {
                    Typed::new(format!("vec4<f32>(i.a{k}, 0.0, 0.0, 1.0)"), Ty::float(4))
                }
                ElementType::Float2 => {
                    Typed::new(format!("vec4<f32>(i.a{k}, 0.0, 1.0)"), Ty::float(4))
                }
                ElementType::Float3 => Typed::new(format!("vec4<f32>(i.a{k}, 1.0)"), Ty::float(4)),
                ElementType::Float4 | ElementType::Short4n => {
                    Typed::new(format!("i.a{k}"), Ty::float(4))
                }
                ElementType::Color => Typed::new(format!("i.a{k}.bgra"), Ty::float(4)),
                ElementType::Short4 => Typed::new(format!("i.a{k}"), Ty::Vec(Scalar::Int, 4)),
                ElementType::Ubyte4 => {
                    Typed::new(format!("vec4<i32>(i.a{k})"), Ty::Vec(Scalar::Int, 4))
                }
            },
        };
        reads.push(ve.convert(wide, &l.ty)?.s);
    }
    for (k, i) in m.inputs.iter().enumerate() {
        let ty = match i.ty {
            ElementType::Float1 => "f32",
            ElementType::Float2 => "vec2<f32>",
            ElementType::Float3 => "vec3<f32>",
            ElementType::Float4 | ElementType::Color | ElementType::Short4n => "vec4<f32>",
            ElementType::Ubyte4 => "vec4<u32>",
            ElementType::Short4 => "vec4<i32>",
        };
        let _ = writeln!(s, "  @location({k}) a{k}: {ty},");
    }
    if m.inputs.is_empty() {
        return Err("no vertex inputs".into());
    }
    s.push_str("};\nstruct V2F {\n  @builtin(position) pos: vec4<f32>,\n");
    for (k, centroid) in &slots {
        // On the original hardware colors never extrapolate past the triangle.
        let c = if *centroid || *k >= 8 {
            "@interpolate(perspective, centroid) "
        } else {
            ""
        };
        let _ = writeln!(s, "  @location({k}) {c}t{k}: vec4<f32>,");
    }
    s.push_str("};\n@vertex\nfn vs(i: In) -> V2F {\n  var v: V2F;\n");
    let statics = |s: &mut String, e: &Emitter| {
        for (w, _) in e.statics.values() {
            let _ = writeln!(s, "  {w} = {w}_();");
        }
    };
    statics(&mut s, &ve);
    for p in &vm.params {
        let _ = writeln!(
            s,
            "  var a_{}: {};",
            p.name,
            name_of(&p.ty).replace("S_", "Sv_")
        );
    }
    for (l, r) in inputs.iter().zip(&reads) {
        let _ = writeln!(s, "  {} = {r};", l.path);
    }
    let args: Vec<String> = vm.params.iter().map(|p| format!("a_{}", p.name)).collect();
    let _ = writeln!(s, "  let o = vsmain_({});", args.join(", "));
    for k in slots.keys() {
        let _ = writeln!(s, "  v.t{k} = vec4<f32>(0.0, 0.0, 0.0, 1.0);");
    }
    for l in &outputs {
        let wide = widen(&l.path, &l.ty)?;
        match slot(&l.semantic) {
            Some(k) if k >= 8 => {
                // Color interpolators are clamped.
                let _ = writeln!(s, "  v.t{k} = clamp({wide}, vec4<f32>(), vec4<f32>(1.0));");
            }
            Some(k) => drop(writeln!(s, "  v.t{k} = {wide};")),
            None if l.semantic.0 == "position" => drop(writeln!(s, "  v.pos = {wide};")),
            None => {}
        }
    }
    s.push_str("  return v;\n}\n");
    m.outputs = shown
        .iter()
        .filter(|l| l.semantic.0 == "color")
        .map(|l| l.semantic.1 + 1)
        .max()
        .unwrap_or(1);
    if setting.depth_only {
        m.outputs = 1;
    }
    s.push_str("struct Shown {\n");
    for n in 0..m.outputs {
        let _ = writeln!(s, "  @location({n}) c{n}: vec4<f32>,");
    }
    s.push_str("};\n@fragment\nfn fs(v: V2F) -> Shown {\n  var c: Shown;\n");
    statics(&mut s, &pe);
    for p in pm.params.iter().filter(|p| p.ty != Ty::Sampler) {
        let _ = writeln!(
            s,
            "  var a_{}: {};",
            p.name,
            name_of(&p.ty).replace("S_", "Sp_")
        );
    }
    for l in &taken {
        let from = match slot(&l.semantic) {
            Some(k) => format!("v.t{k}"),
            None if l.semantic.0 == "vpos" => "v.pos".into(),
            None => continue,
        };
        let v = pe.convert(Typed::new(from, Ty::float(4)), &l.ty)?;
        let _ = writeln!(s, "  {} = {};", l.path, v.s);
    }
    let args: Vec<String> = pm
        .params
        .iter()
        .filter(|p| p.ty != Ty::Sampler)
        .map(|p| format!("a_{}", p.name))
        .collect();
    let _ = writeln!(s, "  let o = psmain_({});", args.join(", "));
    for l in &shown {
        if l.semantic.0 != "color" || l.semantic.1 >= m.outputs {
            continue;
        }
        let wide = widen(&l.path, &l.ty)?;
        // Buffers of 16 bits per channel hold 0 to 1 as well, but are floats here.
        let _ = writeln!(
            s,
            "  c.c{} = clamp({wide}, vec4<f32>(), vec4<f32>(1.0));",
            l.semantic.1
        );
    }
    if let Some((test, value)) = pass.alpha_test {
        let op = match test {
            Compare::Less => "<",
            Compare::LessEqual => "<=",
            Compare::Greater => ">",
            Compare::GreaterEqual => ">=",
            Compare::Equal => "==",
            Compare::NotEqual => "!=",
            Compare::Always => "",
            Compare::Never => return Err("alpha test Never".into()),
        };
        if !op.is_empty() {
            let _ = writeln!(s, "  if (!(c.c0.a {op} {value:?})) {{ discard; }}");
        }
    }
    if setting.depth_only {
        s.push_str("  c.c0 = vec4<f32>(v.pos.z);\n");
    }
    s.push_str("  return c;\n}\n");
    m.source = s;
    Ok(m)
}
