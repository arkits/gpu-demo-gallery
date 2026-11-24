use crate::reader::*;

pub const TEXTURE_RECORD: usize = 0x1ed;

/// A 0x1ed texture record plus its texel blob (all mips, all faces).
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub raw: Vec<u8>,
    /// Empty when the data lives in another record with the same name, or the
    /// texture is supplied by the engine.
    pub texels: Vec<u8>,
}

/// Records are stored contiguously, followed by all blobs in order.
pub fn read_textures(r: &mut Reader, n: usize) -> Result<Vec<Texture>> {
    let recs = r.records(n, TEXTURE_RECORD)?;
    recs.into_iter().map(|raw| read_texels(r, raw)).collect()
}

pub fn read_texels(r: &mut Reader, raw: &[u8]) -> Result<Texture> {
    let texels = r.bytes(u32_at(raw, 0x1c4) as usize)?.to_vec();
    Ok(Texture {
        name: cstr(raw),
        raw: raw.to_vec(),
        texels,
    })
}

/// Pixel formats the demo uses (record +0x100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    L8,
    Argb8,
    Xrgb8,
    Dxt1,
    Dxt5,
}

impl Format {
    /// Bytes of one `w` x `h` level.
    pub fn level_size(self, w: usize, h: usize) -> usize {
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        match self {
            Format::L8 => w * h,
            Format::Argb8 | Format::Xrgb8 => w * h * 4,
            Format::Dxt1 => blocks * 8,
            Format::Dxt5 => blocks * 16,
        }
    }
}

impl Texture {
    /// `None` for engine-supplied textures (type 6) and unknown formats.
    pub fn format(&self) -> Option<Format> {
        Some(match u32_at(&self.raw, 0x100) {
            0 => Format::L8,
            3 => Format::Argb8,
            4 => Format::Xrgb8,
            6 => Format::Dxt1,
            8 => Format::Dxt5,
            _ => return None,
        })
    }
    pub fn width(&self) -> usize {
        usize::from(u16::from_le_bytes([self.raw[0x1d0], self.raw[0x1d1]]))
    }
    pub fn height(&self) -> usize {
        usize::from(u16::from_le_bytes([self.raw[0x1d2], self.raw[0x1d3]]))
    }
    pub fn faces(&self) -> usize {
        usize::from(self.raw[0x1d4])
    }
    pub fn is_cube(&self) -> bool {
        self.raw[0x1d5] == 1
    }
    /// Bytes of one face with all its mips (+0x1c0); 0 on generated spotlights,
    /// which have a single level.
    fn face_size(&self) -> usize {
        u32_at(&self.raw, 0x1c0) as usize
    }

