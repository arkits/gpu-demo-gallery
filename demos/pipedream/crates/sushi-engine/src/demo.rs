//! The whole demo: loaded data plus everything needed to draw one frame.
use std::collections::{BTreeMap, HashMap};

use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};
use sushi_formats::archive::Archive;
use sushi_formats::path::{self, FlyPath};
use sushi_formats::reader::{cstr, f32_at, u32_at};
use sushi_formats::shader_header::ShaderHeader;
use sushi_formats::{anim, object, scene};
use sushi_shader::wgsl::{self, ElementType};
use sushi_shader::{Const, Constants, ShaderFile, VarKind};

use crate::camera::Camera;
use crate::gpu::Gpu;
use crate::light::Light;
use crate::object::Clip;
use crate::pipeline::*;
use crate::targets::Targets;

pub type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

/// Scene time at which the music starts: `startTime` in `Sushi2.ini`.
pub const START_TIME: f32 = 10.0;

/// Mesh record flags (+0), matching the words in `Normal.sds`.
const CASTS: u32 = 1;
const RECEIVES: u32 = 2;
const TRANSPARENT: u32 = 8;
/// Helper geometry that is loaded but never drawn (0x432160).
const HIDDEN: u32 = 0x10;

struct Stream {
    layout: LayoutId,
    buffers: Vec<usize>,
    index_buffers: Vec<usize>,
    commands: usize,
}

struct Mesh {
    name: String,
    shader: usize,
    flags: u32,
    streams: Vec<Stream>,
    maps: BTreeMap<String, usize>,
    commands: Vec<Vec<[u32; 4]>>,
    params: Vec<(String, Vec<[f32; 4]>)>,
}

struct Model {
    meshes: Vec<Mesh>,
    textures: Vec<Option<TextureId>>,
    center: Vec3,
    /// Largest `BoundsExpand` of the meshes' shaders.
    expand: f32,
}

struct Instance {
    model: usize,
    clip: Clip,
    casts_shadow: bool,
}

struct SceneItem {
    draw: usize,
    vertices: usize,
    indices: usize,
    start: u32,
    count: u32,
}

struct SceneDraw {
    shader: usize,
    layout: LayoutId,
    lights: Vec<usize>,
    textures: Vec<Option<TextureId>>,
}

/// Values the engine feeds to shaders for one draw.
struct Context<'a> {
    view_proj: Mat4,
    camera: Vec3,
    time: f32,
    lights: &'a [&'a Light],
    ambient: &'a [&'a Light],
    bones: &'a [Mat4],
    slots: &'a [usize],
    params: &'a BTreeMap<String, [f32; 4]>,
    overrides: &'a BTreeMap<String, [f32; 4]>,
    center: Vec3,
    velocity: [f32; 4],
    orientation: Mat4,
}

type LayoutId = usize;
/// Shader file, shader, pass, vertex layout.
type PipelineKey = (usize, usize, usize, LayoutId);

pub struct Demo {
    path: FlyPath,
    textures: Textures,
    pipelines: Pipelines,
    arena: Arena,
    buffers: Vec<wgpu::Buffer>,
    shaders: Vec<ShaderFile>,
    shader_ids: HashMap<String, usize>,
    layouts: Vec<VertexLayout>,
    cache: HashMap<PipelineKey, Option<PipelineId>>,
    lights: Vec<Light>,
    light_textures: Vec<TextureId>,
    models: Vec<Model>,
    instances: Vec<Instance>,
    scene_draws: Vec<SceneDraw>,
    scene_items: Vec<SceneItem>,
    quad: Option<(usize, usize, usize)>,
    /// Problems found while drawing, each reported once.
    warnings: std::collections::BTreeSet<String>,
    draw_calls: usize,
    chip_logo: Vec<sushi_formats::texture::Level>,
}

fn element_type(kind: u32) -> Result<ElementType> {
    Ok(match kind {
        0 => ElementType::Float1,
        1 => ElementType::Float2,
        2 => ElementType::Float3,
        3 => ElementType::Float4,
        4 => ElementType::Color,
        5 => ElementType::Ubyte4,
        k => return Err(format!("vertex element type {k}").into()),
    })
}

fn layout(header: &ShaderHeader, stream: usize) -> Result<VertexLayout> {
    let s = header.streams.get(stream).ok_or("missing vertex stream")?;
    let mut offset = 0;
    let mut elements = Vec::new();
    for e in &s.elements {
        let (name, index) = e.semantic();
        elements.push((name.to_string(), index, element_type(e.kind)?, offset));
        offset += e.size() as u32;
    }
    Ok(VertexLayout {
        stride: offset,
        elements,
    })
}

