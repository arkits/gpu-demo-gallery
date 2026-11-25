use crate::reader::*;
use crate::scene::Edited;
use crate::shader_header::*;

/// One drawable of a `.so2` object (200-byte record, reader 0x4366c0).
#[derive(Debug, Clone)]
pub struct Mesh {
    pub raw: Vec<u8>,
    pub shader: String,
    pub header: ShaderHeader,
    pub frame_keys: Vec<u32>,
    /// `frames[frame][stream]`.
    pub frames: Vec<Vec<VertexBuffer>>,
    pub index_buffers: Vec<IndexBuffer>,
    /// Command buffers: lists of 16-byte commands.
    pub commands: Vec<Vec<[u32; 4]>>,
    pub stream_arrays: Vec<StreamArray>,
    pub stream_maps: Vec<Vec<u32>>,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone)]
pub struct VertexBuffer {
    pub raw: Vec<u8>,
    pub stride: usize,
    pub count: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct IndexBuffer {
    pub raw: Vec<u8>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct StreamArray {
    pub raw: Vec<u8>,
    pub vertex_buffers: Vec<u32>,
    pub offsets: Vec<u32>,
    pub tail: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub raw: Vec<u8>,
    pub name: String,
    pub values: Vec<Edited>,
}

pub fn read_meshes(r: &mut Reader) -> Result<Vec<Mesh>> {
    let n = r.count()?;
    (0..n).map(|_| read_mesh(r)).collect()
}

fn read_mesh(r: &mut Reader) -> Result<Mesh> {
    let raw = r.bytes(200)?;
    let header = read_shader_header(r)?;
    let (nframes, nstreams) = (count_at(raw, 0x8c), count_at(raw, 0x98));
    let frame_keys = r.u32s(nframes)?;
    let mut frames = Vec::new();
    for _ in 0..nframes {
        let recs = r.records(nstreams, 0x18)?;
        let mut vbs = Vec::new();
        for rec in recs {
            let (stride, count) = (u32_at(rec, 8) as usize, count_at(rec, 12));
            let Some(size) = stride.checked_mul(count) else {
                return r.bad("vertex buffer size overflow");
            };
            vbs.push(VertexBuffer {
                raw: rec.to_vec(),
                stride,
                count,
                data: r.bytes(size)?.to_vec(),
            });
        }
        frames.push(vbs);
    }
    let mut index_buffers = Vec::new();
    for rec in r.records(count_at(raw, 0xa0), 0x10)? {
        index_buffers.push(IndexBuffer {
            raw: rec.to_vec(),
            indices: r.indices(u32_at(rec, 4))?,
        });
    }
    let mut commands = Vec::new();
    for rec in r.records(count_at(raw, 0xa8), 8)? {
        let cmds = r.records(u32_at(rec, 0) as usize, 0x10)?;
        commands.push(
            cmds.iter()
                .map(|c| [0, 4, 8, 12].map(|o| u32_at(c, o)))
                .collect(),
        );
    }
    let mut stream_arrays = Vec::new();
    for rec in r.records(count_at(raw, 0xb0), 0x1c)? {
        let n = u32_at(rec, 4) as usize;
        stream_arrays.push(StreamArray {
            raw: rec.to_vec(),
            vertex_buffers: r.u32s(n)?,
            offsets: r.u32s(n)?,
            tail: r.u32s(u32_at(rec, 0x14) as usize)?,
        });
    }
    let mut stream_maps = Vec::new();
    for rec in r.records(count_at(raw, 0xb8), 8)? {
        stream_maps.push(r.u32s(u32_at(rec, 0) as usize)?);
    }
    let mut params = Vec::new();
    for rec in r.records(count_at(raw, 0xc0), 0x50)? {
        let n = u32_at(rec, 0x48) as usize;
        // Values with a name and maybe keys each, or bare ones (0x462780).
        let values = if u32_at(raw, 0) & 0x80_0000 != 0 {
            let recs = r.records(n, 0x85)?;
            recs.into_iter()
                .map(|e| Edited::read_keys(r, e))
                .collect::<Result<_>>()?
        } else {
            let of = |v: &&[u8]| Edited::fixed(cstr(rec), [0, 4, 8, 12].map(|o| f32_at(v, o)));
            r.records(n, 0x10)?.iter().map(of).collect()
        };
        params.push(Param {
            raw: rec.to_vec(),
            name: cstr(rec),
            values,
        });
    }
    Ok(Mesh {
        raw: raw.to_vec(),
        shader: cstr(&raw[4..0x84]),
        header,
        frame_keys,
        frames,
        index_buffers,
        commands,
        stream_arrays,
        stream_maps,
        params,
    })
}

/// Other data carried by an object or animation, by kind: weights of shapes, light
/// transfer (reader 0x465ca0).
#[derive(Debug, Clone, Default)]
pub struct Blocks {
    pub raw_id: u32,
    pub blocks: Vec<(u32, Vec<u8>)>,
}

pub fn read_blocks(r: &mut Reader) -> Result<Vec<Blocks>> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let raw_id = r.u32()?;
        let mut blocks = Vec::new();
        for _ in 0..r.count()? {
            let kind = r.u32()?;
            let n = r.count()?;
            blocks.push((kind, r.bytes(n)?.to_vec()));
        }
        out.push(Blocks { raw_id, blocks });
    }
    Ok(out)
}
