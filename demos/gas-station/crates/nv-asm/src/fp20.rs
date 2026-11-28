//! The fp20 profile's output: nvparse texture shader (`!!TS1.0`) and register combiner
//! (`!!RC1.0`) programs, as Cg 1.0/1.1 write them for GeForce 3/4 class hardware. The
//! viewers use them for their built-in programs (menus, wireframe).
//!
//! Supported: texture stages `texture_2d()`, `texture_rectangle()`, `texture_cube_map()`,
//! `texture_3d()`, `pass_through()`, `nop()`; general combiners with products, sums of
//! two products and dot products, input mappings (`unsigned`, `expand`, `half_bias`,
//! `unsigned_invert`, negation) and `scale_by_*()`/`bias_by_negative_one_half()`; the final
//! combiner's `out.rgb`/`out.a` from one register. Register combiners clamp to [-1, 1].

use crate::parse::{ParseError, Target, Var};

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    Nop,
    PassThrough,
    Texture(Target),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    pub reg: String,
    /// `rgb`, `a`, `b` (blue as alpha) or `rgba`.
    pub part: String,
    pub map: String,
    pub neg: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Product(Input, Input),
    SumOfProducts(Input, Input, Input, Input),
    Dot(Input, Input),
    Copy(Input),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assign {
    pub dst: String,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Combiner {
    pub rgb: Vec<Assign>,
    pub alpha: Vec<Assign>,
    pub scale: f32,
    pub bias: f32,
    pub consts: Vec<(usize, [f32; 4])>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub stages: Vec<Stage>,
    pub combiners: Vec<Combiner>,
    pub out_rgb: Option<Input>,
    pub out_a: Option<Input>,
    pub global_consts: Vec<(usize, [f32; 4])>,
    pub vars: Vec<Var>,
}

fn err<T>(m: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { line: 0, message: m.into() })
}

fn input(s: &str) -> Result<Input, ParseError> {
    let mut s = s.trim();
    let mut neg = false;
    if let Some(r) = s.strip_prefix('-') {
        neg = true;
        s = r.trim();
    }
    let mut map = "identity".to_string();
    for m in ["unsigned_invert", "unsigned", "expand", "half_bias"] {
        if let Some(r) = s.strip_prefix(m).and_then(|r| r.trim().strip_prefix('(')).and_then(|r| r.strip_suffix(')')) {
            map = m.to_string();
            s = r.trim();
            break;
        }
    }
    if let Some(r) = s.strip_prefix('-') {
        neg = !neg;
        s = r.trim();
    }
    let (reg, part) = match s.split_once('.') {
        Some((r, p)) => (r.to_string(), p.to_string()),
        None => (s.to_string(), "rgba".to_string()),
    };
    Ok(Input { reg, part, map, neg })
}

fn expr(s: &str) -> Result<Expr, ParseError> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('+') {
        let (p, q) = a.split_once('*').ok_or_else(|| ParseError { line: 0, message: s.into() })?;
        let (r, t) = b.split_once('*').ok_or_else(|| ParseError { line: 0, message: s.into() })?;
        return Ok(Expr::SumOfProducts(input(p)?, input(q)?, input(r)?, input(t)?));
    }
    if let Some((a, b)) = s.split_once('*') {
        return Ok(Expr::Product(input(a)?, input(b)?));
    }
    if let Some((a, b)) = s.split_once(" . ") {
        return Ok(Expr::Dot(input(a)?, input(b)?));
    }
    Ok(Expr::Copy(input(s)?))
}

fn constant(s: &str) -> Result<(usize, [f32; 4]), ParseError> {
    // `const0 = (1, 2, 3, 4);`
    let (n, v) = s.split_once('=').ok_or_else(|| ParseError { line: 0, message: s.into() })?;
    let idx = n.trim().trim_start_matches("const").parse().map_err(|_| ParseError { line: 0, message: s.into() })?;
    let vals: Vec<f32> = v
        .trim()
        .trim_matches(|c| c == '(' || c == ')' || c == ';')
        .split(',')
        .map(|x| x.trim().parse().unwrap_or(0.0))
        .collect();
    let mut a = [0.0; 4];
    for (i, x) in vals.iter().take(4).enumerate() {
        a[i] = *x;
    }
    Ok((idx, a))
}

