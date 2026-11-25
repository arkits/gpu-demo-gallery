//! A scene: transforms, meshes, morph targets, shapes and what they are drawn with.
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

    /// As Maya composes them; the last factor acts first (`FUN_0043b480`).
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
    pub weights: [f32; 4],
    pub bones: [f32; 4],
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Add,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cloth {
    None,
    /// Drawn closer, by two amounts.
    Under,
    Over,
}

pub struct Shape {
    pub name: String,
    pub geometry: Geometry,
    /// Transforms, and whether they bend the mesh from its rest (or just carry it).
    pub bones: Vec<(usize, bool)>,
    pub program: String,
    /// Texture nodes by the fragment program's names for them.
    pub textures: HashMap<String, String>,
    pub cull: bool,
    pub blend: Blend,
    pub cloth: Cloth,
    /// Lower sets are drawn first; within one, far to near or near to far.
    pub set: u32,
    pub far_first: bool,
    pub center: Vec3,
    /// Names of the weights of the morph targets.
    weights: Vec<String>,
}

pub enum Texture {
    Plane {
        file: String,
        /// Texels are -1 to 1.
        signed: bool,
        repeat: [bool; 2],
    },
    /// Six faces in one picture.
    Cross { file: String },
    /// The cube that only normalizes; the port computes that instead.
    Normalize,
}

pub struct Model {
    transforms: Vec<Transform>,
    rest: Vec<Mat4>,
    pub shapes: Vec<Shape>,
    pub textures: HashMap<String, Texture>,
    /// The background's cube map.
    pub sky: Option<String>,
    pub animations: Vec<Animation>,
}

/// `models/..` paths of scene files are relative to the program's folder.
fn params<'a>(n: &'a Node, field: &str, kind: &'a str) -> impl Iterator<Item = &'a Node> {
    n.nodes(field).iter().filter(move |p| p.kind == kind)
}

/// Triangles of a list (3) or of strips (5) with restarts.
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
        other => return Err(format!("unknown primitive {other}").into()),
    }
    Ok(())
}

