//! A logo: its meshes, and the shading styles that replace one another.
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3, Vec4};

use crate::Error;
use crate::combiner::{Combiner, Coords, Program};
use crate::tree::Node;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    /// File name without its directory, as in the data folder.
    pub file: String,
    pub cube: bool,
    pub mips: bool,
}

#[derive(Debug, Clone)]
pub struct Style {
    pub program: Program,
    pub textures: Vec<Texture>,
    pub color0: Vec4,
    pub color1: Vec4,
    /// Diffuse color of the material.
    pub diffuse: Vec4,
    /// `color0` is the direction to the light in the model's space (`bumpDiffuse`).
    pub light_in_color0: bool,
}

pub struct Shape {
    pub name: String,
    pub transform: Mat4,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    coords: Vec<Coords>,
}

pub struct Model {
    pub shapes: Vec<Shape>,
    /// For each style, one entry per shape.
    pub styles: Vec<Vec<Style>>,
}

fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or("").to_owned()
}

fn style(shape: &Node, coords: &[Coords]) -> Result<Style, Error> {
    let pass = shape
        .node("renderSettings")
        .ok_or("shape without settings")?;
    let context = pass.node("combinerContext").ok_or("pass without colors")?;
    for other in [
        "cullParams",
        "fogParams",
        "stencilParams",
        "alphaTestParams",
        "depthTestParams",
        "miscParams",
        "blendParams",
    ] {
        if pass.node(other).is_some() {
            return Err(format!("unsupported {other}").into());
        }
    }
    let textures: Vec<Texture> = context
        .nodes("textures")
        .iter()
        .map(|t| Texture {
            file: file_name(t.text("filename")),
            cube: t.kind == "CubicTexture",
            mips: t.int("mipFilter") != 0,
        })
        .collect();
    let combiners = pass.nodes("combiners").iter().map(Combiner::read);
    let hack = pass.int("setupHack");
    if hack > 1 {
        return Err(format!("unsupported setup {hack}").into());
    }
    if coords.len() < textures.len() {
        return Err("texture without coordinates".into());
    }
    Ok(Style {
        program: Program {
            combiners: combiners.collect::<Result<_, _>>()?,
            textures: coords
                .iter()
                .copied()
                .zip(textures.iter().map(|t| t.cube))
                .collect(),
        },
        textures,
        color0: context.vec("color0").unwrap_or([1.0; 4]).into(),
        color1: context.vec("color1").unwrap_or([1.0; 4]).into(),
        diffuse: context
            .node("material")
            .and_then(|m| m.vec("diffuseColor"))
            .unwrap_or([1.0; 4])
            .into(),
        light_in_color0: hack == 1,
    })
}

fn transform(n: &Node) -> Result<Mat4, Error> {
    let v = |f| n.vec::<3>(f).map_or(Vec3::ZERO, Vec3::from);
    let scale = n.vec::<3>("scale").map_or(Vec3::ONE, Vec3::from);
    let turn = n.vec("rotation").map_or(Quat::IDENTITY, Quat::from_array);
    let still = ["scalePivot", "rotationPivot"]
        .iter()
        .all(|f| v(f) == Vec3::ZERO);
    let straight = n
        .vec("rotationOrientation")
        .is_none_or(|q| Quat::from_array(q) == Quat::IDENTITY);
    if !still || !straight || turn != Quat::IDENTITY {
        // None of the three models has any.
        return Err("unsupported transform".into());
    }
    Ok(Mat4::from_scale_rotation_translation(
        scale,
        turn,
        v("translation"),
    ))
}

/// Calls `f` for each shape below `n`, with its transform.
fn shapes<'a>(
    n: &'a Node,
    at: Mat4,
    f: &mut dyn FnMut(&'a Node, Mat4) -> Result<(), Error>,
) -> Result<(), Error> {
    match n.kind {
        "Shape" => f(n, at),
        "Transform" | "Group" => {
            let at = if n.kind == "Transform" {
                at * transform(n)?
            } else {
                at
            };
            n.nodes("children")
                .iter()
                .try_for_each(|c| shapes(c, at, f))
        }
        other => Err(format!("unsupported node {other}").into()),
    }
}

/// Triangles of a list (3) or strip (5).
fn triangles(kind: u32, indices: &[u32], out: &mut Vec<u32>) -> Result<(), Error> {
    match kind {
        3 => out.extend(indices.chunks_exact(3).flatten()),
        5 => {
            for (i, w) in indices.windows(3).enumerate() {
                let [a, b, c] = if i % 2 == 0 {
                    [w[0], w[1], w[2]]
                } else {
                    [w[1], w[0], w[2]]
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

fn shape(n: &Node, at: Mat4) -> Result<Shape, Error> {
    let [g] = n.nodes("geometry") else {
        return Err("expected one mesh per shape".into());
    };
    let (pos, normal) = (g.floats("coords"), g.floats("normals"));
    let mut coords = Vec::new();
    let mut uv: &[f32] = &[];
    for a in g.nodes("textureApplications") {
        coords.push(match a.int("textureFunc") {
            1 => {
                uv = a.floats("textureCoords");
                Coords::Static
            }
            16 => Coords::EyeReflect,
            f => return Err(format!("unsupported texture function {f}").into()),
        });
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
    let vertex = |i: usize| Vertex {
        pos: [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]],
        normal: [normal[i * 3], normal[i * 3 + 1], normal[i * 3 + 2]],
        uv: if uv.is_empty() {
            [0.0; 2]
        } else {
            [uv[i * 2], uv[i * 2 + 1]]
        },
    };
    Ok(Shape {
        name: n.name.clone(),
        transform: at,
        vertices: (0..count).map(vertex).collect(),
        indices,
        coords,
    })
}

impl Model {
    /// `model` holds the geometry; `styles` are trees of the same shapes with only
    /// their settings.
    pub fn new(model: &Node, styles: &[Node]) -> Result<Self, Error> {
        let mut list = Vec::new();
        shapes(model, Mat4::IDENTITY, &mut |n, at| {
            list.push(shape(n, at).map_err(|e| format!("{}: {e}", n.name))?);
            Ok(())
        })?;
        let mut out = Vec::new();
        for tree in styles {
            let mut found = HashMap::new();
            shapes(tree, Mat4::IDENTITY, &mut |n, _| {
                found.insert(n.name.as_str(), n);
                Ok(())
            })?;
            let of = |s: &Shape| -> Result<Style, Error> {
                let n = found
                    .get(s.name.as_str())
                    .ok_or_else(|| format!("no style for {}", s.name))?;
                style(n, &s.coords).map_err(|e| format!("{}: {e}", s.name).into())
            };
            out.push(list.iter().map(of).collect::<Result<_, _>>()?);
        }
        Ok(Self {
            shapes: list,
            styles: out,
        })
    }

    /// Each edge of each triangle once.
    pub fn edges(shape: &Shape) -> Vec<[u32; 2]> {
        let mut edges: Vec<[u32; 2]> = shape
            .indices
            .chunks_exact(3)
            .flat_map(|t| [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]])
            .map(|[a, b]| [a.min(b), a.max(b)])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        edges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_keep_their_winding() {
        let mut out = Vec::new();
        triangles(5, &[7, 7, 0, 1, 2, 3], &mut out).unwrap();
        // (7 7 0) is empty, (7 0 1) is odd and so reversed, then (0 1 2), (1 2 3).
        assert_eq!(out, [0, 7, 1, 0, 1, 2, 2, 1, 3]);
    }
}
