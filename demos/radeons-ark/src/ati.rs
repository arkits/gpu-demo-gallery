//! radiATIon worlds (`.ati`, version 0.45), read as the program reads them (0x4125f0): the
//! world's structure as it lay in memory, then each array it points to, in a fixed order.
//! Pointers in the file are meaningless; counts say how much follows.
use crate::Error;
use crate::image::Image;

pub const MAGIC: &[u8] = b"* ATI radiATIon file v0.45 *";
pub const ROOM: usize = 0x448;
pub const MIRROR: usize = 200;
/// Arrays of a room, by the offset of their count and the size of an element.
const ROOM_ARRAYS: [(usize, usize); 23] = [
    (0x9c, 0x4c),
    (0xa4, 4),
    (0xac, 4),
    (0xb4, 0x14),
    (0xbc, 0xb7),
    (0x13c, 0x10),
    (0x148, 0x5c),
    (0x154, 0xb7),
    (0x1d4, 0x10),
    (0x1e0, 0xb7),
    (0x260, 0x10),
    (0x270, 0x10),
    (0x278, 0x14),
    (0x280, 0x79),
    (0x288, 0x84),
    (0x290, 0x120),
    (0x298, 0x3c),
    (0x2a0, 0x34),
    (0x2a8, 0x10),
    (0x2b0, 0x14),
    (0x2c0, 1),
    (0x3b8, 2),
    (0x438, 0x44),
];
/// Vertex buckets: the vertex blob is in parts by kind of vertex (0x412eb0). Stride in bytes
/// and Direct3D FVF of each; buckets the program never draws have stride 0.
pub const BUCKETS: usize = 30;

