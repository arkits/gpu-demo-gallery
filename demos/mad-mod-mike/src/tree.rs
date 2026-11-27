//! MadModMike's scene (`.nmb`) and scene fragment (`.nma`) files: trees of nodes
//! with a kind, perhaps a name, and fields.
//!
//! The binary tables below are the viewer's own: node ids, kind names and field
//! order come from its registration code (`REA decompile-all`, e.g. `0x4d17d0`
//! for `Scene`, `0x50f190` for `PointLight`, `0x4f07f0` for `ProcessedMesh`).
//! [I] marks field types fitted against the shipped files, read strictly to
//! the last byte. The text reader maps the `.nma` words onto the same model;
//! enum words use the binary values wherever the shipped files pin them down.
use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Type {
    Int,
    Bool,
    Float,
    Double,
    Vector(usize),
    /// A count followed by that many groups of floats.
    Floats(usize),
    /// An animation-set index followed by four floats (an animated value).
    SetVec,
    Str,
    Strs,
    /// A count followed by that many `u32`s.
    Ints,
    One,
    Many,
    Shorts,
    /// Time, value, tangent kinds (skipped) and tangents.
    Keys,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u32),
    Bool(bool),
    Float(f32),
    /// One vector, or a list of vectors end to end.
    Floats(Vec<f32>),
    Ints(Vec<u32>),
    /// An animation-set index with its four-float value.
    Pair(u32, [f32; 4]),
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

