//! 3dfx `.3DF` texture files (3df v1.0/v1.1).
//!
//! Observed layout, confirmed byte-wise against ~2400 staged ATB textures:
//!
//! * ASCII header lines: `3df vX.Y`, `<format>`, `lod range: 1 <max>`,
//!   `aspect ratio: <aw> <ah>`, each newline-terminated. Payload begins right
//!   after the `aspect ratio` line's newline.
//! * The LOD pyramid is **largest mip first**, descending to the smallest. Each
//!   level `L` (from `max` down to 1) has one dimension `L` and the other
//!   `L / r` where `r = max(aw,ah)/min(aw,ah)` (clamped to >= 1). Total payload
//!   = `2 * Σ (L * max(1, L/r))`, which this reader verifies exactly.
//! * Texels are **big-endian** 16-bit words. Known formats: `rgb565`,
//!   `argb4444`, `argb1555`, `argb8332` (all 2 bytes/texel). Compressed
//!   `yiq`/`ayiq8422` variants are refused (a different byte layout); the
//!   scene loader skips such files rather than failing.
//!
//! Verified by decoding `0000.3DF` (animated figure) and `SANDM.3DF` (sand),
//! and by checking the payload-size formula against every staged variant.
//! This is an observed format, not an official spec; unknown versions/formats
//! and inconsistent payloads are rejected, never guessed.

use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    pub width: u32,
    pub height: u32,
    /// RGBA8, top row first.
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture3df {
    pub version: String,
    pub format: String,
    pub lod_min: u32,
    pub lod_max: u32,
    pub aspect_w: u32,
    pub aspect_h: u32,
    /// Largest mip first (index 0 is the base texture).
    pub levels: Vec<Level>,
}

impl Texture3df {
    pub fn base(&self) -> &Level {
        &self.levels[0]
    }
}

#[derive(Clone, Copy)]
enum Format {
    Rgb565,
    Argb4444,
    Argb1555,
    Argb8332,
}

