//! A scene: transforms, meshes, morph targets, shapes and what they are drawn with.
use std::collections::BTreeMap;

use glam::{EulerRot, Mat4, Vec3, Vec4};

use crate::Error;
use crate::anim::Animation;
use crate::data::Files;
use crate::hair::Hair;
use crate::tree::Node;

#[derive(Clone, Copy)]
struct Fields {
    scale: Vec3,
    rotation: Vec3,
    translation: Vec3,
}

struct Transform {
    name: String,
    parent: Option<usize>,
    joint: bool,
    rest: Fields,
    orientation: Vec3,
    joint_orientation: Vec3,
    scale_pivot: Vec3,
    scale_shift: Vec3,
    pivot: Vec3,
    pivot_shift: Vec3,
}

/// Around x, then y, then z.
fn euler(v: Vec3) -> Mat4 {
    Mat4::from_euler(EulerRot::ZYX, v.z, v.y, v.x)
}

impl Transform {
    fn read(n: &Node, parent: Option<usize>) -> Self {
        let v = |f| n.vec::<3>(f).map_or(Vec3::ZERO, Vec3::from);
        Self {
            name: n.name.clone(),
            parent,
            joint: n.kind == "MayaJoint",
            rest: Fields {
                scale: n.vec::<3>("scale").map_or(Vec3::ONE, Vec3::from),
                rotation: v("rotation"),
                translation: v("translation"),
            },
            orientation: v("rotationOrientation"),
            joint_orientation: v("jointOrientation"),
            scale_pivot: v("scalePivot"),
            scale_shift: v("scalePivotTranslate"),
            pivot: v("rotationPivot"),
            pivot_shift: v("rotationPivotTranslate"),
        }
    }

    /// As Maya composes them; the last factor acts first. Every transform in
    /// the shipped files uses `xyz` rotation order, so one composition fits.
    fn matrix(&self, f: Fields) -> Mat4 {
        let t = Mat4::from_translation;
        let turned = euler(f.rotation) * euler(self.orientation);
        if self.joint {
            return t(f.translation)
                * euler(self.joint_orientation)
                * turned
                * Mat4::from_scale(f.scale);
        }
        t(f.translation + self.pivot_shift + self.pivot)
            * turned
            * t(-self.pivot + self.scale_shift + self.scale_pivot)
            * Mat4::from_scale(f.scale)
            * t(-self.scale_pivot)
    }
}

#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Moving {
    pub position: [f32; 3],
    /// With the vertex's own occlusion.
    pub normal: [f32; 4],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Fixed {
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    /// Up to eight skinning influences; unused slots are zero.
    pub weights: [f32; 8],
    pub bones: [f32; 8],
}

/// Differences from the mesh, for the vertices that have any.
pub struct Morph {
    changes: Vec<(u32, Vec3, Vec4)>,
}

pub struct Mesh {
    pub moving: Vec<Moving>,
    pub fixed: Vec<Fixed>,
    pub indices: Vec<u32>,
    pub morphs: Vec<Morph>,
}

impl Mesh {
    /// The mesh with morph targets applied.
    pub fn morphed(&self, weights: &[(usize, f32)]) -> Vec<Moving> {
        let mut out = self.moving.clone();
        for &(target, weight) in weights {
            for &(i, position, normal) in &self.morphs[target].changes {
                let v = &mut out[i as usize];
                v.position = (Vec3::from(v.position) + weight * position).into();
                v.normal = (Vec4::from(v.normal) + weight * normal).into();
            }
        }
        for v in &mut out {
            let n = Vec4::from(v.normal);
            v.normal = n.truncate().normalize_or_zero().extend(n.w).into();
        }
        out
    }
}

pub enum Geometry {
    Mesh(Mesh),
    Hair(Hair),
    /// A subdivision control cage: quads as pairs of triangles.
    Cage(Mesh),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Add,
}

/// How the shape is drawn: shadow depth, color, an overlay, or not at all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// Writes depth for a shadow map or the main view.
    Depth,
    /// The lit picture.
    Color,
    /// Smoke, sparks and flames drawn last.
    Card,
    /// Post passes and runtime-placed cards, which the port does not draw.
    Skip,
}

