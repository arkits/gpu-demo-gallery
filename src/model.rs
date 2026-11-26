//! Reads the scene tree: nodes, geometry, textures, shapes and their passes.
use std::collections::HashMap;

use glam::{EulerRot, Mat4, Vec3};

use crate::anim::Channel;
use crate::pass::{Light, Pass, Resources, Stream};
use crate::tree::{Node, Value};
use crate::{
    Error,
    data::Files,
    demo::{self, DIALECT, Dialect},
};

/// Values and how many belong to a vertex.
#[derive(Debug, Clone, Default)]
pub struct Array {
    pub width: usize,
    pub values: Vec<f32>,
}

/// A mesh. Each kind of stream can come in several versions.
#[derive(Default)]
pub struct Geometry {
    pub name: String,
    pub coords: Vec<Array>,
    pub normals: Vec<Array>,
    pub colors: Vec<Array>,
    pub tex_coords: Vec<Array>,
    pub data: Vec<Array>,
    pub indices: Vec<u32>,
    pub tube: Option<Tube>,
}

impl Geometry {
    pub fn count(&self) -> usize {
        self.coords.first().map_or(0, |a| a.values.len() / 3)
    }

    /// Vertices with the given streams side by side, four numbers each.
    pub fn interleave(&self, streams: &[Stream]) -> Vec<[f32; 4]> {
        let find = |s: &Stream| {
            let (list, i) = match *s {
                Stream::Coords(i) => (&self.coords, i),
                Stream::Normals(i) => (&self.normals, i),
                Stream::Colors(i) => (&self.colors, i),
                Stream::TexCoords(i) => (&self.tex_coords, i),
                Stream::Data(i) => (&self.data, i),
            };
            // Only one version is in the file of those that clips replace.
            match s {
                Stream::Coords(_) | Stream::Normals(_) => list.get(i).or(list.first()),
                _ => list.get(i),
            }
        };
        let arrays: Vec<_> = streams.iter().map(find).collect();
        let mut out = Vec::with_capacity(self.count() * streams.len());
        for v in 0..self.count() {
            for a in &arrays {
                let mut x = [0.0, 0.0, 0.0, 1.0];
                if let Some(a) = a
                    && let Some(row) = a.values.get(v * a.width..(v + 1) * a.width)
                {
                    x[..a.width].copy_from_slice(row);
                }
                out.push(x);
            }
        }
        out
    }

    /// Leads a tube along other points.
    pub fn follow(&mut self, points: Vec<Vec3>) {
        if let Some(mut tube) = self.tube.take() {
            tube.points = points;
            tube.build(self);
            self.tube = Some(tube);
        }
    }

    pub fn points(&self) -> impl Iterator<Item = Vec3> {
        self.coords
            .iter()
            .take(1)
            .flat_map(|a| a.values.chunks_exact(3).map(Vec3::from_slice))
    }
}

#[derive(Clone)]
pub struct Texture {
    pub name: String,
    pub file: String,
    pub cube: bool,
    pub repeat: [bool; 2],
}

pub struct Shape {
    pub name: String,
    pub geometry: usize,
    /// Groups are drawn in this order.
    pub order: u32,
    /// Which of several looks this is.
    pub variant: Option<usize>,
    pub passes: Vec<Pass>,
    pub fur: Option<Fur>,
}

