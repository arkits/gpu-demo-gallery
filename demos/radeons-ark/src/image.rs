//! Pictures in the demos' formats, as RGBA rows from the first one stored.
use crate::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn solid(c: [u8; 4]) -> Self {
        Self {
            width: 1,
            height: 1,
            rgba: c.to_vec(),
        }
    }

    /// The last row first.
    pub fn flipped(mut self) -> Self {
        let rows = self.rgba.chunks_exact(self.width as usize * 4).rev();
        self.rgba = rows.flatten().copied().collect();
        self
    }

    pub fn map(mut self, f: impl Fn([u8; 4]) -> [u8; 4]) -> Self {
        for p in self.rgba.chunks_exact_mut(4) {
            p.copy_from_slice(&f([p[0], p[1], p[2], p[3]]));
        }
        self
    }

    /// Half the size, each pixel the mean of four.
    pub fn half(&self) -> Self {
        let (w, h) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let (dx, dy) = (u32::from(self.width > 1), u32::from(self.height > 1));
        let at =
            |x: u32, y: u32, c: u32| u32::from(self.rgba[((y * self.width + x) * 4 + c) as usize]);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for (y, x) in (0..h).flat_map(|y| (0..w).map(move |x| (y, x))) {
            for c in 0..4 {
                let (x0, y0) = (x << dx, y << dy);
                let sum = at(x0, y0, c)
                    + at(x0 + dx, y0, c)
                    + at(x0, y0 + dy, c)
                    + at(x0 + dx, y0 + dy, c);
                rgba.push((sum / 4) as u8);
            }
        }
        Self {
            width: w,
            height: h,
            rgba,
        }
    }
}

/// By the contents, or for TGA, which has no mark, by the name.
pub fn read(name: &str, d: &[u8]) -> Result<Image, Error> {
    if d.starts_with(b"\x89PNG") {
        png(d)
    } else if d.starts_with(b"P6") {
        ppm(d)
    } else if d.starts_with(b"DDS ") {
        dds(d)
    } else if d.starts_with(b"BM") {
        bmp(d)
    } else if name.ends_with(".tga") || name.ends_with(".dat") {
        tga(d)
    } else {
        Err("unknown kind of picture".into())
    }
    .map_err(|e| format!("{name}: {e}").into())
}

fn png(d: &[u8]) -> Result<Image, Error> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(d));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut raw = vec![0; reader.output_buffer_size().ok_or("too large")?];
    let info = reader.next_frame(&mut raw)?;
    let raw = &raw[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => raw
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => raw.iter().flat_map(|&l| [l, l, l, 255]).collect(),
        png::ColorType::GrayscaleAlpha => raw
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        c => return Err(format!("color type {c:?}").into()),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn ppm(d: &[u8]) -> Result<Image, Error> {
    // "P6", width, height and the largest value, with comments between.
    let mut at = 2;
    let mut number = || -> Result<u32, Error> {
        loop {
            match d.get(at).ok_or("cut short")? {
                b'#' => {
                    let line = d[at..].iter().position(|c| *c == b'\n');
                    at += line.ok_or("cut short")?;
                }
                c if c.is_ascii_whitespace() => at += 1,
                _ => break,
            }
        }
        let n = d[at..].iter().take_while(|c| c.is_ascii_digit()).count();
        let text = std::str::from_utf8(&d[at..at + n])?;
        at += n;
        Ok(text.parse()?)
    };
    let (width, height, most) = (number()?, number()?, number()?);
    if most != 255 {
        return Err("not 8 bits per channel".into());
    }
    let size = width as usize * height as usize * 3;
    let raw = d.get(at + 1..at + 1 + size).ok_or("cut short")?;
    let rgba = raw.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]);
    Ok(Image {
        width,
        height,
        rgba: rgba.collect(),
    })
}