pub struct Shape {
    pub name: String,
    pub geometry: Geometry,
    pub transform: Option<usize>,
    /// Transforms, and whether they bend the mesh from its rest (or just carry it).
    pub bones: Vec<(usize, bool)>,
    /// The vertex and fragment programs, by the viewer's names.
    pub vertex: String,
    pub fragment: String,
    /// Texture nodes by the fragment program's names for them.
    pub textures: BTreeMap<String, String>,
    /// Light, property and space bindings of the vertex program.
    pub lights: Vec<(String, String, u32)>,
    /// Transform bindings of the vertex program: matrix name and map.
    pub matrices: Vec<(String, String, u32)>,
    /// Misc bindings of the vertex program.
    pub misc: Vec<(String, u32)>,
    /// Morph weight parameters of this shape's geometry, in target order.
    pub weights: Vec<String>,
    /// A particle system drawn from this shape, if any.
    pub system: Option<String>,
    pub cull: bool,
    pub blend: Blend,
    pub pass: Pass,
    /// Lower sets are drawn first; within one, file order.
    pub set: u32,
    pub far_first: bool,
    pub center: Vec3,
}

pub enum Texture {
    Plane {
        file: String,
        /// Texels are -1 to 1.
        signed: bool,
        repeat: [bool; 2],
    },
    Cross {
        file: String,
    },
    /// A smoke volume (`.t3d`).
    Volume {
        file: String,
    },
}

pub struct Model {
    transforms: Vec<Transform>,
    rest: Vec<Mat4>,
    pub shapes: Vec<Shape>,
    pub textures: BTreeMap<String, Texture>,
    pub animations: Vec<Animation>,
    /// Morph weight values by node name, driven by the animation.
    pub blends: BTreeMap<String, f32>,
}

/// `models/..` paths of scene files are relative to the program's folder.
fn params<'a>(n: &'a Node, field: &str, kind: &'a str) -> impl Iterator<Item = &'a Node> {
    n.nodes(field).iter().filter(move |p| p.kind == kind)
}

