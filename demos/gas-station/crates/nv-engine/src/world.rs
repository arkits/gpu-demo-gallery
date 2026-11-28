//! The scene as the viewer holds it: every named node of every file in one namespace
//! (the extra files merge into the first, `0x466a50`), with references by name resolved.

use std::collections::HashMap;

use nv_scene::{half, Node, Value};

use crate::anim::Animation;
use crate::math::{Mat4, Vec3, Vec4};

#[derive(Debug, Clone, PartialEq)]
pub enum TransformKind {
    Maya {
        scale_pivot: Vec3,
        scale_pivot_translate: Vec3,
        scale: Vec3,
        rotation_pivot: Vec3,
        rotation_pivot_translate: Vec3,
        rotation_orientation: Vec3,
        rotation: Vec3,
        translation: Vec3,
        rotation_order: u32,
    },
    Joint {
        scale: Vec3,
        rotation_orientation: Vec3,
        rotation: Vec3,
        joint_orientation: Vec3,
        translation: Vec3,
        rotation_order: u32,
    },
    Matrix(Mat4),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub name: String,
    pub kind: TransformKind,
    pub parent: Option<usize>,
    /// The transform's +0x38 matrix (TransformParam maps 23 and up) [I: inverse bind pose].
    pub bind: Mat4,
    pub world: Mat4,
}