/// Shells drawn around a shape.
pub struct Fur {
    pub pass: Pass,
    pub textures: Vec<usize>,
    pub depth: f32,
    pub scale: [f32; 4],
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
    pub parent: Option<usize>,
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
    pub geometries: Vec<Geometry>,
    pub textures: Vec<Texture>,
    pub lights: Vec<Light>,
    pub shapes: Vec<Shape>,
    /// The sky's cube map.
    pub environment: Option<usize>,
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

fn arrays(g: &Node, field: &str) -> Result<Vec<Array>, Error> {
    g.nodes(field)
        .iter()
        .map(|n| {
            // An empty one keeps its place in the list.
            let Some(data) = n.node("data") else {
                return Ok(Array::default());
            };
            let width = match data.kind {
                "Vec2fArray" => 2,
                "Vec3fArray" => 3,
                "Vec4fArray" => 4,
                "FloatArray" | "Vec1fArray" => 1,
                kind => return Err(format!("unsupported vertex stream {kind}").into()),
            };
            Ok(Array {
                width,
                values: data.floats("value").to_vec(),
            })
        })
        .collect()
}

fn indexed(g: &Node) -> Result<Geometry, Error> {
    let mut out = Geometry {
        name: g.name.clone(),
        coords: arrays(g, "coords")?,
        normals: arrays(g, "normals")?,
        colors: arrays(g, "colors")?,
        tex_coords: arrays(g, "texCoords")?,
        data: arrays(g, "data")?,
        ..Default::default()
    };
    let count = out.count();
    let lists = [
        &out.coords,
        &out.normals,
        &out.colors,
        &out.tex_coords,
        &out.data,
    ];
    if count == 0
        || out.coords[0].width != 3
        || lists
            .iter()
            .flat_map(|l| l.iter())
            .any(|a| a.values.len() != count * a.width)
    {
        return Err("inconsistent vertex streams".into());
    }
    for i in g.nodes("indices") {
        triangles(i.int("primType"), i.ints("indices"), &mut out.indices)?;
    }
    if out.indices.is_empty() || out.indices.iter().any(|&i| i as usize >= count) {
        return Err("invalid vertex indices".into());
    }
    Ok(out)
}

fn height_field(g: &Node, files: &Files) -> Result<Geometry, Error> {
    let source = files.resolve(&crate::data::asset_name(g.text("heightImage")))?;
    let ((width, height), pixels) = crate::scene::rgba(files, &source)?;
    let dimensions = Vec3::from(
        g.vec("dimensions")
            .ok_or("height field requires dimensions")?,
    );
    let repeats: Vec<[f32; 2]> = g
        .floats("textureRepeat")
        .chunks_exact(if DIALECT == Dialect::Third { 3 } else { 2 })
        .map(|v| [v[0], v[1]])
        .collect();
    // The two numbers are integers in the file.
    let raw = g.floats("tesselation");
    let number = |i: usize| raw.get(i).map_or(64, |x| x.to_bits().clamp(2, 512)) as usize;
    let (cols, rows) = (number(0), number(1));
    let height_at = |x: f32, y: f32| {
        let i = (x.clamp(0.0, 1.0) * (width - 1) as f32).round() as usize;
        let j = (y.clamp(0.0, 1.0) * (height - 1) as f32).round() as usize;
        f32::from(pixels[(j * width as usize + i) * 4]) * dimensions.y / 255.0
    };
    let (dx, dy) = (1.0 / (cols - 1) as f32, 1.0 / (rows - 1) as f32);
    let mut out = Geometry {
        name: g.name.clone(),
        tex_coords: vec![
            Array {
                width: 2,
                values: Vec::new()
            };
            repeats.len().max(1)
        ],
        ..Default::default()
    };
    let (mut coords, mut normals) = (Vec::new(), Vec::new());
    for (row, col) in (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))) {
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
        coords.extend([
            (x - 0.5) * dimensions.x,
            height_at(x, y),
            (y - 0.5) * dimensions.z,
        ]);
        normals.extend(bitangent.cross(tangent).normalize_or_zero().to_array());
        for (i, list) in out.tex_coords.iter_mut().enumerate() {
            let [u, v] = repeats.get(i).copied().unwrap_or([1.0; 2]);
            list.values.extend([x * u, y * v]);
        }
    }
    for (row, col) in (0..rows - 1).flat_map(|r| (0..cols - 1).map(move |c| (r, c))) {
        let a = (row * cols + col) as u32;
        let c = a + cols as u32;
        out.indices.extend([a, c, a + 1, a + 1, c, c + 1]);
    }
    out.coords = vec![Array {
        width: 3,
        values: coords,
    }];
    out.normals = vec![Array {
        width: 3,
        values: normals,
    }];
    Ok(out)
}