/// Triangles of a list (3), of strips (5) with restarts, or of quad patches
/// (8): four indices and a restart per quad.
fn triangles(kind: u32, indices: &[u32], out: &mut Vec<u32>) -> Result<(), Error> {
    match kind {
        3 => out.extend(indices),
        5 => {
            for strip in indices.split(|i| *i == 0xffff) {
                for (k, t) in strip.windows(3).enumerate() {
                    if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                        out.extend(if k % 2 == 0 {
                            [t[0], t[1], t[2]]
                        } else {
                            [t[1], t[0], t[2]]
                        });
                    }
                }
            }
        }
        8 => {
            for quad in indices.split(|i| *i == 0xffff) {
                if quad.is_empty() {
                    continue;
                }
                if quad.len() == 3 {
                    out.extend([quad[0], quad[1], quad[2]]);
                } else if quad.len() == 4 {
                    out.extend([quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
                } else {
                    return Err(format!("quad patch of {}", quad.len()).into());
                }
            }
        }
        other => return Err(format!("unknown primitive {other}").into()),
    }
    Ok(())
}

fn mesh(n: &Node) -> Result<Mesh, Error> {
    let arrays: Vec<Node> = n.nodes("data").to_vec();
    let stream = |wanted: &str| -> Option<(Vec<f32>, usize)> {
        let found = arrays.iter().find(|v| {
            let name = v.text("channelName");
            name.strip_prefix("a2v.").unwrap_or(name) == wanted
        })?;
        let width = match found.kind {
            "FloatArray" => 1,
            "Pnt2fArray" | "Vec2fArray" => 2,
            "Pnt3fArray" | "Vec3fArray" => 3,
            _ => 4,
        };
        Some((found.floats("value").to_vec(), width))
    };
    let one = |names: &[&str]| names.iter().find_map(|s| stream(s));
    let (coords, 3) = one(&["objCoord", "worldCoord"]).ok_or("mesh without positions")? else {
        return Err("positions have three parts".into());
    };
    let count = coords.len() / 3;
    let vectors = |names: &[&str], fill: Vec4| -> Result<Vec<Vec4>, Error> {
        let Some((data, width)) = one(names) else {
            return Ok(vec![fill; count]);
        };
        if data.len() != count * width {
            return Err(format!("{}: vertex streams differ in length", n.name).into());
        }
        let vector = |c: &[f32]| {
            let mut v = fill;
            for (i, x) in c.iter().enumerate() {
                v[i] = *x;
            }
            v
        };
        Ok(data.chunks_exact(width).map(vector).collect())
    };
    let normals = vectors(&["objNormal", "worldNormal"], Vec4::ZERO)?;
    let tangents = vectors(&["objTangent", "worldTangent"], Vec4::X + Vec4::W)?;
    let uvs = vectors(&["texture", "bumpTex"], Vec4::ZERO)?;
    // Two sets of up to four influences fill the eight slots.
    let weights: Vec<[f32; 8]> = {
        let w0 = vectors(&["boneWeights0"], Vec4::ZERO)?;
        let w1 = vectors(&["boneWeights1"], Vec4::ZERO)?;
        w0.into_iter()
            .zip(w1)
            .map(|(a, b)| [a.x, a.y, a.z, a.w, b.x, b.y, b.z, b.w])
            .collect()
    };
    let bones: Vec<[f32; 8]> = {
        let b0 = vectors(&["boneIndices0"], Vec4::ZERO)?;
        let b1 = vectors(&["boneIndices1"], Vec4::ZERO)?;
        b0.into_iter()
            .zip(b1)
            .map(|(a, b)| {
                let clean = |v: Vec4| {
                    let mut a = [v.x, v.y, v.z, v.w];
                    for x in a.iter_mut() {
                        if *x < -0.5 {
                            *x = 0.0;
                        }
                    }
                    a
                };
                let (a, b) = (clean(a), clean(b));
                [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3]]
            })
            .collect()
    };
    let mut moving = Vec::with_capacity(count);
    let mut fixed = Vec::with_capacity(count);
    for i in 0..count {
        // Unused slots are zero, so they change nothing.
        let (w, b) = (weights[i], bones[i]);
        moving.push(Moving {
            position: [coords[i * 3], coords[i * 3 + 1], coords[i * 3 + 2]],
            normal: normals[i].into(),
        });
        fixed.push(Fixed {
            tangent: tangents[i].into(),
            uv: [uvs[i].x, uvs[i].y],
            weights: w,
            bones: b,
        });
    }
    // Morph targets: `objCoordDiffN` with `objNormalDiffN`, driven by the
    // `morphWeightN` parameters of the same geometry.
    let mut morphs = Vec::new();
    for k in 0.. {
        let coord = format!("objCoordDiff{k}");
        if stream(&coord).is_none() {
            break;
        }
        let coords = vectors(&[coord.as_str()], Vec4::ZERO)?;
        let normals = vectors(&[format!("objNormalDiff{k}").as_str()], Vec4::ZERO)?;
        let changes = coords.iter().zip(normals).enumerate();
        let changes = changes
            .filter(|(_, (c, n))| c.truncate() != Vec3::ZERO || *n != Vec4::ZERO)
            .map(|(i, (c, n))| (i as u32, c.truncate(), n));
        morphs.push(Morph {
            changes: changes.collect(),
        });
    }
    let mut indices = Vec::new();
    for i in n.nodes("indices") {
        triangles(i.int("primType"), i.ints("indices"), &mut indices)?;
    }
    if indices.iter().any(|i| *i as usize >= count) {
        return Err("bad vertex index".into());
    }
    Ok(Mesh {
        moving,
        fixed,
        indices,
        morphs,
    })
}

impl Model {
    pub fn new(root: &Node, files: &Files) -> Result<Self, Error> {
        let mut model = Self {
            transforms: Vec::new(),
            rest: Vec::new(),
            shapes: Vec::new(),
            textures: BTreeMap::new(),
            animations: Vec::new(),
            blends: BTreeMap::new(),
        };
        for n in root.nodes("transforms") {
            model.walk(n, None);
        }
        let fields: Vec<_> = model.transforms.iter().map(|t| t.rest).collect();
        model.rest = model.compose(&fields);
        for n in root.nodes("textures") {
            let file = n.text("filename").to_owned();
            let texture = match n.kind {
                "2DTexture" => Texture::Plane {
                    file,
                    signed: false,
                    // `0` clamps and `1` repeats.
                    repeat: ["wrapU", "wrapV"].map(|f| n.int(f) == 1),
                },
                "CubicCrossTexture" => Texture::Cross { file },
                "3DTexture" => Texture::Volume { file },
                _ => continue,
            };
            model.textures.insert(n.name.clone(), texture);
        }
        // All morph weight parameters, by node name: `<geometry>_blend_0_<k>`.
        for (parent, many) in [
            ("shaders", "defaultParams"),
            ("geometries", "processParams"),
        ] {
            for holder in root.nodes(parent) {
                for p in params(holder, many, "ConstantParam") {
                    if p.name.contains("_blend_") {
                        model.blends.insert(p.name.clone(), 0.0);
                    }
                }
            }
        }
        for g in root.nodes("shapes") {
            // Shape groups hold the shapes; a bare shape draws as is.
            let mut inner = g.nodes("children");
            if inner.is_empty() && g.kind == "Shape" {
                inner = std::slice::from_ref(g);
            }
            for s in inner {
                // Shapes without a geometry are filled in at runtime (the
                // grown hair); the port grows it instead.
                if s.text("geometry").is_empty() {
                    continue;
                }
                let shape = model.read_shape(s, root, files);
                model
                    .shapes
                    .push(shape.map_err(|e| format!("{}: {e}", s.name))?);
            }
        }
        for a in root.nodes("animations") {
            model.animations.push(Animation::read(a)?);
        }
        Ok(model)
    }

