//! The demo's people and places: transforms, baked vertex frames, subdivision
//! surfaces, shapes and what they are drawn with.
use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};

use crate::Error;
use crate::data::Files;
use crate::tree::Node;

/// A transform with no parent: `MatrixTransform` stores a flagged 4 by 4
/// matrix in row-major order [I], `MaxTransform` a rotation, a scale
/// rotation, a scale, a translation and a sign.
#[derive(Clone)]
pub struct Transform {
    pub name: String,
    pub matrix: Mat4,
}

/// Maya pivots, composed as the viewer does (`FUN_0043b480` in the earlier
/// viewer; the layout here is unchanged [V]).
struct Maya {
    joint: bool,
    scale: Vec3,
    rotation: Vec3,
    translation: Vec3,
    orientation: Vec3,
    joint_orientation: Vec3,
    scale_pivot: Vec3,
    scale_shift: Vec3,
    pivot: Vec3,
    pivot_shift: Vec3,
}

impl Maya {
    fn read(n: &Node) -> Self {
        let v = |f| n.vec::<3>(f).map_or(Vec3::ZERO, Vec3::from);
        Self {
            joint: n.kind == "MayaJoint",
            scale: n.vec::<3>("scale").map_or(Vec3::ONE, Vec3::from),
            rotation: v("rotation"),
            translation: v("translation"),
            orientation: v("rotationOrientation"),
            joint_orientation: v("jointOrientation"),
            scale_pivot: v("scalePivot"),
            scale_shift: v("scalePivotTranslate"),
            pivot: v("rotationPivot"),
            pivot_shift: v("rotationPivotTranslate"),
        }
    }

    fn matrix(&self) -> Mat4 {
        use glam::EulerRot;
        let euler = |v: Vec3| Mat4::from_euler(EulerRot::ZYX, v.z, v.y, v.x);
        let t = Mat4::from_translation;
        let turned = euler(self.rotation) * euler(self.orientation);
        if self.joint {
            return t(self.translation)
                * euler(self.joint_orientation)
                * turned
                * Mat4::from_scale(self.scale);
        }
        t(self.translation + self.pivot_shift + self.pivot)
            * turned
            * t(-self.pivot + self.scale_shift + self.scale_pivot)
            * Mat4::from_scale(self.scale)
            * t(-self.scale_pivot)
    }
}

impl Transform {
    fn read(n: &Node) -> Result<Self, Error> {
        let matrix = match n.kind {
            "MatrixTransform" => {
                let (flags, floats) = n.matrix("matrix").ok_or("matrix without data")?;
                let _ = flags;
                if floats.len() != 16 {
                    return Err("matrix is not 4 by 4".into());
                }
                Mat4::from_cols_array(&[
                    floats[0], floats[4], floats[8], floats[12], floats[1], floats[5], floats[9],
                    floats[13], floats[2], floats[6], floats[10], floats[14], floats[3], floats[7],
                    floats[11], floats[15],
                ])
            }
            "MaxTransform" => {
                let r: [f32; 4] = n.vec("rotation").ok_or("transform without rotation")?;
                let s: [f32; 3] = n.vec("scale").unwrap_or([1.0; 3]);
                let t: [f32; 3] = n.vec("translation").unwrap_or([0.0; 3]);
                // Uniform scales commute with the scale rotation, and every
                // scale here is uniform [V].
                Mat4::from_scale_rotation_translation(
                    Vec3::from(s),
                    Quat::from_xyzw(r[0], r[1], r[2], r[3]),
                    Vec3::from(t),
                )
            }
            "MayaTransform" | "MayaJoint" => Maya::read(n).matrix(),
            _ => return Err(format!("bad transform {}", n.kind).into()),
        };
        if !matrix.is_finite() {
            return Err("transform is not finite".into());
        }
        Ok(Self {
            name: n.name.clone(),
            matrix,
        })
    }
}

/// One rigid pose of a `TransformChannel`.
#[derive(Clone, Copy)]
pub struct Pose {
    pub time: f64,
    pub rotation: Quat,
    pub scale_rotation: Quat,
    pub scale: Vec3,
    pub translation: Vec3,
}

