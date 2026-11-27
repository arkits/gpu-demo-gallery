//! TreasureChest.exe loads POF parts and their textures at 0x410e70.
use std::collections::{BTreeMap, btree_map::Entry};

use glam::Vec3;
use sushi_formats::{
    archive::Archive,
    path::{FlyPath, PathFrame},
    reader::{Reader, f32_at, u32_at},
};

use crate::{Demo, Effect, Material, Mesh, Motion, Result, Scene, Vertex, asset, image_texture};

pub(crate) struct Settings {
    pub path: FlyPath,
    pub lights: Vec<FlyPath>,
}

#[derive(Default)]
struct Part<'a> {
    positions: &'a [u8],
    normals: &'a [u8],
    coords: &'a [u8],
    indices: &'a [u8],
}

fn parts(bytes: &[u8]) -> Result<Vec<Part<'_>>> {
    const MAGIC: &[u8] = b"\x89ATI\r\n\x1a\n";
    let mut r = Reader::new(bytes);
    if r.bytes(8)? != MAGIC {
        return Err("invalid POF signature".into());
    }
    let mut parts = BTreeMap::<_, Part>::new();
    while r.pos() < bytes.len() {
        let tag = r.bytes(4)?;
        let len = (r.u32()? as usize)
            .checked_sub(8)
            .ok_or("invalid POF record size")?;
        let body = r.bytes(len)?;
        if !matches!(tag, b"VRTX" | b"NRML" | b"TEXC" | b"FACE") {
            continue;
        }
        if body.len() < 20 {
            return Err("truncated POF chunk header".into());
        }
        let index = u32_at(body, 8) & 0x00ff_ffff;
        let part = parts.entry(index).or_default();
        match tag {
            b"VRTX" => part.positions = &body[20..],
            b"NRML" => part.normals = &body[20..],
            b"TEXC" if body[11] == 0 => part.coords = &body[20..],
            b"FACE" => part.indices = &body[24.min(body.len())..],
            _ => {}
        }
    }
    Ok(parts.into_values().collect())
}

fn path(bytes: &[u8]) -> Result<FlyPath> {
    let text = std::str::from_utf8(bytes)?;
    let mut rows = text.split_whitespace();
    let count: usize = rows.next().ok_or("missing POF path count")?.parse()?;
    let duration: f32 = rows.next().ok_or("missing POF path duration")?.parse()?;
    if count < 2 || duration <= 0.0 {
        return Err("invalid POF path".into());
    }
    let mut number = || -> Result<f32> { Ok(rows.next().ok_or("incomplete POF path")?.parse()?) };
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let pos = [number()?, number()?, number()?];
        let quat = [number()?, number()?, number()?, number()?];
        frames.push(PathFrame {
            pos,
            quat,
            fov: 55.0,
            flag: 0,
        });
    }
    Ok(FlyPath {
        step: duration / (count - 1) as f32,
        frames,
    })
}