/// A tube along a uniform cubic B-spline (0x4aca00 in the Squid's viewer).
#[derive(Debug, Clone)]
pub struct Tube {
    pub points: Vec<Vec3>,
    rings: usize,
    sides: usize,
    radius: f32,
    /// Factors of the radius along the tube.
    radii: Vec<f32>,
}

impl Tube {
    fn read(n: &Node) -> Result<Self, Error> {
        let tube = Self {
            points: n
                .floats("controlPts")
                .chunks_exact(3)
                .map(Vec3::from_slice)
                .collect(),
            rings: n.int("subDivisions") as usize,
            sides: n.int("tubeSides") as usize,
            radius: n.float("tubeRadius"),
            radii: n.floats("tubeRadii").to_vec(),
        };
        if tube.points.len() < 2 || tube.rings < 2 || tube.sides < 3 {
            return Err("bad curve".into());
        }
        Ok(tube)
    }

    /// Both ends count as three points.
    fn point(&self, i: usize) -> Vec3 {
        self.points[i.saturating_sub(2).min(self.points.len() - 1)]
    }

    fn segment(&self, t: f32) -> (usize, f32) {
        let s = t * (self.points.len() + 1) as f32;
        (s as usize, s.fract())
    }

    fn position(&self, t: f32) -> Vec3 {
        let (i, u) = self.segment(t);
        let (u2, u3, v) = (u * u, u * u * u, 1.0 - u);
        (self.point(i) * (v * v * v)
            + self.point(i + 1) * (3.0 * u3 - 6.0 * u2 + 4.0)
            + self.point(i + 2) * (3.0 * u + 3.0 * u2 - 3.0 * u3 + 1.0)
            + self.point(i + 3) * u3)
            / 6.0
    }

    fn tangent(&self, t: f32) -> Vec3 {
        let (i, u) = self.segment(t);
        let n = self.points.len();
        if i == 0 {
            return self.points[1] - self.points[0];
        }
        if i >= n {
            return self.points[n - 1] - self.points[n.saturating_sub(3)];
        }
        let v = 1.0 - u;
        self.point(i) * (-0.5 * v * v)
            + self.point(i + 1) * (1.5 * u * u - 2.0 * u)
            + self.point(i + 2) * (0.5 + u - 1.5 * u * u)
            + self.point(i + 3) * (0.5 * u * u)
    }

    fn width(&self, t: f32) -> f32 {
        let Some(last) = self.radii.len().checked_sub(1) else {
            return 1.0;
        };
        let s = t * last as f32;
        let i = (s as usize).min(last);
        let next = self.radii[(i + 1).min(last)];
        self.radii[i] + (next - self.radii[i]) * s.fract()
    }

    /// Rings of vertices, the first of each repeated at its end.
    fn build(&self, g: &mut Geometry) {
        let array = |width| Array {
            width,
            values: Vec::new(),
        };
        let (mut coords, mut normals, mut uv, mut along) = (array(3), array(3), array(2), array(4));
        let mut up = Vec3::Y;
        for ring in 0..self.rings {
            let t = ring as f32 / (self.rings - 1) as f32;
            let center = self.position(t);
            let tangent = self.tangent(t).normalize_or(Vec3::Z);
            let right = up.cross(tangent).normalize_or(Vec3::X);
            up = tangent.cross(right).normalize_or(Vec3::Y);
            let radius = self.radius * self.width(t);
            for side in 0..=self.sides {
                let v = side as f32 / self.sides as f32;
                let (sin, cos) = (v * std::f32::consts::TAU).sin_cos();
                let normal = up * cos + right * sin;
                coords.values.extend((center + normal * radius).to_array());
                normals.values.extend(normal.to_array());
                uv.values.extend([t, v]);
                along.values.extend(tangent.extend(1.0).to_array());
            }
        }
        g.coords = vec![coords];
        g.normals = vec![normals];
        g.tex_coords = vec![uv, along];
    }