impl Format {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "rgb565" => Self::Rgb565,
            "argb4444" => Self::Argb4444,
            "argb1555" => Self::Argb1555,
            "argb8332" => Self::Argb8332,
            _ => return None,
        })
    }

    /// Decode one big-endian 16-bit word to RGBA8.
    fn rgba(self, px: u16) -> [u8; 4] {
        let expand = |v: u16, bits: u32| -> u8 {
            let max = (1u32 << bits) - 1;
            ((u32::from(v) * 255 + max / 2) / max) as u8
        };
        match self {
            Self::Rgb565 => [
                expand((px >> 11) & 0x1f, 5),
                expand((px >> 5) & 0x3f, 6),
                expand(px & 0x1f, 5),
                255,
            ],
            Self::Argb4444 => [
                expand((px >> 8) & 0xf, 4),
                expand((px >> 4) & 0xf, 4),
                expand(px & 0xf, 4),
                expand((px >> 12) & 0xf, 4),
            ],
            Self::Argb1555 => [
                expand((px >> 10) & 0x1f, 5),
                expand((px >> 5) & 0x1f, 5),
                expand(px & 0x1f, 5),
                if px & 0x8000 != 0 { 255 } else { 0 },
            ],
            Self::Argb8332 => [
                expand((px >> 5) & 0x7, 3),
                expand((px >> 2) & 0x7, 3),
                expand(px & 0x3, 2),
                (px >> 8) as u8,
            ],
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<Texture3df, Error> {
    let mut pos = 0usize;
    let mut lines = Vec::new();
    for _ in 0..4 {
        let end = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| pos + i)
            .ok_or("3DF header is not terminated")?;
        let line = std::str::from_utf8(&bytes[pos..end])
            .map_err(|_| "3DF header is not ASCII")?
            .trim()
            .to_string();
        lines.push(line);
        pos = end + 1;
    }

    let version = lines[0].clone();
    if !version.starts_with("3df v1.") {
        return Err(format!("unsupported 3DF version {version:?}").into());
    }
    let format_str = lines[1].clone();
    let format = Format::parse(&format_str)
        .ok_or_else(|| format!("unsupported 3DF format {format_str:?}"))?;
    let (lod_min, lod_max) = parse_pair(&lines[2], "lod range:")?;
    let (aspect_w, aspect_h) = parse_pair(&lines[3], "aspect ratio:")?;

    if lod_min == 0 || lod_max == 0 || lod_min > lod_max || !lod_max.is_power_of_two() {
        return Err(format!("invalid lod range {lod_min} {lod_max}").into());
    }
    if aspect_w == 0 || aspect_h == 0 {
        return Err(format!("invalid aspect ratio {aspect_w} {aspect_h}").into());
    }
    let hi = aspect_w.max(aspect_h);
    let lo = aspect_w.min(aspect_h);
    if hi % lo != 0 {
        return Err(format!("unsupported aspect ratio {aspect_w} {aspect_h}").into());
    }
    let r = hi / lo;

    // Level dims: one side = L, the other = max(1, L/r); aw>=ah => width is L.
    let mut sizes: Vec<(u32, u32)> = Vec::new();
    let mut l = lod_max;
    loop {
        let small = (l / r).max(1);
        let (w, h) = if aspect_w >= aspect_h {
            (l, small)
        } else {
            (small, l)
        };
        sizes.push((w, h));
        if l <= lod_min {
            break;
        }
        l /= 2;
    }
    let expected: usize = sizes
        .iter()
        .map(|(w, h)| 2 * (*w as usize) * (*h as usize))
        .sum();
    let payload = bytes.get(pos..).ok_or("3DF payload missing")?;
    if payload.len() != expected {
        return Err(format!("3DF payload {} bytes, expected {expected}", payload.len()).into());
    }

    let mut levels = Vec::with_capacity(sizes.len());
    let mut at = 0usize;
    for (w, h) in sizes {
        let count = (w as usize) * (h as usize);
        let mut rgba = Vec::with_capacity(count * 4);
        for texel in payload[at..at + count * 2].chunks_exact(2) {
            rgba.extend_from_slice(&format.rgba(u16::from_be_bytes([texel[0], texel[1]])));
        }
        at += count * 2;
        levels.push(Level {
            width: w,
            height: h,
            rgba,
        });
    }

    Ok(Texture3df {
        version,
        format: format_str,
        lod_min,
        lod_max,
        aspect_w,
        aspect_h,
        levels,
    })
}

fn parse_pair(line: &str, prefix: &str) -> Result<(u32, u32), Error> {
    let rest = line
        .strip_prefix(prefix)
        .ok_or("3DF header line unrecognised")?;
    let mut parts = rest.split_whitespace();
    let a = parts.next().ok_or("missing value")?.parse::<u32>()?;
    let b = parts.next().ok_or("missing value")?.parse::<u32>()?;
    Ok((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny 2x2 + 1x1 pyramid, big-endian rgb565.
    fn fixture() -> Vec<u8> {
        let mut d = b"3df v1.1\nrgb565\nlod range: 1 2\naspect ratio: 1 1\n".to_vec();
        for px in [0xf800u16, 0x07e0, 0x001f, 0xffff] {
            d.extend_from_slice(&px.to_be_bytes());
        }
        d.extend_from_slice(&0u16.to_be_bytes());
        d
    }

    #[test]
    fn reads_pyramid() {
        let t = read(&fixture()).unwrap();
        assert_eq!((t.lod_min, t.lod_max), (1, 2));
        assert_eq!(t.levels.len(), 2);
        assert_eq!((t.base().width, t.base().height), (2, 2));
        assert_eq!(&t.base().rgba[..4], &[255, 0, 0, 255]); // red
        assert_eq!(&t.base().rgba[4..8], &[0, 255, 0, 255]); // green
        assert_eq!(&t.base().rgba[8..12], &[0, 0, 255, 255]); // blue
        assert_eq!(&t.levels[1].rgba, &[0, 0, 0, 255]); // 1x1 black
    }

    #[test]
    fn reads_nonsquare_and_other_formats() {
        // 2x1 argb4444 base + 1x1 mip: aspect 2:1, lod max 2.
        let mut d = b"3df v1.0\nargb4444\nlod range: 1 2\naspect ratio: 2 1\n".to_vec();
        d.extend_from_slice(&0xffffu16.to_be_bytes()); // opaque white
        d.extend_from_slice(&0x0f00u16.to_be_bytes()); // red, alpha 0
        d.extend_from_slice(&0x0000u16.to_be_bytes()); // 1x1 mip
        let t = read(&d).unwrap();
        assert_eq!((t.base().width, t.base().height), (2, 1));
        assert_eq!(t.levels.len(), 2);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(read(b"not a 3df").is_err());
        assert!(
            read(b"3df v1.1\nyiq\nlod range: 1 2\naspect ratio: 1 1\n\0\0\0\0\0\0\0\0\0\0")
                .is_err()
        );
        let mut d = fixture();
        d.truncate(d.len() - 2);
        assert!(read(&d).is_err());
    }
}