impl Demo {
    /// `data` is the contents of the user's `Sushi.ati`.
    pub fn load(gpu: &Gpu, data: &[u8]) -> Result<Self> {
        let archive = Archive::open(data)?;
        let file = |name: &str| {
            archive
                .get(name)
                .ok_or_else(|| format!("{name} not in Sushi.ati"))
        };
        let text = |name: &str| file(name).map(|b| String::from_utf8_lossy(b).into_owned());
        let scene = scene::read_scene(file("Scenes/Animusic.ati")?)?;
        let mut demo = Self {
            path: path::read_path(&text("FlyPaths/Animusic.pth")?)?,
            textures: Textures::new(gpu),
            pipelines: Pipelines::default(),
            arena: Arena::new(gpu),
            buffers: Vec::new(),
            shaders: Vec::new(),
            shader_ids: HashMap::new(),
            layouts: Vec::new(),
            cache: HashMap::new(),
            lights: scene.lights.iter().map(Light::new).collect(),
            light_textures: Vec::new(),
            models: Vec::new(),
            instances: Vec::new(),
            scene_draws: Vec::new(),
            scene_items: Vec::new(),
            quad: None,
            warnings: Default::default(),
            draw_calls: 0,
            chip_logo: Vec::new(),
        };
        demo.load_scene(gpu, &archive, &scene)?;
        demo.load_objects(gpu, &archive, &text("Objects/animusic.ini")?)?;
        demo.prewarm(gpu);
        let globals = scene::read_scene(file("Globals.ati")?)?;
        // Texture 3 of Globals.ati is ChipLogo.tga (0x421c50).
        if let Some(t) = globals
            .textures
            .get(3)
            .filter(|t| t.name.contains("ChipLogo"))
        {
            // Stored bottom row first; the original draws it with v = 0 at the bottom.
            demo.chip_logo = t.decode()?.swap_remove(0);
            for l in &mut demo.chip_logo {
                let rows = l.rgba.chunks_exact(l.width * 4).rev().flatten().copied();
                l.rgba = rows.collect();
            }
        }
        Ok(demo)
    }

    fn intern(&mut self, layout: VertexLayout) -> LayoutId {
        self.layouts
            .iter()
            .position(|l| *l == layout)
            .unwrap_or_else(|| {
                self.layouts.push(layout);
                self.layouts.len() - 1
            })
    }

