//! The viewer's frame (`0x42c480`): update the scene at a time, then draw each render pass
//! (`0x42c110`): viewport, camera, every shader's own parameters, the render list (shapes
//! collected and sorted), target and clears, override settings, backgrounds, the list,
//! and the window pass's menus.

use std::collections::HashMap;

use nv_asm::parse::Stage;

use crate::gl::{ColorTarget, Gl, Mesh, MeshArray, States};
use crate::math::{dot, length, normalize, Mat4, Vec3, Vec4};
use crate::sort::sort;
use crate::world::*;

/// One collected draw: shape, its settings index, distance.
#[derive(Clone, Copy)]
struct Item {
    shape: usize,
    settings: usize,
    distance: f32,
}

#[derive(Default)]
struct Bin {
    set: u32,
    /// sortType 1 (drawn far to near), 2 (near to far), other (as collected).
    far: Vec<Item>,
    near: Vec<Item>,
    any: Vec<Item>,
}

/// A pass's render list (`pass + 0x110`): a bin per sort set, drawn in increasing order of the
/// set as a signed number (`0x469c30` sorts them; sets are unique, so the sort's order is too).
#[derive(Default)]
struct RenderList {
    bins: Vec<Bin>,
    min: f32,
    max: f32,
}

impl RenderList {
    fn clear(&mut self) {
        for b in &mut self.bins {
            b.far.clear();
            b.near.clear();
            b.any.clear();
        }
        self.min = 1e10;
        self.max = 0.0;
    }

    fn add(&mut self, item: Item, set: u32, sort_type: u32, radius: f32) {
        self.min = self.min.min(item.distance - radius);
        self.max = self.max.max(item.distance + radius);
        let b = match self.bins.iter().position(|b| b.set == set) {
            Some(i) => i,
            None => {
                self.bins.push(Bin { set, ..Default::default() });
                self.bins.len() - 1
            }
        };
        let b = &mut self.bins[b];
        match sort_type {
            1 => b.far.push(item),
            2 => b.near.push(item),
            _ => b.any.push(item),
        }
    }
}

/// What a demo can hook into the frame.
pub trait Hooks {
    /// After a shape's geometry is drawn (the viewer's post-draw callbacks).
    fn after_shape(&mut self, _v: &mut Viewer, _shape: usize) {}
    /// The window pass's menus.
    fn menus(&mut self, _v: &mut Viewer) {}
}

pub struct NoHooks;
impl Hooks for NoHooks {}

pub struct Viewer {
    pub world: World,
    pub gl: Gl,
    /// Per geometry.
    pub meshes: Vec<Option<Mesh>>,
    /// Per world shader: the compiled program.
    pub programs: Vec<Option<(Stage, usize)>>,
    /// Per world texture: the GL texture.
    pub textures: Vec<Option<usize>>,
    /// The viewer's built-in programs by name (`_scr_tex`, `_white`, …).
    pub builtins: HashMap<String, (Stage, usize)>,
    background: Mesh,
    lists: Vec<RenderList>,
    /// Viewer time, seconds.
    pub time: f64,
    /// The pass being drawn.
    pub pass: usize,
    /// The camera of the pass being drawn.
    pub camera: Option<usize>,
    /// MSVC `rand()` state, for MiscParam 6.
    pub rand: u32,
    /// The shape whose transform the state manager holds (`0x468c50`).
    current_transform: Option<usize>,
    /// Draw counts of the last frame.
    pub draws: usize,
    /// Debugging: shapes whose names contain one of these are not drawn.
    pub skip: Vec<String>,
    /// The programs and states of the shape being drawn, which stay bound for the
    /// post-draw callbacks (GL state).
    pub current: Option<(usize, usize, States)>,
}