pub fn i32_at(d: &[u8], o: usize) -> i32 {
    d.get(o..o + 4)
        .map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
pub fn f32_at(d: &[u8], o: usize) -> f32 {
    f32::from_bits(i32_at(d, o) as u32)
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let b = self
            .d
            .get(self.at..self.at + n)
            .ok_or("radiATIon file cut short")?;
        self.at += n;
        Ok(b)
    }
    fn array(&mut self, count: i32, size: usize) -> Result<Vec<u8>, Error> {
        if count <= 0 {
            return Ok(Vec::new());
        }
        Ok(self.take(count as usize * size)?.to_vec())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Room {
    pub raw: Vec<u8>,
    /// Arrays by the offset of their count in `raw`.
    pub arrays: Vec<(usize, Vec<u8>)>,
    /// Water grids' and meshes' own arrays: (array offset, element, field offset, data).
    pub sub: Vec<(usize, usize, usize, Vec<u8>)>,
}

impl Room {
    pub fn array(&self, off: usize) -> &[u8] {
        self.arrays
            .iter()
            .find(|a| a.0 == off)
            .map_or(&[], |a| a.1.as_slice())
    }
    pub fn count(&self, off: usize) -> usize {
        i32_at(&self.raw, off).max(0) as usize
    }
    /// The room's box: lowest and highest corners (+0x4).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let f = |o| f32_at(&self.raw, o);
        ([f(4), f(8), f(0xc)], [f(0x10), f(0x14), f(0x18)])
    }
    /// The vertices of bucket `b`, as bytes of the blob (0x2c4, offsets in words at 0x2c8).
    pub fn vertices(&self, b: usize) -> &[u8] {
        let start = i32_at(&self.raw, 0x2c8 + 4 * b).max(0) as usize * 4;
        let end = i32_at(&self.raw, 0x2cc + 4 * b).max(0) as usize * 4;
        let blob = self.array(0x2c0);
        blob.get(start..end.max(start)).unwrap_or(&[])
    }
    /// The command list of bucket `b`: 16-bit words from offset 0x3c0 + 4b of the index array.
    pub fn commands(&self, b: usize) -> Vec<i16> {
        let start = i32_at(&self.raw, 0x3c0 + 4 * b).max(0) as usize * 2;
        let list = self.array(0x3b8);
        list.get(start..)
            .unwrap_or(&[])
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Texture {
    /// 3 RGB, 4 RGBA, 1, 2 and 5 one byte a pixel.
    pub format: u8,
    pub kind: u8,
    pub flag: u8,
    pub name: String,
    pub image: Image,
}

#[derive(Debug, Clone, Default)]
pub struct Mirror {
    pub raw: Vec<u8>,
    pub rooms: Vec<Room>,
}

#[derive(Debug, Clone, Default)]
pub struct World {
    pub header: Vec<u8>,
    pub rooms: Vec<Room>,
    pub block5: Vec<u8>,
    pub block7: Vec<u8>,
    pub mirrors: Vec<Mirror>,
    pub textures: Vec<Texture>,
}

impl World {
    pub fn header_i32(&self, k: usize) -> i32 {
        i32_at(&self.header, 4 * k)
    }
    pub fn header_f32(&self, k: usize) -> f32 {
        f32_at(&self.header, 4 * k)
    }

    pub fn read(d: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { d, at: 0 };
        let n = i32_at(r.take(4)?, 0).clamp(0, 0x3f) as usize;
        let magic = r.take(n)?;
        if !MAGIC.starts_with(magic) || magic.is_empty() {
            return Err("not a radiATIon 0.45 file".into());
        }
        let header = r.take(0x7c)?.to_vec();
        let h = |k: usize| i32_at(&header, 4 * k);
        let rooms_raw = r.array(h(0), ROOM)?;
        let block5 = r.array(h(5), 0x54)?;
        let block7 = r.array(h(7), 0x54)?;
        let mirrors_raw = r.array(h(9), MIRROR)?;
        let tex_raw = r.array(h(0xb), 0x4f)?;
        let pixels = r.array(h(0x12), 1)?;
        let _extra = r.array(h(0x1d), 0x396)?;
        let mut mirror_rooms = Vec::new();
        for m in mirrors_raw.chunks_exact(MIRROR) {
            mirror_rooms.push(r.array(i32_at(m, 0xb8), ROOM)?);
        }
        // Every room's arrays: the world's rooms first, then each mirror's.
        let room_list = |raw: &[u8], r: &mut Reader| -> Result<Vec<Room>, Error> {
            let mut out = Vec::new();
            for rec in raw.chunks_exact(ROOM) {
                let mut room = Room {
                    raw: rec.to_vec(),
                    ..Default::default()
                };
                for (off, size) in ROOM_ARRAYS {
                    let a = r.array(i32_at(rec, off), size)?;
                    room.arrays.push((off, a));
                }
                out.push(room);
            }
            Ok(out)
        };
        let mut rooms = room_list(&rooms_raw, &mut r)?;
        let mut mirrors = Vec::new();
        for (m, raw) in mirrors_raw.chunks_exact(MIRROR).zip(&mirror_rooms) {
            mirrors.push(Mirror {
                raw: m.to_vec(),
                rooms: room_list(raw, &mut r)?,
            });
        }
        // Then, for the world's rooms only: the character lists' entries, water grids, meshes.
        for room in &mut rooms {
            let list = room.array(0xb4).to_vec();
            for (j, e) in list.chunks_exact(0x14).enumerate() {
                let a = r.array(i32_at(e, 4), 4)?;
                room.sub.push((0xb4, j, 4, a));
            }
        }
        for room in &mut rooms {
            let list = room.array(0x288).to_vec();
            for (j, e) in list.chunks_exact(0x84).enumerate() {
                for (off, size) in [(0x68, 2), (0x70, 0x24)] {
                    let a = r.array(i32_at(e, off), size)?;
                    room.sub.push((0x288, j, off, a));
                }
            }
        }
        for room in &mut rooms {
            let list = room.array(0x290).to_vec();
            for (j, e) in list.chunks_exact(0x120).enumerate() {
                for (off, size) in [(0xf0, 0x1c), (0xfc, 2), (0x110, 0xb7), (0x118, 0x10)] {
                    let a = r.array(i32_at(e, off), size)?;
                    room.sub.push((0x290, j, off, a));
                }
            }
        }
        if r.at != d.len() {
            return Err(format!("radiATIon file: {} bytes left over", d.len() - r.at).into());
        }
        let textures = tex_raw
            .chunks_exact(0x4f)
            .map(|t| texture(t, &pixels))
            .collect();
        Ok(Self {
            header,
            rooms,
            block5,
            block7,
            mirrors,
            textures,
        })
    }
}

/// A texture's record: format, two flags, the offset of its pixels, width, height, then the
/// names of the pictures it was made from (colour and alpha, 32 bytes each).
fn texture(t: &[u8], pixels: &[u8]) -> Texture {
    let (off, w, h) = (
        i32_at(t, 3).max(0) as usize,
        i32_at(t, 7).max(1) as u32,
        i32_at(t, 11).max(1) as u32,
    );
    let name =
        String::from_utf8_lossy(t[15..47].split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
    let bpp = match t[0] {
        3 => 3,
        4 => 4,
        _ => 1,
    };
    let n = (w * h) as usize;
    let src = pixels.get(off..off + n * bpp).unwrap_or(&[]);
    let mut rgba = Vec::with_capacity(n * 4);
    for p in src.chunks_exact(bpp) {
        match bpp {
            3 => rgba.extend([p[0], p[1], p[2], 255]),
            4 => rgba.extend([p[0], p[1], p[2], p[3]]),
            _ => rgba.extend([p[0], p[0], p[0], p[0]]),
        }
    }
    rgba.resize(n * 4, 255);
    Texture {
        format: t[0],
        kind: t[1],
        flag: t[2],
        name,
        image: Image {
            width: w,
            height: h,
            rgba,
        },
    }
}
