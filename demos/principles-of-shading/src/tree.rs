//! NVIDIA's scene files: `.nmb` (binary) and `.nma` (the same trees as text).
//! A node has a kind, perhaps a name, and fields; the tables below come from the
//! viewer's own names.
use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Type {
    Int,
    /// A number, or one of these words in text.
    Enum(&'static [&'static str]),
    Bool,
    Float,
    Vector(usize),
    Vectors(usize),
    Ints,
    Str,
    Strs,
    One,
    Many,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u32),
    Bool(bool),
    Float(f32),
    /// One vector, or a list of vectors end to end.
    Floats(Vec<f32>),
    Ints(Vec<u32>),
    Text(String),
    Texts(Vec<String>),
    Node(Box<Node>),
    Nodes(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: &'static str,
    pub name: String,
    pub fields: BTreeMap<&'static str, Value>,
}

use Type::*;
type Fields = &'static [(&'static str, Type)];

const SOURCE: &[&str] = &[
    "ZERO",
    "CONST0",
    "CONST1",
    "FOG",
    "PRIMARY",
    "SECONDARY",
    "TEXTURE0",
    "TEXTURE1",
    "TEXTURE2",
    "TEXTURE3",
    "TEXTURE4",
    "TEXTURE5",
    "TEXTURE6",
    "TEXTURE7",
    "REGISTER0",
    "REGISTER1",
    "REGISTER2",
    "REGISTER3",
    "REGISTER4",
    "REGISTER5",
    "REGISTER6",
    "REGISTER7",
];
const MAP: &[&str] = &[
    "UNSIGNED_IDENTITY",
    "UNSIGNED_INVERT",
    "EXPAND",
    "EXPAND_NEG",
    "HALF_BIAS",
    "HALF_BIAS_NEG",
    "SIGNED_IDENTITY",
    "SIGNED_NEGATE",
];
const OPERATOR: &[&str] = &[
    "NOOP",
    "PASS_THRU",
    "SQUARE",
    "MOD",
    "DOT",
    "ADD",
    "ADD_SMOOTH",
    "ADD_MOD",
    "MOD_ADD",
    "INTERP",
    "ADD_MOD_MOD",
    "2MOD",
    "2DOT",
    "DOT_MOD",
    "ADD_2MOD",
    "MUX_2MOD",
];
const SCALE: &[&str] = &["X1", "X2", "X4", "XHALF"];
const BIAS: &[&str] = &["NONE", "NEG_HALF"];
const DEST: &[&str] = &[
    "REGISTER0",
    "REGISTER1",
    "REGISTER2",
    "REGISTER3",
    "REGISTER4",
    "REGISTER5",
    "REGISTER6",
    "REGISTER7",
    "DISCARD",
];
const COLOR_DEST: &[&str] = &[
    "diffuse",
    "specular",
    "ambient",
    "emissive",
    "ambientAndDiffuse",
    "none",
];
const SORT: &[&str] = &[
    "none",
    "distance",
    "static",
    "staticDoublesidedReverse",
    "unknown",
];
const HACK: &[&str] = &["none", "bumpDiffuse", "bumpSpecular", "bumpTexMatrix"];
const FILTER: &[&str] = &["NONE", "NEAREST", "LINEAR"];
const BLEND: &[&str] = &[
    "ZERO",
    "ONE",
    "SRCCOLOR",
    "INVSRCCOLOR",
    "SRCALPHA",
    "INVSRCALPHA",
];
const PRIMITIVE: &[&str] = &[
    "POINT_LIST",
    "LINE_LIST",
    "TRI_LIST",
    "TRI_LIST",
    "QUAD_LIST",
    "TRI_STRIP",
];
const FUNC: &[&str] = &[
    "NONE",
    "STATIC",
    "WORLD_LINEAR",
    "WORLD_NORMAL",
    "WORLD_REFLECT",
    "WORLD_SPHERE",
    "MODEL_LINEAR",
    "MODEL_NORMAL",
    "MODEL_REFLECT",
    "MODEL_SPHERE",
    "MODEL_ORIENT_LINEAR",
    "MODEL_ORIENT_NORMAL",
    "MODEL_ORIENT_REFLECT",
    "MODEL_ORIENT_SPHERE",
    "EYE_LINEAR",
    "EYE_NORMAL",
    "EYE_REFLECT",
    "EYE_SPHERE",
];