    fn walk(&mut self, n: &Node, parent: Option<usize>) {
        let at = self.transforms.len();
        self.transforms.push(Transform::read(n, parent));
        for child in n.nodes("children") {
            self.walk(child, Some(at));
        }
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.transforms.iter().position(|t| t.name == name)
    }

    pub fn ancestors(&self, start: Option<usize>) -> impl Iterator<Item = &str> {
        std::iter::successors(start, |&at| self.transforms[at].parent)
            .map(|at| self.transforms[at].name.as_str())
    }

    pub fn rest(&self) -> &[Mat4] {
        &self.rest
    }

    pub fn shape(&self, i: usize) -> &Shape {
        &self.shapes[i]
    }

    /// A model holding one shape.
    pub fn lone(shape: Shape) -> Self {
        Self {
            transforms: Vec::new(),
            rest: Vec::new(),
            shapes: vec![shape],
            textures: BTreeMap::new(),
            animations: Vec::new(),
            blends: BTreeMap::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn read_shape(&self, s: &Node, root: &Node, files: &Files) -> Result<Shape, Error> {
        let settings = root
            .nodes("renderSettings")
            .iter()
            .find(|r| r.name == s.text("renderSettings"))
            .ok_or_else(|| format!("missing settings {}", s.text("renderSettings")))?;
        let geometry = s.text("geometry");
        let g = root
            .nodes("geometries")
            .iter()
            .find(|g| g.name == geometry)
            .ok_or_else(|| format!("missing geometry {geometry}"))?;
        let built: (Geometry, Option<String>) = match g.kind {
            "IndexedFaceSet" | "ProcessedMesh" | "Mesh" => (Geometry::Mesh(mesh(g)?), None),
            "SubdivisionMesh" => (Geometry::Cage(mesh(g)?), None),
            "HairPatch" => {
                let text = files.text(g.text("hairFile"))?;
                (
                    Geometry::Hair(Hair::new(
                        &text,
                        g.float("density") as usize,
                        g.int("numSegments") as usize,
                        1.0,
                        0.0,
                    )?),
                    None,
                )
            }
            "ParticleSystem" => (
                Geometry::Mesh(Mesh {
                    moving: Vec::new(),
                    fixed: Vec::new(),
                    indices: Vec::new(),
                    morphs: Vec::new(),
                }),
                Some(geometry.to_owned()),
            ),
            other => return Err(format!("unsupported geometry {other}").into()),
        };
        let (geo, system) = built;
        // Bones: the geometry's `bone[n]` slots bend it (CPU skinning, `map`
        // 23); the shape's `model[n]`/`modelViewProj[n]` slots do the same for
        // the shadow passes. Otherwise the shape's transform carries the mesh.
        let mut bones: Vec<(usize, usize, bool)> = Vec::new();
        let mut matrices = Vec::new();
        for p in params(g, "processParams", "TransformParam") {
            let map = p.int("map");
            let name = p.text("paramName");
            if map == 23
                && let Some(index) = name.strip_prefix("bone").and_then(|n| n.parse().ok())
            {
                let transform = self.find(p.text("transform")).ok_or("missing bone")?;
                bones.push((index, transform, true));
            }
        }
        for p in params(s, "vertexShaderParams", "TransformParam") {
            let map = p.int("map");
            matrices.push((
                p.text("paramName").to_owned(),
                p.text("transform").to_owned(),
                map,
            ));
            let bend = match map {
                7 => false,
                23 | 26 => true,
                _ => continue,
            };
            let name = p.text("paramName");
            let place = name
                .split_once('[')
                .map_or(Ok(0), |(_, n)| n.trim_end_matches(']').parse())?;
            if p.text("transform").is_empty() {
                continue;
            }
            let transform = self.find(p.text("transform")).ok_or("missing transform")?;
            bones.push((place, transform, bend));
        }
        bones.sort_unstable();
        bones.dedup_by_key(|b| b.0);
        if !bones.is_empty() && bones.iter().enumerate().any(|(i, b)| b.0 != i) {
            return Err("bones are not numbered from zero".into());
        }
        let carry = self.find(s.text("transform"));
        if bones.is_empty()
            && let Some(t) = carry
        {
            bones.push((0, t, false));
        }
        if let Geometry::Mesh(m) | Geometry::Cage(m) = &geo {
            let limit = bones.len().max(1) as f32;
            if m.fixed
                .iter()
                .any(|f| f.bones.iter().any(|b| !(0.0..limit).contains(b)))
            {
                return Err("bad bone index".into());
            }
        }
        let mut weights: Vec<(usize, String)> = Vec::new();
        for (parent, many) in [
            ("shaders", "defaultParams"),
            ("geometries", "processParams"),
        ] {
            for holder in root.nodes(parent) {
                for p in params(holder, many, "ConstantParam") {
                    // The node name carries the target: `<geometry>_blend_0_<k>`.
                    // (The hair file also sets `paramName` to `morphWeight<k>`.)
                    if let Some(rest) = p.name.strip_prefix(&format!("{geometry}_blend_0_"))
                        && let Ok(k) = rest.parse::<usize>()
                    {
                        weights.push((k, p.name.clone()));
                    }
                }
            }
        }
        weights.sort_unstable();
        if let Geometry::Mesh(m) | Geometry::Cage(m) = &geo {
            // Weights drive the first of the morph targets; the rest (if any)
            // stay at their rest shape.
            if weights.iter().any(|(k, _)| *k >= m.morphs.len()) {
                return Err("morph weights exceed morph targets".into());
            }
        }
        let vertex = settings.text("vertexShader");
        let fragment = settings.text("fragmentShader");
        let mut textures = BTreeMap::new();
        // The shader's defaults, then the shape's own bindings.
        if let Some(shader) = root
            .nodes("shaders")
            .iter()
            .find(|n| n.name == fragment || n.name == vertex)
        {
            for field in ["globalParams", "defaultParams"] {
                for p in params(shader, field, "TextureParam") {
                    textures.insert(p.text("paramName").to_owned(), p.text("texture").to_owned());
                }
            }
        }
        for p in params(s, "fragmentShaderParams", "TextureParam") {
            textures.insert(p.text("paramName").to_owned(), p.text("texture").to_owned());
        }
        // Blend factors of the shipped `BlendParams` nodes, by settings name.
        let blend = match settings.text("blendParams") {
            "CardAlpha" | "BlendHair" | "BlendByDestAlpha_PreserveSrcAlpha" => Blend::Alpha,
            "BlendAdd_ColorOnly" => Blend::Add,
            _ => Blend::None,
        };
        // Light bindings come from the fragment program's globals; shapes
        // carry none of their own.
        let mut lights = Vec::new();
        if let Some(shader) = root
            .nodes("shaders")
            .iter()
            .find(|n| n.name == fragment || n.name == vertex)
        {
            for p in params(shader, "globalParams", "LightParam") {
                lights.push((
                    p.text("paramName").to_owned(),
                    p.text("light").to_owned(),
                    p.int("property"),
                ));
            }
        }
        for p in params(s, "vertexShaderParams", "LightParam") {
            lights.push((
                p.text("paramName").to_owned(),
                p.text("light").to_owned(),
                p.int("property"),
            ));
        }
        let mut misc = Vec::new();
        for p in params(s, "vertexShaderParams", "MiscParam") {
            misc.push((p.text("paramName").to_owned(), p.int("param")));
        }
        Ok(Shape {
            name: s.name.clone(),
            geometry: geo,
            transform: carry,
            bones: bones.into_iter().map(|(_, t, bend)| (t, bend)).collect(),
            vertex: vertex.trim_end_matches("_v").to_owned(),
            fragment: fragment.trim_end_matches("_f").to_owned(),
            textures,
            lights,
            matrices,
            misc,
            weights: weights.into_iter().map(|w| w.1).collect(),
            system: system.clone(),
            // Movie rigs can shoot through the back of the room shell.
            cull: s.int("cull") == 2
                || self.ancestors(carry).next().is_some_and(|name| {
                    name.starts_with("MMB_Wall_")
                        || matches!(name, "MMW_BackWall" | "MMW_SideWall" | "MMW_SideWall2")
                }),
            blend,
            pass: if vertex.starts_with("cast_") {
                Pass::Depth
            } else if system.is_some()
                || settings.text("rasterParams") == "PointSprite"
                || matches!(
                    fragment.trim_end_matches("_f"),
                    "card1"
                        | "card1_alpha"
                        | "copytex"
                        | "blit_far"
                        | "feedback"
                        | "composite"
                        | "composite_heat"
                        | "composite_heat_passthru"
                        | "flames"
                        | "blurs"
                        | "blur_2D_10pix"
                        | "blur_2D_rounded_square"
                        | "cube_blur_X"
                        | "cube_blur_Y"
                        | "cube_blur_Z"
                        | "HiPassFilter"
                        | "loResBlackShader"
                )
            {
                Pass::Card
            } else {
                Pass::Color
            },
            set: s.int("sortSet"),
            far_first: s.int("sortType") == 1,
            center: s.vec::<3>("sphereCenter").map_or(Vec3::ZERO, Vec3::from),
        })
    }

    fn compose(&self, fields: &[Fields]) -> Vec<Mat4> {
        let mut out: Vec<Mat4> = Vec::with_capacity(fields.len());
        // Parents come before their children.
        for (t, f) in self.transforms.iter().zip(fields) {
            let m = t.matrix(*f);
            out.push(t.parent.map_or(m, |p| out[p] * m));
        }
        out
    }

    /// Transforms to the world, with some of them moved from where they rest.
    pub fn pose(
        &self,
        animation: Option<(&Animation, f64)>,
        moved: &[(&str, Vec3, Vec3)],
    ) -> Vec<Mat4> {
        let mut fields: Vec<Fields> = self.transforms.iter().map(|t| t.rest).collect();
        if let Some((a, seconds)) = animation {
            for c in &a.channels {
                let Some(at) = self.find(&c.node) else {
                    continue;
                };
                let f = &mut fields[at];
                let v = match c.field.as_str() {
                    "rotation" => &mut f.rotation,
                    "scale" => &mut f.scale,
                    _ => &mut f.translation,
                };
                for (i, c) in c.curves.iter().enumerate() {
                    if let Some(c) = c {
                        v[i] = c.at(seconds);
                    }
                }
            }
        }
        for (name, translation, rotation) in moved {
            if let Some(at) = self.find(name) {
                (fields[at].translation, fields[at].rotation) = (*translation, *rotation);
            }
        }
        self.compose(&fields)
    }

    /// The matrices of a shape's bones.
    pub fn bones(&self, shape: &Shape, world: &[Mat4]) -> Vec<Mat4> {
        let one = |&(t, bend): &(usize, bool)| {
            if bend {
                world[t] * self.rest[t].inverse()
            } else {
                world[t]
            }
        };
        shape.bones.iter().map(one).collect()
    }
}

impl Shape {
    /// Morph targets in use: the first few whose weights count.
    pub fn animated(
        &self,
        values: &std::collections::HashMap<String, f32>,
        limit: usize,
        least: f32,
    ) -> Vec<(usize, f32)> {
        self.weights
            .iter()
            .enumerate()
            .filter_map(|(i, name)| Some((i, *values.get(name.as_str())?)))
            .filter(|(_, w)| *w > least)
            .take(limit)
            .collect()
    }

    pub fn targets(&self) -> usize {
        self.weights.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_keep_their_winding_and_restart() {
        let mut out = Vec::new();
        triangles(5, &[0, 1, 2, 3, 0xffff, 4, 5, 6, 6], &mut out).unwrap();
        assert_eq!(out, [0, 1, 2, 2, 1, 3, 4, 5, 6]);
        assert!(triangles(9, &[], &mut out).is_err());
    }

    #[test]
    fn quads_become_pairs() {
        let mut out = Vec::new();
        triangles(8, &[0, 1, 2, 3, 0xffff, 4, 5, 6, 7], &mut out).unwrap();
        assert_eq!(out, [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
        triangles(8, &[0, 1, 2], &mut out).unwrap();
        assert!(triangles(8, &[0, 1], &mut out).is_err());
    }

    #[test]
    fn euler_turns_x_first() {
        let v = euler(Vec3::new(
            std::f32::consts::FRAC_PI_2,
            0.0,
            std::f32::consts::FRAC_PI_2,
        ));
        assert!((v.transform_point3(Vec3::Y) - Vec3::Z).length() < 1e-6);
    }
}
