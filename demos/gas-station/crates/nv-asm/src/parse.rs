//! Reads NV_vertex_program 1.1/2.0 (`!!VP1.1`, `!!VP2.0`) and NV_fragment_program 1.0
//! (`!!FP1.0`) listings, as the Cg 1.0/1.1 compilers write them for the vp20, vp30 and fp30
//! profiles and as the demos' authors wrote some by hand (comments, bare scalar literals).
//!
//! Cg's `#var` and `#const` comment lines are kept: they tell the runtime which constant
//! registers and texture units each named parameter uses.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

/// Storage precision of a register, and arithmetic precision of an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// 32-bit float.
    R,
    /// 16-bit float.
    H,
    /// 12-bit fixed point, -2 to 2 in steps of 1/1024.
    X,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reg {
    /// Vertex program temporary `R0`–`R15`, or fragment `R0`–`R31` / `H0`–`H63`.
    Temp(Precision, u32),
    /// Vertex attribute `v[n]`.
    Attr(u32),
    /// Vertex program constant `c[n]`.
    Const(u32),
    /// `c[A<n>.<comp> + offset]`.
    ConstRel(u8, u8, i32),
    /// Address register `A0` or `A1` (vertex program 2).
    Addr(u8),
    /// `o[NAME]`.
    Out(String),
    /// Fragment input `f[NAME]`.
    In(String),
    /// A `DECLARE`d or `DEFINE`d name.
    Named(String),
    Literal([f32; 4]),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Src {
    pub reg: Reg,
    pub neg: bool,
    pub abs: bool,
    pub swizzle: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Tr,
    Fl,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dst {
    pub reg: Reg,
    pub mask: [bool; 4],
    pub cond: Option<(Cond, [u8; 4])>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    T1D,
    T2D,
    T3D,
    Cube,
    Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inst {
    /// Opcode without precision, `C` or `_SAT`.
    pub op: String,
    /// The precision letter, for fragment programs; `None` means R (32-bit).
    pub precision: Option<Precision>,
    pub set_cc: bool,
    pub sat: bool,
    pub dst: Option<Dst>,
    pub srcs: Vec<Src>,
    /// Texture image unit and target of TEX, TXP and TXD.
    pub texture: Option<(u32, Target)>,
    /// BRA: label and condition.
    pub branch: Option<(String, Cond, [u8; 4])>,
    pub line: usize,
}

/// A `#var` line: `#var float4x4 modelViewProj :  : c[5], 4 : 3 : 1`.
#[derive(Debug, Clone, PartialEq)]
pub struct Var {
    pub ty: String,
    pub name: String,
    /// `$vin.ATTR3`, `$vout.TEX0` or empty.
    pub binding: String,
    /// `c[5], 4`, `texunit 0`, `ATTR3`, `TEX0` or empty.
    pub resource: String,
    pub index: i32,
    pub used: bool,
}

impl Var {
    /// The first constant register and count of a uniform in a vertex program.
    pub fn constants(&self) -> Option<(u32, u32)> {
        let r = self.resource.trim();
        let rest = r.strip_prefix("c[")?;
        let (first, after) = rest.split_once(']')?;
        let n = after.trim_start_matches(',').trim();
        Some((first.trim().parse().ok()?, if n.is_empty() { 1 } else { n.parse().ok()? }))
    }

    pub fn texunit(&self) -> Option<u32> {
        self.resource.trim().strip_prefix("texunit ")?.trim().parse().ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub stage: Stage,
    pub header: String,
    pub insts: Vec<Inst>,
    /// Label name to the index of the instruction after it.
    pub labels: HashMap<String, usize>,
    /// `DECLARE` names in order, with defaults.
    pub declares: Vec<(String, Option<[f32; 4]>)>,
    pub defines: HashMap<String, [f32; 4]>,
    pub vars: Vec<Var>,
    /// `#const c[9] = 0.001953125 0 0 0`.
    pub consts: Vec<(u32, [f32; 4])>,
    /// fp20 programs (texture shaders and register combiners) keep their own form.
    pub fp20: Option<crate::fp20::Program>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { line, message: message.into() })
}

/// Opcodes by stage; the longest match wins, then precision, `C` and `_SAT` follow.
const VP_OPS: &[&str] = &[
    "ARL", "ARR", "ARA", "MOV", "MUL", "ADD", "MAD", "RCP", "RSQ", "DP3", "DP4", "DPH", "DST",
    "MIN", "MAX", "SLT", "SGE", "SGT", "SLE", "SEQ", "SNE", "STR", "SFL", "EXP", "LOG", "LIT",
    "RCC", "SSG", "FLR", "FRC", "EX2", "LG2", "SIN", "COS", "ABS", "BRA", "CAL", "RET", "END",
];
const FP_OPS: &[&str] = &[
    "ADD", "COS", "DDX", "DDY", "DP3", "DP4", "DST", "EX2", "FLR", "FRC", "KIL", "LG2", "LIT",
    "LRP", "MAD", "MAX", "MIN", "MOV", "MUL", "PK2H", "PK2US", "PK4B", "PK4UB", "POW", "RCP",
    "RFL", "RSQ", "SEQ", "SFL", "SGE", "SGT", "SIN", "SLE", "SLT", "SNE", "STR", "TEX", "TXD",
    "TXP", "UP2H", "UP2US", "UP4B", "UP4UB", "X2D", "END",
];

fn swizzle_of(s: &str, line: usize) -> Result<[u8; 4], ParseError> {
    let mut out = [0u8; 4];
    let cs: Vec<u8> = s
        .chars()
        .map(|c| match c {
            'x' => Ok(0),
            'y' => Ok(1),
            'z' => Ok(2),
            'w' => Ok(3),
            _ => Err(()),
        })
        .collect::<Result<_, _>>()
        .or_else(|_| err(line, format!("bad swizzle .{s}")))?;
    match cs.len() {
        1 => out = [cs[0]; 4],
        // Shorter swizzles repeat their last component.
        2..=4 => {
            for (i, o) in out.iter_mut().enumerate() {
                *o = cs[i.min(cs.len() - 1)];
            }
        }
        _ => return err(line, format!("bad swizzle .{s}")),
    }
    Ok(out)
}

fn mask_of(s: &str, line: usize) -> Result<[bool; 4], ParseError> {
    let mut m = [false; 4];
    let mut last = -1i32;
    for c in s.chars() {
        let i = match c {
            'x' => 0,
            'y' => 1,
            'z' => 2,
            'w' => 3,
            _ => return err(line, format!("bad write mask .{s}")),
        };
        if i <= last {
            return err(line, format!("bad write mask .{s}"));
        }
        last = i;
        m[i as usize] = true;
    }
    Ok(m)
}

fn cond_of(s: &str, line: usize) -> Result<Cond, ParseError> {
    Ok(match s {
        "EQ" => Cond::Eq,
        "NE" => Cond::Ne,
        "LT" => Cond::Lt,
        "GT" => Cond::Gt,
        "LE" => Cond::Le,
        "GE" => Cond::Ge,
        "TR" => Cond::Tr,
        "FL" => Cond::Fl,
        _ => return err(line, format!("bad condition {s}")),
    })
}

/// `(GT.x)` or `GT.xy` without the parentheses.
fn cond_swizzle(s: &str, line: usize) -> Result<(Cond, [u8; 4]), ParseError> {
    let (c, sw) = match s.split_once('.') {
        Some((c, sw)) => (c, swizzle_of(sw, line)?),
        None => (s, [0, 1, 2, 3]),
    };
    Ok((cond_of(c.trim(), line)?, sw))
}

fn number(s: &str, line: usize) -> Result<f32, ParseError> {
    s.trim().parse::<f32>().or_else(|_| err(line, format!("bad number {s}")))
}

/// `{1, 2, 3, 4}`, or a bare scalar, which stands for all four components. Braces with
/// fewer values leave y and z 0 and w 1, as in the ARB and NV program grammars.
fn literal(s: &str, line: usize) -> Result<[f32; 4], ParseError> {
    let s = s.trim();
    let Some(inner) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) else {
        return Ok([number(s, line)?; 4]);
    };
    let vals: Vec<f32> = inner.split(',').map(|v| number(v, line)).collect::<Result<_, _>>()?;
    Ok(match vals.len() {
        1 => [vals[0], 0.0, 0.0, 1.0],
        2 => [vals[0], vals[1], 0.0, 1.0],
        3 => [vals[0], vals[1], vals[2], 1.0],
        4 => [vals[0], vals[1], vals[2], vals[3]],
        _ => return err(line, format!("bad vector {s}")),
    })
}

/// Splits operands at top-level commas (not those inside `{}` or `[]`).
fn split_operands(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Splits `R0.xyz` / `c[A0.x + 1].xy` / `{1,2}.x` into the register and the suffix after
/// its last top-level dot.
fn split_suffix(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0;
    let mut dot = None;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            '.' if depth == 0 => dot = Some(i),
            _ => {}
        }
    }
    match dot {
        // A number such as `0.5` is not a register with a suffix.
        Some(i) if !s[..i].chars().all(|c| c.is_ascii_digit() || c == '-' || c == '+') => {
            (&s[..i], Some(&s[i + 1..]))
        }
        _ => (s, None),
    }
}

fn register(s: &str, stage: Stage, line: usize) -> Result<Reg, ParseError> {
    let s = s.trim();
    if s.starts_with('{') {
        return Ok(Reg::Literal(literal(s, line)?));
    }
    if let Ok(v) = s.parse::<f32>() {
        return Ok(Reg::Literal([v; 4]));
    }
    if let Some(rest) = s.strip_prefix("v[") {
        let n = rest.trim_end_matches(']');
        // Attributes may be named (v[OPOS]); Cg writes numbers.
        let idx = match n {
            "OPOS" => 0,
            "WGHT" => 1,
            "NRML" => 2,
            "COL0" => 3,
            "COL1" => 4,
            "FOGC" => 5,
            "TEX0" => 8,
            "TEX1" => 9,
            "TEX2" => 10,
            "TEX3" => 11,
            "TEX4" => 12,
            "TEX5" => 13,
            "TEX6" => 14,
            "TEX7" => 15,
            _ => n.parse().or_else(|_| err(line, format!("bad attribute {s}")))?,
        };
        return Ok(Reg::Attr(idx));
    }
    if let Some(rest) = s.strip_prefix("c[") {
        let inner = rest.trim_end_matches(']').trim();
        let areg = if inner.starts_with("A0.") { Some(0) } else if inner.starts_with("A1.") { Some(1) } else { None };
        if let Some(areg) = areg {
            let a = &inner[3..];
            let comp = a.chars().next().ok_or_else(|| ParseError { line, message: s.into() })?;
            let comp = swizzle_of(&comp.to_string(), line)?[0];
            let after = a[1..].trim();
            let off = if after.is_empty() {
                0
            } else if let Some(p) = after.strip_prefix('+') {
                p.trim().parse().or_else(|_| err(line, format!("bad offset {s}")))?
            } else if let Some(m) = after.strip_prefix('-') {
                -m.trim().parse::<i32>().or_else(|_| err(line, format!("bad offset {s}")))?
            } else {
                return err(line, format!("bad relative constant {s}"));
            };
            return Ok(Reg::ConstRel(areg, comp, off));
        }
        return Ok(Reg::Const(inner.parse().or_else(|_| err(line, format!("bad constant {s}")))?));
    }
    if let Some(rest) = s.strip_prefix("o[") {
        return Ok(Reg::Out(rest.trim_end_matches(']').to_string()));
    }
    if let Some(rest) = s.strip_prefix("f[") {
        return Ok(Reg::In(rest.trim_end_matches(']').to_string()));
    }
    if s == "A0" || s == "A1" {
        return Ok(Reg::Addr(s.as_bytes()[1] - b'0'));
    }
    let digits = |p: &str| -> Option<u32> {
        let n = s.strip_prefix(p)?;
        (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then(|| n.parse().ok())?
    };
    if let Some(n) = digits("R") {
        return Ok(Reg::Temp(Precision::R, n));
    }
    if stage == Stage::Fragment {
        if let Some(n) = digits("H") {
            return Ok(Reg::Temp(Precision::H, n));
        }
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Ok(Reg::Named(s.to_string()));
    }
    err(line, format!("bad register {s}"))
}

fn source(s: &str, stage: Stage, line: usize) -> Result<Src, ParseError> {
    let mut t = s.trim();
    let mut neg = false;
    if let Some(r) = t.strip_prefix('-') {
        // `-0.5` is a literal, `-R0` a negated register.
        if r.trim_start().parse::<f32>().is_err() {
            neg = true;
            t = r.trim_start();
        }
    } else if let Some(r) = t.strip_prefix('+') {
        t = r.trim_start();
    }
    let mut abs = false;
    if t.starts_with('|') {
        let end = t[1..].find('|').ok_or_else(|| ParseError { line, message: s.into() })? + 1;
        if t[end + 1..].trim().is_empty() {
            abs = true;
            t = &t[1..end];
        } else {
            return err(line, format!("bad absolute value {s}"));
        }
    }
    let (r, suffix) = split_suffix(t);
    let swizzle = match suffix {
        Some(sw) => swizzle_of(sw, line)?,
        None => [0, 1, 2, 3],
    };
    Ok(Src { reg: register(r, stage, line)?, neg, abs, swizzle })
}

fn destination(s: &str, stage: Stage, line: usize) -> Result<Dst, ParseError> {
    let s = s.trim();
    let (body, cond) = match s.find('(') {
        Some(i) => {
            let c = s[i + 1..].trim_end().trim_end_matches(')');
            (s[..i].trim(), Some(cond_swizzle(c.trim(), line)?))
        }
        None => (s, None),
    };
    let (r, suffix) = split_suffix(body);
    let mask = match suffix {
        Some(m) => mask_of(m, line)?,
        None => [true; 4],
    };
    Ok(Dst { reg: register(r, stage, line)?, mask, cond })
}

fn target_of(s: &str, line: usize) -> Result<Target, ParseError> {
    Ok(match s.trim() {
        "1D" => Target::T1D,
        "2D" => Target::T2D,
        "3D" => Target::T3D,
        "CUBE" => Target::Cube,
        "RECT" => Target::Rect,
        _ => return err(line, format!("bad texture target {s}")),
    })
}

fn opcode(word: &str, stage: Stage, line: usize) -> Result<(String, Option<Precision>, bool, bool), ParseError> {
    let ops = if stage == Stage::Vertex { VP_OPS } else { FP_OPS };
    let base = ops
        .iter()
        .filter(|o| word.starts_with(**o))
        .max_by_key(|o| o.len())
        .ok_or_else(|| ParseError { line, message: format!("unknown opcode {word}") })?;
    let mut rest = &word[base.len()..];
    let mut precision = None;
    if stage == Stage::Fragment {
        if let Some(c) = rest.chars().next() {
            let p = match c {
                'R' => Some(Precision::R),
                'H' => Some(Precision::H),
                'X' => Some(Precision::X),
                _ => None,
            };
            if p.is_some() {
                precision = p;
                rest = &rest[1..];
            }
        }
    }
    let mut set_cc = false;
    if let Some(r) = rest.strip_prefix('C') {
        set_cc = true;
        rest = r;
    }
    let mut sat = false;
    if let Some(r) = rest.strip_prefix("_SAT") {
        sat = true;
        rest = r;
    }
    if !rest.is_empty() {
        return err(line, format!("unknown opcode {word}"));
    }
    Ok((base.to_string(), precision, set_cc, sat))
}

fn var_line(rest: &str) -> Option<Var> {
    // `float4x4 modelViewProj :  : c[5], 4 : 3 : 1`
    let parts: Vec<&str> = rest.split(" : ").collect();
    if parts.len() < 5 {
        return None;
    }
    let head = parts[0].trim();
    let (ty, name) = head.split_once(' ')?;
    Some(Var {
        ty: ty.to_string(),
        name: name.trim().to_string(),
        binding: parts[1].trim().to_string(),
        resource: parts[2].trim().to_string(),
        index: parts[3].trim().parse().ok()?,
        used: parts[4].trim() == "1",
    })
}

pub fn parse(text: &str) -> Result<Program, ParseError> {
    let text = text.replace('\r', "");
    if text.contains("!!RC1.0") {
        let f = crate::fp20::parse(&text)?;
        // Combiner constants are the named parameters, in their order.
        let declares = f
            .vars
            .iter()
            .filter(|v| v.resource.starts_with("COMBINER"))
            .map(|v| (v.name.clone(), None))
            .collect();
        return Ok(Program {
            stage: Stage::Fragment,
            header: "!!RC1.0".into(),
            insts: vec![],
            labels: HashMap::new(),
            declares,
            defines: HashMap::new(),
            vars: f.vars.clone(),
            consts: vec![],
            fp20: Some(f),
        });
    }
    let mut lines = text.split('\n').enumerate().map(|(i, l)| (i + 1, l));
    let (stage, header) = loop {
        let Some((n, l)) = lines.next() else { return err(0, "empty program") };
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        let h = l.split_whitespace().next().unwrap_or("");
        break match h {
            "!!VP1.0" | "!!VP1.1" | "!!VP2.0" => (Stage::Vertex, h.to_string()),
            "!!FP1.0" => (Stage::Fragment, h.to_string()),
            _ => return err(n, format!("unknown program header {h}")),
        };
    };
    let mut p = Program {
        stage,
        header,
        insts: vec![],
        labels: HashMap::new(),
        declares: vec![],
        defines: HashMap::new(),
        vars: vec![],
        consts: vec![],
        fp20: None,
    };
    // Statements end at `;`; comments run from `#` to the end of the line.
    let mut stmt = String::new();
    let mut stmt_line = 0;
    let mut ended = false;
    for (n, raw) in lines {
        let (code, comment) = match raw.find('#') {
            Some(i) => (&raw[..i], Some(&raw[i + 1..])),
            None => (raw, None),
        };
        if let Some(c) = comment {
            if code.trim().is_empty() {
                if let Some(v) = c.strip_prefix("var ") {
                    if let Some(v) = var_line(v) {
                        p.vars.push(v);
                    }
                } else if let Some(c) = c.strip_prefix("const ") {
                    // `c[9] = 0.001953125 0 0 0`
                    if let Some((r, vals)) = c.split_once('=') {
                        let idx = r.trim().trim_start_matches("c[").trim_end_matches(']');
                        let vals: Vec<f32> =
                            vals.split_whitespace().filter_map(|v| v.parse().ok()).collect();
                        if let (Ok(i), 4) = (idx.parse(), vals.len()) {
                            p.consts.push((i, [vals[0], vals[1], vals[2], vals[3]]));
                        }
                    }
                }
            }
        }
        if ended {
            continue;
        }
        let mut rest = code;
        loop {
            // Labels: `b0:` at the start of a statement.
            if stmt.trim().is_empty() {
                let t = rest.trim_start();
                if let Some(i) = t.find(':') {
                    let name = &t[..i];
                    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        p.labels.insert(name.to_string(), p.insts.len());
                        rest = &t[i + 1..];
                        continue;
                    }
                }
            }
            match rest.find(';') {
                Some(i) => {
                    if stmt.trim().is_empty() {
                        stmt_line = n;
                    }
                    stmt.push_str(&rest[..i]);
                    let s = std::mem::take(&mut stmt);
                    if statement(&mut p, s.trim(), stmt_line)? {
                        ended = true;
                        break;
                    }
                    rest = &rest[i + 1..];
                }
                None => {
                    if !rest.trim().is_empty() {
                        if stmt.trim().is_empty() {
                            stmt_line = n;
                        }
                        stmt.push_str(rest);
                        stmt.push(' ');
                    }
                    break;
                }
            }
        }
        // `END` needs no semicolon.
        if !ended && stmt.trim() == "END" {
            ended = true;
        }
    }
    if !ended {
        return err(0, "no END");
    }
    Ok(p)
}

/// One statement; true at END.
fn statement(p: &mut Program, s: &str, line: usize) -> Result<bool, ParseError> {
    if s.is_empty() {
        return Ok(false);
    }
    let (word, ops) = match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    };
    if word == "END" {
        return Ok(true);
    }
    if word == "DECLARE" || word == "DEFINE" {
        let (name, value) = match ops.split_once('=') {
            Some((n, v)) => (n.trim(), Some(literal(v.trim(), line)?)),
            None => (ops.trim(), None),
        };
        if word == "DEFINE" {
            let v = value.ok_or_else(|| ParseError { line, message: "DEFINE without value".into() })?;
            p.defines.insert(name.to_string(), v);
        } else {
            p.declares.push((name.to_string(), value));
        }
        return Ok(false);
    }
    let (op, precision, set_cc, sat) = opcode(word, p.stage, line)?;
    let mut inst = Inst {
        op,
        precision,
        set_cc,
        sat,
        dst: None,
        srcs: vec![],
        texture: None,
        branch: None,
        line,
    };
    let operands = split_operands(ops);
    match inst.op.as_str() {
        "BRA" => {
            // `BRA b2 (EQ.x)` or `BRA b3`.
            let (label, cond) = match ops.find('(') {
                Some(i) => {
                    let c = ops[i + 1..].trim().trim_end_matches(')');
                    (ops[..i].trim(), cond_swizzle(c.trim(), line)?)
                }
                None => (ops.trim(), (Cond::Tr, [0, 1, 2, 3])),
            };
            inst.branch = Some((label.to_string(), cond.0, cond.1));
        }
        "KIL" => {
            let c = ops.trim().trim_start_matches('(').trim_end_matches(')');
            inst.branch = Some((String::new(), cond_swizzle(c, line)?.0, cond_swizzle(c, line)?.1));
        }
        "TEX" | "TXP" | "TXD" => {
            let n = if inst.op == "TXD" { 6 } else { 4 };
            if operands.len() != n {
                return err(line, format!("{} needs {n} operands", inst.op));
            }
            inst.dst = Some(destination(&operands[0], p.stage, line)?);
            for o in &operands[1..n - 2] {
                inst.srcs.push(source(o, p.stage, line)?);
            }
            let unit = operands[n - 2]
                .strip_prefix("TEX")
                .and_then(|u| u.parse().ok())
                .ok_or_else(|| ParseError { line, message: format!("bad texture unit {}", operands[n - 2]) })?;
            inst.texture = Some((unit, target_of(&operands[n - 1], line)?));
        }
        _ => {
            if operands.is_empty() {
                return err(line, format!("{} without operands", inst.op));
            }
            inst.dst = Some(destination(&operands[0], p.stage, line)?);
            for o in &operands[1..] {
                inst.srcs.push(source(o, p.stage, line)?);
            }
            let want = match inst.op.as_str() {
                "MAD" | "LRP" | "X2D" => 3,
                "ADD" | "MUL" | "DP3" | "DP4" | "DPH" | "DST" | "MIN" | "MAX" | "SLT" | "SGE"
                | "SGT" | "SLE" | "SEQ" | "SNE" | "STR" | "SFL" | "POW" | "RFL" => 2,
                _ => 1,
            };
            if inst.srcs.len() != want {
                return err(line, format!("{} needs {want} sources", inst.op));
            }
        }
    }
    p.insts.push(inst);
    Ok(false)
}
