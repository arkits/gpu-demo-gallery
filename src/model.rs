//! The creature, fish and beam: a tree of transforms with meshes.
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Mat3, Mat4, Vec3, Vec4};

use crate::Error;
use crate::anim::Channel;
use crate::tree::Node;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// How a shape is shaded (`setupHack` and the combiners that go with it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `mat`: lighting alone.
    Lit,
    /// `mat_c`: lighting and a texture; caustics are added.
    Textured,
    /// `bc_d`: a normal map in the shape's space and a texture.
    Bump,
}

pub struct Shape {
    pub name: String,
    /// Index into `Model::nodes`.
    pub node: usize,
    pub kind: Kind,
    pub textures: Vec<String>,
    pub diffuse: Vec4,
    pub ambient: Vec4,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
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
}

fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
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

    /// As Maya composes them; the last factor acts first.
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

fn shape(n: &Node, node: usize) -> Result<Shape, Error> {
    let [g] = n.nodes("geometry") else {
        return Err("expected one mesh per shape".into());
    };
    let (pos, normal) = (g.floats("coords"), g.floats("normals"));
    let mut uv: &[f32] = &[];
    for a in g.nodes("textureApplications") {
        if a.int("textureFunc") != 1 {
            return Err("unsupported texture function".into());
        }
        uv = a.floats("textureCoords");
    }
    let count = pos.len() / 3;
    if normal.len() != pos.len() || (!uv.is_empty() && uv.len() != count * 2) {
        return Err("vertex arrays differ in length".into());
    }
    let mut indices = Vec::new();
    for i in g.nodes("vertexIndices") {
        triangles(i.int("type"), i.ints("indices"), &mut indices)?;
    }
    if indices.iter().any(|i| *i as usize >= count) {
        return Err("vertex index out of range".into());
    }
    let pass = n.node("renderSettings").ok_or("shape without settings")?;
    let context = pass.node("combinerContext").ok_or("pass without colors")?;
    let textures: Vec<String> = context
        .nodes("textures")
        .iter()
        .map(|t| file_name(t.text("filename")))
        .collect();
    let kind = match (pass.int("setupHack"), textures.len()) {
        (1, 0) => Kind::Lit,
        (2, 1) => Kind::Textured,
        (9, 2) => Kind::Bump,
        other => return Err(format!("unsupported setup {other:?}").into()),
    };
    let material = context.node("material");
    let color = |f, or: f32| -> Vec4 {
        let v = material.and_then(|m| m.vec::<4>(f));
        v.map_or(Vec4::splat(or), Vec4::from)
    };
    let vertex = |i: usize| Vertex {
        pos: [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]],
        normal: [normal[i * 3], normal[i * 3 + 1], normal[i * 3 + 2]],
        uv: match uv.is_empty() {
            true => [0.0; 2],
            false => [uv[i * 2], uv[i * 2 + 1]],
        },
    };
    Ok(Shape {
        name: n.name.clone(),
        node,
        kind,
        textures,
        diffuse: color("diffuseColor", 1.0),
        // The viewer sets this for everything that gets caustics (0x404060).
        ambient: match kind {
            Kind::Textured => Vec4::splat(0.14),
            _ => color("ambientColor", 0.0),
        },
        vertices: (0..count).map(vertex).collect(),
        indices,
    })
}

impl Model {
    pub fn new(root: &Node) -> Result<Self, Error> {
        let mut m = Self::default();
        m.walk(root, None)?;
        Ok(m)
    }

    fn walk(&mut self, n: &Node, parent: Option<usize>) -> Result<(), Error> {
        match n.kind {
            "Shape" => {
                let node = parent.ok_or("shape without transform")?;
                let s = shape(n, node).map_err(|e| format!("{}: {e}", n.name))?;
                self.shapes.push(s);
                Ok(())
            }
            "Group" => n
                .nodes("children")
                .iter()
                .try_for_each(|c| self.walk(c, parent)),
            "MayaTransform" | "MayaJoint" => {
                self.nodes.push(Transform::read(n, parent));
                let me = self.nodes.len() - 1;
                n.nodes("children")
                    .iter()
                    .try_for_each(|c| self.walk(c, Some(me)))
            }
            other => Err(format!("unsupported node {other}").into()),
        }
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    /// World transforms of all nodes at a time.
    pub fn pose(&self, anim: &Animation, ms: f64) -> Vec<Mat4> {
        let mut fields: Vec<Fields> = self.nodes.iter().map(|n| n.rest).collect();
        for (node, channel) in &anim.0 {
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

    /// The radius of the viewer's bounding sphere (0x430551 in the older viewer): of
    /// groups, the middle of the children's boxes and the farthest child.
    pub fn radius(&self) -> f32 {
        let place = self.pose(&Animation(Vec::new()), 0.0);
        let spheres: Vec<(Vec3, f32)> = self
            .shapes
            .iter()
            .filter_map(|s| {
                let at = |v: &Vertex| place[s.node].transform_point3(v.pos.into());
                let points: Vec<Vec3> = s.vertices.iter().map(at).collect();
                let low = points.iter().copied().reduce(Vec3::min)?;
                let high = points.iter().copied().reduce(Vec3::max)?;
                let middle = (low + high) / 2.0;
                let far = points.iter().map(|p| p.distance(middle));
                Some((middle, far.fold(0.0, f32::max)))
            })
            .collect();
        let low = spheres
            .iter()
            .map(|(c, r)| c - Vec3::splat(*r))
            .reduce(Vec3::min);
        let high = spheres
            .iter()
            .map(|(c, r)| c + Vec3::splat(*r))
            .reduce(Vec3::max);
        let middle = (low.unwrap_or_default() + high.unwrap_or_default()) / 2.0;
        let far = spheres.iter().map(|(c, r)| c.distance(middle) + r);
        far.fold(0.0, f32::max)
    }
}

/// Channels and the nodes they move.
pub struct Animation(Vec<(usize, Channel)>);

impl Animation {
    pub fn new(model: &Model, channels: Vec<Channel>) -> Result<Self, Error> {
        let bind = |c: Channel| -> Result<_, Error> {
            if !["rotation", "scale", "translation"].contains(&c.field.as_str()) {
                return Err(format!("unsupported channel {}", c.field).into());
            }
            let node = model
                .find(&c.node)
                .ok_or_else(|| format!("no node {}", c.node))?;
            Ok((node, c))
        };
        Ok(Self(
            channels.into_iter().map(bind).collect::<Result<_, _>>()?,
        ))
    }
}

/// Direction in a node's own space.
pub fn to_local(place: &Mat4, v: Vec3) -> Vec3 {
    (Mat3::from_mat4(*place).inverse() * v).normalize_or_zero()
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
        let w =
            Mat3::from_rotation_z(3.0) * Mat3::from_rotation_y(2.0) * Mat3::from_rotation_x(1.0);
        assert!((v - w * Vec3::Y).length() < 1e-5);
    }
}
