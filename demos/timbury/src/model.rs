//! A scene: transforms, skinned meshes, morph targets, shapes and materials.
use std::collections::HashMap;

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
    matrix: Option<Mat4>,
}

/// Around x, then y, then z.
fn euler(v: Vec3) -> Mat4 {
    Mat4::from_euler(EulerRot::ZYX, v.z, v.y, v.x)
}

impl Transform {
    fn read(n: &Node, parent: Option<usize>) -> Self {
        let v = |f| n.vec::<3>(f).map_or(Vec3::ZERO, Vec3::from);
        let matrix = if n.kind == "MatrixTransform" {
            n.floats("matrix")
                .try_into()
                .ok()
                .map(Mat4::from_cols_array)
        } else if n.kind == "MaxTransform" {
            let (r, t) = (v("rotation"), v("translation"));
            Some(Mat4::from_translation(t) * euler(r) * Mat4::from_scale(v("scale")))
        } else {
            None
        };
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
            matrix,
        }
    }

    /// As Maya composes them; the last factor acts first.
    fn matrix(&self, f: Fields) -> Mat4 {
        if let Some(m) = self.matrix {
            return m;
        }
        let t = Mat4::from_translation;
        let turned = euler(f.rotation) * euler(self.orientation);
        if self.joint {
            return t(f.translation)
                * euler(self.joint_orientation)
                * turned
                * Mat4::from_scale(f.scale);
        }
        t(f.translation) * turned * Mat4::from_scale(f.scale)
    }
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
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
    pub weights: [f32; 4],
    pub bones: [f32; 4],
}

/// Differences from the mesh, for the vertices that have any.
pub struct Morph {
    changes: Vec<(u32, Vec3, Vec3)>,
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
            if let Some(morph) = self.morphs.get(target) {
                for &(i, position, normal) in &morph.changes {
                    if let Some(v) = out.get_mut(i as usize) {
                        v.position = (Vec3::from(v.position) + weight * position).into();
                        let n = Vec4::from(v.normal) + weight * normal.extend(0.0);
                        v.normal = n.into();
                    }
                }
            }
        }
        for v in &mut out {
            let n = Vec4::from(v.normal);
            v.normal = n.truncate().normalize_or_zero().extend(n.w).into();
        }
        out
    }
}

pub struct GrassParams {
    pub blade_size: f32,
    pub density: f32,
    pub segments: usize,
    pub area: Option<[f32; 4]>,
    pub life_map: String,
    pub terrain: String,
    pub texture: String,
}

pub enum Geometry {
    Mesh(Mesh),
    Hair(Hair),
    Grass(GrassParams),
    /// A terrain or screen quad the port draws plainly.
    Plain(Mesh),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Add,
}

pub struct Shape {
    pub name: String,
    pub geometry: Geometry,
    /// Transforms, and whether they bend the mesh from its rest (or just carry it).
    pub bones: Vec<(usize, bool)>,
    pub program: String,
    /// Texture nodes by the fragment program's names for them.
    pub textures: HashMap<String, String>,
    pub constants: HashMap<String, [f32; 4]>,
    pub cull: bool,
    pub blend: Blend,
    /// Lower sets are drawn first; within one, far to near or near to far.
    pub set: i32,
    pub far_first: bool,
    pub center: Vec3,
    /// Names of the weights of the morph targets.
    pub weights: Vec<String>,
}

#[derive(Clone)]
pub enum Texture {
    Plane {
        file: String,
        signed: bool,
    },
    /// Six faces in one picture.
    Cross {
        file: String,
    },
    /// A render target the port draws into (or reads back).
    Target {
        width: u32,
        height: u32,
    },
    /// The cube that only normalizes; the port computes that instead.
    Normalize,
}

pub struct Camera {
    pub name: String,
    pub fov: f32,
    pub position: Vec3,
    pub orientation: Vec4,
    pub transform: String,
}

pub struct Light {
    pub name: String,
    pub color: Vec3,
    pub direction: Vec3,
}

pub struct Pass {
    pub name: String,
    pub colors: Vec<String>,
    pub shapes: Vec<String>,
    pub camera: String,
    pub clears_color: bool,
}