/// Node ids, kind names and field order from the viewer's registration
/// functions; see the module docs.
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
            ("renderSettings", Many),
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
            ("value", Floats(4)),
        ],
    ),
    (
        0x10000029,
        "StridedData",
        &[
            ("channelName", Str),
            ("animationSet", Int),
            ("subChannelNames", Strs),
            ("subChannelSizes", Ints),
            ("value", Floats(1)),
        ],
    ),
    (
        0x20000100,
        "2DTexture",
        &[("filename", Str), ("format", Int), ("numMipmaps", Int)],
    ),
    (
        0x20000101,
        "ProcTexture",
        &[
            ("dimensions", Vector(2)),
            ("format", Int),
            ("numMipmaps", Int),
            ("numSamples", Int),
        ],
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
            ("numMipmaps", Int),
        ],
    ),
    (
        0x20000301,
        "CubicCrossTexture",
        &[("filename", Str), ("format", Int), ("numMipmaps", Int)],
    ),
    (
        0x20000303,
        "ProcCubicTexture",
        &[("faceSize", Int), ("format", Int), ("numMipmaps", Int)],
    ),
    (
        0x23000000,
        "Background",
        &[
            ("renderSettings", Str),
            ("vertexShaderParams", Many),
            ("fragmentShaderParams", Many),
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
    (0x3f000000, "ButtonCB", &[]),
    (0x3f800000, "AnimationProgCB", &[("name", Str)]),
    (
        0x40000000,
        "Shader",
        &[
            ("filename", Str),
            ("profile", Int),
            ("entryFunc", Str),
            ("globalParams", Many),
            ("defaultParams", Many),
        ],
    ),
    (
        0x41000001,
        "ConstantParam",
        &[
            ("paramName", Str),
            ("value", SetVec),
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
        &[
            ("paramName", Str),
            ("param", Int),
            ("referencedObject", Str),
        ],
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
            ("sortSet", Int),
            ("sortType", Int),
            ("transform", Str),
            ("renderSettings", Str),
            ("geometry", Str),
            ("vertexShaderParams", Many),
            ("fragmentShaderParams", Many),
            ("preDrawCallback", Str),
            ("postDrawCallback", Str),
            ("nodeHistory", Str),
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
        "IndexedFaceSet",
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
            ("dimensions", Vector(3)),
            ("textureRepeat", Vector(2)),
            ("tesselation", Vector(2)),
        ],
    ),
    (
        0x70000037,
        "SubdivisionMesh",
        &[
            ("isActive", Bool),
            ("useRestartPrimIndex", Bool),
            ("normalizeVectors", Bool),
            ("cpuOutputSpace", Int),
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
            ("isActive", Bool),
            ("useRestartPrimIndex", Bool),
            ("normalizeVectors", Bool),
            ("cpuOutputSpace", Int),
            ("processParams", Many),
            ("data", Many),
            ("indices", Many),
            ("writeToGPU", Bool),
        ],
    ),
    (
        0x70000050,
        "ParticleSystem",
        &[
            ("force", Vector(3)),
            ("dragMultPerSec", Float),
            ("isPointSprite", Bool),
            ("streakExtraSegments", Int),
            ("clipToScreen", Bool),
            ("streakLength", Float),
            ("bounceY", Float),
            ("bounceX", Vector(2)),
            ("bounceZ", Vector(2)),
            ("emitters", Many),
        ],
    ),
    (
        0x70000051,
        "ParticleEmitter",
        &[
            ("transform", Str),
            ("randOffsetMinLS", Vector(3)),
            ("randOffsetMaxLS", Vector(3)),
            ("randOffsetMinWS", Vector(3)),
            ("randOffsetMaxWS", Vector(3)),
            ("direction", Vector(3)),
            ("speedVariance", Float),
            ("lifespan", Float),
            ("lifespanVariance", Float),
            ("spread", Float),
            ("rate", Float),
            ("velTransferFromTransformMvmt", Float),
            ("brightScale", Float),
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
            ("sign", Vector(3)),
            ("children", Many),
        ],
    ),
    (
        0x80000200,
        "WireDeformer",
        &[("scale", Float), ("dropoffDistance", Float), ("cvs", Strs)],
    ),
    (
        0x90000000,
        "RenderSettings",
        &[
            ("cullParams", Str),
            ("blendParams", Str),
            ("depthParams", Str),
            ("alphaParams", Str),
            ("stencilParams", Str),
            ("rasterParams", Str),
            ("vertexShader", Str),
            ("fragmentShader", Str),
        ],
    ),
    (
        0xa0000000,
        "Animation",
        &[("timeScale", Float), ("channels", Many)],
    ),
    (
        0xa0000115,
        "MayaCurveRegular",
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
        0xa0000116,
        "MayaCurveLite",
        &[
            ("startTime", Double),
            ("stopTime", Double),
            ("preStart", Int),
            ("postStop", Int),
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
            ("looping", Int),
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
            ("umbra", Float),
            ("penumbra", Float),
            ("direction", Vector(3)),
            ("outerVolume", Float),
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
            ("overrideSettings", Str),
            ("useListIndices", Bool),
            ("cubeFace", Int),
            ("viewport", Vector(4)),
            ("active", Bool),
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
            ("fragmentShaderParams", Many),
        ],
    ),
    (
        0xd0000002,
        "Icon",
        &[
            ("image", Str),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("fragmentShaderParams", Many),
        ],
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
            ("fragmentShaderParams", Many),
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
            ("fragmentShaderParams", Many),
        ],
    ),
    (
        0xd0000031,
        "OnOffRadioGroup",
        &[
            ("callback", Str),
            ("initial", Int),
            ("alignment", Int),
            ("spacing", Float),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("onColor", Vector(3)),
            ("offColor", Vector(3)),
            ("radioList", Strs),
            ("fragmentShaderParams", Many),
        ],
    ),
    (
        0xd0000032,
        "UpDownRadioGroup",
        &[
            ("callback", Str),
            ("initial", Int),
            ("alignment", Int),
            ("spacing", Float),
            ("position", Vector(2)),
            ("size", Vector(2)),
            ("radioListOn", Strs),
            ("radioListOff", Strs),
            ("fragmentShaderParams", Many),
        ],
    ),
];

/// Node kinds that only appear in the `.nma` text, with no binary id.
const TEXTONLY: &[(&str, Fields)] = &[
    (
        "Mesh",
        &[
            ("useRestartPrimIndex", Bool),
            ("data", Many),
            ("indices", Many),
        ],
    ),
    (
        "3DTexture",
        &[("filename", Str), ("format", Int), ("numMipmaps", Int)],
    ),
];

