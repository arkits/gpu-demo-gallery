//! Timbury's scene files: `.nmb` (binary) and `.nma` (the same trees as text).
//!
//! A node has a kind, perhaps a name, and fields. The tables below come from the
//! viewer's own registrations (BugLooker.exe `FUN_004b6ea0` and friends, found
//! through REA/Ghidra and never run; [I] below marks inference). Binary field ids
//! are the registration order; text matches fields by name. Both readers are
//! strict: binary reads to the last byte, text to the last word.
use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Type {
    Int,
    /// One byte.
    Bool,
    Float,
    Double,
    Vector(usize),
    /// A counted list of float vectors.
    Floats(usize),
    /// A counted list of four-byte values read as bytes (color bytes).
    Bytes(usize),
    /// A counted list of raw bytes.
    Raw,
    /// A counted list of four-byte integers.
    Ints,
    /// A fixed list of four-byte integers.
    IVec(usize),
    /// A counted list of two-byte indices.
    Shorts,
    /// Time, value, two tangent kinds, then in x, in y, out x, out y.
    Keys,
    Str,
    Strs,
    One,
    Many,
    /// A child node that the binary writer omits when empty (RenderPass
    /// field 13, `overrideSettings`): parsed when its id is next [I].
    OptOne,
    /// A string the binary writer omits when empty (Shape callbacks).
    OptStr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u32),
    Bool(bool),
    Float(f32),
    /// One vector, or a list of vectors end to end.
    Floats(Vec<f32>),
    Ints(Vec<u32>),
    Bytes(Vec<u8>),
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

