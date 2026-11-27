//! NVIDIA's scene (`.nmb`) and animation (`.nab`) files: trees of nodes with a kind,
//! perhaps a name, and fields. The tables below come from the viewer's own names.
use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Type {
    Int,
    Bool,
    Float,
    Vector(usize),
    Vectors(usize),
    Ints,
    Str,
    Strs,
    One,
    Many,
    /// A value, two kinds of tangent, and the tangents: x in and out, y in and out.
    Key,
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
        &[("textureFunc", Int), ("textureCoords", Vectors(2))],
    ),
    (
        0x1000_f002,
        "VertexIndex",
        &[("type", Int), ("indices", Ints)],
    ),
    (
        0x2000_0000,
        "RenderPass",
        &[
            ("sortFunc", Int),
            ("staticSortIndex", Int),
            ("setupHack", Int),
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
            ("operator", Int),
            ("outputScale", Int),
            ("outputBias", Int),
            ("destAB", Int),
            ("destCD", Int),
            ("destABCD", Int),
            ("inputs", Many),
        ],
    ),
    (
        0x3000_0002,
        "CombinerInput",
        &[("source", Int), ("map", Int), ("isAlpha", Bool)],
    ),
    (
        0x3000_0003,
        "CombinerContext",
        &[
            ("vertColor0Dest", Int),
            ("vertColor1Dest", Int),
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
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
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
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
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
        &[("srcBlend", Int), ("destBlend", Int)],
    ),
    (
        0x1000_0023,
        "MayaTransform",
        &[
            ("scalePivot", Vector(3)),
            ("scalePivotTranslate", Vector(3)),
            ("scale", Vector(3)),
            ("rotationPivot", Vector(3)),
            ("rotationPivotTranslate", Vector(3)),
            ("rotationOrientation", Vector(3)),
            ("rotation", Vector(3)),
            ("translation", Vector(3)),
            ("children", Many),
        ],
    ),
    (
        0x1000_0024,
        "MayaJoint",
        &[
            ("scale", Vector(3)),
            ("rotationOrientation", Vector(3)),
            ("rotation", Vector(3)),
            ("jointOrientation", Vector(3)),
            ("translation", Vector(3)),
            ("children", Many),
        ],
    ),
    (
        0x1000_0037,
        "MiscParams",
        &[
            ("0", Bool),
            ("1", Bool),
            ("2", Bool),
            ("3", Vector(4)),
            ("4", Bool),
            ("5", Bool),
            ("6", Float),
            ("7", Float),
            ("8", Int),
            ("9", Bool),
            ("10", Float),
            ("11", Float),
            ("12", Float),
            ("13", Vector(3)),
            ("14", Bool),
            ("15", Bool),
            ("16", Bool),
            ("17", Bool),
            ("18", Bool),
            ("19", Float),
            ("20", Float),
        ],
    ),
    (
        0x4100_0000,
        "TextureTransform",
        &[
            ("scalePivot", Vector(3)),
            ("scale", Vector(3)),
            ("rotationPivot", Vector(3)),
            ("rotationOrientation", Vector(4)),
            ("rotation", Vector(4)),
            ("translation", Vector(3)),
            ("fov", Float),
            ("aspectRatio", Float),
            ("near", Float),
            ("far", Float),
        ],
    ),
    (
        0x6000_0000,
        "Container",
        &[("speed", Float), ("channels", Many)],
    ),
    (
        0x6000_0311,
        "Channel",
        &[
            ("targetNode", Str),
            ("targetField", Str),
            ("xCurve", One),
            ("yCurve", One),
            ("zCurve", One),
        ],
    ),
    (
        0x6000_0115,
        "Curve",
        &[
            ("startTime", Int),
            ("stopTime", Int),
            ("preStart", Int),
            ("postStop", Int),
            ("interpMethod", Int),
            ("keyframes", Many),
        ],
    ),
    (0x6000_0110, "Keyframe", &[("time", Int), ("data", Key)]),
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
                Int => Value::Int(self.int()?),
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
                Key => {
                    let mut f = self.floats(1)?;
                    self.take(8)?;
                    f.extend(self.floats(4)?);
                    Value::Floats(f)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_to_the_end() {
        let mut b: Vec<u8> = Vec::new();
        for v in [0x1000_0021, 0, 1, 0x1000_0040, -3i32 as u32, 1] {
            b.extend(v.to_le_bytes());
        }
        b.push(b'a');
        for _ in 0..2 {
            b.extend((-4i32).to_le_bytes());
        }
        let n = read_binary(&b).unwrap();
        assert_eq!(
            (n.kind, n.nodes("children")[0].name.as_str()),
            ("Group", "a")
        );
        assert!(read_binary(&b[..b.len() - 1]).is_err());
        assert!(read_binary(&[&b[..], &[0]].concat()).is_err());
    }
}