/// The default window cube of backgrounds (`0x46fe70`): corners, and as texture coordinates
/// the corners' directions with y turned over.
fn background_mesh(gl: &Gl) -> Mesh {
    let p: [[f32; 3]; 8] = [
        [-1., -1., 1.],
        [-1., 1., 1.],
        [1., -1., 1.],
        [1., 1., 1.],
        [-1., 1., -1.],
        [1., 1., -1.],
        [-1., -1., -1.],
        [1., -1., -1.],
    ];
    let k = f32::from_bits(0x3f13cd36);
    let t: Vec<f32> = p.iter().flat_map(|v| [v[0] * k, -v[1] * k, v[2] * k]).collect();
    let pos: Vec<f32> = p.iter().flatten().copied().collect();
    let idx: Vec<u16> = vec![0, 1, 2, 2, 1, 3, 1, 4, 3, 3, 4, 5, 4, 6, 5, 5, 6, 7, 6, 0, 7, 7, 0, 2, 2, 3, 7, 7, 3, 5, 6, 4, 0, 0, 4, 1];
    gl.mesh(
        &[("a2v.objCoord".into(), MeshArray::F32(pos, 3)), ("a2v.texture".into(), MeshArray::F32(t, 3))],
        &[(3, idx)],
        false,
        false,
    )
}

/// MSVC's `rand()`.
pub fn crt_rand(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(214013).wrapping_add(2531011);
    (*state >> 16) & 0x7fff
}

impl Viewer {
    pub fn new(world: World, gl: Gl) -> Viewer {
        let background = background_mesh(&gl);
        let n = world.passes.len();
        Viewer {
            meshes: (0..world.geometries.len()).map(|_| None).collect(),
            programs: vec![None; world.shaders.len()],
            textures: vec![None; world.textures.len()],
            builtins: HashMap::new(),
            world,
            gl,
            background,
            lists: (0..n).map(|_| RenderList::default()).collect(),
            time: 0.0,
            pass: 0,
            camera: None,
            rand: 1,
            current_transform: None,
            draws: 0,
            skip: vec![],
            current: None,
        }
    }

    /// Builds the GPU buffers of every geometry.
    pub fn build_meshes(&mut self) {
        for (i, g) in self.world.geometries.iter().enumerate() {
            let arrays: Vec<(String, MeshArray)> = g
                .arrays
                .iter()
                .map(|a| {
                    let data = match &a.data {
                        ArrayData::Floats(v, k) => MeshArray::F32(v.clone(), *k),
                        ArrayData::Halves(v, k) => MeshArray::F16(v.clone(), *k),
                        ArrayData::Bytes(v, k) => MeshArray::U8(v.clone(), *k),
                    };
                    (a.channel.clone(), data)
                })
                .collect();
            self.meshes[i] = Some(self.gl.mesh(&arrays, &g.lists, g.restart, false));
        }
    }

    /// World matrices of every transform, parents first (they are added that way).
    pub fn update_transforms(&mut self) {
        for i in 0..self.world.transforms.len() {
            let local = self.world.transforms[i].local();
            let world = match self.world.transforms[i].parent {
                Some(p) => self.world.transforms[p].world * local,
                None => local,
            };
            self.world.transforms[i].world = world;
        }
        // Lights take world values from their transforms (0x441b00).
        for l in &mut self.world.lights {
            match l.transform {
                Some(t) => {
                    let m = self.world.transforms[t].world;
                    l.world_direction = m.vector(l.direction);
                    l.world_position = m.point(l.position);
                }
                None => {
                    l.world_direction = l.direction;
                    l.world_position = l.position;
                }
            }
        }
    }

    /// Sets the channels of an animation at a time (`0x45cc30`); true when every curve is
    /// past its end.
    pub fn animate(&mut self, anim: usize, t: f64) {
        let a = &self.world.animations[anim];
        let local = (t - a.start) * a.time_scale as f64;
        let mut sets: Vec<(usize, String, [Option<f32>; 3])> = vec![];
        for c in &a.channels {
            if let crate::anim::Target::Vec3 { node, field, curves } = c {
                let Some(ti) = self.world.transform(node) else { continue };
                let v = [0, 1, 2].map(|k| curves[k].as_ref().map(|c| c.at(local)));
                sets.push((ti, field.clone(), v));
            }
        }
        for (ti, field, v) in sets {
            set_vec3(&mut self.world.transforms[ti].kind, &field, v);
        }
    }

