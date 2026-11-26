//! NVIDIA's scene (`.nmb`) and animation (`.nab`) files: trees of nodes with a kind,
//! perhaps a name, and fields. The tables below come from the viewer's own names.
use std::collections::BTreeMap;

use crate::Error;
use crate::demo::Dialect;

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
    Shorts,
    Floats,
    Marker,
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

/// The second-generation viewer uses named resources and 16-bit index arrays.
const KINDS: Schema = &[
    (
        0x00000001,
        "Scene",
        &[
            ("sceneInfo", Str),
            ("materials", Many),
            ("textures", Many),
            ("lights", Many),
            ("sounds", Many),
            ("vertexPrograms", Many),
            ("pixelShaders", Many),
            ("renderParams", Many),
            ("geometries", Many),
            ("animations", Many),
            ("transforms", Many),
            ("backgrounds", Many),
            ("renderGroups", Many),
        ],
    ),
    (0x10000010, "Vec1f", &[("value", Vector(1))]),
    (0x10000011, "Vec2f", &[("value", Vector(2))]),
    (0x10000012, "Vec3f", &[("value", Vector(3))]),
    (0x10000013, "Vec4f", &[("value", Vector(4))]),
    (0x10000021, "Vec2fArray", &[("value", Vectors(2))]),
    (0x10000022, "Vec3fArray", &[("value", Vectors(3))]),
    (0x10000023, "Vec4fArray", &[("value", Vectors(4))]),
    (
        0x20000101,
        "2DTexture",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Int),
            ("wrapU", Int),
            ("wrapV", Int),
            ("filename", Str),
        ],
    ),
    (
        0x20000102,
        "ShadowTexture",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Int),
            ("wrapU", Int),
            ("wrapV", Int),
            ("width", Int),
            ("height", Int),
        ],
    ),
    (
        0x20000111,
        "3DTexture",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Int),
            ("wrapU", Int),
            ("wrapV", Int),
            ("wrapW", Int),
            ("filename", Str),
        ],
    ),
    (
        0x20000201,
        "CubicTexture",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Int),
            ("mipmapBias", Float),
            ("anisotropy", Int),
            ("filename", Str),
        ],
    ),
    (
        0x22000300,
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
        0x23000000,
        "Background",
        &[("renderSettings", One), ("positionScale", Float)],
    ),
    (
        0x30000001,
        "DirectionalLight",
        &[
            ("transform", Str),
            ("diffuse", Vector(4)),
            ("specular", Vector(4)),
            ("ambient", Vector(4)),
            ("isEyespace", Bool),
            ("direction", Vector(3)),
        ],
    ),
    (
        0x30000002,
        "PointLight",
        &[
            ("transform", Str),
            ("diffuse", Vector(4)),
            ("specular", Vector(4)),
            ("ambient", Vector(4)),
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
            ("diffuse", Vector(4)),
            ("specular", Vector(4)),
            ("ambient", Vector(4)),
            ("isEyespace", Bool),
            ("position", Vector(3)),
            ("direction", Vector(3)),
            ("inner", Float),
            ("outer", Float),
            ("attenuation", Vector(3)),
        ],
    ),
    (
        0x30000004,
        "ShadowSpotLight",
        &[
            ("transform", Str),
            ("diffuse", Vector(4)),
            ("specular", Vector(4)),
            ("ambient", Vector(4)),
            ("isEyespace", Bool),
            ("position", Vector(3)),
            ("direction", Vector(3)),
            ("inner", Float),
            ("outer", Float),
            ("attenuation", Vector(3)),
            ("nearPlane", Float),
            ("farPlane", Float),
            ("shadowColor", Vector(4)),
            ("regions", Vector(2)),
            ("spot", Bool),
            ("shadow", Bool),
            ("shadowTransform", Str),
            ("spotlightTexture", Str),
            ("shadowTexture", Str),
            ("shadowOccluders", Strs),
        ],
    ),
    (
        0x40000000,
        "RenderSettings",
        &[
            ("modelTransform", Str),
            ("textures", Strs),
            ("texGenFuncs", Ints),
            ("texcoordIndices", Ints),
            ("spareTransforms", Strs),
            ("textureTransforms", Strs),
            ("material", Str),
            ("lights", Strs),
            ("vertexProgram", Str),
            ("pixelShader", Str),
            ("cullParams", Str),
            ("blendParams", Str),
            ("depthTestParams", Str),
            ("alphaTestParams", Str),
            ("fogParams", Str),
            ("stencilParams", Str),
            ("miscParams", Str),
            ("shadowVertexProgram", Str),
        ],
    ),
    (
        0x41000010,
        "VertexProgram",
        &[
            ("filename", Str),
            ("vertexRegister0", Int),
            ("vertexRegister1", Int),
            ("vertexRegister2", Int),
            ("vertexRegister3", Int),
            ("vertexRegister4", Int),
            ("vertexRegister5", Int),
            ("vertexRegister6", Int),
            ("vertexRegister7", Int),
            ("vertexRegister8", Int),
            ("vertexRegister9", Int),
            ("vertexRegister10", Int),
            ("vertexRegister11", Int),
            ("vertexRegister12", Int),
            ("vertexRegister13", Int),
            ("vertexRegister14", Int),
            ("vertexRegister15", Int),
            ("constantParams", Many),
        ],
    ),
    (
        0x41000021,
        "MatrixParam",
        &[
            ("paramName", Str),
            ("startingRegister", Int),
            ("matrix", Int),
            ("transpose", Bool),
            ("invert", Bool),
            ("localTransform", One),
        ],
    ),
    (
        0x41000022,
        "EvaluatedMatrixParam",
        &[
            ("paramName", Str),
            ("startingRegister", Int),
            ("matrices", Many),
            ("transpose", Bool),
            ("invert", Bool),
        ],
    ),
    (
        0x41000023,
        "LightParam",
        &[
            ("paramName", Str),
            ("startingRegister", Int),
            ("light", Str),
        ],
    ),
    (
        0x41000024,
        "MaterialParam",
        &[
            ("paramName", Str),
            ("startingRegister", Int),
            ("material", Str),
        ],
    ),
    (
        0x41000025,
        "StaticParam",
        &[
            ("paramName", Str),
            ("startingRegister", Int),
            ("value", One),
        ],
    ),
    (
        0x41000030,
        "EvaluatedMatrix",
        &[
            ("matrix", Int),
            ("transpose", Bool),
            ("invert", Bool),
            ("initialValue", Bool),
        ],
    ),
    (
        0x42000000,
        "PixelShader",
        &[("combiners", Many), ("textureExpr", Many)],
    ),
    (
        0x42000011,
        "Combiner",
        &[
            ("colorExpr", One),
            ("alphaExpr", One),
            ("constColor0", Vector(4)),
            ("constColor1", Vector(4)),
        ],
    ),
    (
        0x42000012,
        "CombinerExpr",
        &[
            ("operator", Int),
            ("outputScale", Int),
            ("outputBias", Int),
            ("dest0", Int),
            ("dest1", Int),
            ("inputs", Many),
        ],
    ),
    (
        0x42000013,
        "CombinerInput",
        &[("source", Int), ("map", Int), ("otherComponent", Bool)],
    ),
    (0x42000021, "SimpleExpr", &[("expr", Int)]),
    (
        0x42000022,
        "CullExpr",
        &[
            ("cullNegS", Bool),
            ("cullNegT", Bool),
            ("cullNegR", Bool),
            ("cullNegQ", Bool),
        ],
    ),
    (
        0x42000023,
        "OffsetExpr",
        &[
            ("input", Int),
            ("matrix", Vector(4)),
            ("scale", Float),
            ("expr", Int),
        ],
    ),
    (
        0x42000025,
        "DotExpr",
        &[
            ("expr", Int),
            ("input", Int),
            ("expandRGBA", Bool),
            ("eyeVector", Vector(3)),
        ],
    ),
    (
        0x50000011,
        "BlendParams",
        &[("srcBlend", Int), ("destBlend", Int)],
    ),
    (
        0x50000012,
        "CullParams",
        &[("ccwIsFront", Bool), ("cullFace", Int)],
    ),
    (
        0x50000013,
        "DepthTestParams",
        &[
            ("depthTestEnabled", Bool),
            ("depthTestFunc", Int),
            ("depthMask", Bool),
        ],
    ),
    (
        0x50000014,
        "AlphaTestParams",
        &[
            ("alphaTestEnabled", Bool),
            ("alphaTestFunc", Int),
            ("alphaTestRef", Int),
        ],
    ),
    (
        0x50000015,
        "FogParams",
        &[
            ("fogEnable", Bool),
            ("fogIsExponential", Bool),
            ("fogDistanceMode", Int),
            ("fogColor", Vector(4)),
            ("fogStart", Float),
            ("fogEnd", Float),
            ("fogPower", Float),
            ("fogDensity", Float),
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
            ("stencilWriteMask", Bool),
        ],
    ),
    (
        0x50000017,
        "MiscParams",
        &[
            ("normalizeNormals", Bool),
            ("rescaleNormals", Bool),
            ("lightingEnable", Bool),
            ("ambientColor", Vector(4)),
            ("localViewer", Bool),
            ("twoSidedLighting", Bool),
            ("pointSize", Float),
            ("lineWidth", Float),
            ("drawStyle", Int),
            ("smoothShade", Bool),
            ("pointMinSize", Float),
            ("pointMaxSize", Float),
            ("pointFadeThresh", Float),
            ("pointAttenuation", Vector(3)),
            ("pointStyle", Int),
            ("ditherEnabled", Bool),
            ("redMask", Bool),
            ("greenMask", Bool),
            ("blueMask", Bool),
            ("alphaMask", Bool),
            ("polygonOffsetScale", Float),
            ("polygonOffsetBias", Float),
        ],
    ),
    (
        0x60000011,
        "CullingGroup",
        &[
            ("transform", Str),
            ("sphereCenter", Vector(3)),
            ("sphereRadius", Float),
            ("culling", Bool),
            ("children", Many),
        ],
    ),
    (
        0x60000012,
        "SortingGroup",
        &[
            ("transform", Str),
            ("sphereCenter", Vector(3)),
            ("sphereRadius", Float),
            ("sortType", Int),
            ("sortSet", Int),
            ("shapes", Many),
        ],
    ),
    (
        0x60000013,
        "Group",
        &[
            ("transform", Str),
            ("sphereCenter", Vector(3)),
            ("sphereRadius", Float),
            ("children", Many),
        ],
    ),
    (
        0x60000020,
        "Shape",
        &[("renderSettings", Many), ("geometries", Strs)],
    ),
    (
        0x60000021,
        "LodShape",
        &[("distances", Floats), ("children", Many)],
    ),
    (
        0x60000031,
        "IndexedFaceSet",
        &[
            ("coords", Many),
            ("normals", Many),
            ("texCoords", Many),
            ("colors", Many),
            ("data", Many),
            ("indices", Many),
        ],
    ),
    (
        0x60000033,
        "Curve",
        &[
            ("controlPts", Vectors(3)),
            ("subDivisions", Int),
            ("tubeSides", Int),
            ("tubeRadius", Float),
            ("tubeRadii", Vectors(1)),
        ],
    ),
    (
        0x60000034,
        "HeightField",
        &[
            ("heightImage", Str),
            ("dimensions", Vector(3)),
            ("textureRepeat", Vectors(2)),
            ("tesselation", Vector(2)),
        ],
    ),
    (
        0x60000042,
        "VertexCoord",
        &[("animationSet", Int), ("data", One)],
    ),
    (
        0x60000043,
        "VertexData",
        &[("animationSet", Int), ("data", One)],
    ),
    (
        0x60000044,
        "VertexIndex",
        &[("primType", Int), ("indices", Shorts)],
    ),
    (
        0x60000045,
        "VertexNormal",
        &[("animationSet", Int), ("data", One)],
    ),
    (
        0x60000046,
        "VertexTexCoord",
        &[("animationSet", Int), ("data", One)],
    ),
    (
        0x600000ff,
        "FurShape",
        &[
            ("furLayers", Int),
            ("furDepth", Float),
            ("finHeight", Float),
            ("furScale", Vector(4)),
            ("furTextures", Strs),
            ("renderSettings", One),
            ("furRenderSettings", One),
            ("finRenderSettings", One),
            ("geometries", Strs),
        ],
    ),
    (
        0x70000001,
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
        0x70000002,
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
        0x70000003,
        "MatrixTransform",
        &[
            ("matrix", Vector(16)),
            ("children", Many),
            ("label", Str),
            ("field3", Int),
            ("field4", Int),
            ("field5", Int),
            ("legacyMarker", Marker),
        ],
    ),
    (
        0xa0000000,
        "Animation",
        &[("timeScale", Float), ("channels", Many)],
    ),
    (
        0xa0000110,
        "MayaKeyframe",
        &[("time", Int), ("value", Float)],
    ),
    (
        0xa0000111,
        "MeshCurve",
        &[
            ("time", Int),
            ("coords", Vectors(3)),
            ("normals", Vectors(3)),
            ("tangents", Vectors(4)),
            ("binormals", Vectors(3)),
        ],
    ),
    (
        0xa0000115,
        "MayaCurve",
        &[
            ("startTime", Int),
            ("stopTime", Int),
            ("preStart", Int),
            ("postStop", Int),
            ("interpMethod", Int),
            ("keyframes", Many),
        ],
    ),
    (
        0xa0000311,
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
        "MeshChannel",
        &[("targetNode", Str), ("curves", Many)],
    ),
    (
        0xb0000000,
        "Sound",
        &[("transform", Str), ("filename", Str), ("volume", Float)],
    ),
    (
        0xb0000001,
        "PositionalSound",
        &[
            ("transform", Str),
            ("filename", Str),
            ("volume", Float),
            ("position", Vector(3)),
            ("mindistance", Float),
            ("maxdistance", Float),
            ("penumbra", Float),
            ("broadcastdirection", Vector(3)),
            ("outervolume", Float),
        ],
    ),
];

