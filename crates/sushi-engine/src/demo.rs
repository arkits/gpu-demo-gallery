//! The whole demo: loaded data plus everything needed to draw one frame.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use glam::{Mat3, Mat4, Vec3, Vec4};
use sushi_formats::archive::Archive;
use sushi_formats::path::{self, FlyPath};
use sushi_formats::reader::{cstr, f32_at, u32_at};
use sushi_formats::scene::{self, Edited};
use sushi_formats::shader_header::ShaderHeader;
use sushi_formats::transfer::{self as blocks, Transfer, Weights};
use sushi_formats::{anim, mesh, object, particles};
use sushi_shader::wgsl::{self, ElementType, Uniform};
use sushi_shader::{Pass, ShaderFile, VarKind};

use crate::camera::{self, Camera, Lens};
use crate::gpu::{COLOR_FORMAT, DEEP_FORMAT, Gpu, SHADOW_FORMAT, TEN_FORMAT};
use crate::light::{Light, Moving};
use crate::lua::{self, State, Value};
use crate::object::{Clip, Place as BonePlace, Times};
use crate::overlay::Mark;
use crate::pipeline::*;
use crate::psystem;
use crate::settings::Settings;
use crate::targets::{self, Clears, ColorBuffer, DepthBuffer, SAMPLES, Targets};
use crate::transfer::{self, Around, Harmonics};

pub type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

/// Mesh record flags (+0), matching the words in the scripts.
const CASTS: u32 = 1;
const RECEIVES: u32 = 2;
const TRANSPARENT: u32 = 8;
/// Helper geometry that only casts shadows (0x432160).
const HIDDEN: u32 = 0x10;

/// Seconds between a frame and the one before it, for motion blur. The original used its
/// own frame time; a fixed step keeps the look frame-rate independent.
const BEFORE: f32 = 1.0 / 60.0;
/// Script numbers of buffers start here; 0 is the back buffer.
const FIRST_COLOR: usize = 100;
const FIRST_DEPTH: usize = 200;

struct Stream {
    layout: StreamLayout,
    /// Which shape it holds, if the mesh changes shape.
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
    params: Vec<(String, Vec<Edited>)>,
    /// Names of the shader's textures, as draw commands number them.
    slots: Vec<String>,
    times: Times,
    /// Seconds after which the shapes repeat.
    shapes_end: f32,
    weights: Option<Weighted>,
    transfer: Option<(Transfer, Harmonics, usize)>,
}

/// Shapes that are added by weights.
struct Weighted {
    weights: Weights,
    times: Times,
}

impl Weighted {
    /// The strongest shapes and their weights (0x46eb70).
    fn at(&self, time: f32) -> Vec<(usize, f32)> {
        let w = &self.weights;
        let (i, f) = self
            .times
            .locate(time.max(0.0).rem_euclid(w.duration.max(1e-3)));
        let (a, b) = (&w.keys[i], &w.keys[(i + 1).min(w.keys.len() - 1)]);
        let mut all: Vec<(usize, f32)> = a
            .iter()
            .zip(b)
            .map(|(a, b)| a * (1.0 - f) + b * f)
            .enumerate()
            .collect();
        all.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        all.truncate(w.most);
        all
    }
}

struct Model {
    meshes: Vec<Mesh>,
    textures: Vec<Option<TextureId>>,
    center: Vec3,
}

struct Instance {
    name: String,
    model: usize,
    clip: Rc<Clip>,
    /// Seconds of the show.
    start: f32,
    end: f32,
    normal: bool,
    reflect: bool,
}

struct ParticleSet {
    system: psystem::System,
    shader: usize,
    layout: LayoutId,
    /// What each element of each stream holds.
    elements: Vec<Vec<String>>,
    textures: Vec<(String, Option<TextureId>)>,
}

#[derive(Default)]
struct ObjectSet {
    instances: Vec<Instance>,
    particles: Vec<ParticleSet>,
}

#[derive(Clone)]
struct SceneItem {
    draw: usize,
    vertices: usize,
    indices: usize,
    start: u32,
    count: u32,
}

#[derive(Clone)]
struct SceneDraw {
    transparent: bool,
    shader: usize,
    layout: LayoutId,
    slots: Vec<String>,
    textures: Vec<Option<TextureId>>,
    params: Vec<Edited>,
}

struct Mirror {
    number: u32,
    plane: Vec4,
    matrix: Mat4,
    /// The scene as seen in the mirror, prepared by the original's compiler.
    items: Vec<SceneItem>,
}

#[derive(Default)]
struct SceneData {
    draws: Vec<SceneDraw>,
    items: Vec<SceneItem>,
    mirrors: Vec<Mirror>,
    lights: Vec<Moving>,
    /// Draw records by the names that scripts use for rectangles.
    quads: HashMap<String, usize>,
}

/// One camera path of a chapter.
struct Part {
    start: f32,
    path: FlyPath,
    call: String,
    set: usize,
    lights: Option<usize>,
}

/// What the menu lists.
pub struct Chapter {
    pub title: String,
    parts: Vec<Part>,
    scripts: Vec<String>,
    duration: f32,
    /// Its script changes the time.
    jumps: bool,
}

impl Chapter {
    pub fn duration(&self) -> f32 {
        self.duration
    }
    fn end(&self) -> f32 {
        self.parts.last().map_or(0.0, |p| p.start + p.path.length())
    }
    fn part(&self, time: f32) -> &Part {
        let i = self
            .parts
            .iter()
            .rposition(|p| p.start <= time + 1e-3)
            .unwrap_or(0);
        &self.parts[i]
    }
}

/// A buffer that scripts draw to and shaders read.
struct Buffer {
    /// Asked for; those as large as the window follow it.
    size: (u32, u32),
    format: wgpu::TextureFormat,
    color: ColorBuffer,
    texture: TextureId,
}

/// A depth buffer. Shaders may read it: then there is a picture of it.
struct Depth {
    size: (u32, u32),
    read: Option<usize>,
}

/// Where draws land; `None` in the outer options is nowhere, in the inner ones the back
/// buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Destination {
    colors: [Option<Option<usize>>; 4],
    face: usize,
    depth: Option<Option<usize>>,
}

const BACK: Destination = Destination {
    colors: [Some(None), None, None, None],
    face: 0,
    depth: Some(None),
};

/// Viewport and scissor as fractions.
#[derive(Debug, Clone, Copy)]
struct Area {
    viewport: [f32; 4],
    scissor: [f32; 4],
}

const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

enum Step {
    Draw {
        to: Destination,
        clears: Clears,
        calls: Vec<(Area, std::ops::Range<usize>)>,
    },
    Copy(Option<usize>, Option<usize>),
}

/// How one pass over the world is seen.
#[derive(Clone)]
struct Eye {
    camera: Camera,
    view: Mat4,
    projection: Mat4,
    view_proj: Mat4,
    /// Set while drawing what a mirror shows.
    mirror: Option<Mat4>,
    to: Destination,
    formats: Formats,
    /// Only depth is drawn.
    depth_only: bool,
}

impl Eye {
    fn aim(&mut self, view: Mat4, projection: Mat4) {
        self.view = view;
        self.projection = projection;
        self.view_proj = projection * view;
    }
}

/// Values the engine feeds to shaders for one draw.
struct Context<'a> {
    eye: &'a Eye,
    frame: &'a Moment,
    /// Seconds of the object's own time.
    time: f32,
    lights: &'a [&'a Light],
    bones: &'a [BonePlace],
    /// A frame earlier.
    bones_before: &'a [BonePlace],
    slots: &'a [usize],
    tween: f32,
    weights: &'a [(usize, f32)],
    params: &'a BTreeMap<String, [f32; 4]>,
    overrides: &'a BTreeMap<String, [f32; 4]>,
    edited: &'a [Edited],
    around: Option<&'a (Around, Vec<[f32; 4]>)>,
    center: Vec3,
    tile: [f32; 2],
}

impl<'a> Context<'a> {
    fn new(
        eye: &'a Eye,
        frame: &'a Moment,
        time: f32,
        none: &'a BTreeMap<String, [f32; 4]>,
    ) -> Self {
        Self {
            eye,
            frame,
            time,
            lights: &[],
            bones: &[],
            bones_before: &[],
            slots: &[],
            tween: 0.0,
            weights: &[],
            params: none,
            overrides: none,
            edited: &[],
            around: None,
            center: Vec3::ZERO,
            tile: [1.0; 2],
        }
    }
}

/// What all draws of a frame share.
#[derive(Default)]
struct Moment {
    time: f32,
    window: (u32, u32),
    view_before: Mat4,
    camera_view_before: Mat4,
    focus: [f32; 4],
    script: BTreeMap<String, Vec<[f32; 4]>>,
    /// Texture matrices of `LightAgentViewProjMatrix` and the like, by object.
    toward: Vec<(Vec3, f32)>,
    key: Option<Vec3>,
}

type LayoutId = usize;
/// Shader file, shader, pass, vertex layout, whether winding is reversed, targets, depth
/// only.
type PipelineKey = (usize, usize, usize, LayoutId, bool, Formats, bool);

/// Where the show is, for scripts that remember things.
#[derive(Default)]
struct Show {
    chapter: Option<usize>,
    /// Seconds since the chapter began, and of the script's clock.
    at: f32,
    script: f32,
}

pub struct Demo {
    archive: Archive,
    chapters: Vec<Chapter>,
    state: State,
    show: Show,
    buffer_ids: HashMap<String, usize>,
    targets: Vec<Buffer>,
    depth_ids: HashMap<String, usize>,
    depths: Vec<Depth>,
    /// By number (`None` for the back buffer's), size and samples.
    depth_buffers: HashMap<(Option<usize>, (u32, u32), u32), DepthBuffer>,
    back_format: wgpu::TextureFormat,
    copies: HashMap<(wgpu::TextureFormat, u32), wgpu::RenderPipeline>,
    /// Textures that shaders read depth from.
    depth_names: BTreeSet<String>,
    /// Size of the back buffer that the buffers were made for.
    window: (u32, u32),
    lens: Lens,
    fov: f32,
    pub settings: Settings,
    quad_buffers: HashMap<[u32; 8], usize>,
    quad_indices: usize,
    quad_layout: LayoutId,
    clears: HashMap<(bool, bool, Option<u32>, Formats), PipelineId>,
    font: Vec<sushi_formats::texture::Level>,
    textures: Textures,
    by_file: HashMap<String, TextureId>,
    pipelines: Pipelines,
    arena: Arena,
    buffers: Vec<wgpu::Buffer>,
    index_formats: HashMap<usize, wgpu::IndexFormat>,
    /// Vertex buffers of particles, made anew every frame.
    transient: Vec<usize>,
    free_buffers: Vec<usize>,
    shaders: Vec<ShaderFile>,
    shader_ids: HashMap<String, usize>,
    shader_names: Vec<String>,
    layouts: Vec<VertexLayout>,
    cache: HashMap<PipelineKey, Option<PipelineId>>,
    scenes: Vec<SceneData>,
    scene_ids: HashMap<String, usize>,
    quads: SceneData,
    models: Vec<Model>,
    sets: Vec<ObjectSet>,
    /// Problems found while drawing, each reported once.
    warnings: BTreeSet<String>,
    draw_calls: usize,
    chip_logo: Vec<sushi_formats::texture::Level>,
    /// Pictures the engine brings along, by their `AppUpdate` name.
    pictures: HashMap<String, TextureId>,
}

fn element_type(kind: u32) -> Result<ElementType> {
    Ok(match kind {
        0 => ElementType::Float1,
        1 => ElementType::Float2,
        2 => ElementType::Float3,
        3 => ElementType::Float4,
        4 => ElementType::Color,
        5 => ElementType::Ubyte4,
        // Ten bits each are widened to sixteen when loaded.
        10 | 14 => ElementType::Short4n,
        k => return Err(format!("vertex element type {k}").into()),
    })
}

/// Bytes of an element in the file and in a buffer.
fn sizes(kind: u32) -> (usize, usize) {
    match kind {
        0..=3 => (4 * (kind as usize + 1), 4 * (kind as usize + 1)),
        10 => (8, 8),
        14 => (4, 8),
        _ => (4, 4),
    }
}

fn layout(header: &ShaderHeader, stream: usize) -> Result<StreamLayout> {
    let s = header.streams.get(stream).ok_or("missing vertex stream")?;
    let mut offset = 0;
    let mut elements = Vec::new();
    for e in &s.elements {
        let (name, index) = e.semantic();
        elements.push((name.to_string(), index, element_type(e.kind)?, offset));
        offset += sizes(e.kind).1 as u32;
    }
    Ok(StreamLayout {
        stride: offset,
        elements,
    })
}