    /// Camera matrices (`0x439ec0`, `0x439680`, `0x43a5c0`).
    pub fn update_camera(&mut self, c: usize) {
        let aspect = self.gl.window.0 as f32 / self.gl.window.1 as f32;
        let parent = self.world.cameras[c].transform.map(|t| self.world.transforms[t].world);
        let cam = &mut self.world.cameras[c];
        let q = cam.orientation;
        let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        let conj = [q[0] / len, -q[1] / len, -q[2] / len, -q[3] / len];
        let mut rot = Mat4::from_quat(conj);
        if let Some(w) = parent {
            rot = w.inverse() * rot;
        }
        let p = cam.position;
        cam.view = rot * Mat4::translation([-p[0], -p[1], -p[2]]);
        let a = if cam.screen_aspect { aspect } else { cam.aspect };
        cam.proj = if cam.ortho {
            Mat4::ortho(cam.left, cam.right, cam.bottom, cam.top, cam.near, cam.far)
        } else {
            Mat4::perspective(cam.fov, a, cam.near, cam.far)
        };
        cam.view_proj = cam.proj * cam.view;
        let inv = cam.view.inverse();
        cam.eye = [inv.at(0, 3), inv.at(1, 3), inv.at(2, 3)];
        cam.dir = inv.vector([0.0, 0.0, -1.0]);
    }

    fn state(&self, s: Option<usize>) -> Option<State> {
        s.map(|i| self.world.states[i].1)
    }

    /// The states of a settings, the pass's override taking what it sets (`0x4436b0`).
    fn states_of(&self, s: &Settings) -> States {
        let ov = self.world.passes.get(self.pass).and_then(|p| p.override_settings.as_ref());
        let pick = |own: Option<usize>, o: Option<usize>| o.or(own);
        let get = |f: fn(&Settings) -> Option<usize>| pick(f(s), ov.and_then(f));
        let cull = match self.state(get(|s| s.cull)) {
            Some(State::Cull(c)) => c,
            _ => CULL_DEFAULT,
        };
        let blend = match self.state(get(|s| s.blend)) {
            Some(State::Blend(c)) => c,
            _ => BLEND_DEFAULT,
        };
        let depth = match self.state(get(|s| s.depth)) {
            Some(State::Depth(c)) => c,
            _ => DEPTH_DEFAULT,
        };
        let alpha = match self.state(get(|s| s.alpha)) {
            Some(State::Alpha(c)) => c,
            _ => ALPHA_DEFAULT,
        };
        let stencil = match self.state(get(|s| s.stencil)) {
            Some(State::Stencil(c)) => c,
            _ => STENCIL_DEFAULT,
        };
        let raster = match self.state(get(|s| s.raster)) {
            Some(State::Raster(c)) => c,
            _ => RASTER_DEFAULT,
        };
        States { cull, blend, depth, alpha, stencil, raster }
    }