fn mesh(n: &Node) -> Result<Mesh, Error> {
    let stream = |wanted: &str| -> Option<(Vec<f32>, usize)> {
        let found = n.nodes("vertices").iter().find(|v| {
            let name = v.text("paramName");
            name.strip_prefix("a2v.").unwrap_or(name) == wanted
        })?;
        let width = match found.kind {
            "FloatArray" => 1,
            "Pnt2fArray" | "Vec2fArray" | "Pnt2hfArray" => 2,
            "Pnt3fArray" | "Pnt3hfArray" => 3,
            _ => 4,
        };
        Some((found.floats("value").to_vec(), width))
    };
    let one = |names: &[&str]| names.iter().find_map(|s| stream(s));
    let (coords, 3) = one(&["coord"]).ok_or("mesh without positions")? else {
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
    let normals = vectors(&["normal"], Vec4::ZERO)?;
    let tangents = vectors(&["tangent"], Vec4::X + Vec4::W)?;
    let uvs = vectors(&["diffCol", "skinColor_frontSpec", "texture"], Vec4::ZERO)?;
    let all = |base: &str| -> Vec<String> {
        ["0_3", "0_2", "0_1", "0"]
            .iter()
            .map(|s| format!("{base}{s}"))
            .collect()
    };
    let named = |base: &str, fill| {
        let names = all(base);
        vectors(&names.iter().map(String::as_str).collect::<Vec<_>>(), fill)
    };
    // Meshes that a transform only carries have neither.
    let weights = named("boneWeight", Vec4::ZERO)?;
    let bones = named("boneIndex", Vec4::ZERO)?;
    let whole = one(&[
        "boneWeight0",
        "boneWeight0_1",
        "boneWeight0_2",
        "boneWeight0_3",
    ])
    .is_none();
    let mut moving = Vec::with_capacity(count);
    let mut fixed = Vec::with_capacity(count);
    for i in 0..count {
        moving.push(Moving {
            position: [coords[i * 3], coords[i * 3 + 1], coords[i * 3 + 2]],
            normal: normals[i].into(),
        });
        fixed.push(Fixed {
            tangent: tangents[i].into(),
            uv: [uvs[i].x, uvs[i].y],
            weights: if whole { Vec4::X } else { weights[i] }.into(),
            // -1 stands for none.
            bones: bones[i].max(Vec4::ZERO).into(),
        });
    }
    let mut morphs = Vec::new();
    while stream(&format!("coordMorph{}", morphs.len())).is_some() {
        let k = morphs.len();
        let coords = vectors(&[&format!("coordMorph{k}")], Vec4::ZERO)?;
        let normals = vectors(&[&format!("normalMorph{k}")], Vec4::ZERO)?;
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
            textures: HashMap::new(),
            sky: None,
            animations: Vec::new(),
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
                    signed: n.bool("remapped", false),
                    // `FUN_00420460`: 0 clamps and 1 repeats.
                    repeat: ["wrapU", "wrapV"].map(|f| n.int(f) == 1),
                },
                "CubicCrossTexture" => Texture::Cross { file },
                _ => Texture::Normalize,
            };
            model.textures.insert(n.name.clone(), texture);
        }
        let textures = |n: &Node, field| -> HashMap<String, String> {
            params(n, field, "TextureParam")
                .map(|p| (p.text("paramName").to_owned(), p.text("texture").to_owned()))
                .collect()
        };
        let shared: HashMap<&str, _> = root
            .nodes("shaders")
            .iter()
            .map(|s| (s.name.as_str(), textures(s, "params")))
            .collect();
        for b in root.nodes("backgrounds") {
            let settings = b
                .node("renderSettings")
                .ok_or("background without settings")?;
            model.sky = textures(settings, "fragmentShaderParams").remove("texture");
        }
        for s in root.nodes("shapes") {
            let shape = model.shape(s, root, &shared, files);
            model
                .shapes
                .push(shape.map_err(|e| format!("{}: {e}", s.name))?);
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

    fn shape(
        &self,
        s: &Node,
        root: &Node,
        shared: &HashMap<&str, HashMap<String, String>>,
        files: &Files,
    ) -> Result<Shape, Error> {
        let [settings] = s.nodes("renderSettings") else {
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
        let geometry = if g.kind == "HairPatch" {
            let text = files.text(g.text("hairFile"))?;
            Geometry::Hair(Hair::new(
                &text,
                g.float("density"),
                g.int("numSegments") as usize,
            )?)
        } else {
            Geometry::Mesh(mesh(g)?)
        };
        // `model` (7) carries a mesh, `model[n]` (23) bends it; products with the camera
        // (9 and 26) are made in the shader here.
        let mut bones: Vec<(usize, usize, bool)> = Vec::new();
        for p in params(settings, "vertexShaderParams", "TransformParam") {
            let bend = match p.int("property") {
                7 => false,
                23 | 26 => true,
                _ => continue,
            };
            let name = p.text("paramName");
            let place = name
                .split_once('[')
                .map_or(Ok(0), |(_, n)| n.trim_end_matches(']').parse())?;
            let transform = self.find(p.text("transform")).ok_or("missing transform")?;
            bones.push((place, transform, bend));
        }
        bones.sort_unstable();
        if bones.is_empty() || bones.iter().enumerate().any(|(i, b)| b.0 != i) {
            return Err("bones are not numbered from zero".into());
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
        let weights: Vec<(usize, String)> = params(settings, "vertexShaderParams", "ConstantParam")
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
            && m.morphs.len() != weights.len()
        {
            return Err("morph targets and their weights differ".into());
        }
        let program = settings.text("fragmentShader");
        let mut textures = shared.get(program).cloned().unwrap_or_default();
        for p in params(settings, "fragmentShaderParams", "TextureParam") {
            textures.insert(p.text("paramName").to_owned(), p.text("texture").to_owned());
        }
        Ok(Shape {
            name: s.name.clone(),
            geometry,
            bones: bones.into_iter().map(|(_, t, bend)| (t, bend)).collect(),
            program: program.trim_end_matches("_f").to_owned(),
            textures,
            cull: settings.text("cullParams") != "_CullNoCull",
            blend: match settings.text("blendParams") {
                "_BlendAlpha" => Blend::Alpha,
                "_BlendAdd" => Blend::Add,
                _ => Blend::None,
            },
            cloth: match settings.text("depthParams") {
                "ClothBias1" => Cloth::Under,
                "ClothBias2" => Cloth::Over,
                _ => Cloth::None,
            },
            set: settings.int("sortSet"),
            far_first: settings.int("sortType") == 1,
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
}

impl Shape {
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
