//! DXT1, DXT3, and DXT5 textures in DDS files.
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dxt1,
    Dxt3,
    Dxt5,
}

impl Format {
    pub fn block(self) -> usize {
        match self {
            Self::Dxt1 => 8,
            Self::Dxt3 | Self::Dxt5 => 16,
        }
    }
}

pub struct Image<'a> {
    pub format: Format,
    pub width: usize,
    pub height: usize,
    /// Compressed levels, largest first.
    pub levels: Vec<&'a [u8]>,
}

impl<'a> Image<'a> {
    pub fn read(d: &'a [u8]) -> Result<Self, Error> {
        let int = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize;
        if d.len() < 128 || &d[..4] != b"DDS " {
            return Err("not a DDS file".into());
        }
        let format = match &d[84..88] {
            b"DXT1" => Format::Dxt1,
            b"DXT3" => Format::Dxt3,
            b"DXT5" => Format::Dxt5,
            other => return Err(format!("unsupported DDS format {other:?}").into()),
        };
        let (height, width) = (int(12), int(16));
        if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
            return Err("bad DDS size".into());
        }
        let (mut at, mut levels) = (128, Vec::new());
        for k in 0..int(28).clamp(1, 13) {
            let (w, h) = ((width >> k).max(1), (height >> k).max(1));
            let n = w.div_ceil(4) * h.div_ceil(4) * format.block();
            levels.push(d.get(at..at + n).ok_or("DDS file is cut short")?);
            at += n;
        }
        Ok(Self {
            format,
            width,
            height,
            levels,
        })
    }

    /// Level `k` as RGBA rows.
    pub fn decode(&self, k: usize) -> (usize, usize, Vec<u8>) {
        let (w, h) = ((self.width >> k).max(1), (self.height >> k).max(1));
        let mut out = vec![0; w * h * 4];
        for (i, block) in self.levels[k].chunks_exact(self.format.block()).enumerate() {
            let px = decode_block(self.format, block);
            let (bx, by) = (i % w.div_ceil(4) * 4, i / w.div_ceil(4) * 4);
            for (j, p) in px.iter().enumerate() {
                let (x, y) = (bx + j % 4, by + j / 4);
                if x < w && y < h {
                    out[(y * w + x) * 4..][..4].copy_from_slice(p);
                }
            }
        }
        (w, h, out)
    }
}

fn decode_block(format: Format, b: &[u8]) -> [[u8; 4]; 16] {
    let color = if format == Format::Dxt1 { b } else { &b[8..] };
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
    let four = format != Format::Dxt1 || c[0] > c[1];
    let palette = if four {
        [rgb[0], rgb[1], mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [rgb[0], rgb[1], mix(1, 1, 2), [0; 3]]
    };
    let bits = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
    let alpha = |i: usize, k: usize| -> u8 {
        if format == Format::Dxt1 {
            return if !four && k == 3 { 0 } else { 255 };
        }
        if format == Format::Dxt3 {
            return ((b[i / 2] >> (4 * (i % 2))) & 15) * 17;
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
        let [r, g, b] = palette[k].map(|v| v as u8);
        [r, g, b, alpha(i, k)]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_decodes() {
        let mut d = b"DDS ".to_vec();
        d.resize(128, 0);
        (d[12], d[16], d[28]) = (4, 4, 1);
        d[84..88].copy_from_slice(b"DXT1");
        // Red and blue endpoints; every pixel uses index 1 (blue).
        d.extend([0x00, 0xf8, 0x1f, 0x00, 0x55, 0x55, 0x55, 0x55]);
        let i = Image::read(&d).unwrap();
        let (w, h, px) = i.decode(0);
        assert_eq!((w, h), (4, 4));
        assert!(px.chunks(4).all(|p| p == [0, 0, 255, 255]));
        assert!(Image::read(&d[..d.len() - 1]).is_err());
    }
}
