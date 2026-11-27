//! Resolve the scene tree into rigid meshes, materials, and animated transforms.
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};

use crate::Error;
use crate::anim::Channel;
use crate::tree::Node;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub uv2: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lit,
    Textured,
    Bump,
    Reflect,
    Metal,
    Refract,
    Glass,
    Table,
}

pub struct Shape {
    pub node: usize,
    pub kind: Kind,
    pub textures: [Option<String>; 2],
    pub cube: Option<String>,
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
    quaternion: Option<Quat>,
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
            quaternion: (n.kind == "Transform").then(|| {
                let q = |f| {
                    n.vec::<4>(f).map_or(Quat::IDENTITY, |a| {
                        let q = Quat::from_array(a);
                        if q.length_squared() > 0.0 {
                            q.normalize()
                        } else {
                            Quat::IDENTITY
                        }
                    })
                };
                q("rotation") * q("rotationOrientation")
            }),
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
        let turned = self.quaternion.map_or_else(
            || euler(f.rotation) * euler(self.orientation),
            Mat4::from_quat,
        );
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

fn named<'a>(nodes: &'a [Node], name: &str) -> Result<&'a Node, Error> {
    nodes
        .iter()
        .find(|n| n.name == name)
        .ok_or_else(|| format!("unresolved resource {name:?}").into())
}

fn shape(n: &Node, node: usize, root: &Node) -> Result<Shape, Error> {
    let (g, pass, material, textures) = if root.kind == "Scene" {
        let [reference] = n.texts("geometryRefs") else {
            return Err("expected one indexed face set".into());
        };
        let g = named(root.nodes("geometry"), reference)?;
        let pass = n
            .nodes("renderPasses")
            .first()
            .ok_or("shape without pass")?;
        let material = root
            .nodes("materials")
            .iter()
            .find(|m| m.name == pass.text("material"));
        let textures = pass
            .texts("textures")
            .iter()
            .map(|name| named(root.nodes("textures"), name))
            .collect::<Result<Vec<_>, _>>()?;
        (g, pass, material, textures)
    } else {
        let [g] = n.nodes("geometry") else {
            return Err("expected one mesh".into());
        };
        let pass = n.node("renderSettings").ok_or("shape without settings")?;
        let context = pass.node("combinerContext").ok_or("pass without colors")?;
        (
            g,
            pass,
            context.node("material"),
            context.nodes("textures").iter().collect(),
        )
    };
    let mut color_maps = textures
        .iter()
        .filter(|t| t.kind == "2DTexture")
        .map(|n| crate::data::asset_name(n.text("filename")));
    let textures_2d = [color_maps.next(), color_maps.next()];
    let cube = textures
        .iter()
        .find(|t| t.kind == "CubicTexture")
        .map(|t| crate::data::asset_name(t.text("filename")));
    let kind = match n.name.as_str() {
        "ball_inner_mesh_SHAPE" => Kind::Refract,
        "ball_outer_mesh_SHAPE" => Kind::Glass,
        "table_SHAPE" => Kind::Table,
        _ if root.kind != "Scene" && pass.int("setupHack") == 1 => Kind::Bump,
        _ if cube.is_some() && textures_2d[0].is_some() => Kind::Reflect,
        _ if cube.is_some() => Kind::Metal,
        _ if textures_2d[0].is_some() => Kind::Textured,
        _ => Kind::Lit,
    };
    let color = |f, or| material.and_then(|m| m.vec::<4>(f)).map_or(or, Vec4::from);
    let (pos, normal) = (g.floats("coords"), g.floats("normals"));
    let mut uv = g.nodes("textureApplications").iter().filter_map(|a| {
        let coords = a.floats("textureCoords");
        (!coords.is_empty()).then_some(coords)
    });
    let uv0 = uv.next();
    let uv1 = uv.next().or(uv0);
    let count = pos.len() / 3;
    if count == 0
        || normal.len() != pos.len()
        || [uv0, uv1]
            .into_iter()
            .flatten()
            .any(|v| v.len() != count * 2)
    {
        return Err("vertex arrays differ in length".into());
    }
    let mut indices = Vec::new();
    for i in g.nodes("vertexIndices") {
        triangles(i.int("type"), i.ints("indices"), &mut indices)?;
    }
    if indices.is_empty() || indices.iter().any(|i| *i as usize >= count) {
        return Err("invalid vertex indices".into());
    }
    let texcoord = |uv: Option<&[f32]>, i| uv.map_or([0.0; 2], |v| [v[i], v[i + 1]]);
    let vertices = (0..count)
        .map(|i| Vertex {
            pos: [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]],
            normal: [normal[i * 3], normal[i * 3 + 1], normal[i * 3 + 2]],
            uv: texcoord(uv0, i * 2),
            uv2: texcoord(uv1, i * 2),
        })
        .collect();
    Ok(Shape {
        node,
        kind,
        textures: textures_2d,
        cube,
        diffuse: color("diffuseColor", Vec4::ONE),
        ambient: color("ambientColor", Vec4::ZERO),
        vertices,
        indices,
    })
}

impl Model {
    pub fn new(root: &Node) -> Result<Self, Error> {
        let mut model = Self::default();
        model.walk(root, None, root)?;
        model.read_animations(root)?;
        Ok(model)
    }

    pub fn read_animations(&mut self, root: &Node) -> Result<(), Error> {
        self.animations = root
            .nodes("animations")
            .iter()
            .filter(|node| !node.nodes("channels").is_empty())
            .map(|node| Animation::new(self, crate::anim::read(node)?))
            .collect::<Result<_, _>>()?;
        Ok(())
    }

    fn walk(&mut self, n: &Node, parent: Option<usize>, root: &Node) -> Result<(), Error> {
        let parent = match n.kind {
            "Shape" => {
                let index = parent.ok_or("shape without transform")?;
                self.shapes
                    .push(shape(n, index, root).map_err(|e| format!("{}: {e}", n.name))?);
                return Ok(());
            }
            "Environment" => {
                let texture = n.node("texture").ok_or("environment without cube")?;
                self.environment = Some(crate::data::asset_name(texture.text("filename")));
                return Ok(());
            }
            "Group" | "Scene" => parent,
            "Transform" | "MayaTransform" | "MayaJoint" => {
                self.nodes.push(Transform::read(n, parent));
                Some(self.nodes.len() - 1)
            }
            other => return Err(format!("unsupported scene node {other}").into()),
        };
        n.nodes("children")
            .iter()
            .try_for_each(|c| self.walk(c, parent, root))
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
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