    fn buffer(&mut self, gpu: &Gpu, data: &[u8], usage: wgpu::BufferUsages) -> usize {
        use wgpu::util::DeviceExt;
        let mut data = data.to_vec();
        data.resize(data.len().next_multiple_of(4).max(4), 0);
        self.buffers.push(
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: &data,
                    usage,
                }),
        );
        self.buffers.len() - 1
    }

    fn index_buffer(&mut self, gpu: &Gpu, indices: &[u16]) -> usize {
        self.buffer(
            gpu,
            bytemuck::cast_slice(indices),
            wgpu::BufferUsages::INDEX,
        )
    }

    fn shader(&mut self, archive: &Archive, name: &str) -> Result<usize> {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(id) = self.shader_ids.get(&key) {
            return Ok(*id);
        }
        let path = format!("shaders/{key}");
        let dir = path.rsplit_once('/').map_or("", |p| p.0).to_string();
        let read = |n: &str| -> Option<String> {
            Some(archive.get(n)?.iter().map(|&c| c as char).collect())
        };
        let src = read(&path).ok_or_else(|| format!("no shader {path}"))?;
        let include = |n: &str| read(&format!("{dir}/{n}"));
        let file = sushi_shader::parse(&src, &include).map_err(|e| format!("{name}: {e}"))?;
        self.shaders.push(file);
        self.shader_ids.insert(key, self.shaders.len() - 1);
        Ok(self.shaders.len() - 1)
    }

    fn load_scene(&mut self, gpu: &Gpu, archive: &Archive, s: &scene::Scene) -> Result<()> {
        let mut textures = Vec::new();
        for t in &s.textures {
            textures.push(if t.name.is_empty() {
                None
            } else {
                self.textures.add(gpu, t)?
            });
        }
        self.light_textures = self
            .lights
            .iter()
            .map(|l| l.texture.and_then(|t| textures[t]).unwrap_or(BLACK))
            .collect();
        for d in &s.draws {
            let g = &s.geometry[d.geometry()];
            let sh = &s.shaders[g.shader()];
            let shader = self.shader(archive, &cstr(&sh.raw))?;
            let n = self.shaders[shader].textures.len();
            let layout = self.intern(layout(&sh.header, 0).unwrap_or(VertexLayout {
                stride: 0,
                elements: Vec::new(),
            }));
            self.scene_draws.push(SceneDraw {
                shader,
                layout,
                lights: d.lights(),
                textures: (0..n)
                    .map(|k| d.texture(k).and_then(|t| textures[t]))
                    .collect(),
            });
        }
        let mut made: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
        for item in s.groups.iter().flat_map(|g| g.draw_items()) {
            let gi = s.draws[item.draw].geometry();
            let g = &s.geometry[gi];
            let st = &g.chunk.streams[0];
            let (vertices, indices) = match made.get(&(gi, item.vertex_buffer)) {
                Some(b) => *b,
                None => {
                    let raw = &st.buffers[0][item.vertex_buffer].1;
                    let v = self.buffer(gpu, raw, wgpu::BufferUsages::VERTEX);
                    let ib = st
                        .index_buffer_of(item.vertex_buffer)
                        .ok_or("missing index buffer")?;
                    let i = self.index_buffer(gpu, &g.chunk.indices[ib].1);
                    made.insert((gi, item.vertex_buffer), (v, i));
                    (v, i)
                }
            };
            if self.layouts[self.scene_draws[item.draw].layout].stride != st.stride() {
                return Err(format!("scene vertex stride mismatch in geometry {gi}").into());
            }
            self.scene_items.push(SceneItem {
                draw: item.draw,
                vertices,
                indices,
                start: item.start,
                count: item.count,
            });
        }
        // Merge neighboring index ranges of the same draw.
        self.scene_items
            .sort_by_key(|i| (i.draw, i.vertices, i.start));
        self.scene_items.dedup_by(|b, a| {
            let joins = (a.draw, a.vertices, a.start + a.count) == (b.draw, b.vertices, b.start);
            if joins {
                a.count += b.count;
            }
            joins
        });
        let shader = self.shader(archive, "Animusic/qDestAlphaShadow.ssh")?;
        let corners: [[f32; 3]; 4] = [
            [-1.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
        ];
        let v = self.buffer(
            gpu,
            bytemuck::cast_slice(&corners),
            wgpu::BufferUsages::VERTEX,
        );
        let i = self.index_buffer(gpu, &[0, 1, 2, 2, 1, 3]);
        self.quad = Some((shader, v, i));
        Ok(())
    }

    fn load_objects(&mut self, gpu: &Gpu, archive: &Archive, ini: &str) -> Result<()> {
        let mut models: HashMap<String, usize> = HashMap::new();
        let lines: Vec<Vec<&str>> = ini
            .lines()
            .skip(1)
            .map(|l| l.split_whitespace().collect())
            .collect();
        let get = |n: &str| {
            archive
                .get(&format!("Objects/{n}"))
                .ok_or_else(|| format!("no object {n}"))
        };
        // Texel data is stored once and shared by name, so load it all first.
        let mut objects: HashMap<&str, object::Object> = HashMap::new();
        for l in lines.iter().filter(|l| l.len() == 4) {
            if !objects.contains_key(l[2]) {
                let o = object::read_object(get(l[2])?)?;
                for t in o.textures.iter().filter(|t| !t.name.is_empty()) {
                    self.textures.add(gpu, t)?;
                }
                objects.insert(l[2], o);
            }
        }
        for l in lines.iter().filter(|l| l.len() == 4) {
            let casts_shadow = match l[0] {
                "SGlobal" => true,
                "Global" => false,
                k => return Err(format!("unknown object kind {k}").into()),
            };
            let model = match models.get(l[2]) {
                Some(m) => *m,
                None => {
                    let m = self.model(gpu, archive, &objects[l[2]])?;
                    models.insert(l[2].to_string(), m);
                    m
                }
            };
            let clip = Clip::new(&anim::read_animation(get(l[3])?)?);
            self.instances.push(Instance {
                model,
                clip,
                casts_shadow,
            });
        }
        Ok(())
    }

    fn model(&mut self, gpu: &Gpu, archive: &Archive, o: &object::Object) -> Result<usize> {
        let textures = o.textures.iter().map(|t| self.textures.find(t)).collect();
        let mut meshes = Vec::new();
        for m in &o.meshes {
            let shader = self.shader(archive, &m.shader)?;
            let frame = m.frames.first().ok_or("mesh without vertices")?;
            let vb: Vec<usize> = frame
                .iter()
                .map(|v| self.buffer(gpu, &v.data, wgpu::BufferUsages::VERTEX))
                .collect();
            let ib: Vec<usize> = m
                .index_buffers
                .iter()
                .map(|i| self.index_buffer(gpu, &i.indices))
                .collect();
            let mut streams = Vec::new();
            for (k, a) in m.stream_arrays.iter().enumerate() {
                let layout = layout(&m.header, k)?;
                for v in &a.vertex_buffers {
                    if frame[*v as usize].stride != layout.stride as usize {
                        return Err(format!("{}: vertex stride mismatch", m.shader).into());
                    }
                }
                streams.push(Stream {
                    layout: self.intern(layout),
                    buffers: a.vertex_buffers.iter().map(|v| vb[*v as usize]).collect(),
                    index_buffers: a.offsets.iter().map(|i| ib[*i as usize]).collect(),
                    commands: u32_at(&a.raw, 0x10) as usize,
                });
            }
            let names = m.header.stream_maps.iter().map(|s| s.0.clone());
            let maps = names
                .zip(m.stream_maps.iter().map(|s| s[0] as usize))
                .collect();
            meshes.push(Mesh {
                name: m.shader.clone(),
                shader,
                flags: u32_at(&m.raw, 0),
                streams,
                maps,
                commands: m.commands.clone(),
                params: m
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.values.clone()))
                    .collect(),
            });
        }
        let center = Vec3::from([6, 7, 8].map(|i| f32_at(&o.bounds, i * 4)));
        let expand = meshes.iter().filter_map(|m: &Mesh| -> Option<f32> {
            self.shaders[m.shader]
                .misc
                .get("BoundsExpand")?
                .first()?
                .parse()
                .ok()
        });
        let expand = expand.fold(1.0, f32::max);
        self.models.push(Model {
            meshes,
            textures,
            center,
            expand,
        });
        Ok(self.models.len() - 1)
    }

    /// Scene seconds covered by the camera path.
    pub fn duration(&self) -> f32 {
        self.path.frames.len() as f32 * self.path.step
    }

    pub fn chip_logo(&self) -> &[sushi_formats::texture::Level] {
        &self.chip_logo
    }

    /// Compiles every pipeline the show can need, so playback does not stall.
    fn prewarm(&mut self, gpu: &Gpu) {
        let mut keys: std::collections::HashSet<PipelineKey> = Default::default();
        let wanted = |s: &sushi_shader::Shader| s.has("Normal") || s.has("Shadow");
        for d in &self.scene_draws {
            for (i, s) in self.shaders[d.shader].shaders.iter().enumerate() {
                if s.has("Normal") && s.properties.iter().any(|p| p.starts_with("FastRT")) {
                    keys.extend((0..s.passes.len()).map(|p| (d.shader, i, p, d.layout)));
                }
            }
        }
        for m in self
            .models
            .iter()
            .flat_map(|m| &m.meshes)
            .filter(|m| m.flags & HIDDEN == 0 || m.flags & CASTS != 0)
        {
            for (i, s) in self.shaders[m.shader]
                .shaders
                .iter()
                .enumerate()
                .filter(|s| wanted(s.1))
            {
                for (p, pass) in s.passes.iter().enumerate() {
                    let stream = m.maps.get(&pass.stream_map).and_then(|s| m.streams.get(*s));
                    if let Some(stream) = stream {
                        keys.insert((m.shader, i, p, stream.layout));
                    }
                }
            }
        }
        for k in keys {
            self.pipeline(gpu, k);
        }
        // Unused variants may legitimately not fit a mesh; only report real draws.
        self.warnings.clear();
    }

    /// Draws nothing visible with every pipeline. Browsers compile pipelines on
    /// first use, so wait for this work to finish before starting the show.
    pub fn warm_up(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, targets: &Targets) {
        let vertices = self.buffer(gpu, &[0; 1024], wgpu::BufferUsages::VERTEX);
        let indices = self.index_buffer(gpu, &[0; 4]);
        self.arena.clear();
        self.arena.upload(gpu);
        self.pipelines.bind_uniforms(gpu, &self.arena);
        let none = BTreeMap::new();
        let mut calls = Vec::new();
        for pipeline in 0..self.pipelines.list.len() {
            let n = self.pipelines.list[pipeline].module.samplers.len();
            let textures = vec![BLACK; n];
            self.pipelines
                .prepare(gpu, &mut self.textures, pipeline, &textures, &none);
            calls.push(DrawCall {
                pipeline,
                vs_offset: 0,
                ps_offset: 0,
                textures,
                vertices,
                indices,
                start: 0,
                count: 3,
            });
        }
        let mut pass = targets.begin(enc);
        for c in &calls {
            self.pipelines.record(&mut pass, c, &self.buffers);
        }
        // The placeholder bind groups use default samplers; do not reuse them.
        drop(pass);
        self.pipelines.forget_textures();
    }

    /// Draws in the last frame.
    pub fn draw_calls(&self) -> usize {
        self.draw_calls
    }

    pub fn warnings(&self) -> impl Iterator<Item = &str> {
        self.warnings.iter().map(String::as_str)
    }

    /// Finds the first shader with all the properties (0x445c20). If none exists
    /// for this many lights, fewer lights are used so the object still draws.
    fn variant(&self, file: usize, property: &str, lights: usize, extra: &str) -> Option<usize> {
        (0..=lights).rev().find_map(|n| {
            let rt = format!("FastRT{n}");
            self.shaders[file]
                .shaders
                .iter()
                .position(|s| s.has(property) && s.has(&rt) && (extra.is_empty() || s.has(extra)))
        })
    }

    fn pipeline(&mut self, gpu: &Gpu, key: PipelineKey) -> Option<PipelineId> {
        if let Some(p) = self.cache.get(&key) {
            return *p;
        }
        let (file, shader, pass, layout) = key;
        let layout = &self.layouts[layout];
        let f = &self.shaders[file];
        let s = &f.shaders[shader];
        let p = &s.passes[pass];
        let cube = |stage: u32| {
            p.samplers
                .get(&stage)
                .is_some_and(|s| f.textures.iter().any(|t| t.name == s.texture && t.cube))
        };
        let made = wgsl::translate(p, &|s, i| layout.find(s, i), &cube);
        let id = match made {
            Ok(m) => Some(self.pipelines.create(gpu, &s.name, p, m, layout)),
            Err(e) => {
                self.warnings.insert(format!("{} pass {pass}: {e}", s.name));
                None
            }
        };
        self.cache.insert(key, id);
        id
    }

    /// Builds one draw: constants, textures and buffers.
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        gpu: &Gpu,
        key: &PipelineKey,
        id: PipelineId,
        ctx: &Context,
        light_textures: &[TextureId],
        texture: &dyn Fn(usize) -> Option<TextureId>,
        geometry: (usize, usize, u32, u32),
        out: &mut Vec<DrawCall>,
    ) {
        let file = &self.shaders[key.0];
        let pass = &file.shaders[key.1].passes[key.2];
        let module = &self.pipelines.list[id].module;
        let mut vs = vec![[0.0; 4]; module.vs_consts as usize];
        if pass.vs.is_none() {
            let m = ctx.view_proj.transpose().to_cols_array_2d();
            vs[..4].copy_from_slice(&m);
        }
        fill(&mut vs, &pass.vs_const, file, ctx);
        let mut ps = vec![[0.0; 4]; module.ps_consts as usize];
        if pass.ps.is_none() {
            ps[0] = pass.factor;
        }
        fill(&mut ps, &pass.ps_const, file, ctx);
        let textures: Vec<TextureId> = module
            .samplers
            .keys()
            .map(|stage| {
                let name = &pass.samplers.get(stage)?.texture;
                let k = file.textures.iter().position(|t| t.name == *name)?;
                match &file.textures[k].app {
                    Some(app) => {
                        let n: usize = app.strip_prefix("FastRTTexture")?.parse().ok()?;
                        light_textures.get(n - 1).copied()
                    }
                    None => texture(k),
                }
            })
            .map(|t| t.unwrap_or(BLACK))
            .collect();
        let (vertices, indices, start, count) = geometry;
        let samplers = &self.shaders[key.0].shaders[key.1].passes[key.2].samplers;
        self.pipelines
            .prepare(gpu, &mut self.textures, id, &textures, samplers);
        out.push(DrawCall {
            pipeline: id,
            vs_offset: self.arena.push(&vs),
            ps_offset: self.arena.push(&ps),
            textures,
            vertices,
            indices,
            start,
            count,
        });
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        time: f32,
    ) {
        let camera = Camera::on_path(&self.path, time);
        self.draw_from(gpu, enc, targets, time, &camera);
    }

    /// Runs the original frame script, `Normal.sds`.
    pub fn draw_from(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        time: f32,
        camera: &Camera,
    ) {
        self.arena.clear();
        let size = targets.resolved.size();
        let aspect = size.width as f32 / size.height as f32;
        let vp = Camera::projection(aspect) * camera.view();
        let mut calls = Vec::new();
        let visible = self.visible(time, camera.position);
        // The shadow pass alone ignores HIDDEN.
        let shown = |f: u32, mask: u32, want: u32| f & (mask | HIDDEN) == want;
        let opaque = |f: u32| shown(f, TRANSPARENT | RECEIVES, RECEIVES);
        let transparent = |f: u32| shown(f, TRANSPARENT | RECEIVES, TRANSPARENT | RECEIVES);
        let unshadowed = |f: u32| shown(f, RECEIVES, 0);
        self.objects(
            gpu, time, camera, vp, &visible, "Normal", &opaque, &mut calls,
        );
        self.scene(gpu, time, camera, vp, &mut calls);
        self.objects(
            gpu,
            time,
            camera,
            vp,
            &visible,
            "Shadow",
            &|f| f & CASTS != 0,
            &mut calls,
        );
        self.shadow_quad(gpu, &mut calls);
        self.objects(
            gpu,
            time,
            camera,
            vp,
            &visible,
            "Normal",
            &transparent,
            &mut calls,
        );
        self.objects(
            gpu,
            time,
            camera,
            vp,
            &visible,
            "Normal",
            &unshadowed,
            &mut calls,
        );

        self.draw_calls = calls.len();
        self.arena.upload(gpu);
        self.pipelines.bind_uniforms(gpu, &self.arena);
        let mut pass = targets.begin(enc);
        for c in &calls {
            self.pipelines.record(&mut pass, c, &self.buffers);
        }
    }

    /// Visible instances, farthest first so blended ones stack correctly.
    fn visible(&self, time: f32, eye: Vec3) -> Vec<usize> {
        let mut v: Vec<(f32, usize)> = self
            .instances
            .iter()
            .enumerate()
            .filter(|(_, i)| i.clip.visible(time))
            .map(|(k, i)| (i.clip.bounds(time).map_or(0.0, |b| b.2.distance(eye)), k))
            .collect();
        v.sort_by(|a, b| b.0.total_cmp(&a.0));
        v.into_iter().map(|x| x.1).collect()
    }

    fn scene(&mut self, gpu: &Gpu, time: f32, camera: &Camera, vp: Mat4, out: &mut Vec<DrawCall>) {
        let none = BTreeMap::new();
        let lights = std::mem::take(&mut self.lights);
        let draws = std::mem::take(&mut self.scene_draws);
        let items = std::mem::take(&mut self.scene_items);
        // Items are grouped by draw, which all share constants and textures.
        for items in items.chunk_by(|a, b| a.draw == b.draw) {
            let d = &draws[items[0].draw];
            let lit: Vec<&Light> = d.lights.iter().map(|l| &lights[*l]).collect();
            let Some(shader) = self.variant(d.shader, "Normal", lit.len(), "") else {
                self.warnings
                    .insert(format!("no scene shader for {} lights", lit.len()));
                continue;
            };
            let ctx = Context {
                view_proj: vp,
                camera: camera.position,
                time,
                lights: &lit,
                ambient: &[],
                bones: &[],
                slots: &[],
                params: &none,
                overrides: &none,
                center: Vec3::ZERO,
                velocity: [0.0; 4],
                orientation: Mat4::IDENTITY,
            };
            let by_light: Vec<TextureId> =
                d.lights.iter().map(|l| self.light_textures[*l]).collect();
            for pass in 0..self.shaders[d.shader].shaders[shader].passes.len() {
                let key = (d.shader, shader, pass, d.layout);
                let Some(id) = self.pipeline(gpu, key) else {
                    continue;
                };
                let tex = |k: usize| d.textures.get(k).copied().flatten();
                let geometry = |i: &SceneItem| (i.vertices, i.indices, i.start, i.count);
                self.emit(
                    gpu,
                    &key,
                    id,
                    &ctx,
                    &by_light,
                    &tex,
                    geometry(&items[0]),
                    out,
                );
                let first = out.len() - 1;
                for i in &items[1..] {
                    let mut call = out[first].clone();
                    (call.vertices, call.indices, call.start, call.count) = geometry(i);
                    out.push(call);
                }
            }
        }
        self.lights = lights;
        self.scene_draws = draws;
        self.scene_items = items;
    }

    #[allow(clippy::too_many_arguments)]
    fn objects(
        &mut self,
        gpu: &Gpu,
        time: f32,
        camera: &Camera,
        vp: Mat4,
        visible: &[usize],
        property: &str,
        want: &dyn Fn(u32) -> bool,
        out: &mut Vec<DrawCall>,
    ) {
        let shadow = property == "Shadow";
        let lights = std::mem::take(&mut self.lights);
        let instances = std::mem::take(&mut self.instances);
        let models = std::mem::take(&mut self.models);
        let frustum = Frustum::new(vp);
        for &k in visible {
            let inst = &instances[k];
            let model = &models[inst.model];
            if (shadow && !inst.casts_shadow) || !model.meshes.iter().any(|m| want(m.flags)) {
                continue;
            }
            let Some((min, max, center, radius)) = inst.clip.bounds(time) else {
                continue;
            };
            let radius = radius * model.expand;
            // Shadows can reach the view from outside it.
            if !shadow && !frustum.touches(center, radius) {
                continue;
            }
            let chosen: Vec<usize> = if shadow {
                // The single strongest shadow light whose volume holds the object.
                let mut best = (0.0, None);
                for (i, l) in lights.iter().enumerate().filter(|(_, l)| l.casts_shadows) {
                    let r = l.reach(center, radius);
                    if r > best.0 && l.encloses(center, radius, min, max) {
                        best = (r, Some(i));
                    }
                }
                best.1.into_iter().collect()
            } else {
                let lit = lights.iter().enumerate().filter(|(_, l)| {
                    l.fast_rt && l.reach(center, radius) > 0.0 && l.color(time) != Vec3::ZERO
                });
                lit.map(|(i, _)| i).take(4).collect()
            };
            if shadow && chosen.is_empty() {
                continue;
            }
            let ambient: Vec<&Light> = lights
                .iter()
                .filter(|l| l.object_ambient && l.reach(center, radius) > 0.0)
                .take(4)
                .collect();
            let bones = inst.clip.bones(time);
            let params = inst.clip.params(time);
            let large = |n: &str, min: f32| params.get(n).is_some_and(|v| v[0].abs() > min);
            // 0x43e5e0
            let extra = if large("SOLGlow", 1.0 / 255.0) {
                "Glow"
            } else if large("SOLVibrate", 1e-4) {
                "Vibrate"
            } else {
                ""
            };
            let (velocity, orientation) = inst.clip.motion(model.center, time);
            let lit: Vec<&Light> = chosen.iter().map(|i| &lights[*i]).collect();
            let by_light: Vec<TextureId> = chosen.iter().map(|i| self.light_textures[*i]).collect();
            for mesh in model.meshes.iter().filter(|m| want(m.flags)) {
                let find = |e| self.variant(mesh.shader, property, lit.len(), e);
                let exact = format!("FastRT{}", lit.len());
                let Some(shader) = find(extra).or_else(|| find("")) else {
                    continue;
                };
                let s = &self.shaders[mesh.shader].shaders[shader];
                if !s.has(&exact) || (!extra.is_empty() && !s.has(extra)) {
                    let w = format!("{} {property} {exact} {extra}: using {}", mesh.name, s.name);
                    self.warnings.insert(w);
                }
                for pass in 0..self.shaders[mesh.shader].shaders[shader].passes.len() {
                    let p = &self.shaders[mesh.shader].shaders[shader].passes[pass];
                    let Some(stream) = mesh
                        .maps
                        .get(&p.stream_map)
                        .and_then(|s| mesh.streams.get(*s))
                    else {
                        self.warnings
                            .insert(format!("no stream map {:?}", p.stream_map));
                        continue;
                    };
                    let key = (mesh.shader, shader, pass, stream.layout);
                    let Some(id) = self.pipeline(gpu, key) else {
                        let w = format!("{}: no pipeline for pass {pass}", mesh.name);
                        self.warnings.insert(w);
                        continue;
                    };
                    let mut slots = [0usize; 40];
                    let mut overrides = BTreeMap::new();
                    let mut textures: Vec<Option<TextureId>> =
                        vec![None; self.shaders[mesh.shader].textures.len()];
                    let mut geometry = (0, 0);
                    for c in &mesh.commands[stream.commands] {
                        let (a, b) = (c[1] as usize, c[2] as usize);
                        match c[0] {
                            0 => {
                                let ctx = Context {
                                    view_proj: vp,
                                    camera: camera.position,
                                    time,
                                    lights: &lit,
                                    ambient: &ambient,
                                    bones: &bones,
                                    slots: &slots,
                                    params: &params,
                                    overrides: &overrides,
                                    center: model.center,
                                    velocity,
                                    orientation,
                                };
                                let g = (geometry.0, geometry.1, c[1], c[2] - c[1] + 1);
                                let tex = |k: usize| textures.get(k).copied().flatten();
                                self.emit(gpu, &key, id, &ctx, &by_light, &tex, g, out);
                            }
                            1 => {
                                if let Some(s) = slots.get_mut(a) {
                                    *s = b;
                                }
                            }
                            2 => {
                                if let Some(t) = textures.get_mut(a) {
                                    *t = model.textures.get(b).copied().flatten();
                                }
                            }
                            3 => {
                                let (name, values) = &mesh.params[a];
                                overrides.insert(name.clone(), values[b]);
                            }
                            4 => geometry = (stream.buffers[a], stream.index_buffers[a]),
                            n => {
                                self.warnings.insert(format!("unknown draw command {n}"));
                            }
                        }
                    }
                }
            }
        }
        self.lights = lights;
        self.instances = instances;
        self.models = models;
    }

    fn shadow_quad(&mut self, gpu: &Gpu, out: &mut Vec<DrawCall>) {
        let Some((file, v, i)) = self.quad else {
            return;
        };
        let layout = VertexLayout {
            stride: 12,
            elements: vec![("position".into(), 0, ElementType::Float3, 0)],
        };
        let Some(shader) = self.shaders[file]
            .shaders
            .iter()
            .position(|s| s.has("Normal"))
        else {
            return;
        };
        let layout = self.intern(layout);
        let key = (file, shader, 0, layout);
        let Some(id) = self.pipeline(gpu, key) else {
            return;
        };
        let none = BTreeMap::new();
        let ctx = Context {
            view_proj: Mat4::IDENTITY,
            camera: Vec3::ZERO,
            time: 0.0,
            lights: &[],
            ambient: &[],
            bones: &[],
            slots: &[],
            params: &none,
            overrides: &none,
            center: Vec3::ZERO,
            velocity: [0.0; 4],
            orientation: Mat4::IDENTITY,
        };
        self.emit(gpu, &key, id, &ctx, &[], &|_| None, (v, i, 0, 6), out);
    }
}

