//! The demo's binary scenes (`.nmb`): trees of nodes with a kind, perhaps a name,
//! and fields. Kind ids, field order and types were read from `PirateShip.exe`
//! with REA and Ghidra (never run); `[I]` marks inferences. Every byte of
//! `PS_03_19.nmb` and `sail_animation.nmb` must be understood.
use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Type {
    Int,
    Bool,
    Float,
    Double,
    Vector(usize),
    /// A list of vectors of 32-bit floats.
    Floats(usize),
    /// A list of vectors of 16-bit floats.
    Halves(usize),
    /// Fixed 32-bit integers, e.g. dimensions and viewports (no count).
    Ints(usize),
    Str,
    Strs,
    One,
    Many,
    Shorts,
    /// Time, value, tangent kinds (skipped) and tangents.
    Keys,
    /// GeometryChannel keyframes: a count, then morph targets, each a
    /// `0xa0000120` node (index 0, time as double, index 1, weight index, a
    /// positions array, a normals array, end). [I: layout from the data]
    Keyframes,
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
type Schema = &'static [(u32, &'static str, Fields)];

/// Names come from the program's own strings; ids and order from its
/// constructors (e.g. `FUN_00468430` for RenderPass, `FUN_00484280` for
/// GeometryChannel). Every kind below appears in the demo's two scenes.
const KINDS: Schema = &[
    (
        0x00000001,
        "Scene",
        &[
            ("sceneInfo", Str),
            ("backgrounds", Many),
            ("lights", Many),
            ("cameras", Many),
            ("shaders", Many),
            ("textures", Many),
            ("renderParams", Many),
            ("renderPasses", Many),
            ("transforms", Many),
            ("shapes", Many),
            ("geometries", Many),
            ("animations", Many),
            ("sounds", Many),
        ],
    ),
    (
        0x10000020,
        "FloatArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(1)),
        ],
    ),
    (
        0x10000021,
        "Pnt2fArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(2)),
        ],
    ),
    (
        0x10000022,
        "Pnt3fArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(3)),
        ],
    ),
    (
        0x10000023,
        "Pnt4fArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(4)),
        ],
    ),
    (
        0x10000024,
        "Vec2fArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(2)),
        ],
    ),
    (
        0x10000025,
        "Pnt3fArrayStatic",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(3)),
        ],
    ),
    (
        0x10000026,
        "Pnt4fArrayStatic",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(4)),
        ],
    ),
    (
        0x10000027,
        "Vec2fArrayStatic",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(2)),
        ],
    ),
    (
        0x10000028,
        "FloatArrayStatic",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Floats(1)),
        ],
    ),
    (
        0x10000030,
        "Pnt2hfArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Halves(2)),
        ],
    ),
    (
        0x10000031,
        "Pnt3hfArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Halves(3)),
        ],
    ),
    (
        0x10000032,
        "Pnt4hfArray",
        &[
            ("paramName", Str),
            ("animationSet", Int),
            ("value", Halves(4)),
        ],
    ),
    (
        0x20000100,
        "2DTexture",
        &[
            ("filename", Str),
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Float),
            ("wrapU", Int),
            ("wrapV", Int),
            ("remapped", Bool),
        ],
    ),
    (
        0x20000101,
        "ProcTexture",
        &[("dimensions", Ints(2)), ("format", Int), ("extra", Int)],
    ),
    (
        0x20000300,
        "CubicTexture",
        &[
            ("xpos", Str),
            ("xneg", Str),
            ("ypos", Str),
            ("yneg", Str),
            ("zpos", Str),
            ("zneg", Str),
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Float),
            ("remapped", Bool),
        ],
    ),
    (
        0x20000301,
        "CubicCrossTexture",
        &[
            ("filename", Str),
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Float),
            ("remapped", Bool),
        ],
    ),
    (
        0x23000000,
        "Background",
        &[
            ("renderSettings", One),
            ("positionScale", Float),
            ("fovScale", Float),
        ],
    ),
    (
        0x2a000000,
        "Camera",
        &[
            ("useScreenAspectRatio", Bool),
            ("near", Float),
            ("minNear", Float),
            ("dynamicNear", Bool),
            ("far", Float),
            ("maxFar", Float),
            ("dynamicFar", Bool),
            ("fieldOfView", Float),
            ("aspectRatio", Float),
            ("position", Vector(3)),
            ("orientation", Vector(4)),
            ("transform", Str),
        ],
    ),
    (
        0x30000003,
        "SpotLight",
        &[
            ("transform", Str),
            ("color", Vector(3)),
            ("intensity", Float),
            ("isEyespace", Bool),
            ("position", Vector(3)),
            ("direction", Vector(3)),
            ("inner", Float),
            ("outer", Float),
            ("attenuation", Vector(3)),
        ],
    ),
    (
        0x40000000,
        "Shader",
        &[
            ("filename", Str),
            ("profile", Int),
            ("entryFunc", Str),
            ("params", Many),
        ],
    ),
    (
        0x41000001,
        "ConstantParam",
        &[
            ("paramName", Str),
            ("value", Vector(4)),
            ("min", Vector(4)),
            ("max", Vector(4)),
        ],
    ),
    (
        0x41000002,
        "TextureParam",
        &[("paramName", Str), ("texture", Str), ("state", Str)],
    ),
    (
        0x41000003,
        "TransformParam",
        &[
            ("paramName", Str),
            ("transform", Str),
            ("renderPass", Str),
            ("property", Int),
        ],
    ),
    (
        0x41000005,
        "MiscParam",
        &[("paramName", Str), ("property", Int)],
    ),
    (
        0x50000013,
        "DepthParams",
        &[
            ("depthTestEnabled", Bool),
            ("depthTestFunc", Int),
            ("depthMask", Bool),
            ("polygonOffsetScale", Float),
            ("polygonOffsetBias", Float),
        ],
    ),
    (
        0x60000020,
        "Shape",
        &[
            ("drawStyle", Int),
            ("sphereCenter", Vector(3)),
            ("sphereRadius", Float),
            ("transform", Str),
            ("renderSettings", Many),
            ("geometries", Strs),
        ],
    ),
    (0x60000022, "Group", &[("children", Many)]),
    (
        0x70000031,
        "IndexedFaceSet",
        &[
            ("useRestartPrimIndex", Bool),
            ("vertices", Many),
            ("indices", Many),
        ],
    ),
    (
        0x70000032,
        "HairPatch",
        &[
            ("useRestartPrimIndex", Bool),
            ("hairFile", Str),
            ("type", Int),
            ("width", Float),
            ("density", Float),
            ("numSegments", Int),
            ("noiseAmplitude", Float),
            ("changeWidth", Bool),
            ("widthDelta", Float),
            ("widthNear", Float),
            ("widthFar", Float),
            ("useAlphaToCoverage", Bool),
        ],
    ),
    (
        0x70050000,
        "VertexIndex",
        &[("primType", Int), ("indices", Shorts)],
    ),
    (
        0x80000001,
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
            ("rotationOrder", Int),
            ("children", Many),
        ],
    ),
    (
        0x80000002,
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
        0x90000000,
        "RenderSettings",
        &[
            ("sortSet", Int),
            ("sortType", Int),
            ("cullParams", Str),
            ("blendParams", Str),
            ("depthParams", Str),
            ("alphaParams", Str),
            ("stencilParams", Str),
            ("rasterParams", Str),
            ("vertexShader", Str),
            ("vertexShaderParams", Many),
            ("fragmentShader", Str),
            ("fragmentShaderParams", Many),
        ],
    ),
    (
        0xa0000000,
        "Animation",
        &[("timeScale", Float), ("channels", Many)],
    ),
    (
        0xa0000115,
        "MayaCurve",
        &[
            ("startTime", Double),
            ("stopTime", Double),
            ("preStart", Int),
            ("postStop", Int),
            ("interpMethod", Int),
            ("keyframes", Keys),
        ],
    ),
    (
        0xa0000311,
        "MayaFloatChannel",
        &[
            ("targetNodes", Strs),
            ("targetFields", Strs),
            ("curve", One),
            ("index", Int),
        ],
    ),
    (
        0xa0000312,
        "MayaVec3fChannel",
        &[
            ("targetNode", Str),
            ("targetField", Str),
            ("xCurve", One),
            ("yCurve", One),
            ("zCurve", One),
            ("index", Int),
        ],
    ),
    (
        0xa0000313,
        "GeometryChannel",
        &[
            ("target", Str),
            ("keyframeInterpMethod", Int),
            ("looping", Bool),
            ("keyframes", Keyframes),
        ],
    ),
    (
        0xc0000000,
        "RenderPass",
        &[
            ("colorTargets", Strs),
            ("depthTarget", Str),
            ("clearsColor", Bool),
            ("clearColor", Vector(4)),
            ("clearsDepth", Bool),
            ("clearDepth", Float),
            ("clearsStencil", Bool),
            ("clearStencil", Int),
            ("camera", Str),
            ("useSceneShapes", Bool),
            ("shapes", Strs),
            ("useSceneBackgrounds", Bool),
            ("backgrounds", Strs),
            ("overrideSettings", One),
            ("useListIndices", Bool),
            ("cubeFace", Int),
            ("viewport", Ints(4)),
        ],
    ),
];