/// Id in binary files, kind, fields in the order of their ids.
const KINDS: &[(u32, &str, Fields)] = &[
    (0x1000_0021, "Group", &[("children", Many)]),
    (
        0x1000_0022,
        "Transform",
        &[
            ("scalePivot", Vector(3)),
            ("scale", Vector(3)),
            ("rotationPivot", Vector(3)),
            ("rotationOrientation", Vector(4)),
            ("rotation", Vector(4)),
            ("translation", Vector(3)),
            ("children", Many),
        ],
    ),
    (
        0x1000_0040,
        "Shape",
        &[
            ("lightsUsed", Strs),
            ("renderSettings", One),
            ("geometry", Many),
        ],
    ),
    (
        0x1000_0060,
        "IndexedFaceSet",
        &[
            ("coords", Vectors(3)),
            ("normals", Vectors(3)),
            ("vertexColors", Many),
            ("vertexIndices", Many),
            ("vertexWeights", Many),
            ("textureApplications", Many),
        ],
    ),
    (
        0x1000_f000,
        "TextureApplication",
        &[("textureFunc", Enum(FUNC)), ("textureCoords", Vectors(2))],
    ),
    (
        0x1000_f002,
        "VertexIndex",
        &[("type", Enum(PRIMITIVE)), ("indices", Ints)],
    ),
    (
        0x2000_0000,
        "RenderPass",
        &[
            ("sortFunc", Enum(SORT)),
            ("staticSortIndex", Int),
            ("setupHack", Enum(HACK)),
            ("setupHackParams", Strs),
            ("cullParams", One),
            ("fogParams", One),
            ("blendParams", One),
            ("stencilParams", One),
            ("alphaTestParams", One),
            ("depthTestParams", One),
            ("miscParams", One),
            ("combinerContext", One),
            ("combiners", Many),
        ],
    ),
    (
        0x3000_0000,
        "Combiner",
        &[("colorExpr", One), ("alphaExpr", One)],
    ),
    (
        0x3000_0001,
        "CombinerExpr",
        &[
            ("operator", Enum(OPERATOR)),
            ("outputScale", Enum(SCALE)),
            ("outputBias", Enum(BIAS)),
            ("destAB", Enum(DEST)),
            ("destCD", Enum(DEST)),
            ("destABCD", Enum(DEST)),
            ("inputs", Many),
        ],
    ),
    (
        0x3000_0002,
        "CombinerInput",
        &[
            ("source", Enum(SOURCE)),
            ("map", Enum(MAP)),
            ("isAlpha", Bool),
        ],
    ),
    (
        0x3000_0003,
        "CombinerContext",
        &[
            ("vertColor0Dest", Enum(COLOR_DEST)),
            ("vertColor1Dest", Enum(COLOR_DEST)),
            ("color0", Vector(4)),
            ("color1", Vector(4)),
            ("material", One),
            ("textures", Many),
        ],
    ),
    (
        0x4000_0101,
        "2DTexture",
        &[
            ("textureTransform", One),
            ("minFilter", Enum(FILTER)),
            ("magFilter", Enum(FILTER)),
            ("mipFilter", Enum(FILTER)),
            ("compress", Bool),
            ("mipmapBias", Float),
            ("repeatU", Bool),
            ("repeatV", Bool),
            ("filename", Str),
        ],
    ),
    (
        0x4000_0201,
        "CubicTexture",
        &[
            ("textureTransform", One),
            ("minFilter", Enum(FILTER)),
            ("magFilter", Enum(FILTER)),
            ("mipFilter", Enum(FILTER)),
            ("compress", Bool),
            ("mipmapBias", Float),
            ("filename", Str),
        ],
    ),
    (
        0x5000_0300,
        "Material",
        &[
            ("diffuseColor", Vector(4)),
            ("specularColor", Vector(4)),
            ("ambientColor", Vector(4)),
            ("emissiveColor", Vector(4)),
            ("shininess", Float),
        ],
    ),
    (
        0x1000_0031,
        "BlendParams",
        &[("srcBlend", Enum(BLEND)), ("destBlend", Enum(BLEND))],
    ),
];

impl Node {
    fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field)
    }
    pub fn int(&self, field: &str) -> u32 {
        match self.get(field) {
            Some(Value::Int(v)) => *v,
            Some(Value::Bool(b)) => u32::from(*b),
            _ => 0,
        }
    }
    pub fn floats(&self, field: &str) -> &[f32] {
        match self.get(field) {
            Some(Value::Floats(v)) => v,
            _ => &[],
        }
    }
    pub fn vec<const N: usize>(&self, field: &str) -> Option<[f32; N]> {
        self.floats(field).try_into().ok()
    }
    pub fn ints(&self, field: &str) -> &[u32] {
        match self.get(field) {
            Some(Value::Ints(v)) => v,
            _ => &[],
        }
    }
    pub fn text(&self, field: &str) -> &str {
        match self.get(field) {
            Some(Value::Text(v)) => v,
            _ => "",
        }
    }
    pub fn node(&self, field: &str) -> Option<&Node> {
        match self.get(field) {
            Some(Value::Node(n)) => Some(n),
            _ => None,
        }
    }
    pub fn nodes(&self, field: &str) -> &[Node] {
        match self.get(field) {
            Some(Value::Nodes(n)) => n,
            _ => &[],
        }
    }
}

struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.data.len());
        let end = end.ok_or_else(|| format!("cut short at {:#x}", self.at))?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }
    fn int(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn floats(&mut self, n: usize) -> Result<Vec<f32>, Error> {
        let b = self.take(n.checked_mul(4).ok_or("bad count")?)?;
        let f = |c: &[u8]| f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        Ok(b.chunks_exact(4).map(f).collect())
    }
    fn ints(&mut self, n: usize) -> Result<Vec<u32>, Error> {
        let b = self.take(n.checked_mul(4).ok_or("bad count")?)?;
        let f = |c: &[u8]| u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        Ok(b.chunks_exact(4).map(f).collect())
    }
    fn text(&mut self) -> Result<String, Error> {
        let n = self.int()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    /// Counts are limited by what is left, so that bad ones fail early.
    fn count(&mut self) -> Result<usize, Error> {
        let n = self.int()? as usize;
        if n > self.data.len() - self.at {
            return Err(format!("bad count at {:#x}", self.at).into());
        }
        Ok(n)
    }

    fn node(&mut self, depth: usize) -> Result<Node, Error> {
        if depth > 64 {
            return Err("nodes are nested too deeply".into());
        }
        let at = self.at;
        let id = self.int()?;
        let (_, kind, schema) = KINDS
            .iter()
            .find(|k| k.0 == id)
            .ok_or_else(|| format!("unknown node {id:#x} at {at:#x}"))?;
        let mut node = Node {
            kind,
            name: String::new(),
            fields: BTreeMap::new(),
        };
        loop {
            let at = self.at;
            let field = self.int()?;
            match field as i32 {
                -4 => return Ok(node),
                -3 => {
                    node.name = self.text()?;
                    continue;
                }
                _ => {}
            }
            let (name, ty) = schema
                .get(field as usize)
                .ok_or_else(|| format!("unknown field {field} of {kind} at {at:#x}"))?;
            let value = match ty {
                Int | Enum(_) => Value::Int(self.int()?),
                Bool => Value::Bool(self.take(1)?[0] != 0),
                Float => Value::Float(self.floats(1)?[0]),
                Vector(n) => Value::Floats(self.floats(*n)?),
                Vectors(n) => {
                    let count = self.count()?;
                    Value::Floats(self.floats(count * n)?)
                }
                Ints => {
                    let count = self.count()?;
                    Value::Ints(self.ints(count)?)
                }
                Str => Value::Text(self.text()?),
                Strs => {
                    let count = self.count()?;
                    Value::Texts((0..count).map(|_| self.text()).collect::<Result<_, _>>()?)
                }
                One => Value::Node(Box::new(self.node(depth + 1)?)),
                Many => {
                    let count = self.count()?;
                    let nodes = (0..count).map(|_| self.node(depth + 1));
                    Value::Nodes(nodes.collect::<Result<_, _>>()?)
                }
            };
            node.fields.insert(name, value);
        }
    }
}

/// Reads a whole `.nmb` file; every byte must be understood.
pub fn read_binary(data: &[u8]) -> Result<Node, Error> {
    let mut b = Bytes { data, at: 0 };
    let node = b.node(0)?;
    if b.at != data.len() {
        return Err(format!("{} unread bytes", data.len() - b.at).into());
    }
    Ok(node)
}

struct Words<'a> {
    words: Vec<&'a str>,
    at: usize,
}