/// Field tables where the `.nma` text differs from the binary: the text
/// exporter wrote fewer fields. The text reader tries these first.
const TEXTKINDS: &[(&str, Fields)] = &[
    (
        "MayaFloatChannel",
        &[
            ("targetNodes", Strs),
            ("targetFields", Strs),
            ("curve", One),
        ],
    ),
    (
        "ConstantParam",
        &[
            ("paramName", Str),
            ("value", Vector(4)),
            ("min", Vector(4)),
            ("max", Vector(4)),
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
            let (name, ty) = schema.get(field as usize).ok_or_else(|| {
                format!("unknown field {field} of {kind} {:?} at {at:#x}", node.name)
            })?;
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
                SetVec => {
                    let set = self.int()?;
                    let v = self.floats(4)?;
                    Value::Pair(set, v.try_into().unwrap())
                }
                Str => Value::Text(self.text()?),
                Strs => {
                    let count = self.count()?;
                    Value::Texts((0..count).map(|_| self.text()).collect::<Result<_, _>>()?)
                }
                Ints => {
                    let count = self.count()?;
                    Value::Ints(self.ints(count)?)
                }
                One => Value::Node(Box::new(self.node(depth + 1)?)),
                Many => {
                    let count = self.count()?;
                    let nodes = (0..count).map(|_| self.node(depth + 1));
                    Value::Nodes(nodes.collect::<Result<_, _>>()?)
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

/// Traces `(offset, kind, name)` of every node read; on error the trace ends
/// where parsing got stuck. A test and debugging aid, not used by the player.
#[allow(clippy::type_complexity)]
pub fn trace_binary(data: &[u8]) -> (Vec<(usize, String, String)>, Result<Node, Error>) {
    struct T<'a> {
        b: Bytes<'a>,
        trace: Vec<(usize, String, String)>,
    }
    impl<'a> T<'a> {
        fn node(&mut self, depth: usize) -> Result<Node, Error> {
            if depth > 64 {
                return Err("nodes are nested too deeply".into());
            }
            let at = self.b.at;
            let id = self.b.int()?;
            let (_, kind, schema) = KINDS
                .iter()
                .find(|k| k.0 == id)
                .ok_or_else(|| format!("unknown node {id:#x} at {at:#x}"))?;
            let mut node = Node {
                kind,
                name: String::new(),
                fields: BTreeMap::new(),
            };
            self.trace.push((at, kind.to_string(), String::new()));
            loop {
                let at = self.b.at;
                let field = self.b.int()?;
                match field as i32 {
                    -4 => {
                        return Ok(node);
                    }
                    -3 => {
                        node.name = self.b.text()?;
                        continue;
                    }
                    _ => {}
                }
                let (name, ty) = schema.get(field as usize).ok_or_else(|| {
                    format!("unknown field {field} of {kind} {:?} at {at:#x}", node.name)
                })?;
                let value = match ty {
                    Int => Value::Int(self.b.int()?),
                    Bool => Value::Bool(self.b.take(1)?[0] != 0),
                    Float => Value::Float(self.b.floats(1)?[0]),
                    Double => Value::Float(f64::from_le_bytes(self.b.take(8)?.try_into()?) as f32),
                    Vector(n) => Value::Floats(self.b.floats(*n)?),
                    Floats(n) => {
                        let count = self.b.count()?;
                        Value::Floats(self.b.floats(count * n)?)
                    }
                    SetVec => {
                        let set = self.b.int()?;
                        let v = self.b.floats(4)?;
                        Value::Pair(set, v.try_into().unwrap())
                    }
                    Str => Value::Text(self.b.text()?),
                    Strs => {
                        let count = self.b.count()?;
                        Value::Texts(
                            (0..count)
                                .map(|_| self.b.text())
                                .collect::<Result<_, _>>()?,
                        )
                    }
                    Ints => {
                        let count = self.b.count()?;
                        Value::Ints(self.b.ints(count)?)
                    }
                    One => Value::Node(Box::new(self.node(depth + 1)?)),
                    Many => {
                        let count = self.b.count()?;
                        let mut nodes = Vec::with_capacity(count.min(1 << 20));
                        for _ in 0..count {
                            nodes.push(self.node(depth + 1)?);
                        }
                        Value::Nodes(nodes)
                    }
                    Shorts => {
                        let n = self.b.count()?;
                        let data = self.b.take(n.checked_mul(2).ok_or("bad index count")?)?;
                        Value::Ints(
                            data.chunks_exact(2)
                                .map(|p| u16::from_le_bytes([p[0], p[1]]).into())
                                .collect(),
                        )
                    }
                    Keys => {
                        let count = self.b.count()?;
                        let keys = self.b.take(count.checked_mul(32).ok_or("bad count")?)?;
                        let float = |c: &[u8]| f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                        let key = |k: &[u8]| [0, 4, 16, 20, 24, 28].map(|o| float(&k[o..]));
                        Value::Floats(keys.chunks_exact(32).flat_map(key).collect())
                    }
                };
                node.fields.insert(name, value);
            }
        }
    }
    let mut t = T {
        b: Bytes { data, at: 0 },
        trace: Vec::new(),
    };
    let r = t.node(0).and_then(|n| {
        if t.b.at != data.len() {
            return Err(format!("{} unread bytes", data.len() - t.b.at).into());
        }
        Ok(n)
    });
    (t.trace, r)
}

/// Enum words of the `.nma` files and the binary values they stand for.
/// Values pinned by the shipped files; the rest ([I]) only keep text and
/// binary apart per path and never steer the renderer by number.
fn word(kind: &str, field: &str, word: &str) -> Result<Value, Error> {
    let num = |v: u32| Ok(Value::Int(v));
    match (kind, field, word) {
        (_, _, "default") if field == "format" => num(25),
        (_, _, "bgra") if field == "format" => num(25),
        (_, "profile", "gl_vertex_30") => num(6148),
        (_, "profile", "gl_fragment_40") => num(6151),
        (_, "profile", "gl_vertex_20") => num(6147),
        (_, "profile", "gl_fragment_20") => num(6150),
        (_, "primType", "tri_list") => num(3),
        (_, "primType", "tri_strip") => num(5),
        (_, "interpMethod", "bezier") => num(0),
        (_, "interpMethod", "linear") => num(1),
        (_, "interpMethod", "hermite") => num(2),
        (_, _, "constant") if field == "preStart" || field == "postStop" => num(0),
        (_, "rotationOrder", "xyz") => num(0),
        (_, "cull", "never") => num(1),
        (_, "cull", "static") => num(2),
        (_, "sortType", "front_to_back") => num(0),
        (_, "sortType", "back_to_front") => num(1),
        (_, "map", w) => num(match w {
            "view" => 0,
            "view_proj" => 1,
            "view_proj_win" => 2,
            "xform" => 3,
            "xform_view_proj" => 4,
            _ => return Err(format!("unknown map {w:?}").into()),
        }),
        (_, "property", "color") => num(0),
        (_, "property", "position") => num(1),
        (_, "property", "direction") => num(2),
        (_, "space", "world") => num(1),
        (_, "param", "time") => num(0),
        (_, "param", "textureDim") => num(1),
        (_, "param", "camWorldPos") => num(4),
        (_, "mapping", "identity") => num(0),
        (_, "colorSrcBlend", w) | (_, "colorDestBlend", w) => blend(w),
        (_, "alphaSrcBlend", w) | (_, "alphaDestBlend", w) => blend(w),
        (_, "blendOp", "add") => num(0),
        (_, "depthTestFunc", w) => num(match w {
            "never" => 0,
            "less" => 1,
            "equal" => 2,
            "lequal" => 3,
            "greater" => 4,
            "notequal" => 5,
            "gequal" => 6,
            "always" => 7,
            _ => return Err(format!("unknown depthTestFunc {w:?}").into()),
        }),
        (_, "alphaTestFunc", w) => num(match w {
            "never" => 0,
            "less" => 1,
            "equal" => 2,
            "lequal" => 3,
            "greater" => 4,
            "notequal" => 5,
            "gequal" => 6,
            "always" => 7,
            _ => return Err(format!("unknown alphaTestFunc {w:?}").into()),
        }),
        (_, "minFilter", w) | (_, "magFilter", w) | (_, "mipFilter", w) => num(match w {
            "none" => 0,
            "nearest" => 1,
            "linear" => 2,
            _ => return Err(format!("unknown filter {w:?}").into()),
        }),
        (_, "wrapU", w) | (_, "wrapV", w) | (_, "wrapW", w) => num(match w {
            "clamp" => 0,
            "repeat" => 1,
            _ => return Err(format!("unknown wrap {w:?}").into()),
        }),
        (_, "stencilFunc", w)
        | (_, "stencilOp", w)
        | (_, "stencilZPassOp", w)
        | (_, "stencilZFailOp", w) => num(match w {
            "never" => 0,
            "less" => 1,
            "equal" => 2,
            "lequal" => 3,
            "greater" => 4,
            "notequal" => 5,
            "gequal" => 6,
            "always" => 7,
            "keep" => 8,
            "zero" => 9,
            "replace" => 10,
            "incr" => 11,
            "decr" => 12,
            "invert" => 13,
            _ => return Err(format!("unknown stencil word {w:?}").into()),
        }),
        (_, "cullFace", w) => num(match w {
            "none" => 0,
            "front" => 1,
            "back" => 2,
            _ => return Err(format!("unknown cullFace {w:?}").into()),
        }),
        (_, "drawStyle", w) => num(match w {
            "point" => 0,
            "wireframe" => 1,
            "solid" => 2,
            _ => return Err(format!("unknown drawStyle {w:?}").into()),
        }),
        (_, "pointStyle", "sprite") => num(0),
        (_, "cpuOutputSpace", "world") => num(1),
        (_, _, w) => Err(format!("unknown word {w:?} for {kind}.{field}").into()),
    }
}

fn blend(w: &str) -> Result<Value, Error> {
    let v = match w {
        "zero" => 0,
        "one" => 1,
        "src_color" => 2,
        "inv_src_color" => 3,
        "src_alpha" => 4,
        "inv_src_alpha" => 5,
        "dest_alpha" => 6,
        "inv_dest_alpha" => 7,
        "dest_color" => 8,
        "inv_dest_color" => 9,
        _ => return Err(format!("unknown blend {w:?}").into()),
    };
    Ok(Value::Int(v))
}

/// Words of one `.nma` file: comments removed, quoted strings, braces and
/// brackets kept whole, commas only separate.
struct Words {
    clean: String,
    spans: Vec<(usize, usize)>,
    at: usize,
}

impl Words {
    fn new(text: &str) -> Self {
        let mut clean = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '/' && chars.peek() == Some(&'/') {
                for c in chars.by_ref() {
                    if c == '\n' {
                        clean.push('\n');
                        break;
                    }
                }
            } else if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
                clean.push(' ');
            } else {
                clean.push(c);
            }
        }
        let mut spans = Vec::new();
        let mut rest = clean.as_str();
        let mut base = 0;
        loop {
            let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
            base += rest.len() - trimmed.len();
            rest = trimmed;
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
            spans.push((base, base + n));
            rest = &rest[n..];
            base += n;
        }
        Self {
            clean,
            spans,
            at: 0,
        }
    }
    fn peek(&self) -> &str {
        self.spans
            .get(self.at)
            .map(|&(a, b)| &self.clean[a..b])
            .unwrap_or("")
    }
    fn next(&mut self) -> Result<&str, Error> {
        let at = self.at;
        let w = self.peek();
        if w.is_empty() {
            return Err("unexpected end of text".into());
        }
        self.at = at + 1;
        Ok(self
            .spans
            .get(at)
            .map(|&(a, b)| &self.clean[a..b])
            .unwrap_or(""))
    }
    fn expect(&mut self, what: &str) -> Result<(), Error> {
        let w = self.next()?;
        if w == what {
            Ok(())
        } else {
            Err(format!("expected {what}, found {w:?}").into())
        }
    }
    fn number(&mut self) -> Result<f64, Error> {
        let w = self.next()?;
        w.parse().map_err(|_| format!("not a number: {w:?}").into())
    }
    fn quoted(&mut self) -> Result<String, Error> {
        Ok(self.next()?.trim_matches('"').to_owned())
    }

    fn value(&mut self, kind: &str, field: &str, ty: &Type) -> Result<Value, Error> {
        match ty {
            Int => {
                let w = self.next()?;
                if let Ok(v) = w.parse::<i32>() {
                    Ok(Value::Int(v as u32))
                } else if let Ok(v) = w.parse::<f64>() {
                    Ok(Value::Int(v as i32 as u32))
                } else {
                    word(kind, field, w)
                }
            }
            Bool => Ok(Value::Bool(self.number()? != 0.0)),
            Float => Ok(Value::Float(self.number()? as f32)),
            Double => Ok(Value::Float(self.number()? as f32)),
            Vector(n) => {
                let mut v = Vec::with_capacity(*n);
                for _ in 0..*n {
                    v.push(self.number()? as f32);
                }
                Ok(Value::Floats(v))
            }
            SetVec => {
                let mut v = [0.0; 4];
                for x in v.iter_mut() {
                    *x = self.number()? as f32;
                }
                Ok(Value::Pair(0, v))
            }
            Floats(n) => {
                self.expect("[")?;
                let mut v = Vec::new();
                while self.peek() != "]" {
                    v.push(self.number()? as f32);
                }
                self.expect("]")?;
                if v.len() % n != 0 {
                    return Err(format!("{kind}.{field} has {} numbers", v.len()).into());
                }
                Ok(Value::Floats(v))
            }
            Str => Ok(Value::Text(self.quoted()?)),
            Strs => {
                self.expect("[")?;
                let mut v = Vec::new();
                while self.peek() != "]" {
                    v.push(self.quoted()?);
                }
                self.expect("]")?;
                Ok(Value::Texts(v))
            }
            Ints => {
                self.expect("[")?;
                let mut v = Vec::new();
                while self.peek() != "]" {
                    v.push(self.number()? as i32 as u32);
                }
                self.expect("]")?;
                Ok(Value::Ints(v))
            }
            One => Ok(Value::Node(Box::new(self.node(0)?))),
            Many => {
                self.expect("[")?;
                let mut v = Vec::new();
                while self.peek() != "]" {
                    v.push(self.node(0)?);
                }
                self.expect("]")?;
                Ok(Value::Nodes(v))
            }
            Shorts => {
                self.expect("[")?;
                let mut v = Vec::new();
                while self.peek() != "]" {
                    v.push(self.number()? as i32 as u32);
                }
                self.expect("]")?;
                Ok(Value::Ints(v))
            }
            Keys => {
                self.expect("[")?;
                let mut v = Vec::new();
                while self.peek() != "]" {
                    let mut key = Vec::with_capacity(8);
                    for _ in 0..8 {
                        key.push(self.number()? as f32);
                    }
                    // Like the binary reader: time, value, then the four
                    // tangent floats; the tangent-kind pair is skipped.
                    v.extend([key[0], key[1], key[4], key[5], key[6], key[7]]);
                }
                self.expect("]")?;
                Ok(Value::Floats(v))
            }
        }
    }

    fn node(&mut self, depth: usize) -> Result<Node, Error> {
        if depth > 64 {
            return Err("nodes are nested too deeply".into());
        }
        let word = self.next()?;
        if let Some(found) = TEXTONLY.iter().find(|k| k.0 == word) {
            let kind: &'static str = found.0;
            let mut node = Node {
                kind,
                name: String::new(),
                fields: BTreeMap::new(),
            };
            if self.peek() != "{" {
                node.name = self.next()?.to_owned();
            }
            self.expect("{")?;
            while self.peek() != "}" {
                let word = self.next()?;
                if word == "}" {
                    break;
                }
                let (name, ty) = found
                    .1
                    .iter()
                    .find(|f| f.0.eq_ignore_ascii_case(word))
                    .ok_or_else(|| format!("unknown field {word:?} of {kind}"))?;
                node.fields.insert(name, self.value(kind, name, ty)?);
            }
            self.expect("}")?;
            return Ok(node);
        }
        let (_, kind, schema) = KINDS
            .iter()
            .find(|k| k.1 == word)
            .ok_or_else(|| format!("unknown node {word:?}"))?;
        let schema: Fields = TEXTKINDS
            .iter()
            .find(|k| k.0 == *kind)
            .map(|k| k.1)
            .unwrap_or(*schema);
        let mut node = Node {
            kind,
            name: String::new(),
            fields: BTreeMap::new(),
        };
        if self.peek() != "{" {
            node.name = self.next()?.to_owned();
        }
        self.expect("{")?;
        while self.peek() != "}" {
            let word = self.next()?;
            if word == "}" {
                break;
            }
            let (name, ty) = schema
                .iter()
                .find(|f| f.0.eq_ignore_ascii_case(word))
                .ok_or_else(|| format!("unknown field {word:?} of {kind}"))?;
            node.fields.insert(name, self.value(kind, name, ty)?);
        }
        self.expect("}")?;
        Ok(node)
    }
}

/// Reads a whole `.nma` file; every word must be understood.
pub fn read_text(text: &str) -> Result<Node, Error> {
    let (node, rest) = read_text_first(text)?;
    if !rest.is_empty() {
        return Err(format!("unread text from {:?}", rest[0]).into());
    }
    Ok(node)
}

/// Reads the first node of `.nma` text, returning the words left over.
/// Only `particles.nma` has any: its Scene is complete, then a corrupt
/// two-token tail (`0`, `o3`) that the shipped file ends with.
pub fn read_text_first(text: &str) -> Result<(Node, Vec<String>), Error> {
    let mut w = Words::new(text);
    let node = w.node(0)?;
    let rest = w.spans[w.at..]
        .iter()
        .map(|&(a, b)| w.clean[a..b].to_owned())
        .collect();
    Ok((node, rest))
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

    #[test]
    fn text_matches_binary_curves() {
        // `keyframe [ t v a b c d e f ]` drops the tangent-kind pair like binary.
        let t = read_text(
            "Scene { animations [ Animation { timeScale 1 channels [ \
             MayaVec3fChannel { targetNode n targetField scale \
             xCurve MayaCurveRegular { startTime 0 stopTime 1 preStart constant \
             postStop constant interpMethod linear \
             keyframes [ 0 1 10 0 0 10 0 0 , ] } \
             yCurve MayaCurveRegular { startTime 0 stopTime 1 preStart constant \
             postStop constant interpMethod hermite \
             keyframes [ 0 2 10 0 0 10 0 0 , ] } \
             zCurve MayaCurveRegular { startTime 0 stopTime 1 preStart constant \
             postStop constant interpMethod linear \
             keyframes [ 0 3 10 0 0 10 0 0 , ] } index 0 } ] } ] }",
        )
        .unwrap();
        assert_eq!(t.kind, "Scene");
        let anim = &t.nodes("animations")[0];
        assert_eq!(anim.float("timeScale"), 1.0);
        let ch = &anim.nodes("channels")[0];
        assert_eq!(ch.text("targetNode"), "n");
        assert_eq!(ch.int("index"), 0);
        let x = ch.node("xCurve").unwrap();
        assert_eq!(x.floats("keyframes"), &[0.0, 1.0, 0.0, 10.0, 0.0, 0.0]);
        assert_eq!(x.int("interpMethod"), 1);
    }

    #[test]
    fn text_rejects_unknown_words() {
        assert!(read_text("Scene { nonsense 1 }").is_err());
        assert!(read_text("Group { }").is_err());
    }
}