    fn transform_matrix(&self, transform: Option<usize>, map: u32, pass: Option<usize>) -> Mat4 {
        let pass = pass.unwrap_or(self.pass);
        let cam = self.world.passes.get(pass).and_then(|p| p.camera).or(self.camera).map(|c| &self.world.cameras[c]);
        let (v, p, vp) = match cam {
            Some(c) => (c.view, c.proj, c.view_proj),
            None => (Mat4::IDENTITY, Mat4::IDENTITY, Mat4::IDENTITY),
        };
        // The window matrix (0x43ee70): [-1, 1] to [0, 1].
        let w = Mat4([0.5, 0., 0., 0.5, 0., 0.5, 0., 0.5, 0., 0., 0.5, 0.5, 0., 0., 0., 1.]);
        let Some(t) = transform else {
            // Without a transform the model matrix is the identity.
            return match map {
                0 | 8 | 12 | 16 | 20 | 24 | 29 | 33 | 37 => v,
                1 | 13 | 17 | 21 | 25 | 30 | 34 | 38 => p,
                2 | 10 | 27 => w * vp,
                3 => w * p,
                4 | 9 | 14 | 18 | 22 | 26 | 31 | 35 | 39 => vp,
                5 => w * vp,
                6 => v.inverse(),
                7 | 11 | 15 | 19 | 23 | 28 | 32 | 36 => Mat4::IDENTITY,
                _ => Mat4::IDENTITY,
            };
        };
        let m = self.world.transforms[t].world;
        let b = self.world.transforms[t].bind;
        let mb = m * b;
        match map {
            0 => v,
            1 => p,
            2 => w,
            3 => w * p,
            4 => vp,
            5 => w * vp,
            6 => v.inverse(),
            7 => m,
            8 => v * m,
            9 => vp * m,
            10 => w * vp * m,
            11 => m.inverse(),
            12 => v * m.inverse(),
            13 => p * m.inverse(),
            14 => vp * m.inverse(),
            15 => m.transpose(),
            16 => v * m.transpose(),
            17 => p * m.transpose(),
            18 => vp * m.transpose(),
            23 => mb,
            24 => v * mb,
            25 => p * mb,
            26 => vp * mb,
            27 => w * vp * mb,
            28 => mb.inverse(),
            29 => v * mb.inverse(),
            30 => p * mb.inverse(),
            31 => vp * mb.inverse(),
            32 => mb.transpose(),
            33 => v * mb.transpose(),
            34 => p * mb.transpose(),
            35 => vp * mb.transpose(),
            36 => mb.inverse().transpose(),
            37 => v * mb.inverse().transpose(),
            38 => p * mb.inverse().transpose(),
            39 => vp * mb.inverse().transpose(),
            42 => (v * m).inverse(),
            _ => Mat4::IDENTITY,
        }
    }

    /// A light property (`0x440080`).
    fn light_value(&self, light: usize, property: u32, space: u32, transform: Option<usize>) -> Vec4 {
        let l = &self.world.lights[light];
        let cam = self.camera.map(|c| &self.world.cameras[c]);
        let to_space = |v: Vec3, point: bool| -> Vec3 {
            match space {
                0 => match transform {
                    Some(t) => {
                        let inv = self.world.transforms[t].world.inverse();
                        if point { inv.point(v) } else { inv.vector(v) }
                    }
                    None => v,
                },
                2 => match cam {
                    Some(c) => {
                        if point { c.view.point(v) } else { c.view.vector(v) }
                    }
                    None => v,
                },
                _ => v,
            }
        };
        match property {
            0 => [l.color[0] * l.intensity, l.color[1] * l.intensity, l.color[2] * l.intensity, 0.0],
            1 => {
                let p = to_space(l.world_position, true);
                [p[0], p[1], p[2], 1.0]
            }
            2 => {
                let d = to_space(l.world_direction, false);
                [d[0], d[1], d[2], 1.0]
            }
            3 => {
                let a = l.attenuation;
                [a[0], a[1], a[2], if l.kind == LightKind::Point { 1.0 } else { 0.0 }]
            }
            4 => [l.inner, l.outer, l.inner.cos(), l.outer.cos()],
            _ => {
                // [I] 5: the half-angle of the light direction and the view direction (0x4407c4).
                let d = l.world_direction;
                let e = cam.map(|c| c.view.inverse().vector([0.0, 0.0, -1.0])).unwrap_or([0.0, 0.0, -1.0]);
                let h = normalize([-d[0] - e[0], -d[1] - e[1], -d[2] - e[2]]);
                [h[0], h[1], h[2], 1.0]
            }
        }
    }