/// View volume for culling whole objects.
struct Frustum([Vec4; 6]);

impl Frustum {
    fn new(vp: Mat4) -> Self {
        let r = |i| vp.row(i);
        let planes = [
            r(3) + r(0),
            r(3) - r(0),
            r(3) + r(1),
            r(3) - r(1),
            r(2),
            r(3) - r(2),
        ];
        Self(planes.map(|p| p / p.xyz().length().max(1e-9)))
    }
    fn touches(&self, center: Vec3, radius: f32) -> bool {
        self.0.iter().all(|p| p.xyz().dot(center) + p.w >= -radius)
    }
}

/// Shader constants hold matrix rows, for `dp4` against column vectors.
fn rows(m: Mat4) -> Vec<[f32; 4]> {
    m.transpose().to_cols_array_2d().to_vec()
}

fn fill(out: &mut [[f32; 4]], consts: &Constants, file: &ShaderFile, ctx: &Context) {
    for (reg, c) in consts.iter() {
        let values = match c {
            Const::Literal(v) => vec![*v],
            Const::Builtin(what, _) => builtin(what, ctx).unwrap_or_default(),
            Const::Var(name) => variable(name, file, ctx),
        };
        for (i, v) in values.into_iter().enumerate() {
            if let Some(o) = out.get_mut(reg as usize + i) {
                *o = v;
            }
        }
    }
}