/// The largest picture of a DXT1 or DXT5 file.
fn dds(d: &[u8]) -> Result<Image, Error> {
    let h = d.get(..128).ok_or("cut short")?;
    let int = |o: usize| u32::from_le_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]);
    let (height, width) = (int(12), int(16));
    // Uncompressed, 32 bits: the first level, top row first.
    if int(80) & 0x40 != 0 && int(88) == 32 {
        let masks = [int(92), int(96), int(100), int(104)];
        let size = (width * height) as usize;
        let raw = d.get(128..128 + size * 4).ok_or("cut short")?;
        let channel = |p: u32, m: u32| match m {
            0 => 255,
            m => ((p & m) >> m.trailing_zeros()) as u8,
        };
        let rgba = raw
            .chunks_exact(4)
            .flat_map(|p| {
                let p = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                masks.map(|m| channel(p, m))
            })
            .collect();
        return Ok(Image {
            width,
            height,
            rgba,
        });
    }
    let step = match &h[84..88] {
        b"DXT1" => 8,
        b"DXT5" => 16,
        kind => return Err(format!("kind {kind:?}").into()),
    };
    if ![width, height]
        .iter()
        .all(|s| (4..=4096).contains(s) && s.is_multiple_of(4))
    {
        return Err(format!("size {width} {height}").into());
    }
    let size = (width * height) as usize;
    let blocks = d.get(128..128 + size / 16 * step).ok_or("cut short")?;
    let mut rgba = vec![0; size * 4];
    for (i, b) in blocks.chunks_exact(step).enumerate() {
        let across = width as usize / 4;
        let (x, y) = (i % across * 4, i / across * 4);
        for (row, p) in pixels(b).as_chunks::<4>().0.iter().enumerate() {
            let at = ((y + row) * width as usize + x) * 4;
            rgba[at..at + 16].copy_from_slice(p.as_flattened());
        }
    }
    Ok(Image {
        width,
        height,
        rgba,
    })
}

/// Sixteen pixels: two colors and 2 bits each; with DXT5 before them two alphas and 3 bits
/// each.
fn pixels(b: &[u8]) -> [[u8; 4]; 16] {
    let (alphas, b) = b.split_at(b.len() - 8);
    let ends = [0, 2].map(|o| u32::from(u16::from_le_bytes([b[o], b[o + 1]])));
    let colors = ends.map(|c| {
        [
            (c >> 11) * 255 / 31,
            (c >> 5 & 63) * 255 / 63,
            (c & 31) * 255 / 31,
        ]
    });
    let number = |b: &[u8]| b.iter().rev().fold(0u64, |n, &v| n << 8 | u64::from(v));
    let codes = number(&b[4..]);
    let three = alphas.is_empty() && ends[0] <= ends[1];
    std::array::from_fn(|i| {
        let code = (codes >> (2 * i) & 3) as usize;
        // Of the second color, and of how many.
        let (part, all) = match three {
            true => ([0, 2, 1, 0][code], 2),
            false => ([0, 3, 1, 2][code], 3),
        };
        let mix = |c: usize| ((colors[0][c] * (all - part) + colors[1][c] * part) / all) as u8;
        let mut alpha = 255;
        if let [a0, a1, bits @ ..] = alphas {
            let (a0, a1) = (u32::from(*a0), u32::from(*a1));
            alpha = match ((number(bits) >> (3 * i) & 7) as u32, a0 > a1) {
                (0, _) => a0,
                (1, _) => a1,
                (k, true) => ((8 - k) * a0 + (k - 1) * a1) / 7,
                (6, false) => 0,
                (7, false) => 255,
                (k, false) => ((6 - k) * a0 + (k - 1) * a1) / 5,
            };
        }
        match three && code == 3 {
            true => [0; 4],
            false => [mix(0), mix(1), mix(2), alpha as u8],
        }
    })
}