    fn geometry(self, name: &str) -> Geometry {
        let mut out = Geometry {
            name: name.to_owned(),
            ..Default::default()
        };
        self.build(&mut out);
        let across = self.sides as u32 + 1;
        for ring in 0..self.rings as u32 - 1 {
            for side in 0..across - 1 {
                let a = ring * across + side;
                let b = a + across;
                out.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        out.tube = Some(self);
        out
    }
}

/// Finds nodes by name, ignoring case, the way scenes refer to them.
struct Names(HashMap<String, usize>);

impl Names {
    fn new<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        let mut out = HashMap::new();
        for (i, n) in names.enumerate() {
            out.entry(n.trim().to_ascii_lowercase()).or_insert(i);
        }
        Self(out)
    }

    fn get(&self, name: &str) -> Option<usize> {
        self.0.get(&name.trim().to_ascii_lowercase()).copied()
    }
}

/// What a shape has from the groups around it.
#[derive(Clone, Copy, Default)]
struct Place {
    node: usize,
    order: u32,
    variant: Option<usize>,
}

struct Reader<'a> {
    resources: Resources<'a>,
    geometries: Names,
}

impl Model {
    /// `shared` is the scene whose textures and lights this one can name too.
    pub fn new(root: &Node, files: &Files, shared: Option<&Model>) -> Result<Self, Error> {
        let mut model = Self::default();
        model
            .nodes
            .push(Transform::read(&Node::empty("Transform"), None));
        for node in root.nodes("transforms") {
            model.walk_transforms(node, None);
        }
        for t in root.nodes("textures") {
            if t.text("filename").is_empty() {
                continue;
            }
            model.textures.push(Texture {
                name: t.name.clone(),
                file: files.resolve(&crate::data::asset_name(t.text("filename")))?,
                cube: t.kind == "CubicTexture",
                repeat: [t.int("wrapU") != 0, t.int("wrapV") != 0],
            });
        }
        if let Some(shared) = shared {
            model.textures.extend(shared.textures.iter().cloned());
        }
        for g in root.nodes("geometries") {
            let geometry = match g.kind {
                "IndexedFaceSet" => indexed(g),
                "HeightField" => height_field(g, files),
                "Curve" => Tube::read(g).map(|t| t.geometry(&g.name)),
                _ => continue,
            };
            model
                .geometries
                .push(geometry.map_err(|e| format!("{}: {e}", g.name))?);
        }
        let nodes = Names::new(model.nodes.iter().map(|n| n.name.as_str()));
        let find = |name: &str| nodes.get(name).filter(|&i| i > 0);
        model.lights = root
            .nodes("lights")
            .iter()
            .filter_map(|l| Light::read(l, find))
            .collect();
        if let Some(shared) = shared {
            // Nodes are not shared, so these must not move.
            let free = shared.lights.iter().filter(|l| l.node.is_none());
            model.lights.extend(free.cloned());
        }
        let textures = Names::new(model.textures.iter().map(|t| t.name.as_str()));
        let cubes: Vec<bool> = model.textures.iter().map(|t| t.cube).collect();
        let texture = |name: &str| textures.get(name).map(|i| (i, cubes[i]));
        let program = |name: &str| files.text(&crate::data::asset_name(name));
        for background in root.nodes("backgrounds") {
            if let Some(settings) = background.node("renderSettings") {
                model.environment = settings
                    .texts("textures")
                    .iter()
                    .filter_map(|name| texture(name))
                    .find(|t| t.1)
                    .map(|t| t.0);
            }
        }
        let lights = model.lights.clone();
        let reader = Reader {
            resources: Resources {
                root,
                spares: if DIALECT == Dialect::First { 8 } else { 16 },
                lights: &lights,
                program: &program,
                find: &find,
                texture: &texture,
            },
            geometries: Names::new(model.geometries.iter().map(|g| g.name.as_str())),
        };
        for group in root.nodes("renderGroups") {
            let place = Place::default();
            model.walk_shapes(&reader, group, place)?;
        }
        model.read_animations(root)?;
        Ok(model)
    }