impl<'a> Words<'a> {
    fn new(text: &'a str) -> Self {
        let mut words = Vec::new();
        let mut rest = text;
        loop {
            // Commas only separate.
            rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
            let Some(first) = rest.chars().next() else {
                break;
            };
            let n = match first {
                '"' => rest[1..].find('"').map_or(rest.len(), |i| i + 2),
                '{' | '}' | '[' | ']' => 1,
                _ => rest
                    .find(|c: char| c.is_whitespace() || ",{}[]\"".contains(c))
                    .unwrap_or(rest.len()),
            };
            words.push(&rest[..n]);
            rest = &rest[n..];
        }
        Self { words, at: 0 }
    }
    fn peek(&self) -> &'a str {
        self.words.get(self.at).copied().unwrap_or("")
    }
    fn next(&mut self) -> &'a str {
        let w = self.peek();
        self.at += 1;
        w
    }
    fn expect(&mut self, what: &str) -> Result<(), Error> {
        let w = self.next();
        if w == what {
            Ok(())
        } else {
            Err(format!("expected {what}, found {w:?}").into())
        }
    }
    fn float(&mut self) -> Result<f32, Error> {
        let w = self.next();
        w.parse().map_err(|_| format!("not a number: {w:?}").into())
    }
    fn quoted(&mut self) -> String {
        self.next().trim_matches('"').to_owned()
    }

    fn node(&mut self, depth: usize) -> Result<Node, Error> {
        if depth > 64 {
            return Err("nodes are nested too deeply".into());
        }
        let word = self.next();
        let (_, kind, schema) = KINDS
            .iter()
            .find(|k| k.1 == word)
            .ok_or_else(|| format!("unknown node {word:?}"))?;
        let mut node = Node {
            kind,
            name: String::new(),
            fields: BTreeMap::new(),
        };
        if self.peek() != "{" {
            node.name = self.next().to_owned();
        }
        self.expect("{")?;
        while self.peek() != "}" {
            let word = self.next();
            let (name, ty) = schema
                .iter()
                .find(|f| f.0.eq_ignore_ascii_case(word))
                .ok_or_else(|| format!("unknown field {word:?} of {kind}"))?;
            let value = match ty {
                Int => Value::Int(self.float()? as u32),
                Enum(names) => {
                    let w = self.next();
                    let i = names.iter().position(|n| *n == w);
                    Value::Int(i.ok_or_else(|| format!("unknown {name} {w:?}"))? as u32)
                }
                Bool => Value::Bool(self.float()? != 0.0),
                Float => Value::Float(self.float()?),
                Vector(n) => {
                    Value::Floats((0..*n).map(|_| self.float()).collect::<Result<_, _>>()?)
                }
                Str => Value::Text(self.quoted()),
                One => Value::Node(Box::new(self.node(depth + 1)?)),
                Vectors(_) | Ints | Strs | Many => {
                    self.expect("[")?;
                    let (mut floats, mut texts, mut nodes) = (vec![], vec![], vec![]);
                    while self.peek() != "]" {
                        match ty {
                            Vectors(_) | Ints => floats.push(self.float()?),
                            Strs => texts.push(self.quoted()),
                            _ => nodes.push(self.node(depth + 1)?),
                        }
                    }
                    self.expect("]")?;
                    match ty {
                        Vectors(_) => Value::Floats(floats),
                        Ints => Value::Ints(floats.into_iter().map(|f| f as u32).collect()),
                        Strs => Value::Texts(texts),
                        _ => Value::Nodes(nodes),
                    }
                }
            };
            node.fields.insert(name, value);
        }
        self.expect("}")?;
        Ok(node)
    }
}

/// Reads a whole `.nma` file.
pub fn read_text(text: &str) -> Result<Node, Error> {
    let mut w = Words::new(text);
    let node = w.node(0)?;
    if w.at != w.words.len() {
        return Err(format!("unread text from {:?}", w.peek()).into());
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_binary_agree() {
        let t = read_text(
            "Group { children [ Shape a { lightsUsed [ \"L\" , ] renderSettings RenderPass { \
             setupHack bumpDiffuse combinerContext CombinerContext { color0 1 0.5 0 1 } } } ] }",
        )
        .unwrap();
        let mut b: Vec<u8> = Vec::new();
        let int = |b: &mut Vec<u8>, v: u32| b.extend(v.to_le_bytes());
        for v in [0x1000_0021, 0, 1, 0x1000_0040, -3i32 as u32, 1] {
            int(&mut b, v);
        }
        b.push(b'a');
        for v in [0, 1, 1] {
            int(&mut b, v);
        }
        b.push(b'L');
        for v in [1, 0x2000_0000, 2, 1, 11, 0x3000_0003, 2] {
            int(&mut b, v);
        }
        for f in [1.0f32, 0.5, 0.0, 1.0] {
            b.extend(f.to_le_bytes());
        }
        for _ in 0..4 {
            int(&mut b, -4i32 as u32);
        }
        assert_eq!(read_binary(&b).unwrap(), t);
        assert!(read_binary(&b[..b.len() - 1]).is_err());
        assert!(read_binary(&[&b[..], &[0]].concat()).is_err());
        assert!(read_text("Group { nonsense 1 }").is_err());
    }
}
