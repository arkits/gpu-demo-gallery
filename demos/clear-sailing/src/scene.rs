//! Transform hierarchy, mesh loading, and shared math.

use glam::{EulerRot, Mat4, Vec2, Vec3};

use crate::Error;
use crate::nma::Node as TNode;
use crate::nmb::Node;

/// Around x, then y, then z (the viewer's `FUN_0043b480`, as in Dawn).
fn euler(v: Vec3) -> Mat4 {
    Mat4::from_euler(EulerRot::ZYX, v.z, v.y, v.x)
}

fn vec3(n: &Node, field: &str) -> Vec3 {
    let v = n.floats(field);
    if v.len() == 3 {
        Vec3::new(v[0], v[1], v[2])
    } else {
        Vec3::ZERO
    }
}

fn tvec3(n: &TNode, field: &str) -> Vec3 {
    let v = n.nums(field);
    if v.len() >= 3 {
        Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)
    } else {
        Vec3::ZERO
    }
}

/// The viewer's composition for a Maya transform node.
pub fn local(n: &Node) -> Mat4 {
    let t = Mat4::from_translation;
    let scale = n.vec::<3>("scale").map(Vec3::from).unwrap_or(Vec3::ONE);
    let turned = euler(vec3(n, "rotation")) * euler(vec3(n, "rotationOrientation"));
    t(vec3(n, "translation") + vec3(n, "rotationPivotTranslate") + vec3(n, "rotationPivot"))
        * turned
        * t(-vec3(n, "rotationPivot") + vec3(n, "scalePivotTranslate") + vec3(n, "scalePivot"))
        * Mat4::from_scale(scale)
        * t(-vec3(n, "scalePivot"))
}

/// Same, for the text scenes.
pub fn tlocal(n: &TNode) -> Mat4 {
    let one = Vec3::ONE;
    let scale = {
        let v = n.nums("scale");
        if v.len() >= 3 {
            Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)
        } else {
            one
        }
    };
    let turned = euler(tvec3(n, "rotation")) * euler(tvec3(n, "rotationOrientation"));
    Mat4::from_translation(
        tvec3(n, "translation") + tvec3(n, "rotationPivotTranslate") + tvec3(n, "rotationPivot"),
    ) * turned
        * Mat4::from_translation(
            -tvec3(n, "rotationPivot") + tvec3(n, "scalePivotTranslate") + tvec3(n, "scalePivot"),
        )
        * Mat4::from_scale(scale)
        * Mat4::from_translation(-tvec3(n, "scalePivot"))
}

/// World matrices of every transform under `roots`, by name.
pub fn worlds(n: &Node, out: &mut std::collections::BTreeMap<String, Mat4>) -> Result<(), Error> {
    for t in n.nodes("transforms") {
        walk(t, &Mat4::IDENTITY, out)?;
    }
    Ok(())
}

fn walk(
    n: &Node,
    parent: &Mat4,
    out: &mut std::collections::BTreeMap<String, Mat4>,
) -> Result<(), Error> {
    if n.kind != "MayaTransform" && n.kind != "MayaJoint" && n.kind != "Group" {
        return Err(format!("bad transform {}", n.kind).into());
    }
    let order = n.int("rotationOrder");
    if order != 0 && n.kind == "MayaTransform" {
        return Err("non-xyz rotation order".into());
    }
    let world = if n.kind == "Group" {
        *parent
    } else {
        *parent * local(n)
    };
    if !n.name.is_empty() {
        out.insert(n.name.clone(), world);
    }
    for c in n.nodes("children") {
        walk(c, &world, out)?;
    }
    Ok(())
}

/// Same, for the text scenes (`MatrixTransform` holds a row matrix as well).
pub fn tworlds(n: &TNode, out: &mut std::collections::BTreeMap<String, Mat4>) -> Result<(), Error> {
    for t in n.nodes("transforms") {
        twalk(t, &Mat4::IDENTITY, out)?;
    }
    Ok(())
}

fn twalk(
    n: &TNode,
    parent: &Mat4,
    out: &mut std::collections::BTreeMap<String, Mat4>,
) -> Result<(), Error> {
    let world = match n.kind.as_str() {
        "MayaTransform" => *parent * tlocal(n),
        "MatrixTransform" => {
            let v = n.nums("matrix");
            if v.len() < 16 {
                return Err("short matrix".into());
            }
            let m = Mat4::from_cols_array(
                &v[..16]
                    .iter()
                    .map(|x| *x as f32)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            );
            *parent * m
        }
        k => return Err(format!("bad transform {k}").into()),
    };
    if !n.name.is_empty() {
        out.insert(n.name.clone(), world);
    }
    for c in n.nodes("children") {
        twalk(c, &world, out)?;
    }
    Ok(())
}

