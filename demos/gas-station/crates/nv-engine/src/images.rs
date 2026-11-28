//! Texture files: DXT1/3/5 DDS and uncompressed TGA (true colour or grey), rows kept in
//! file order (OpenGL's bottom row first, as the viewers hand them over).

use crate::gl::{Image, TexKind};

pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dxt {
    D1,
    D3,
    D5,
}

impl Dxt {
    fn block(self) -> usize {
        if self == Dxt::D1 { 8 } else { 16 }
    }
}

struct Dds<'a> {
    format: Dxt,
    width: u32,
    height: u32,
    levels: Vec<&'a [u8]>,
}

fn read_dds(d: &[u8]) -> Result<Dds<'_>, Error> {
    let int = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
    if d.len() < 128 || &d[..4] != b"DDS " {
        return Err("not a DDS file".into());
    }
    let format = match &d[84..88] {
        b"DXT1" => Dxt::D1,
        b"DXT3" => Dxt::D3,
        b"DXT5" => Dxt::D5,
        other => return Err(format!("unsupported DDS format {other:?}").into()),
    };
    let (height, width) = (int(12), int(16));
    let flags = int(8);
    let count = if flags & 0x20000 != 0 { int(28).max(1) } else { 1 };
    let mut levels = vec![];
    let mut at = 128usize;
    for k in 0..count {
        let w = (width >> k).max(1) as usize;
        let h = (height >> k).max(1) as usize;
        let size = w.div_ceil(4) * h.div_ceil(4) * format.block();
        let Some(l) = d.get(at..at + size) else { break };
        levels.push(l);
        at += size;
    }
    if levels.is_empty() {
        return Err("DDS file is cut short".into());
    }
    Ok(Dds { format, width, height, levels })
}

fn decode_block(format: Dxt, b: &[u8]) -> [[u8; 4]; 16] {
    let color = if format == Dxt::D1 { b } else { &b[8..] };
    let c = [0, 2].map(|o| u16::from_le_bytes([color[o], color[o + 1]]));
    let rgb = c.map(|v| {
        let v = u32::from(v);
        [(v >> 11) * 255 / 31, (v >> 5 & 63) * 255 / 63, (v & 31) * 255 / 31]
    });
    let mix = |a: u32, b: u32, d: u32| [0, 1, 2].map(|i| (a * rgb[0][i] + b * rgb[1][i]) / d);
    let four = format != Dxt::D1 || c[0] > c[1];
    let palette = if four { [rgb[0], rgb[1], mix(2, 1, 3), mix(1, 2, 3)] } else { [rgb[0], rgb[1], mix(1, 1, 2), [0; 3]] };
    let bits = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
    let alpha = |i: usize, k: usize| -> u8 {
        match format {
            Dxt::D1 => {
                if !four && k == 3 {
                    0
                } else {
                    255
                }
            }
            Dxt::D3 => ((b[i / 2] >> (4 * (i % 2))) & 15) * 17,
            Dxt::D5 => {
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
            }
        }
    };
    std::array::from_fn(|i| {
        let k = (bits >> (2 * i) & 3) as usize;
        let [r, g, bl] = palette[k].map(|v| v as u8);
        [r, g, bl, alpha(i, k)]
    })
}