pub struct Model {
    transforms: Vec<Transform>,
    rest: Vec<Mat4>,
    pub shapes: Vec<Shape>,
    pub groups: Vec<(String, Vec<usize>)>,
    pub textures: HashMap<String, Texture>,
    /// The background's faces.
    pub sky: Vec<String>,
    pub animations: Vec<Animation>,
    pub cameras: Vec<Camera>,
    pub lights: Vec<Light>,
    pub passes: Vec<Pass>,
}

fn params<'a>(n: &'a Node, field: &'a str, kind: &'a str) -> impl Iterator<Item = &'a Node> {
    n.nodes(field).iter().filter(move |p| p.kind == kind)
}

/// Triangles of a list (3) or of strips (5) with restarts.
fn triangles(kind: u32, indices: &[u32], out: &mut Vec<u32>) -> Result<(), Error> {
    match kind {
        3 => out.extend(indices.iter().filter(|i| **i != 0xffff)),
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
        // Quads, restarted like strips.
        8 => {
            for quad in indices.split(|i| *i == 0xffff) {
                for q in quad.chunks_exact(4) {
                    out.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
                }
            }
        }
        other => return Err(format!("unknown primitive {other}").into()),
    }
    Ok(())
}

fn stream(n: &Node) -> Option<(String, Vec<f32>, usize)> {
    let name = n.text("channelName").to_owned();
    let width = match n.kind {
        "FloatArray" => 1,
        "Pnt2fArray" | "Vec2fArray" => 2,
        "Pnt3fArray" | "Vec3fArray" | "Color3fArray" => 3,
        "Pnt4fArray" | "Color4fArray" => 4,
        "Color4ucArray" => 4,
        _ => return None,
    };
    let data = match n.fields.get("value") {
        Some(crate::tree::Value::Floats(v)) => v.clone(),
        Some(crate::tree::Value::Bytes(v)) => v.iter().map(|b| *b as f32).collect(),
        _ => return None,
    };
    Some((name, data, width))
}

