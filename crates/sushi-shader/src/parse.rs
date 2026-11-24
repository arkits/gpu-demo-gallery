use std::collections::BTreeMap;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Default)]
pub struct ShaderFile {
    pub misc: BTreeMap<String, Vec<String>>,
    pub vars: BTreeMap<String, Var>,
    /// In declaration order: mesh and scene records refer to them by index.
    pub textures: Vec<TextureDecl>,
    pub shaders: Vec<Shader>,
}

#[derive(Debug, Clone)]
pub struct TextureDecl {
    pub name: String,
    pub cube: bool,
    /// `AppUpdate` name when the engine supplies the texture.
    pub app: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    Float,
    Vector,
    Matrix,
}

#[derive(Debug, Clone)]
pub struct Var {
    pub kind: VarKind,
    pub default: Vec<f32>,
    /// Engine source: an `AppUpdate` name, or a builtin such as `WVP`.
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Shader {
    pub name: String,
    pub properties: Vec<String>,
    pub passes: Vec<Pass>,
}

impl Shader {
    pub fn has(&self, property: &str) -> bool {
        self.properties
            .iter()
            .any(|p| p.eq_ignore_ascii_case(property))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Var(String),
    Literal([f32; 4]),
    /// `(CameraPosition, WorldSpace)` and similar, by first word and space.
    Builtin(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sampler {
    pub texture: String,
    pub filter: Filter,
    pub clamp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    /// No mips.
    Linear,
    /// Linear within a level, nearest level.
    Bilinear,
    Trilinear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Factor {
    Zero,
    One,
    SrcAlpha,
    InvSrcAlpha,
    DestAlpha,
    InvDestAlpha,
    SrcColor,
    InvSrcColor,
    DestColor,
    InvDestColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compare {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StencilOp {
    Keep,
    IncrSat,
    DecrSat,
    Incr,
    Decr,
    Zero,
    Replace,
    Invert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stencil {
    pub func: Compare,
    pub reference: u32,
    pub zfail: StencilOp,
    /// Counter-clockwise faces; equal to `zfail` unless `StencilCCW` is set.
    pub ccw_zfail: StencilOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cull {
    None,
    Cw,
    Ccw,
}

/// Fixed-function stage 0, for passes without a pixel shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Diffuse,
    Texture,
    Factor,
    DiffuseTimesFactor,
}

/// Complete device state for one pass. Passes start from the previous pass of
/// the same shader, then apply an inherited pass, then their own statements.
#[derive(Debug, Clone)]
pub struct Pass {
    pub name: String,
    pub stream_map: String,
    pub vs: Option<Arc<str>>,
    pub ps: Option<Arc<str>>,
    pub vs_const: BTreeMap<u32, Const>,
    pub ps_const: BTreeMap<u32, Const>,
    pub samplers: BTreeMap<u32, Sampler>,
    pub blend: Option<(Factor, Factor)>,
    pub z_test: bool,
    pub z_write: bool,
    pub z_func: Compare,
    pub cull: Cull,
    pub stencil: Option<Stencil>,
    pub color_write: bool,
    pub wireframe: bool,
    pub factor: [f32; 4],
    pub stage: Option<Stage>,
}

impl Default for Pass {
    /// `gDefault.ssh` state for everything the demo's shaders rely on.
    fn default() -> Self {
        Self {
            name: String::new(),
            stream_map: String::new(),
            vs: None,
            ps: None,
            vs_const: BTreeMap::new(),
            ps_const: BTreeMap::new(),
            samplers: BTreeMap::new(),
            blend: None,
            z_test: true,
            z_write: true,
            z_func: Compare::LessEqual,
            cull: Cull::Cw,
            stencil: None,
            color_write: true,
            wireframe: false,
            factor: [1.0; 4],
            stage: None,
        }
    }
}

/// Splits a statement into words, quoted strings and parenthesized groups.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Group(Vec<String>),
}

fn tokens(line: &str) -> Result<Vec<Tok>> {
    let mut out = Vec::new();
    let c: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            ch if ch.is_whitespace() || ch == ',' => i += 1,
            '"' => {
                let end = (i + 1..c.len())
                    .find(|&j| c[j] == '"')
                    .ok_or("unclosed quote")?;
                out.push(Tok::Word(c[i + 1..end].iter().collect()));
                i = end + 1;
            }
            '(' => {
                let mut depth = 0;
                let end = (i..c.len())
                    .find(|&j| {
                        match c[j] {
                            '(' => depth += 1,
                            ')' => depth -= 1,
                            _ => {}
                        }
                        depth == 0
                    })
                    .ok_or("unclosed parenthesis")?;
                let inner: String = c[i + 1..end].iter().collect();
                let parts = inner
                    .split(',')
                    .map(|p| p.trim().trim_matches('"').to_string());
                out.push(Tok::Group(parts.collect()));
                i = end + 1;
            }
            _ => {
                let end = (i..c.len())
                    .find(|&j| c[j].is_whitespace() || "(,\"".contains(c[j]))
                    .unwrap_or(c.len());
                out.push(Tok::Word(c[i..end].iter().collect()));
                i = end;
            }
        }
    }
    Ok(out)
}

fn word(t: Option<&Tok>) -> Result<&str> {
    match t {
        Some(Tok::Word(w)) => Ok(w),
        other => Err(format!("expected a word, found {other:?}")),
    }
}

/// The group following word `key`, e.g. `Func(Less)`.
fn arg<'a>(t: &'a [Tok], key: &str) -> Option<&'a [String]> {
    t.windows(2).find_map(|w| match w {
        [Tok::Word(k), Tok::Group(g)] if k.eq_ignore_ascii_case(key) => Some(g.as_slice()),
        _ => None,
    })
}

fn floats(g: &[String]) -> Result<Vec<f32>> {
    g.iter()
        .map(|s| s.parse().map_err(|_| format!("not a number: {s:?}")))
        .collect()
}

fn vec4(g: &[String]) -> Result<[f32; 4]> {
    let v = floats(g)?;
    Ok(std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0)))
}

fn boolean(s: &str) -> Result<bool> {
    match s.to_ascii_uppercase().as_str() {
        "TRUE" => Ok(true),
        "FALSE" => Ok(false),
        _ => Err(format!("not a boolean: {s}")),
    }
}

fn factor(s: &str) -> Result<Factor> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "zero" => Factor::Zero,
        "one" => Factor::One,
        "srcalpha" => Factor::SrcAlpha,
        "invsrcalpha" => Factor::InvSrcAlpha,
        "destalpha" | "dstalpha" => Factor::DestAlpha,
        "invdestalpha" | "invdstalpha" => Factor::InvDestAlpha,
        "srccolor" => Factor::SrcColor,
        "invsrccolor" => Factor::InvSrcColor,
        "destcolor" | "dstcolor" => Factor::DestColor,
        "invdestcolor" | "invdstcolor" => Factor::InvDestColor,
        _ => return Err(format!("unknown blend factor {s}")),
    })
}