    fn misc_value(&mut self, param: u32, transform: Option<usize>) -> Vec4 {
        let cam = self.camera.map(|c| self.world.cameras[c].clone());
        match param {
            0 => [self.time as f32; 4],
            1 => {
                let e = cam.map(|c| c.eye).unwrap_or([0.0; 3]);
                let p = match transform {
                    Some(t) => self.world.transforms[t].world.inverse().point(e),
                    None => e,
                };
                [p[0], p[1], p[2], 1.0]
            }
            2 => {
                let d = cam.map(|c| c.dir).unwrap_or([0.0, 0.0, -1.0]);
                let p = match transform {
                    Some(t) => self.world.transforms[t].world.inverse().vector(d),
                    None => d,
                };
                let n = normalize(p);
                [n[0], n[1], n[2], 1.0]
            }
            3 => {
                let e = cam.map(|c| c.eye).unwrap_or([0.0; 3]);
                [e[0], e[1], e[2], 1.0]
            }
            4 => {
                let d = cam.map(|c| c.dir).unwrap_or([0.0, 0.0, -1.0]);
                [d[0], d[1], d[2], 1.0]
            }
            5 => {
                let (n, f) = cam.map(|c| (c.near, c.far)).unwrap_or((0.1, 100.0));
                [n, f, f - n, 0.0]
            }
            _ => {
                let mut r = || crt_rand(&mut self.rand) as f32 * (1.0 / 32767.0);
                [r(), r(), r(), r()]
            }
        }
    }

    /// Applies one parameter to a program (`0x43f210`, `0x43d050`, `0x43d0b0`, …).
    fn apply(&mut self, prog: (Stage, usize), param: &Param, shape_transform: Option<usize>) {
        match param {
            Param::Transform { name, transform, pass, map } => {
                let m = self.transform_matrix(*transform, *map, *pass);
                if prog.0 == Stage::Vertex {
                    let rows = [m.row(0), m.row(1), m.row(2), m.row(3)];
                    self.gl.set_vertex_param(prog.1, name, &rows);
                }
            }
            Param::Constant { name, value, .. } => self.set_vector(prog, name, *value),
            Param::Texture { name, texture, state } => {
                let tex = texture.and_then(|t| self.textures[t]);
                if prog.0 == Stage::Fragment {
                    self.gl.bind_texture(prog.1, name, tex);
                }
                // A texture state goes to the texture object (0x470690).
                if let (Some(t), Some(s)) = (tex, state) {
                    if let State::Texture(ts) = self.world.states[*s].1 {
                        if let Some(x) = self.gl.textures[t].as_mut() {
                            x.state = ts;
                        }
                    }
                }
            }
            Param::Light { name, light, property, space } => {
                if let Some(l) = light {
                    let v = self.light_value(*l, *property, *space, shape_transform);
                    self.set_vector(prog, name, v);
                }
            }
            Param::Misc { name, param } => {
                let v = self.misc_value(*param, shape_transform);
                self.set_vector(prog, name, v);
            }
        }
    }

    pub fn set_vector(&mut self, prog: (Stage, usize), name: &str, v: Vec4) {
        match prog.0 {
            Stage::Vertex => self.gl.set_vertex_param(prog.1, name, &[v]),
            Stage::Fragment => self.gl.fragment_programs[prog.1].set(name, v),
        }
    }

    /// Sets a ConstantParam node's value by its name, as the demos do (`0x404f00`).
    pub fn set_constant(&mut self, node: &str, v: Vec4) -> bool {
        let mut found = false;
        let mut set = |params: &mut Vec<Param>| {
            for p in params {
                if let Param::Constant { node: n, value, .. } = p {
                    if n == node {
                        *value = v;
                        found = true;
                    }
                }
            }
        };
        for s in &mut self.world.shaders {
            set(&mut s.params);
        }
        for sh in &mut self.world.shapes {
            for st in &mut sh.settings {
                set(&mut st.vertex_params);
                set(&mut st.fragment_params);
            }
        }
        for b in &mut self.world.backgrounds {
            set(&mut b.settings.vertex_params);
            set(&mut b.settings.fragment_params);
        }
        for p in &mut self.world.passes {
            if let Some(o) = &mut p.override_settings {
                set(&mut o.vertex_params);
                set(&mut o.fragment_params);
            }
        }
        found
    }