/// The baked positions (and sometimes normals, tangents, occlusion) of one
/// geometry at one moment.
pub struct GeoFrame {
    pub time: f64,
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
    pub occlusion: Option<Vec<f32>>,
}

/// Control mesh of a subdivision surface: positions, texture coordinates and
/// quad corners.
pub struct Control {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub faces: Vec<Vec<u32>>,
    pub frames: Vec<GeoFrame>,
    pub occlusion_rest: Option<Vec<f32>>,
    /// How many Catmull-Clark levels to run, from `StaticSubdivisionLevel`.
    pub levels: u32,
    /// Baked tessellated render mesh, when the file stores one: triangles,
    /// texture coordinates and the control vertex behind each tessellated one
    /// (`boss_body` splits for texture seams [V]).
    pub baked: Option<BakedTopo>,
}

pub struct BakedTopo {
    pub indices: Vec<u32>,
    pub uvs: Vec<[f32; 2]>,
    /// Tessellated vertex to control vertex.
    pub split: Vec<u32>,
}

/// A mesh with baked frames and unchanging topology.
pub struct Baked {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub frames: Vec<GeoFrame>,
}

pub enum Geometry {
    Subdiv(Control),
    Baked(Baked),
    Static {
        positions: Vec<[f32; 3]>,
        normals: Vec<[f32; 3]>,
        tangents: Vec<[f32; 4]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
    },
}

/// A draw call: a geometry with a transform, programs and textures.
pub struct Shape {
    pub name: String,
    pub geometry: String,
    pub transform: String,
    pub vertex: String,
    pub fragment: String,
    pub textures: HashMap<String, String>,
    /// `ConstantParam` values by name (`specularColor`, `bumpScale`, ...).
    pub constants: HashMap<String, [f32; 4]>,
    pub cull: bool,
    pub alpha_kill: bool,
}

pub struct TransformAnim {
    pub target: String,
    pub poses: Vec<Pose>,
}

pub struct Model {
    pub transforms: HashMap<String, Transform>,
    pub geometries: HashMap<String, Geometry>,
    pub shapes: Vec<Shape>,
    pub transform_anims: HashMap<String, TransformAnim>,
    /// `texture` nodes by name.
    pub textures: HashMap<String, String>,
    pub cameras: HashMap<String, Camera>,
    pub key_light_dir: Vec3,
}

#[derive(Clone)]
pub struct Camera {
    pub position: Vec3,
    pub orientation: Quat,
    pub fov_y: f32,
}

fn quat(v: [f32; 4]) -> Quat {
    Quat::from_xyzw(v[0], v[1], v[2], v[3])
}

/// Vertex arrays of an `IndexedFaceSet` or `SubdivisionIFS`: the streams the
/// demo's shaders read (`a2v.coord`, `a2v.normal`, `a2v.tangent`,
/// `a2v.texture`, `a2v.occlusion`).
struct Streams {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 4]>,
    /// Every texture stream, in file order, for matching by size.
    textures: Vec<Vec<[f32; 2]>>,
    occlusion: Vec<f32>,
}

impl Streams {
    fn read(nodes: &[Node]) -> Result<Self, Error> {
        let mut out = Self {
            positions: Vec::new(),
            normals: Vec::new(),
            tangents: Vec::new(),
            textures: Vec::new(),
            occlusion: Vec::new(),
        };
        for n in nodes {
            let name = n.text("paramName");
            let floats = n.floats("value");
            let vecs = |w: usize| -> Vec<Vec<f32>> {
                floats.chunks_exact(w).map(<[f32]>::to_vec).collect()
            };
            match (name.strip_prefix("a2v.").unwrap_or(name), n.kind) {
                ("coord", _) => {
                    out.positions = vecs(3).into_iter().map(|v| [v[0], v[1], v[2]]).collect();
                }
                ("normal", _) => {
                    // Full and half float normals alike arrive as floats.
                    out.normals = vecs(3).into_iter().map(|v| [v[0], v[1], v[2]]).collect();
                }
                ("tangent", _) => {
                    out.tangents = vecs(4)
                        .into_iter()
                        .map(|v| [v[0], v[1], v[2], v[3]])
                        .collect();
                }
                ("texture" | "texcoord", _) => {
                    out.textures
                        .push(vecs(2).into_iter().map(|v| [v[0], v[1]]).collect());
                }
                ("occlusion", _) => out.occlusion = floats.to_vec(),
                _ => {}
            }
        }
        Ok(out)
    }
}