fn compare(s: &str) -> Result<Compare> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "never" => Compare::Never,
        "less" => Compare::Less,
        "equal" => Compare::Equal,
        "lessequal" => Compare::LessEqual,
        "greater" => Compare::Greater,
        "notequal" => Compare::NotEqual,
        "greaterequal" => Compare::GreaterEqual,
        "always" => Compare::Always,
        _ => return Err(format!("unknown comparison {s}")),
    })
}

fn stencil_op(s: &str) -> Result<StencilOp> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "keep" => StencilOp::Keep,
        "incrsat" => StencilOp::IncrSat,
        "decrsat" => StencilOp::DecrSat,
        "incr" => StencilOp::Incr,
        "decr" => StencilOp::Decr,
        "zero" => StencilOp::Zero,
        "replace" => StencilOp::Replace,
        "invert" => StencilOp::Invert,
        _ => return Err(format!("unknown stencil operation {s}")),
    })
}

/// Removes `/* */`, keeping line numbers. `//` comments hide block markers.
fn strip_block_comments(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        let mut rest = line;
        loop {
            if inside {
                match rest.split_once("*/") {
                    Some((_, r)) => (inside, rest) = (false, r),
                    None => break,
                }
            } else {
                match rest
                    .find("/*")
                    .filter(|&i| rest.find("//").is_none_or(|j| i < j))
                {
                    Some(i) => {
                        out.push_str(&rest[..i]);
                        (inside, rest) = (true, &rest[i + 2..]);
                    }
                    None => {
                        out.push_str(rest);
                        break;
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

/// Expands `#replicate ($i, start, count, step) ($j, start, step)`.
fn preprocess(text: &str) -> Result<Vec<(usize, String)>> {
    let mut out = Vec::new();
    let mut block: Option<(Vec<(String, i64, i64)>, i64, Vec<(usize, String)>)> = None;
    let text = strip_block_comments(text);
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("").trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#replicate") {
            let mut vars = Vec::new();
            let mut count = 0;
            for (k, t) in tokens(&line)?.iter().skip(1).enumerate() {
                let Tok::Group(g) = t else {
                    return Err(format!("bad {line}"));
                };
                let num = |i: usize| -> Result<i64> {
                    g.get(i)
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| format!("bad {line}"))
                };
                if k == 0 {
                    count = num(2)?;
                    vars.push((g[0].clone(), num(1)?, num(3)?));
                } else {
                    vars.push((g[0].clone(), num(1)?, num(2)?));
                }
            }
            block = Some((vars, count, Vec::new()));
        } else if line.starts_with("#endreplicate") {
            let (vars, count, body) = block.take().ok_or("stray #endreplicate")?;
            for i in 0..count {
                for (n, l) in &body {
                    let mut l = l.clone();
                    for (name, start, step) in &vars {
                        l = l.replace(name.as_str(), &(start + i * step).to_string());
                    }
                    out.push((*n, l));
                }
            }
        } else if let Some((.., body)) = &mut block {
            body.push((n + 1, line));
        } else {
            out.push((n + 1, line));
        }
    }
    Ok(out)
}

/// Loads an included file by name, relative to the including shader.
pub type Include<'a> = dyn Fn(&str) -> Option<String> + 'a;

pub fn parse(text: &str, include: &Include) -> Result<ShaderFile> {
    let mut file = ShaderFile::default();
    let lines = preprocess(text)?;
    let mut it = lines.into_iter();
    while let Some((n, line)) = it.next() {
        statement(&mut file, &line, &mut it, include).map_err(|e| format!("line {n}: {e}"))?;
    }
    Ok(file)
}

type Lines = std::vec::IntoIter<(usize, String)>;

fn skip(it: &mut Lines, end: &str) {
    let _ = it.find(|(_, l)| l.starts_with(end));
}

fn statement(f: &mut ShaderFile, line: &str, it: &mut Lines, include: &Include) -> Result<()> {
    // Art notes are free text and may not tokenize.
    if line.starts_with("StartArtNotes") {
        skip(it, "EndArtNotes");
        return Ok(());
    }
    let t = tokens(line)?;
    match word(t.first())? {
        "StartMisc" => {
            for (_, l) in it.by_ref().take_while(|(_, l)| !l.starts_with("EndMisc")) {
                let t = tokens(&l)?;
                let rest = t[1..].iter().map(|t| match t {
                    Tok::Word(w) => w.clone(),
                    Tok::Group(g) => g.join(","),
                });
                f.misc.insert(word(t.first())?.into(), rest.collect());
            }
        }
        // Vertex layouts come from the compiled headers in the data files.
        "StartStream" => skip(it, "EndStream"),
        "StreamMap" | "Validate" | "Optimize" | "Fallback" => {}
        "Texture" => {
            let cube = t.iter().any(|t| *t == Tok::Word("CM".into()));
            let app = arg(&t, "AppUpdate").map(|g| g[0].clone());
            f.textures.push(TextureDecl {
                name: word(t.get(1))?.into(),
                cube,
                app,
            });
        }
        k @ ("Float" | "Int" | "Vector" | "Matrix") => variable(f, k, &t)?,
        "StartShader" => {
            let shader = shader(f, word(t.get(1))?.into(), it, include)?;
            f.shaders.push(shader);
        }
        other => return Err(format!("unknown statement {other}")),
    }
    Ok(())
}

fn variable(f: &mut ShaderFile, kind: &str, t: &[Tok]) -> Result<()> {
    let kind = match kind {
        "Matrix" => VarKind::Matrix,
        "Vector" => VarKind::Vector,
        _ => VarKind::Float,
    };
    let mut var = Var {
        kind,
        default: Vec::new(),
        source: None,
    };
    if let Some(Tok::Group(g)) = t.get(2) {
        match floats(g) {
            Ok(v) => var.default = v,
            Err(_) => var.source = Some(g.join(",")),
        }
    }
    if let Some(app) = arg(t, "AppUpdate") {
        var.source = Some(app[0].clone());
    }
    f.vars.insert(word(t.get(1))?.into(), var);
    Ok(())
}

fn shader(f: &mut ShaderFile, name: String, it: &mut Lines, include: &Include) -> Result<Shader> {
    let mut s = Shader {
        name,
        properties: Vec::new(),
        passes: Vec::new(),
    };
    while let Some((n, line)) = it.next() {
        let t = tokens(&line)?;
        match word(t.first())? {
            "EndShader" => return Ok(s),
            "Property" => {
                for p in &t[1..] {
                    s.properties.push(word(Some(p))?.into());
                }
            }
            "Optimize" | "Validate" => {}
            // Restores state after drawing; every pass here sets its own.
            "StartCleanup" => skip(it, "EndCleanup"),
            "StartPass" => {
                let mut pass = s.passes.last().cloned().unwrap_or_default();
                if let Some(Tok::Word(w)) = t.get(1)
                    && w != "Inherit"
                {
                    pass.name = w.clone();
                } else {
                    pass.name = String::new();
                }
                if let Some(g) = arg(&t, "Inherit") {
                    inherit(f, &mut pass, g, include).map_err(|e| format!("line {n}: {e}"))?;
                }
                pass_body(f, &mut pass, it)?;
                s.passes.push(pass);
            }
            other => return Err(format!("line {n}: unknown shader statement {other}")),
        }
    }
    Err("missing EndShader".into())
}

/// Applies a pass of this file or an included one on top of `pass`.
fn inherit(f: &mut ShaderFile, pass: &mut Pass, g: &[String], include: &Include) -> Result<()> {
    let [file, shader, name] = g else {
        return Err(format!("bad Inherit {g:?}"));
    };
    let find = |s: &[Shader]| -> Result<Pass> {
        let s = s
            .iter()
            .find(|s| s.name == *shader)
            .ok_or(format!("no shader {shader:?}"))?;
        let p = s.passes.iter().find(|p| p.name == *name);
        Ok(p.ok_or(format!("no pass {name:?} in {shader:?}"))?.clone())
    };
    let keep = pass.name.clone();
    if file == "this" {
        // The inherited pass is already fully resolved.
        *pass = find(&f.shaders)?;
    } else {
        let text = include(file).ok_or(format!("cannot include {file}"))?;
        let other = parse(&text, include)?;
        let p = find(&other.shaders)?;
        pass.vs_const.extend(p.vs_const);
        pass.ps_const.extend(p.ps_const);
        for (k, v) in other.vars {
            f.vars.entry(k).or_insert(v);
        }
    }
    pass.name = keep;
    Ok(())
}

fn pass_body(f: &ShaderFile, p: &mut Pass, it: &mut Lines) -> Result<()> {
    while let Some((n, line)) = it.next() {
        let t = tokens(&line)?;
        let at = |e: String| format!("line {n}: {e}");
        match word(t.first()).map_err(at)? {
            "EndPass" => return Ok(()),
            k @ ("StartVertexShader" | "StartPixelShader") => {
                let end = k.replace("Start", "End");
                let mut src = String::new();
                for (_, l) in it.by_ref().take_while(|(_, l)| !l.starts_with(&end)) {
                    src.push_str(&l);
                    src.push('\n');
                }
                let src = Some(Arc::from(src));
                if k == "StartVertexShader" {
                    p.vs = src
                } else {
                    p.ps = src
                }
            }
            k => pass_statement(f, p, k, &t).map_err(at)?,
        }
    }
    Err("missing EndPass".into())
}

fn constant(t: &[Tok]) -> Result<Const> {
    match t.get(2) {
        Some(Tok::Word(w)) => Ok(Const::Var(w.clone())),
        Some(Tok::Group(g)) if g[0].starts_with(|c: char| c.is_ascii_alphabetic()) => Ok(
            Const::Builtin(g[0].clone(), g.get(1).cloned().unwrap_or_default()),
        ),
        Some(Tok::Group(g)) => Ok(Const::Literal(vec4(g)?)),
        None => Err("constant without a value".into()),
    }
}

fn pass_statement(f: &ShaderFile, p: &mut Pass, k: &str, t: &[Tok]) -> Result<()> {
    let index = || -> Result<u32> {
        word(t.get(1))?
            .parse()
            .map_err(|_| format!("{k}: bad index"))
    };
    match k {
        "SetStreamMap" => p.stream_map = word(t.get(1))?.into(),
        "VsConst" => {
            p.vs_const.insert(index()?, constant(t)?);
        }
        "PsConst" => {
            p.ps_const.insert(index()?, constant(t)?);
        }
        "SetTexture" => {
            let texture = word(t.get(2))?.to_string();
            if texture == "NULL" {
                p.samplers.remove(&index()?);
                return Ok(());
            }
            let filter = match t.get(3) {
                Some(Tok::Word(w)) => match w.as_str() {
                    "Nearest" => Filter::Nearest,
                    "Linear" => Filter::Linear,
                    "Bilinear" => Filter::Bilinear,
                    "Trilinear" => Filter::Trilinear,
                    w => return Err(format!("unknown filter {w}")),
                },
                _ => Filter::Linear,
            };
            let clamp = arg(t, "Clamp").is_some_and(|g| g[0].starts_with('c'));
            p.samplers.insert(
                index()?,
                Sampler {
                    texture,
                    filter,
                    clamp,
                },
            );
        }
        "Blend" => {
            p.blend = if boolean(word(t.get(1))?)? {
                let get = |key| arg(t, key).map(|g| factor(&g[0]));
                let old = p.blend.unwrap_or((Factor::One, Factor::Zero));
                Some((
                    get("Src").transpose()?.unwrap_or(old.0),
                    get("Dest").transpose()?.unwrap_or(old.1),
                ))
            } else {
                None
            };
        }
        "Z" => {
            if let Some(Tok::Word(w)) = t.get(1)
                && let Ok(b) = boolean(w)
            {
                p.z_test = b;
                if !b {
                    // D3D does not write depth while the test is off.
                    p.z_write = false;
                }
            }
            if let Some(g) = arg(t, "Write") {
                p.z_write = boolean(&g[0])? && p.z_test;
            }
            if let Some(g) = arg(t, "Func") {
                p.z_func = compare(&g[0])?;
            }
        }
        "Cull" => {
            p.cull = match word(t.get(1))? {
                "None" => Cull::None,
                "CW" => Cull::Cw,
                "CCW" => Cull::Ccw,
                w => return Err(format!("unknown cull mode {w}")),
            }
        }
        "Stencil" => {
            p.stencil = if boolean(word(t.get(1))?)? {
                let func = arg(t, "Func").map(|g| compare(&g[0])).transpose()?;
                let zfail = arg(t, "ZFail").map(|g| stencil_op(&g[0])).transpose()?;
                let zfail = zfail.unwrap_or(StencilOp::Keep);
                Some(Stencil {
                    func: func.unwrap_or(Compare::Always),
                    reference: arg(t, "Ref").and_then(|g| g[0].parse().ok()).unwrap_or(0),
                    zfail,
                    ccw_zfail: zfail,
                })
            } else {
                None
            };
        }
        "StencilCCW" => {
            if let (Some(s), Some(g)) = (&mut p.stencil, arg(t, "ZFail")) {
                s.ccw_zfail = stencil_op(&g[0])?;
            }
        }
        "ColorWriteEnable" => {
            let Some(Tok::Group(g)) = t.get(1) else {
                return Err("bad ColorWriteEnable".into());
            };
            p.color_write = !g[0].eq_ignore_ascii_case("FALSE");
        }
        "FillMode" => p.wireframe = word(t.get(1))? == "Wireframe",
        "TextureFactor" => {
            p.factor = match t.get(1) {
                Some(Tok::Group(g)) => vec4(g)?,
                Some(Tok::Word(w)) => match f.vars.get(w) {
                    Some(v) => std::array::from_fn(|i| v.default.get(i).copied().unwrap_or(0.0)),
                    None => p.factor, // hexadecimal default
                },
                None => return Err("TextureFactor without a value".into()),
            }
        }
        "SetBlender" if index()? == 0 => {
            let g = arg(t, "Color").ok_or("SetBlender without Color")?;
            let g: Vec<&str> = g.iter().map(String::as_str).collect();
            p.stage = Some(match g.as_slice() {
                ["SelectArg1", "Texture", ..] => Stage::Texture,
                ["SelectArg1", "TFactor", ..] => Stage::Factor,
                ["SelectArg1", "Diffuse", ..] => Stage::Diffuse,
                ["Modulate", "Diffuse", "TFactor"] => Stage::DiffuseTimesFactor,
                other => {
                    // Debug and font shaders use other modes; they are not drawn.
                    let _ = other;
                    Stage::Texture
                }
            });
        }
        // State the demo leaves at its default or that has no equivalent here.
        "SetBlender"
        | "ZBias"
        | "ZSlopeScale"
        | "AlphaTest"
        | "ShadeMode"
        | "PixelShader"
        | "VertexShader"
        | "Optimize"
        | "MultiSampleAntiAlias"
        | "MultiSampleMask"
        | "Lighting"
        | "SetTexRoute"
        | "SetCoord"
        | "SetEnvBump"
        | "Transform"
        | "VertexBlend"
        | "Fog"
        | "Clipping"
        | "LastPixel"
        | "DitherEnable"
        | "SpecularEnable"
        | "RenderTarget"
        | "PointSize"
        | "PointSpriteEnable"
        | "PointScaleEnable"
        | "PointScaleA"
        | "PointScaleB"
        | "PointScaleC"
        | "MinPointSize"
        | "MaxPointSize"
        | "ClipPlaneEnable"
        | "NormalizeNormals"
        | "LocalViewer"
        | "ColorVertex"
        | "Ambient"
        | "MaterialProperty"
        | "DiffuseMaterialSource"
        | "SpecularMaterialSource"
        | "AmbientMaterialSource"
        | "EmissiveMaterialSource"
        | "IndexedVertexBlendEnable"
        | "StencilMask" => {}
        other => return Err(format!("unknown pass statement {other}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn passes_continue_from_the_previous_pass() {
        let f = parse(
            "Vector a(1,2)\nStartShader \"s\"\nProperty \"P\" \"Q\"\n\
             StartPass\nVsConst 5 a\nCull None\nEndPass\n\
             StartPass\nVsConst 5 b // moved\nEndPass\nEndShader",
            &none,
        )
        .unwrap();
        let s = &f.shaders[0];
        assert!(s.has("p") && s.has("Q"));
        assert_eq!(s.passes[0].vs_const[&5], Const::Var("a".into()));
        assert_eq!(s.passes[1].vs_const[&5], Const::Var("b".into()));
        assert_eq!(s.passes[1].cull, Cull::None);
        assert_eq!(f.vars["a"].default, [1.0, 2.0]);
    }

    #[test]
    fn replicate_steps_both_variables() {
        let out =
            preprocess("#replicate ($i, 0, 3, 1) ($j, 30, 4)\nVsConst $j w$iM\n#endreplicate");
        let lines: Vec<_> = out.unwrap().into_iter().map(|l| l.1).collect();
        assert_eq!(
            lines,
            ["VsConst 30 w0M", "VsConst 34 w1M", "VsConst 38 w2M"]
        );
    }

    #[test]
    fn stencil_faces_differ_with_stencil_ccw() {
        let f = parse(
            "StartShader \"s\"\nStartPass\nStencil TRUE Func(Always) ZFail(DecrSat)\n\
             StencilCCW TRUE ZFail(IncrSat)\nEndPass\nEndShader",
            &none,
        )
        .unwrap();
        let s = f.shaders[0].passes[0].stencil.unwrap();
        assert_eq!(
            (s.zfail, s.ccw_zfail),
            (StencilOp::DecrSat, StencilOp::IncrSat)
        );
    }

    #[test]
    fn unknown_statements_are_errors() {
        assert!(parse("Nonsense 1", &none).is_err());
    }
}