    /// Collects a shape or group into the pass's render list (`0x499430`, `0x497450`).
    fn collect(&mut self, r: ShapeRef, list: &mut RenderList) {
        match r {
            ShapeRef::Group(g) => {
                let children = self.world.groups[g].children.clone();
                for c in children {
                    self.collect(c, list);
                }
            }
            ShapeRef::Shape(s) => {
                if self.world.shapes[s].cull == 3 {
                    return;
                }
                if self.skip.iter().any(|k| self.world.shapes[s].name.contains(k.as_str())) {
                    return;
                }
                self.update_bounds(s);
                let cam = self.camera.map(|c| self.world.cameras[c].clone());
                let shape = &self.world.shapes[s];
                // Distance along the view: near plus the distance to the near plane.
                let distance = match cam {
                    Some(c) => {
                        let r2 = c.view_proj.row(2);
                        let r3 = c.view_proj.row(3);
                        let mut pl = [-(r3[0] + r2[0]), -(r3[1] + r2[1]), -(r3[2] + r2[2]), -(r3[3] + r2[3])];
                        let l = length([pl[0], pl[1], pl[2]]);
                        if l > 0.0 {
                            pl = pl.map(|x| x / l);
                        }
                        let c3 = shape.world_center;
                        c.near - dot(c3, [pl[0], pl[1], pl[2]]) - pl[3]
                    }
                    None => 0.0,
                };
                let ov = self.world.passes.get(self.pass).and_then(|p| p.override_settings.as_ref());
                let n = shape.settings.len();
                let radius = shape.world_radius;
                for k in 0..n {
                    let set = &self.world.shapes[s].settings[k];
                    // With an override, its sort set and type place the draw [I].
                    let (sset, stype) = match ov {
                        Some(o) => (o.sort_set, o.sort_type),
                        None => (set.sort_set, set.sort_type),
                    };
                    list.add(Item { shape: s, settings: k, distance }, sset, stype, radius);
                }
            }
        }
    }

    /// World bounds (`0x497970`).
    fn update_bounds(&mut self, s: usize) {
        let k = f32::from_bits(0x3f13cd3a);
        let t = self.world.shapes[s].transform.map(|t| self.world.transforms[t].world);
        let shape = &mut self.world.shapes[s];
        match t {
            Some(m) => {
                shape.world_center = m.point(shape.center);
                shape.world_radius = length(m.vector([k, k, k])) * shape.radius;
            }
            None => {
                shape.world_center = shape.center;
                shape.world_radius = shape.radius;
            }
        }
    }

    /// Applies a settings (states and both programs with their parameters) and draws the
    /// shape's geometries.
    fn draw_item(&mut self, hooks: &mut dyn Hooks, item: Item) -> Result<(), Box<dyn std::error::Error>> {
        let settings = self.world.shapes[item.shape].settings[item.settings].clone();
        let transform = self.world.shapes[item.shape].transform;
        self.current_transform = transform;
        self.bind_settings(&settings, transform);
        let geoms = self.world.shapes[item.shape].geometries.clone();
        let states = self.states_of(&settings);
        let (vp, fp) = self.programs_of(&settings);
        self.current = match (vp, fp) {
            (Some(vp), Some(fp)) => Some((vp, fp, states)),
            _ => None,
        };
        if let (Some(vp), Some(fp)) = (vp, fp) {
            for g in geoms {
                let Some(mesh) = self.meshes[g].take() else { continue };
                for l in 0..mesh.lists.len() {
                    self.gl.draw(vp, fp, &states, &mesh, l)?;
                    self.draws += 1;
                }
                self.meshes[g] = Some(mesh);
            }
        }
        hooks.after_shape(self, item.shape);
        Ok(())
    }