/// Rows stay in the order of the file, which is usually from the bottom.
/// Windows bitmaps of 24 bits, top row first.
fn bmp(d: &[u8]) -> Result<Image, Error> {
    let h = d.get(..54).ok_or("cut short")?;
    let int = |o: usize| u32::from_le_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]);
    let (start, width, height) = (int(10) as usize, int(18), int(22) as i32);
    if u16::from_le_bytes([h[28], h[29]]) != 24 || int(30) != 0 {
        return Err("only plain 24 bit bitmaps".into());
    }
    let rows = height.unsigned_abs() as usize;
    let stride = (width as usize * 3).div_ceil(4) * 4;
    let mut rgba = Vec::with_capacity(width as usize * rows * 4);
    for r in 0..rows {
        let r = if height > 0 { rows - 1 - r } else { r };
        let row = d.get(start + r * stride..).ok_or("cut short")?;
        let row = row.get(..width as usize * 3).ok_or("cut short")?;
        rgba.extend(row.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0], 255]));
    }
    Ok(Image {
        width,
        height: rows as u32,
        rgba,
    })
}

fn tga(d: &[u8]) -> Result<Image, Error> {
    let h = d.get(..18).ok_or("cut short")?;
    let int = |o: usize| u32::from(u16::from_le_bytes([h[o], h[o + 1]]));
    let (width, height, bits) = (int(12), int(14), h[16]);
    let count = width as usize * height as usize;
    let step = match (h[1], h[2], bits) {
        (0, 2, 24) => 3,
        (0, 2, 32) => 4,
        (0, 3, 8) | (1, 1, 8) => 1,
        k => return Err(format!("kind {k:?}").into()),
    };
    let mut start = 18 + usize::from(h[0]);
    // A palette of 24 bits, from its first color.
    let mut palette = None;
    if h[1] == 1 {
        if (int(3), int(5), h[7]) != (0, 256, 24) {
            return Err("palette".into());
        }
        palette = Some(d.get(start..start + 768).ok_or("cut short")?);
        start += 768;
    }
    let raw = d.get(start..start + count * step).ok_or("cut short")?;
    let rgba = raw.chunks_exact(step).flat_map(|p| match (step, palette) {
        (1, Some(c)) => {
            let i = usize::from(p[0]) * 3;
            [c[i + 2], c[i + 1], c[i], 255]
        }
        (1, None) => [p[0], p[0], p[0], 255],
        (3, _) => [p[2], p[1], p[0], 255],
        _ => [p[2], p[1], p[0], p[3]],
    });
    let image = Image {
        width,
        height,
        rgba: rgba.collect(),
    };
    // Rows run from the bottom unless bit 5 of the descriptor says the top, as D3DX reads them.
    Ok(if h[17] & 0x20 == 0 {
        image.flipped()
    } else {
        image
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_pictures() {
        let p = read("a.ppm", b"P6\n# note\n2 1\n255\nabcdef").unwrap();
        assert_eq!((p.width, p.height), (2, 1));
        assert_eq!(p.rgba, b"abc\xffdef\xff");
        let mut t = vec![0u8; 18];
        (t[2], t[12], t[14], t[16], t[17]) = (2, 1, 2, 24, 0x20);
        t.extend(b"abcdef");
        let i = read("a.tga", &t).unwrap();
        assert_eq!(i.rgba, b"cba\xfffed\xff");
        t[17] = 0;
        assert_eq!(read("a.tga", &t).unwrap().rgba, b"fed\xffcba\xff");
        t[17] = 0x20;
        assert_eq!(i.clone().flipped().rgba, b"fed\xffcba\xff");
        assert_eq!(i.half().rgba, [100, 99, 98, 255].map(|v: u8| v.max(1)));
        assert!(read("a.tga", &t[..20]).is_err());
        // Red to blue, and alpha 7 to 0: the pixels of a row take each in turn.
        let mut d = b"DDS ".to_vec();
        d.resize(128, 0);
        (d[12], d[16]) = (4, 4);
        d[84..88].copy_from_slice(b"DXT5");
        d.extend([7, 0, 0b1000_1000, 0b0000_0110, 0, 0, 0, 0]);
        d.extend([0x00, 0xf8, 0x1f, 0x00, 0b1110_0100, 0, 0, 0]);
        let i = read("a.dds", &d).unwrap();
        let row = [255, 0, 0, 7, 0, 0, 255, 0, 170, 0, 85, 6, 85, 0, 170, 5];
        assert_eq!(i.rgba[..16], row);
        assert!(read("a.dds", &d[..140]).is_err());
    }
}