/// Three signed numbers of ten bits as four of sixteen.
fn widen(b: &[u8]) -> [u8; 8] {
    let v = u32_at(b, 0);
    let mut out = [0; 8];
    for k in 0..3 {
        let x = ((v >> (10 * k) & 0x3ff) << 22) as i32 >> 22;
        let x = (x.max(-511) * 32767 / 511) as i16;
        out[k * 2..][..2].copy_from_slice(&x.to_le_bytes());
    }
    out[6..].copy_from_slice(&32767i16.to_le_bytes());
    out
}

/// Vertices of a stream, with ten-bit elements widened.
fn vertices(header: &ShaderHeader, stream: usize, stride: usize, data: &[u8]) -> Result<Vec<u8>> {
    let s = header.streams.get(stream).ok_or("missing vertex stream")?;
    let size: usize = s.elements.iter().map(|e| sizes(e.kind).0).sum();
    if size != stride {
        return Err(format!("vertex stride {stride}, not {size}").into());
    }
    if s.elements.iter().all(|e| e.kind != 14) {
        return Ok(data.to_vec());
    }
    let mut out = Vec::new();
    for v in data.chunks_exact(stride) {
        let mut at = 0;
        for e in &s.elements {
            let n = sizes(e.kind).0;
            match e.kind {
                14 => out.extend(widen(&v[at..])),
                _ => out.extend(&v[at..at + n]),
            }
            at += n;
        }
    }
    Ok(out)
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

/// `Start(Frame, 5)` or `End(3.5)` in seconds.
fn moment(line: &str, what: &str, or: f32) -> f32 {
    let Some(i) = line.find(&format!("{what}(")) else {
        return or;
    };
    let inner = line[i + what.len() + 1..].split(')').next().unwrap_or("");
    let last: &str = inner.split(',').next_back().unwrap_or("").trim();
    match last.parse::<f32>() {
        Ok(v) if inner.contains("Frame") => (v - 1.0) / 30.0,
        Ok(v) => v,
        Err(_) => or,
    }
}

impl Demo {
    /// `data` is the contents of the user's `Sushi.ati`, in one or more parts.
    pub fn load(gpu: &Gpu, data: &[&[u8]], ten: bool) -> Result<Self> {
        let archive = match data {
            [one] => Archive::open(one)?,
            parts => Archive::open(&parts.concat())?,
        };
        let text = |name: &str| -> Result<String> {
            let b = archive
                .get(name)
                .ok_or_else(|| format!("{name} not in Sushi.ati"))?;
            Ok(b.iter().map(|&c| c as char).collect())
        };
        let settings = Settings::parse(&text("Sushi2.ini")?)?;
        let mut paths = Vec::new();
        for c in &settings.chapters {
            for p in &c.parts {
                paths.push(
                    path::read_path(&text(&format!("FlyPaths/{}", p.path))?)
                        .map_err(|e| format!("{}: {e}", p.path))?,
                );
            }
        }
        let lens = Lens {
            near: settings.near,
            far: settings.far,
            film: settings.film,
            blur: settings.blur,
        };
        let mut demo = Self {
            archive,
            chapters: Vec::new(),
            state: State::new(),
            show: Show::default(),
            buffer_ids: HashMap::new(),
            targets: Vec::new(),
            depth_ids: HashMap::new(),
            depths: Vec::new(),
            depth_buffers: HashMap::new(),
            back_format: if ten { TEN_FORMAT } else { COLOR_FORMAT },
            copies: HashMap::new(),
            depth_names: BTreeSet::new(),
            window: (0, 0),
            lens,
            fov: settings.fov,
            settings,
            quad_buffers: HashMap::new(),
            quad_indices: 0,
            quad_layout: 0,
            clears: HashMap::new(),
            font: Vec::new(),
            textures: Textures::new(gpu),
            by_file: HashMap::new(),
            pipelines: Pipelines::default(),
            arena: Arena::new(gpu),
            buffers: Vec::new(),
            index_formats: HashMap::new(),
            transient: Vec::new(),
            free_buffers: Vec::new(),
            shaders: Vec::new(),
            shader_ids: HashMap::new(),
            shader_names: Vec::new(),
            layouts: Vec::new(),
            cache: HashMap::new(),
            scenes: Vec::new(),
            scene_ids: HashMap::new(),
            quads: SceneData::default(),
            models: Vec::new(),
            sets: Vec::new(),
            warnings: Default::default(),
            draw_calls: 0,
            chip_logo: Vec::new(),
            pictures: HashMap::new(),
        };
        demo.quad_indices = demo.index_buffer(gpu, &[0, 2, 1, 2, 0, 3]);
        demo.quad_layout = demo.intern(VertexLayout(vec![quad_stream()]));
        // Whatever names a texture may be the first to hold it, so objects come first.
        let mut models = HashMap::new();
        let first = demo.settings.scenes[0].clone();
        demo.load_set(gpu, &first, &mut models)?;
        let mut sets = HashMap::from([(first, 0)]);
        let chapters = demo.settings.chapters.clone();
        for p in chapters.iter().flat_map(|c| &c.parts) {
            if let Some(o) = &p.objects
                && !sets.contains_key(o)
            {
                sets.insert(o.clone(), demo.sets.len());
                demo.load_set(gpu, o, &mut models)?;
            }
        }
        for s in demo.settings.scenes.clone() {
            demo.load_scene(gpu, &s)?;
        }
        for p in chapters.iter().flat_map(|c| &c.parts) {
            if let Some(l) = &p.lights {
                demo.load_scene(gpu, &l.clone())?;
            }
        }
        let quads = demo.settings.quads.clone();
        let q = demo.load_scene(gpu, &quads)?;
        demo.quads = std::mem::take(&mut demo.scenes[q]);
        let mut paths = paths.into_iter();
        for c in &chapters {
            let mut parts: Vec<Part> = Vec::new();
            for p in &c.parts {
                let path = paths.next().expect("one path per part");
                parts.push(Part {
                    start: parts.last().map_or(0.0, |l| l.start + l.path.length()),
                    path,
                    call: p.call.clone(),
                    set: p.objects.as_ref().map_or(0, |o| sets[o]),
                    lights: p.lights.as_ref().map(|l| demo.scene_ids[l]),
                });
            }
            let mut scripts: Vec<String> = Vec::new();
            for p in &c.parts {
                if !scripts.contains(&p.script) {
                    scripts.push(p.script.clone());
                }
            }
            let jumps = scripts.iter().any(|s| {
                demo.text(&format!("Scripts/{s}"))
                    .is_some_and(|t| t.contains("SetCurrentTime"))
            });
            let mut chapter = Chapter {
                title: c.title.clone(),
                parts,
                scripts,
                duration: 0.0,
                jumps,
            };
            chapter.duration = chapter.end();
            demo.chapters.push(chapter);
        }
        demo.load_pictures(gpu)?;
        for f in &demo.shaders {
            for t in f.textures.iter().filter_map(|t| t.target.as_ref()) {
                if t.starts_with('z') {
                    demo.depth_names.insert(t.clone());
                }
            }
        }
        demo.prepare(gpu, (1024, 768))?;
        Ok(demo)
    }

    fn text(&self, name: &str) -> Option<String> {
        Some(self.archive.get(name)?.iter().map(|&c| c as char).collect())
    }

    /// Gives back the files, which are not needed after loading, except for scripts.
    pub fn forget_files(&mut self) {
        self.archive.keep(|n| n.starts_with("scripts/"));
    }

    fn load_pictures(&mut self, gpu: &Gpu) -> Result<()> {
        let data = self.archive.get("Globals.ati").ok_or("no Globals.ati")?;
        let globals = scene::read_globals(data)?;
        for t in &globals.textures {
            let Some(id) = self.textures.add(gpu, t)? else {
                continue;
            };
            // Texture 3 of Globals.ati is ChipLogo.tga (0x421c50).
            if t.name.contains("ChipLogo") {
                self.chip_logo = upright(t)?;
                self.pictures.insert("ChipLogoTexture".into(), id);
            } else if t.name.contains("AtiLogo") {
                self.pictures.insert("ATILogoTexture".into(), id);
            }
            if t.name.to_ascii_lowercase().ends_with("timesroman.tga") {
                self.font = upright(t)?;
            }
        }
        Ok(())
    }

    /// The picture that `Logo2Texture` shows.
    pub fn set_logo(&mut self, gpu: &Gpu, l: &sushi_formats::texture::Level) {
        let t = gpu.picture(l);
        let id = self
            .textures
            .add_view(t.create_view(&Default::default()), false);
        self.pictures.insert("Logo2Texture".into(), id);
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    /// The font: 8 columns and 16 rows of characters.
    pub fn font(&self) -> &[sushi_formats::texture::Level] {
        &self.font
    }

    pub fn chip_logo(&self) -> &[sushi_formats::texture::Level] {
        &self.chip_logo
    }

    /// Draws in the last frame.
    pub fn draw_calls(&self) -> usize {
        self.draw_calls
    }

    pub fn warnings(&self) -> impl Iterator<Item = &str> {
        self.warnings.iter().map(String::as_str)
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
        let buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &data,
                usage,
            });
        if let Some(id) = self.free_buffers.pop() {
            self.buffers[id] = buffer;
            self.index_formats.remove(&id);
            id
        } else {
            self.buffers.push(buffer);
            self.buffers.len() - 1
        }
    }

    fn index_buffer(&mut self, gpu: &Gpu, indices: &[u32]) -> usize {
        if indices.iter().all(|i| *i < 0xffff) {
            let short: Vec<u16> = indices.iter().map(|i| *i as u16).collect();
            return self.buffer(gpu, bytemuck::cast_slice(&short), wgpu::BufferUsages::INDEX);
        }
        let b = self.buffer(
            gpu,
            bytemuck::cast_slice(indices),
            wgpu::BufferUsages::INDEX,
        );
        self.index_formats.insert(b, wgpu::IndexFormat::Uint32);
        b
    }

    fn indices(&self, buffer: usize) -> (usize, wgpu::IndexFormat) {
        let f = self.index_formats.get(&buffer).copied();
        (buffer, f.unwrap_or(wgpu::IndexFormat::Uint16))
    }

    fn shader(&mut self, name: &str) -> Result<usize> {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(id) = self.shader_ids.get(&key) {
            return Ok(*id);
        }
        let path = format!("shaders/{key}");
        let dir = path.rsplit_once('/').map_or("", |p| p.0).to_string();
        let src = self
            .text(&path)
            .ok_or_else(|| format!("no shader {path}"))?;
        let include = |n: &str| {
            let n = n.replace('\\', "/");
            [
                format!("{dir}/{n}"),
                format!("shaders/includes/{n}"),
                format!("shaders/{n}"),
            ]
            .iter()
            .find_map(|p| self.text(p))
        };
        let file = sushi_shader::parse(&src, &include).map_err(|e| format!("{name}: {e}"))?;
        self.shaders.push(file);
        self.shader_names.push(key.clone());
        self.shader_ids.insert(key, self.shaders.len() - 1);
        Ok(self.shaders.len() - 1)
    }

    fn add_texture(
        &mut self,
        gpu: &Gpu,
        t: &sushi_formats::texture::Texture,
    ) -> Result<Option<TextureId>> {
        if t.name.is_empty() {
            return Ok(None);
        }
        let id = self.textures.add(gpu, t)?;
        if let Some(id) = id {
            let file = t.name.replace('\\', "/");
            let file = file.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
            self.by_file.entry(file).or_insert(id);
        }
        Ok(id)
    }

    fn load_scene(&mut self, gpu: &Gpu, name: &str) -> Result<usize> {
        if let Some(id) = self.scene_ids.get(name) {
            return Ok(*id);
        }
        let data = self
            .archive
            .get(&format!("Scenes/{name}.ati"))
            .ok_or_else(|| format!("no scene {name}"))?;
        let s = scene::read_scene(data).map_err(|e| format!("{name}: {e}"))?;
        let mut textures = Vec::new();
        for t in &s.textures {
            textures.push(self.add_texture(gpu, t)?);
        }
        let mut out = SceneData {
            lights: s.lights.iter().map(Moving::new).collect(),
            ..Default::default()
        };
        for d in &s.draws {
            let g = &s.geometry[d.geometry()];
            let sh = &s.shaders[g.shader()];
            let shader = self.shader(&cstr(&sh.raw))?;
            let slots: Vec<String> = sh.header.textures.iter().map(|t| t.0.clone()).collect();
            let stream = layout(&sh.header, 0);
            let layout = self.intern(VertexLayout(stream.into_iter().collect()));
            out.draws.push(SceneDraw {
                transparent: g.transparent(),
                shader,
                layout,
                textures: (0..slots.len())
                    .map(|k| d.texture(k).and_then(|t| textures[t]))
                    .collect(),
                slots,
                params: d.params.clone(),
            });
        }
        for q in &s.table_112 {
            out.quads.insert(
                cstr(&q[..0x20]).to_ascii_lowercase(),
                u32_at(q, 0x20) as usize,
            );
        }
        let mut made = HashMap::new();
        out.items = self.scene_list(gpu, &s, &s.groups, &out.draws, &mut made)?;
        for m in &s.mirrors {
            // All of these demos' mirrors draw to a buffer. Those limited to their outline
            // (+5) need no limit here: the rest of the buffer is not seen.
            // The surface itself is drawn with the scene (the end of 0x415610).
            let surface = self.scene_list(gpu, &s, &m.sets[0].groups, &out.draws, &mut made)?;
            out.items.splice(0..0, surface);
            let items = self.scene_list(gpu, &s, &m.sets[1].groups, &out.draws, &mut made)?;
            out.mirrors.push(Mirror {
                number: u32_at(&m.raw, 0),
                plane: Vec4::from_array(m.plane()),
                matrix: Mat4::from_cols_array(&m.matrix()),
                items,
            });
        }
        self.scenes.push(out);
        self.scene_ids.insert(name.into(), self.scenes.len() - 1);
        Ok(self.scenes.len() - 1)
    }

    fn scene_list(
        &mut self,
        gpu: &Gpu,
        s: &scene::Scene,
        groups: &[scene::Group],
        draws: &[SceneDraw],
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
                    let header = &s.shaders[g.shader()].header;
                    let raw = self::vertices(header, 0, st.stride() as usize, raw)?;
                    let v = self.buffer(gpu, &raw, wgpu::BufferUsages::VERTEX);
                    let ib = st
                        .index_buffer_of(item.vertex_buffer)
                        .ok_or("missing index buffer")?;
                    let i = self.index_buffer(gpu, &g.chunk.indices[ib].1);
                    made.insert((gi, item.vertex_buffer), (v, i));
                    (v, i)
                }
            };
            let _ = draws;
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

    fn load_set(
        &mut self,
        gpu: &Gpu,
        name: &str,
        models: &mut HashMap<String, usize>,
    ) -> Result<()> {
        let ini = self
            .text(&format!("Objects/{name}.ini"))
            .ok_or_else(|| format!("no Objects/{name}.ini"))?;
        let mut set = ObjectSet::default();
        // Block comments that begin with `//` are none.
        let lines: Vec<String> = ini
            .lines()
            .skip(1)
            .map(|l| l.split("//").next().unwrap_or("").trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let mut clips: HashMap<String, Rc<Clip>> = HashMap::new();
        for l in &lines {
            let (start, end) = (moment(l, "Start", 0.0), moment(l, "End", f32::INFINITY));
            let file = |ext: &str| {
                let w = l.split(|c: char| c.is_whitespace() || c == '(' || c == ')');
                w.map(str::to_string)
                    .find(|w| w.to_ascii_lowercase().ends_with(ext))
            };
            if l.starts_with("PG_Global") {
                let f = file(".spg").ok_or_else(|| format!("bad line {l}"))?;
                self.load_particles(gpu, &f, start, end, &mut clips, &mut set)?;
                continue;
            }
            if !l.starts_with("Global") && !l.starts_with("SGlobal") {
                return Err(format!("unknown object {l}").into());
            }
            let (Some(so), Some(sa)) = (file(".so2"), file(".sa2")) else {
                return Err(format!("bad line {l}").into());
            };
            let data = self
                .archive
                .get(&format!("Objects/{sa}"))
                .ok_or_else(|| format!("no {sa}"))?;
            let animation = anim::read_animation(data).map_err(|e| format!("{sa}: {e}"))?;
            // Shapes and weights come with the animation.
            let own = !animation.morph_meshes.is_empty() || !animation.blocks.is_empty();
            let key = if own {
                format!("{so}|{sa}")
            } else {
                so.clone()
            }
            .to_ascii_lowercase();
            let model = match models.get(&key) {
                Some(m) => *m,
                None => {
                    let data = self
                        .archive
                        .get(&format!("Objects/{so}"))
                        .ok_or_else(|| format!("no {so}"))?;
                    let o = object::read_object(data).map_err(|e| format!("{so}: {e}"))?;
                    for t in &o.textures {
                        self.add_texture(gpu, t)?;
                    }
                    let m = self
                        .model(gpu, &o, &animation)
                        .map_err(|e| format!("{so}: {e}"))?;
                    models.insert(key, m);
                    m
                }
            };
            let name = l
                .find("Name(")
                .map(|i| l[i + 5..].split(')').next().unwrap_or("").trim_matches('"'));
            set.instances.push(Instance {
                name: name
                    .unwrap_or_else(|| so.rsplit_once('.').map_or(so.as_str(), |x| x.0))
                    .into(),
                model,
                clip: Rc::new(Clip::new(&animation)),
                start,
                end,
                normal: !l.contains("NoNormal"),
                reflect: !l.contains("NoReflect"),
            });
        }
        self.sets.push(set);
        Ok(())
    }

    fn load_particles(
        &mut self,
        gpu: &Gpu,
        file: &str,
        start: f32,
        end: f32,
        clips: &mut HashMap<String, Rc<Clip>>,
        set: &mut ObjectSet,
    ) -> Result<()> {
        let data = self
            .archive
            .get(&format!("Objects/{file}"))
            .ok_or_else(|| format!("no {file}"))?;
        for (k, s) in particles::read_particles(data)
            .map_err(|e| format!("{file}: {e}"))?
            .iter()
            .enumerate()
        {
            let mut wanted = Vec::new();
            fn names(n: &particles::Node, out: &mut Vec<String>) {
                out.extend(
                    n.words
                        .iter()
                        .filter(|w| w.to_ascii_lowercase().ends_with(".sa2"))
                        .cloned(),
                );
                n.children.iter().for_each(|c| names(c, out));
            }
            names(&s.text, &mut wanted);
            for w in wanted {
                let key = w.to_ascii_lowercase();
                if !clips.contains_key(&key)
                    && let Some(d) = self.archive.get(&format!("Objects/{w}"))
                {
                    clips.insert(key, Rc::new(Clip::new(&anim::read_animation(d)?)));
                }
            }
            let seed =
                0x9e37_79b9u32.wrapping_mul(set.particles.len() as u32 * 16 + k as u32 + 1) | 1;
            let find = |n: &str| clips.get(&n.to_ascii_lowercase()).cloned();
            let system = psystem::System::new(s, start, end, seed, &find)
                .map_err(|e| format!("{file}: {e}"))?;
            let name = s
                .text
                .child("Shader")
                .and_then(|n| n.words.get(1))
                .ok_or("particles without shader")?;
            let shader = self.shader(&name.clone())?;
            let mut textures = Vec::new();
            for ((slot, _), t) in s.header.textures.iter().zip(&s.textures) {
                textures.push((slot.clone(), self.add_texture(gpu, t)?));
            }
            let streams = (0..s.header.streams.len())
                .map(|k| layout(&s.header, k))
                .collect::<Result<_>>()?;
            set.particles.push(ParticleSet {
                system,
                shader,
                layout: self.intern(VertexLayout(streams)),
                elements: s
                    .header
                    .streams
                    .iter()
                    .map(|s| s.elements.iter().map(|e| e.source.clone()).collect())
                    .collect(),
                textures,
            });
        }
        Ok(())
    }

    fn model(&mut self, gpu: &Gpu, o: &object::Object, a: &anim::Animation) -> Result<usize> {
        let textures = o.textures.iter().map(|t| self.textures.find(t)).collect();
        let mut meshes = Vec::new();
        for (number, m) in o.meshes.iter().chain(&a.morph_meshes).enumerate() {
            let shader = self.shader(&m.shader)?;
            let frame = m.frames.first().ok_or("mesh without vertices")?;
            // Which stream of the header each vertex buffer belongs to.
            let mut stream_of = vec![0; frame.len()];
            for (k, a) in m.stream_arrays.iter().enumerate() {
                a.vertex_buffers
                    .iter()
                    .for_each(|v| stream_of[*v as usize] = k);
            }
            // Arrays of streams share one description.
            let stream_index: Vec<usize> = self.stream_numbers(m);
            let mut vb: Vec<Vec<usize>> = Vec::new();
            for f in &m.frames {
                let mut made = Vec::new();
                for (v, k) in f.iter().zip(&stream_of) {
                    // What stays the same is stored in the first frame only.
                    made.push(match v.count {
                        0 => usize::MAX,
                        _ => {
                            let data = vertices(&m.header, stream_index[*k], v.stride, &v.data)
                                .map_err(|e| format!("{}: {e}", m.shader))?;
                            self.buffer(gpu, &data, wgpu::BufferUsages::VERTEX)
                        }
                    });
                }
                vb.push(made);
            }
            let ib: Vec<usize> = m
                .index_buffers
                .iter()
                .map(|i| self.index_buffer(gpu, &i.indices))
                .collect();
            let mut streams = Vec::new();
            for (k, a) in m.stream_arrays.iter().enumerate() {
                let layout = layout(&m.header, stream_index[k])?;
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
                let layout = map
                    .iter()
                    .map(|s| streams.get(*s).map(|s: &Stream| s.layout.clone()))
                    .collect::<Option<Vec<_>>>();
                let layout = layout.ok_or_else(|| format!("{}: bad stream map", m.shader))?;
                // Streams of one kind continue each other's numbers.
                let layout = self.numbered(layout, &map, m);
                maps.insert(name, (map, self.intern(VertexLayout(layout))));
            }
            let find = |list: &[mesh::Blocks], kind: u32| -> Option<Vec<u8>> {
                let b = list.iter().find(|b| b.raw_id as usize == number)?;
                Some(b.blocks.iter().find(|b| b.0 == kind)?.1.clone())
            };
            let weights = match find(&a.blocks, blocks::WEIGHTS) {
                Some(d) => {
                    let weights = blocks::read_weights(&d)?;
                    let gaps: Vec<f64> = weights.gaps.iter().map(|g| f64::from(*g)).collect();
                    Some(Weighted {
                        weights,
                        times: Times::new(&gaps),
                    })
                }
                None => None,
            };
            let transfer = match find(&o.blocks, blocks::TRANSFER) {
                Some(d) => {
                    let t = blocks::read_transfer(&d).map_err(|e| format!("{}: {e}", m.shader))?;
                    let parts = m.header.streams.iter().flat_map(|s| &s.elements);
                    let vectors = parts
                        .filter(|e| e.source.starts_with("PrtPCAWeight"))
                        .count();
                    let h = Harmonics::new(t.order);
                    Some((t, h, vectors))
                }
                None => None,
            };
            let gaps: Vec<f64> = m
                .frame_keys
                .iter()
                .map(|k| f64::from(f32::from_bits(*k)))
                .collect();
            let total: f64 = gaps.iter().filter(|g| **g > 0.0).sum();
            meshes.push(Mesh {
                times: Times::new(&gaps),
                shapes_end: total as f32,
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
                slots: m.header.textures.iter().map(|t| t.0.clone()).collect(),
                weights,
                transfer,
            });
        }
        let center = Vec3::from([6, 7, 8].map(|i| f32_at(&o.bounds, i * 4)));
        self.models.push(Model {
            meshes,
            textures,
            center,
        });
        Ok(self.models.len() - 1)
    }

    /// The header stream that each stream array fills. Streams of several shapes have one
    /// description for all of them.
    fn stream_numbers(&self, m: &mesh::Mesh) -> Vec<usize> {
        let same = m.header.streams.len() == m.stream_arrays.len();
        let mut out = Vec::new();
        let mut next = 0;
        for (k, a) in m.stream_arrays.iter().enumerate() {
            if same {
                out.push(k);
                continue;
            }
            let frame = u32_at(&a.raw, 0) as i32;
            // Shapes after the first belong to the same stream.
            if k > 0 && frame <= 0 {
                next += 1;
            }
            out.push(next.min(m.header.streams.len() - 1));
        }
        out
    }

    /// Gives the k-th shape the semantics with number k.
    fn numbered(
        &self,
        layout: Vec<StreamLayout>,
        map: &[usize],
        m: &mesh::Mesh,
    ) -> Vec<StreamLayout> {
        let same = m.header.streams.len() == m.stream_arrays.len();
        if same {
            return layout;
        }
        layout
            .into_iter()
            .zip(map)
            .map(|(mut l, s)| {
                let frame = u32_at(&m.stream_arrays[*s].raw, 0) as i32;
                if frame > 0 {
                    l.elements.iter_mut().for_each(|e| e.1 = frame as u32);
                }
                l
            })
            .collect()
    }

    /// Runs the scripts' first functions, which make the buffers, and follows the show
    /// once to see how long it takes.
    fn prepare(&mut self, gpu: &Gpu, window: (u32, u32)) -> Result<()> {
        self.window = window;
        for c in 0..self.chapters.len() {
            self.reset(gpu, c);
            if self.chapters[c].jumps {
                let end = self.chapters[c].end();
                let mut t = 0.0;
                while self.show.script < end - 1e-3 && t < 600.0 {
                    t += 1.0 / 30.0;
                    self.skip(gpu, c, t);
                }
                self.chapters[c].duration = t;
            }
        }
        match self.warnings.is_empty() {
            true => Ok(()),
            false => Err(self
                .warnings
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
                .into()),
        }
    }

    /// Starts a chapter's scripts over.
    fn reset(&mut self, gpu: &Gpu, chapter: usize) {
        self.state = State::new();
        self.show = Show {
            chapter: Some(chapter),
            at: 0.0,
            script: 0.0,
        };
        for name in self.chapters[chapter].scripts.clone() {
            let mut state = std::mem::take(&mut self.state);
            let files = |n: &str, me: &Self| me.text(&format!("Scripts/{n}"));
            let mut all = HashMap::new();
            for n in [
                "ShadowMapInclude.sds",
                "TextureLightingInclude.sds",
                "ShaderParams.sds",
                name.as_str(),
            ] {
                if let Some(t) = files(n, self) {
                    all.insert(n.to_string(), t);
                }
            }
            let mut host = Frame::new(self, gpu, None, state.global("shaderParam"));
            let done = state.load(&mut host, &name, &|n| all.get(n).cloned());
            drop(host);
            self.state = state;
            if let Err(e) = done {
                self.warnings.insert(e);
            }
        }
    }

    /// Runs a frame's script without drawing.
    fn skip(&mut self, gpu: &Gpu, chapter: usize, at: f32) {
        self.show.script += at - self.show.at;
        self.show.at = at;
        self.call(gpu, chapter, None);
    }

    fn call(&mut self, gpu: &Gpu, chapter: usize, frame: Option<Drawing>) -> Option<Drawing> {
        let call = self.chapters[chapter].part(self.show.script).call.clone();
        let mut state = std::mem::take(&mut self.state);
        let mut host = Frame::new(self, gpu, frame, state.global("shaderParam"));
        let done = state.execute(&mut host, &call);
        let drawing = host.drawing.take();
        drop(host);
        self.state = state;
        if let Err(e) = done {
            self.warnings.insert(format!("{call}: {e}"));
        }
        drawing
    }

    /// Makes the buffers fit a window.
    fn fit(&mut self, gpu: &Gpu, window: (u32, u32)) {
        if self.window == window {
            return;
        }
        self.window = window;
        self.depth_buffers.clear();
        let chapter = self.show.chapter;
        for c in 0..self.chapters.len() {
            self.reset(gpu, c);
        }
        self.show = Show {
            chapter,
            ..Show::default()
        };
        self.show.chapter = None;
        self.pipelines.forget_textures();
    }

    /// Buffers made for 1024 by 768 keep their share of the window.
    fn scaled(&self, gpu: &Gpu, (w, h): (u32, u32)) -> (u32, u32) {
        let max = gpu.device.limits().max_texture_dimension_2d;
        let size = match w * 3 == h * 4 && 1024 % w == 0 {
            true => (self.window.0 * w / 1024, self.window.1 * h / 768),
            false => (w, h),
        };
        (size.0.clamp(1, max), size.1.clamp(1, max))
    }

    fn create_color(
        &mut self,
        gpu: &Gpu,
        name: &str,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        cube: bool,
    ) -> usize {
        let size = if cube { size } else { self.scaled(gpu, size) };
        let make = || match cube {
            true => ColorBuffer::cube(gpu, size.0, format),
            false => ColorBuffer::new(gpu, size, format, 1),
        };
        if let Some(id) = self.buffer_ids.get(name).copied() {
            let b = &mut self.targets[id];
            if b.size != size || b.format != format {
                b.color = make();
                (b.size, b.format) = (size, format);
                let view = b.color.resolved_view.clone();
                let texture = b.texture;
                self.textures.replace_view(texture, view);
                self.pipelines.forget_textures();
            }
            return id;
        }
        let color = make();
        let texture = self.textures.add_view(color.resolved_view.clone(), cube);
        self.buffer_ids.insert(name.into(), self.targets.len());
        self.targets.push(Buffer {
            size,
            format,
            color,
            texture,
        });
        self.targets.len() - 1
    }

    fn create_depth(&mut self, gpu: &Gpu, name: &str, size: (u32, u32)) -> usize {
        if let Some(id) = self.depth_ids.get(name) {
            return *id;
        }
        let read = self.depth_names.contains(name);
        let read = read.then(|| self.create_color(gpu, name, size, SHADOW_FORMAT, false));
        self.depth_ids.insert(name.into(), self.depths.len());
        self.depths.push(Depth { size, read });
        self.depths.len() - 1
    }

    fn size_of(&self, to: &Destination) -> (u32, u32) {
        match to.colors[0] {
            Some(Some(b)) => self.targets[b].color.size(),
            _ => to
                .depth
                .flatten()
                .map_or(self.window, |d| self.scaled_if_needed(self.depths[d].size)),
        }
    }

    fn scaled_if_needed(&self, size: (u32, u32)) -> (u32, u32) {
        let (w, h) = size;
        if w != 0 && w * 3 == h * 4 && 1024 % w == 0 {
            (
                (self.window.0 * w / 1024).max(1),
                (self.window.1 * h / 768).max(1),
            )
        } else {
            size
        }
    }

    fn formats(&self, to: &Destination) -> Formats {
        let back = to.colors[0] == Some(None);
        Formats {
            colors: to
                .colors
                .map(|c| c.map(|c| c.map_or(self.back_format, |c| self.targets[c].color.format()))),
            depth: to.depth.is_some(),
            samples: if back { SAMPLES } else { 1 },
        }
    }

    /// Draws a moment of every chapter. Browsers compile pipelines on first use, so wait
    /// for this work to finish before starting the show.
    pub fn warm_up(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        part: f32,
    ) {
        if let Some(first) = self.chapters.first() {
            self.draw(gpu, enc, targets, 0, first.duration() * part, None);
        }
    }

    /// The show's own camera.
    pub fn camera(&self, chapter: usize, time: f32) -> Camera {
        let c = &self.chapters[chapter];
        let t = match c.jumps {
            true => self.show.script,
            false => time,
        };
        let p = c.part(t);
        Camera::on_path(&p.path, t - p.start, self.fov)
    }

    /// Draws a moment of a chapter from its own camera, or from another. Returns the text
    /// to draw on top.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        chapter: usize,
        time: f32,
        camera: Option<&dyn Fn(&Camera) -> Camera>,
    ) -> Vec<Mark> {
        self.fit(gpu, targets.size());
        let chapter = chapter.min(self.chapters.len() - 1);
        let step = time - self.show.at;
        if self.show.chapter != Some(chapter) || !(0.0..=0.25).contains(&step) {
            self.reset(gpu, chapter);
            if self.chapters[chapter].jumps {
                let mut t = 1.0 / 30.0;
                while t < time {
                    self.skip(gpu, chapter, t);
                    t += 1.0 / 30.0;
                }
            }
        }
        self.show.script += time - self.show.at;
        self.show.at = time;
        let own = self.camera(chapter, time);
        let used = camera.map_or(own, |f| f(&own));
        self.arena.clear();
        self.free_buffers.append(&mut self.transient);
        let drawing = Drawing::new(self, chapter, used, camera.is_some());
        let drawing = self.call(gpu, chapter, Some(drawing));
        let Some(d) = drawing else { return Vec::new() };
        self.draw_calls = d.calls.len();
        self.arena.upload(gpu);
        self.pipelines.bind_uniforms(gpu, &self.arena);
        self.record(gpu, enc, targets, &d);
        d.marks
    }

    fn record(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        d: &Drawing,
    ) {
        // Depth buffers come in the size of what they are used with.
        for s in &d.steps {
            if let Step::Draw { to, .. } = s
                && let Some(depth) = to.depth
            {
                let key = (depth, self.size_of(to), self.formats(to).samples);
                self.depth_buffers
                    .entry(key)
                    .or_insert_with(|| DepthBuffer::new(gpu, key.1, key.2));
            }
        }
        for step in &d.steps {
            let (to, clears, calls) = match step {
                Step::Draw { to, clears, calls } => (to, clears, calls),
                Step::Copy(from, to) => {
                    self.copy(gpu, enc, targets, *from, *to);
                    continue;
                }
            };
            let color = |c: Option<usize>| {
                (
                    c.map_or(&targets.color, |c| &self.targets[c].color),
                    to.face,
                )
            };
            let formats = self.formats(to);
            let colors: Vec<_> = to.colors[..formats.colors().len()]
                .iter()
                .map(|c| c.map(color))
                .collect();
            let (w, h) = self.size_of(to);
            let depth = to
                .depth
                .map(|d| &self.depth_buffers[&(d, (w, h), formats.samples)]);
            let pixels = |r: [f32; 4]| {
                let (x0, x1) = (r[0] * w as f32, r[2] * w as f32);
                let (y0, y1) = ((1.0 - r[3]) * h as f32, (1.0 - r[1]) * h as f32);
                [x0, y0, x1, y1].map(|v| v.round().max(0.0))
            };
            let mut pass = targets::begin(enc, &colors, depth, *clears);
            for (area, range) in calls {
                let [x0, y0, x1, y1] = pixels(area.viewport);
                let (x1, y1) = (x1.min(w as f32), y1.min(h as f32));
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                pass.set_viewport(x0, y0, x1 - x0, y1 - y0, 0.0, 1.0);
                // D3D limits drawing to the viewport as well.
                let [a0, b0, a1, b1] = pixels(area.scissor);
                let (a0, b0) = (a0.max(x0) as u32, b0.max(y0) as u32);
                let (a1, b1) = (a1.min(x1) as u32, b1.min(y1) as u32);
                if a1 <= a0 || b1 <= b0 {
                    continue;
                }
                pass.set_scissor_rect(a0, b0, a1 - a0, b1 - b0);
                for c in &d.calls[range.clone()] {
                    self.pipelines.record(&mut pass, c, &self.buffers);
                }
            }
            // In WebGL the scissor also limits the resolve at the end of the pass.
            pass.set_scissor_rect(0, 0, w, h);
        }
    }

    /// `Blit` of whole buffers.
    fn copy(
        &mut self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        targets: &Targets,
        from: Option<usize>,
        to: Option<usize>,
    ) {
        let of = |c: Option<usize>| c.map_or(&targets.color, |c| &self.targets[c].color);
        let (source, target) = (of(from), of(to));
        if from == to || source.is_cube() || target.is_cube() {
            return;
        }
        let format = target.format();
        let samples = if to.is_none() { SAMPLES } else { 1 };
        let pipeline = self
            .copies
            .entry((format, samples))
            .or_insert_with(|| targets::copier(gpu, format, samples));
        targets::copy(gpu, enc, pipeline, source, target);
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

    /// Clears part of the targets by drawing over it.
    fn clear(
        &mut self,
        gpu: &Gpu,
        formats: Formats,
        depth: Option<f32>,
        stencil: Option<u32>,
        color: Option<[f32; 4]>,
        out: &mut Vec<DrawCall>,
    ) {
        use sushi_shader::{Compare, Cull, Stencil, StencilOp};
        let key = (depth.is_some(), color.is_some(), stencil, formats);
        let id = match self.clears.get(&key) {
            Some(id) => *id,
            None => {
                let replace = StencilOp::Replace;
                let mut pass = Pass::default();
                pass.z_func = Compare::Always;
                pass.z_write = depth.is_some();
                pass.cull = Cull::None;
                pass.stencil = stencil.map(|reference| Stencil {
                    func: Compare::Always,
                    reference,
                    zfail: replace,
                    pass: replace,
                    ccw_zfail: replace,
                    ccw_pass: replace,
                });
                pass.color_write = [color.is_some(); 4];
                pass.vs = Some("float4x4 m; struct I { float4 p : POSITION0; }; struct O { float4 p : POSITION0; }; O main(I i) { O o; o.p = mul(i.p, m); return o; }".into());
                pass.ps = Some("float4 c; float4 main() : COLOR { return c; }".into());
                let layout = &self.layouts[self.quad_layout];
                let setting = wgsl::Setting {
                    element: &|s, i| layout.find(s, i),
                    unfiltered: &|_| false,
                    sampler: &|_| None,
                    depth_only: false,
                };
                let module = wgsl::translate(&pass, &setting).expect("a fixed pass translates");
                let id = self
                    .pipelines
                    .create(gpu, "clear", &pass, module, layout, false, &formats);
                self.clears.insert(key, id);
                id
            }
        };
        let depth = Mat4::from_translation(Vec3::Z * depth.unwrap_or(0.0));
        let vertices = vec![self.quad_buffer(gpu, FULL, FULL)];
        out.push(DrawCall {
            pipeline: id,
            vs_offset: self.arena.push(&matrix_registers(depth * quad_matrix())),
            ps_offset: self.arena.push(&[color.unwrap_or_default()]),
            textures: Vec::new(),
            vertices,
            indices: self.indices(self.quad_indices),
            start: 0,
            count: 6,
        });
    }

    /// Finds the first shader with the property for a number of lights (0x445c20). If none
    /// exists for this many, fewer lights are used so the object still draws.
    fn variant(&self, file: usize, property: &str, lights: usize) -> Option<usize> {
        let shaders = &self.shaders[file].shaders;
        let tagged =
            |s: &sushi_shader::Shader, tag: &str| s.properties.iter().any(|p| p.starts_with(tag));
        let fits = |s: &sushi_shader::Shader| s.has(property);
        let ambient = (0..=lights).rev().find_map(|n| {
            let tag = format!("ObjAmb{n}");
            shaders.iter().position(|s| fits(s) && s.has(&tag))
        });
        ambient
            .or_else(|| {
                shaders
                    .iter()
                    .position(|s| fits(s) && !tagged(s, "ObjAmb") && s.has("FastRT0"))
            })
            .or_else(|| {
                shaders
                    .iter()
                    .position(|s| fits(s) && !tagged(s, "ObjAmb") && !tagged(s, "FastRT"))
            })
            .or_else(|| shaders.iter().position(fits))
    }

    fn pipeline(&mut self, gpu: &Gpu, key: PipelineKey) -> Option<PipelineId> {
        if let Some(p) = self.cache.get(&key) {
            return *p;
        }
        let (file, shader, pass, layout, mirrored, formats, depth_only) = key;
        let layout = &self.layouts[layout];
        let f = &self.shaders[file];
        let s = &f.shaders[shader];
        let p = &s.passes[pass];
        let reads_depth = |n: &str| {
            let t = f.textures.iter().find(|t| t.name == n);
            t.and_then(|t| t.target.as_ref())
                .is_some_and(|t| self.depth_names.contains(t))
        };
        let setting = wgsl::Setting {
            element: &|s, i| layout.find(s, i),
            unfiltered: &reads_depth,
            sampler: &|n| f.samplers.get(n).cloned(),
            depth_only,
        };
        let id = match wgsl::translate(p, &setting) {
            Ok(m) => Some(
                self.pipelines
                    .create(gpu, &s.name, p, m, layout, mirrored, &formats),
            ),
            Err(e) => {
                let name = &self.shader_names[file];
                self.warnings
                    .insert(format!("{name} {} pass {pass}: {e}", s.name));
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
        texture: &dyn Fn(&str) -> Option<TextureId>,
        geometry: (&[usize], usize, u32, u32),
        out: &mut Vec<DrawCall>,
    ) {
        let file = &self.shaders[key.0];
        let pass = &file.shaders[key.1].passes[key.2];
        let module = &self.pipelines.list[id].module;
        let mut vs = vec![[0.0; 4]; module.vs_consts as usize];
        fill(&mut vs, &module.vs, file, ctx);
        let mut ps = vec![[0.0; 4]; module.ps_consts as usize];
        fill(&mut ps, &module.ps, file, ctx);
        let mut textures = Vec::new();
        for (name, ..) in &module.samplers {
            let decl = file.textures.iter().find(|t| t.name == *name);
            let target = decl.and_then(|d| {
                if let Some(t) = &d.target {
                    return self.buffer_ids.get(t).copied();
                }
                let script = d.script.as_deref()?.trim_matches('"');
                let value = ctx.frame.script.get(script)?.first()?[0] as usize;
                value
                    .checked_sub(FIRST_COLOR)
                    .filter(|i| *i < self.targets.len())
            });
            if let Some(b) = target {
                // A buffer cannot be sampled while attached to the same pass.
                if ctx.eye.to.colors.contains(&Some(Some(b))) {
                    return;
                }
                textures.push(self.targets[b].texture);
                continue;
            }
            let made = match decl.and_then(|d| d.app.as_ref()) {
                Some(app) => self.pictures.get(app).copied(),
                // Included files name pictures that come with the first object using them.
                None => texture(name),
            };
            textures.push(made.unwrap_or(BLACK));
        }
        let (vertices, indices, start, count) = geometry;
        let samplers = |n: &str| {
            pass.samplers
                .get(n)
                .or_else(|| file.samplers.get(n))
                .cloned()
        };
        self.pipelines
            .prepare(gpu, &mut self.textures, id, &textures, &samplers);
        out.push(DrawCall {
            pipeline: id,
            vs_offset: self.arena.push(&vs),
            ps_offset: self.arena.push(&ps),
            textures,
            vertices: vertices.to_vec(),
            indices: self.indices(indices),
            start,
            count,
        });
    }
}

/// Everything recorded while a frame's script runs.
struct Drawing {
    set: usize,
    lights: Option<usize>,
    scene: usize,
    manual: bool,
    eye: Eye,
    /// The camera's projection for the current viewport.
    usual: Mat4,
    overridden: bool,
    frame: Moment,
    calls: Vec<DrawCall>,
    steps: Vec<Step>,
    marks: Vec<Mark>,
    area: Area,
    /// `Clear` keeps to the last viewport set, even after the target changes, when
    /// drawing has the whole target again (0x42f2e0, 0x42ec80).
    cleared: [f32; 4],
    clear_color: [f32; 4],
    clear_depth: f32,
    clear_stencil: u32,
    text_size: f32,
    text_color: [f32; 4],
    camera_overrides: bool,
}
fn upright(t: &sushi_formats::texture::Texture) -> Result<Vec<sushi_formats::texture::Level>> {
    let mut mips = t.decode()?.swap_remove(0);
    for l in &mut mips {
        let rows = l.rgba.chunks_exact(l.width * 4).rev().flatten().copied();
        l.rgba = rows.collect();
    }
    Ok(mips)
}

fn matrix_registers(m: Mat4) -> Vec<[f32; 4]> {
    m.transpose().to_cols_array_2d().to_vec()
}

fn fill(out: &mut [[f32; 4]], uniforms: &[Uniform], file: &ShaderFile, ctx: &Context) {
    for u in uniforms {
        let direct = file.vars.get(&u.name);
        let stride = u.registers as usize;
        let count = u.count.unwrap_or(1) as usize;
        let begin = u.at as usize;
        let dst = &mut out[begin..begin + count * stride];
        // Replicated declarations become individual engine parameters in .ssh files,
        // while the HLSL still addresses them as elements of one array.
        if count > 1 && direct.is_none() {
            let first = usize::from(!file.vars.contains_key(&format!("{}0", u.name)));
            if file.vars.contains_key(&format!("{}{first}", u.name)) {
                for (i, chunk) in dst.chunks_exact_mut(stride).enumerate() {
                    let name = format!("{}{}", u.name, first + i);
                    fill_value(chunk, &name, file, ctx, &[]);
                }
                continue;
            }
        }
        fill_value(dst, &u.name, file, ctx, &u.default);
    }
}

fn fill_value(out: &mut [[f32; 4]], name: &str, file: &ShaderFile, ctx: &Context, default: &[f32]) {
    let var = file.vars.get(name);
    let source = var.and_then(|v| v.source.as_deref());
    let edited = ctx.edited.iter().find(|v| v.name == name);
    let local = ctx
        .overrides
        .get(name)
        .or_else(|| ctx.params.get(name))
        .copied();
    let values = local
        .map(|v| vec![v])
        .or_else(|| {
            edited.filter(|v| !v.values.is_empty()).map(|v| {
                let i = v
                    .times
                    .partition_point(|t| *t <= ctx.time)
                    .saturating_sub(1);
                let n = if v.kind == 3 || v.kind == 7 { 4 } else { 1 };
                vec![std::array::from_fn(|j| {
                    v.values.get(i * n + j % n).copied().unwrap_or(0.0)
                })]
            })
        })
        .or_else(|| {
            source.and_then(|s| {
                if s.first()?.eq_ignore_ascii_case("ScriptVariable") {
                    ctx.frame.script.get(s.get(1)?.trim_matches('"')).cloned()
                } else {
                    builtin(s.first()?, &s[1..], ctx)
                }
            })
        })
        .or_else(|| builtin(name, &[], ctx))
        .unwrap_or_else(|| {
            let v = var
                .map(|v| v.default.as_slice())
                .filter(|v| !v.is_empty())
                .unwrap_or(default);
            (0..out.len().min(v.len().div_ceil(4)))
                .map(|r| std::array::from_fn(|j| v.get(r * 4 + j).copied().unwrap_or(0.0)))
                .collect()
        });
    for (dst, src) in out.iter_mut().zip(values) {
        *dst = if var.is_some_and(|v| v.kind == VarKind::Float) {
            [src[0]; 4]
        } else {
            src
        };
    }
}

fn builtin(name: &str, args: &[String], ctx: &Context) -> Option<Vec<[f32; 4]>> {
    let name_lc = name.trim_matches('"').to_ascii_lowercase();
    let idx = |prefix: &str| {
        name_lc
            .strip_prefix(prefix)
            .and_then(|s| s.parse::<usize>().ok())
    };
    let bone = |slot: usize| {
        let bones = if args.iter().any(|a| a.eq_ignore_ascii_case("lastFrame")) {
            ctx.bones_before
        } else {
            ctx.bones
        };
        ctx.slots.get(slot).and_then(|b| bones.get(*b))
    };
    let world = |slot| bone(slot).map_or(Mat4::IDENTITY, |p| p.matrix);
    let named = |arg: &str| {
        ctx.lights
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(arg.trim_matches('"')))
            .copied()
    };
    if let Some(kind) = name_lc.strip_prefix("objectambient") {
        let n = kind.trim_end_matches(|c: char| c.is_ascii_digit());
        let light = args.first().and_then(|a| named(a)).or_else(|| {
            let index = kind[n.len()..]
                .parse::<usize>()
                .unwrap_or(1)
                .saturating_sub(1);
            ctx.lights.get(index).copied()
        });
        if n == "viewprojmatrix" {
            return Some(matrix_registers(
                light.map_or(Mat4::IDENTITY, Light::texture_matrix),
            ));
        }
        return Some(vec![light.map_or([0.0; 4], |l| match n {
            "pos" => l.place(),
            "vec" => l.vector(),
            "param" | "params" => l.params(),
            "lightcomponents" => l.components(),
            "diffusecolor" => l.diffuse_color(),
            "specularcolor" => l.specular_color(),
            _ => l.shade(),
        })]);
    }
    if let Some(n) = name_lc
        .strip_prefix("world")
        .and_then(|s| s.strip_suffix("mat"))
        .and_then(|s| s.parse::<usize>().ok())
    {
        return Some(matrix_registers(world(n)));
    }
    let slot = args
        .first()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    let eye = ctx.eye;
    let vector = |v: Vec3, w| vec![v.extend(w).to_array()];
    Some(match name_lc.as_str() {
        "wvp" | "vp" => matrix_registers(eye.view_proj),
        "w" => matrix_registers(Mat4::IDENTITY),
        "view" | "v" => matrix_registers(eye.view),
        "projection" | "proj" | "p" => matrix_registers(eye.projection),
        "viewinverse" | "viewinv" => matrix_registers(eye.view.inverse()),
        "previousviewprojectionmatrix" | "prevwvp" | "previouswvp" | "prevvp" => {
            matrix_registers(ctx.frame.view_before)
        }
        "prevframeviewmatrix" => matrix_registers(ctx.frame.camera_view_before),
        "screenprojectionmatrix" => matrix_registers(Light::texturing(eye.view_proj)),
        "cameraposition" => vector(eye.camera.position, 1.0),
        "cameraaxisx" => vector(eye.view.inverse().x_axis.truncate(), 0.0),
        "cameraaxisy" => vector(eye.view.inverse().y_axis.truncate(), 0.0),
        "cameraaxisz" => vector(eye.view.inverse().z_axis.truncate(), 0.0),
        "time" | "currenttime" => vec![[ctx.frame.time; 4]],
        "objecttime" => vec![[ctx.time; 4]],
        "dofparams" => vec![ctx.frame.focus],
        "morphlerp0" => vec![[1.0 - ctx.tween, ctx.tween, 0.0, 0.0]],
        "worldmat" => matrix_registers(world(slot)),
        "worldmatrow1" | "worldmatrow2" | "worldmatrow3" => {
            let i = name_lc.as_bytes().last().copied().unwrap_or(b'1') - b'1';
            vec![world(slot).transpose().to_cols_array_2d()[usize::from(i)]]
        }
        "visibility" => vec![[bone(slot).map_or(1.0, |b| b.visible); 4]],
        "visibilityframe" => vec![bone(slot).map_or([1.0, 0.0, 0.0, 0.0], |b| {
            [b.visible, b.frame, b.frames, 0.0]
        })],
        "solobjectcenter" => vector(ctx.center, 1.0),
        "solorientation" | "solorientationx" => matrix_registers(world(0)),
        "solvelocity" | "solvelocityx" => {
            let previous = ctx
                .bones_before
                .first()
                .map_or(Mat4::IDENTITY, |b| b.matrix);
            vector(
                (world(0).transform_point3(ctx.center) - previous.transform_point3(ctx.center))
                    / BEFORE,
                0.0,
            )
        }
        "screenres" | "screensize" => {
            let (w, h) = (ctx.frame.window.0 as f32, ctx.frame.window.1 as f32);
            vec![[w, h, w.recip(), h.recip()]]
        }
        "psbillboardscalefactor" => {
            vec![[eye.projection.x_axis.x, -eye.projection.y_axis.y, 0.0, 0.0]]
        }
        "pstextile" | "particletexturescale" => vec![[ctx.tile[0], ctx.tile[1], 0.0, 0.0]],
        "prtcpcabuffer" => {
            let i = args.get(1)?.parse::<usize>().ok()?;
            let scale = args
                .windows(2)
                .find(|s| s[0].eq_ignore_ascii_case("Scale"))
                .and_then(|s| s[1].parse::<f32>().ok())
                .unwrap_or(1.0);
            vec![ctx.around?.1.get(i)?.map(|v| v * scale)]
        }
        "irradiancesamplepos" => vector(ctx.around?.0.position, 1.0),
        "irradiancesample" | "irradiancegradient" => {
            let index = args.get(1)?.parse::<usize>().ok()?;
            let color = match args.get(2)?.to_ascii_lowercase().as_str() {
                "red" => 0,
                "green" => 1,
                "blue" => 2,
                _ => return None,
            };
            let light = &ctx.around?.0;
            if name_lc == "irradiancesample" {
                vec![std::array::from_fn(|c| {
                    light.colors[color]
                        .get(index * 4 + c)
                        .copied()
                        .unwrap_or(0.0)
                })]
            } else {
                let slopes = light.slopes.as_ref()?;
                vec![std::array::from_fn(|axis| {
                    slopes
                        .get(axis)
                        .and_then(|v| v[color].get(index))
                        .copied()
                        .unwrap_or(0.0)
                })]
            }
        }
        "lightagentviewprojmatrix" | "lightkingpinviewprojmatrix" => {
            let (point, radius) = *ctx.frame.toward.get(slot)?;
            let position = ctx.frame.key?;
            let distance = position.distance(point).max(radius + 0.1);
            let view = look(position, point - position, Vec3::Y);
            let scale = args
                .get(1)
                .and_then(|s| s.trim_end_matches('f').parse::<f32>().ok())
                .unwrap_or(1.0);
            let fov = (2.0 * (radius * scale / distance).atan()).clamp(0.01, 3.0);
            matrix_registers(Light::texturing(
                projection(fov.to_degrees(), 0.1, distance + radius + 1.0) * view,
            ))
        }
        _ if idx("morphweight").is_some() => {
            let n = idx("morphweight").unwrap();
            vec![[ctx.weights.get(n).map_or(0.0, |w| w.1); 4]]
        }
        _ => return None,
    })
}

impl Drawing {
    fn new(demo: &Demo, chapter: usize, camera: Camera, manual: bool) -> Self {
        let c = &demo.chapters[chapter];
        let time = demo.show.script;
        let part = c.part(time);
        let old = Camera::on_path(&part.path, (time - part.start - BEFORE).max(0.0), demo.fov);
        let aspect = demo.window.0 as f32 / demo.window.1 as f32;
        let usual = demo.lens.projection(camera.fov, aspect, aspect);
        let view = camera.view();
        let scene = part.lights.unwrap_or(0);
        let key = demo.scenes[scene]
            .lights
            .iter()
            .map(|light| light.at(time))
            .find(|light| light.name.to_ascii_lowercase().contains("key"))
            .map(|l| l.position);
        let toward = ["girl", "kingpin"]
            .map(|name| {
                demo.sets[part.set]
                    .instances
                    .iter()
                    .find(|i| {
                        i.name.to_ascii_lowercase().contains(name)
                            && time >= i.start
                            && time < i.end
                    })
                    .and_then(|i| i.clip.bounds(time - i.start))
                    .map_or((Vec3::ZERO, 1.0), |(_, _, center, radius)| (center, radius))
            })
            .to_vec();
        let frame = Moment {
            time,
            key,
            toward,
            window: demo.window,
            view_before: demo.lens.projection(old.fov, aspect, aspect) * old.view(),
            camera_view_before: old.view(),
            focus: demo.lens.focus(&camera),
            ..Default::default()
        };
        Self {
            set: part.set,
            lights: part.lights,
            scene: 0,
            manual,
            eye: Eye {
                camera,
                view,
                projection: usual,
                view_proj: usual * view,
                mirror: None,
                to: BACK,
                formats: demo.formats(&BACK),
                depth_only: false,
            },
            usual,
            overridden: false,
            frame,
            calls: Vec::new(),
            steps: vec![Step::Draw {
                to: BACK,
                clears: Clears::all(wgpu::Color::BLACK),
                calls: Vec::new(),
            }],
            marks: Vec::new(),
            area: Area {
                viewport: FULL,
                scissor: FULL,
            },
            cleared: FULL,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            clear_depth: 1.0,
            clear_stencil: 128,
            text_size: 0.04,
            text_color: [1.0; 4],
            camera_overrides: true,
        }
    }

    fn target(&mut self, demo: &Demo, to: Destination, depth_only: bool) {
        if self.eye.to != to
            || !matches!(self.steps.last(), Some(Step::Draw { to: last, .. }) if *last == to)
        {
            self.steps.push(Step::Draw {
                to,
                clears: Clears::default(),
                calls: Vec::new(),
            });
        }
        self.eye.to = to;
        self.eye.formats = demo.formats(&to);
        self.eye.depth_only = depth_only;
        self.area.viewport = FULL;
        self.area.scissor = FULL;
    }

    fn append(&mut self, start: usize) {
        if self.calls.len() == start {
            return;
        }
        if let Some(Step::Draw { to, calls, .. }) = self.steps.last_mut()
            && *to == self.eye.to
        {
            calls.push((self.area, start..self.calls.len()));
        } else {
            self.steps.push(Step::Draw {
                to: self.eye.to,
                clears: Clears::default(),
                calls: vec![(self.area, start..self.calls.len())],
            });
        }
    }

    fn clear(&mut self, demo: &mut Demo, gpu: &Gpu, mask: u32) {
        let color = if self.eye.depth_only && mask & Z != 0 {
            Some([self.clear_depth; 4])
        } else {
            (mask & COLOR != 0).then_some(self.clear_color)
        };
        let depth = (mask & Z != 0 && self.eye.to.depth.is_some()).then_some(self.clear_depth);
        let stencil =
            (mask & STENCIL != 0 && self.eye.to.depth.is_some()).then_some(self.clear_stencil);
        if self.cleared == FULL {
            let c = color.map(|[r, g, b, a]| wgpu::Color {
                r: r.into(),
                g: g.into(),
                b: b.into(),
                a: a.into(),
            });
            let clears = Clears {
                color: [c; 4],
                depth,
                stencil,
            };
            self.steps.push(Step::Draw {
                to: self.eye.to,
                clears,
                calls: Vec::new(),
            });
        } else {
            let start = self.calls.len();
            demo.clear(
                gpu,
                self.eye.formats,
                depth,
                stencil,
                color,
                &mut self.calls,
            );
            let old = self.area;
            self.area = Area {
                viewport: self.cleared,
                scissor: self.cleared,
            };
            self.append(start);
            self.area = old;
        }
    }
}

const COLOR: u32 = 1;
const Z: u32 = 2;
const STENCIL: u32 = 4;
const OPAQUE: u32 = 8;
const TRANS: u32 = 16;
const RECEIVE: u32 = 32;
const NOT_RECEIVE: u32 = 64;
const FORCE: u32 = 128;
const OBJECTS: u32 = 256;

struct Frame<'a> {
    demo: &'a mut Demo,
    gpu: &'a Gpu,
    drawing: Option<Drawing>,
    params: Value,
}

