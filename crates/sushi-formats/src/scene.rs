use crate::object::MATERIAL_MAGIC;
use crate::reader::*;
use crate::shader_header::*;
use crate::texture::*;

/// Compiled `.ati` file: `Scenes/*.ati` and `Globals.ati` (reader 0x40bba0,
/// writer 0x40b180). Record names are provisional where the meaning is not yet
/// recovered; `raw` always holds the complete on-disk record.
#[derive(Debug, Clone)]
pub struct Scene {
    pub header: Vec<u8>,
    pub materials98: Vec<Vec<u8>>,
    pub cells: Vec<Cell>,
    pub groups: Vec<Group>,
    pub list_be: Vec<u32>,
    pub list_c6: Vec<u32>,
    pub geometry: Vec<Geometry>,
    pub draws: Vec<Draw>,
    pub lights: Vec<Light>,
    pub textures: Vec<Texture>,
    pub shaders: Vec<Shader>,
    pub table_112: Vec<Vec<u8>>,
    pub table_18a: Vec<Vec<u8>>,
    pub table_192: Vec<Vec<u8>>,
    pub shaders_17a: Vec<(Vec<u8>, ShaderHeader)>,
    pub textures_182: Vec<Texture>,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub raw: Vec<u8>,
    pub list_ee: Vec<u32>,
    pub list_fe: Vec<u32>,
    pub list_106: Vec<u32>,
    pub table_10e: Vec<Vec<u8>>,
}

/// 0x14 record owning a list of 0x14 items.
#[derive(Debug, Clone)]
pub struct Group {
    pub raw: Vec<u8>,
    pub items: Vec<[u32; 5]>,
}

