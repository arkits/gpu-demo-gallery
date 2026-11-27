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

use crate::camera::{self, Camera, Lens};
use crate::gpu::Gpu;
use crate::light::Light;
use crate::object::{Clip, Times};
use crate::overlay::Mark;
use crate::pipeline::*;
use crate::script::{self, Command, Surface};
use crate::settings::Settings;
use crate::targets::{self, Clears, ColorBuffer, DepthBuffer, Targets};

pub type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

/// Mesh record flags (+0), matching the words in the scripts.
const CASTS: u32 = 1;
const RECEIVES: u32 = 2;
const TRANSPARENT: u32 = 8;
/// Helper geometry that only casts shadows (0x432160).
const HIDDEN: u32 = 0x10;

struct Stream {
    layout: StreamLayout,
    /// Which of the neighboring frames it holds, if the mesh changes shape.
    frame: Option<usize>,
    /// By frame, then as draw commands number them.
    buffers: Vec<Vec<usize>>,
    index_buffers: Vec<usize>,
    commands: usize,
}

struct Mesh {
    name: String,
    shader: usize,
    flags: u32,
    streams: Vec<Stream>,
    /// Streams bound together, by stream map.
    maps: BTreeMap<String, (Vec<usize>, LayoutId)>,
    commands: Vec<Vec<[u32; 4]>>,
    params: Vec<(String, Vec<[f32; 4]>)>,
    times: Times,
    /// Meshes of a group take turns, so that layers of fur stack: name and layers.
    group: Option<(String, usize)>,
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

/// One camera path and what is drawn along it.
pub struct Chapter {
    pub title: String,
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
    size: Option<[u32; 2]>,
    deep: bool,
    color: ColorBuffer,
    texture: TextureId,
}

struct Depth {
    size: Option<[u32; 2]>,
    buffer: DepthBuffer,
}

/// Where draws land; `None` in the outer options is nowhere, in the inner ones the back
/// buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Destination {
    colors: [Option<Option<usize>>; 4],
    depth: Option<Option<usize>>,
}

const BACK: Destination = Destination {
    colors: [Some(None), None, None, None],
    depth: Some(None),
};

/// Viewport and scissor as fractions.
#[derive(Debug, Clone, Copy)]
struct Place {
    viewport: [f32; 4],
    scissor: [f32; 4],
}

const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

struct Step {
    to: Destination,
    clears: Clears,
    calls: Vec<(Place, std::ops::Range<usize>)>,
}

/// How one pass over the world is seen.
#[derive(Clone, Copy)]
struct Eye<'a> {
    camera: &'a Camera,
    time: f32,
    view_proj: Mat4,
    mirror: Option<Mat4>,
    to: Destination,
    formats: Formats,
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
    to: Destination,
    /// Width over height of the window.
    window: f32,
    camera: Vec3,
    time: f32,
    lights: &'a [&'a Light],
    ambient: &'a [&'a Light],
    bones: &'a [Mat4],
    slots: &'a [usize],
    /// Where between two shapes the mesh is.
    tween: f32,
    params: &'a BTreeMap<String, [f32; 4]>,
    overrides: &'a BTreeMap<String, [f32; 4]>,
    center: Vec3,
    velocity: [f32; 4],
    orientation: Mat4,
}

type LayoutId = usize;
/// Shader file, shader, pass, vertex layout, whether winding is reversed, targets.
type PipelineKey = (usize, usize, usize, LayoutId, bool, Formats);

