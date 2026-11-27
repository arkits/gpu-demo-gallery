//! Half-float OpenEXR pictures: the skybox faces and environment cubes.
use std::io::Cursor;

use exr::math::Vec2;
use exr::prelude::*;

/// Width, height and RGBA half bits, top row first.
pub fn read(data: &[u8]) -> std::result::Result<(usize, usize, Vec<u16>), crate::Error> {
    let image = exr::image::read::read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(
            |size: Vec2<usize>, _| {
                let px = (
                    half::f16::ZERO,
                    half::f16::ZERO,
                    half::f16::ZERO,
                    half::f16::ZERO,
                );
                vec![vec![px; size.0]; size.1]
            },
            |pixels: &mut Vec<Vec<(half::f16, half::f16, half::f16, half::f16)>>,
             pos: Vec2<usize>,
             pixel: (half::f16, half::f16, half::f16, half::f16)| {
                pixels[pos.1][pos.0] = pixel;
            },
        )
        .first_valid_layer()
        .all_attributes()
        .from_buffered(Cursor::new(data))?;
    let pixels = &image.layer_data.channel_data.pixels;
    let (h, w) = (pixels.len(), pixels.first().map_or(0, Vec::len));
    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in pixels {
        for &(r, g, b, a) in row {
            rgba.extend([r, g, b, a].map(half::f16::to_bits));
        }
    }
    if rgba.len() != w * h * 4 {
        return Err(format!("EXR layer holds {} texels, not {w}x{h}", rgba.len() / 4).into());
    }
    Ok((w, h, rgba))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_truncated_files() {
        assert!(super::read(&[]).is_err());
        assert!(super::read(b"not an exr").is_err());
    }
}