/// Triangles of a list (3) or of strips (5) with restarts (as in Dawn).
pub fn triangles(kind: u32, indices: &[u32], out: &mut Vec<u32>) -> Result<(), Error> {
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

/// A mesh of the ship, in its own space.
#[derive(Default)]
pub struct MeshData {
    pub pos: Vec<Vec3>,
    pub nor: Vec<Vec3>,
    pub tan: Vec<Vec3>,
    pub bin: Vec<Vec3>,
    pub uv: Vec<Vec2>,
    pub idx: Vec<u32>,
}

fn channel(n: &Node, want: &str) -> Vec<f32> {
    n.nodes("vertices")
        .iter()
        .find(|v| v.text("paramName") == want)
        .map(|v| v.floats("value").to_vec())
        .unwrap_or_default()
}

/// Reads an `IndexedFaceSet` of the binary scenes.
pub fn mesh(n: &Node) -> Result<MeshData, Error> {
    if n.kind != "IndexedFaceSet" {
        return Err(format!("bad mesh {}", n.kind).into());
    }
    let flat = |want: &str, dim: usize| -> Result<Vec<f32>, Error> {
        let v = channel(n, want);
        if !v.len().is_multiple_of(dim) {
            return Err(format!("ragged {want}").into());
        }
        Ok(v)
    };
    let pos = flat("a2v.objCoord", 3)?;
    let nor = flat("a2v.objNormal", 3)?;
    let tan = flat("a2v.objTangent", 3)?;
    let bin = flat("a2v.objBinormal", 3)?;
    let uv = flat("a2v.TexCoord", 2)?;
    let verts = pos.len() / 3;
    for (name, v) in [("normal", &nor), ("tangent", &tan), ("binormal", &bin)] {
        if !v.is_empty() && v.len() / 3 != verts {
            return Err(format!("ragged {name}").into());
        }
    }
    if !uv.is_empty() && uv.len() / 2 != verts {
        return Err("ragged uv".into());
    }
    let mut idx = Vec::new();
    for part in n.nodes("indices") {
        triangles(part.int("primType"), part.ints("indices"), &mut idx)?;
    }
    if idx.iter().any(|i| (*i as usize) >= verts) {
        return Err("index out of range".into());
    }
    let at = |v: &[f32], i: usize| Vec3::new(v[3 * i], v[3 * i + 1], v[3 * i + 2]);
    let zero = Vec3::ZERO;
    Ok(MeshData {
        pos: (0..verts).map(|i| at(&pos, i)).collect(),
        nor: (0..verts)
            .map(|i| if nor.is_empty() { zero } else { at(&nor, i) })
            .collect(),
        tan: (0..verts)
            .map(|i| if tan.is_empty() { zero } else { at(&tan, i) })
            .collect(),
        bin: (0..verts)
            .map(|i| if bin.is_empty() { zero } else { at(&bin, i) })
            .collect(),
        uv: (0..verts)
            .map(|i| {
                if uv.is_empty() {
                    Vec2::ZERO
                } else {
                    Vec2::new(uv[2 * i], uv[2 * i + 1])
                }
            })
            .collect(),
        idx,
    })
}

/// Reads a text `Mesh` (splinters, ropes): positions, and line or triangle indices.
pub fn tmesh(n: &TNode) -> Result<MeshData, Error> {
    if n.kind != "Mesh" {
        return Err(format!("bad mesh {}", n.kind).into());
    }
    let mut pos = Vec::new();
    let mut uvs = Vec::new();
    for a in n.nodes("data") {
        let v = a.nums("value");
        match a.text("channelName") {
            "a2v.objCoord" => {
                if v.len() % 3 != 0 {
                    return Err("ragged positions".into());
                }
                pos = v
                    .chunks_exact(3)
                    .map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32))
                    .collect();
            }
            "a2v.texcoord" | "a2v.TexCoord" => {
                if v.len() % 2 != 0 {
                    return Err("ragged uvs".into());
                }
                uvs = v
                    .chunks_exact(2)
                    .map(|p| Vec2::new(p[0] as f32, p[1] as f32))
                    .collect();
            }
            c => return Err(format!("bad channel {c}").into()),
        }
    }
    if pos.is_empty() {
        return Err("mesh without positions".into());
    }
    let mut idx = Vec::new();
    let mut lines = Vec::new();
    for part in n.nodes("indices") {
        let kind = match part.ident("primType") {
            "tri_list" => 3,
            "line_strip" => 1,
            p => return Err(format!("unknown primitive {p}").into()),
        };
        let raw: Vec<u32> = part.nums("indices").iter().map(|x| *x as u32).collect();
        if kind == 3 {
            if !raw.len().is_multiple_of(3) {
                return Err("ragged triangles".into());
            }
            triangles(3, &raw, &mut idx)?;
        } else {
            for w in raw.windows(2) {
                lines.push(w[0]);
                lines.push(w[1]);
            }
        }
    }
    if idx
        .iter()
        .chain(lines.iter())
        .any(|i| (*i as usize) >= pos.len())
    {
        return Err("index out of range".into());
    }
    let n = pos.len();
    Ok(MeshData {
        nor: vec![Vec3::Z; n],
        tan: vec![Vec3::X; n],
        bin: vec![Vec3::Y; n],
        uv: if uvs.is_empty() {
            vec![Vec2::ZERO; n]
        } else {
            if uvs.len() != n {
                return Err("ragged uvs".into());
            }
            uvs
        },
        pos,
        idx: if idx.is_empty() { lines } else { idx },
    })
}