fn decode_level(format: Dxt, w: usize, h: usize, data: &[u8]) -> Vec<u8> {
    let mut out = vec![0; w * h * 4];
    for (i, block) in data.chunks_exact(format.block()).enumerate() {
        let px = decode_block(format, block);
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

/// RGBA texels, OpenGL's luminance mode giving (L, L, L, 1) and (L, L, L, A).
fn read_tga(d: &[u8]) -> Result<(u32, u32, Vec<u8>), Error> {
    if d.len() < 18 || d[1] != 0 {
        return Err("unsupported TGA file".into());
    }
    let int = |o: usize| u32::from(u16::from_le_bytes([d[o], d[o + 1]]));
    let (w, h) = (int(12), int(14));
    let bpp = d[16] as usize / 8;
    let kind = d[2];
    if !((kind == 2 && (bpp == 3 || bpp == 4)) || (kind == 3 && (bpp == 1 || bpp == 2))) {
        return Err(format!("unsupported TGA type {kind} at {} bits", d[16]).into());
    }
    let at = 18 + usize::from(d[0]);
    let n = (w * h) as usize;
    let body = d.get(at..at + n * bpp).ok_or("TGA file is cut short")?;
    let mut out = vec![255; n * 4];
    for (i, p) in body.chunks_exact(bpp).enumerate() {
        let o = &mut out[i * 4..][..4];
        match (kind, bpp) {
            (2, _) => {
                (o[0], o[1], o[2]) = (p[2], p[1], p[0]);
                if bpp == 4 {
                    o[3] = p[3];
                }
            }
            (_, 1) => (o[0], o[1], o[2]) = (p[0], p[0], p[0]),
            _ => (o[0], o[1], o[2], o[3]) = (p[0], p[0], p[0], p[1]),
        }
    }
    // Rows from the bottom unless the descriptor says otherwise.
    if d[17] & 0x20 != 0 {
        let row = w as usize * 4;
        let mut flipped = vec![0; out.len()];
        for y in 0..h as usize {
            flipped[y * row..(y + 1) * row].copy_from_slice(&out[(h as usize - 1 - y) * row..(h as usize - y) * row]);
        }
        out = flipped;
    }
    Ok((w, h, out))
}

/// GL_GENERATE_MIPMAP: each level the 2×2 box average of the one above.
pub fn mips(w: u32, h: u32, base: Vec<u8>) -> Vec<Vec<u8>> {
    let mut levels = vec![base];
    let (mut w, mut h) = (w as usize, h as usize);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let src = levels.last().unwrap();
        let mut out = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let at = |xx: usize, yy: usize| src[((yy.min(h - 1)) * w + xx.min(w - 1)) * 4 + c] as u32;
                    let (x0, y0) = (x * 2, y * 2);
                    let x1 = if w > 1 { x0 + 1 } else { x0 };
                    let y1 = if h > 1 { y0 + 1 } else { y0 };
                    let s = at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1);
                    out[(y * nw + x) * 4 + c] = ((s + 2) / 4) as u8;
                }
            }
        }
        levels.push(out);
        w = nw;
        h = nh;
    }
    levels
}

/// One face or 2D image: levels in a GPU format.
fn face(data: &[u8], name: &str, bc: bool) -> Result<(u32, u32, wgpu::TextureFormat, Vec<Vec<u8>>), Error> {
    // By content: some `.dds` files are TGA.
    let _ = name;
    if data.starts_with(b"DDS ") {
        let dds = read_dds(data)?;
        let blocks_ok = bc && dds.width % 4 == 0 && dds.height % 4 == 0;
        if blocks_ok {
            let f = match dds.format {
                Dxt::D1 => wgpu::TextureFormat::Bc1RgbaUnorm,
                Dxt::D3 => wgpu::TextureFormat::Bc2RgbaUnorm,
                Dxt::D5 => wgpu::TextureFormat::Bc3RgbaUnorm,
            };
            return Ok((dds.width, dds.height, f, dds.levels.iter().map(|l| l.to_vec()).collect()));
        }
        let levels = dds
            .levels
            .iter()
            .enumerate()
            .map(|(k, l)| decode_level(dds.format, (dds.width >> k).max(1) as usize, (dds.height >> k).max(1) as usize, l))
            .collect();
        return Ok((dds.width, dds.height, wgpu::TextureFormat::Rgba8Unorm, levels));
    }
    let (w, h, rgba) = read_tga(data)?;
    Ok((w, h, wgpu::TextureFormat::Rgba8Unorm, mips(w, h, rgba)))
}

pub fn load_2d(data: &[u8], name: &str, bc: bool) -> Result<Image, Error> {
    let (w, h, format, levels) = face(data, name, bc)?;
    Ok(Image { width: w, height: h, depth: 1, format, layers: vec![levels], kind: TexKind::D2 })
}

/// Six faces, +x, -x, +y, -y, +z, -z.
pub fn load_cube(faces: &[(Vec<u8>, String)], bc: bool) -> Result<Image, Error> {
    let mut layers = vec![];
    let mut fmt = None;
    let mut size = (0, 0);
    for (d, n) in faces {
        let (w, h, f, levels) = face(d, n, bc)?;
        if fmt.is_some_and(|x| x != f) {
            return Err("cube faces of different formats".into());
        }
        fmt = Some(f);
        size = (w, h);
        layers.push(levels);
    }
    let n = layers.iter().map(|l| l.len()).min().unwrap_or(1);
    for l in &mut layers {
        l.truncate(n);
    }
    Ok(Image { width: size.0, height: size.1, depth: 1, format: fmt.unwrap_or(wgpu::TextureFormat::Rgba8Unorm), layers, kind: TexKind::Cube })
}