pub(crate) fn load(files: &Archive) -> Result<Scene> {
    let mut scene = Scene::new(Demo::TreasureChest);
    scene.path = path(asset(files, "paths/chest.pth")?)?;
    scene.chest = Some(Settings {
        path: path(asset(files, "paths/object.pth")?)?,
        lights: ["light", "light2", "light3", "light4"]
            .into_iter()
            .map(|name| path(asset(files, &format!("paths/{name}.pth"))?))
            .collect::<Result<_>>()?,
    });
    scene.far = 250.0;
    scene.clear = [0.017, 0.018, 0.020];
    scene.motions.push(Motion::ChestLid);

    let mut textures = BTreeMap::new();
    let mut texture = |scene: &mut Scene, name: &str| -> Result<usize> {
        if name.is_empty() {
            return Ok(0);
        }
        match textures.entry(name.to_string()) {
            Entry::Occupied(v) => Ok(*v.get()),
            Entry::Vacant(v) => {
                let path = format!("media/{name}");
                let format = if name.ends_with(".tga") {
                    image::ImageFormat::Tga
                } else {
                    image::ImageFormat::Bmp
                };
                let index = image_texture(scene, asset(files, &path)?, format)?;
                v.insert(index);
                Ok(index)
            }
        }
    };
    let stage = [
        (
            "chest_bot_wood",
            "chest_front.bmp",
            "chest_front_bump.bmp",
            Effect::Lit,
            [1.0; 4],
        ),
        (
            "chest_top_wood",
            "chest_front.bmp",
            "chest_front_bump.bmp",
            Effect::Lit,
            [1.0; 4],
        ),
        (
            "chest_bot_metal",
            "",
            "gold_band.bmp",
            Effect::Metal,
            [0.84, 0.57, 0.24, 1.0],
        ),
        (
            "chest_top_metal",
            "",
            "gold_band.bmp",
            Effect::Metal,
            [0.84, 0.57, 0.24, 1.0],
        ),
        (
            "chest_ati_logo",
            "atilogo_cut.bmp",
            "atilogo_bump.bmp",
            Effect::Metal,
            [1.0, 0.86, 0.57, 1.0],
        ),
        (
            "keep_floor",
            "set_floor_b.bmp",
            "set_floor_b_bump.bmp",
            Effect::Lit,
            [1.0; 4],
        ),
        (
            "keep_walls",
            "stone_wall.bmp",
            "stone_wall_bump.bmp",
            Effect::Lit,
            [1.0; 4],
        ),
        ("keep_bars", "", "", Effect::Metal, [0.25, 0.23, 0.21, 1.0]),
        (
            "keep_door",
            "door.bmp",
            "door_bump.bmp",
            Effect::Lit,
            [1.0; 4],
        ),
        (
            "window",
            "stainedglass_rgb.bmp",
            "stainedglass_bump.bmp",
            Effect::Unlit,
            [0.68, 0.67, 0.85, 1.0],
        ),
        (
            "chalice",
            "chaliceuv_rgb.bmp",
            "chaliceuv_bump.bmp",
            Effect::Metal,
            [1.0; 4],
        ),
    ];
    let specimens = [
        (
            "ball",
            "rustchrome_rgb.bmp",
            "rustchrome_bump.bmp",
            Effect::Metal,
            [1.0; 4],
        ),
        (
            "angel_head",
            "angel_rgb.bmp",
            "angel_bump.bmp",
            Effect::Metal,
            [1.0; 4],
        ),
        (
            "ball",
            "ground2.bmp",
            "ground2bump.bmp",
            Effect::Lit,
            [1.0; 4],
        ),
        (
            "window",
            "stainedglass_rgb.bmp",
            "stainedglass_bump.bmp",
            Effect::Unlit,
            [1.0; 4],
        ),
        (
            "chalice",
            "chaliceuv_rgb.bmp",
            "chaliceuv_bump.bmp",
            Effect::Metal,
            [1.0; 4],
        ),
        (
            "skull",
            "rustchrome_rgb.bmp",
            "rustchrome_bump.bmp",
            Effect::Metal,
            [1.0; 4],
        ),
        ("box", "tiles.tga", "tilesheight.tga", Effect::Lit, [1.0; 4]),
    ];
    for (showcase, (name, base, bump, effect, tint)) in
        stage.into_iter().map(|part| (0, part)).chain(
            specimens
                .into_iter()
                .enumerate()
                .map(|(i, part)| (i as u8 + 1, part)),
        )
    {
        let material = Material {
            texture: texture(&mut scene, base)?,
            detail: texture(&mut scene, bump)?,
            third: 0,
            effect,
            alpha_cut: 0.0,
            blend: false,
            strength: if bump.is_empty() { 0.0 } else { 0.25 },
            tint,
        };
        let (scale, offset) = match name {
            "window" => (1.0, [0.0, -13.6, 7.6]),
            "chalice" => (0.014, [0.0, 0.0, 1.97]),
            _ => (1.0, [0.0; 3]),
        };
        let motion = (showcase == 0 && name.starts_with("chest_top")).then_some(0);
        let asset_parts = parts(asset(files, &format!("pof/{name}.pof"))?)?;
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut min = Vec3::splat(f32::INFINITY);
        for data in asset_parts
            .iter()
            .flat_map(|part| part.positions.chunks_exact(12))
        {
            let location = Vec3::from([0, 8, 4].map(|o| f32_at(data, o)));
            min = min.min(location);
            max = max.max(location);
        }
        let center = (min + max) * 0.5;
        let normalization = 1.1 / (max - min).max_element().max(0.001);
        for part in asset_parts {
            let count = part.positions.len() / 12;
            if part.positions.len() % 12 != 0
                || part.indices.len() % 12 != 0
                || part.normals.len() != count * 12
                || (!part.coords.is_empty() && part.coords.len() != count * 8)
            {
                return Err(format!("inconsistent POF streams in {name}").into());
            }
            let vertices = (0..count)
                .map(|i| {
                    let pos = [0, 8, 4].map(|o| f32_at(part.positions, i * 12 + o));
                    let pos = if showcase > 0 {
                        ((Vec3::from(pos) - center) * normalization + Vec3::new(-2.8, 2.65, 5.6))
                            .into()
                    } else {
                        [0, 1, 2].map(|j| pos[j] * scale + offset[[0, 2, 1][j]])
                    };
                    let normal = [0, 8, 4].map(|o| f32_at(part.normals, i * 12 + o));
                    let uv = if part.coords.is_empty() {
                        [0.0; 2]
                    } else {
                        [0, 4].map(|o| f32_at(part.coords, i * 8 + o))
                    };
                    Vertex::new(pos, normal, uv)
                })
                .collect();
            let indices = part
                .indices
                .chunks_exact(4)
                .map(|b| u32_at(b, 0))
                .collect::<Vec<_>>();
            if indices.iter().any(|&index| index as usize >= count) {
                return Err(format!("POF index outside vertices in {name}").into());
            }
            scene.meshes.push(Mesh {
                vertices,
                indices,
                material,
                motion,
                showcase,
            });
        }
    }
    Ok(scene)
}