/// Corner groups separated by restarts.
fn groups(indices: &[u32]) -> Vec<Vec<u32>> {
    let mut out = Vec::new();
    let mut group = Vec::new();
    for &i in indices {
        if i == 0xffff {
            if !group.is_empty() {
                out.push(std::mem::take(&mut group));
            }
        } else {
            group.push(i);
        }
    }
    if !group.is_empty() {
        out.push(group);
    }
    out
}

/// Every corner group as triangles; larger polygons fan out [I].
fn triangles_from(groups: &[Vec<u32>], out: &mut Vec<u32>) -> Result<(), Error> {
    for g in groups {
        if g.len() < 3 {
            return Err("bad polygon".into());
        }
        for k in 1..g.len() - 1 {
            out.extend([g[0], g[k], g[k + 1]]);
        }
    }
    Ok(())
}

/// Triangle lists (3) and strips (5) with restarts.
fn triangles(kind: u32, indices: &[u32], out: &mut Vec<u32>) -> Result<(), Error> {
    match kind {
        0 | 3 => out.extend(indices.iter().filter(|&&i| i != 0xffff)),
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

impl Model {
    /// Merges the demo's scene with its animation files: the scene carries
    /// the static structure, each `*_anim` file the frames of its geometries
    /// and transforms.
    pub fn new(files: &Files) -> Result<Self, Error> {
        let mut model = Self {
            transforms: HashMap::new(),
            geometries: HashMap::new(),
            shapes: Vec::new(),
            transform_anims: HashMap::new(),
            textures: HashMap::new(),
            cameras: HashMap::new(),
            key_light_dir: Vec3::NEG_Y,
        };
        for file in crate::demo::MODELS {
            let root =
                crate::tree::read_binary(files.get(file)?).map_err(|e| format!("{file}: {e}"))?;
            model.add(&root)?;
        }
        Ok(model)
    }

    fn add(&mut self, root: &Node) -> Result<(), Error> {
        // Animated below; the scene's own structure wins over the animation
        // files' stale copies [V].
        let mut animated = std::collections::HashSet::new();
        for n in root.nodes("animations") {
            for channel in n.nodes("channels") {
                if channel.kind == "GeometryChannel" {
                    animated.insert(channel.text("targetNode").to_owned());
                }
            }
        }
        for n in root.nodes("transforms") {
            if self.transforms.contains_key(&n.name) {
                continue;
            }
            let t = Transform::read(n).map_err(|e| format!("{}: {e}", n.name))?;
            self.transforms.insert(t.name.clone(), t);
        }
        for n in root.nodes("textures") {
            if self.textures.contains_key(&n.name) {
                continue;
            }
            if n.kind == "2DTexture" || n.kind == "CubicCrossTexture" {
                let file = n.text("filename");
                if !file.is_empty() {
                    self.textures.insert(n.name.clone(), file.to_owned());
                }
            }
        }
        for n in root.nodes("cameras") {
            let orientation: [f32; 4] = n.vec("orientation").unwrap_or([0.0, 0.0, 0.0, 1.0]);
            self.cameras.insert(
                n.name.clone(),
                Camera {
                    position: Vec3::from(n.vec("position").unwrap_or([0.0; 3])),
                    orientation: quat(orientation),
                    fov_y: n.float("fieldOfView").to_radians(),
                },
            );
        }
        for n in root.nodes("lights") {
            if n.kind == "DirectionalLight" {
                let d: [f32; 3] = n.vec("direction").unwrap_or([0.0, -1.0, 0.0]);
                self.key_light_dir = Vec3::from(d).normalize_or_zero();
            }
        }
        for n in root.nodes("geometries") {
            // The scene's bases win, unless this file animates the geometry:
            // base and frames must come from the same file [V].
            if self.geometries.contains_key(&n.name) && !animated.contains(&n.name) {
                continue;
            }
            let geometry = Self::geometry(n)?;
            self.geometries.insert(n.name.clone(), geometry);
        }
        for n in root.nodes("shapes") {
            let shape = Self::shape(n)?;
            // The animation files repeat the creeps with placeholder
            // programs; the scene's own shapes draw them [V].
            if shape.vertex.ends_with("_default") {
                continue;
            }
            if !self.shapes.iter().any(|s| s.name == shape.name) {
                self.shapes.push(shape);
            }
        }
        for n in root.nodes("animations") {
            self.animation(n)?;
        }
        Ok(())
    }

    fn geometry(n: &Node) -> Result<Geometry, Error> {
        match n.kind {
            "IndexedFaceSet" => {
                let streams = Streams::read(n.nodes("vertices"))?;
                let mut indices = Vec::new();
                for index in n.nodes("indices") {
                    triangles(index.int("primType"), index.ints("indices"), &mut indices)?;
                }
                let count = streams.positions.len();
                if indices.iter().any(|i| *i as usize >= count) {
                    return Err(format!("{}: bad vertex index", n.name).into());
                }
                let uvs = streams.textures.first().cloned().unwrap_or_default();
                Ok(Geometry::Baked(Baked {
                    positions: streams.positions,
                    normals: streams.normals,
                    tangents: streams.tangents,
                    uvs,
                    indices,
                    frames: Vec::new(),
                }))
            }
            "SubdivisionIFS" => {
                let streams = Streams::read(n.nodes("vertices"))?;
                let count = streams.positions.len();
                let mut uvs = Vec::new();
                let mut tess_uvs = None;
                for t in streams.textures {
                    if t.len() == count && uvs.is_empty() {
                        uvs = t;
                    } else if tess_uvs.is_none() {
                        tess_uvs = Some(t);
                    }
                }
                if uvs.is_empty() {
                    // Only tessellated uvs: control uvs are unused.
                    uvs = vec![[0.0, 0.0]; count];
                }
                let mut flats: Vec<Vec<u32>> = Vec::new();
                for index in n.nodes("indices") {
                    flats.push(index.ints("indices").to_vec());
                }
                // Control faces reference the control vertices; a baked
                // tessellation references more.
                let mut control: Option<Vec<Vec<u32>>> = None;
                let mut tess: Option<&Vec<u32>> = None;
                for flat in &flats {
                    if flat.iter().any(|i| *i != 0xffff && *i as usize >= count) {
                        if tess.is_none() {
                            tess = Some(flat);
                        }
                    } else if control.is_none() {
                        control = Some(groups(flat));
                    }
                }
                let control: Vec<Vec<u32>> =
                    control.ok_or_else(|| format!("{}: no control mesh", n.name))?;
                let baked = match (tess, tess_uvs) {
                    (Some(tess), Some(tuvs)) => {
                        let tgroups = groups(tess);
                        if tgroups.len() != control.len() {
                            return Err(format!("{}: topology mismatch", n.name).into());
                        }
                        let mut split = vec![0u32; tuvs.len()];
                        let mut seen = HashMap::new();
                        for (cg, tg) in control.iter().zip(&tgroups) {
                            if cg.len() != tg.len() {
                                return Err(format!("{}: face mismatch", n.name).into());
                            }
                            for (&c, &t) in cg.iter().zip(tg) {
                                if seen.get(&t).is_some_and(|&prev| prev != c) {
                                    // The tessellated topology keeps first wins on splits [I].
                                    continue;
                                }
                                seen.insert(t, c);
                                split[t as usize] = c;
                            }
                        }
                        let mut triangles = Vec::new();
                        triangles_from(&tgroups, &mut triangles)?;
                        Some(BakedTopo {
                            indices: triangles,
                            uvs: tuvs,
                            split,
                        })
                    }
                    _ => None,
                };
                Ok(Geometry::Subdiv(Control {
                    positions: streams.positions,
                    uvs,
                    faces: control,
                    frames: Vec::new(),
                    occlusion_rest: None,
                    levels: n.int("staticSubdivisionLevel"),
                    baked,
                }))
            }
            other => Err(format!("bad geometry {other}").into()),
        }
    }

    fn shape(n: &Node) -> Result<Shape, Error> {
        let [settings] = n.nodes("renderSettings") else {
            return Err("expected one pass".into());
        };
        let [geometry] = n.texts("geometries") else {
            return Err("expected one geometry".into());
        };
        let [transform] = n.texts("transforms") else {
            return Err("expected one transform".into());
        };
        let mut textures = HashMap::new();
        let mut constants = HashMap::new();
        for p in settings
            .nodes("fragmentShaderParams")
            .iter()
            .chain(settings.nodes("vertexShaderParams"))
        {
            if p.kind == "TextureParam" {
                textures.insert(p.text("paramName").to_owned(), p.text("texture").to_owned());
            }
            if p.kind == "ConstantParam"
                && let Ok([x, y, z, w]) = <[f32; 4]>::try_from(p.floats("value"))
            {
                constants.insert(p.text("paramName").to_owned(), [x, y, z, w]);
            }
        }
        Ok(Shape {
            name: n.name.clone(),
            geometry: geometry.clone(),
            transform: transform.clone(),
            vertex: settings.text("vertexShader").to_owned(),
            fragment: settings.text("fragmentShader").to_owned(),
            textures,
            constants,
            cull: settings.text("cullParams") != "noCull",
            alpha_kill: settings.text("alphaParams") == "alphaKill",
        })
    }

    fn animation(&mut self, n: &Node) -> Result<(), Error> {
        for channel in n.nodes("channels") {
            match channel.kind {
                "GeometryChannel" => {
                    let target = channel.text("targetNode");
                    let mut frames = Vec::new();
                    for frame in channel.nodes("frames") {
                        let time = frame.floats("range").first().copied().unwrap_or(0.0);
                        let mut positions = Vec::new();
                        let mut normals = None;
                        let mut tangents = None;
                        let mut occlusion = None;
                        for array in frame.nodes("arrays") {
                            let set = array.int("animationSet");
                            let floats = array.floats("value");
                            let name = array.text("paramName");
                            let name = name.strip_prefix("a2v.").unwrap_or(name);
                            match (name, array.kind) {
                                ("coord", _) => {
                                    positions = floats
                                        .chunks_exact(3)
                                        .map(|v| [v[0], v[1], v[2]])
                                        .collect();
                                }
                                ("normal", _) => {
                                    normals = Some(
                                        floats
                                            .chunks_exact(3)
                                            .map(|v| [v[0], v[1], v[2]])
                                            .collect(),
                                    );
                                }
                                ("tangent", _) => {
                                    tangents = Some(
                                        floats
                                            .chunks_exact(4)
                                            .map(|v| [v[0], v[1], v[2], v[3]])
                                            .collect(),
                                    );
                                }
                                ("occlusion", _) => {
                                    occlusion = Some(floats.to_vec());
                                }
                                _ => {
                                    let _ = set;
                                }
                            }
                        }
                        frames.push(GeoFrame {
                            time: f64::from(time),
                            positions,
                            normals,
                            tangents,
                            occlusion,
                        });
                    }
                    match self.geometries.get_mut(target) {
                        Some(Geometry::Subdiv(control)) => {
                            if let Some(first) = frames.first() {
                                if first.positions.len() != control.positions.len() {
                                    return Err(format!(
                                        "{target}: frames have {} verts, base has {}",
                                        first.positions.len(),
                                        control.positions.len()
                                    )
                                    .into());
                                }
                                if let Some(occ) = first.occlusion.clone() {
                                    control.occlusion_rest = Some(occ);
                                }
                            }
                            control.frames = frames;
                        }
                        Some(Geometry::Baked(baked)) => {
                            if let Some(first) = frames.first()
                                && first.positions.len() != baked.positions.len()
                            {
                                return Err(format!(
                                    "{target}: frames have {} verts, base has {}",
                                    first.positions.len(),
                                    baked.positions.len()
                                )
                                .into());
                            }
                            baked.frames = frames;
                        }
                        _ => return Err(format!("animated {target} is not a mesh").into()),
                    }
                }
                "TransformChannel" => {
                    let target = channel.text("target");
                    let mut poses = Vec::new();
                    for key in channel.nodes("keyframes") {
                        let time = key.float("time");
                        let rotation: [f32; 6] = key
                            .floats("rotation")
                            .try_into()
                            .map_err(|_| "bad rotation")?;
                        let scale_rotation: [f32; 4] = key
                            .floats("scaleRotation")
                            .try_into()
                            .map_err(|_| "bad scale rotation")?;
                        let scale: [f32; 3] =
                            key.floats("scale").try_into().map_err(|_| "bad scale")?;
                        let translation: [f32; 3] = key
                            .floats("translation")
                            .try_into()
                            .map_err(|_| "bad translation")?;
                        poses.push(Pose {
                            time: f64::from(time),
                            rotation: quat([rotation[2], rotation[3], rotation[4], rotation[5]]),
                            scale_rotation: quat(scale_rotation),
                            scale: Vec3::from(scale),
                            translation: Vec3::from(translation),
                        });
                    }
                    self.transform_anims.insert(
                        target.to_owned(),
                        TransformAnim {
                            target: target.to_owned(),
                            poses,
                        },
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The matrix carrying a shape: its transform's rest matrix, moved by the
    /// transform animation if it has one.
    pub fn transform_at(&self, name: &str, seconds: f64) -> Mat4 {
        let base = self
            .transforms
            .get(name)
            .map_or(Mat4::IDENTITY, |t| t.matrix);
        let Some(anim) = self.transform_anims.get(name) else {
            return base;
        };
        if anim.poses.is_empty() {
            return base;
        }
        let (before, after, mix) = bracket(&anim.poses, seconds, |p| p.time);
        let (before, after) = (*before, *after);
        let pose = if mix <= 0.0 {
            before
        } else if mix >= 1.0 {
            after
        } else {
            Pose {
                time: seconds,
                rotation: before.rotation.slerp(after.rotation, mix),
                scale_rotation: before.scale_rotation.slerp(after.scale_rotation, mix),
                scale: before.scale.lerp(after.scale, mix),
                translation: before.translation.lerp(after.translation, mix),
            }
        };
        // Uniform scales commute with the scale rotation [I].
        Mat4::from_scale_rotation_translation(pose.scale, pose.rotation, pose.translation)
    }

    /// Interpolated control positions (and occlusion) of a subdivision
    /// surface at a moment.
    #[allow(clippy::type_complexity)]
    pub fn control_at(
        frames: &[GeoFrame],
        seconds: f64,
    ) -> Option<(Vec<[f32; 3]>, Option<Vec<f32>>)> {
        if frames.is_empty() {
            return None;
        }
        let (before, after, mix) = bracket(frames, seconds, |f| f.time);
        let lerp3 = |a: &[[f32; 3]], b: &[[f32; 3]]| -> Vec<[f32; 3]> {
            a.iter()
                .zip(b)
                .map(|(x, y)| Vec3::from(*x).lerp(Vec3::from(*y), mix).to_array())
                .collect()
        };
        let positions = if mix <= 0.0 {
            before.positions.clone()
        } else if mix >= 1.0 {
            after.positions.clone()
        } else {
            lerp3(&before.positions, &after.positions)
        };
        let occlusion = match (&before.occlusion, &after.occlusion) {
            (Some(a), Some(b)) => Some(if mix <= 0.0 {
                a.clone()
            } else if mix >= 1.0 {
                b.clone()
            } else {
                a.iter().zip(b).map(|(x, y)| x + (y - x) * mix).collect()
            }),
            (Some(a), None) => Some(a.clone()),
            _ => None,
        };
        Some((positions, occlusion))
    }
}

/// Surrounding elements and blend for a sorted time.
pub fn bracket<T>(items: &[T], seconds: f64, at: impl Fn(&T) -> f64) -> (&T, &T, f32) {
    let mut before = &items[0];
    for after in &items[1..] {
        if at(after) >= seconds {
            let span = at(after) - at(before);
            let mix = if span <= 0.0 {
                0.0
            } else {
                ((seconds - at(before)) / span) as f32
            };
            return (before, after, mix.clamp(0.0, 1.0));
        }
        before = after;
    }
    let last = &items[items.len() - 1];
    (before, last, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_split_on_restart() {
        assert_eq!(
            groups(&[0, 1, 2, 3, 0xffff, 4, 5, 6]),
            [vec![0, 1, 2, 3], vec![4, 5, 6]]
        );
    }
}