const OLD: Schema = &[
    (
        0x20000101,
        "2DTexture",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("format", Bool),
            ("mipmapBias", Float),
            ("wrapU", Bool),
            ("wrapV", Bool),
            ("filename", Str),
        ],
    ),
    (
        0x20000201,
        "CubicTexture",
        &[
            ("minFilter", Int),
            ("magFilter", Int),
            ("mipFilter", Int),
            ("compress", Bool),
            ("mipmapBias", Float),
            ("filename", Str),
        ],
    ),
    (
        0x50000017,
        "MiscParams",
        &[
            ("normalizeNormals", Bool),
            ("rescaleNormals", Bool),
            ("lightingEnable", Bool),
            ("ambientColor", Vector(4)),
            ("localViewer", Bool),
            ("twoSidedLighting", Bool),
            ("pointSize", Float),
            ("lineWidth", Float),
            ("drawStyle", Int),
            ("smoothShade", Bool),
            ("pointMinSize", Float),
            ("pointMaxSize", Float),
            ("pointFadeThresh", Float),
            ("pointAttenuation", Vector(3)),
            ("ditherEnabled", Bool),
            ("redMask", Bool),
            ("greenMask", Bool),
            ("blueMask", Bool),
            ("alphaMask", Bool),
            ("polygonOffsetScale", Float),
            ("polygonOffsetBias", Float),
        ],
    ),
];

