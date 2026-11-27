//! TreasureChest.exe loads POF parts and their textures at 0x410e70.
use std::collections::{BTreeMap, btree_map::Entry};

use glam::Vec3;
use sushi_formats::{
    archive::Archive,
    path::{FlyPath, PathFrame},
    reader::{Reader, f32_at, u32_at},
    texture::Level,
};

use crate::{Demo, Effect, Material, Mesh, Motion, Result, Scene, Vertex, asset};

pub(crate) struct Settings {
    pub path: FlyPath,
    pub lights: Vec<FlyPath>,
    pub environment: usize,
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
        environment: 0,
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
                let mut image =
                    image::load_from_memory_with_format(asset(files, &path)?, format)?.into_rgba8();
                let mut levels = Vec::new();
                loop {
                    let (width, height) = image.dimensions();
                    let next = (width > 1 || height > 1).then(|| {
                        image::imageops::resize(
                            &image,
                            (width / 2).max(1),
                            (height / 2).max(1),
                            image::imageops::FilterType::Triangle,
                        )
                    });
                    levels.push(Level {
                        width: width as usize,
                        height: height as usize,
                        rgba: image.into_raw(),
                    });
                    let Some(smaller) = next else { break };
                    image = smaller;
                }
                let index = scene.textures.len();
                scene.textures.push(levels);
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
    let mut room = Vec::new();
    for (showcase, (name, base, bump, effect, tint)) in
        stage.into_iter().map(|part| (0, part)).chain(
            specimens
                .into_iter()
                .enumerate()
                .map(|(i, part)| (i as u8 + 1, part)),
        )
    {
        let reflect = match (name, showcase) {
            ("ball", 1) | ("skull", _) => "rustchrome_reflect_rgb.bmp",
            ("ball", 3) => "ground2gloss.bmp",
            ("chalice", _) => "chaliceuv_reflect.bmp",
            ("angel_head", _) => "angel_reflect.bmp",
            ("window", _) => "stainedglass_trans.bmp",
            ("box", _) => "tilesspecexp.tga",
            _ => "",
        };
        let bump_scale = match name {
            "chest_bot_metal" | "chest_top_metal" => 0.22,
            "angel_head" | "chalice" | "window" => 0.40,
            _ => 0.54,
        };
        let material = Material {
            texture: texture(&mut scene, base)?,
            detail: texture(&mut scene, bump)?,
            third: texture(&mut scene, reflect)?,
            effect: if name == "window" {
                Effect::StainedGlass
            } else {
                effect
            },
            alpha_cut: 0.0,
            blend: false,
            strength: if bump.is_empty() { 0.0 } else { bump_scale },
            tint,
        };
        let motion = if showcase > 0 {
            let index = scene.motions.len();
            scene.motions.push(Motion::ChestShowpiece);
            Some(index)
        } else {
            name.starts_with("chest_top").then_some(0)
        };
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
                        ((Vec3::from(pos) - center) * normalization).into()
                    } else {
                        pos
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
            if showcase == 0 && name.starts_with("keep_") {
                room.push(scene.meshes.len());
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
    let environment = capture_room(&scene, &room);
    scene.chest.as_mut().unwrap().environment = scene.textures.len();
    scene.textures.push(vec![environment]);
    Ok(scene)
}

/// The original cube map is rendered from (0, 2.5, 0). Capture the static
/// masonry from the same point, in a 2D panorama shareable by both web backends.
fn capture_room(scene: &Scene, room: &[usize]) -> Level {
    struct Triangle {
        pos: Vec3,
        edge_a: Vec3,
        edge_b: Vec3,
        vertices: [Vertex; 3],
        texture: usize,
        tint: [f32; 4],
    }
    let origin = Vec3::new(0.0, 2.5, 0.0);
    let mut triangles = Vec::new();
    for &index in room {
        let mesh = &scene.meshes[index];
        for indices in mesh.indices.chunks_exact(3) {
            let vertices = [indices[0], indices[1], indices[2]].map(|i| mesh.vertices[i as usize]);
            let [a, b, c] = vertices.map(|v| Vec3::from(v.pos));
            triangles.push(Triangle {
                pos: a,
                edge_a: b - a,
                edge_b: c - a,
                vertices,
                texture: mesh.material.texture,
                tint: mesh.material.tint,
            });
        }
    }

    let (width, height) = (160, 80);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let theta = (y as f32 + 0.5) / height as f32 * std::f32::consts::PI;
        for x in 0..width {
            let phi = ((x as f32 + 0.5) / width as f32 - 0.5) * std::f32::consts::TAU;
            let ray = Vec3::new(
                phi.cos() * theta.sin(),
                theta.cos(),
                phi.sin() * theta.sin(),
            );
            let mut closest = 100.0;
            let mut hit = None;
            for triangle in &triangles {
                let p = ray.cross(triangle.edge_b);
                let determinant = p.dot(triangle.edge_a);
                if determinant.abs() < 0.00001 {
                    continue;
                }
                let delta = origin - triangle.pos;
                let u = delta.dot(p) / determinant;
                if !(0.0..=1.0).contains(&u) {
                    continue;
                }
                let q = delta.cross(triangle.edge_a);
                let v = ray.dot(q) / determinant;
                if v < 0.0 || u + v > 1.0 {
                    continue;
                }
                let distance = triangle.edge_b.dot(q) / determinant;
                if distance > 0.01 && distance < closest {
                    closest = distance;
                    hit = Some((triangle, [1.0 - u - v, u, v]));
                }
            }
            let color = if let Some((triangle, weights)) = hit {
                let uv = [0, 1].map(|i| {
                    triangle
                        .vertices
                        .iter()
                        .zip(weights)
                        .map(|(v, w)| v.uv[i] * w)
                        .sum::<f32>()
                });
                let map = &scene.textures[triangle.texture][0];
                let texel =
                    |value: f32, size: usize| (value.rem_euclid(1.0) * size as f32) as usize % size;
                let pixel = (texel(uv[1], map.height) * map.width + texel(uv[0], map.width)) * 4;
                let brightness = 0.42 + 0.58 / (1.0 + closest * closest * 0.016);
                [0, 1, 2]
                    .map(|i| (f32::from(map.rgba[pixel + i]) * triangle.tint[i] * brightness) as u8)
            } else {
                [32, 34, 39]
            };
            rgba.extend([color[0], color[1], color[2], 255]);
        }
    }
    Level {
        width,
        height,
        rgba,
    }
}
