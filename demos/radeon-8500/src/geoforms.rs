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
    pub stencils: Vec<usize>,
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
        stencils.push(image_texture(&mut scene, bytes, image::ImageFormat::Tga)?);
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