fn variable(name: &str, file: &ShaderFile, ctx: &Context) -> Vec<[f32; 4]> {
    let Some(var) = file.vars.get(name) else {
        return Vec::new();
    };
    let spread = |v: [f32; 4]| match var.kind {
        // `Float` constants are read through any component.
        VarKind::Float => [v[0]; 4],
        _ => v,
    };
    if let Some(v) = ctx.params.get(name).or_else(|| ctx.overrides.get(name)) {
        return vec![spread(*v)];
    }
    if let Some(source) = &var.source {
        let key = source.split(',').next().unwrap_or("").trim();
        if let Some(v) = ctx.params.get(key) {
            return vec![spread(*v)];
        }
        if let Some(v) = builtin(key, ctx) {
            return v;
        }
    }
    let n = if var.kind == VarKind::Matrix { 4 } else { 1 };
    let value =
        |r: usize| std::array::from_fn(|i| var.default.get(r * 4 + i).copied().unwrap_or(0.0));
    (0..n).map(|r| spread(value(r))).collect()
}

/// Engine-supplied values (0x4265f0, 0x43db70).
fn builtin(name: &str, ctx: &Context) -> Option<Vec<[f32; 4]>> {
    let nth = |prefix: &str| -> Option<usize> {
        name.strip_prefix(prefix)?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)
    };
    if let Some(n) = nth("FastRTPosMatrix") {
        return Some(
            ctx.lights
                .get(n)
                .map_or(vec![[0.0; 4]; 4], |l| l.matrix.to_vec()),
        );
    }
    if let Some(n) = nth("FastRTColor") {
        return Some(vec![
            ctx.lights
                .get(n)
                .map_or([0.0; 4], |l| l.fast_rt_color(ctx.time)),
        ]);
    }
    if let Some(n) = nth("ObjectAmbientPos") {
        let p = ctx.ambient.get(n).map_or(Vec3::ZERO, |l| l.position());
        return Some(vec![p.extend(0.0).to_array()]);
    }
    if let Some(n) = nth("ObjectAmbientColor") {
        let c = ctx.ambient.get(n).map_or(Vec3::ZERO, |l| l.ambient_color);
        return Some(vec![c.extend(0.0).to_array()]);
    }
    if let Some(n) = name
        .strip_prefix("world")
        .and_then(|n| n.strip_suffix("mat"))
    {
        let slot: usize = n.parse().ok()?;
        let bone = *ctx.slots.get(slot)?;
        return Some(rows(ctx.bones.get(bone).copied().unwrap_or(Mat4::IDENTITY)));
    }
    Some(match name {
        // Objects and the scene sit at the origin; bones carry all motion.
        "WVP" | "VP" => rows(ctx.view_proj),
        "W" => rows(Mat4::IDENTITY),
        "CameraPosition" => vec![ctx.camera.extend(1.0).to_array()],
        "SOLObjectCenter" => vec![ctx.center.extend(1.0).to_array()],
        "SOLVelocityX" | "SOLVelocity" => vec![ctx.velocity],
        "SOLOrientationX" | "SOLOrientation" => rows(ctx.orientation),
        "Time" => vec![[ctx.time; 4]],
        _ => return None,
    })
}