    /// Decodes every face and mip to RGBA8: `[face][mip] = (w, h, rgba)`.
    pub fn decode(&self) -> Result<Vec<Vec<Level>>> {
        let bad = |what: &str| Error::Bad {
            at: 0,
            what: format!("{}: {what}", self.name),
        };
        let fmt = self.format().ok_or_else(|| bad("unknown format"))?;
        let faces = self.faces();
        if faces == 0 || self.texels.len() % faces != 0 {
            return Err(bad("face count does not divide data"));
        }
        let size = self.texels.len() / faces;
        if self.face_size() != 0 && self.face_size() != size {
            return Err(bad("face size mismatch"));
        }
        let mut out = Vec::new();
        for face in self.texels.chunks_exact(size.max(1)) {
            let (mut w, mut h, mut pos, mut mips) = (self.width(), self.height(), 0, Vec::new());
            while pos < size {
                let n = fmt.level_size(w, h);
                let data = face
                    .get(pos..pos + n)
                    .ok_or_else(|| bad("mip chain mismatch"))?;
                mips.push(Level {
                    width: w,
                    height: h,
                    rgba: decode_level(fmt, w, h, data),
                });
                pos += n;
                if w == 1 && h == 1 && pos < size {
                    return Err(bad("data after last mip"));
                }
                (w, h) = ((w / 2).max(1), (h / 2).max(1));
            }
            out.push(mips);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct Level {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

fn decode_level(fmt: Format, w: usize, h: usize, data: &[u8]) -> Vec<u8> {
    match fmt {
        Format::L8 => data.iter().flat_map(|&l| [l, l, l, 255]).collect(),
        Format::Argb8 => data
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect(),
        Format::Xrgb8 => data
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect(),
        Format::Dxt1 | Format::Dxt5 => {
            let mut out = vec![0; w * h * 4];
            let size = if fmt == Format::Dxt1 { 8 } else { 16 };
            for (i, block) in data.chunks_exact(size).enumerate() {
                let px = decode_block(fmt, block);
                let (bx, by) = (i % w.div_ceil(4) * 4, i / w.div_ceil(4) * 4);
                for (j, p) in px.iter().enumerate() {
                    let (x, y) = (bx + j % 4, by + j / 4);
                    if x < w && y < h {
                        out[(y * w + x) * 4..][..4].copy_from_slice(p);
                    }
                }
            }
            out
        }
    }
}

fn decode_block(fmt: Format, b: &[u8]) -> [[u8; 4]; 16] {
    let color = if fmt == Format::Dxt1 { b } else { &b[8..] };
    let c = [0, 2].map(|o| u16::from_le_bytes([color[o], color[o + 1]]));
    let rgb = c.map(|v| {
        let v = u32::from(v);
        [
            (v >> 11) * 255 / 31,
            (v >> 5 & 63) * 255 / 63,
            (v & 31) * 255 / 31,
        ]
    });
    let mix = |a: u32, b: u32, d: u32| [0, 1, 2].map(|i| (a * rgb[0][i] + b * rgb[1][i]) / d);
    let four = fmt == Format::Dxt5 || c[0] > c[1];
    let pal = if four {
        [rgb[0], rgb[1], mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [rgb[0], rgb[1], mix(1, 1, 2), [0; 3]]
    };
    let bits = u32_at(color, 4);
    let alpha = |i: usize, k: usize| -> u8 {
        if fmt == Format::Dxt1 {
            return if !four && k == 3 { 0 } else { 255 };
        }
        let (a0, a1) = (u32::from(b[0]), u32::from(b[1]));
        let mut ab = [0u8; 8];
        ab[..6].copy_from_slice(&b[2..8]);
        let code = (u64::from_le_bytes(ab) >> (3 * i) & 7) as u32;
        (match (code, a0 > a1) {
            (0, _) => a0,
            (1, _) => a1,
            (c, true) => ((8 - c) * a0 + (c - 1) * a1) / 7,
            (6, false) => 0,
            (7, false) => 255,
            (c, false) => ((6 - c) * a0 + (c - 1) * a1) / 5,
        }) as u8
    };
    std::array::from_fn(|i| {
        let k = (bits >> (2 * i) & 3) as usize;
        [
            pal[k][0] as u8,
            pal[k][1] as u8,
            pal[k][2] as u8,
            alpha(i, k),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_two_colour_block() {
        // Red and blue endpoints; every pixel uses index 1 (blue).
        let b = [0x00, 0xf8, 0x1f, 0x00, 0x55, 0x55, 0x55, 0x55];
        assert_eq!(decode_block(Format::Dxt1, &b), [[0, 0, 255, 255]; 16]);
    }

    #[test]
    fn dxt1_three_colour_mode_is_transparent_at_index_3() {
        let b = [0x1f, 0x00, 0x00, 0xf8, 0xff, 0xff, 0xff, 0xff];
        assert_eq!(decode_block(Format::Dxt1, &b)[0], [0, 0, 0, 0]);
    }

    #[test]
    fn dxt5_alpha_endpoints() {
        let mut b = [0; 16];
        b[0] = 200;
        b[1] = 100;
        b[2] = 0b001; // pixel 0 -> a1
        let px = decode_block(Format::Dxt5, &b);
        assert_eq!((px[0][3], px[1][3]), (100, 200));
    }
}
