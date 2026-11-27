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

use crate::camera::{self, Camera};
use crate::gpu::Gpu;
use crate::light::Light;
use crate::object::Clip;
use crate::overlay::Mark;
use crate::pipeline::*;
use crate::script::{self, Command};
use crate::targets::Targets;

pub type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

/// The `script` lines of `Sushi2.ini`: file, menu text, camera path.
const CHAPTERS: [(&str, &str, &str); 5] = [
    ("Normal", "Normal", "CarRoom"),
    ("NoNormalMap", "Normal maps", "NormalMap"),
    ("NormalMap", "8 vs 16 bit normal map", "NormalMap8vs16"),
    ("Paint", "Paint build up", "Paint"),
    ("MultiColor", "Multi-color", "CarRoom"),
];

/// Mesh record flags (+0), matching the words in the scripts.
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
}

struct SceneItem {
    draw: usize,
    vertices: usize,
    indices: usize,
    start: u32,
    count: u32,
}

/// One camera path and what is drawn along it.
pub struct Chapter {
    pub title: &'static str,
    path: FlyPath,
    script: Vec<Command>,
}

impl Chapter {
    pub fn duration(&self) -> f32 {
        self.path.frames.len() as f32 * self.path.step
    }
}

struct Mirror {
    plane: Vec4,
    matrix: Mat4,
    /// The scene as seen in the mirror, prepared by the original's compiler.
    items: Vec<SceneItem>,
}

/// A buffer that scripts draw to and shaders read.
struct Buffer {
    targets: Targets,
    texture: TextureId,
}

/// Where a draw lands: target, viewport and scissor as fractions.
#[derive(Clone, Copy)]
struct Place {
    target: Option<usize>,
    viewport: [f32; 4],
    scissor: [f32; 4],
}

struct Step {
    target: Option<usize>,
    color: Option<wgpu::Color>,
    depth: bool,
    calls: Vec<(Place, std::ops::Range<usize>)>,
}

/// How one pass over the world is seen.
#[derive(Clone, Copy)]
struct Eye<'a> {
    camera: &'a Camera,
    time: f32,
    view_proj: Mat4,
    mirror: Option<Mat4>,
    target: Option<usize>,
}

struct SceneDraw {
    transparent: bool,
    shader: usize,
    layout: LayoutId,
    lights: Vec<usize>,
    textures: Vec<Option<TextureId>>,
}

/// Values the engine feeds to shaders for one draw.
struct Context<'a> {
    view_proj: Mat4,
    view: Mat4,
    /// Set while drawing what a mirror shows.
    mirror: Option<Mat4>,
    target: Option<usize>,
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
/// Shader file, shader, pass, vertex layout, whether winding is reversed.
type PipelineKey = (usize, usize, usize, LayoutId, bool);

