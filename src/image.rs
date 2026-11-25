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

/// Rows stay in the order of the file, which is usually from the bottom.
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
    Ok(Image {
        width,
        height,
        rgba: rgba.collect(),
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
        (t[2], t[12], t[14], t[16]) = (2, 1, 2, 24);
        t.extend(b"abcdef");
        let i = read("a.tga", &t).unwrap();
        assert_eq!(i.rgba, b"cba\xfffed\xff");
        assert_eq!(i.clone().flipped().rgba, b"fed\xffcba\xff");
        assert_eq!(i.half().rgba, [100, 99, 98, 255].map(|v: u8| v.max(1)));
        assert!(read("a.tga", &t[..20]).is_err());
    }
}