#[derive(Debug, Clone)]
pub struct Geometry {
    pub raw: Vec<u8>,
    pub chunk: Chunk,
    pub table_25: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Draw {
    pub raw: Vec<u8>,
    pub table_118: Vec<Vec<u8>>,
    pub table_120: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Light {
    pub raw: Vec<u8>,
    pub table_4b: Vec<Vec<u8>>,
    pub a: Visibility,
    pub b: Visibility,
    pub table_1b9: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Visibility {
    pub groups: Vec<Group>,
    pub list1: Vec<u32>,
    pub list2: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Shader {
    pub raw: Vec<u8>,
    pub header: ShaderHeader,
    pub table_88: Vec<Vec<u8>>,
}

/// Vertex/index chunk (reader 0x411520).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub raw: Vec<u8>,
    pub decls: Vec<(Vec<u8>, Vec<u8>)>,
    pub streams: Vec<ChunkStream>,
    pub indices: Vec<(Vec<u8>, Vec<u16>)>,
    pub maps: Vec<(Vec<u8>, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub struct ChunkStream {
    pub raw: Vec<u8>,
    pub elements: Vec<Vec<u8>>,
    /// Vertex buffers: 0x18 record and bytes.
    pub buffers: Vec<Vec<(Vec<u8>, Vec<u8>)>>,
}

fn owned(v: Vec<&[u8]>) -> Vec<Vec<u8>> {
    v.into_iter().map(<[u8]>::to_vec).collect()
}

/// Record counts the engine stores as signed bytes.
fn count8(b: u8) -> usize {
    (b as i8).max(0) as usize
}

fn table(r: &mut Reader, raw: &[u8], at: usize, stride: usize) -> Result<Vec<Vec<u8>>> {
    Ok(owned(r.records(count_at(raw, at), stride)?))
}

fn list(r: &mut Reader, raw: &[u8], at: usize) -> Result<Vec<u32>> {
    r.u32s(count_at(raw, at))
}

fn group_items(r: &mut Reader, recs: Vec<&[u8]>) -> Result<Vec<Group>> {
    recs.into_iter()
        .map(|raw| {
            let items = r.records(count_at(raw, 4), 0x14)?;
            let items = items
                .iter()
                .map(|b| [0, 4, 8, 12, 16].map(|o| u32_at(b, o)))
                .collect();
            Ok(Group {
                raw: raw.to_vec(),
                items,
            })
        })
        .collect()
}

fn visibility(r: &mut Reader, raw: &[u8], at: [usize; 3]) -> Result<Visibility> {
    let recs = r.records(count_at(raw, at[0]), 0x14)?;
    let (list1, list2) = (list(r, raw, at[1])?, list(r, raw, at[2])?);
    Ok(Visibility {
        groups: group_items(r, recs)?,
        list1,
        list2,
    })
}

fn read_chunk(r: &mut Reader) -> Result<Chunk> {
    let raw = r.bytes(0x20)?;
    let mut decls = Vec::new();
    for rec in r.records(count_at(raw, 0), 0x15)? {
        decls.push((rec.to_vec(), r.bytes(count8(rec[4]))?.to_vec()));
    }
    let mut streams = Vec::new();
    for rec in r.records(count_at(raw, 8), 0x46)? {
        let elements = table(r, rec, 0x2e, 0x18)?;
        let mut buffers = Vec::new();
        for sub in r.records(count_at(rec, 0x3a), 8)? {
            let mut bufs = Vec::new();
            for vb in r.records(count_at(sub, 0), 0x18)? {
                bufs.push((vb.to_vec(), r.bytes(count_at(vb, 4))?.to_vec()));
            }
            buffers.push(bufs);
        }
        streams.push(ChunkStream {
            raw: rec.to_vec(),
            elements,
            buffers,
        });
    }
    let mut indices = Vec::new();
    for rec in r.records(count_at(raw, 0x10), 0x10)? {
        indices.push((rec.to_vec(), r.u16s(count_at(rec, 0))?));
    }
    let mut maps = Vec::new();
    for rec in r.records(count_at(raw, 0x18), 6)? {
        maps.push((rec.to_vec(), r.bytes(count8(rec[1]))?.to_vec()));
    }
    Ok(Chunk {
        raw: raw.to_vec(),
        decls,
        streams,
        indices,
        maps,
    })
}

pub fn read_scene(data: &[u8]) -> Result<Scene> {
    let mut rd = Reader::new(data);
    let r = &mut rd;
    r.magic(0x20, "* ATI Sushi 2.0 File v19 *")?;
    r.magic(0x20, MATERIAL_MAGIC)?;
    let h = r.bytes(0x19a)?;
    let materials98 = table(r, h, 0x9a, 0x98)?;
    let cells = r.records(count_at(h, 0xa2), 0x11a)?;
    let groups = r.records(count_at(h, 0xb2), 0x14)?;
    let list_be = list(r, h, 0xbe)?;
    let list_c6 = list(r, h, 0xc6)?;
    let geometry = r.records(count_at(h, 0xce), 0x31)?;
    let draws = r.records(count_at(h, 0xda), 0x12c)?;
    let lights = r.records(count_at(h, 0xe2), 0x1c5)?;
    let textures = r.records(count_at(h, 0xea), TEXTURE_RECORD)?;
    let shaders = r.records(count_at(h, 0xf2), 0x90)?;
    for (at, what) in [(0xfa, "0x290"), (0x10a, "0x310")] {
        // Absent from both demo files; fail loudly rather than misparse.
        if count_at(h, at) != 0 {
            return r.bad(format!("unsupported {what} table"));
        }
    }
    let table_112 = table(r, h, 0x112, 0x24)?;
    let shaders_17a = r.records(count_at(h, 0x17a), 0x33a)?;
    let textures_182 = r.records(count_at(h, 0x182), TEXTURE_RECORD)?;
    let table_18a = table(r, h, 0x18a, 0x32c)?;
    let table_192 = table(r, h, 0x192, 0xd0)?;

    let mut out_cells = Vec::new();
    for raw in cells {
        out_cells.push(Cell {
            raw: raw.to_vec(),
            list_ee: list(r, raw, 0xee)?,
            list_fe: list(r, raw, 0xfe)?,
            list_106: list(r, raw, 0x106)?,
            table_10e: table(r, raw, 0x10e, 0x32c)?,
        });
    }
    let groups = group_items(r, groups)?;
    let mut out_geometry = Vec::new();
    for raw in geometry {
        let chunk = read_chunk(r)?;
        let table_25 = table(r, raw, 0x25, 0x32c)?;
        out_geometry.push(Geometry {
            raw: raw.to_vec(),
            chunk,
            table_25,
        });
    }
    let mut out_draws = Vec::new();
    for raw in draws {
        out_draws.push(Draw {
            raw: raw.to_vec(),
            table_118: table(r, raw, 0x118, 0x85)?,
            table_120: table(r, raw, 0x120, 0x32c)?,
        });
    }
    let mut out_lights = Vec::new();
    for raw in lights {
        out_lights.push(Light {
            raw: raw.to_vec(),
            table_4b: table(r, raw, 0x4b, 0x10)?,
            a: visibility(r, raw, [0x163, 0x16f, 0x177])?,
            b: visibility(r, raw, [0x17f, 0x18b, 0x193])?,
            table_1b9: table(r, raw, 0x1b9, 0x32c)?,
        });
    }
    let textures = textures
        .into_iter()
        .map(|t| read_texels(r, t))
        .collect::<Result<_>>()?;
    let mut out_shaders = Vec::new();
    for raw in shaders {
        let header = read_shader_header(r)?;
        out_shaders.push(Shader {
            raw: raw.to_vec(),
            header,
            table_88: table(r, raw, 0x88, 0x91)?,
        });
    }
    let mut out_17a = Vec::new();
    for raw in shaders_17a {
        out_17a.push((raw.to_vec(), read_shader_header(r)?));
    }
    let textures_182 = textures_182
        .into_iter()
        .map(|t| read_texels(r, t))
        .collect::<Result<_>>()?;
    let header = h.to_vec();
    rd.finish()?;
    Ok(Scene {
        header,
        materials98,
        cells: out_cells,
        groups,
        list_be,
        list_c6,
        geometry: out_geometry,
        draws: out_draws,
        lights: out_lights,
        textures,
        shaders: out_shaders,
        table_112,
        table_18a,
        table_192,
        shaders_17a: out_17a,
        textures_182,
    })
}

/// Vertex attribute (0x18 element record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute {
    pub semantic: Semantic,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semantic {
    Position,
    Normal,
    Uv,
    /// Baked vertex light, D3DCOLOR (BGRA).
    VertexLight,
    /// Material color, D3DCOLOR (BGRA).
    BaseColor,
    Other(u32),
}

impl ChunkStream {
    /// Attributes are packed in order; records hold sizes, not offsets.
    pub fn attributes(&self) -> Vec<Attribute> {
        let mut offset = 0;
        let mut attr = |e: &Vec<u8>| {
            let size = u32_at(e, 12);
            let semantic = match u32_at(e, 0) {
                1 => Semantic::Position,
                2 => Semantic::Normal,
                7 => Semantic::Uv,
                10 => Semantic::VertexLight,
                14 => Semantic::BaseColor,
                k => Semantic::Other(k),
            };
            let a = Attribute {
                semantic,
                offset,
                size,
            };
            offset += size;
            a
        };
        self.elements.iter().map(&mut attr).collect()
    }
    pub fn stride(&self) -> u32 {
        u32_at(&self.raw, 0x42)
    }
    /// Index buffer paired with vertex buffer `i` (record +0x14).
    pub fn index_buffer_of(&self, i: usize) -> Option<usize> {
        self.buffers
            .first()?
            .get(i)
            .map(|b| u32_at(&b.0, 0x14) as usize)
    }
}

impl Geometry {
    pub fn transparent(&self) -> bool {
        self.raw[0] == 1
    }
    pub fn shader(&self) -> usize {
        u32_at(&self.raw, 1) as usize
    }
}

impl Draw {
    pub fn geometry(&self) -> usize {
        u32_at(&self.raw, 0) as usize
    }
    /// Scene texture index bound to shader texture slot `slot`.
    pub fn texture(&self, slot: usize) -> Option<usize> {
        assert!(slot < 64);
        usize::try_from(u32_at(&self.raw, 4 + slot * 4) as i32).ok()
    }
    /// FastRT lights affecting this draw, in shader order.
    pub fn lights(&self) -> Vec<usize> {
        let n = count_at(&self.raw, 0x104).min(4);
        (0..n)
            .filter_map(|i| usize::try_from(u32_at(&self.raw, 0x108 + i * 4) as i32).ok())
            .collect()
    }
}

/// One indexed triangle list inside a draw's geometry.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub draw: usize,
    pub vertex_buffer: usize,
    pub start: u32,
    pub count: u32,
}

impl Group {
    pub fn draw_items(&self) -> impl Iterator<Item = Item> + '_ {
        self.items.iter().map(|i| Item {
            draw: i[0] as usize,
            vertex_buffer: i[2] as usize,
            start: i[3],
            count: i[4],
        })
    }
}

impl Light {
    /// Projects through a spotlight texture and is applied per pixel (+0x2b).
    pub fn is_fast_rt(&self) -> bool {
        self.raw[0x2b] == 1
    }
    /// Uses the object-ambient path (+0x2a).
    pub fn is_object_ambient(&self) -> bool {
        self.raw[0x2a] == 1
    }
    pub fn color(&self) -> [f32; 3] {
        [0x36, 0x3a, 0x3e].map(|o| f32_at(&self.raw, o))
    }
    pub fn position(&self) -> [f32; 3] {
        [0x5f, 0x63, 0x67].map(|o| f32_at(&self.raw, o))
    }
    pub fn texture(&self) -> Option<usize> {
        usize::try_from(u32_at(&self.raw, 0x15f) as i32).ok()
    }
    /// `FastRTPosMatrix`: position with 1/(+0x32), then the light-space axes,
    /// the first with 1/(+0x6b) (0x4265f0).
    pub fn fast_rt_matrix(&self) -> [[f32; 4]; 4] {
        let row = |o: usize, w: f32| {
            let f = |k| f32_at(&self.raw, o + k);
            [f(0), f(4), f(8), w]
        };
        let inv = |o| 1.0 / f32_at(&self.raw, o);
        [
            row(0x5f, inv(0x32)),
            row(0x6f, inv(0x6b)),
            row(0x7f, 0.0),
            row(0x8f, 0.0),
        ]
    }
}