    /// Draws a mesh's first list with the programs and states left bound by the last shape.
    pub fn draw_current(&mut self, mesh: &Mesh) -> Result<(), Box<dyn std::error::Error>> {
        if let Some((vp, fp, st)) = self.current {
            self.gl.draw(vp, fp, &st, mesh, 0)?;
            self.draws += 1;
        }
        Ok(())
    }

    fn programs_of(&self, s: &Settings) -> (Option<usize>, Option<usize>) {
        let ov = self.world.passes.get(self.pass).and_then(|p| p.override_settings.as_ref());
        let vs = ov.and_then(|o| o.vertex).or(s.vertex);
        let fs = ov.and_then(|o| o.fragment).or(s.fragment);
        let vp = vs.and_then(|i| self.programs[i]).filter(|p| p.0 == Stage::Vertex).map(|p| p.1);
        let fp = fs.and_then(|i| self.programs[i]).filter(|p| p.0 == Stage::Fragment).map(|p| p.1);
        (vp, fp)
    }

    /// Binds a settings' programs and applies their parameters, every draw (`0x45fdd0`).
    pub fn bind_settings(&mut self, s: &Settings, transform: Option<usize>) {
        let ov = self.world.passes.get(self.pass).and_then(|p| p.override_settings.clone());
        // Override programs bring their own parameter lists.
        let (vs, vparams) = match &ov {
            Some(o) if o.vertex.is_some() => (o.vertex, o.vertex_params.clone()),
            _ => (s.vertex, s.vertex_params.clone()),
        };
        let (fs, fparams) = match &ov {
            Some(o) if o.fragment.is_some() => (o.fragment, o.fragment_params.clone()),
            _ => (s.fragment, s.fragment_params.clone()),
        };
        if let Some(p) = vs.and_then(|i| self.programs[i]) {
            if p.0 == Stage::Vertex {
                self.gl.bind_vertex_program(p.1);
            }
            for q in &vparams {
                self.apply(p, q, transform);
            }
        }
        if let Some(p) = fs.and_then(|i| self.programs[i]) {
            for q in &fparams {
                self.apply(p, q, transform);
            }
        }
    }

    /// Draws a background (`0x46fa90`): the camera moves to position · positionScale with
    /// near 0.1, far 2 and the lens times fovScale for the cube.
    fn draw_background(&mut self, b: usize) -> Result<(), Box<dyn std::error::Error>> {
        let Some(c) = self.camera else { return Ok(()) };
        let saved = self.world.cameras[c].clone();
        let bg = self.world.backgrounds[b].clone();
        {
            let cam = &mut self.world.cameras[c];
            cam.position = saved.position.map(|x| x * bg.position_scale);
            cam.near = 0.1;
            cam.far = 2.0;
            cam.fov = saved.fov * bg.fov_scale;
        }
        self.update_camera(c);
        self.bind_settings(&bg.settings, None);
        let states = self.states_of(&bg.settings);
        if let (Some(vp), Some(fp)) = self.programs_of(&bg.settings) {
            let mesh = std::mem::replace(&mut self.background, Mesh { streams: HashMap::new(), lists: vec![], lines: vec![], vertices: 0 });
            let r = self.gl.draw(vp, fp, &states, &mesh, 0);
            self.background = mesh;
            r?;
            self.draws += 1;
        }
        let cam = &mut self.world.cameras[c];
        cam.position = saved.position;
        cam.near = saved.near;
        cam.far = saved.far;
        cam.fov = saved.fov;
        self.update_camera(c);
        Ok(())
    }