const TIDEPOOL: Schema = &[(
    0x60000034,
    "HeightField",
    &[
        ("heightImage", Str),
        ("dimensions", Vector(3)),
        ("textureRepeat", Vectors(3)),
        ("tesselation", Vector(2)),
    ],
)];

impl Node {
    pub(crate) fn empty(kind: &'static str) -> Self {
        Self {
            kind,
            name: String::new(),
            fields: BTreeMap::new(),
        }
    }
    pub fn set(&mut self, field: &'static str, value: Value) {
        self.fields.insert(field, value);
    }
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
    schema: Schema,
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
        let (_, kind, schema) = self
            .schema
            .iter()
            .chain(KINDS)
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
            // Maya key tangent selectors are written directly after the sample value.
            if *kind == "MayaKeyframe" && node.fields.contains_key("value") {
                self.take(4)?;
                let tangents = self.floats(4)?;
                node.fields.insert("tangents", Value::Floats(tangents));
                continue;
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
                Shorts => {
                    let n = self.count()?;
                    let data = self.take(n.checked_mul(2).ok_or("bad index count")?)?;
                    Value::Ints(
                        data.chunks_exact(2)
                            .map(|p| u16::from_le_bytes([p[0], p[1]]).into())
                            .collect(),
                    )
                }
                Floats => {
                    let n = self.count()?;
                    Value::Floats(self.floats(n)?)
                }
                Marker => Value::Bool(true),
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
    let schema = match crate::demo::DIALECT {
        Dialect::First => OLD,
        Dialect::Third => TIDEPOOL,
        Dialect::Second => &[],
    };
    let mut b = Bytes {
        data,
        at: 0,
        schema,
    };
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