impl Node {
    fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field)
    }
    pub fn bool(&self, field: &str, default: bool) -> bool {
        match self.get(field) {
            Some(Value::Bool(value)) => *value,
            _ => default,
        }
    }
    pub fn float(&self, field: &str) -> f32 {
        match self.get(field) {
            Some(Value::Float(value)) => *value,
            _ => 0.0,
        }
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
    pub fn texts(&self, field: &str) -> &[String] {
        match self.get(field) {
            Some(Value::Texts(value)) => value,
            _ => &[],
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

/// A 16-bit float.
fn half(b: &[u8]) -> f32 {
    let bits = u32::from(u16::from_le_bytes([b[0], b[1]]));
    let (sign, exponent, fraction) = (bits >> 15, bits >> 10 & 31, bits & 1023);
    let value = match exponent {
        0 => fraction as f32 / 16_777_216.0,
        31 => return f32::NAN,
        _ => f32::from_bits((exponent + 112) << 23 | fraction << 13),
    };
    if sign == 1 { -value } else { value }
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
                Double => Value::Float(f64::from_le_bytes(self.take(8)?.try_into()?) as f32),
                Vector(n) => Value::Floats(self.floats(*n)?),
                Floats(n) => {
                    let count = self.count()?;
                    Value::Floats(self.floats(count * n)?)
                }
                Halves(n) => {
                    let count = self.count()?.checked_mul(n * 2).ok_or("bad count")?;
                    Value::Floats(self.take(count)?.chunks_exact(2).map(half).collect())
                }
                Ints(n) => {
                    let b = self.take(n.checked_mul(4).ok_or("bad count")?)?;
                    Value::Ints(
                        b.chunks_exact(4)
                            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                            .collect(),
                    )
                }
                Str => Value::Text(self.text()?),
                Strs => {
                    let count = self.count()?;
                    Value::Texts((0..count).map(|_| self.text()).collect::<Result<_, _>>()?)
                }
                Shorts => {
                    let n = self.count()?;
                    let data = self.take(n.checked_mul(2).ok_or("bad index count")?)?;
                    Value::Ints(
                        data.chunks_exact(2)
                            .map(|p| u16::from_le_bytes([p[0], p[1]]).into())
                            .collect(),
                    )
                }
                Keys => {
                    let count = self.count()?;
                    let keys = self.take(count.checked_mul(32).ok_or("bad count")?)?;
                    let float = |c: &[u8]| f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    let key = |k: &[u8]| [0, 4, 16, 20, 24, 28].map(|o| float(&k[o..]));
                    Value::Floats(keys.chunks_exact(32).flat_map(key).collect())
                }
                Keyframes => {
                    let count = self.count()?;
                    let mut keys = Vec::with_capacity(count);
                    for _ in 0..count {
                        let kat = self.at;
                        let kid = self.int()?;
                        if kid != 0xa0000120 {
                            return Err(format!("bad keyframe {kid:#x} at {kat:#x}").into());
                        }
                        if self.int()? != 0 {
                            return Err(format!("bad keyframe head at {kat:#x}").into());
                        }
                        let time = f64::from_le_bytes(self.take(8)?.try_into()?) as f32;
                        if self.int()? != 1 {
                            return Err(format!("bad keyframe middle at {kat:#x}").into());
                        }
                        let index = self.int()?;
                        let coord = Box::new(self.node(depth + 1)?);
                        let normal = Box::new(self.node(depth + 1)?);
                        if self.int()? as i32 != -4 {
                            return Err(format!("unterminated keyframe at {kat:#x}").into());
                        }
                        let mut fields = BTreeMap::new();
                        fields.insert("time", Value::Float(time));
                        fields.insert("index", Value::Int(index));
                        fields.insert("coord", Value::Node(coord));
                        fields.insert("normal", Value::Node(normal));
                        keys.push(Node {
                            kind: "MorphKeyframe",
                            name: String::new(),
                            fields,
                        });
                    }
                    Value::Nodes(keys)
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
    fn rejects_unread_data() {
        let mut data = 1u32.to_le_bytes().to_vec();
        data.extend((-4_i32).to_le_bytes());
        assert_eq!(read_binary(&data).unwrap().kind, "Scene");
        data.push(0);
        assert!(read_binary(&data).is_err());
        data.truncate(6);
        assert!(read_binary(&data).is_err());
    }
}