    /// Draws every render pass.
    pub fn draw(&mut self, hooks: &mut dyn Hooks) -> Result<(), Box<dyn std::error::Error>> {
        self.draws = 0;
        for pass in 0..self.world.passes.len() {
            self.pass = pass;
            let p = self.world.passes[pass].clone();
            self.camera = p.camera;
            // Viewport (0x4469e0): the target's size, or the window's.
            let size = match p.color.or(p.depth).and_then(|t| self.textures[t]).and_then(|t| self.gl.textures[t].as_ref()) {
                Some(t) => (t.width as i32, t.height as i32),
                None => (self.gl.window.0 as i32, self.gl.window.1 as i32),
            };
            let custom = p.viewport != [0, 0, -1, -1];
            let vx = if custom { p.viewport[0].clamp(0, size.0) } else { 0 };
            let vy = if custom { p.viewport[1].clamp(0, size.1) } else { 0 };
            let vw = if p.viewport[2] != -1 { p.viewport[2].min(size.0 - vx) } else { size.0 };
            let vh = if p.viewport[3] != -1 { p.viewport[3].min(size.1 - vy) } else { size.1 };
            if let Some(c) = self.camera {
                self.update_camera(c);
            }
            // Every shader's own parameters (0x45fe90).
            for s in 0..self.world.shaders.len() {
                if let Some(prog) = self.programs[s] {
                    let params = self.world.shaders[s].params.clone();
                    for q in &params {
                        self.apply(prog, q, None);
                    }
                }
            }
            // Render list (0x42bb80).
            let mut list = std::mem::take(&mut self.lists[pass]);
            list.clear();
            let shapes = if p.use_scene_shapes { self.world.scene_shapes.clone() } else { p.shapes.clone() };
            for r in shapes {
                self.collect(r, &mut list);
            }
            for b in &mut list.bins {
                sort(&mut b.near, |a, b| a.distance < b.distance);
                sort(&mut b.far, |a, b| a.distance < b.distance);
            }
            list.bins.sort_by_key(|b| b.set as i32);
            // Target, viewport, clears (0x42b7e0, 0x42bac0).
            let color = match p.color.and_then(|t| self.textures[t]) {
                Some(t) => ColorTarget::Texture(t),
                None if p.depth.is_some() => ColorTarget::None,
                None => ColorTarget::Window,
            };
            let depth = p.depth.and_then(|t| self.textures[t]);
            self.gl.begin_pass(
                color,
                depth,
                p.clears_color.then_some(p.clear_color),
                p.clears_depth.then_some(p.clear_depth),
                p.clears_stencil.then_some(p.clear_stencil),
                [vx, vy, vw, vh],
            );
            let backgrounds = if p.use_scene_backgrounds { self.world.scene_backgrounds.clone() } else { p.backgrounds.clone() };
            for b in backgrounds {
                self.draw_background(b)?;
            }
            // The list (0x42d240): per set, near to far, as collected, far to near.
            for bi in 0..list.bins.len() {
                let near = list.bins[bi].near.clone();
                let any = list.bins[bi].any.clone();
                let far = list.bins[bi].far.clone();
                for it in near.iter().chain(any.iter()).chain(far.iter().rev()) {
                    self.draw_item(hooks, *it)?;
                }
            }
            self.lists[pass] = list;
            if p.color.is_none() && p.depth.is_none() {
                hooks.menus(self);
            }
        }
        Ok(())
    }
}

fn set_vec3(kind: &mut TransformKind, field: &str, v: [Option<f32>; 3]) {
    let target: Option<&mut Vec3> = match kind {
        TransformKind::Maya { rotation, translation, scale, .. } => match field {
            "rotation" => Some(rotation),
            "translation" => Some(translation),
            "scale" => Some(scale),
            _ => None,
        },
        TransformKind::Joint { rotation, translation, scale, .. } => match field {
            "rotation" => Some(rotation),
            "translation" => Some(translation),
            "scale" => Some(scale),
            _ => None,
        },
        TransformKind::Matrix(_) => None,
    };
    if let Some(t) = target {
        for k in 0..3 {
            if let Some(x) = v[k] {
                t[k] = x;
            }
        }
    }
}