pub struct Demo {
    chapters: Vec<Chapter>,
    buffer_ids: HashMap<String, usize>,
    targets: Vec<Buffer>,
    depth_ids: HashMap<String, usize>,
    depths: Vec<Depth>,
    /// Size of the back buffer that the buffers were made for.
    window: (u32, u32),
    lens: Lens,
    pub settings: Settings,
    /// Draw records by the names that scripts use for rectangles.
    quads: HashMap<String, usize>,
    quad_buffers: HashMap<[u32; 8], usize>,
    quad_indices: usize,
    quad_layout: LayoutId,
    clears: HashMap<(bool, bool, Option<u32>, Formats), PipelineId>,
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

fn layout(header: &ShaderHeader, stream: usize) -> Result<StreamLayout> {
    let s = header.streams.get(stream).ok_or("missing vertex stream")?;
    let mut offset = 0;
    let mut elements = Vec::new();
    for e in &s.elements {
        let (name, index) = e.semantic();
        elements.push((name.to_string(), index, element_type(e.kind)?, offset));
        offset += e.size() as u32;
    }
    Ok(StreamLayout {
        stride: offset,
        elements,
    })
}

/// Position and texture coordinates of a rectangle's corners.
fn quad_stream() -> StreamLayout {
    StreamLayout {
        stride: 20,
        elements: vec![
            ("position".into(), 0, ElementType::Float3, 0),
            ("texcoord".into(), 0, ElementType::Float2, 12),
        ],
    }
}

/// Maps fractions of the viewport to clip space.
fn quad_matrix() -> Mat4 {
    Mat4::from_translation(Vec3::new(-1.0, -1.0, 0.0)) * Mat4::from_scale(Vec3::new(2.0, 2.0, 1.0))
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
        let settings = Settings::parse(&text("Sushi2.ini")?)?;
        let scene = scene::read_scene(file(&format!("Scenes/{}.ati", settings.scene))?)?;
        let mut chapters = Vec::new();
        for c in &settings.chapters {
            let script = script::parse(&text(&format!("Scripts/{}", c.script))?);
            chapters.push(Chapter {
                title: c.title.clone(),
                path: path::read_path(&text(&format!("FlyPaths/{}", c.path))?)?,
                script: script.map_err(|e| format!("{}: {e}", c.script))?,
            });
        }
        let lens = Lens {
            near: settings.near,
            far: settings.far,
            fov: settings.fov,
        };
        let mut demo = Self {
            chapters,
            buffer_ids: HashMap::new(),
            targets: Vec::new(),
            depth_ids: HashMap::new(),
            depths: Vec::new(),
            window: (0, 0),
            lens,
            settings,
            quads: HashMap::new(),
            quad_buffers: HashMap::new(),
            quad_indices: 0,
            quad_layout: 0,
            clears: HashMap::new(),
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
        demo.quad_indices = demo.index_buffer(gpu, &[0, 2, 1, 2, 0, 3]);
        demo.quad_layout = demo.intern(VertexLayout(vec![quad_stream()]));
        demo.create_buffers(gpu);
        demo.load_scene(gpu, &archive, &scene)?;
        // Some demos have only a scene.
        if let Ok(ini) = text(&format!("Objects/{}.ini", demo.settings.scene)) {
            demo.load_objects(gpu, &archive, &ini)?;
        }
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

    /// Buffers that any script creates are there for all of them. Their sizes are set by
    /// `fit`, before the first frame.
    fn create_buffers(&mut self, gpu: &Gpu) {
        for c in self.chapters.iter().flat_map(|c| &c.script) {
            match c {
                Command::CreateColor { name, size, deep } => {
                    if self.buffer_ids.contains_key(name) {
                        continue;
                    }
                    let color = ColorBuffer::new(gpu, (1, 1), *deep);
                    let texture = self.textures.add_view(color.resolved_view.clone());
                    self.buffer_ids.insert(name.clone(), self.targets.len());
                    self.targets.push(Buffer {
                        size: *size,
                        deep: *deep,
                        color,
                        texture,
                    });
                }
                Command::CreateDepth { name, size } => {
                    if self.depth_ids.contains_key(name) {
                        continue;
                    }
                    self.depth_ids.insert(name.clone(), self.depths.len());
                    self.depths.push(Depth {
                        size: *size,
                        buffer: DepthBuffer::new(gpu, (1, 1)),
                    });
                }
                _ => {}
            }
        }
    }

    /// Makes the buffers fit a window. Those with a size, chosen for 1024 by 768, get the
    /// window's shape and at least its height if they were meant to have that.
    fn fit(&mut self, gpu: &Gpu, window: (u32, u32)) {
        if self.window == window {
            return;
        }
        self.window = window;
        let max = gpu.device.limits().max_texture_dimension_2d;
        let fit = |size: Option<[u32; 2]>| {
            let Some([w, h]) = size else {
                return window;
            };
            if h < 768 {
                return (w, h);
            }
            let h = h.max(window.1).min(max);
            let w = u64::from(h) * u64::from(window.0) / u64::from(window.1);
            ((w as u32).clamp(1, max), h)
        };
        for b in &mut self.targets {
            b.color = ColorBuffer::new(gpu, fit(b.size), b.deep);
            let view = b.color.resolved_view.clone();
            self.textures.replace_view(b.texture, view);
        }
        for d in &mut self.depths {
            d.buffer = DepthBuffer::new(gpu, fit(d.size));
        }
        self.pipelines.forget_textures();
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
            let stream = layout(&sh.header, 0);
            let layout = self.intern(VertexLayout(stream.into_iter().collect()));
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
        for q in &s.table_112 {
            self.quads.insert(
                cstr(&q[..0x20]).to_ascii_lowercase(),
                u32_at(q, 0x20) as usize,
            );
        }
        let mut made = HashMap::new();
        self.scene_items = self.scene_list(gpu, s, &s.groups, &mut made)?;
        for m in &s.mirrors {
            // All of these demos' mirrors draw to a buffer. Those limited to their outline
            // (+5) need no limit here: the rest of the buffer is not seen.
            if m.raw[6] != 0 {
                return Err("unsupported mirror".into());
            }
            // The surface itself is drawn with the scene (the end of 0x415610).
            let surface = self.scene_list(gpu, s, &m.sets[0].groups, &mut made)?;
            self.scene_items.splice(0..0, surface);
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
            let layout = &self.layouts[self.scene_draws[item.draw].layout];
            if layout.0.first().is_none_or(|l| l.stride != st.stride()) {
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
            let casts_shadow = match l[0] {
                "SGlobal" => true,
                "Global" => false,
                k => return Err(format!("unknown object kind {k}").into()),
            };
            let animation = anim::read_animation(get(l[3])?)?;
            // Meshes that change shape come with the animation.
            let morphs = &animation.morph_meshes;
            let model = match models.get(l[2]).filter(|_| morphs.is_empty()) {
                Some(m) => *m,
                None => {
                    let m = self.model(gpu, archive, &objects[l[2]], morphs)?;
                    models.insert(l[2].to_string(), m);
                    m
                }
            };
            let clip = Clip::new(&animation);
            self.instances.push(Instance {
                model,
                clip,
                casts_shadow,
            });
        }
        Ok(())
    }

    fn model(
        &mut self,
        gpu: &Gpu,
        archive: &Archive,
        o: &object::Object,
        morphs: &[sushi_formats::mesh::Mesh],
    ) -> Result<usize> {
        let textures = o.textures.iter().map(|t| self.textures.find(t)).collect();
        let mut meshes = Vec::new();
        for m in o.meshes.iter().chain(morphs) {
            let shader = self.shader(archive, &m.shader)?;
            let frame = m.frames.first().ok_or("mesh without vertices")?;
            // What stays the same is stored in the first frame only.
            let vb: Vec<Vec<usize>> = m
                .frames
                .iter()
                .map(|frame| {
                    let made = frame.iter().map(|v| match v.count {
                        0 => 0,
                        _ => self.buffer(gpu, &v.data, wgpu::BufferUsages::VERTEX),
                    });
                    made.collect()
                })
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
                    layout,
                    frame: usize::try_from(u32_at(&a.raw, 0) as i32).ok(),
                    buffers: vb
                        .iter()
                        .map(|f| a.vertex_buffers.iter().map(|v| f[*v as usize]).collect())
                        .collect(),
                    index_buffers: a.offsets.iter().map(|i| ib[*i as usize]).collect(),
                    commands: u32_at(&a.raw, 0x10) as usize,
                });
            }
            let names = m.header.stream_maps.iter().map(|s| s.0.clone());
            let mut maps = BTreeMap::new();
            for (name, map) in names.zip(&m.stream_maps) {
                let map: Vec<usize> = map.iter().map(|s| *s as usize).collect();
                let of = |s: &usize| streams.get(*s).map(|s: &Stream| s.layout.clone());
                let layout: Option<Vec<_>> = map.iter().map(of).collect();
                let layout = layout.ok_or_else(|| format!("{}: bad stream map", m.shader))?;
                maps.insert(name, (map, self.intern(VertexLayout(layout))));
            }
            let group = self.shaders[shader].misc.get("PassGroup").map(|g| {
                let text = g.join(" ");
                let layers = text
                    .trim_end_matches(')')
                    .rsplit(|c: char| !c.is_ascii_digit());
                let layers = layers.map(str::parse).find_map(|n| n.ok()).unwrap_or(1);
                let name = text.split(|c: char| !c.is_ascii_alphabetic()).next();
                (name.unwrap_or_default().to_string(), layers)
            });
            let gaps: Vec<f32> = m.frame_keys.iter().map(|k| f32::from_bits(*k)).collect();
            meshes.push(Mesh {
                times: Times::new(&gaps),
                group,
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

    fn formats(&self, to: &Destination) -> Formats {
        Formats {
            colors: to.colors.map(|c| {
                c.map(|c| c.map_or(crate::gpu::COLOR_FORMAT, |c| self.targets[c].color.format()))
            }),
            depth: to.depth.is_some(),
        }
    }

    /// Follows a `SetRenderTarget`.
    fn aim(&mut self, to: &mut Destination, number: usize, color: &Surface, depth: &Surface) {
        fn find(ids: &HashMap<String, usize>, s: &Surface) -> Option<Option<usize>> {
            match s {
                Surface::BackBuffer => Some(None),
                Surface::Named(n) => ids.get(n).copied().map(Some),
                Surface::None => None,
            }
        }
        to.colors[number] = find(&self.buffer_ids, color);
        if *color != Surface::None && to.colors[number].is_none() {
            self.warnings.insert(format!("no buffer {color:?}"));
        }
        if number == 0 {
            // Without a name it is the back buffer's (0x404810).
            to.depth = find(&self.depth_ids, depth).or(Some(None));
        }
    }

    /// Compiles every pipeline the show can need, so playback does not stall.
    fn prewarm(&mut self, gpu: &Gpu) {
        let mut uses: std::collections::HashSet<(String, Formats)> = Default::default();
        let mut quads: std::collections::HashSet<(String, String, Formats)> = Default::default();
        for c in 0..self.chapters.len() {
            let script = std::mem::take(&mut self.chapters[c].script);
            let mut to = BACK;
            for c in &script {
                let formats = self.formats(&to);
                match c {
                    Command::Target(n, color, depth) => self.aim(&mut to, *n, color, depth),
                    Command::Objects { property, .. } | Command::Scene { property, .. } => {
                        uses.insert((property.clone(), formats));
                    }
                    Command::Reflections(property) => {
                        uses.insert((property.clone(), formats));
                        uses.insert(("Reflections".into(), formats));
                    }
                    Command::Quad { name, property, .. } => {
                        quads.insert((name.clone(), property.clone(), formats));
                    }
                    _ => {}
                }
            }
            self.chapters[c].script = script;
        }
        let mut keys: std::collections::HashSet<PipelineKey> = Default::default();
        for (property, formats) in uses {
            for d in &self.scene_draws {
                for (i, s) in self.shaders[d.shader].shaders.iter().enumerate() {
                    if s.has(&property) {
                        let passes = 0..s.passes.len();
                        keys.extend(passes.map(|p| (d.shader, i, p, d.layout, false, formats)));
                    }
                }
            }
            for m in self.models.iter().flat_map(|m| &m.meshes) {
                for (i, s) in self.shaders[m.shader].shaders.iter().enumerate() {
                    if !s.has(&property) {
                        continue;
                    }
                    for (p, pass) in s.passes.iter().enumerate() {
                        if let Some((_, layout)) = m.maps.get(&pass.stream_map) {
                            let mirrored = !self.mirrors.is_empty();
                            let key = |r| (m.shader, i, p, *layout, r, formats);
                            keys.insert(key(false));
                            keys.extend(mirrored.then(|| key(true)));
                        }
                    }
                }
            }
        }
        for (name, property, formats) in quads {
            let Some(d) = self.quads.get(&name.to_ascii_lowercase()) else {
                continue;
            };
            let file = self.scene_draws[*d].shader;
            if let Some(i) = self.shaders[file]
                .shaders
                .iter()
                .position(|s| s.has(&property))
            {
                for p in 0..self.shaders[file].shaders[i].passes.len() {
                    keys.insert((file, i, p, self.quad_layout, false, formats));
                }
            }
        }
        for k in keys {
            self.pipeline(gpu, k);
        }
        // Unused variants may legitimately not fit a mesh; only report real draws.
        self.warnings.clear();
    }

    /// Draws a moment of every chapter. Browsers compile pipelines on first use, so wait
    /// for this work to finish before starting the show.
    pub fn warm_up(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, targets: &Targets) {
        for c in 0..self.chapters.len() {
            let time = self.chapters[c].duration() * 0.5;
            self.draw(gpu, enc, targets, c, time, None);
        }
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
    fn variant(
        &self,
        file: usize,
        property: &str,
        extra: &[&str],
        lights: usize,
        mirrored: bool,
    ) -> Option<usize> {
        // What a mirror shows prefers its own shaders (0x413f50).
        if mirrored && let Some(s) = self.lit_variant(file, "Reflections", extra, lights) {
            return Some(s);
        }
        self.lit_variant(file, property, extra, lights)
    }

    /// Shaders that name no number of lights serve any. The original's comparison has no
    /// such rule, but whole scenes would be missing without it.
    fn lit_variant(
        &self,
        file: usize,
        property: &str,
        extra: &[&str],
        lights: usize,
    ) -> Option<usize> {
        let shaders = &self.shaders[file].shaders;
        let fits = |s: &sushi_shader::Shader| {
            s.has(property) && extra.iter().all(|e| e.is_empty() || s.has(e))
        };
        let exact = (0..=lights).rev().find_map(|n| {
            let rt = format!("FastRT{n}");
            shaders.iter().position(|s| fits(s) && s.has(&rt))
        });
        let counts =
            |s: &sushi_shader::Shader| s.properties.iter().any(|p| p.starts_with("FastRT"));
        exact.or_else(|| shaders.iter().position(|s| fits(s) && !counts(s)))
    }

    fn pipeline(&mut self, gpu: &Gpu, key: PipelineKey) -> Option<PipelineId> {
        if let Some(p) = self.cache.get(&key) {
            return *p;
        }
        let (file, shader, pass, layout, mirrored, formats) = key;
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
            Ok(m) => {
                let name = &s.name;
                Some(
                    self.pipelines
                        .create(gpu, name, p, m, layout, mirrored, &formats),
                )
            }
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
        geometry: (&[usize], usize, u32, u32),
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
            t.is_some_and(|t| ctx.to.colors.contains(&Some(Some(*t))))
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
            vertices: vertices.to_vec(),
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
        self.fit(gpu, targets.size());
        self.arena.clear();
        let chapter = chapter.min(self.chapters.len() - 1);
        let camera = camera.unwrap_or_else(|| Camera::on_path(&self.chapters[chapter].path, time));
        let size = |demo: &Self, to: &Destination| match to.colors[0] {
            Some(Some(b)) => demo.targets[b].color.size(),
            _ => targets.size(),
        };
        let window = targets.size().0 as f32 / targets.size().1 as f32;
        let mut projection = self.lens.projection(window, window);
        let mut eye = Eye {
            camera: &camera,
            time,
            view_proj: projection * camera.view(),
            mirror: None,
            to: BACK,
            formats: self.formats(&BACK),
        };
        let visible = self.visible(time, camera.position);
        let script = std::mem::take(&mut self.chapters[chapter].script);
        let mut calls = Vec::new();
        let mut steps: Vec<Step> = Vec::new();
        let mut marks = Vec::new();
        let mut place = Place {
            viewport: FULL,
            scissor: FULL,
        };
        // `Clear` keeps to the last viewport set, even after the target changes, when
        // drawing has the whole target again (0x42f2e0, 0x42ec80).
        let mut cleared = FULL;
        let mut shape = FULL;
        let mut seen: (Vec<Option<usize>>, Vec<Option<usize>>) = Default::default();
        let mut begin = |steps: &mut Vec<Step>, to: Destination| {
            if steps.last().is_some_and(|s| s.to == to) {
                return;
            }
            // Contents are kept between targets, and black at first.
            let mut clears = Clears::default();
            for (c, clear) in to.colors.iter().zip(&mut clears.color) {
                if let Some(c) = c
                    && !seen.0.contains(c)
                {
                    seen.0.push(*c);
                    *clear = Some(wgpu::Color::BLACK);
                }
            }
            if let Some(d) = to.depth
                && !seen.1.contains(&d)
            {
                seen.1.push(d);
                (clears.depth, clears.stencil) = (Some(1.0), Some(128));
            }
            steps.push(Step {
                to,
                clears,
                calls: Vec::new(),
            });
        };
        let (mut text_size, mut text_color) = (0.03, [1.0; 4]);
        for c in &script {
            let first = calls.len();
            let mut at = place;
            match c {
                Command::CreateColor { .. } | Command::CreateDepth { .. } => {}
                Command::Target(number, color, depth) => {
                    self.aim(&mut eye.to, *number, color, depth);
                    let matches = |demo: &Self| match eye.to.depth {
                        Some(Some(d)) => demo.depths[d].buffer.size() == size(demo, &eye.to),
                        Some(None) => targets.size() == size(demo, &eye.to),
                        None => true,
                    };
                    if !matches(self) {
                        self.warnings
                            .insert(format!("depth buffer does not fit {color:?}"));
                        eye.to.depth = None;
                    }
                    eye.formats = self.formats(&eye.to);
                    if *number == 0 {
                        place.viewport = FULL;
                    }
                }
                Command::Viewport(v) => (place.viewport, cleared, shape) = (*v, *v, *v),
                Command::Scissor(v) => place.scissor = *v,
                Command::Projection => {
                    let (w, h) = size(self, &eye.to);
                    let part = |a: f32, b: f32, n: u32| (b - a) * n as f32;
                    let aspect = part(shape[0], shape[2], w) / part(shape[1], shape[3], h);
                    projection = self.lens.projection(aspect, window);
                    eye.view_proj = projection * camera.view();
                }
                Command::Clear {
                    depth,
                    stencil,
                    color,
                } => {
                    begin(&mut steps, eye.to);
                    let step = steps.last_mut().expect("begun");
                    let shade = |c: [f32; 4]| {
                        let [r, g, b, a] = c.map(f64::from);
                        wgpu::Color { r, g, b, a }
                    };
                    if cleared == FULL {
                        if !step.calls.is_empty() {
                            let to = eye.to;
                            steps.push(Step {
                                to,
                                clears: Clears::default(),
                                calls: Vec::new(),
                            });
                        }
                        let clears = &mut steps.last_mut().expect("begun").clears;
                        clears.color[0] = color.map(shade).or(clears.color[0]);
                        clears.depth = depth.or(clears.depth);
                        clears.stencil = stencil.or(clears.stencil);
                    } else {
                        at = Place {
                            viewport: cleared,
                            scissor: FULL,
                        };
                        self.clear(gpu, &eye, *depth, *stencil, *color, &mut calls);
                    }
                }
                Command::Objects {
                    property,
                    opaque,
                    transparent,
                    receive,
                    other,
                    casting,
                } => {
                    begin(&mut steps, eye.to);
                    // The shadow pass alone ignores HIDDEN.
                    let want = |f: u32| match casting {
                        true => f & CASTS != 0,
                        false => {
                            f & HIDDEN == 0
                                && (if f & TRANSPARENT != 0 {
                                    *transparent
                                } else {
                                    *opaque
                                })
                                && (if f & RECEIVES != 0 { *receive } else { *other })
                        }
                    };
                    self.objects(gpu, &eye, &visible, property, &want, *casting, &mut calls);
                }
                Command::Scene {
                    property,
                    opaque,
                    transparent,
                } => {
                    begin(&mut steps, eye.to);
                    let items = std::mem::take(&mut self.scene_items);
                    let kinds = (*opaque, *transparent);
                    self.scene(gpu, &eye, property, kinds, &items, &mut calls);
                    self.scene_items = items;
                }
                Command::Quad {
                    name,
                    property,
                    position,
                    texture,
                } => {
                    begin(&mut steps, eye.to);
                    self.quad(gpu, &eye, name, property, (*position, *texture), &mut calls);
                }
                Command::Reflections(property) => {
                    begin(&mut steps, eye.to);
                    let step = steps.last_mut().expect("begun");
                    if step.calls.is_empty() {
                        step.clears = Clears::all(wgpu::Color::TRANSPARENT);
                    }
                    self.reflections(gpu, &eye, projection, &visible, property, &mut calls);
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
                let step = steps.last_mut().expect("drawing begins a step");
                step.calls.push((at, first..calls.len()));
            }
        }
        self.chapters[chapter].script = script;
        self.draw_calls = calls.len();
        self.arena.upload(gpu);
        self.pipelines.bind_uniforms(gpu, &self.arena);
        for step in &steps {
            let color = |c: Option<usize>| c.map_or(&targets.color, |c| &self.targets[c].color);
            let used = self.formats(&step.to).colors().len();
            let colors: Vec<_> = step.to.colors[..used]
                .iter()
                .map(|c| c.map(color))
                .collect();
            let depth = step.to.depth;
            let depth = depth.map(|d| d.map_or(&targets.depth, |d| &self.depths[d].buffer));
            let (w, h) = size(self, &step.to);
            let pixels = |r: [f32; 4]| {
                let (x0, x1) = (r[0] * w as f32, r[2] * w as f32);
                let (y0, y1) = ((1.0 - r[3]) * h as f32, (1.0 - r[1]) * h as f32);
                [x0, y0, x1, y1].map(f32::round)
            };
            let mut pass = targets::begin(enc, &colors, depth, step.clears);
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

    fn quad_buffer(&mut self, gpu: &Gpu, position: [f32; 4], texture: [f32; 4]) -> usize {
        let key = std::array::from_fn(|i| if i < 4 { position[i] } else { texture[i - 4] });
        let key = key.map(f32::to_bits);
        if let Some(b) = self.quad_buffers.get(&key) {
            return *b;
        }
        let ([l, b, r, t], [u0, v0, u1, v1]) = (position, texture);
        // Textures begin at the top here.
        let corner = |x: f32, y: f32, u: f32, v: f32| [x, y, 0.0, u, 1.0 - v];
        let corners = [
            corner(l, t, u0, v1),
            corner(r, t, u1, v1),
            corner(r, b, u1, v0),
            corner(l, b, u0, v0),
        ];
        let data = bytemuck::cast_slice(&corners);
        let made = self.buffer(gpu, data, wgpu::BufferUsages::VERTEX);
        self.quad_buffers.insert(key, made);
        made
    }

    /// A rectangle drawn with one of the scene's shaders (0x404810, command 18).
    fn quad(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        name: &str,
        property: &str,
        (position, texture): ([f32; 4], [f32; 4]),
        out: &mut Vec<DrawCall>,
    ) {
        let Some(d) = self.quads.get(&name.to_ascii_lowercase()).copied() else {
            self.warnings.insert(format!("no rectangle {name}"));
            return;
        };
        let file = self.scene_draws[d].shader;
        let found = self.shaders[file]
            .shaders
            .iter()
            .position(|s| s.has(property));
        let Some(shader) = found else {
            self.warnings
                .insert(format!("{name}: no {property} shader"));
            return;
        };
        let vertices = self.quad_buffer(gpu, position, texture);
        let textures = self.scene_draws[d].textures.clone();
        let none = BTreeMap::new();
        for pass in 0..self.shaders[file].shaders[shader].passes.len() {
            let key = (file, shader, pass, self.quad_layout, false, eye.formats);
            let Some(id) = self.pipeline(gpu, key) else {
                continue;
            };
            let ctx = Context {
                view_proj: quad_matrix(),
                view: Mat4::IDENTITY,
                mirror: None,
                to: eye.to,
                window: self.window.0 as f32 / self.window.1 as f32,
                camera: eye.camera.position,
                time: eye.time,
                lights: &[],
                ambient: &[],
                bones: &[],
                slots: &[],
                tween: 0.0,
                params: &none,
                overrides: &none,
                center: Vec3::ZERO,
                velocity: [0.0; 4],
                orientation: Mat4::IDENTITY,
            };
            let tex = |k: usize| textures.get(k).copied().flatten();
            let geometry = (&[vertices][..], self.quad_indices, 0, 6);
            self.emit(gpu, &key, id, &ctx, &[], &tex, geometry, out);
        }
    }

    /// Clears part of the targets by drawing over it.
    fn clear(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        depth: Option<f32>,
        stencil: Option<u32>,
        color: Option<[f32; 4]>,
        out: &mut Vec<DrawCall>,
    ) {
        use sushi_shader::{Compare, Cull, Pass, Stage, Stencil, StencilOp};
        let key = (depth.is_some(), color.is_some(), stencil, eye.formats);
        let id = match self.clears.get(&key) {
            Some(id) => *id,
            None => {
                let replace = StencilOp::Replace;
                let pass = Pass {
                    z_func: Compare::Always,
                    z_write: depth.is_some(),
                    cull: Cull::None,
                    stencil: stencil.map(|reference| Stencil {
                        func: Compare::Always,
                        reference,
                        zfail: replace,
                        pass: replace,
                        ccw_zfail: replace,
                        ccw_pass: replace,
                    }),
                    color_write: [color.is_some(); 4],
                    stage: Some(Stage::Factor),
                    ..Pass::default()
                };
                let layout = &self.layouts[self.quad_layout];
                let made = wgsl::translate(&pass, &|s, i| layout.find(s, i), &|_| false);
                let module = made.expect("a fixed pass translates");
                let made = &mut self.pipelines;
                let id = made.create(gpu, "clear", &pass, module, layout, false, &eye.formats);
                self.clears.insert(key, id);
                id
            }
        };
        let depth = Mat4::from_translation(Vec3::Z * depth.unwrap_or(0.0));
        let vertices = vec![self.quad_buffer(gpu, FULL, FULL)];
        out.push(DrawCall {
            pipeline: id,
            vs_offset: self.arena.push(&rows(depth * quad_matrix())),
            ps_offset: self.arena.push(&[color.unwrap_or_default()]),
            textures: Vec::new(),
            vertices,
            indices: self.quad_indices,
            start: 0,
            count: 6,
        });
    }

    /// What the mirrors show (0x415610): the prepared scene, then the objects
    /// reflected and cut off at the mirror.
    fn reflections(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        projection: Mat4,
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
                ..*eye
            };
            // The scene's mirror image needs no reflection, only its shaders.
            let shown = Eye {
                mirror: Some(Mat4::IDENTITY),
                ..reflected
            };
            self.scene(gpu, &shown, property, (true, true), &m.items, out);
            let want = |f: u32| f & HIDDEN == 0;
            self.objects(gpu, &reflected, visible, property, &want, false, out);
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
        let window = self.window.0 as f32 / self.window.1 as f32;
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
                let mirrored = eye.mirror.is_some();
                let variant = self.variant(d.shader, property, &[], lit.len(), mirrored);
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
                    to: eye.to,
                    window,
                    camera: eye.camera.position,
                    time: eye.time,
                    lights: &lit,
                    ambient: &[],
                    bones: &[],
                    slots: &[],
                    tween: 0.0,
                    params: &none,
                    overrides: &none,
                    center: Vec3::ZERO,
                    velocity: [0.0; 4],
                    orientation: Mat4::IDENTITY,
                };
                let by_light: Vec<TextureId> =
                    d.lights.iter().map(|l| self.light_textures[*l]).collect();
                for pass in 0..self.shaders[d.shader].shaders[shader].passes.len() {
                    let key = (d.shader, shader, pass, d.layout, false, eye.formats);
                    let Some(id) = self.pipeline(gpu, key) else {
                        continue;
                    };
                    let tex = |k: usize| d.textures.get(k).copied().flatten();
                    let first = out.len();
                    let i = &items[0];
                    let g = (&[i.vertices][..], i.indices, i.start, i.count);
                    self.emit(gpu, &key, id, &ctx, &by_light, &tex, g, out);
                    if out.len() == first {
                        continue;
                    }
                    for i in &items[1..] {
                        let mut call = out[first].clone();
                        call.vertices = vec![i.vertices];
                        (call.indices, call.start, call.count) = (i.indices, i.start, i.count);
                        out.push(call);
                    }
                }
            }
        }
        self.lights = lights;
        self.scene_draws = draws;
    }

    #[allow(clippy::too_many_arguments)]
    fn objects(
        &mut self,
        gpu: &Gpu,
        eye: &Eye,
        visible: &[usize],
        property: &str,
        want: &dyn Fn(u32) -> bool,
        casting: bool,
        out: &mut Vec<DrawCall>,
    ) {
        let window = self.window.0 as f32 / self.window.1 as f32;
        let time = eye.time;
        let mirrored = eye.mirror.is_some();
        let lights = std::mem::take(&mut self.lights);
        let instances = std::mem::take(&mut self.instances);
        let models = std::mem::take(&mut self.models);
        let frustum = Frustum::new(eye.view_proj);
        for &k in visible {
            let inst = &instances[k];
            let model = &models[inst.model];
            if (casting && !inst.casts_shadow) || !model.meshes.iter().any(|m| want(m.flags)) {
                continue;
            }
            let Some((min, max, center, radius)) = inst.clip.bounds(time) else {
                continue;
            };
            let radius = radius * model.expand;
            let seen_at = eye.mirror.map_or(center, |m| m.transform_point3(center));
            // Shadows can reach the view from outside it.
            if !casting && !frustum.touches(seen_at, radius) {
                continue;
            }
            let chosen: Vec<usize> = if casting {
                // The single strongest shadow light whose volume holds the object (0x415140).
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
            if casting && chosen.is_empty() {
                continue;
            }
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
            let large = |n: &str, min: f32| params.get(n).is_some_and(|v| v[0].abs() > min);
            // 0x43e5e0
            let extra = if large("SOLGlow", 1.0 / 255.0) {
                "Glow"
            } else if large("SOLVibrate", 1e-4) {
                "Vibrate"
            } else {
                ""
            };
            let wanted: Vec<&Mesh> = model.meshes.iter().filter(|m| want(m.flags)).collect();
            let mut turns: Vec<(&Mesh, String)> = Vec::new();
            for meshes in wanted.chunk_by(|a, b| a.group.is_some() && a.group == b.group) {
                let layers = meshes[0].group.as_ref().map_or(1, |g| g.1);
                for layer in 0..layers {
                    let layer = meshes[0].group.as_ref().map(|g| format!("{}{layer}", g.0));
                    let layer = layer.unwrap_or_default();
                    turns.extend(meshes.iter().map(|m| (*m, layer.clone())));
                }
            }
            for (mesh, layer) in turns {
                // Meshes take part only in the passes their shaders name.
                let find = |extra: &str| {
                    let extra = [extra, layer.as_str()];
                    self.variant(mesh.shader, property, &extra, lit.len(), mirrored)
                };
                let Some(shader) = find(extra).or_else(|| find("")) else {
                    continue;
                };
                let exact = format!("FastRT{}", lit.len());
                let s = &self.shaders[mesh.shader].shaders[shader];
                let counts = s.properties.iter().any(|p| p.starts_with("FastRT"));
                if counts && !s.has(&exact) {
                    let w = format!("{} {property} {exact}: using {}", mesh.name, s.name);
                    self.warnings.insert(w);
                }
                for pass in 0..self.shaders[mesh.shader].shaders[shader].passes.len() {
                    let p = &self.shaders[mesh.shader].shaders[shader].passes[pass];
                    let Some((map, layout)) = mesh.maps.get(&p.stream_map) else {
                        let w = format!("no stream map {:?}", p.stream_map);
                        self.warnings.insert(w);
                        continue;
                    };
                    // Streams drawn together share their commands.
                    let stream = &mesh.streams[map[0]];
                    let key = (mesh.shader, shader, pass, *layout, mirrored, eye.formats);
                    let Some(id) = self.pipeline(gpu, key) else {
                        let w = format!("{}: no pipeline for pass {pass}", mesh.name);
                        self.warnings.insert(w);
                        continue;
                    };
                    let (frame, tween) = mesh.times.locate(inst.clip.wrap(time));
                    let mut slots = [0usize; 40];
                    let mut overrides = BTreeMap::new();
                    let mut textures: Vec<Option<TextureId>> =
                        vec![None; self.shaders[mesh.shader].textures.len()];
                    let mut geometry = (Vec::new(), 0);
                    for c in &mesh.commands[stream.commands] {
                        let (a, b) = (c[1] as usize, c[2] as usize);
                        match c[0] {
                            0 => {
                                let ctx = Context {
                                    view_proj: eye.view_proj,
                                    view: eye.camera.view(),
                                    mirror: eye.mirror,
                                    to: eye.to,
                                    window,
                                    camera: eye.camera.position,
                                    time,
                                    lights: &lit,
                                    ambient: &ambient,
                                    bones: &bones,
                                    slots: &slots,
                                    tween,
                                    params: &params,
                                    overrides: &overrides,
                                    center: model.center,
                                    velocity,
                                    orientation,
                                };
                                let g = (&geometry.0[..], geometry.1, c[1], c[2] - c[1] + 1);
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
                            4 => {
                                let bound = map.iter().map(|s| {
                                    let s = &mesh.streams[*s];
                                    let f = s.frame.map_or(0, |f| frame + f);
                                    s.buffers[f.min(s.buffers.len() - 1)][a]
                                });
                                geometry = (bound.collect(), stream.index_buffers[a]);
                            }
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
    let mut value: Vec<f32> = var.default.clone();
    // The size of a pixel is written down for a 4:3 picture; keep it square.
    if name.starts_with("vTexel") && !value.is_empty() {
        value[0] *= camera::ORIGINAL_ASPECT / ctx.window;
    }
    let value = |r: usize| std::array::from_fn(|i| value.get(r * 4 + i).copied().unwrap_or(0.0));
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
        "morphlerp0" => vec![[1.0 - ctx.tween, ctx.tween, 0.0, 0.0]],
        "Time" => vec![[ctx.time; 4]],
        _ => return None,
    })
}