pub struct Demo {
    chapters: Vec<Chapter>,
    buffer_ids: HashMap<String, usize>,
    targets: Vec<Buffer>,
    mirrors: Vec<Mirror>,
    font: Vec<sushi_formats::texture::Level>,
    textures: Textures,
    pipelines: Pipelines,
    arena: Arena,
    buffers: Vec<wgpu::Buffer>,
    shaders: Vec<ShaderFile>,
    shader_ids: HashMap<String, usize>,
    shader_names: Vec<String>,
    layouts: Vec<VertexLayout>,
    cache: HashMap<PipelineKey, Option<PipelineId>>,
    lights: Vec<Light>,
    light_textures: Vec<TextureId>,
    models: Vec<Model>,
    instances: Vec<Instance>,
    scene_draws: Vec<SceneDraw>,
    scene_items: Vec<SceneItem>,
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
        let scene = scene::read_scene(file("Scenes/CarRoom.ati")?)?;
        let mut chapters = Vec::new();
        for (name, title, fly) in CHAPTERS {
            let script = script::parse(&text(&format!("Scripts/{name}.sds"))?);
            chapters.push(Chapter {
                title,
                path: path::read_path(&text(&format!("FlyPaths/{fly}.pth"))?)?,
                script: script.map_err(|e| format!("{name}.sds: {e}"))?,
            });
        }
        let mut demo = Self {
            chapters,
            buffer_ids: HashMap::new(),
            targets: Vec::new(),
            mirrors: Vec::new(),
            font: Vec::new(),
            textures: Textures::new(gpu),
            pipelines: Pipelines::default(),
            arena: Arena::new(gpu),
            buffers: Vec::new(),
            shaders: Vec::new(),
            shader_ids: HashMap::new(),
            shader_names: Vec::new(),
            layouts: Vec::new(),
            cache: HashMap::new(),
            lights: scene.lights.iter().map(Light::new).collect(),
            light_textures: Vec::new(),
            models: Vec::new(),
            instances: Vec::new(),
            scene_draws: Vec::new(),
            scene_items: Vec::new(),
            warnings: Default::default(),
            draw_calls: 0,
            chip_logo: Vec::new(),
        };
        demo.create_buffers(gpu);
        demo.load_scene(gpu, &archive, &scene)?;
        demo.load_objects(gpu, &archive, &text("Objects/CarRoom.ini")?)?;
        demo.prewarm(gpu);
        let globals = scene::read_scene(file("Globals.ati")?)?;
        // Texture 3 of Globals.ati is ChipLogo.tga (0x421c50).
        if let Some(t) = globals
            .textures
            .get(3)
            .filter(|t| t.name.contains("ChipLogo"))
        {
            demo.chip_logo = upright(t)?;
        }
        if let Some(t) = globals.textures.first().filter(|t| t.name == "Text.tga") {
            demo.font = upright(t)?;
        }
        Ok(demo)
    }

    /// Buffers that any script creates are there for all of them.
    fn create_buffers(&mut self, gpu: &Gpu) {
        for c in self.chapters.iter().flat_map(|c| &c.script) {
            let Command::CreateColor {
                name,
                width,
                height,
            } = c
            else {
                continue;
            };
            let targets = Targets::new(gpu, *width, *height);
            let texture = self.textures.add_view(targets.resolved_view.clone());
            self.buffer_ids.insert(name.clone(), self.targets.len());
            self.targets.push(Buffer { targets, texture });
        }
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    /// The font: 8 columns and 16 rows of characters.
    pub fn font(&self) -> &[sushi_formats::texture::Level] {
        &self.font
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
        self.shader_names.push(key.clone());
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
                transparent: g.transparent(),
                shader,
                layout,
                lights: d.lights(),
                textures: (0..n)
                    .map(|k| d.texture(k).and_then(|t| textures[t]))
                    .collect(),
            });
        }
        let mut made = HashMap::new();
        self.scene_items = self.scene_list(gpu, s, &s.groups, &mut made)?;
        for m in &s.mirrors {
            // A mirror whose picture goes to a buffer: the demo has no other kind.
            if m.raw[6] != 0 || m.raw[5] != 0 {
                return Err("unsupported mirror".into());
            }
            let items = self.scene_list(gpu, s, &m.sets[1].groups, &mut made)?;
            self.mirrors.push(Mirror {
                plane: Vec4::from_array(m.plane()),
                matrix: Mat4::from_cols_array(&m.matrix()),
                items,
            });
        }
        Ok(())
    }

    fn scene_list(
        &mut self,
        gpu: &Gpu,
        s: &scene::Scene,
        groups: &[scene::Group],
        made: &mut HashMap<(usize, usize), (usize, usize)>,
    ) -> Result<Vec<SceneItem>> {
        let mut out: Vec<SceneItem> = Vec::new();
        for item in groups.iter().flat_map(|g| g.draw_items()) {
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
            out.push(SceneItem {
                draw: item.draw,
                vertices,
                indices,
                start: item.start,
                count: item.count,
            });
        }
        // Merge neighboring index ranges of the same draw.
        out.sort_by_key(|i| (i.draw, i.vertices, i.start));
        out.dedup_by(|b, a| {
            let joins = (a.draw, a.vertices, a.start + a.count) == (b.draw, b.vertices, b.start);
            if joins {
                a.count += b.count;
            }
            joins
        });
        Ok(out)
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
            if l[0] != "Global" {
                return Err(format!("unknown object kind {}", l[0]).into());
            }
            let model = match models.get(l[2]) {
                Some(m) => *m,
                None => {
                    let m = self.model(gpu, archive, &objects[l[2]])?;
                    models.insert(l[2].to_string(), m);
                    m
                }
            };
            let clip = Clip::new(&anim::read_animation(get(l[3])?)?);
            self.instances.push(Instance { model, clip });
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

    pub fn chip_logo(&self) -> &[sushi_formats::texture::Level] {
        &self.chip_logo
    }

    /// Compiles every pipeline the show can need, so playback does not stall.
    fn prewarm(&mut self, gpu: &Gpu) {
        let mut keys: std::collections::HashSet<PipelineKey> = Default::default();
        let mut used: std::collections::BTreeSet<String> = ["Reflections".to_string()].into();
        for c in self.chapters.iter().flat_map(|c| &c.script) {
            if let Command::Objects { property, .. }
            | Command::Scene { property, .. }
            | Command::Reflections(property) = c
            {
                used.insert(property.clone());
            }
        }
        let wanted = |s: &sushi_shader::Shader| used.iter().any(|u| s.has(u));
        for d in &self.scene_draws {
            for (i, s) in self.shaders[d.shader].shaders.iter().enumerate() {
                if wanted(s) {
                    keys.extend((0..s.passes.len()).map(|p| (d.shader, i, p, d.layout, false)));
                }
            }
        }
        for m in self
            .models
            .iter()
            .flat_map(|m| &m.meshes)
            .filter(|m| m.flags & HIDDEN == 0)
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
                        keys.extend([false, true].map(|r| (m.shader, i, p, stream.layout, r)));
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
        let mut pass = targets.begin(enc, Some(wgpu::Color::BLACK), true);
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
    fn variant(&self, file: usize, property: &str, lights: usize, mirrored: bool) -> Option<usize> {
        // What a mirror shows prefers its own shaders (0x413f50).
        if mirrored && let Some(s) = self.lit_variant(file, "Reflections", lights) {
            return Some(s);
        }
        self.lit_variant(file, property, lights)
    }

    fn lit_variant(&self, file: usize, property: &str, lights: usize) -> Option<usize> {
        (0..=lights).rev().find_map(|n| {
            let rt = format!("FastRT{n}");
            self.shaders[file]
                .shaders
                .iter()
                .position(|s| s.has(property) && s.has(&rt))
        })
    }

    fn pipeline(&mut self, gpu: &Gpu, key: PipelineKey) -> Option<PipelineId> {
        if let Some(p) = self.cache.get(&key) {
            return *p;
        }
        let (file, shader, pass, layout, mirrored) = key;
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
            Ok(m) => Some(self.pipelines.create(gpu, &s.name, p, m, layout, mirrored)),
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
        // A buffer cannot be read while it is drawn to; the original's result is undefined.
        let reads_target = pass.samplers.values().any(|s| {
            let t = file.textures.iter().find(|t| t.name == s.texture);
            let t = t.and_then(|t| self.buffer_ids.get(t.target.as_ref()?));
            t.is_some() && t.copied() == ctx.target
        });
        if reads_target {
            return;
        }
        let textures: Vec<TextureId> = module
            .samplers
            .keys()
            .map(|stage| {
                let name = &pass.samplers.get(stage)?.texture;
                let k = file.textures.iter().position(|t| t.name == *name)?;
                if let Some(buffer) = &file.textures[k].target {
                    return Some(self.targets[*self.buffer_ids.get(buffer)?].texture);
                }
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

    /// Runs a chapter's frame script from its own camera, or from another. Returns
    /// the lines and text to draw on top.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        chapter: usize,
        time: f32,
        camera: Option<Camera>,
    ) -> Vec<Mark> {
        self.arena.clear();
        let (w, h) = targets.size();
        let chapter = chapter.min(self.chapters.len() - 1);
        let camera = camera.unwrap_or_else(|| Camera::on_path(&self.chapters[chapter].path, time));
        let projection = Camera::projection(w as f32 / h as f32);
        let eye = Eye {
            camera: &camera,
            time,
            view_proj: projection * camera.view(),
            mirror: None,
            target: None,
        };
        let visible = self.visible(time, camera.position);
        let script = std::mem::take(&mut self.chapters[chapter].script);
        let mut calls = Vec::new();
        let mut steps: Vec<Step> = Vec::new();
        let mut marks = Vec::new();
        let full = [0.0, 0.0, 1.0, 1.0];
        let mut place = Place {
            target: None,
            viewport: full,
            scissor: full,
        };
        let (mut text_size, mut text_color) = (0.03, [1.0; 4]);
        for c in &script {
            let first = calls.len();
            match c {
                Command::CreateColor { .. } => {}
                Command::Target(name) => {
                    place.target = name.as_ref().and_then(|n| self.buffer_ids.get(n).copied());
                    if name.is_some() != place.target.is_some() {
                        self.warnings.insert(format!("no buffer {name:?}"));
                    }
                    let seen = steps.iter().any(|s| s.target == place.target);
                    steps.push(Step {
                        target: place.target,
                        color: (!seen).then_some(wgpu::Color::BLACK),
                        depth: !seen,
                        calls: Vec::new(),
                    });
                    place.viewport = full;
                }
                Command::Viewport(v) => place.viewport = *v,
                Command::Scissor(v) => place.scissor = *v,
                Command::Clear { depth, color } => {
                    let step = steps.last_mut().expect("scripts begin with a target");
                    // Clears come first, except of halves already cleared whole.
                    if step.calls.is_empty() {
                        step.depth |= depth;
                        step.color = color
                            .map(|c| c.map(f64::from))
                            .map(|[r, g, b, a]| wgpu::Color { r, g, b, a })
                            .or(step.color);
                    }
                }
                Command::Objects {
                    property,
                    opaque,
                    transparent,
                    receive,
                    other,
                } => {
                    let want = |f: u32| {
                        f & HIDDEN == 0
                            && (if f & TRANSPARENT != 0 {
                                *transparent
                            } else {
                                *opaque
                            })
                            && (if f & RECEIVES != 0 { *receive } else { *other })
                    };
                    self.objects(gpu, &eye, &visible, property, &want, &mut calls);
                }
                Command::Scene {
                    property,
                    opaque,
                    transparent,
                } => {
                    let items = std::mem::take(&mut self.scene_items);
                    self.scene(
                        gpu,
                        &eye,
                        property,
                        (*opaque, *transparent),
                        &items,
                        &mut calls,
                    );
                    self.scene_items = items;
                }
                Command::Reflections(property) => {
                    let step = steps.last_mut().expect("scripts begin with a target");
                    if step.calls.is_empty() {
                        (step.depth, step.color) = (true, Some(wgpu::Color::TRANSPARENT));
                    }
                    self.reflections(
                        gpu,
                        &eye,
                        projection,
                        place.target,
                        &visible,
                        property,
                        &mut calls,
                    );
                }
                Command::Outline(color) => marks.push(Mark::Outline {
                    viewport: place.viewport,
                    color: *color,
                }),
                Command::TextSize(v) => text_size = *v,
                Command::TextColor(v) => text_color = *v,
                Command::Text {
                    x,
                    y,
                    text,
                    background,
                } => marks.push(Mark::Text {
                    x: *x,
                    y: *y,
                    size: text_size,
                    color: text_color,
                    text: text.clone(),
                    background: *background,
                }),
            }
            if calls.len() > first {
                let step = steps.last_mut().expect("scripts begin with a target");
                step.calls.push((place, first..calls.len()));
            }
        }
        self.chapters[chapter].script = script;
        self.draw_calls = calls.len();
        self.arena.upload(gpu);
        self.pipelines.bind_uniforms(gpu, &self.arena);
        for step in &steps {
            let to = step.target.map_or(targets, |t| &self.targets[t].targets);
            let (w, h) = to.size();
            let pixels = |r: [f32; 4]| {
                let (x0, x1) = (r[0] * w as f32, r[2] * w as f32);
                let (y0, y1) = ((1.0 - r[3]) * h as f32, (1.0 - r[1]) * h as f32);
                [x0, y0, x1, y1].map(f32::round)
            };
            let mut pass = to.begin(enc, step.color, step.depth);
            for (place, range) in &step.calls {
                let [x0, y0, x1, y1] = pixels(place.viewport);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                pass.set_viewport(x0, y0, x1 - x0, y1 - y0, 0.0, 1.0);
                // D3D limits drawing to the viewport as well.
                let [a0, b0, a1, b1] = pixels(place.scissor);
                let (a0, b0) = (a0.max(x0) as u32, b0.max(y0) as u32);
                let (a1, b1) = (a1.min(x1) as u32, b1.min(y1) as u32);
                if a1 <= a0 || b1 <= b0 {
                    continue;
                }
                pass.set_scissor_rect(a0, b0, a1 - a0, b1 - b0);
                for c in &calls[range.clone()] {
                    self.pipelines.record(&mut pass, c, &self.buffers);
                }
            }
            // In WebGL the scissor also limits the resolve at the end of the pass.
            pass.set_scissor_rect(0, 0, w, h);
        }
        marks
    }

    /// What the mirrors show (0x415610): the prepared scene, then the objects
    /// reflected and cut off at the mirror.
    #[allow(clippy::too_many_arguments)]
    fn reflections(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        projection: Mat4,
        target: Option<usize>,
        visible: &[usize],
        property: &str,
        out: &mut Vec<DrawCall>,
    ) {
        let mirrors = std::mem::take(&mut self.mirrors);
        for m in &mirrors {
            // Scene and objects share the projection that cuts at the mirror, so that
            // their depths agree. The original clips only the objects.
            let reflected = Eye {
                view_proj: camera::clipped(projection, eye.camera.view(), -m.plane),
                mirror: Some(m.matrix),
                target,
                ..*eye
            };
            // The scene's mirror image needs no reflection, only its shaders.
            let shown = Eye {
                mirror: Some(Mat4::IDENTITY),
                ..reflected
            };
            self.scene(gpu, &shown, property, (true, true), &m.items, out);
            let want = |f: u32| f & HIDDEN == 0;
            self.objects(gpu, &reflected, visible, property, &want, out);
        }
        self.mirrors = mirrors;
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

    fn scene(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        property: &str,
        (opaque, transparent): (bool, bool),
        items: &[SceneItem],
        out: &mut Vec<DrawCall>,
    ) {
        let none = BTreeMap::new();
        let lights = std::mem::take(&mut self.lights);
        let draws = std::mem::take(&mut self.scene_draws);
        // Items are grouped by draw, which all share constants and textures.
        for blended in [false, true] {
            if !(if blended { transparent } else { opaque }) {
                continue;
            }
            for items in items.chunk_by(|a, b| a.draw == b.draw) {
                let d = &draws[items[0].draw];
                if d.transparent != blended {
                    continue;
                }
                let lit: Vec<&Light> = d.lights.iter().map(|l| &lights[*l]).collect();
                let variant = self.variant(d.shader, property, lit.len(), eye.mirror.is_some());
                let Some(shader) = variant else {
                    // A mirror shows only what its shaders name; the original skips the
                    // rest as well (0x413f50).
                    if eye.mirror.is_none() {
                        let name = &self.shader_names[d.shader];
                        self.warnings.insert(format!(
                            "{name}: no {property} shader for {} lights",
                            lit.len()
                        ));
                    }
                    continue;
                };
                let ctx = Context {
                    view_proj: eye.view_proj,
                    view: eye.camera.view(),
                    mirror: None,
                    target: eye.target,
                    camera: eye.camera.position,
                    time: eye.time,
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
                    let key = (d.shader, shader, pass, d.layout, false);
                    let Some(id) = self.pipeline(gpu, key) else {
                        continue;
                    };
                    let tex = |k: usize| d.textures.get(k).copied().flatten();
                    let geometry = |i: &SceneItem| (i.vertices, i.indices, i.start, i.count);
                    let first = out.len();
                    let g = geometry(&items[0]);
                    self.emit(gpu, &key, id, &ctx, &by_light, &tex, g, out);
                    if out.len() == first {
                        continue;
                    }
                    for i in &items[1..] {
                        let mut call = out[first].clone();
                        (call.vertices, call.indices, call.start, call.count) = geometry(i);
                        out.push(call);
                    }
                }
            }
        }
        self.lights = lights;
        self.scene_draws = draws;
    }

    fn objects(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        visible: &[usize],
        property: &str,
        want: &dyn Fn(u32) -> bool,
        out: &mut Vec<DrawCall>,
    ) {
        let time = eye.time;
        let mirrored = eye.mirror.is_some();
        let lights = std::mem::take(&mut self.lights);
        let instances = std::mem::take(&mut self.instances);
        let models = std::mem::take(&mut self.models);
        let frustum = Frustum::new(eye.view_proj);
        for &k in visible {
            let inst = &instances[k];
            let model = &models[inst.model];
            if !model.meshes.iter().any(|m| want(m.flags)) {
                continue;
            }
            let Some((_, _, center, radius)) = inst.clip.bounds(time) else {
                continue;
            };
            let radius = radius * model.expand;
            let seen_at = eye.mirror.map_or(center, |m| m.transform_point3(center));
            if !frustum.touches(seen_at, radius) {
                continue;
            }
            let lit = lights.iter().enumerate().filter(|(_, l)| {
                l.fast_rt && l.reach(center, radius) > 0.0 && l.color(time) != Vec3::ZERO
            });
            let chosen: Vec<usize> = lit.map(|(i, _)| i).take(4).collect();
            let ambient: Vec<&Light> = lights
                .iter()
                .filter(|l| l.object_ambient && l.reach(center, radius) > 0.0)
                .take(4)
                .collect();
            let mut bones = inst.clip.bones(time);
            if let Some(m) = eye.mirror {
                bones.iter_mut().for_each(|b| *b = m * *b);
            }
            let params = inst.clip.params(time);
            let (velocity, orientation) = inst.clip.motion(model.center, time);
            let lit: Vec<&Light> = chosen.iter().map(|i| &lights[*i]).collect();
            let by_light: Vec<TextureId> = chosen.iter().map(|i| self.light_textures[*i]).collect();
            for mesh in model.meshes.iter().filter(|m| want(m.flags)) {
                // Meshes take part only in the passes their shaders name.
                let Some(shader) = self.variant(mesh.shader, property, lit.len(), mirrored) else {
                    continue;
                };
                let exact = format!("FastRT{}", lit.len());
                let s = &self.shaders[mesh.shader].shaders[shader];
                if !s.has(&exact) {
                    let w = format!("{} {property} {exact}: using {}", mesh.name, s.name);
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
                    let key = (mesh.shader, shader, pass, stream.layout, mirrored);
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
                                    view_proj: eye.view_proj,
                                    view: eye.camera.view(),
                                    mirror: eye.mirror,
                                    target: eye.target,
                                    camera: eye.camera.position,
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
}

/// Decodes an image stored bottom row first, as the original draws it.
fn upright(t: &sushi_formats::texture::Texture) -> Result<Vec<sushi_formats::texture::Level>> {
    let mut mips = t.decode()?.swap_remove(0);
    for l in &mut mips {
        let rows = l.rgba.chunks_exact(l.width * 4).rev().flatten().copied();
        l.rgba = rows.collect();
    }
    Ok(mips)
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
        let Some(l) = ctx.lights.get(n) else {
            return Some(vec![[0.0; 4]; 4]);
        };
        // A mirror moves every row as a point (0x4265f0), keeping its scale.
        let row = |r: &[f32; 4]| match ctx.mirror {
            Some(m) => {
                let p = m.transform_point3(Vec3::new(r[0], r[1], r[2]));
                [p.x, p.y, p.z, r[3]]
            }
            None => *r,
        };
        return Some(l.matrix.iter().map(row).collect());
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
        let p = ctx.mirror.map_or(p, |m| m.transform_point3(p));
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
        "View" => rows(ctx.view),
        // Clip space to texture coordinates times w (0x4265f0, case 0x2a).
        "ScreenProjectionMatrix" => {
            let [x, y, _, w] = ctx.view_proj.transpose().to_cols_array_2d();
            let mix = |a: [f32; 4], s: f32| std::array::from_fn(|i| 0.5 * (s * a[i] + w[i]));
            vec![mix(x, 1.0), mix(y, -1.0), w, [0.0; 4]]
        }
        "CameraPosition" => vec![ctx.camera.extend(1.0).to_array()],
        "SOLObjectCenter" => vec![ctx.center.extend(1.0).to_array()],
        "SOLVelocityX" | "SOLVelocity" => vec![ctx.velocity],
        "SOLOrientationX" | "SOLOrientation" => rows(ctx.orientation),
        "Time" => vec![[ctx.time; 4]],
        _ => return None,
    })
}
