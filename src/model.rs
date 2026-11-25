//! Resolve the scene tree into rigid meshes, materials, and animated transforms.
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Mat4, Vec3, Vec4};

use crate::anim::Channel;
use crate::tree::Node;
use crate::{
    Error,
    data::Files,
    demo::{DEMO, Demo},
};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub uv2: [f32; 2],
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lit,
    Textured,
    Bump,
    Reflect,
    Metal,
    Water,
    Cutout,
    Lightmap,
    Fur,
}

pub struct Shape {
    pub name: String,
    pub geometry: String,
    pub node: usize,
    pub kind: Kind,
    pub textures: [Option<String>; 2],
    pub cube: Option<String>,
    pub diffuse: Vec4,
    pub ambient: Vec4,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub skin: Option<Skin>,
    pub fur: Vec<String>,
    pub fur_depth: f32,
}

pub struct Skin {
    bones: Vec<usize>,
    inverse_bind: Vec<Mat4>,
    weights: Vec<[(usize, f32); 4]>,
}

impl Skin {
    pub fn deform(&self, vertices: &mut [Vertex], pose: &[Mat4]) {
        let matrices: Vec<_> = self
            .bones
            .iter()
            .zip(&self.inverse_bind)
            .map(|(&bone, &rest)| pose[bone] * rest)
            .collect();
        for (vertex, weights) in vertices.iter_mut().zip(&self.weights) {
            let source = Vec3::from(vertex.pos);
            let normal = Vec3::from(vertex.normal);
            let (p, n) =
                weights
                    .iter()
                    .fold((Vec3::ZERO, Vec3::ZERO), |(p, n), &(index, weight)| {
                        let m = matrices[index];
                        (
                            p + weight * m.transform_point3(source),
                            n + weight * m.transform_vector3(normal),
                        )
                    });
            vertex.pos = p.to_array();
            vertex.normal = n.normalize_or_zero().to_array();
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Fields {
    scale: Vec3,
    rotation: Vec3,
    translation: Vec3,
}

#[derive(Debug, Clone)]
pub struct Transform {
    pub name: String,
    parent: Option<usize>,
    joint: bool,
    matrix: Option<Mat4>,
    rest: Fields,
    /// Orientation of the rotation's axes, and for joints of the joint.
    orientation: Vec3,
    joint_orientation: Vec3,
    scale_pivot: Vec3,
    scale_shift: Vec3,
    pivot: Vec3,
    pivot_shift: Vec3,
}

#[derive(Default)]
pub struct Model {
    pub nodes: Vec<Transform>,
    pub shapes: Vec<Shape>,
    pub environment: Option<String>,
    pub animations: Vec<Animation>,
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
            matrix: n.vec::<16>("matrix").map(|m| Mat4::from_cols_array(&m)),
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

    /// As Maya composes them; the last factor acts first.
    fn matrix(&self, f: Fields) -> Mat4 {
        let t = Mat4::from_translation;
        if let Some(matrix) = self.matrix {
            return matrix;
        }
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

/// Triangles of a list (3) or strip (5).
fn triangles(kind: u32, indices: &[u32], out: &mut Vec<u32>) -> Result<(), Error> {
    match kind {
        3 => out.extend(indices.chunks_exact(3).flatten()),
        5 => {
            for (i, w) in indices.windows(3).enumerate() {
                let [a, b, c] = match i % 2 {
                    0 => [w[0], w[1], w[2]],
                    _ => [w[1], w[0], w[2]],
                };
                if a != b && b != c && a != c {
                    out.extend([a, b, c]);
                }
            }
        }
        n => return Err(format!("unsupported primitive {n}").into()),
    }
    Ok(())
}

fn named<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Node> {
    nodes
        .iter()
        .find(|n| n.name.trim().eq_ignore_ascii_case(name.trim()))
}

fn array(node: &Node) -> &[f32] {
    node.node("data").map_or(&[], |n| n.floats("value"))
}

struct Geometry {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

fn indexed(g: &Node) -> Result<Geometry, Error> {
    let stream = |field| g.nodes(field).first().map_or(&[][..], array);
    let pos = stream("coords");
    let normal = stream("normals");
    let mut uv = g
        .nodes("texCoords")
        .iter()
        .filter(|n| n.node("data").is_some_and(|d| d.kind == "Vec2fArray"))
        .map(array);
    let uv0 = uv.next().unwrap_or(&[]);
    let uv1 = uv.next().unwrap_or(uv0);
    let count = pos.len() / 3;
    if count == 0
        || normal.len() != pos.len()
        || [uv0, uv1]
            .iter()
            .any(|v| !v.is_empty() && v.len() != count * 2)
    {
        return Err("inconsistent vertex streams".into());
    }
    let mut indices = Vec::new();
    for i in g.nodes("indices") {
        triangles(i.int("primType"), i.ints("indices"), &mut indices)?;
    }
    if indices.is_empty() || indices.iter().any(|&i| i as usize >= count) {
        return Err("invalid vertex indices".into());
    }
    let uv_at = |v: &[f32], i| v.get(i..i + 2).map_or([0.0; 2], |a| [a[0], a[1]]);
    let vertices = (0..count)
        .map(|i| Vertex {
            pos: [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]],
            normal: [normal[i * 3], normal[i * 3 + 1], normal[i * 3 + 2]],
            uv: uv_at(uv0, i * 2),
            uv2: uv_at(uv1, i * 2),
        })
        .collect();
    Ok(Geometry { vertices, indices })
}

fn heightfield(g: &Node, files: &Files) -> Result<Geometry, Error> {
    let source = files.resolve(&crate::data::asset_name(g.text("heightImage")))?;
    let ((width, height), pixels) = crate::scene::rgba(files, &source)?;
    let mut dimensions = Vec3::from(
        g.vec("dimensions")
            .ok_or("height field requires dimensions")?,
    );
    let water = DEMO == Demo::Tidepool && g.name.starts_with("grid_");
    if water {
        dimensions.x = 88.0;
        dimensions.z = 88.0;
    }
    let mut repeat = g
        .floats("textureRepeat")
        .chunks_exact(if DEMO == Demo::Tidepool { 3 } else { 2 });
    let repeats = [repeat.next(), repeat.next()];
    let uv = |i: usize, x: f32, y: f32| match repeats[i] {
        Some(v) => [x * v[0], y * v[1]],
        None => [x, y],
    };
    let raw = g.floats("tesselation");
    let number = |i: usize| raw.get(i).map_or(64, |x| x.to_bits().clamp(2, 256)) as usize;
    let (cols, rows) = (number(0), number(1));
    let mut vertices = Vec::with_capacity(cols * rows);
    let height_at = |x: f32, y: f32| {
        if water {
            return 0.0;
        }
        let i = (x.clamp(0.0, 1.0) * (width - 1) as f32).round() as usize;
        let j = (y.clamp(0.0, 1.0) * (height - 1) as f32).round() as usize;
        f32::from(pixels[(j * width as usize + i) * 4]) * dimensions.y / 255.0
    };
    let dx = 1.0 / (cols - 1) as f32;
    let dy = 1.0 / (rows - 1) as f32;
    for row in 0..rows {
        for col in 0..cols {
            let (x, y) = (col as f32 * dx, row as f32 * dy);
            let tangent = Vec3::new(
                2.0 * dx * dimensions.x,
                height_at(x + dx, y) - height_at(x - dx, y),
                0.0,
            );
            let bitangent = Vec3::new(
                0.0,
                height_at(x, y + dy) - height_at(x, y - dy),
                2.0 * dy * dimensions.z,
            );
            vertices.push(Vertex {
                pos: [
                    (x - 0.5) * dimensions.x,
                    height_at(x, y),
                    (y - 0.5) * dimensions.z,
                ],
                normal: bitangent.cross(tangent).normalize_or_zero().to_array(),
                uv: uv(0, x, y),
                uv2: uv(1, x, y),
            });
        }
    }
    let mut indices = Vec::with_capacity((cols - 1) * (rows - 1) * 6);
    for row in 0..rows - 1 {
        for col in 0..cols - 1 {
            let a = (row * cols + col) as u32;
            let c = a + cols as u32;
            indices.extend([a, c, a + 1, a + 1, c, c + 1]);
        }
    }
    Ok(Geometry { vertices, indices })
}

fn texture(files: &Files, node: &Node) -> Result<String, Error> {
    files.resolve(&crate::data::asset_name(node.text("filename")))
}

struct Resources<'a> {
    root: &'a Node,
    files: &'a Files,
    rest: &'a [Mat4],
}

impl Model {
    pub fn new(root: &Node, files: &Files) -> Result<Self, Error> {
        let mut model = Self::default();
        model
            .nodes
            .push(Transform::read(&Node::empty("Transform"), None));
        for node in root.nodes("transforms") {
            model.walk_transforms(node, None);
        }
        for background in root.nodes("backgrounds") {
            if let Some(settings) = background.node("renderSettings") {
                model.environment = settings
                    .texts("textures")
                    .iter()
                    .filter_map(|name| named(root.nodes("textures"), name))
                    .find(|t| t.kind == "CubicTexture")
                    .map(|t| texture(files, t))
                    .transpose()?;
            }
        }
        let rest = model.pose(None, 0.0);
        let resources = Resources {
            root,
            files,
            rest: &rest,
        };
        for group in root.nodes("renderGroups") {
            model.walk_shapes(&resources, group, 0)?;
        }
        model.read_animations(root)?;
        Ok(model)
    }

    pub fn read_animations(&mut self, root: &Node) -> Result<(), Error> {
        self.animations = root
            .nodes("animations")
            .iter()
            .map(|node| Animation::new(self, crate::anim::read(node)?))
            .collect::<Result<_, _>>()?;
        Ok(())
    }

    fn walk_transforms(&mut self, node: &Node, parent: Option<usize>) {
        let index = self.nodes.len();
        self.nodes.push(Transform::read(node, parent));
        for child in node.nodes("children") {
            self.walk_transforms(child, Some(index));
        }
    }

    fn walk_shapes(
        &mut self,
        resources: &Resources<'_>,
        node: &Node,
        inherited: usize,
    ) -> Result<(), Error> {
        let transform = self
            .find(node.text("transform"))
            .filter(|&i| i > 0)
            .unwrap_or(inherited);
        match node.kind {
            "Shape" | "FurShape" => {
                let passes = node.nodes("renderSettings");
                let settings = if node.name.starts_with("pGroundShape") && DEMO == Demo::Bugs {
                    passes.get(1).or(passes.first())
                } else {
                    passes.first().or_else(|| node.node("renderSettings"))
                };
                if let Some(settings) = settings {
                    for name in node.texts("geometries") {
                        if let Some(shape) = self
                            .shape(resources, node, settings, name, transform)
                            .map_err(|e| format!("{}: {e}", node.name))?
                        {
                            self.shapes.push(shape);
                        }
                    }
                }
            }
            "LodShape" => {
                if let Some(first) = node.nodes("children").first() {
                    self.walk_shapes(resources, first, inherited)?;
                }
            }
            _ => {
                for child in node.nodes("children").iter().chain(node.nodes("shapes")) {
                    self.walk_shapes(resources, child, transform)?;
                }
            }
        }
        Ok(())
    }

    fn shape(
        &self,
        resources: &Resources<'_>,
        node: &Node,
        settings: &Node,
        name: &str,
        transform: usize,
    ) -> Result<Option<Shape>, Error> {
        let Resources { root, files, rest } = *resources;
        let Some(g) = named(root.nodes("geometries"), name) else {
            return Ok(None);
        };
        let shader = settings.text("pixelShader").to_ascii_lowercase();
        if g.kind == "Curve" || shader.contains("depthreplace") || node.name.contains("caustics") {
            return Ok(None);
        }
        if DEMO == Demo::Tidepool
            && (node.name.contains("_<1>") && node.name.starts_with("Water")
                || node.name.starts_with("Water_plane_poly")
                || node.name == "Rocky_plane_polyShape_SHAPE_0"
                || node.name.contains("Cave_<1>")
                || shader == "decal_alphamap")
        {
            return Ok(None);
        }
        let pass_transform = self
            .find(settings.text("modelTransform"))
            .filter(|&i| i > 0);
        let index = pass_transform.unwrap_or(transform);
        let material = named(root.nodes("materials"), settings.text("material"));
        let textures = root.nodes("textures");
        let maps: Vec<_> = settings
            .texts("textures")
            .iter()
            .filter_map(|name| named(textures, name))
            .filter(|n| !n.text("filename").is_empty())
            .collect();
        let cube = maps
            .iter()
            .find(|n| n.kind == "CubicTexture")
            .map(|n| texture(files, n))
            .transpose()?;
        let mut color = maps.iter().copied().filter(|n| {
            n.kind == "2DTexture"
                && !n.text("filename").to_ascii_lowercase().ends_with(".ifl")
                && !n.name.to_ascii_lowercase().contains("phong")
        });
        let water = DEMO == Demo::Tidepool && node.name.starts_with("Water");
        let bump = shader.contains("colormapbumpmap")
            || shader == "bugshader"
            || shader == "groundshader2";
        let kind = match (color.clone().count(), cube.is_some()) {
            _ if water => Kind::Water,
            (n, _) if bump && n >= 2 => Kind::Bump,
            (0, true) => Kind::Metal,
            (_, true) => Kind::Reflect,
            (0, false) => Kind::Lit,
            (_, _) if shader.contains("lightmap") => Kind::Lightmap,
            (_, _)
                if shader.contains("alpha") || node.name.to_ascii_lowercase().contains("leaf") =>
            {
                Kind::Cutout
            }
            _ => Kind::Textured,
        };
        let [mut a, mut b] = [color.next(), color.next()];
        if a.is_some_and(|n| n.name.to_ascii_lowercase().contains("bump")) && kind == Kind::Bump {
            std::mem::swap(&mut a, &mut b);
        }
        let images = [
            a.map(|t| texture(files, t)).transpose()?,
            b.map(|t| texture(files, t)).transpose()?,
        ];
        let cube = if water && cube.is_none() {
            named(textures, "reflectionTexture")
                .map(|n| texture(files, n))
                .transpose()?
        } else {
            cube
        };
        let Geometry { vertices, indices } = if g.kind == "HeightField" {
            heightfield(g, files)?
        } else {
            indexed(g)?
        };
        let skin = self.skin(settings, g, rest, vertices.len())?;
        let fur = node
            .texts("furTextures")
            .iter()
            .filter_map(|n| named(textures, n))
            .map(|t| texture(files, t))
            .collect::<Result<Vec<_>, _>>()?;
        let value = |name, default| {
            material
                .and_then(|m| m.vec::<4>(name))
                .map_or(default, Vec4::from)
        };
        Ok(Some(Shape {
            name: node.name.clone(),
            geometry: g.name.clone(),
            node: if skin.is_some() { 0 } else { index },
            kind,
            textures: images,
            cube,
            diffuse: value("diffuseColor", Vec4::ONE),
            ambient: value("ambientColor", Vec4::ZERO),
            vertices,
            indices,
            skin,
            fur,
            fur_depth: node.float("furDepth"),
        }))
    }

    fn skin(
        &self,
        settings: &Node,
        g: &Node,
        rest: &[Mat4],
        count: usize,
    ) -> Result<Option<Skin>, Error> {
        let names = settings.texts("spareTransforms");
        let data = g.nodes("data");
        let Some(stream) = data.first() else {
            return Ok(None);
        };
        let values = array(stream);
        if names.len() < 2 || values.len() < count || !values.len().is_multiple_of(count) {
            return Ok(None);
        }
        let width = values.len() / count;
        if width > 4 || values.iter().any(|&x| !(0.0..=1.01).contains(&x)) {
            return Ok(None);
        }
        let bones = names
            .iter()
            .map(|n| self.find(n).ok_or_else(|| format!("missing bone {n}")))
            .collect::<Result<Vec<_>, _>>()?;
        let lookup = data.get(1).map_or(&[][..], array);
        let indexed = lookup.len() == values.len();
        let weights = values
            .chunks_exact(width)
            .enumerate()
            .map(|(i, row)| {
                let mut out = [(0, 0.0); 4];
                let total = row.iter().sum::<f32>();
                if total < 1e-6 {
                    out[0].1 = 1.0;
                    return out;
                }
                for (k, &w) in row.iter().enumerate() {
                    let index = if indexed {
                        ((lookup[i * width + k] / 4.0).round() as isize).max(0) as usize
                    } else {
                        k
                    };
                    out[k] = (index.min(bones.len() - 1), w / total);
                }
                out
            })
            .collect();
        let inverse_bind = bones.iter().map(|&i| rest[i].inverse()).collect();
        Ok(Some(Skin {
            bones,
            inverse_bind,
            weights,
        }))
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    /// World transforms of all nodes at a time.
    pub fn pose(&self, anim: Option<&Animation>, ms: f64) -> Vec<Mat4> {
        let mut fields: Vec<Fields> = self.nodes.iter().map(|n| n.rest).collect();
        for (node, channel) in anim.iter().flat_map(|a| &a.0) {
            let f = &mut fields[*node];
            let v = match channel.field.as_str() {
                "rotation" => &mut f.rotation,
                "scale" => &mut f.scale,
                _ => &mut f.translation,
            };
            for (i, c) in channel.curves.iter().enumerate() {
                if let Some(x) = c.as_ref().and_then(|c| c.at(ms)) {
                    v[i] = x;
                }
            }
        }
        let mut out: Vec<Mat4> = Vec::with_capacity(fields.len());
        // Parents come before their children.
        for (n, f) in self.nodes.iter().zip(fields) {
            let m = n.matrix(f);
            out.push(n.parent.map_or(m, |p| out[p] * m));
        }
        out
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        let transforms = self.pose(None, 0.0);
        let points = self.shapes.iter().flat_map(|s| {
            s.vertices
                .iter()
                .map(|v| transforms[s.node].transform_point3(v.pos.into()))
        });
        points.fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(p), max.max(p)),
        )
    }
}

/// Channels and the nodes they move.
pub struct Animation(Vec<(usize, Channel)>);

impl Animation {
    pub fn new(model: &Model, channels: Vec<Channel>) -> Result<Self, Error> {
        Ok(Self(
            channels
                .into_iter()
                .filter_map(|c| match c.field.as_str() {
                    "rotation" | "scale" | "translation" => {
                        model.find(&c.node).map(|node| (node, c))
                    }
                    _ => None,
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_keep_their_winding() {
        let mut out = Vec::new();
        triangles(5, &[7, 7, 0, 1, 2, 3], &mut out).unwrap();
        assert_eq!(out, [0, 7, 1, 0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn euler_turns_x_first() {
        let v = euler(Vec3::new(1.0, 2.0, 3.0)).transform_vector3(Vec3::Y);
        let w = glam::Mat3::from_rotation_z(3.0)
            * glam::Mat3::from_rotation_y(2.0)
            * glam::Mat3::from_rotation_x(1.0);
        assert!((v - w * Vec3::Y).length() < 1e-5);
    }
}