impl<'a> Frame<'a> {
    fn new(demo: &'a mut Demo, gpu: &'a Gpu, drawing: Option<Drawing>, params: Value) -> Self {
        Self {
            demo,
            gpu,
            drawing,
            params,
        }
    }

    fn lights(&self) -> Vec<(usize, Light)> {
        let Some(d) = &self.drawing else {
            return Vec::new();
        };
        let id = d.lights.unwrap_or(d.scene);
        self.demo
            .scenes
            .get(id)
            .map(|s| {
                s.lights
                    .iter()
                    .enumerate()
                    .map(|(i, l)| ((id << 16) | i, l.at(self.demo.show.script)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn light_at(&self, id: usize) -> Option<Light> {
        Some(
            self.demo
                .scenes
                .get(id >> 16)?
                .lights
                .get(id & 65535)?
                .at(self.demo.show.script),
        )
    }

    fn color_id(&self, n: usize) -> Option<Option<usize>> {
        if n == 0 {
            Some(None)
        } else {
            n.checked_sub(FIRST_COLOR)
                .filter(|i| *i < self.demo.targets.len())
                .map(Some)
        }
    }

    fn bounds(&self, name: &str) -> Option<(Vec3, f32)> {
        let d = self.drawing.as_ref()?;
        let i = self.demo.sets[d.set]
            .instances
            .iter()
            .find(|i| i.name.eq_ignore_ascii_case(name))?;
        let t = self.demo.show.script - i.start;
        i.clip.bounds(t).map(|(_, _, c, r)| (c, r)).or_else(|| {
            Some((
                i.clip
                    .bone(0, t)
                    .transform_point3(self.demo.models[i.model].center),
                0.0,
            ))
        })
    }

    fn sync(&self, drawing: &mut Drawing) {
        drawing.frame.script = self.params.shader_values();
    }
}

impl lua::Host for Frame<'_> {
    fn has(&self, name: &str) -> bool {
        matches!(
            name,
            "CreateColorBuffer"
                | "CreateRenderableCubemap"
                | "CreateZBuffer"
                | "GetScreenWidth"
                | "GetScreenHeight"
                | "GetCurrentTime"
                | "SetCurrentTime"
                | "GetCurrentAnimationFrame"
                | "GetCurrentCameraPosition"
                | "IsManualCameraCurrent"
                | "SetCurrentScene"
                | "SetRenderTarget"
                | "SetCubemapRenderTarget"
                | "SetViewport"
                | "SetViewportAbsolute"
                | "SetScissor"
                | "SetClearColor"
                | "SetClearZ"
                | "SetClearStencil"
                | "Clear"
                | "Blit"
                | "SetProjectionBasedOnViewport"
                | "EnableOverrideCamera"
                | "DisableOverrideCamera"
                | "SetWorldSpaceCubemapCamera"
                | "SetSpotLightCamera"
                | "DrawScene"
                | "DrawObjects"
                | "DrawParticles"
                | "DrawReflections"
                | "DrawQuad"
                | "DrawNonSceneQuad"
                | "DrawText"
                | "DrawOutlineAroundViewport"
                | "SetTextSize"
                | "SetTextColor"
                | "SetTextShader"
                | "GetBoundingSphereCenter"
                | "GetBoundingSphereRadius"
                | "IsObjectInViewFrustum"
                | "SetOverrideWireframeShader"
                | "RestartAudio"
        )
    }

    fn global(&self, name: &str) -> Option<Value> {
        if name == "TRUE" || name == "FALSE" {
            return Some(Value::Bool(name == "TRUE"));
        }
        if name == "currentLights" {
            let t = Value::table();
            for (id, light) in self.lights() {
                t.set(&light.name, Value::Light(id));
            }
            return Some(t);
        }
        Some(Value::Number(f64::from(match name {
            "Color" => COLOR,
            "Z" => Z,
            "Stencil" => STENCIL,
            "Opaque" => OPAQUE,
            "Trans" => TRANS,
            "Receive" => RECEIVE,
            "NotReceive" => NOT_RECEIVE,
            "ForceDraw" => FORCE,
            "Objects" => OBJECTS,
            "BUFFER_FORMAT_RGBA" => 1,
            "BUFFER_FORMAT_16161616" => 2,
            "BUFFER_FORMAT_1010102" => 3,
            "backBuffer" | "CubemapFacePosX" | "AlignCenterX" => 0,
            "AutoMipgen" => 1,
            "NoZBuffer" | "NoColorBuffer" | "noBuffer" | "noZBuffer" | "noColorBuffer" => u32::MAX,
            _ => return None,
        })))
    }

    fn light(&mut self, id: usize, method: &str, args: &[Value]) -> lua::Result<Value> {
        let l = self.light_at(id).ok_or("no such light")?;
        let f = |n: usize, default| {
            args.get(n)
                .and_then(Value::number)
                .map_or(default, |v| v as f32)
        };
        Ok(match method {
            "GetConeAngle" => Value::Number(l.cone.into()),
            "GetViewProjectionMatrixForTexturing" => Value::Matrix(Light::texturing(
                l.projection(f(0, 1.0), f(1, 10000.0), f(2, 1.0)) * l.view(),
            )),
            _ => return Err(format!("unknown light method {method}")),
        })
    }

    fn call(&mut self, name: &str, args: &[Value]) -> lua::Result<Value> {
        let num = |i: usize| args.get(i).and_then(Value::number).unwrap_or(0.0);
        let f = |i| num(i) as f32;
        let n = |i| num(i) as usize;
        let string = |i| match args.get(i) {
            Some(Value::Text(s)) => s.as_ref(),
            _ => "",
        };
        let rect = |i| std::array::from_fn(|j| f(i + j));
        let flags = |i| {
            args.iter()
                .skip(i)
                .filter_map(Value::number)
                .fold(0, |a, n| a | n as u32)
        };
        let vec3 = |i| match args.get(i) {
            Some(Value::Vector(v, _)) => Vec4::from_array(*v.borrow()).truncate(),
            _ => Vec3::ZERO,
        };
        let number = |x: usize| Ok(Value::Number(x as f64));
        match name {
            "GetScreenWidth" => return number(1024),
            "GetScreenHeight" => return number(768),
            "GetCurrentTime" => return Ok(Value::Number(self.demo.show.script.into())),
            "GetCurrentAnimationFrame" => {
                return Ok(Value::Number(
                    (1.0 + (self.demo.show.script * 30.0).floor()).into(),
                ));
            }
            "SetCurrentTime" => {
                self.demo.show.script = f(0);
                return Ok(Value::Nil);
            }
            "CreateColorBuffer" | "CreateRenderableCubemap" => {
                let cube = name == "CreateRenderableCubemap";
                let format = match n(if cube { 2 } else { 3 }) {
                    2 => DEEP_FORMAT,
                    3 => TEN_FORMAT,
                    _ => COLOR_FORMAT,
                };
                let size = (
                    n(1).max(1) as u32,
                    n(if cube { 1 } else { 2 }).max(1) as u32,
                );
                return number(
                    FIRST_COLOR
                        + self
                            .demo
                            .create_color(self.gpu, string(0), size, format, cube),
                );
            }
            "CreateZBuffer" => {
                return number(
                    FIRST_DEPTH
                        + self.demo.create_depth(
                            self.gpu,
                            string(0),
                            (n(1).max(1) as u32, n(2).max(1) as u32),
                        ),
                );
            }
            "GetBoundingSphereCenter" => {
                let center = self.bounds(string(0)).map_or(Vec3::ZERO, |b| b.0);
                return Ok(Value::vector(center.extend(0.0).to_array(), 3));
            }
            "GetBoundingSphereRadius" => {
                return Ok(Value::Number(
                    self.bounds(string(0)).map_or(0.0, |b| b.1).into(),
                ));
            }
            "IsObjectInViewFrustum" => {
                return Ok(Value::Bool(self.bounds(string(0)).is_some_and(|(p, r)| {
                    self.drawing.as_ref().is_some_and(|d| {
                        let p = d.eye.view_proj * p.extend(1.0);
                        p.w + r > 0.0 && p.x.abs() < p.w + r && p.y.abs() < p.w + r
                    })
                })));
            }
            "IsManualCameraCurrent" => {
                return Ok(Value::Bool(self.drawing.as_ref().is_some_and(|d| d.manual)));
            }
            "GetCurrentCameraPosition" => {
                return Ok(Value::vector(
                    self.drawing
                        .as_ref()
                        .map_or(Vec3::ZERO, |d| d.eye.camera.position)
                        .extend(0.0)
                        .to_array(),
                    3,
                ));
            }
            _ => {}
        }
        let Some(mut d) = self.drawing.take() else {
            return Ok(Value::Nil);
        };
        match name {
            "SetCurrentScene" => d.scene = n(0),
            "SetRenderTarget" | "SetCubemapRenderTarget" => {
                let face = name == "SetCubemapRenderTarget";
                let depth = n(if face { 3 } else { 2 });
                let depth = if depth == 0 {
                    Some(None)
                } else {
                    depth
                        .checked_sub(FIRST_DEPTH)
                        .filter(|i| *i < self.demo.depths.len())
                        .map(Some)
                };
                let mut to = BACK;
                to.colors[0] = self.color_id(n(1));
                to.depth = depth;
                to.face = if face { n(2).min(5) } else { 0 };
                let shadow = depth.flatten().and_then(|i| self.demo.depths[i].read);
                let depth_only = shadow.is_some() || to.colors[0].is_none() && depth.is_some();
                if let Some(shadow) = shadow {
                    to.colors[0] = Some(Some(shadow));
                }
                d.target(self.demo, to, depth_only);
            }
            "SetViewport" | "SetViewportAbsolute" => {
                let mut r = rect(0);
                if name == "SetViewportAbsolute" {
                    let (w, h) = self.demo.size_of(&d.eye.to);
                    for (i, x) in r.iter_mut().enumerate() {
                        *x /= if i % 2 == 0 { w } else { h } as f32;
                    }
                }
                d.area.viewport = r;
                d.cleared = r;
            }
            "SetScissor" => d.area.scissor = rect(0),
            "SetClearColor" => d.clear_color = rect(0),
            "SetClearZ" => d.clear_depth = f(0).clamp(0.0, 1.0),
            "SetClearStencil" => d.clear_stencil = n(0) as u32,
            "Clear" => d.clear(self.demo, self.gpu, flags(0)),
            "Blit" => {
                if let (Some(from), Some(to)) = (self.color_id(n(0)), self.color_id(n(5))) {
                    d.steps.push(Step::Copy(from, to));
                }
            }
            "SetProjectionBasedOnViewport" => {
                let [x, y, r, t] = d.area.viewport;
                let window = self.demo.window.0 as f32 / self.demo.window.1 as f32;
                let aspect = window * ((r - x) / (t - y).max(0.01)).max(0.01);
                d.usual = self.demo.lens.projection(d.eye.camera.fov, aspect, window);
                if !d.overridden {
                    d.eye.aim(d.eye.camera.view(), d.usual);
                }
            }
            "EnableOverrideCamera" | "DisableOverrideCamera" => {
                d.camera_overrides = name == "EnableOverrideCamera";
                d.overridden = false;
                d.eye.aim(d.eye.camera.view(), d.usual);
            }
            "SetWorldSpaceCubemapCamera" if d.camera_overrides => {
                let pos = vec3(0);
                let (direction, up) = [
                    (Vec3::X, -Vec3::Y),
                    (-Vec3::X, -Vec3::Y),
                    (Vec3::Y, Vec3::Z),
                    (-Vec3::Y, -Vec3::Z),
                    (Vec3::Z, -Vec3::Y),
                    (-Vec3::Z, -Vec3::Y),
                ][n(1).min(5)];
                let p = projection(90.0, self.demo.lens.near, self.demo.lens.far);
                d.eye.aim(look(pos, direction, up), p);
                d.overridden = true;
            }
            "SetSpotLightCamera" if d.camera_overrides => {
                if let Some(Value::Light(id)) = args.first()
                    && let Some(l) = self.light_at(*id)
                {
                    d.eye.aim(l.view(), l.projection(f(1), f(2), f(3)));
                    d.overridden = true;
                }
            }
            "SetTextSize" => d.text_size = f(0),
            "SetTextColor" => d.text_color = rect(0),
            "DrawText" => d.marks.push(Mark::Text {
                x: f(0),
                y: f(1),
                size: d.text_size,
                color: d.text_color,
                text: string(2).into(),
                background: args.get(4).is_some_and(Value::truth),
            }),
            "DrawOutlineAroundViewport" => d.marks.push(Mark::Outline {
                viewport: d.area.viewport,
                color: [f(0), f(1), f(2)],
            }),
            "DrawScene" | "DrawObjects" | "DrawParticles" | "DrawReflections" | "DrawQuad"
            | "DrawNonSceneQuad" => {
                self.sync(&mut d);
                let start = d.calls.len();
                match name {
                    "DrawScene" => {
                        self.demo
                            .draw_scene(self.gpu, &mut d, string(0), flags(1), None)
                    }
                    "DrawObjects" => {
                        self.demo
                            .draw_objects(self.gpu, &mut d, string(0), flags(1), false)
                    }
                    "DrawParticles" => self.demo.draw_particles(self.gpu, &mut d, string(0)),
                    "DrawReflections" => self.demo.draw_reflection(
                        self.gpu,
                        &mut d,
                        string(0),
                        n(1) as u32,
                        flags(2) & OBJECTS != 0,
                    ),
                    "DrawQuad" => self
                        .demo
                        .draw_quad(self.gpu, &mut d, string(0), string(1), None),
                    _ => self
                        .demo
                        .draw_quad(self.gpu, &mut d, string(0), string(1), Some(rect(2))),
                }
                d.append(start);
            }
            _ => {}
        }
        self.drawing = Some(d);
        Ok(Value::Nil)
    }
}

impl Demo {
    fn draw_scene(
        &mut self,
        gpu: &Gpu,
        d: &mut Drawing,
        property: &str,
        flags: u32,
        mirror: Option<usize>,
    ) {
        let Some(scene) = self.scenes.get(d.scene) else {
            return;
        };
        let items = mirror
            .map_or(&scene.items, |m| &scene.mirrors[m].items)
            .clone();
        let draws = scene.draws.clone();
        let params = BTreeMap::new();
        let lights = self.scene_lights(d);
        let lit: Vec<_> = lights.iter().collect();
        let mut ctx = Context::new(&d.eye, &d.frame, d.frame.time, &params);
        ctx.lights = &lit;
        for i in items {
            let draw = &draws[i.draw];
            if flags & (OPAQUE | TRANS) != 0
                && flags & if draw.transparent { TRANS } else { OPAQUE } == 0
            {
                continue;
            }
            ctx.edited = &draw.params;
            let tex = |s: &str| {
                draw.slots
                    .iter()
                    .position(|t| t == s)
                    .and_then(|i| draw.textures[i])
            };
            let Some(shader) = self.variant(draw.shader, property, 0) else {
                continue;
            };
            for pass in 0..self.shaders[draw.shader].shaders[shader].passes.len() {
                let key = (
                    draw.shader,
                    shader,
                    pass,
                    draw.layout,
                    d.eye.mirror.is_some(),
                    d.eye.formats,
                    d.eye.depth_only,
                );
                if let Some(id) = self.pipeline(gpu, key) {
                    self.emit(
                        gpu,
                        &key,
                        id,
                        &ctx,
                        &tex,
                        (&[i.vertices], i.indices, i.start, i.count),
                        &mut d.calls,
                    );
                }
            }
        }
    }

    fn scene_lights(&self, d: &Drawing) -> Vec<Light> {
        self.scenes
            .get(d.lights.unwrap_or(d.scene))
            .map(|s| s.lights.iter().map(|l| l.at(d.frame.time)).collect())
            .unwrap_or_default()
    }

    fn draw_reflection(
        &mut self,
        gpu: &Gpu,
        d: &mut Drawing,
        property: &str,
        number: u32,
        objects: bool,
    ) {
        let Some((index, mirror)) = self.scenes.get(d.scene).and_then(|s| {
            s.mirrors
                .iter()
                .enumerate()
                .find(|(_, m)| m.number == number)
        }) else {
            return;
        };
        let old = d.eye.clone();
        let view = d.eye.view * mirror.matrix;
        let projection = camera::clipped(d.eye.projection, view, mirror.plane);
        d.eye.aim(view, projection);
        d.eye.mirror = Some(mirror.matrix);
        self.draw_scene(gpu, d, property, OPAQUE | TRANS, Some(index));
        if objects {
            self.draw_objects(
                gpu,
                d,
                property,
                OPAQUE | TRANS | RECEIVE | NOT_RECEIVE,
                true,
            );
        }
        d.eye = old;
    }

    fn draw_objects(
        &mut self,
        gpu: &Gpu,
        d: &mut Drawing,
        property: &str,
        flags: u32,
        reflect: bool,
    ) {
        let models = std::mem::take(&mut self.models);
        let instances = std::mem::take(&mut self.sets[d.set].instances);
        let all_lights = self.scene_lights(d);
        let mut active: Vec<_> = instances
            .iter()
            .filter(|i| {
                let t = d.frame.time;
                (if reflect { i.reflect } else { i.normal })
                    && t >= i.start
                    && t < i.end
                    && (flags & FORCE != 0 || i.clip.visible(t - i.start))
            })
            .collect();
        if flags & TRANS != 0 {
            active.sort_by(|a, b| {
                let distance = |i: &Instance| {
                    i.clip
                        .bounds(d.frame.time - i.start)
                        .map_or(0.0, |(_, _, c, _)| {
                            c.distance_squared(d.eye.camera.position)
                        })
                };
                distance(b).total_cmp(&distance(a))
            });
        }
        for i in active {
            let model = &models[i.model];
            let t = d.frame.time - i.start;
            let bones = i.clip.bones(t);
            let before = i.clip.bones((t - BEFORE).max(0.0));
            let params = i.clip.params(t);
            let (center, radius) = i
                .clip
                .bounds(t)
                .map_or((model.center, 0.0), |(_, _, c, r)| (c, r));
            let mut lights: Vec<_> = all_lights
                .iter()
                .filter(|l| l.object_ambient && l.shines_on(center, radius))
                .take(4)
                .collect();
            let light_count = lights.len();
            lights.extend(
                all_lights
                    .iter()
                    .filter(|l| !l.object_ambient || !l.shines_on(center, radius)),
            );
            for m in &model.meshes {
                let trans = m.flags & TRANSPARENT != 0;
                let receives = m.flags & RECEIVES != 0;
                if flags & STENCIL != 0 {
                    if m.flags & CASTS == 0 {
                        continue;
                    }
                } else if (flags & (OPAQUE | TRANS) != 0
                    && flags & if trans { TRANS } else { OPAQUE } == 0)
                    || (flags & (RECEIVE | NOT_RECEIVE) != 0
                        && flags & if receives { RECEIVE } else { NOT_RECEIVE } == 0)
                    || m.flags & HIDDEN != 0
                {
                    continue;
                }
                let Some(shader) = self.variant(m.shader, property, light_count) else {
                    continue;
                };
                let morph = m.weights.as_ref().map_or_else(Vec::new, |w| w.at(t));
                let around = m.transfer.as_ref().map(|(transfer, harmonics, vectors)| {
                    let world = bones
                        .first()
                        .map_or(Mat3::IDENTITY, |b| Mat3::from_mat4(b.matrix));
                    let light =
                        transfer::around(transfer, f64::from(d.frame.time), harmonics, world);
                    let buffer = transfer::cluster_buffer(transfer, *vectors, &light, 1.0);
                    (light, buffer)
                });
                let shape_time = if m.shapes_end > 0.0 {
                    t.rem_euclid(m.shapes_end)
                } else {
                    0.0
                };
                let (frame, tween) = m.times.locate(shape_time);
                for pass in 0..self.shaders[m.shader].shaders[shader].passes.len() {
                    let p = &self.shaders[m.shader].shaders[shader].passes[pass];
                    let Some((map, layout)) = m
                        .maps
                        .get(&p.stream_map)
                        .or_else(|| m.maps.values().next().filter(|_| p.stream_map.is_empty()))
                    else {
                        self.warnings
                            .insert(format!("{}: no stream map {:?}", m.name, p.stream_map));
                        continue;
                    };
                    let Some(stream) = map.first().and_then(|s| m.streams.get(*s)) else {
                        continue;
                    };
                    let Some(commands) = m.commands.get(stream.commands) else {
                        continue;
                    };
                    let key = (
                        m.shader,
                        shader,
                        pass,
                        *layout,
                        d.eye.mirror.is_some(),
                        d.eye.formats,
                        d.eye.depth_only,
                    );
                    let Some(id) = self.pipeline(gpu, key) else {
                        continue;
                    };
                    let mut slots = [0usize; 64];
                    let mut textures = vec![None; m.slots.len()];
                    let mut edited = BTreeMap::<usize, &Edited>::new();
                    let mut vertices = Vec::new();
                    let mut indices = None;
                    for &[op, a, b, _] in commands {
                        let (x, y) = (a as usize, b as usize);
                        match op {
                            0 if indices.is_some() && !vertices.is_empty() && b >= a => {
                                let edits: Vec<_> = edited.values().map(|e| (*e).clone()).collect();
                                let ctx = Context {
                                    eye: &d.eye,
                                    frame: &d.frame,
                                    time: t,
                                    lights: &lights,
                                    bones: &bones,
                                    bones_before: &before,
                                    slots: &slots,
                                    tween,
                                    weights: &morph,
                                    params: &params,
                                    overrides: &params,
                                    edited: &edits,
                                    around: around.as_ref(),
                                    center: model.center,
                                    tile: [1.0; 2],
                                };
                                let texture = |name: &str| {
                                    m.slots
                                        .iter()
                                        .position(|n| n == name)
                                        .and_then(|i| textures[i])
                                };
                                self.emit(
                                    gpu,
                                    &key,
                                    id,
                                    &ctx,
                                    &texture,
                                    (&vertices, indices.unwrap(), a, b - a + 1),
                                    &mut d.calls,
                                );
                            }
                            0 => {}
                            1 => {
                                if let Some(s) = slots.get_mut(x) {
                                    *s = y;
                                }
                            }
                            2 => {
                                if let Some(s) = textures.get_mut(x) {
                                    *s = model.textures.get(y).copied().flatten();
                                }
                            }
                            3 => {
                                if let Some((_, values)) = m.params.get(x)
                                    && let Some(v) = values.get(y)
                                {
                                    edited.insert(x, v);
                                }
                            }
                            4 => {
                                vertices.clear();
                                indices = stream.index_buffers.get(x).copied();
                                for stream in map.iter().map(|i| &m.streams[*i]) {
                                    let f = stream.frame.map_or(0, |f| {
                                        if morph.is_empty() {
                                            frame + f
                                        } else {
                                            morph.get(f).map_or(0, |w| w.0)
                                        }
                                    });
                                    let selected = stream
                                        .buffers
                                        .get(f)
                                        .and_then(|b| b.get(x))
                                        .copied()
                                        .filter(|b| *b != usize::MAX)
                                        .or_else(|| {
                                            stream
                                                .buffers
                                                .first()
                                                .and_then(|b| b.get(x))
                                                .copied()
                                                .filter(|b| *b != usize::MAX)
                                        });
                                    if let Some(v) = selected {
                                        vertices.push(v);
                                    } else {
                                        indices = None;
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        self.sets[d.set].instances = instances;
        self.models = models;
    }

    fn draw_quad(
        &mut self,
        gpu: &Gpu,
        d: &mut Drawing,
        name: &str,
        property: &str,
        position: Option<[f32; 4]>,
    ) {
        let (file, params, slots, textures) = if position.is_some() {
            let Ok(file) = self.shader(name) else { return };
            (file, Vec::new(), Vec::new(), Vec::new())
        } else {
            let Some(draw) = self
                .quads
                .quads
                .get(&name.to_ascii_lowercase())
                .and_then(|i| self.quads.draws.get(*i))
            else {
                self.warnings.insert(format!("no quad {name}"));
                return;
            };
            (
                draw.shader,
                draw.params.clone(),
                draw.slots.clone(),
                draw.textures.clone(),
            )
        };
        let Some(shader) = self.variant(file, property, 0) else {
            return;
        };
        let mut eye = d.eye.clone();
        eye.aim(Mat4::IDENTITY, quad_matrix());
        let empty = BTreeMap::new();
        let mut ctx = Context::new(&eye, &d.frame, d.frame.time, &empty);
        ctx.edited = &params;
        let texture = |name: &str| {
            slots
                .iter()
                .position(|s| s == name)
                .and_then(|i| textures[i])
        };
        let [a, b, c, e] = position.unwrap_or(FULL);
        let v = self.quad_buffer(gpu, [a.min(c), b.min(e), a.max(c), b.max(e)], FULL);
        for pass in 0..self.shaders[file].shaders[shader].passes.len() {
            let key = (
                file,
                shader,
                pass,
                self.quad_layout,
                false,
                eye.formats,
                eye.depth_only,
            );
            if let Some(id) = self.pipeline(gpu, key) {
                self.emit(
                    gpu,
                    &key,
                    id,
                    &ctx,
                    &texture,
                    (&[v], self.quad_indices, 0, 6),
                    &mut d.calls,
                );
            }
        }
    }

    fn draw_particles(&mut self, gpu: &Gpu, d: &mut Drawing, property: &str) {
        let mut systems = std::mem::take(&mut self.sets[d.set].particles);
        let params = BTreeMap::new();
        for p in &mut systems {
            if !p.system.go(d.frame.time) || p.system.particles.is_empty() {
                continue;
            }
            let Some(shader) = self.variant(p.shader, property, 0) else {
                continue;
            };
            let mut particles: Vec<_> = p.system.particles.iter().collect();
            if p.system.sort {
                let pos = d.eye.camera.position;
                particles.sort_by(|a, b| {
                    b.position
                        .distance_squared(pos)
                        .total_cmp(&a.position.distance_squared(pos))
                });
            }
            let mut indices = Vec::with_capacity(particles.len() * 6);
            for i in 0..particles.len() {
                let first = i as u32 * 4;
                indices.extend([0, 1, 2, 0, 2, 3].map(|n| first + n));
            }
            let streams: Vec<_> = self.layouts[p.layout]
                .0
                .iter()
                .zip(&p.elements)
                .map(|(layout, elements)| particle_vertices(&particles, layout, elements))
                .collect();
            let vertices: Vec<_> = streams
                .iter()
                .map(|bytes| self.buffer(gpu, bytes, wgpu::BufferUsages::VERTEX))
                .collect();
            let ib = self.index_buffer(gpu, &indices);
            self.transient.extend(vertices.iter().copied().chain([ib]));
            let texture = |n: &str| {
                p.textures
                    .iter()
                    .find(|(s, _)| s == n)
                    .and_then(|(_, t)| *t)
            };
            let pparams = p
                .system
                .params
                .iter()
                .map(|(s, v)| (s.clone(), [*v; 4]))
                .collect();
            let mut ctx = Context::new(&d.eye, &d.frame, d.frame.time - p.system.start, &params);
            ctx.params = &pparams;
            ctx.tile = p.system.tile;
            for pass in 0..self.shaders[p.shader].shaders[shader].passes.len() {
                let key = (
                    p.shader,
                    shader,
                    pass,
                    p.layout,
                    false,
                    d.eye.formats,
                    d.eye.depth_only,
                );
                if let Some(id) = self.pipeline(gpu, key) {
                    self.emit(
                        gpu,
                        &key,
                        id,
                        &ctx,
                        &texture,
                        (&vertices, ib, 0, indices.len() as u32),
                        &mut d.calls,
                    );
                }
            }
        }
        self.sets[d.set].particles = systems;
    }
}

fn look(position: Vec3, direction: Vec3, up: Vec3) -> Mat4 {
    let z = -direction.normalize_or_zero();
    let x = up.cross(z).normalize_or_zero();
    let y = z.cross(x);
    Mat4::from_rows(
        x.extend(-x.dot(position)),
        y.extend(-y.dot(position)),
        z.extend(-z.dot(position)),
        Vec4::W,
    )
}

fn projection(fov: f32, near: f32, far: f32) -> Mat4 {
    Lens {
        near,
        far,
        film: 0.0,
        blur: 0.0,
    }
    .projection(fov, 1.0, 1.0)
}

fn particle_vertices(
    particles: &[&psystem::Particle],
    stream: &StreamLayout,
    names: &[String],
) -> Vec<u8> {
    let corners = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let stride = stream.stride as usize;
    let mut bytes = Vec::with_capacity(particles.len() * stride * corners.len());
    for p in particles {
        for [u, v] in corners {
            let start = bytes.len();
            bytes.resize(start + stride, 0);
            for (name, (_, _, ty, offset)) in names.iter().zip(&stream.elements) {
                let value = match name.as_str() {
                    "PsColor" => p.color,
                    "PsPosition" => p.position.extend(1.0).to_array(),
                    "PsOldPosition" => (p.position - p.velocity * BEFORE).extend(1.0).to_array(),
                    "PsVelocity" => p.velocity.extend(0.0).to_array(),
                    "PsRotation" => p.rotation,
                    "PsTimeAndSize" => [p.age / p.life.max(0.001), p.age, p.life, p.size],
                    "PsTexFrameAndID" => [p.tex_frame, p.id as f32, 0.0, 0.0],
                    "PsTexCoord" => [u, v, 0.0, 0.0],
                    _ => [0.0; 4],
                };
                let out = &mut bytes[start + *offset as usize..];
                match ty {
                    ElementType::Color | ElementType::Ubyte4 => {
                        let rgba = value.map(|f| (f.clamp(0.0, 1.0) * 255.0).round() as u8);
                        out[..4].copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
                    }
                    ElementType::Short4n => out[..8].copy_from_slice(bytemuck::bytes_of(
                        &value.map(|f| (f.clamp(-1.0, 1.0) * 32767.0) as i16),
                    )),
                    _ => {
                        let width = match ty {
                            ElementType::Float1 => 1,
                            ElementType::Float2 => 2,
                            ElementType::Float3 => 3,
                            _ => 4,
                        };
                        out[..width * 4].copy_from_slice(bytemuck::cast_slice(&value[..width]));
                    }
                }
            }
        }
    }
    bytes
}
