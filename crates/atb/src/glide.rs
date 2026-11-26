//! Glide texture formats and the `atrMaterial` record, which holds Glide state.

use crate::Error;
use crate::bof::{Image, Material};

/// One mip level as RGBA8, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn expand(v: u32, bits: u32) -> u8 {
    let max = (1u32 << bits) - 1;
    ((v * 255 + max / 2) / max) as u8
}

impl Image {
    /// Bytes a texel takes in this image's format.
    fn texel_size(&self) -> usize {
        match self.format {
            0..=7 => 1,
            8..=15 => 2,
            _ => 4,
        }
    }

    /// The largest level, decoded. Palette and YIQ formats are refused: no
    /// staged file uses them.
    pub fn rgba(&self) -> Result<Rgba, Error> {
        let (w, h) = (self.width as usize, self.height as usize);
        let size = self.texel_size();
        let need = w * h * size;
        if w == 0 || h == 0 || self.data.len() < need {
            return Err("ATB image smaller than its size".into());
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for i in 0..w * h {
            let t = &self.data[i * size..i * size + size];
            let v = match size {
                1 => u32::from(t[0]),
                2 => u32::from(u16::from_be_bytes([t[0], t[1]])),
                _ => u32::from_be_bytes([t[0], t[1], t[2], t[3]]),
            };
            let px = match self.format {
                // RGB_332
                0 => [
                    expand(v >> 5, 3),
                    expand((v >> 2) & 7, 3),
                    expand(v & 3, 2),
                    255,
                ],
                // ALPHA_8
                2 => [255, 255, 255, v as u8],
                // INTENSITY_8
                3 => [v as u8; 4],
                // ALPHA_INTENSITY_44
                4 => {
                    let i = expand(v & 15, 4);
                    [i, i, i, expand(v >> 4, 4)]
                }
                // ARGB_8332
                8 => [
                    expand((v >> 5) & 7, 3),
                    expand((v >> 2) & 7, 3),
                    expand(v & 3, 2),
                    (v >> 8) as u8,
                ],
                // RGB_565
                10 => [
                    expand(v >> 11, 5),
                    expand((v >> 5) & 63, 6),
                    expand(v & 31, 5),
                    255,
                ],
                // ARGB_1555
                11 => [
                    expand((v >> 10) & 31, 5),
                    expand((v >> 5) & 31, 5),
                    expand(v & 31, 5),
                    if v & 0x8000 != 0 { 255 } else { 0 },
                ],
                // ARGB_4444
                12 => [
                    expand((v >> 8) & 15, 4),
                    expand((v >> 4) & 15, 4),
                    expand(v & 15, 4),
                    expand(v >> 12, 4),
                ],
                // ALPHA_INTENSITY_88
                13 => [v as u8, v as u8, v as u8, (v >> 8) as u8],
                // ARGB_8888
                16 => [(v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8],
                f => return Err(format!("ATB image format {f} is not supported").into()),
            };
            rgba.extend(px);
        }
        Ok(Rgba {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
}

impl Material {
    /// A material as `atrMaterialSetup` (`demoroom.exe` `0x420830`) makes it
    /// from flags: the low byte picks the texture set-up, 0xf00 the blend,
    /// 0x3000 the color combine and 0xc000 where the color comes from. Defaults
    /// are `0x4205c0`'s.
    pub fn preset(flags: u32) -> Self {
        let mut w = [0u32; 79];
        w[0] = flags;
        w[10] = 0x14;
        w[17] = 0.04f32.to_bits();
        w[45] = 7; // alpha test: always
        [w[51], w[52], w[53], w[54], w[55], w[56]] = [1; 6];
        w[68] = 1; // color "other" is the texture unit
        w[77] = u32::MAX;
        // TMU 0: mode, coordinates, rgb function and factor, alpha function
        // and factor, LOD blend; then clamp s and t.
        let tmu: Option<(u32, u32, [u32; 4], u32)> = match flags & 0xff {
            0 => None,
            1 => Some((0x100, 0x1000, [1, 0, 1, 0], 0)),
            2 => None,
            3 => Some((0x100, 0x1000, [9, 0xd, 9, 0xd], 1)),
            4 => Some((0x100, 0x1000, [9, 5, 9, 5], 1)),
            6 => Some((0x100, 0x1000, [9, 4, 9, 4], 0)),
            7 => Some((0x100, 0x1000, [9, 0xc, 9, 0xc], 0)),
            9 => Some((0x300, 0x4000, [1, 0, 1, 0], 0)),
            0x0a => Some((0x400, 0x5000, [1, 0, 1, 0], 0)),
            0x0b => {
                w[47] = 1;
                w[49] = 1;
                Some((0x500, 0x6000, [1, 0, 1, 0], 0))
            }
            0x0d => Some((0x500, 0x7000, [1, 0, 1, 0], 0)),
            _ => Some((0x100, 0x1000, [1, 0, 1, 0], 0)),
        };
        if let Some((mode, coords, [rf, rk, af, ak], lod)) = tmu {
            w[29] = mode;
            w[31] = coords;
            [w[33], w[35], w[37], w[39]] = [rf, rk, af, ak];
            w[57] = lod;
        }
        match flags & 0xc000 {
            0x4000 => w[26] = 1,
            0x8000 => w[26] = 2,
            0xc000 => {
                w[28] = 0x20;
                w[67] = 1;
            }
            _ => w[67] = 1,
        }
        (w[65], w[66]) = match flags & 0x3000 {
            0x1000 => (3, 1),
            0x2000 => (4, 8),
            0x3000 => (7, 4),
            _ => (3, 8),
        };
        [w[70], w[71], w[72], w[73]] = [3, 8, 1, 1];
        (w[75], w[76]) = match flags & 0xf00 {
            0x100 => (0, 2),
            0x200 => (4, 4),
            0x300 => (1, 5),
            0x400 => (0, 0),
            0x500 => (4, 15),
            _ => (4, 0),
        };
        Self { words: w }
    }

    /// Sets the three colors: emissive, diffuse and specular.
    pub fn with_colors(mut self, emissive: [f32; 3], diffuse: [f32; 3]) -> Self {
        for i in 0..3 {
            self.words[1 + i] = emissive[i].to_bits();
            self.words[4 + i] = diffuse[i].to_bits();
        }
        self
    }
}

/// `atrMaterial` fields, by the order the loader (`0x41a0b0`) reads them and
/// the use `atrMaterialRealize` (`wiz.exe` `0x4326a0`) makes of them.
impl Material {
    pub fn flags(&self) -> u32 {
        self.words[0]
    }
    fn rgb(&self, at: usize) -> [f32; 3] {
        [0, 1, 2].map(|i| f32::from_bits(self.words[at + i]))
    }
    pub fn ambient(&self) -> [f32; 3] {
        self.rgb(1)
    }
    pub fn diffuse(&self) -> [f32; 3] {
        self.rgb(4)
    }
    pub fn specular(&self) -> [f32; 3] {
        self.rgb(7)
    }
    /// Glide constant colour, ARGB.
    pub fn constant(&self) -> u32 {
        self.words[11]
    }
    pub fn chromakey(&self) -> Option<u32> {
        (self.words[12] != 0).then_some(self.words[13])
    }
    /// Texture ids for TMU 0 and 1.
    pub fn textures(&self) -> [u32; 2] {
        [self.words[14], self.words[15]]
    }
    /// The projector light a projected texture follows (`texgen` 0x6000).
    pub fn light(&self) -> u32 {
        self.words[16]
    }
    pub fn two_sided(&self) -> bool {
        self.words[18] != 0
    }
    /// The material of a second pass over the same triangles.
    pub fn next_pass(&self) -> u32 {
        self.words[25]
    }
    /// 1: lit from the scene's lights; 2: the vertices' own colours.
    pub fn lighting(&self) -> u32 {
        self.words[26]
    }
    /// 4: alpha from the vertices.
    pub fn vertex_alpha(&self) -> bool {
        self.words[27] == 4
    }
    /// TMU 0 mode: 0 none, 0x100 texture, 0x200 detail, ...
    pub fn tmu_mode(&self) -> [u32; 2] {
        [self.words[29], self.words[30]]
    }
    /// Texture coordinates for TMU 0 and 1: 0x1000 the vertices' first pair,
    /// 0x2000 second, 0x3000 third, 0x4000 sphere map, 0x6000 projected.
    pub fn texgen(&self) -> [u32; 2] {
        [self.words[31], self.words[32]]
    }
    /// `grTexCombine` for TMU 0: rgb function, factor, alpha function,
    /// factor, rgb invert, alpha invert.
    pub fn tex_combine(&self) -> [u32; 6] {
        [33, 35, 37, 39, 41, 43].map(|i| self.words[i])
    }
    pub fn alpha_test(&self) -> (u32, u8) {
        (self.words[45], self.words[46] as u8)
    }
    /// Clamp s, t for TMU 0 (1 = clamp).
    pub fn clamp(&self) -> [bool; 2] {
        [self.words[47] != 0, self.words[49] != 0]
    }
    pub fn mipmap(&self) -> u32 {
        self.words[51]
    }
    pub fn bilinear(&self) -> bool {
        self.words[53] != 0
    }
    /// `grColorCombine`: function, factor, local, other, invert.
    pub fn color_combine(&self) -> [u32; 5] {
        [65, 66, 67, 68, 69].map(|i| self.words[i])
    }
    /// `grAlphaCombine`: function, factor, local, other, invert.
    pub fn alpha_combine(&self) -> [u32; 5] {
        [70, 71, 72, 73, 74].map(|i| self.words[i])
    }
    /// `grAlphaBlendFunction` rgb source and destination factors.
    pub fn blend(&self) -> (u32, u32) {
        (self.words[75], self.words[76])
    }
    pub fn depth_write(&self) -> bool {
        self.words[77] != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_565_and_refuses_short() {
        let img = Image {
            format: 10,
            width: 2,
            height: 1,
            levels: 1,
            file: None,
            data: vec![0xf8, 0x00, 0x00, 0x1f],
            table: None,
        };
        assert_eq!(img.rgba().unwrap().rgba, [255, 0, 0, 255, 0, 0, 255, 255]);
        let short = Image {
            data: vec![0; 3],
            ..img
        };
        assert!(short.rgba().is_err());
    }
}