/// Id, kind and fields in registration order. From the viewer's registrations;
/// the order is the binary field order.
const KINDS: Schema = &[
    (
        0x00000001,
        "Scene",
        &[
            ("sounds", Many),
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
            ("menus", Many),
            ("deformers", Many),
        ],
    ),
    (
        0x10000020,
        "FloatArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(1)),
        ],
    ),
    (
        0x10000021,
        "Pnt2fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(2)),
        ],
    ),
    (
        0x10000022,
        "Pnt3fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(3)),
        ],
    ),
    (
        0x10000023,
        "Pnt4fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(4)),
        ],
    ),
    (
        0x10000024,
        "Vec2fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(2)),
        ],
    ),
    (
        0x10000025,
        "Vec3fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(3)),
        ],
    ),
    (
        0x10000026,
        "Color3fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(3)),
        ],
    ),
    (
        0x10000027,
        "Color4fArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Floats(4)),
        ],
    ),
    (
        0x10000028,
        "Color4ucArray",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("value", Bytes(4)),
        ],
    ),
    (
        0x10000029,
        "StridedData",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("subChannelNames", Strs),
            ("subChannelSizes", Many),
            ("value", Raw),
        ],
    ),
    (
        0x20000100,
        "2DTexture",
        &[("filename", Str), ("format", Int)],
    ),
    (
        0x20000101,
        "ProcTexture",
        &[
            ("dimensions", Vector(2)),
            ("format", Int),
            ("numSamples", Int),
        ],
    ),
    (
        0x20000200,
        "3DTexture",
        &[("filename", Str), ("format", Int)],
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
            ("format", Int),
        ],
    ),
    (
        0x20000301,
        "CubicCrossTexture",
        &[("filename", Str), ("format", Int)],
    ),
    (
        0x20000303,
        "ProcCubicTexture",
        &[("faceSize", Int), ("format", Int)],
    ),
    (
        0x23000000,
        "Background",
        &[("renderSettings", One), ("fovScale", Float)],
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
        0x2a000001,
        "FrustumCamera",
        &[
            ("useScreenAspectRatio", Bool),
            ("near", Float),
            ("far", Float),
            ("left", Float),
            ("right", Float),
            ("bottom", Float),
            ("top", Float),
            ("position", Vector(3)),
            ("orientation", Vector(4)),
            ("transform", Str),
        ],
    ),
    (
        0x30000001,
        "DirectionalLight",
        &[
            ("transform", Str),
            ("color", Vector(3)),
            ("intensity", Float),
            ("isEyespace", Bool),
            ("direction", Vector(3)),
        ],
    ),
    (
        0x30000002,
        "PointLight",
        &[
            ("transform", Str),
            ("color", Vector(3)),
            ("intensity", Float),
            ("isEyespace", Bool),
            ("position", Vector(3)),
            ("attenuation", Vector(3)),
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
        &[("paramName", Str), ("texture", Str), ("textureState", Str)],
    ),
    (
        0x41000003,
        "TransformParam",
        &[
            ("paramName", Str),
            ("transform", Str),
            ("renderPass", Str),
            ("map", Int),
        ],
    ),
    (
        0x41000004,
        "LightParam",
        &[
            ("paramName", Str),
            ("light", Str),
            ("property", Int),
            ("space", Int),
            ("mapping", Int),
        ],
    ),
    (
        0x41000005,
        "MiscParam",
        &[("paramName", Str), ("param", Int)],
    ),
    (
        0x41000007,
        "WireDeformParam",
        &[("paramName", Str), ("wire", Str)],
    ),
    (
        0x50000011,
        "BlendParams",
        &[
            ("colorSrcBlend", Int),
            ("colorDestBlend", Int),
            ("alphaSrcBlend", Int),
            ("alphaDestBlend", Int),
            ("blendOp", Int),
            ("blendColor", Vector(4)),
            ("redMask", Bool),
            ("greenMask", Bool),
            ("blueMask", Bool),
            ("alphaMask", Bool),
        ],
    ),
    (
        0x50000012,
        "CullParams",
        &[("ccwIsFront", Bool), ("cullFace", Int)],
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
        0x50000014,
        "AlphaParams",
        &[
            ("alphaTestEnabled", Bool),
            ("alphaTestFunc", Int),
            ("alphaTestRef", Float),
        ],
    ),
    (
        0x50000016,
        "StencilParams",
        &[
            ("stencilEnabled", Bool),
            ("stencilFuncMask", Int),
            ("stencilFunc", Int),
            ("stencilRef", Int),
            ("stencilOp", Int),
            ("stencilZPassOp", Int),
            ("stencilZFailOp", Int),
            ("stencilWriteMask", Int),
        ],
    ),
    (
        0x50000017,
        "RasterParams",
        &[
            ("pointStyle", Int),
            ("lineWidth", Float),
            ("drawStyle", Int),
        ],
    ),
    (
        0x50000018,
        "TextureParams",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("wrapU", Int),
            ("wrapV", Int),
            ("wrapW", Int),
            ("mipmapBias", Float),
            ("anisotropy", Float),
            ("enableShadowCompare", Bool),
        ],
    ),
    (
        0x60000020,
        "Shape",
        &[
            ("cull", Int),
            ("sphereCenter", Vector(3)),
            ("sphereRadius", Float),
            ("transform", Str),
            ("renderSettings", Many),
            ("geometries", Strs),
            ("preDrawCallback", OptStr),
            ("postDrawCallback", OptStr),
        ],
    ),
    (
        0x60000021,
        "ShapeLOD",
        &[("distances", Floats(1)), ("children", Many)],
    ),
    (0x60000022, "ShapeGroup", &[("children", Many)]),
    (
        0x70000031,
        "Mesh",
        &[
            ("useRestartPrimIndex", Bool),
            ("data", Many),
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
            ("cullThreshold", Float),
            ("colorMapFile", Str),
        ],
    ),
    (
        0x70000036,
        "HeightField",
        &[
            ("useRestartPrimIndex", Bool),
            ("heightImage", Str),
            ("dimensions", Vector(2)),
            ("textureRepeat", Float),
            ("tesselation", Int),
        ],
    ),
    (
        0x70000037,
        "SubdivisionMesh",
        &[
            ("useRestartPrimIndex", Bool),
            ("normalizeVectors", Bool),
            ("subdivisionPool", Int),
            ("adaptiveTessellation", Bool),
            ("staticSubdivisionLevel", Int),
            ("tessellationPrecision", Float),
            ("clipping", Bool),
            ("backFaceCulling", Bool),
            ("isCatmullClark", Bool),
            ("vertexBufferSize", Int),
            ("processParams", Many),
            ("data", Many),
            ("indices", Many),
        ],
    ),
    (
        0x70000038,
        "ProcessedMesh",
        &[
            ("useRestartPrimIndex", Bool),
            ("normalizeVectors", Bool),
            ("processParams", Many),
            ("data", Many),
            ("indices", Many),
        ],
    ),
    (
        0x700005e7,
        "Grass",
        &[
            ("bladeSize", Float),
            ("density", Float),
            ("segmentsPerBlade", Int),
            ("grassY", Float),
            ("grassMinX", Float),
            ("grassMinZ", Float),
            ("grassMaxX", Float),
            ("grassMaxZ", Float),
            ("lifeMapNoise", Float),
            ("minBend", Float),
            ("maxBend", Float),
            ("windStrength", Float),
            ("windIrregularity", Int),
            ("windDirection", Float),
            ("grassTexture", Str),
            ("grassLifeMapTex", Str),
            ("grassTerrainGeom", Str),
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
            ("rotationOrder", Int),
            ("children", Many),
        ],
    ),
    (
        0x80000003,
        "MatrixTransform",
        &[("matrix", Vector(16)), ("children", Many)],
    ),
    (
        0x80000004,
        "MaxTransform",
        &[
            ("rotation", Vector(3)),
            ("scaleRotation", Vector(3)),
            ("scale", Vector(3)),
            ("translation", Vector(3)),
            ("sign", Float),
            ("children", Many),
        ],
    ),
    (
        0x80000200,
        "WireDeformer",
        &[
            ("scale", Vector(3)),
            ("dropoffDistance", Float),
            ("CVs", Ints),
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
        0xa0000120,
        "GeometryKeyframe",
        &[("time", Float), ("data", Many)],
    ),
    (
        0xa0000131,
        "TransformKeyframe",
        &[("time", Float), ("transform", Str)],
    ),
    (
        0xa0000311,
        "MayaFloatChannel",
        &[
            ("targetNodes", Strs),
            ("targetFields", Strs),
            ("curve", One),
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
            ("keyframes", Many),
        ],
    ),
    (
        0xa0000316,
        "TransformChannel",
        &[
            ("target", Str),
            ("keyframeInterpMethod", Int),
            ("keyframes", Many),
        ],
    ),
    (
        0xb0000000,
        "Sound",
        &[
            ("filename", Str),
            ("loop", Bool),
            ("volume", Float),
            ("play", Bool),
        ],
    ),
    (
        0xb0000001,
        "PositionalSound",
        &[
            ("transform", Str),
            ("filename", Str),
            ("loop", Bool),
            ("volume", Float),
            ("position", Vector(3)),
            ("minDistance", Float),
            ("maxDistance", Float),
            ("umbra", Vector(3)),
            ("penumbra", Vector(3)),
            ("direction", Vector(3)),
            ("outerVolume", Vector(3)),
        ],
    ),
    (
        0xb0000002,
        "StreamingSound",
        &[
            ("filename", Str),
            ("loop", Bool),
            ("volume", Float),
            ("play", Bool),
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
            ("overrideSettings", OptOne),
            ("useListIndices", Bool),
            ("cubeFace", Int),
            ("viewport", IVec(4)),
        ],
    ),
    (
        0xd0000001,
        "Slider",
        &[
            ("callback", Str),
            ("value", Float),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("drawtext", Bool),
            ("textcolor", Vector(3)),
            ("slider", Str),
            ("knob", Str),
        ],
    ),
    (
        0xd0000002,
        "Icon",
        &[("image", Str), ("position", Vector(2)), ("size", Vector(2))],
    ),
    (
        0xd0000010,
        "DropDownMenu",
        &[
            ("menuItems", Many),
            ("openPosition", Vector(2)),
            ("openSize", Vector(2)),
            ("buttonImage", Str),
            ("initiallyOpen", Bool),
        ],
    ),
    (0xd0000011, "StaticMenu", &[("menuItems", Many)]),
    (
        0xd0000021,
        "OnOffButton",
        &[
            ("callback", Str),
            ("initial", Bool),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("onColor", Vector(3)),
            ("offColor", Vector(3)),
            ("buttonImage", Str),
        ],
    ),
    (
        0xd0000022,
        "UpDownButton",
        &[
            ("callback", Str),
            ("initial", Bool),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("buttonImageOn", Str),
            ("buttonImageOff", Str),
        ],
    ),
    (
        0xd0000031,
        "OnOffRadioGroup",
        &[
            ("callback", Str),
            ("initial", Bool),
            ("alignment", Int),
            ("spacing", Float),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("onColor", Vector(3)),
            ("offColor", Vector(3)),
            ("radioList", Strs),
        ],
    ),
    (
        0xd0000032,
        "UpDownRadioGroup",
        &[
            ("callback", Str),
            ("initial", Bool),
            ("alignment", Int),
            ("spacing", Float),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("radioListOn", Strs),
            ("radioListOff", Strs),
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
    fn doubles(&mut self, n: usize) -> Result<Vec<f32>, Error> {
        let b = self.take(n.checked_mul(8).ok_or("bad count")?)?;
        let f = |c: &[u8]| f64::from_le_bytes(c.try_into().unwrap()) as f32;
        Ok(b.chunks_exact(8).map(f).collect())
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
            // Empty collections are omitted from the file, so ids may skip [I].
            let (name, ty) = schema
                .get(field as usize)
                .ok_or_else(|| format!("unknown field {field} of {kind} at {at:#x}"))?;
            let value = match ty {
                Int => Value::Int(self.int()?),
                Bool => Value::Bool(self.take(1)?[0] != 0),
                Float => Value::Float(self.floats(1)?[0]),
                Double => Value::Float(self.doubles(1)?[0]),
                Vector(n) => Value::Floats(self.floats(*n)?),
                Floats(n) => {
                    let count = self.count()?;
                    Value::Floats(self.floats(count * n)?)
                }
                Bytes(n) => {
                    let count = self.count()?;
                    Value::Bytes(self.take(count * n)?.to_vec())
                }
                Raw => {
                    let count = self.count()?;
                    Value::Bytes(self.take(count)?.to_vec())
                }
                Ints => {
                    let count = self.count()?;
                    let b = self.take(count.checked_mul(4).ok_or("bad count")?)?;
                    Value::Ints(
                        b.chunks_exact(4)
                            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect(),
                    )
                }
                IVec(n) => {
                    let b = self.take(n.checked_mul(4).ok_or("bad count")?)?;
                    Value::Ints(
                        b.chunks_exact(4)
                            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect(),
                    )
                }
                Str | OptStr => Value::Text(self.text()?),
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
                    let key = |k: &[u8]| [0, 4, 8, 12, 16, 20, 24, 28].map(|o| float(&k[o..]));
                    Value::Floats(keys.chunks_exact(32).flat_map(key).collect())
                }
                One | OptOne => Value::Node(Box::new(self.node(depth + 1)?)),
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
    /// Splits text without comments.
    fn split(text: &'a str) -> Vec<&'a str> {
        let mut words = Vec::new();
        let mut rest = text;
        loop {
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
        words
    }

    fn new(text: &'a str) -> Self {
        let mut words = Vec::new();
        let mut rest = text;
        // Block comments first; none hide in quoted strings.
        while let Some(start) = rest.find("/*") {
            Self::push_line_commented(&mut words, &rest[..start]);
            rest = rest[start..]
                .find("*/")
                .map_or("", |e| &rest[start + e + 2..]);
        }
        Self::push_line_commented(&mut words, rest);
        Self { words, at: 0 }
    }

    /// Words without `//` comments, which run to the end of the line.
    fn push_line_commented(words: &mut Vec<&'a str>, mut rest: &'a str) {
        while let Some(comment) = rest.find("//") {
            let end = rest[comment..]
                .find('\n')
                .map_or(rest.len(), |i| comment + i);
            words.extend(Self::split(&rest[..comment]));
            rest = &rest[end..];
        }
        words.extend(Self::split(rest));
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
    fn number(&mut self) -> Result<f32, Error> {
        let w = self.next();
        w.parse().map_err(|_| format!("not a number: {w:?}").into())
    }
    fn word(&mut self) -> String {
        self.next().trim_matches('"').to_owned()
    }
    /// An integer or one of the viewer's enum words [I].
    fn int(&mut self) -> Result<u32, Error> {
        let w = self.next();
        if let Ok(n) = w.parse::<i32>() {
            return Ok(n as u32);
        }
        let n = match w.trim_matches('"') {
            "tri_list" => 3,
            "tri_strip" => 5,
            "never" => 0,
            "static" => 1,
            "none" => 2,
            "front_to_back" => 0,
            "back_to_front" => 1,
            "xform" => 7,
            "xform_view" => 4,
            "xform_view_proj" => 26,
            "view_proj_win" => 5,
            "camObjVec" => 3,
            "camWorldPos" => 4,
            "direction" => 0,
            "color" => 0,
            "object" => 1,
            "world" => 2,
            "eye" => 3,
            "identity" => 0,
            "default" => 24,
            "signed_rgba" => 6,
            "bgra" => 5,
            "float4_16" => 3,
            "depth32" => 14,
            "gl_vertex_20" => 6144,
            "gl_vertex_30" => 6148,
            "gl_fragment_20" => 6145,
            "gl_fragment_30" => 6149,
            "posx" => 5,
            "negz" => 5,
            "xyz" => 0,
            "linesNormUV" => 0,
            "cycle" => 2,
            "hermite" => 1,
            "constant" => 0,
            "add" => 1,
            "always" => 1,
            "src_alpha" => 4,
            "inv_src_alpha" => 5,
            "linear" => 1,
            "nearest" => 0,
            "clamp" => 0,
            "repeat" => 1,
            _ => return Err(format!("not a number: {w:?}").into()),
        };
        Ok(n)
    }

    fn node(&mut self, depth: usize) -> Result<Node, Error> {
        if depth > 64 {
            return Err("nodes are nested too deeply".into());
        }
        let name = self.next();
        let (_, kind, schema) = KINDS
            .iter()
            .find(|k| {
                k.1 == name
                    || (k.1 == "Mesh" && name == "IndexedFaceSet")
                    || (k.1 == "IndexedFaceSet" && name == "Mesh")
            })
            .ok_or_else(|| format!("unknown node {name:?}"))?;
        let mut node = Node {
            kind,
            name: String::new(),
            fields: BTreeMap::new(),
        };
        if self.peek() != "{" {
            node.name = self.word();
        }
        self.expect("{")?;
        while self.peek() != "}" {
            let word = self.next();
            let (field, ty) = schema
                .iter()
                .find(|f| f.0.eq_ignore_ascii_case(word))
                .ok_or_else(|| format!("unknown field {word:?} of {kind}"))?;
            let value = match ty {
                Int => Value::Int(self.int()?),
                Bool => Value::Bool(self.number()? != 0.0),
                Float | Double => Value::Float(self.number()?),
                Vector(n) => {
                    Value::Floats((0..*n).map(|_| self.number()).collect::<Result<_, _>>()?)
                }
                Type::IVec(n) => {
                    Value::Ints((0..*n).map(|_| self.int()).collect::<Result<_, _>>()?)
                }
                Str | OptStr => Value::Text(self.word()),
                One | OptOne => Value::Node(Box::new(self.node(depth + 1)?)),
                Floats(_) | Bytes(_) | Raw | Type::Ints | Strs | Shorts | Keys | Many => {
                    self.expect("[")?;
                    let (mut numbers, mut texts, mut nodes) = (vec![], vec![], vec![]);
                    while self.peek() != "]" {
                        match ty {
                            Many => nodes.push(self.node(depth + 1)?),
                            Strs => texts.push(self.word()),
                            _ => numbers.push(self.number()?),
                        }
                    }
                    self.expect("]")?;
                    match ty {
                        Many => Value::Nodes(nodes),
                        Strs => Value::Texts(texts),
                        Type::Ints | Type::IVec(_) | Shorts => {
                            Value::Ints(numbers.into_iter().map(|f| f as u32).collect())
                        }
                        _ => Value::Floats(numbers),
                    }
                }
            };
            node.fields.insert(field, value);
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