impl Transform {
    /// The local matrix (`0x436be0` chain).
    pub fn local(&self) -> Mat4 {
        match &self.kind {
            TransformKind::Maya {
                scale_pivot: sp,
                scale_pivot_translate: spt,
                scale: s,
                rotation_pivot: rp,
                rotation_pivot_translate: rpt,
                rotation_orientation: ra,
                rotation: r,
                translation: t,
                rotation_order: o,
            } => {
                let neg = |v: &Vec3| [-v[0], -v[1], -v[2]];
                Mat4::translation(*t)
                    * Mat4::translation(*rpt)
                    * Mat4::translation(*rp)
                    * Mat4::euler(*o, *r)
                    * Mat4::euler(*o, *ra)
                    * Mat4::translation(neg(rp))
                    * Mat4::translation(*spt)
                    * Mat4::translation(*sp)
                    * Mat4::scale(*s)
                    * Mat4::translation(neg(sp))
            }
            TransformKind::Joint { scale, rotation_orientation, rotation, joint_orientation, translation, rotation_order } => {
                // [I] Maya's joint: T · JO · R · RA · S, as in Dawn (0x43b480 of Fairy.exe).
                Mat4::translation(*translation)
                    * Mat4::euler(0, *joint_orientation)
                    * Mat4::euler(*rotation_order, *rotation)
                    * Mat4::euler(0, *rotation_orientation)
                    * Mat4::scale(*scale)
            }
            TransformKind::Matrix(m) => *m,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    pub name: String,
    pub ortho: bool,
    pub screen_aspect: bool,
    pub near: f32,
    pub far: f32,
    pub fov: f32,
    pub aspect: f32,
    /// Ortho box.
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub position: Vec3,
    /// (w, x, y, z).
    pub orientation: Vec4,
    pub transform: Option<usize>,
    pub view: Mat4,
    pub proj: Mat4,
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub dir: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Directional,
    Point,
    Spot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub name: String,
    pub kind: LightKind,
    pub transform: Option<usize>,
    pub color: Vec3,
    pub intensity: f32,
    pub position: Vec3,
    pub direction: Vec3,
    pub inner: f32,
    pub outer: f32,
    pub attenuation: Vec3,
    pub world_position: Vec3,
    pub world_direction: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Transform { name: String, transform: Option<usize>, pass: Option<usize>, map: u32 },
    /// `node` is the ConstantParam's own name, by which the demos set it.
    Constant { name: String, node: String, value: Vec4 },
    Texture { name: String, texture: Option<usize>, state: Option<usize> },
    Light { name: String, light: Option<usize>, property: u32, space: u32 },
    Misc { name: String, param: u32 },
}

impl Param {
    pub fn name(&self) -> &str {
        match self {
            Param::Transform { name, .. }
            | Param::Constant { name, .. }
            | Param::Texture { name, .. }
            | Param::Light { name, .. }
            | Param::Misc { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shader {
    pub name: String,
    pub filename: String,
    pub profile: u32,
    pub entry: String,
    /// Applied once per pass.
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextureSource {
    File(String),
    /// Render target.
    Proc { width: u32, height: u32, samples: u32 },
    Cube([String; 6]),
    ProcCube { size: u32 },
    File3D(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub name: String,
    pub source: TextureSource,
    pub format: u32,
    /// The older viewers keep sampling state in the texture node.
    pub state: Option<TextureState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureState {
    pub min: u32,
    pub mag: u32,
    pub mip: u32,
    pub wrap: [u32; 3],
    pub bias: f32,
    pub anisotropy: f32,
    pub remapped: bool,
    pub shadow: bool,
}

impl Default for TextureState {
    fn default() -> Self {
        TextureState { min: 2, mag: 2, mip: 2, wrap: [0; 3], bias: 0.0, anisotropy: 1.0, remapped: false, shadow: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cull {
    pub ccw_front: bool,
    pub face: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    pub color_src: u32,
    pub color_dst: u32,
    pub alpha_src: u32,
    pub alpha_dst: u32,
    pub op: u32,
    pub color: Vec4,
    pub mask: [bool; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Depth {
    pub test: bool,
    pub func: u32,
    pub write: bool,
    pub offset_scale: f32,
    pub offset_bias: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alpha {
    pub test: bool,
    pub func: u32,
    pub reference: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stencil {
    pub enabled: bool,
    pub func_mask: u32,
    pub func: u32,
    pub reference: u32,
    pub fail: u32,
    pub zpass: u32,
    pub zfail: u32,
    pub write_mask: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Raster {
    pub point_style: u32,
    pub line_width: f32,
    pub draw_style: u32,
}

/// A render state object (`renderParams`, or a built-in).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Cull(Cull),
    Blend(Blend),
    Depth(Depth),
    Alpha(Alpha),
    Stencil(Stencil),
    Raster(Raster),
    Texture(TextureState),
}

pub const CULL_DEFAULT: Cull = Cull { ccw_front: true, face: 1 };
pub const BLEND_DEFAULT: Blend =
    Blend { color_src: 1, color_dst: 0, alpha_src: 1, alpha_dst: 0, op: 0, color: [0.0; 4], mask: [true; 4] };
pub const DEPTH_DEFAULT: Depth = Depth { test: true, func: 1, write: true, offset_scale: 0.0, offset_bias: 0.0 };
pub const ALPHA_DEFAULT: Alpha = Alpha { test: false, func: 1, reference: 0 };
pub const STENCIL_DEFAULT: Stencil =
    Stencil { enabled: false, func_mask: 0, func: 0, reference: 0, fail: 0, zpass: 0, zfail: 0, write_mask: false };
pub const RASTER_DEFAULT: Raster = Raster { point_style: 0, line_width: 1.0, draw_style: 0 };

/// The built-in states (`0x454fb0`…`0x455f80`).
pub fn builtin(name: &str) -> Option<State> {
    let b = BLEND_DEFAULT;
    let d = DEPTH_DEFAULT;
    let t = TextureState::default();
    Some(match name {
        "_CullDefault" => State::Cull(CULL_DEFAULT),
        "_CullNoCull" => State::Cull(Cull { face: 3, ..CULL_DEFAULT }),
        "_BlendDefault" => State::Blend(b),
        "_BlendNoColor" => State::Blend(Blend { mask: [false; 4], ..b }),
        "_BlendAlpha" => State::Blend(Blend { color_src: 4, color_dst: 5, alpha_src: 4, alpha_dst: 5, op: 1, ..b }),
        "_BlendAdd" => State::Blend(Blend { color_src: 1, color_dst: 1, alpha_src: 1, alpha_dst: 1, op: 1, ..b }),
        "_DepthDefault" => State::Depth(d),
        "_DepthEqual" => State::Depth(Depth { func: 5, write: false, ..d }),
        "_DepthNoWrite" => State::Depth(Depth { write: false, ..d }),
        "_DepthNoTest" => State::Depth(Depth { test: false, write: false, ..d }),
        "_AlphaDefault" => State::Alpha(ALPHA_DEFAULT),
        "_AlphaTest0" => State::Alpha(Alpha { test: true, func: 6, reference: 0 }),
        "_StencilDefault" => State::Stencil(STENCIL_DEFAULT),
        "_RasterDefault" => State::Raster(RASTER_DEFAULT),
        "_RasterPoint" => State::Raster(Raster { draw_style: 2, ..RASTER_DEFAULT }),
        "_RasterLine" => State::Raster(Raster { draw_style: 1, ..RASTER_DEFAULT }),
        "_TextureDefault" => State::Texture(t),
        "_TextureBump" => State::Texture(TextureState { remapped: true, ..t }),
        "_TextureRepeat" => State::Texture(TextureState { wrap: [1; 3], ..t }),
        "_TextureShadow" => State::Texture(TextureState { shadow: true, wrap: [2; 3], ..t }),
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    pub sort_set: u32,
    pub sort_type: u32,
    pub cull: Option<usize>,
    pub blend: Option<usize>,
    pub depth: Option<usize>,
    pub alpha: Option<usize>,
    pub stencil: Option<usize>,
    pub raster: Option<usize>,
    pub vertex: Option<usize>,
    pub vertex_params: Vec<Param>,
    pub fragment: Option<usize>,
    pub fragment_params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub name: String,
    pub cull: u32,
    pub center: Vec3,
    pub radius: f32,
    pub transform: Option<usize>,
    pub settings: Vec<Settings>,
    pub geometries: Vec<usize>,
    /// World bounds (`0x497970`).
    pub world_center: Vec3,
    pub world_radius: f32,
}

/// A shape or a group of them, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeRef {
    Shape(usize),
    Group(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: String,
    pub children: Vec<ShapeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrayData {
    Floats(Vec<f32>, u32),
    Halves(Vec<u16>, u32),
    Bytes(Vec<u8>, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub channel: String,
    pub data: ArrayData,
}

impl Array {
    pub fn components(&self) -> u32 {
        match &self.data {
            ArrayData::Floats(_, k) | ArrayData::Halves(_, k) | ArrayData::Bytes(_, k) => *k,
        }
    }

    pub fn len(&self) -> usize {
        match &self.data {
            ArrayData::Floats(v, k) => v.len() / *k as usize,
            ArrayData::Halves(v, k) => v.len() / *k as usize,
            ArrayData::Bytes(v, k) => v.len() / *k as usize,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, i: usize) -> Vec4 {
        let k = self.components() as usize;
        let mut o = [0.0, 0.0, 0.0, 1.0];
        for (c, v) in o.iter_mut().enumerate().take(k) {
            *v = match &self.data {
                ArrayData::Floats(d, _) => d[i * k + c],
                ArrayData::Halves(d, _) => half(d[i * k + c]),
                ArrayData::Bytes(d, _) => d[i * k + c] as f32 / 255.0,
            };
        }
        o
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub name: String,
    pub restart: bool,
    pub arrays: Vec<Array>,
    /// Primitive type (3 triangle list, 5 strip) and indices.
    pub lists: Vec<(u32, Vec<u16>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pass {
    pub name: String,
    pub color: Option<usize>,
    pub depth: Option<usize>,
    pub clears_color: bool,
    pub clear_color: Vec4,
    pub clears_depth: bool,
    pub clear_depth: f32,
    pub clears_stencil: bool,
    pub clear_stencil: u32,
    pub camera: Option<usize>,
    pub use_scene_shapes: bool,
    pub shapes: Vec<ShapeRef>,
    pub use_scene_backgrounds: bool,
    pub backgrounds: Vec<usize>,
    pub override_settings: Option<Settings>,
    pub use_list_indices: bool,
    pub cube_face: u32,
    /// x, y, width, height; -1 is the target's size.
    pub viewport: [i32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Background {
    pub name: String,
    pub settings: Settings,
    pub position_scale: f32,
    pub fov_scale: f32,
}

/// Menu nodes, positions in 0–1 of the window from the bottom left.
#[derive(Debug, Clone, PartialEq)]
pub enum Widget {
    Static { name: String, items: Vec<Widget> },
    DropDown { name: String, items: Vec<Widget>, open_position: [f32; 2], open_size: [f32; 2], image: Option<usize>, initially_open: bool },
    OnOff { name: String, callback: String, initial: bool, position: [f32; 2], size: [f32; 2], on_color: Vec3, off_color: Vec3, image: Option<usize> },
    TexSlider { name: String, callback: String, initial: f32, alignment: u32, position: [f32; 2], size: [f32; 2], slider: Option<usize>, knob: Option<usize> },
    Icon { name: String, image: Option<usize>, position: [f32; 2], size: [f32; 2] },
    /// Kinds these demos do not draw.
    Other { name: String, kind: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub name: String,
    pub filename: String,
    pub looping: bool,
    pub volume: f32,
    pub play: bool,
}

#[derive(Debug, Default)]
pub struct World {
    pub transforms: Vec<Transform>,
    pub cameras: Vec<Camera>,
    pub lights: Vec<Light>,
    pub shaders: Vec<Shader>,
    pub textures: Vec<Texture>,
    pub states: Vec<(String, State)>,
    pub shapes: Vec<Shape>,
    pub groups: Vec<Group>,
    pub geometries: Vec<Geometry>,
    pub passes: Vec<Pass>,
    pub backgrounds: Vec<Background>,
    pub animations: Vec<Animation>,
    pub menus: Vec<Widget>,
    pub sounds: Vec<Sound>,
    /// Shapes and groups at the scenes' top level (`useSceneShapes`).
    pub scene_shapes: Vec<ShapeRef>,
    /// The scenes' backgrounds (`useSceneBackgrounds`).
    pub scene_backgrounds: Vec<usize>,
    /// Widget images: `Widget` image fields index this.
    pub widget_textures: Vec<Option<usize>>,
    pub names: HashMap<String, Named>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Transform(usize),
    Camera(usize),
    Light(usize),
    Shader(usize),
    Texture(usize),
    State(usize),
    Shape(usize),
    Group(usize),
    Geometry(usize),
    Pass(usize),
    Background(usize),
    Animation(usize),
}

fn v3(n: &Node, f: &str, d: Vec3) -> Vec3 {
    n.vector(f).filter(|v| v.len() >= 3).map(|v| [v[0], v[1], v[2]]).unwrap_or(d)
}

fn v4(n: &Node, f: &str, d: Vec4) -> Vec4 {
    n.vector(f).filter(|v| v.len() >= 4).map(|v| [v[0], v[1], v[2], v[3]]).unwrap_or(d)
}

fn v2(n: &Node, f: &str) -> [f32; 2] {
    n.vector(f).filter(|v| v.len() >= 2).map(|v| [v[0], v[1]]).unwrap_or([0.0; 2])
}

/// Names to resolve once every file is read.
#[derive(Default)]
struct Pending {
    shape_transform: Vec<(usize, String)>,
    shape_geometries: Vec<(usize, Vec<String>)>,
    camera_transform: Vec<(usize, String)>,
    light_transform: Vec<(usize, String)>,
    pass: Vec<(usize, PassNames)>,
    settings: Vec<(SettingsAt, SettingsNames)>,
    shader_params: Vec<(usize, Vec<ParamNames>)>,
    scene_backgrounds: Vec<usize>,
    widget_textures: Vec<String>,
}

struct PassNames {
    color: String,
    depth: String,
    camera: String,
    shapes: Vec<String>,
    backgrounds: Vec<String>,
}

#[derive(Clone, Copy)]
enum SettingsAt {
    Shape(usize, usize),
    Background(usize),
    Override(usize),
}

#[derive(Default, Clone)]
struct SettingsNames {
    cull: String,
    blend: String,
    depth: String,
    alpha: String,
    stencil: String,
    raster: String,
    vertex: String,
    fragment: String,
    vertex_params: Vec<ParamNames>,
    fragment_params: Vec<ParamNames>,
}

#[derive(Clone)]
enum ParamNames {
    Transform { name: String, transform: String, pass: String, map: u32 },
    Constant { name: String, node: String, value: Vec4 },
    Texture { name: String, texture: String, state: String },
    Light { name: String, light: String, property: u32, space: u32 },
    Misc { name: String, param: u32 },
}

fn param_names(n: &Node) -> Option<ParamNames> {
    let name = n.str("paramName").unwrap_or("").to_string();
    Some(match n.kind {
        "TransformParam" => ParamNames::Transform {
            name,
            transform: n.str("transform").unwrap_or("").into(),
            pass: n.str("renderPass").unwrap_or("").into(),
            map: n.int("map").unwrap_or(0),
        },
        "ConstantParam" => ParamNames::Constant { name, node: n.name.clone(), value: v4(n, "value", [0.0; 4]) },
        "TextureParam" => ParamNames::Texture {
            name,
            texture: n.str("texture").unwrap_or("").into(),
            // Older viewers call it `state`.
            state: n.str("textureState").or(n.str("state")).unwrap_or("").into(),
        },
        "LightParam" => ParamNames::Light {
            name,
            light: n.str("light").unwrap_or("").into(),
            property: n.int("property").unwrap_or(0),
            space: n.int("space").unwrap_or(0),
        },
        "MiscParam" => ParamNames::Misc { name, param: n.int("param").unwrap_or(0) },
        _ => return None,
    })
}

fn settings_names(n: &Node) -> SettingsNames {
    let s = |f: &str| n.str(f).unwrap_or("").to_string();
    SettingsNames {
        cull: s("cullParams"),
        blend: s("blendParams"),
        depth: s("depthParams"),
        alpha: s("alphaParams"),
        stencil: s("stencilParams"),
        raster: s("rasterParams"),
        vertex: s("vertexShader"),
        fragment: s("fragmentShader"),
        vertex_params: n.many("vertexShaderParams").iter().filter_map(param_names).collect(),
        fragment_params: n.many("fragmentShaderParams").iter().filter_map(param_names).collect(),
    }
}

fn state_of(n: &Node) -> Option<State> {
    let b = |f: &str, d: bool| n.bool(f).unwrap_or(d);
    let i = |f: &str, d: u32| n.int(f).unwrap_or(d);
    let fl = |f: &str, d: f32| n.float(f).unwrap_or(d);
    Some(match n.kind {
        "CullParams" => State::Cull(Cull { ccw_front: b("ccwIsFront", true), face: i("cullFace", 1) }),
        "BlendParams" => State::Blend(Blend {
            color_src: i("colorSrcBlend", 1),
            color_dst: i("colorDestBlend", 0),
            alpha_src: i("alphaSrcBlend", 1),
            alpha_dst: i("alphaDestBlend", 0),
            op: i("blendOp", 0),
            color: v4(n, "blendColor", [0.0; 4]),
            mask: [b("redMask", true), b("greenMask", true), b("blueMask", true), b("alphaMask", true)],
        }),
        "DepthParams" => State::Depth(Depth {
            test: b("depthTestEnabled", true),
            func: i("depthTestFunc", 1),
            write: b("depthMask", true),
            offset_scale: fl("polygonOffsetScale", 0.0),
            offset_bias: fl("polygonOffsetBias", 0.0),
        }),
        "AlphaParams" => {
            State::Alpha(Alpha { test: b("alphaTestEnabled", false), func: i("alphaTestFunc", 1), reference: i("alphaTestRef", 0) })
        }
        "StencilParams" => State::Stencil(Stencil {
            enabled: b("stencilEnabled", false),
            func_mask: i("stencilFuncMask", 0),
            func: i("stencilFunc", 0),
            reference: i("stencilRef", 0),
            fail: i("stencilOp", 0),
            zpass: i("stencilZPassOp", 0),
            zfail: i("stencilZFailOp", 0),
            write_mask: b("stencilWriteMask", false),
        }),
        "RasterParams" => {
            State::Raster(Raster { point_style: i("pointStyle", 0), line_width: fl("lineWidth", 1.0), draw_style: i("drawStyle", 0) })
        }
        "TextureParams" => State::Texture(texture_state(n, TextureState::default())),
        _ => return None,
    })
}

fn texture_state(n: &Node, d: TextureState) -> TextureState {
    let i = |f: &str, v: u32| n.int(f).unwrap_or(v);
    TextureState {
        min: i("minFilter", d.min),
        mag: i("magFilter", d.mag),
        mip: i("mipFilter", d.mip),
        wrap: [i("wrapU", d.wrap[0]), i("wrapV", d.wrap[1]), i("wrapW", d.wrap[2])],
        bias: n.float("mipmapBias").unwrap_or(d.bias),
        anisotropy: n.float("anisotropy").unwrap_or(d.anisotropy),
        remapped: n.bool("remapped").unwrap_or(d.remapped),
        shadow: n.bool("enableShadowCompare").or(n.bool("doShadowCompare")).unwrap_or(d.shadow),
    }
}

fn array_of(n: &Node) -> Option<Array> {
    let channel = n.str("channelName").unwrap_or("").to_string();
    let data = match n.get("value")? {
        Value::Floats(v, k) => ArrayData::Floats(v.clone(), *k),
        Value::Halves(v, k) => ArrayData::Halves(v.clone(), *k),
        Value::Ints(v) => ArrayData::Bytes(v.iter().flat_map(|x| x.to_le_bytes()).collect(), 4),
        _ => return None,
    };
    Some(Array { channel, data })
}

impl World {
    fn add_name(&mut self, name: &str, n: Named) {
        if !name.is_empty() {
            // The first of a name wins, as the namespace keeps it.
            self.names.entry(name.to_string()).or_insert(n);
        }
    }

    fn add_transform(&mut self, n: &Node, parent: Option<usize>) {
        let kind = match n.kind {
            "MayaTransform" => TransformKind::Maya {
                scale_pivot: v3(n, "scalePivot", [0.0; 3]),
                scale_pivot_translate: v3(n, "scalePivotTranslate", [0.0; 3]),
                scale: v3(n, "scale", [1.0; 3]),
                rotation_pivot: v3(n, "rotationPivot", [0.0; 3]),
                rotation_pivot_translate: v3(n, "rotationPivotTranslate", [0.0; 3]),
                rotation_orientation: v3(n, "rotationOrientation", [0.0; 3]),
                rotation: v3(n, "rotation", [0.0; 3]),
                translation: v3(n, "translation", [0.0; 3]),
                rotation_order: n.int("rotationOrder").unwrap_or(0),
            },
            "MayaJoint" => TransformKind::Joint {
                scale: v3(n, "scale", [1.0; 3]),
                rotation_orientation: v3(n, "rotationOrientation", [0.0; 3]),
                rotation: v3(n, "rotation", [0.0; 3]),
                joint_orientation: v3(n, "jointOrientation", [0.0; 3]),
                translation: v3(n, "translation", [0.0; 3]),
                rotation_order: n.int("rotationOrder").unwrap_or(0),
            },
            "MatrixTransform" => {
                let m = n.vector("matrix").filter(|v| v.len() == 16).map(|v| {
                    let mut a = [0.0; 16];
                    a.copy_from_slice(v);
                    Mat4(a)
                });
                TransformKind::Matrix(m.unwrap_or_default())
            }
            _ => return,
        };
        let i = self.transforms.len();
        self.transforms.push(Transform { name: n.name.clone(), kind, parent, bind: Mat4::IDENTITY, world: Mat4::IDENTITY });
        self.add_name(&n.name, Named::Transform(i));
        for c in n.many("children") {
            self.add_transform(c, Some(i));
        }
    }

    fn add_shapes(&mut self, n: &Node, p: &mut Pending) -> Option<ShapeRef> {
        match n.kind {
            "ShapeGroup" | "ShapeLOD" => {
                let g = self.groups.len();
                self.groups.push(Group { name: n.name.clone(), children: vec![] });
                self.add_name(&n.name, Named::Group(g));
                let mut children = vec![];
                for c in n.many("children") {
                    if let Some(r) = self.add_shapes(c, p) {
                        children.push(r);
                    }
                }
                self.groups[g].children = children;
                Some(ShapeRef::Group(g))
            }
            "Shape" => {
                let s = self.shapes.len();
                let settings: Vec<&Node> = match n.get("renderSettings") {
                    Some(Value::Many(v)) => v.iter().collect(),
                    Some(Value::One(v)) => vec![v],
                    _ => vec![],
                };
                for (k, rs) in settings.iter().enumerate() {
                    p.settings.push((SettingsAt::Shape(s, k), settings_names(rs)));
                }
                let mut sets = vec![];
                for rs in &settings {
                    sets.push(Settings {
                        sort_set: rs.int("sortSet").unwrap_or(0),
                        sort_type: rs.int("sortType").unwrap_or(0),
                        ..Default::default()
                    });
                }
                self.shapes.push(Shape {
                    name: n.name.clone(),
                    cull: n.int("cull").unwrap_or(1),
                    center: v3(n, "sphereCenter", [0.0; 3]),
                    radius: n.float("sphereRadius").unwrap_or(0.0),
                    transform: None,
                    settings: sets,
                    geometries: vec![],
                    world_center: [0.0; 3],
                    world_radius: 0.0,
                });
                self.add_name(&n.name, Named::Shape(s));
                p.shape_transform.push((s, n.str("transform").unwrap_or("").into()));
                p.shape_geometries.push((s, n.strs("geometries").to_vec()));
                Some(ShapeRef::Shape(s))
            }
            _ => None,
        }
    }

    /// Adds one file's scene.
    pub fn add_scene(&mut self, scene: &Node, p: &mut PendingNames) {
        let p = &mut p.0;
        for n in scene.many("transforms") {
            self.add_transform(n, None);
        }
        for n in scene.many("cameras") {
            let i = self.cameras.len();
            let ortho = n.kind == "OrthoCamera";
            self.cameras.push(Camera {
                name: n.name.clone(),
                ortho,
                screen_aspect: n.bool("useScreenAspectRatio").unwrap_or(true),
                near: n.float("near").unwrap_or(0.1),
                far: n.float("far").unwrap_or(1000.0),
                fov: n.float("fieldOfView").unwrap_or(0.8),
                aspect: n.float("aspectRatio").unwrap_or(1.333),
                left: n.float("left").unwrap_or(-1.0),
                right: n.float("right").unwrap_or(1.0),
                bottom: n.float("bottom").unwrap_or(-1.0),
                top: n.float("top").unwrap_or(1.0),
                position: v3(n, "position", [0.0; 3]),
                orientation: v4(n, "orientation", [1.0, 0.0, 0.0, 0.0]),
                transform: None,
                view: Mat4::IDENTITY,
                proj: Mat4::IDENTITY,
                view_proj: Mat4::IDENTITY,
                eye: [0.0; 3],
                dir: [0.0, 0.0, -1.0],
            });
            self.add_name(&n.name, Named::Camera(i));
            p.camera_transform.push((i, n.str("transform").unwrap_or("").into()));
        }
        for n in scene.many("lights") {
            let kind = match n.kind {
                "DirectionalLight" => LightKind::Directional,
                "PointLight" => LightKind::Point,
                "SpotLight" => LightKind::Spot,
                _ => continue,
            };
            let i = self.lights.len();
            self.lights.push(Light {
                name: n.name.clone(),
                kind,
                transform: None,
                color: v3(n, "color", [1.0; 3]),
                intensity: n.float("intensity").unwrap_or(1.0),
                position: v3(n, "position", [0.0; 3]),
                direction: v3(n, "direction", [0.0, 0.0, -1.0]),
                inner: n.float("inner").unwrap_or(0.0),
                outer: n.float("outer").unwrap_or(0.0),
                attenuation: v3(n, "attenuation", [1.0, 0.0, 0.0]),
                world_position: [0.0; 3],
                world_direction: [0.0; 3],
            });
            self.add_name(&n.name, Named::Light(i));
            p.light_transform.push((i, n.str("transform").unwrap_or("").into()));
        }
        for n in scene.many("shaders") {
            let i = self.shaders.len();
            let params = n.many("params").iter().filter_map(param_names).collect();
            self.shaders.push(Shader {
                name: n.name.clone(),
                filename: n.str("filename").unwrap_or("").into(),
                profile: n.int("profile").unwrap_or(0),
                entry: n.str("entryFunc").unwrap_or("").into(),
                params: vec![],
            });
            p.shader_params.push((i, params));
            self.add_name(&n.name, Named::Shader(i));
        }
        for n in scene.many("textures") {
            let source = match n.kind {
                "2DTexture" => TextureSource::File(n.str("filename").unwrap_or("").into()),
                "3DTexture" => TextureSource::File3D(n.str("filename").unwrap_or("").into()),
                "ProcTexture" => {
                    let d = n.ivec("dimensions").unwrap_or(&[0, 0]);
                    TextureSource::Proc { width: d[0] as u32, height: d[1] as u32, samples: n.int("numSamples").unwrap_or(0) }
                }
                "CubicTexture" => {
                    let f = |k: &str| n.str(k).unwrap_or("").to_string();
                    TextureSource::Cube([f("xpos"), f("xneg"), f("ypos"), f("yneg"), f("zpos"), f("zneg")])
                }
                "ProcCubicTexture" => TextureSource::ProcCube { size: n.int("faceSize").unwrap_or(0) },
                _ => continue,
            };
            let state = n.has("minFilter").then(|| texture_state(n, TextureState::default()));
            let i = self.textures.len();
            self.textures.push(Texture { name: n.name.clone(), source, format: n.int("format").unwrap_or(24), state });
            self.add_name(&n.name, Named::Texture(i));
        }
        for n in scene.many("renderParams") {
            if let Some(s) = state_of(n) {
                let i = self.states.len();
                self.states.push((n.name.clone(), s));
                self.add_name(&n.name, Named::State(i));
            }
        }
        for n in scene.many("geometries") {
            let i = self.geometries.len();
            let arrays = n.many("data").iter().filter_map(array_of).collect();
            let lists = n
                .many("indices")
                .iter()
                .map(|v| {
                    let idx = match v.get("indices") {
                        Some(Value::Shorts(s)) => s.clone(),
                        _ => vec![],
                    };
                    (v.int("primType").unwrap_or(3), idx)
                })
                .collect();
            self.geometries.push(Geometry {
                name: n.name.clone(),
                restart: n.bool("useRestartPrimIndex").unwrap_or(false),
                arrays,
                lists,
            });
            self.add_name(&n.name, Named::Geometry(i));
        }
        for n in scene.many("shapes") {
            if let Some(r) = self.add_shapes(n, p) {
                self.scene_shapes.push(r);
            }
        }
        for n in scene.many("renderPasses") {
            let i = self.passes.len();
            let vp = n.ivec("viewport").map(|v| [v[0], v[1], v[2], v[3]]).unwrap_or([0, 0, -1, -1]);
            let ov = n.one("overrideSettings");
            self.passes.push(Pass {
                name: n.name.clone(),
                color: None,
                depth: None,
                clears_color: n.bool("clearsColor").unwrap_or(false),
                clear_color: v4(n, "clearColor", [0.0; 4]),
                clears_depth: n.bool("clearsDepth").unwrap_or(false),
                clear_depth: n.float("clearDepth").unwrap_or(1.0),
                clears_stencil: n.bool("clearsStencil").unwrap_or(false),
                clear_stencil: n.int("clearStencil").unwrap_or(0),
                camera: None,
                use_scene_shapes: n.bool("useSceneShapes").unwrap_or(true),
                shapes: vec![],
                use_scene_backgrounds: n.bool("useSceneBackgrounds").unwrap_or(true),
                backgrounds: vec![],
                override_settings: ov.map(|o| Settings {
                    sort_set: o.int("sortSet").unwrap_or(0),
                    sort_type: o.int("sortType").unwrap_or(0),
                    ..Default::default()
                }),
                use_list_indices: n.bool("useListIndices").unwrap_or(false),
                cube_face: n.int("cubeFace").unwrap_or(5),
                viewport: vp,
            });
            if let Some(o) = ov {
                p.settings.push((SettingsAt::Override(i), settings_names(o)));
            }
            self.add_name(&n.name, Named::Pass(i));
            p.pass.push((
                i,
                PassNames {
                    color: n.str("colorTarget").unwrap_or("").into(),
                    depth: n.str("depthTarget").unwrap_or("").into(),
                    camera: n.str("camera").unwrap_or("").into(),
                    shapes: n.strs("shapes").to_vec(),
                    backgrounds: n.strs("backgrounds").to_vec(),
                },
            ));
        }
        for n in scene.many("backgrounds") {
            let i = self.backgrounds.len();
            let rs = match n.get("renderSettings") {
                Some(Value::One(v)) => Some(v.as_ref()),
                Some(Value::Many(v)) => v.first(),
                _ => None,
            };
            self.backgrounds.push(Background {
                name: n.name.clone(),
                settings: Settings {
                    sort_set: rs.and_then(|r| r.int("sortSet")).unwrap_or(0),
                    sort_type: rs.and_then(|r| r.int("sortType")).unwrap_or(0),
                    ..Default::default()
                },
                position_scale: n.float("positionScale").unwrap_or(0.0),
                fov_scale: n.float("fovScale").unwrap_or(1.0),
            });
            if let Some(rs) = rs {
                p.settings.push((SettingsAt::Background(i), settings_names(rs)));
            }
            self.add_name(&n.name, Named::Background(i));
            p.scene_backgrounds.push(i);
        }
        for n in scene.many("animations") {
            let i = self.animations.len();
            self.animations.push(Animation::read(n));
            self.add_name(&n.name, Named::Animation(i));
        }
        for n in scene.many("menus") {
            self.menus.push(self.widget(n, p));
        }
        for n in scene.many("sounds") {
            self.sounds.push(Sound {
                name: n.name.clone(),
                filename: n.str("filename").unwrap_or("").into(),
                looping: n.bool("loop").unwrap_or(false),
                volume: n.float("volume").unwrap_or(1.0),
                play: n.bool("play").unwrap_or(false),
            });
        }
    }

    fn widget(&self, n: &Node, p: &mut Pending) -> Widget {
        let name = n.name.clone();
        let img = |f: &str, p: &mut Pending| {
            p.widget_textures.push(n.str(f).unwrap_or("").to_string());
            Some(p.widget_textures.len() - 1)
        };
        match n.kind {
            "StaticMenu" => Widget::Static { name, items: n.many("menuItems").iter().map(|c| self.widget(c, p)).collect() },
            "DropDownMenu" => Widget::DropDown {
                name,
                items: n.many("menuItems").iter().map(|c| self.widget(c, p)).collect(),
                open_position: v2(n, "openPosition"),
                open_size: v2(n, "openSize"),
                image: img("buttonImage", p),
                initially_open: n.bool("initiallyOpen").unwrap_or(false),
            },
            "OnOffButton" => Widget::OnOff {
                name,
                callback: n.str("callback").unwrap_or("").into(),
                initial: n.bool("initial").unwrap_or(false),
                position: v2(n, "position"),
                size: v2(n, "size"),
                on_color: v3(n, "onColor", [1.0; 3]),
                off_color: v3(n, "offColor", [0.5; 3]),
                image: img("buttonImage", p),
            },
            "TexSlider" => Widget::TexSlider {
                name,
                callback: n.str("callback").unwrap_or("").into(),
                initial: n.float("initial").unwrap_or(0.0),
                alignment: n.int("alignment").unwrap_or(0),
                position: v2(n, "position"),
                size: v2(n, "size"),
                slider: img("slider", p),
                knob: img("knob", p),
            },
            "Icon" => Widget::Icon { name, image: img("image", p), position: v2(n, "position"), size: v2(n, "size") },
            _ => Widget::Other { name, kind: n.kind.to_string() },
        }
    }

    pub fn find(&self, name: &str) -> Option<Named> {
        self.names.get(name).copied()
    }

    pub fn transform(&self, name: &str) -> Option<usize> {
        match self.find(name)? {
            Named::Transform(i) => Some(i),
            _ => None,
        }
    }

    pub fn texture(&self, name: &str) -> Option<usize> {
        match self.find(name)? {
            Named::Texture(i) => Some(i),
            _ => None,
        }
    }

    pub fn camera(&self, name: &str) -> Option<usize> {
        match self.find(name)? {
            Named::Camera(i) => Some(i),
            _ => None,
        }
    }

    pub fn shape_ref(&self, name: &str) -> Option<ShapeRef> {
        match self.find(name)? {
            Named::Shape(i) => Some(ShapeRef::Shape(i)),
            Named::Group(i) => Some(ShapeRef::Group(i)),
            _ => None,
        }
    }

    /// A state by name: the scene's, then the built-ins (made on first use).
    fn state(&mut self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        if let Some(Named::State(i)) = self.find(name) {
            return Some(i);
        }
        let s = builtin(name)?;
        let i = self.states.len();
        self.states.push((name.to_string(), s));
        self.add_name(name, Named::State(i));
        Some(i)
    }

    fn resolve_param(&mut self, p: &ParamNames) -> Param {
        match p {
            ParamNames::Transform { name, transform, pass, map } => Param::Transform {
                name: name.clone(),
                transform: self.transform(transform),
                pass: match self.find(pass) {
                    Some(Named::Pass(i)) => Some(i),
                    _ => None,
                },
                map: *map,
            },
            ParamNames::Constant { name, node, value } => Param::Constant { name: name.clone(), node: node.clone(), value: *value },
            ParamNames::Texture { name, texture, state } => {
                Param::Texture { name: name.clone(), texture: self.texture(texture), state: self.state(state) }
            }
            ParamNames::Light { name, light, property, space } => Param::Light {
                name: name.clone(),
                light: match self.find(light) {
                    Some(Named::Light(i)) => Some(i),
                    _ => None,
                },
                property: *property,
                space: *space,
            },
            ParamNames::Misc { name, param } => Param::Misc { name: name.clone(), param: *param },
        }
    }

    fn resolve_settings(&mut self, s: &mut Settings, n: &SettingsNames) {
        s.cull = self.state(&n.cull);
        s.blend = self.state(&n.blend);
        s.depth = self.state(&n.depth);
        s.alpha = self.state(&n.alpha);
        s.stencil = self.state(&n.stencil);
        s.raster = self.state(&n.raster);
        let shader = |w: &World, name: &str| match w.find(name) {
            Some(Named::Shader(i)) => Some(i),
            _ => None,
        };
        s.vertex = shader(self, &n.vertex);
        s.fragment = shader(self, &n.fragment);
        s.vertex_params = n.vertex_params.iter().map(|p| self.resolve_param(p)).collect();
        s.fragment_params = n.fragment_params.iter().map(|p| self.resolve_param(p)).collect();
    }

    /// Resolves the names of every file added.
    pub fn resolve(&mut self, p: PendingNames) {
        let p = p.0;
        for (s, t) in p.shape_transform {
            self.shapes[s].transform = self.transform(&t);
        }
        for (s, g) in p.shape_geometries {
            self.shapes[s].geometries = g
                .iter()
                .filter_map(|n| match self.find(n) {
                    Some(Named::Geometry(i)) => Some(i),
                    _ => None,
                })
                .collect();
        }
        for (c, t) in p.camera_transform {
            self.cameras[c].transform = self.transform(&t);
        }
        for (l, t) in p.light_transform {
            self.lights[l].transform = self.transform(&t);
        }
        for (i, params) in p.shader_params {
            self.shaders[i].params = params.iter().map(|q| self.resolve_param(q)).collect();
        }
        for (at, names) in p.settings {
            let mut s = match at {
                SettingsAt::Shape(i, k) => std::mem::take(&mut self.shapes[i].settings[k]),
                SettingsAt::Background(i) => std::mem::take(&mut self.backgrounds[i].settings),
                SettingsAt::Override(i) => self.passes[i].override_settings.take().unwrap_or_default(),
            };
            self.resolve_settings(&mut s, &names);
            match at {
                SettingsAt::Shape(i, k) => self.shapes[i].settings[k] = s,
                SettingsAt::Background(i) => self.backgrounds[i].settings = s,
                SettingsAt::Override(i) => self.passes[i].override_settings = Some(s),
            }
        }
        for (i, n) in p.pass {
            let color = self.texture(&n.color);
            let depth = self.texture(&n.depth);
            let camera = self.camera(&n.camera);
            let shapes = n.shapes.iter().filter_map(|s| self.shape_ref(s)).collect();
            let backgrounds = n
                .backgrounds
                .iter()
                .filter_map(|b| match self.find(b) {
                    Some(Named::Background(i)) => Some(i),
                    _ => None,
                })
                .collect();
            let pass = &mut self.passes[i];
            pass.color = color;
            pass.depth = depth;
            pass.camera = camera;
            pass.shapes = shapes;
            pass.backgrounds = backgrounds;
        }
        self.widget_textures = p.widget_textures.iter().map(|n| self.texture(n)).collect();
        self.scene_backgrounds = p.scene_backgrounds;
    }
}

/// Names gathered while files are added.
#[derive(Default)]
pub struct PendingNames(Pending);