fn mesh(n: &Node) -> Result<Mesh, Error> {
    let arrays: Vec<(String, Vec<f32>, usize)> =
        n.nodes("data").iter().filter_map(stream).collect();
    let one = |names: &[&str]| -> Option<(Vec<f32>, usize)> {
        names.iter().find_map(|s| {
            arrays
                .iter()
                .find(|(name, _, _)| name.strip_prefix("a2v.").unwrap_or(name) == *s)
                .filter(|(_, d, _)| !d.is_empty())
                .map(|(_, d, w)| (d.clone(), *w))
        })
    };
    let (coords, 3) = one(&["objCoord"]).ok_or("mesh without positions")? else {
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
            for (i, x) in c.iter().enumerate().take(4) {
                v[i] = *x;
            }
            v
        };
        Ok(data.chunks_exact(width).map(vector).collect())
    };
    let normals = vectors(&["objNormal"], Vec4::ZERO)?;
    let tangents = vectors(&["objTangent"], Vec4::X + Vec4::W)?;
    let colors = vectors(&["color"], Vec4::ONE)?;
    let uvs = vectors(&["colorTex", "c_texCoord", "texture"], Vec4::ZERO)?;
    let weights = vectors(&["boneWeights0", "boneWeight0"], Vec4::X)?;
    let bones = vectors(&["boneIndices0", "boneIndex0"], Vec4::ZERO)?;
    // A -1 bone binds nothing; its weight must be nothing too.
    for (w, b) in weights.iter().zip(&bones) {
        for (w, b) in w.to_array().into_iter().zip(b.to_array()) {
            if b < 0.0 && w != 0.0 {
                return Err(format!("{}: a missing bone carries weight", n.name).into());
            }
        }
    }
    let mut moving = Vec::with_capacity(count);
    let mut fixed = Vec::with_capacity(count);
    for i in 0..count {
        let occlusion = colors[i].w;
        moving.push(Moving {
            position: [coords[i * 3], coords[i * 3 + 1], coords[i * 3 + 2]],
            normal: [normals[i].x, normals[i].y, normals[i].z, occlusion],
        });
        fixed.push(Fixed {
            tangent: tangents[i].into(),
            uv: [uvs[i].x, uvs[i].y],
            weights: weights[i].into(),
            bones: [
                bones[i].x.max(0.0),
                bones[i].y.max(0.0),
                bones[i].z.max(0.0),
                bones[i].w.max(0.0),
            ],
        });
    }
    if normals.iter().all(|n| n.truncate() == Vec3::ZERO) {
        // Subdivided cages ship without normals; smooth them from the faces [I].
        let mut indices = Vec::new();
        for idx in n.nodes("indices") {
            triangles(idx.int("primType"), idx.ints("indices"), &mut indices)?;
        }
        let mut acc = vec![Vec3::ZERO; count];
        for t in indices.chunks_exact(3) {
            let (a, b, c) = (
                Vec3::from(moving[t[0] as usize].position),
                Vec3::from(moving[t[1] as usize].position),
                Vec3::from(moving[t[2] as usize].position),
            );
            let n = (b - a).cross(c - a);
            for i in t {
                acc[*i as usize] += n;
            }
        }
        for (v, n) in moving.iter_mut().zip(acc) {
            let w = v.normal[3];
            let n = n.normalize_or_zero();
            v.normal = [n.x, n.y, n.z, w];
        }
    }
    let mut morphs = Vec::new();
    for k in 0.. {
        let coord = format!("objCoordDiff{k}");
        let Some((data, 3)) = one(&[coord.as_str()]) else {
            break;
        };
        let normal = format!("objNormalDiff{k}");
        let diffs = vectors(&[normal.as_str()], Vec4::ZERO)?;
        let changes = data
            .chunks_exact(3)
            .zip(diffs)
            .enumerate()
            .filter(|(_, (c, n))| c.iter().any(|x| *x != 0.0) || n.truncate() != Vec3::ZERO)
            .map(|(i, (c, n))| (i as u32, Vec3::from_slice(c), n.truncate()));
        morphs.push(Morph {
            changes: changes.collect(),
        });
    }
    let mut indices = Vec::new();
    for idx in n.nodes("indices") {
        triangles(idx.int("primType"), idx.ints("indices"), &mut indices)?;
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
            groups: Vec::new(),
            textures: HashMap::new(),
            sky: Vec::new(),
            animations: Vec::new(),
            cameras: Vec::new(),
            lights: Vec::new(),
            passes: Vec::new(),
        };
        for n in root.nodes("transforms") {
            model.walk(n, None);
        }
        // A fallback for shapes that hang off nothing [I].
        model.transforms.push(Transform {
            name: "__identity__".to_owned(),
            parent: None,
            joint: false,
            rest: Fields {
                scale: Vec3::ONE,
                rotation: Vec3::ZERO,
                translation: Vec3::ZERO,
            },
            orientation: Vec3::ZERO,
            joint_orientation: Vec3::ZERO,
            matrix: None,
        });
        let fields: Vec<_> = model.transforms.iter().map(|t| t.rest).collect();
        model.rest = model.compose(&fields);
        for n in root.nodes("textures") {
            let file = n.text("filename").to_owned();
            let texture = match n.kind {
                "2DTexture" => {
                    // Format 24 is plain color; the bump maps use the signed formats.
                    let signed = n.int("format") != 24 && !file.is_empty();
                    Texture::Plane { file, signed }
                }
                "CubicCrossTexture" | "CubicTexture" => Texture::Cross { file },
                "ProcTexture" | "ProcCubicTexture" => {
                    let w = n.vec::<2>("dimensions").unwrap_or([256.0, 128.0]);
                    Texture::Target {
                        width: w[0] as u32,
                        height: w[1] as u32,
                    }
                }
                _ => Texture::Normalize,
            };
            model.textures.insert(n.name.clone(), texture);
        }
        for n in root.nodes("cameras") {
            model.cameras.push(Camera {
                name: n.name.clone(),
                fov: n.float("fieldOfView"),
                position: n.vec::<3>("position").map_or(Vec3::ZERO, Vec3::from),
                orientation: n.vec::<4>("orientation").map_or(Vec4::W, Vec4::from),
                transform: n.text("transform").to_owned(),
            });
        }
        for n in root.nodes("lights") {
            model.lights.push(Light {
                name: n.name.clone(),
                color: n.vec::<3>("color").map_or(Vec3::ONE, Vec3::from) * n.float("intensity"),
                direction: n.vec::<3>("direction").map_or(Vec3::NEG_Z, Vec3::from),
            });
        }
        for n in root.nodes("renderPasses") {
            model.passes.push(Pass {
                name: n.name.clone(),
                colors: n.texts("colorTargets").to_vec(),
                shapes: n.texts("shapes").to_vec(),
                camera: n.text("camera").to_owned(),
                clears_color: n.bool("clearsColor", false),
            });
        }
        for n in root.nodes("backgrounds") {
            for p in params(
                n.node("renderSettings").ok_or("background")?,
                "fragmentShaderParams",
                "TextureParam",
            ) {
                model.sky.push(p.text("texture").to_owned());
            }
        }
        for s in root.nodes("shapes") {
            if s.kind == "ShapeGroup" {
                let at = model.shapes.len();
                for c in s.nodes("children") {
                    let shape = model
                        .shape(c, root, files)
                        .map_err(|e| format!("{}: {e}", c.name))?;
                    model.shapes.push(shape);
                }
                let last = model.groups.len();
                model.groups.push((s.name.clone(), Vec::new()));
                model.groups[last].1.extend(at..model.shapes.len());
            } else {
                let shape = model
                    .shape(s, root, files)
                    .map_err(|e| format!("{}: {e}", s.name))?;
                let at = model.shapes.len();
                model.shapes.push(shape);
                model.groups.push((s.name.clone(), vec![at]));
            }
        }
        for a in root.nodes("animations") {
            model.animations.push(Animation::read(a)?);
        }
        Ok(model)
    }

    pub fn group(&self, name: &str) -> Option<&(String, Vec<usize>)> {
        self.groups.iter().find(|g| g.0 == name)
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

    pub fn camera(&self, name: &str) -> Option<&Camera> {
        self.cameras.iter().find(|c| c.name == name)
    }

    /// Shapes of a group, or all of them.
    pub fn shapes_in(&self, group: Option<&str>) -> Vec<usize> {
        match group.and_then(|g| self.group(g)) {
            Some((_, shapes)) => shapes.clone(),
            None => (0..self.shapes.len()).collect(),
        }
    }

    fn shape(&self, s: &Node, root: &Node, files: &Files) -> Result<Shape, Error> {
        let settings = s.nodes("renderSettings");
        let [setting] = settings else {
            return Err("expected one pass".into());
        };
        let [geometry] = s.texts("geometries") else {
            return Err("expected one geometry".into());
        };
        let g = root
            .nodes("geometries")
            .iter()
            .find(|g| g.name == *geometry);
        let g = g.ok_or("missing geometry")?;
        let geometry = match g.kind {
            "HairPatch" => {
                let text = files.text(g.text("hairFile"))?;
                Geometry::Hair(Hair::new(
                    &text,
                    g.float("density"),
                    g.int("numSegments") as usize,
                )?)
            }
            "Grass" => Geometry::Grass(GrassParams {
                blade_size: g.float("bladeSize"),
                density: g.float("density"),
                segments: g.int("segmentsPerBlade").clamp(1, 8) as usize,
                area: None,
                life_map: g.text("grassLifeMapTex").to_owned(),
                terrain: g.text("grassTerrainGeom").to_owned(),
                texture: g.text("grassTexture").to_owned(),
            }),
            _ => Geometry::Mesh(mesh(g)?),
        };
        // `model` (7) carries a mesh, `model[n]` (23) bends it; `model[n]` (26)
        // bends it into the camera's picture. The port uploads both products.
        let mut bones: Vec<(usize, usize, bool)> = Vec::new();
        for p in params(setting, "vertexShaderParams", "TransformParam") {
            let bend = match p.int("map") {
                7 => false,
                9 | 23 | 26 => true,
                _ => continue,
            };
            let name = p.text("paramName");
            let place = name
                .split(['[', ']'])
                .nth(1)
                .map_or(Ok(0), str::parse)
                .map_err(|_| "bad bone place")?;
            // Missing transforms (the hair's) hang off the head [I].
            let transform = self
                .find(p.text("transform"))
                .or_else(|| self.find("BL_Head_J"))
                .or_else(|| self.find("__identity__"))
                .ok_or("missing transform")?;
            bones.push((place, transform, bend));
        }
        bones.sort_unstable();
        // modelViewProj duplicates model at the same place; keep the first.
        let mut dense: Vec<(usize, usize, bool)> = Vec::new();
        for b in bones {
            if dense.iter().all(|d| d.0 != b.0) {
                dense.push(b);
            }
        }
        let mut bones = dense;
        bones.sort_unstable();
        if !bones.is_empty() && bones.iter().enumerate().any(|(i, b)| b.0 != i) {
            return Err("bones are not numbered from zero".into());
        }
        // Subdivided cages bend on the bones of their process parameters.
        if bones.is_empty() {
            for (at, p) in params(g, "processParams", "TransformParam").enumerate() {
                let transform = self
                    .find(p.text("transform"))
                    .or_else(|| self.find("__identity__"))
                    .ok_or("missing transform")?;
                bones.push((at, transform, true));
            }
        }
        if bones.is_empty() {
            // Screen-space quads hang off nothing [I].
            let identity = self.find("__identity__").ok_or("no identity")?;
            bones.push((0, identity, false));
        }
        if let Geometry::Mesh(m) = &geometry {
            let limit = bones.len() as f32;
            if m.fixed
                .iter()
                .any(|f| f.bones.iter().any(|b| !(0.0..limit).contains(b)))
            {
                return Err("bad bone index".into());
            }
        }
        let weights: Vec<(usize, String)> = params(setting, "vertexShaderParams", "ConstantParam")
            .filter_map(|p| {
                let place = p
                    .text("paramName")
                    .strip_prefix("morphWeight")?
                    .parse()
                    .ok()?;
                Some((place, p.name.clone()))
            })
            .collect();
        if let Geometry::Mesh(m) = &geometry
            && m.morphs.len() < weights.len()
        {
            return Err("more weights than morph targets".into());
        }
        let program = setting.text("fragmentShader");
        let mut textures = HashMap::new();
        for p in params(setting, "fragmentShaderParams", "TextureParam") {
            textures.insert(p.text("paramName").to_owned(), p.text("texture").to_owned());
        }
        let mut constants = HashMap::new();
        for p in params(setting, "fragmentShaderParams", "ConstantParam") {
            constants.insert(
                p.text("paramName").to_owned(),
                p.vec::<4>("value").unwrap_or([0.0; 4]),
            );
        }
        Ok(Shape {
            name: s.name.clone(),
            geometry,
            bones: bones.into_iter().map(|(_, t, bend)| (t, bend)).collect(),
            program: program.to_owned(),
            textures,
            constants,
            cull: s.text("cull") != "never" && s.int("cull") == 0,
            blend: match setting.text("blendParams") {
                "_BlendAlpha" | "AlphaBlendProxy" => Blend::Alpha,
                "_BlendAdd" => Blend::Add,
                _ => Blend::None,
            },
            set: setting.int("sortSet") as i32,
            far_first: setting.text("sortType") == "back_to_front" || setting.int("sortType") == 1,
            center: s.vec::<3>("sphereCenter").map_or(Vec3::ZERO, Vec3::from),
            weights: weights.into_iter().map(|w| w.1).collect(),
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

    /// Animations from another file of the same scene.
    pub fn add_animation(&mut self, animation: Animation) {
        self.animations.push(animation);
    }

    /// Moves another model's shapes into this one, matching bones by name.
    pub fn absorb(&mut self, other: Model) -> Result<(), Error> {
        for shape in other.shapes {
            let mut bones = Vec::new();
            for &(t, bend) in &shape.bones {
                let name = &other.transforms[t].name;
                // The hair's transform is created at runtime under the head [I].
                let name = if name == "__identity__" && self.find("BL_Head_J").is_some() {
                    "BL_Head_J"
                } else {
                    name
                };
                let at = self
                    .find(name)
                    .ok_or_else(|| format!("no transform {name}"))?;
                bones.push((at, bend));
            }
            let mut shape = shape;
            shape.bones = bones;
            let at = self.shapes.len();
            self.shapes.push(shape);
            self.groups.push((format!("absorbed{at}"), vec![at]));
        }
        for (k, v) in other.textures {
            self.textures.entry(k).or_insert(v);
        }
        Ok(())
    }

    /// Groups added by absorbing other models.
    pub fn absorbed(&self) -> Vec<usize> {
        self.groups
            .iter()
            .filter(|g| g.0.starts_with("absorbed"))
            .flat_map(|g| g.1.clone())
            .collect()
    }
}

impl Shape {
    pub fn is_hair(&self) -> bool {
        matches!(self.geometry, Geometry::Hair(_))
    }

    /// Morph targets in use: the first few whose weights count.
    pub fn animated(
        &self,
        values: &HashMap<String, f32>,
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
    fn euler_turns_x_first() {
        let v = euler(Vec3::new(
            std::f32::consts::FRAC_PI_2,
            0.0,
            std::f32::consts::FRAC_PI_2,
        ));
        assert!((v.transform_point3(Vec3::Y) - Vec3::Z).length() < 1e-6);
    }
}