    pub fn read_animations(&mut self, root: &Node) -> Result<(), Error> {
        self.animations = root
            .nodes("animations")
            .iter()
            .map(|node| Ok::<_, Error>(Animation::new(self, crate::anim::read(node)?)))
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

    fn walk_shapes(&mut self, reader: &Reader<'_>, node: &Node, at: Place) -> Result<(), Error> {
        let mut at = at;
        if let Some(i) = (reader.resources.find)(node.text("transform")) {
            at.node = i;
        }
        match node.kind {
            "Shape" | "FurShape" => self
                .read_shape(reader, node, at)
                .map_err(|e| format!("{}: {e}", node.name))?,
            "LodShape" => {
                for (i, child) in node.nodes("children").iter().enumerate() {
                    at.variant = Some(i);
                    self.walk_shapes(reader, child, at)?;
                }
            }
            kind => {
                if kind == "SortingGroup" {
                    at.order = node.int("sortSet");
                }
                for child in node.nodes("children").iter().chain(node.nodes("shapes")) {
                    self.walk_shapes(reader, child, at)?;
                }
            }
        }
        Ok(())
    }

    fn read_shape(&mut self, reader: &Reader<'_>, node: &Node, at: Place) -> Result<(), Error> {
        let r = &reader.resources;
        if node.texts("geometries").is_empty() {
            // The program makes the mesh. It is filled in before it is drawn.
            self.geometries.push(Geometry::default());
            let passes = node
                .nodes("renderSettings")
                .iter()
                .map(|s| Pass::read(s, at.node, r));
            self.shapes.push(Shape {
                name: node.name.clone(),
                geometry: self.geometries.len() - 1,
                order: at.order,
                variant: at.variant,
                passes: passes.collect::<Result<_, _>>()?,
                fur: None,
            });
        }
        // Fur shapes have one pass where others have a list.
        let one = node.node("renderSettings").into_iter();
        let settings: Vec<_> = node.nodes("renderSettings").iter().chain(one).collect();
        let swaps = demo::SWAPS.iter().filter(|s| s.shape == node.name);
        let swapped: Vec<Node> = swaps
            .filter_map(|s| {
                let mut own = (*settings.first()?).clone();
                own.set("vertexProgram", Value::Text(s.program.to_owned()));
                own.set("pixelShader", Value::Text(s.shader.to_owned()));
                let names = s.textures.iter().map(|t| (*t).to_owned());
                own.set("textures", Value::Texts(names.collect()));
                Some(own)
            })
            .collect();
        let settings: Vec<_> = settings.into_iter().chain(&swapped).collect();
        for geometry in node
            .texts("geometries")
            .iter()
            .filter_map(|n| reader.geometries.get(n))
        {
            let passes = settings
                .iter()
                .map(|s| Pass::read(s, at.node, r))
                .collect::<Result<Vec<_>, _>>()?;
            let fur = match node.node("furRenderSettings") {
                Some(s) => Some(Fur {
                    pass: Pass::read(s, at.node, r)?,
                    textures: node
                        .texts("furTextures")
                        .iter()
                        .filter_map(|n| (r.texture)(n).map(|t| t.0))
                        .collect(),
                    depth: node.float("furDepth"),
                    scale: node.vec::<4>("furScale").unwrap_or([1.0; 4]),
                }),
                None => None,
            };
            self.shapes.push(Shape {
                name: node.name.clone(),
                geometry,
                order: at.order,
                variant: at.variant,
                passes,
                fur,
            });
        }
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    /// World transforms of all nodes without animation.
    pub fn rest(&self) -> Vec<Mat4> {
        self.pose([])
    }

    /// World transforms of all nodes, with animations at their times.
    pub fn pose<'a>(
        &self,
        animations: impl IntoIterator<Item = (&'a Animation, f64)>,
    ) -> Vec<Mat4> {
        let mut fields: Vec<Fields> = self.nodes.iter().map(|n| n.rest).collect();
        let channels = animations
            .into_iter()
            .flat_map(|(a, ms)| a.0.iter().map(move |c| (c, ms)));
        for ((node, channel), ms) in channels {
            let f = &mut fields[*node];
            let v = match channel.field.as_str() {
                "rotation" => &mut f.rotation,
                "scale" => &mut f.scale,
                _ => &mut f.translation,
            };
            for (i, c) in channel.curves.iter().enumerate() {
                if let Some(c) = c {
                    v[i] = c.at(ms);
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

    /// Moves the control points of tubes. Returns the geometries that changed.
    pub fn bend(&mut self, ms: f64) -> Vec<usize> {
        let mut changed = Vec::new();
        for a in &self.animations {
            for (geometry, channel) in &a.1 {
                let Some(tube) = &mut self.geometries[*geometry].tube else {
                    continue;
                };
                let Some(point) = tube.points.get_mut(channel.index) else {
                    continue;
                };
                for (i, c) in channel.curves.iter().enumerate() {
                    if let Some(c) = c {
                        point[i] = c.at(ms);
                    }
                }
                if !changed.contains(geometry) {
                    changed.push(*geometry);
                }
            }
        }
        for &i in &changed {
            let g = &mut self.geometries[i];
            if let Some(tube) = g.tube.take() {
                tube.build(g);
                g.tube = Some(tube);
            }
        }
        changed
    }

    /// The box around the shapes a filter picks, at rest.
    pub fn bounds(&self, pick: impl Fn(&Shape) -> bool) -> (Vec3, Vec3) {
        let world = self.rest();
        let points = self.shapes.iter().filter(|s| pick(s)).flat_map(|s| {
            let m = world[s.passes.first().map_or(0, |p| p.node)];
            self.geometries[s.geometry]
                .points()
                .map(move |p| m.transform_point3(p))
        });
        points.fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(p), max.max(p)),
        )
    }
}

/// Channels and the nodes they move, and those that move points of geometries.
pub struct Animation(Vec<(usize, Channel)>, Vec<(usize, Channel)>);

impl Animation {
    pub fn new(model: &Model, channels: Vec<Channel>) -> Self {
        let mut out = Self(Vec::new(), Vec::new());
        for c in channels {
            match c.field.as_str() {
                "rotation" | "scale" | "translation" => {
                    if let Some(node) = model.find(&c.node) {
                        out.0.push((node, c));
                    }
                }
                "controlPts" => {
                    let name = c.node.trim();
                    if let Some(i) = model.geometries.iter().position(|g| g.name.trim() == name) {
                        out.1.push((i, c));
                    }
                }
                _ => {}
            }
        }
        out
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

    #[test]
    fn tubes_face_outward() {
        let tube = Tube {
            points: vec![Vec3::ZERO, Vec3::Z, Vec3::Z * 2.0, Vec3::Z * 3.0],
            rings: 5,
            sides: 8,
            radius: 0.5,
            radii: vec![1.0],
        };
        assert!((tube.position(0.0) - Vec3::ZERO).length() < 1e-5);
        assert!((tube.position(1.0) - Vec3::Z * 3.0).length() < 1e-5);
        let g = tube.geometry("");
        let at = |i: u32| Vec3::from_slice(&g.coords[0].values[i as usize * 3..]);
        let out = |i: u32| Vec3::from_slice(&g.normals[0].values[i as usize * 3..]);
        for t in g.indices.chunks_exact(3) {
            let face = (at(t[1]) - at(t[0])).cross(at(t[2]) - at(t[0]));
            assert!(face.length() < 1e-6 || face.dot(out(t[0])) > 0.0);
        }
    }
}