pub fn parse(text: &str) -> Result<Program, ParseError> {
    let text = text.replace('\r', "");
    let mut p = Program::default();
    let mut code = String::new();
    for line in text.lines() {
        let (c, comment) = match line.find("//") {
            Some(i) => (&line[..i], Some(&line[i + 2..])),
            None => (line, None),
        };
        if let Some(v) = comment.and_then(|c| c.strip_prefix("var ")) {
            let parts: Vec<&str> = v.split(" : ").collect();
            if parts.len() >= 5 {
                if let Some((ty, name)) = parts[0].trim().split_once(' ') {
                    p.vars.push(Var {
                        ty: ty.into(),
                        name: name.trim().into(),
                        binding: parts[1].trim().into(),
                        resource: parts[2].trim().into(),
                        index: parts[3].trim().parse().unwrap_or(0),
                        used: parts[4].trim() == "1",
                    });
                }
            }
        }
        code.push_str(c);
        code.push('\n');
    }
    let ts = code.find("!!TS1.0");
    let rc = code.find("!!RC1.0").ok_or_else(|| ParseError { line: 0, message: "no !!RC1.0".into() })?;
    if let Some(t) = ts {
        let body = &code[t + 7..if rc > t { rc } else { code.len() }];
        for st in body.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let name = st.trim_end_matches("()").trim();
            p.stages.push(match name {
                "nop" => Stage::Nop,
                "pass_through" => Stage::PassThrough,
                "texture_1d" | "texture_2d" => Stage::Texture(Target::T2D),
                "texture_rectangle" => Stage::Texture(Target::Rect),
                "texture_cube_map" => Stage::Texture(Target::Cube),
                "texture_3d" => Stage::Texture(Target::T3D),
                other => return err(format!("unsupported texture shader stage {other}")),
            });
        }
    }
    // Register combiners: global constants, `{ ... }` general stages, then the final one.
    let body = &code[rc + 7..];
    let mut rest = body;
    loop {
        let t = rest.trim_start();
        if t.starts_with("const") {
            let end = t.find(';').ok_or_else(|| ParseError { line: 0, message: "const".into() })?;
            p.global_consts.push(constant(&t[..end])?);
            rest = &t[end + 1..];
            continue;
        }
        if t.starts_with('{') {
            // A stage: balanced braces.
            let mut depth = 0;
            let mut end = 0;
            for (i, c) in t.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let stage = &t[1..end];
            p.combiners.push(combiner(stage)?);
            rest = &t[end + 1..];
            continue;
        }
        break;
    }
    for st in rest.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(v) = st.strip_prefix("out.rgb") {
            p.out_rgb = Some(input(v.trim().trim_start_matches('='))?);
        } else if let Some(v) = st.strip_prefix("out.a") {
            p.out_a = Some(input(v.trim().trim_start_matches('='))?);
        } else if st.starts_with("final_product") || st.starts_with("clamp_color_sum") {
            return err(format!("unsupported final combiner statement {st}"));
        }
    }
    Ok(p)
}

fn combiner(s: &str) -> Result<Combiner, ParseError> {
    let mut c = Combiner { scale: 1.0, ..Default::default() };
    let mut rest = s;
    loop {
        let t = rest.trim_start();
        if t.is_empty() {
            break;
        }
        for (part, which) in [("rgb", 0), ("alpha", 1)] {
            if let Some(r) = t.strip_prefix(part) {
                let r = r.trim_start();
                if let Some(r) = r.strip_prefix('{') {
                    let end = r.find('}').ok_or_else(|| ParseError { line: 0, message: "}".into() })?;
                    for st in r[..end].split(';').map(str::trim).filter(|s| !s.is_empty()) {
                        if st == "scale_by_two()" {
                            c.scale = 2.0;
                        } else if st == "scale_by_four()" {
                            c.scale = 4.0;
                        } else if st == "scale_by_one_half()" {
                            c.scale = 0.5;
                        } else if st == "bias_by_negative_one_half()" {
                            c.bias = -0.5;
                        } else {
                            let (d, e) = st.split_once('=').ok_or_else(|| ParseError { line: 0, message: st.into() })?;
                            let a = Assign { dst: d.trim().into(), expr: expr(e)? };
                            if which == 0 {
                                c.rgb.push(a);
                            } else {
                                c.alpha.push(a);
                            }
                        }
                    }
                    rest = &r[end + 1..];
                    break;
                }
            }
        }
        if rest.trim_start() == t {
            if t.starts_with("const") {
                let end = t.find(';').ok_or_else(|| ParseError { line: 0, message: "const".into() })?;
                c.consts.push(constant(&t[..end])?);
                rest = &t[end + 1..];
            } else {
                return err(format!("unsupported combiner text {t}"));
            }
        }
    }
    Ok(c)
}
