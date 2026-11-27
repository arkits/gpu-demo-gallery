//! Original background inputs, stencil maps and 3D noise volume.
use sushi_formats::{
    archive::Archive,
    reader::{Reader, u32_at},
    texture::Level,
};

use crate::{Demo, Result, Scene, asset, image_texture};

pub(crate) struct Settings {
    pub noise: usize,
    pub warp: usize,
    pub stencils: Vec<(usize, f32)>,
    pub fov: f32,
}

pub(crate) fn load(files: &Archive) -> Result<Scene> {
    let mut scene = Scene::new(Demo::GeoForms);
    let mut r = Reader::new(asset(
        files,
        "textures/voronoi_noise_00_32x32x32_with_mips.vol",
    )?);
    let header = r.bytes(20)?;
    if [0, 4, 8, 12].map(|o| u32_at(header, o)) != [32, 32, 32, 4] {
        return Err("invalid GeoForms volume".into());
    }
    // Lay the 32 slices vertically for the common WebGL2/WebGPU sampling path.
    let noise = scene.textures.len();
    scene.textures.push(vec![Level {
        width: 32,
        height: 32 * 32,
        rgba: r
            .bytes(32 * 32 * 32 * 4)?
            .chunks_exact(4)
            .flat_map(|rgba| [rgba[2], rgba[1], rgba[0], rgba[3]])
            .collect(),
    }]);
    let warp = image_texture(
        &mut scene,
        asset(files, "textures/warptex1.tga")?,
        image::ImageFormat::Tga,
    )?;
    let mut stencils = Vec::new();
    for name in ["knot", "bunny", "lizard", "peace", "nv"] {
        let bytes = asset(files, &format!("stencils/{name}.tga"))?;
        let image =
            image::load_from_memory_with_format(bytes, image::ImageFormat::Tga)?.into_luma8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        let level = stencil_distance(width, height, image.as_raw());
        let index = scene.textures.len();
        scene.textures.push(vec![level]);
        stencils.push((index, width as f32 / height as f32));
    }
    let model = std::str::from_utf8(asset(files, "models/scenebs.nma")?)?;
    let fov = model
        .split_once("Camera mainCamShape")
        .and_then(|(_, t)| t.split_once("fieldOfView"))
        .and_then(|(_, t)| t.split_whitespace().next())
        .ok_or("GeoForms camera has no field of view")?
        .parse()?;
    scene.geo = Some(Settings {
        noise,
        warp,
        stencils,
        fov,
    });
    Ok(scene)
}

/// Store signed distances to the original stencil contour, in units of image height.
/// A two-pass chamfer transform keeps thin strokes continuous under interpolation.
fn stencil_distance(width: usize, height: usize, gray: &[u8]) -> Level {
    let inside = |i: usize| gray[i] >= 90;
    let distance = |to_inside: bool| {
        let mut grid = gray
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if inside(i) == to_inside {
                    0.0f32
                } else {
                    f32::INFINITY
                }
            })
            .collect::<Vec<_>>();
        let mut propagate = |x: usize, y: usize, sign: isize| {
            let i = y * width + x;
            for (dx, dy, cost) in [
                (-1, 0, 1.0),
                (0, -1, 1.0),
                (-1, -1, std::f32::consts::SQRT_2),
                (1, -1, std::f32::consts::SQRT_2),
            ] {
                if let (Some(a), Some(b)) = (
                    x.checked_add_signed(dx * sign),
                    y.checked_add_signed(dy * sign),
                ) && a < width
                    && b < height
                {
                    grid[i] = grid[i].min(grid[b * width + a] + cost);
                }
            }
        };
        for y in 0..height {
            for x in 0..width {
                propagate(x, y, 1);
            }
        }
        for y in (0..height).rev() {
            for x in (0..width).rev() {
                propagate(x, y, -1);
            }
        }
        grid
    };
    let outside = distance(true);
    let inside = distance(false);
    let rgba = (0..gray.len())
        .flat_map(|i| {
            let signed = (outside[i] - inside[i]) / height as f32;
            let encoded = ((signed * 3.0 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            [encoded, gray[i], 0, 255]
        })
        .collect();
    Level {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stencil_encodes_inside_and_outside_of_the_contour() {
        let level = stencil_distance(3, 3, &[0, 0, 0, 0, 255, 0, 0, 0, 0]);
        assert!(level.rgba[0] > 128);
        assert!(level.rgba[4 * 4] < 128);
    }
}
