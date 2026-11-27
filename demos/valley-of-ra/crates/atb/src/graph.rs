//! Walking an ATB scene graph to the shapes it draws, as `atsDraw` does:
//! each node's draw (vtable slot 0x34) is in `docs/ENGINE.md`.

use glam::{Mat4, Vec3};

use crate::bof::{Bof, Object, Seq};

/// One part of a shape: a material over a geometry, under a transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub shape: u32,
    pub material: u32,
    pub geometry: u32,
    pub matrix: Mat4,
}

/// What the walk needs to choose children.
pub struct View<'a> {
    /// Seconds since the scene started, for `Seq` nodes.
    pub time: f64,
    /// Eye position in world space, for `LOD` nodes.
    pub eye: Vec3,
    /// A frame for a `Seq` or `Anim` by id, overriding its own.
    pub frame: &'a dyn Fn(u32) -> Option<usize>,
    /// A transform for a node by id: a `Frame`'s own matrix is replaced by
    /// it (as demos overwrite it), any other node is put under it.
    pub pose: &'a dyn Fn(u32) -> Option<Mat4>,
}

impl Default for View<'_> {
    fn default() -> Self {
        Self {
            time: 0.0,
            eye: Vec3::ZERO,
            frame: &|_| None,
            pose: &|_| None,
        }
    }
}

/// A `Frame` matrix as glam's: ATB stores rows for row vectors, which is the
/// same sixteen numbers as columns for column vectors.
pub fn frame_matrix(m: &[f32; 16]) -> Mat4 {
    Mat4::from_cols_array(m)
}

/// The child a `Seq` shows at `time` (`0x416840`): `fields` are bounce,
/// counted (not timed), seconds per child, start offset and repeat count.
pub fn seq_child(seq: &Seq, time: f64) -> Option<usize> {
    let n = seq.group.children.len();
    let [bounce, counted, rate, offset, repeats] = seq.fields;
    if n == 0 || counted != 0 {
        return (n > 0).then_some(0);
    }
    let (rate, offset) = (f32::from_bits(rate) as f64, f32::from_bits(offset) as f64);
    let t = time - offset;
    if rate <= 0.0 || t < 0.0 {
        return Some(0);
    }
    if repeats > 0 && t >= f64::from(repeats) * rate * n as f64 {
        return None;
    }
    let i = (t / rate) as usize;
    if bounce == 0 {
        return Some(i % n);
    }
    let period = (2 * n).saturating_sub(2).max(1);
    let i = i % period;
    Some(if i >= n { period - i } else { i })
}

/// Collects every shape part under `root`.
pub fn walk(bof: &Bof, root: u32, view: &View, out: &mut Vec<Item>) {
    visit(bof, root, Mat4::IDENTITY, view, out, 0);
}

fn visit(bof: &Bof, id: u32, m: Mat4, view: &View, out: &mut Vec<Item>, depth: usize) {
    let Some(o) = bof.get(id) else { return };
    if depth > 64 {
        return;
    }
    let pose = (view.pose)(id);
    let m = match pose {
        Some(p) if !matches!(o, Object::Frame(_)) => m * p,
        _ => m,
    };
    let kids = |out: &mut Vec<Item>, m: Mat4, list: &[u32]| {
        for &c in list {
            visit(bof, c, m, view, out, depth + 1);
        }
    };
    match o {
        Object::Group(g) => kids(out, m, &g.children),
        Object::Anim(a) => {
            // `0x413720`: each child under its matrix for the current frame,
            // kept in the object `fields.2` names.
            let (frame, _, data) = a.fields;
            let frame = (view.frame)(id).unwrap_or(frame as usize);
            match bof.get(data) {
                Some(Object::Raw(raw)) => {
                    let n = a.group.children.len();
                    for (k, &c) in a.group.children.iter().enumerate() {
                        let at = (frame * n + k) * 64;
                        let Some(b) = raw.get(at..at + 64) else {
                            continue;
                        };
                        let mut f = [0.0; 16];
                        for (i, v) in f.iter_mut().enumerate() {
                            *v = f32::from_be_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
                        }
                        visit(bof, c, m * frame_matrix(&f), view, out, depth + 1);
                    }
                }
                _ => kids(out, m, &a.group.children),
            }
        }
        Object::Frame(f) => {
            let own = pose.unwrap_or_else(|| frame_matrix(&f.matrix));
            kids(out, m * own, &f.group.children)
        }
        Object::Seq(s) => {
            let n = s.group.children.len().max(1);
            let pick = (view.frame)(id)
                .map(|i| i % n)
                .or_else(|| seq_child(s, view.time));
            if let Some(&c) = pick.and_then(|i| s.group.children.get(i)) {
                visit(bof, c, m, view, out, depth + 1);
            }
        }
        Object::Lod(l) => {
            // Mode 4: the child with the largest range still nearer than the
            // eye's distance to the centre.
            let n = l.group.children.len().min(l.ranges.len());
            let d = m.transform_point3(Vec3::from(l.center)).distance(view.eye);
            let mut pick: Option<usize> = None;
            for i in 0..n {
                if l.ranges[i] < d && pick.is_none_or(|p| l.ranges[p] < l.ranges[i]) {
                    pick = Some(i);
                }
            }
            if let Some(i) = pick {
                visit(bof, l.group.children[i], m, view, out, depth + 1);
            }
        }
        Object::Shape(s) => {
            for &(material, geometry) in &s.parts {
                out.push(Item {
                    shape: id,
                    material,
                    geometry,
                    matrix: m,
                });
            }
        }
        Object::Bboard(b) => {
            for &(material, geometry) in &b.shape.parts {
                out.push(Item {
                    shape: id,
                    material,
                    geometry,
                    matrix: m,
                });
            }
        }
        _ => {}
    }
}

/// Every node under `root` with a name, for finding parts the demo animates.
pub fn named(bof: &Bof, root: u32) -> Vec<(String, u32)> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(o) = bof.get(id) else { continue };
        if let Some(name) = o.node().and_then(|n| n.name.clone()) {
            found.push((name, id));
        }
        if let Some(g) = o.group() {
            stack.extend(g.children.iter().rev());
        }
    }
    found
}
