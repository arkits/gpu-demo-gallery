use std::collections::BTreeMap;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Default)]
pub struct ShaderFile {
    pub misc: BTreeMap<String, Vec<String>>,
    pub vars: BTreeMap<String, Var>,
    /// In declaration order: mesh and scene records refer to them by index.
    pub textures: Vec<TextureDecl>,
    /// How textures are sampled unless a pass says otherwise.
    pub samplers: BTreeMap<String, Sampler>,
    pub shaders: Vec<Shader>,
    blocks: Vec<Block>,
    /// Files whose declarations were already taken over.
    included: Vec<String>,
}

/// Shared HLSL. Files it needs are named beside it.
#[derive(Debug, Clone)]
struct Block {
    name: String,
    needs: Vec<String>,
    code: String,
}

#[derive(Debug, Clone)]
pub struct TextureDecl {
    pub name: String,
    pub cube: bool,
    /// `AppUpdate` name when the engine supplies the texture.
    pub app: Option<String>,
    /// Name of the render target shown here, if it is one.
    pub target: Option<String>,
    /// Script variable that names the render target shown here.
    pub script: Option<String>,
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
    /// Engine source and its arguments: an `AppUpdate`, or a builtin such as `WVP`.
    pub source: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Shader {
    pub name: String,
    pub properties: Vec<String>,
    pub passes: Vec<Pass>,
}

impl ShaderFile {
    /// Whether some shader has the property.
    pub fn any(&self, property: &str) -> bool {
        self.shaders.iter().any(|s| s.has(property))
    }
}

impl Shader {
    pub fn has(&self, property: &str) -> bool {
        self.properties
            .iter()
            .any(|p| p.eq_ignore_ascii_case(property))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sampler {
    pub texture: String,
    pub filter: Filter,
    /// Across u and v.
    pub clamp: [Address; 2],
    /// Largest anisotropy, 1 for none.
    pub aniso: u16,
    /// Added to the mip level.
    pub lod_bias: f32,
    /// Largest mip level used.
    pub max_mip: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Address {
    Wrap,
    Clamp,
    Mirror,
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
    pub pass: StencilOp,
    /// Counter-clockwise faces; equal to the above unless `StencilCCW` is set.
    pub ccw_zfail: StencilOp,
    pub ccw_pass: StencilOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cull {
    None,
    Cw,
    Ccw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendOp {
    Add,
    Subtract,
    /// What is there minus what is drawn.
    ReverseSubtract,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Blend {
    pub src: Factor,
    pub dst: Factor,
    pub op: BlendOp,
}

/// Complete device state for one pass. Passes start from the previous pass of
/// the same shader, then apply an inherited pass, then their own statements.
#[derive(Debug, Clone)]
pub struct Pass {
    pub name: String,
    /// `PointSpriteEnable TRUE`: each vertex is a square of `PSIZE` pixels.
    pub point_sprite: bool,
    pub stream_map: String,
    /// Included files and blocks of the vertex and pixel programs.
    parts: [Vec<(String, String)>; 2],
    pub vs: Option<Arc<str>>,
    pub ps: Option<Arc<str>>,
    /// Samplers the pass itself sets, by texture.
    pub samplers: BTreeMap<String, Sampler>,
    pub blend: Option<Blend>,
    pub z_test: bool,
    pub z_write: bool,
    pub z_func: Compare,
    /// Constant depth bias as written in the file.
    pub z_bias: f32,
    pub z_slope: f32,
    pub cull: Cull,
    pub stencil: Option<Stencil>,
    /// Red, green, blue, alpha.
    pub color_write: [bool; 4],
    /// Pixels are kept when their alpha compares this way to the value.
    pub alpha_test: Option<(Compare, f32)>,
    pub wireframe: bool,
    /// Pixels' alpha says how many samples they cover.
    pub alpha_to_coverage: bool,
}

impl Default for Pass {
    /// `gDefault.ssh` state for everything the demo's shaders rely on.
    fn default() -> Self {
        Self {
            name: String::new(),
            point_sprite: false,
            stream_map: String::new(),
            parts: Default::default(),
            vs: None,
            ps: None,
            samplers: BTreeMap::new(),
            blend: None,
            z_test: true,
            z_write: true,
            z_func: Compare::LessEqual,
            z_bias: 0.0,
            z_slope: 0.0,
            cull: Cull::Cw,
            stencil: None,
            color_write: [true; 4],
            alpha_test: None,
            wireframe: false,
            alpha_to_coverage: false,
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
        .map(|s| {
            let n = s.strip_suffix('f').unwrap_or(s);
            n.parse().map_err(|_| format!("not a number: {s:?}"))
        })
        .collect()
}

fn integer(s: &str) -> Option<u32> {
    match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
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
        "less" | "<" => Compare::Less,
        "equal" | "==" => Compare::Equal,
        "lessequal" | "<=" => Compare::LessEqual,
        "greater" | ">" => Compare::Greater,
        "notequal" | "!=" => Compare::NotEqual,
        "greaterequal" | ">=" => Compare::GreaterEqual,
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

/// Loop variables (name, start, step), repeat count and body lines.
type Replicate = (Vec<(String, i64, i64)>, i64, Vec<(usize, String)>);

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
/// Files named by `#include` are put in place.
fn preprocess(text: &str, include: &Include) -> Result<Vec<(usize, String)>> {
    let mut out = Vec::new();
    let mut block: Option<Replicate> = None;
    let mut other_platform = false;
    let text = strip_block_comments(text);
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("").trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("#include") {
            let name = name.trim().trim_matches('"');
            let text = include(name).ok_or_else(|| format!("cannot include {name}"))?;
            let lines = preprocess(&text, include)?;
            out.extend(lines.into_iter().map(|(_, l)| (n + 1, l)));
        } else if let Some(platform) = line.strip_prefix("#ifplatform") {
            other_platform = platform.trim() != "PC";
        } else if line.starts_with("#endplatform") {
            other_platform = false;
        } else if other_platform {
        } else if line.starts_with("#replicate") {
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
    let lines = preprocess(text, include)?;
    // A file may name itself as the one to inherit from.
    let include = &|name: &str| include(name).filter(|t| t != text);
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

fn block(it: &mut Lines, end: &str) -> String {
    let lines = it.by_ref().take_while(|(_, l)| !l.starts_with(end));
    lines.map(|(_, l)| l + "\n").collect()
}

fn statement(f: &mut ShaderFile, line: &str, it: &mut Lines, include: &Include) -> Result<()> {
    // Art notes are free text and may not tokenize.
    if line.starts_with("StartArtNotes") {
        skip(it, "EndArtNotes");
        return Ok(());
    }
    if let Some(rest) = line.strip_prefix("StartHLSL") {
        let mut words = tokens(rest)?.into_iter().filter_map(|t| match t {
            Tok::Word(w) => Some(w),
            Tok::Group(_) => None,
        });
        f.blocks.push(Block {
            name: words.next().unwrap_or_default(),
            needs: words.collect(),
            code: block(it, "EndHLSL"),
        });
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
        // Left over by authors.
        "must" | "Textures" => {}
        "Texture" => {
            let cube = t.iter().any(|t| *t == Tok::Word("CM".into()));
            let app = arg(&t, "AppUpdate").map(|g| g.to_vec());
            let name: String = word(t.get(1))?.into();
            let scripted = app.as_ref().filter(|a| a[0] == "ScriptRenderTarget");
            let script = scripted.and_then(|a| a.get(1)).cloned();
            f.textures.retain(|t| t.name != name);
            f.textures.push(TextureDecl {
                name,
                cube,
                app: app
                    .filter(|a| a[0] != "ScriptRenderTarget")
                    .map(|a| a[0].clone()),
                target: arg(&t, "Renderable").map(|g| g[0].clone()),
                script,
            });
        }
        "DefTexture" => {
            let name = word(t.get(1))?.to_string();
            let s = sampler(&name, &t[2..])?;
            f.samplers.insert(name, s);
        }
        k @ ("Float" | "Int" | "Vector" | "Matrix" | "Bool") => variable(f, k, &t)?,
        "StartShader" => {
            let name = t.get(1).map_or(Ok(""), |t| word(Some(t)))?;
            let shader = shader(f, name.into(), it, include)?;
            f.shaders.push(shader);
        }
        other => return Err(format!("unknown statement {other}")),
    }
    Ok(())
}

/// Adds the blocks of a file that a program includes, once each.
fn add_blocks(
    f: &mut ShaderFile,
    (file, name): (&str, &str),
    include: &Include,
    seen: &mut Vec<(String, usize)>,
    src: &mut String,
) -> Result<()> {
    let blocks = blocks_of(f, file, include)?;
    // Without a name: the block that has none, or all.
    let plain = name.is_empty() && blocks.iter().all(|b| !b.name.is_empty());
    let mut found = plain;
    for (index, b) in blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| plain || b.name == name)
    {
        found = true;
        for n in &b.needs {
            add_blocks(f, (n, ""), include, seen, src)?;
        }
        let key = (file.to_ascii_lowercase(), index);
        if !seen.contains(&key) {
            src.push_str(&b.code);
            seen.push(key);
        }
    }
    match found || name.is_empty() {
        true => Ok(()),
        false => Err(format!("{file} has no block {name}")),
    }
}

/// Takes over the declarations of an included file and returns its HLSL blocks.
fn blocks_of(f: &mut ShaderFile, file: &str, include: &Include) -> Result<Vec<Block>> {
    if file.eq_ignore_ascii_case("this") {
        return Ok(f.blocks.clone());
    }
    let text = include(file).ok_or_else(|| format!("cannot include {file}"))?;
    let other = parse(&text, include).map_err(|e| format!("{file}: {e}"))?;
    if !f.included.iter().any(|i| i == file) {
        f.included.push(file.into());
        for (k, v) in other.vars {
            f.vars.entry(k).or_insert(v);
        }
        for t in other.textures {
            if !f.textures.iter().any(|o| o.name == t.name) {
                f.textures.push(t);
            }
        }
        for (k, v) in other.samplers {
            f.samplers.entry(k).or_insert(v);
        }
    }
    Ok(other.blocks)
}

fn sampler(texture: &str, t: &[Tok]) -> Result<Sampler> {
    let mut filter = Filter::Linear;
    let mut rest = t.iter().peekable();
    while let Some(w) = rest.next() {
        // Words before parentheses are options, read below.
        if rest.next_if(|n| matches!(n, Tok::Group(_))).is_some() {
            continue;
        }
        filter = match word(Some(w))? {
            "Nearest" => Filter::Nearest,
            "Linear" => Filter::Linear,
            "Bilinear" => Filter::Bilinear,
            "Trilinear" => Filter::Trilinear,
            w => return Err(format!("unknown filter {w}")),
        };
    }
    let clamp = arg(t, "Clamp").map_or("", |g| g[0].as_str()).as_bytes();
    let number = |key: &str| arg(t, key).and_then(|g| g[0].parse::<f32>().ok());
    let aniso = number("Aniso").map_or(1, |a| a as u16);
    if aniso > 1 {
        filter = Filter::Trilinear;
    }
    Ok(Sampler {
        texture: texture.into(),
        filter,
        clamp: [0, 1].map(|i| match clamp.get(i) {
            Some(b'c') => Address::Clamp,
            Some(b'm') => Address::Mirror,
            _ => Address::Wrap,
        }),
        aniso,
        lod_bias: number("LODBias").unwrap_or(0.0),
        max_mip: number("MaxMip").map_or(32, |m| m as u32),
    })
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
            Err(_) => {
                var.source = Some(g.to_vec()).filter(|g| !g[0].eq_ignore_ascii_case("slider"));
            }
        }
    }
    if let Some(app) = arg(t, "AppUpdate") {
        var.source = Some(app.to_vec());
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
    // State set outside of passes.
    let mut start = Pass::default();
    while let Some((n, line)) = it.next() {
        let t = tokens(&line)?;
        match word(t.first())? {
            "EndShader" => return Ok(s),
            // Some files end a shader twice.
            "EndPass" => {}
            "Property" => {
                for p in &t[1..] {
                    s.properties.push(word(Some(p))?.into());
                }
            }
            "Optimize" | "Validate" => {}
            // Restores state after drawing; every pass here sets its own.
            "StartCleanup" => skip(it, "EndCleanup"),
            "StartPass" => {
                let mut pass = s.passes.last().cloned().unwrap_or(start.clone());
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
                pass_body(f, &mut pass, it, include).map_err(|e| format!("{}: {e}", s.name))?;
                s.passes.push(pass);
            }
            k => {
                let k = k.to_string();
                pass_statement(f, &mut start, &k, &t).map_err(|e| format!("line {n}: {e}"))?;
            }
        }
    }
    Err("missing EndShader".into())
}

/// Applies a pass of this file or an included one on top of `pass`.
/// Passes go by name, or by number from 1.
fn inherit(f: &mut ShaderFile, pass: &mut Pass, g: &[String], include: &Include) -> Result<()> {
    let [file, shader, name] = g else {
        return Err(format!("bad Inherit {g:?}"));
    };
    let find = |s: &[Shader]| -> Result<Pass> {
        let s = s
            .iter()
            .find(|s| s.name == *shader)
            .ok_or(format!("no shader {shader:?}"))?;
        let number = name.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
        let p = s.passes.iter().find(|p| p.name == *name);
        let p = p.or_else(|| s.passes.get(number?));
        Ok(p.ok_or(format!("no pass {name:?} in {shader:?}"))?.clone())
    };
    let keep = pass.name.clone();
    let text = include(file).filter(|_| file != "this");
    if let Ok(p) = find(&f.shaders)
        && text.is_none()
    {
        // The inherited pass is already fully resolved.
        *pass = p;
    } else if file == "this" {
        *pass = find(&f.shaders)?;
    } else {
        let text = text.ok_or(format!("cannot include {file}"))?;
        let other = parse(&text, include)?;
        let p = find(&other.shaders)?;
        *pass = p;
        for (k, v) in other.vars {
            f.vars.entry(k).or_insert(v);
        }
        for (k, v) in other.samplers {
            f.samplers.entry(k).or_insert(v);
        }
        for t in other.textures {
            if !f.textures.iter().any(|o| o.name == t.name) {
                f.textures.push(t);
            }
        }
    }
    pass.name = keep;
    Ok(())
}

fn pass_body(f: &mut ShaderFile, p: &mut Pass, it: &mut Lines, include: &Include) -> Result<()> {
    // Includes of earlier passes stay in force.
    let mut parts = std::mem::take(&mut p.parts);
    while let Some((n, line)) = it.next() {
        let at = |e: String| format!("line {n}: {e}");
        let t = tokens(&line).map_err(at)?;
        match word(t.first()).map_err(at)? {
            "EndPass" => {
                p.parts = parts;
                return Ok(());
            }
            k @ ("StartVertexShader" | "StartPixelShader") => {
                let body = block(it, &k.replace("Start", "End"));
                let pixel = usize::from(k == "StartPixelShader");
                let mut seen = Vec::new();
                let mut src = String::new();
                for (file, name) in &parts[pixel] {
                    add_blocks(f, (file, name), include, &mut seen, &mut src).map_err(at)?;
                }
                src.push_str(&body);
                match pixel {
                    0 => p.vs = Some(src.into()),
                    _ => p.ps = Some(src.into()),
                }
            }
            k @ ("VsInclude" | "PsInclude") => {
                let file = word(t.get(1)).map_err(at)?.to_string();
                let names = match t.get(2) {
                    Some(Tok::Group(g)) => g.clone(),
                    _ => vec![String::new()],
                };
                let list = &mut parts[usize::from(k == "PsInclude")];
                list.extend(names.into_iter().map(|n| (file.clone(), n)));
            }
            k => pass_statement(f, p, k, &t).map_err(at)?,
        }
    }
    Err("missing EndPass".into())
}

fn pass_statement(f: &ShaderFile, p: &mut Pass, k: &str, t: &[Tok]) -> Result<()> {
    match k {
        "SetStreamMap" => p.stream_map = word(t.get(1))?.into(),
        "PointSpriteEnable" => p.point_sprite = boolean(word(t.get(1))?)?,
        // Sampler settings for this pass only.
        "DefTexture" => {
            let name = word(t.get(1))?;
            p.samplers.insert(name.into(), sampler(name, &t[2..])?);
        }
        "SetTexture" => {
            // Stage or sampler, then the texture.
            let Ok(texture) = word(t.get(2)) else {
                return Ok(());
            };
            if f.textures.iter().any(|x| x.name == texture) {
                p.samplers
                    .insert(texture.into(), sampler(texture, &t[3..])?);
            }
        }
        "Blend" => {
            p.blend = if boolean(word(t.get(1))?)? {
                let get = |key| arg(t, key).map(|g| factor(&g[0]));
                let old = p.blend.unwrap_or(Blend {
                    src: Factor::One,
                    dst: Factor::Zero,
                    op: BlendOp::Add,
                });
                let op = arg(t, "Op").map_or("add".into(), |g| g[0].to_ascii_lowercase());
                Some(Blend {
                    src: get("Src").transpose()?.unwrap_or(old.src),
                    dst: get("Dest")
                        .or_else(|| get("Dst"))
                        .transpose()?
                        .unwrap_or(old.dst),
                    op: match op.as_str() {
                        "add" => BlendOp::Add,
                        "subtract" | "sub" => BlendOp::Subtract,
                        "revsubtract" => BlendOp::ReverseSubtract,
                        "min" => BlendOp::Min,
                        "max" => BlendOp::Max,
                        o => return Err(format!("unknown blend operation {o}")),
                    },
                })
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
                let op = |key| arg(t, key).map_or(Ok(StencilOp::Keep), |g| stencil_op(&g[0]));
                let (zfail, pass) = (op("ZFail")?, op("Pass")?);
                Some(Stencil {
                    func: func.unwrap_or(Compare::Always),
                    reference: arg(t, "Ref").and_then(|g| integer(&g[0])).unwrap_or(0),
                    zfail,
                    pass,
                    ccw_zfail: zfail,
                    ccw_pass: pass,
                })
            } else {
                None
            };
        }
        "StencilCCW" => {
            if let Some(s) = &mut p.stencil {
                if let Some(g) = arg(t, "ZFail") {
                    s.ccw_zfail = stencil_op(&g[0])?;
                }
                if let Some(g) = arg(t, "Pass") {
                    s.ccw_pass = stencil_op(&g[0])?;
                }
            }
        }
        "ColorWriteEnable" => {
            let g = match t.get(1) {
                Some(Tok::Group(g)) => g.as_slice(),
                Some(Tok::Word(w)) => std::slice::from_ref(w),
                None => return Err("bad ColorWriteEnable".into()),
            };
            p.color_write = match g[0].to_ascii_uppercase().as_str() {
                "TRUE" => [true; 4],
                "FALSE" => [false; 4],
                _ => ["R", "G", "B", "A"].map(|c| g.iter().any(|x| x.eq_ignore_ascii_case(c))),
            };
        }
        "AlphaTest" => {
            p.alpha_test = if boolean(word(t.get(1))?)? {
                let Ok(text) = word(t.get(3)) else {
                    return Ok(());
                };
                // Hexadecimal values are of 255.
                let value = match integer(text).filter(|_| text.starts_with("0x")) {
                    Some(v) => v as f32 / 255.0,
                    None => text.parse().map_err(|_| format!("bad {text}"))?,
                };
                Some((compare(word(t.get(2))?)?, value))
            } else {
                None
            };
        }
        "FillMode" => p.wireframe = word(t.get(1))? == "Wireframe",
        "AlphaToCoverage" => p.alpha_to_coverage = boolean(word(t.get(1))?)?,
        "ZSlopeScale" => p.z_slope = word(t.get(1))?.parse().map_err(|_| "bad ZSlopeScale")?,
        // State the demo leaves at its default or that has no equivalent here.
        "ZBias" => p.z_bias = word(t.get(1))?.parse().map_err(|_| "bad ZBias")?,
        "SetBlender"
        | "SetBlendState"
        | "TextureFactor"
        | "SeparateAlphaBlend"
        | "ReGamma(FALSE)"
        | "ReGamma"
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
             StartPass\nCull None\nEndPass\nStartPass\nZ FALSE\nEndPass\nEndShader",
            &none,
        )
        .unwrap();
        let s = &f.shaders[0];
        assert!(s.has("p") && s.has("Q"));
        assert_eq!((s.passes[1].cull, s.passes[1].z_test), (Cull::None, false));
        assert_eq!(f.vars["a"].default, [1.0, 2.0]);
    }

    #[test]
    fn included_blocks_come_before_the_program() {
        let file = |n: &str| {
            let text = "Vector v AppUpdate(ScriptVariable, x)\nStartHLSL A\na\nEndHLSL\nStartHLSL B\nb\nEndHLSL";
            (n == "i.shl").then(|| text.into())
        };
        let f = parse(
            "StartHLSL\nmine\nEndHLSL\nStartShader \"s\"\nStartPass\nVsInclude \"i.shl\"(B)\n\
             VsInclude this\nStartVertexShader(HLSL)\nbody\nEndVertexShader\nEndPass\nEndShader",
            &file,
        )
        .unwrap();
        assert_eq!(
            f.shaders[0].passes[0].vs.as_deref(),
            Some("b\nmine\nbody\n")
        );
        assert_eq!(
            f.vars["v"].source.as_deref(),
            Some(&["ScriptVariable".into(), "x".into()][..])
        );
    }

    #[test]
    fn replicate_steps_both_variables() {
        let out = preprocess(
            "#replicate ($i, 0, 3, 1) ($j, 30, 4)\nVsConst $j w$iM\n#endreplicate",
            &none,
        );
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
