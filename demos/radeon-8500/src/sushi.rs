//! SushiDX8 v54, read from the file I/O at 0x405a50, 0x430860 and 0x431120.
use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use sushi_formats::{
    archive::Archive,
    path::read_path,
    reader::{Reader, Result as ReadResult, count_at, cstr, f32_at, u32_at},
    texture::{Level, Texture},
};

use crate::{Demo, Effect, Material, Mesh, Motion, Result, Scene, Vertex, asset};

const MAGIC: &str = "* ATI Sushi File v54 *";

fn skip(r: &mut Reader<'_>, record: &[u8], count: usize, stride: usize) -> ReadResult<()> {
    let bytes = count_at(record, count).saturating_mul(stride);
    r.bytes(bytes)?;
    Ok(())
}

type DrawRanges = BTreeMap<(usize, usize), Vec<(usize, usize)>>;

fn group_items(
    r: &mut Reader<'_>,
    groups: Vec<&[u8]>,
    mut batches: Option<&mut DrawRanges>,
) -> ReadResult<()> {
    for group in groups {
        for item in r.records(count_at(group, 4), 0x10)? {
            if let Some(batches) = batches.as_deref_mut() {
                batches
                    .entry((count_at(item, 0), count_at(item, 4)))
                    .or_default()
                    .push((count_at(item, 8), count_at(item, 12)));
            }
        }
    }
    Ok(())
}

fn visibility(
    r: &mut Reader<'_>,
    record: &[u8],
    offset: usize,
    batches: Option<&mut DrawRanges>,
) -> ReadResult<()> {
    let groups = r.records(count_at(record, offset), 0x10)?;
    skip(r, record, offset + 8, 4)?;
    skip(r, record, offset + 16, 4)?;
    group_items(r, groups, batches)
}

fn textures(scene: &mut Scene, r: &mut Reader<'_>, records: &[&[u8]]) -> ReadResult<Vec<usize>> {
    records
        .iter()
        .map(|record| {
            let bytes = r.bytes(count_at(record, 0x108))?;
            let faces = record[0x11a].max(1);
            let name = cstr(&record[..0x100]);
            let environment = faces == 6
                && ["oc00.bmp", "bub00.bmp", "cry00.bmp"]
                    .iter()
                    .any(|suffix| name.to_ascii_lowercase().ends_with(suffix));
            if bytes.is_empty() || (faces != 1 && !environment) {
                return Ok(0);
            }
            // The DX9 format uses the same texel ordering but relocated the fields.
            let mut raw = vec![0; 0x1ed];
            raw[0x100] = record[0x100];
            raw[0x1d0..0x1d4].copy_from_slice(&record[0x114..0x118]);
            raw[0x1d4] = faces;
            let texture = Texture {
                name,
                raw,
                texels: bytes.to_vec(),
            };
            if texture.format().is_none() || texture.width() == 0 || texture.height() == 0 {
                return Ok(0);
            }
            let mut decoded = texture.decode()?;
            let index = scene.textures.len();
            scene.textures.push(if environment {
                vec![cube_panorama(&decoded)]
            } else {
                decoded.remove(0)
            });
            Ok(index)
        })
        .collect()
}

/// Flatten the shipped Direct3D cubemap for the common two-dimensional material bindings.
fn cube_panorama(cube: &[Vec<Level>]) -> Level {
    let width = 4 * cube[0][0].width;
    let height = width / 2;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let theta = (row as f32 + 0.5) * std::f32::consts::PI / height as f32;
        for col in 0..width {
            let phi = ((col as f32 + 0.5) / width as f32 - 0.5) * std::f32::consts::TAU;
            let (x, y, z) = (
                theta.sin() * phi.cos(),
                theta.sin() * phi.sin(),
                theta.cos(),
            );
            let (a, b, c) = (x.abs(), y.abs(), z.abs());
            let (face, u, v, major) = if a >= b && a >= c {
                if x > 0.0 {
                    (0, -z, -y, a)
                } else {
                    (1, z, -y, a)
                }
            } else if b >= c {
                if y > 0.0 { (2, x, z, b) } else { (3, x, -z, b) }
            } else if z > 0.0 {
                (4, x, -y, c)
            } else {
                (5, -x, -y, c)
            };
            let level = &cube[face][0];
            let tx = ((u / major * 0.5 + 0.5) * level.width as f32) as usize;
            let ty = ((v / major * 0.5 + 0.5) * level.height as f32) as usize;
            let offset = (ty.min(level.height - 1) * level.width + tx.min(level.width - 1)) * 4;
            rgba.extend_from_slice(&level.rgba[offset..offset + 4]);
        }
    }
    Level {
        width,
        height,
        rgba,
    }
}

fn vec3(bytes: &[u8], offset: usize) -> Vec3 {
    Vec3::from_array([0, 4, 8].map(|n| f32_at(bytes, offset + n)))
}

fn uv(bytes: &[u8], offset: usize) -> [f32; 2] {
    [f32_at(bytes, offset), f32_at(bytes, offset + 4)]
}

fn color(bytes: &[u8], offset: usize) -> [f32; 4] {
    [2, 1, 0, 3].map(|n| f32::from(bytes[offset + n]) / 255.0)
}

fn static_vertices(bytes: &[u8], stride: usize, layout: u8) -> Vec<Vertex> {
    // The declarations passed to FUN_00420b20 use geometry byte 2.
    let (normal_at, color_at, uv_at, extra_at, bump_at) = match layout {
        2 => (None, Some(12), None, None, None),
        5..=8 | 45 => (Some(12), Some(24), Some(28), None, None),
        12 => (None, Some(12), Some(16), None, None),
        13 | 34 => (None, Some(12), Some(16), Some(24), None),
        14 => (None, Some(12), Some(20), Some(28), Some(36)),
        15 => (Some(12), Some(24), Some(28), Some(36), Some(44)),
        16 => (None, Some(12), Some(20), Some(28), None),
        18 | 19 | 41 | 46 | 50 => (Some(12), None, Some(24), None, None),
        26 => (Some(12), Some(24), Some(28), None, None),
        27 => (Some(12), None, Some(24), Some(32), None),
        47 => (Some(12), Some(24), Some(32), Some(40), Some(48)),
        _ => (None, None, None, None, None),
    };
    bytes
        .chunks_exact(stride.max(1))
        .filter(|v| v.len() >= 12)
        .map(|v| {
            let pos = vec3(v, 0);
            let normal = normal_at
                .filter(|&o| o + 12 <= v.len())
                .map_or(Vec3::Z, |o| vec3(v, o).normalize_or_zero());
            let coords = uv_at
                .filter(|&o| o + 8 <= v.len())
                .map_or([0.0; 2], |o| uv(v, o));
            let mut vertex = Vertex::new(pos.to_array(), normal.to_array(), coords);
            let [u, w] = extra_at
                .filter(|o| o + 8 <= v.len())
                .map_or(coords, |o| uv(v, o));
            let [b, c] = bump_at
                .filter(|o| o + 8 <= v.len())
                .map_or(coords, |o| uv(v, o));
            vertex.uv_extra = [u, w, b, c];
            if let Some(o) = color_at.filter(|&o| o + 4 <= v.len()) {
                vertex.color = color(v, o);
            }
            vertex
        })
        .collect()
}

fn material(shader: &str, base: usize, detail: usize, has_vertex_color: bool) -> Material {
    let name = shader.to_ascii_lowercase();
    let waterfall = name.starts_with("rfalls");
    let water = name.contains("oceanwater");
    let portal = waterfall || name.starts_with("rwater");
    let foam = name == "rfallslowerfoam.ssh";
    let distorted_sprite = name.contains("perturbationfire");
    let sky = name.contains("skys") || name.contains("cloudsky");
    let teleporter = name.starts_with("steleporter");
    let glass = name.contains("vlighttrans") || name.contains("glass") || name.contains("cornea");
    let film = match name.as_str() {
        "obubble.ssh" => Some(Effect::Bubble),
        "scrystal.ssh" => Some(Effect::Crystal),
        "senvcrystal.ssh" | "scrystaltree.ssh" => Some(Effect::ReflectiveCrystal),
        "oiridescenttrans.ssh" => Some(Effect::IridescentWing),
        "opsychedelic.ssh" | "opsychedelic_p.ssh" | "olightningglass.ssh" => {
            Some(Effect::LightningGlass)
        }
        _ => None,
    };
    let effect = if distorted_sprite {
        Effect::DistortedSprite
    } else if portal {
        Effect::PortalWater
    } else if teleporter {
        Effect::Teleporter
    } else if name.contains("terrain1") {
        Effect::Terrain
    } else if name == "scloudsky.ssh" {
        Effect::CloudSky
    } else if sky {
        Effect::Sky
    } else if water {
        Effect::Water
    } else if name.contains("caustic") {
        Effect::Caustics
    } else if name.contains("lfxface") {
        Effect::Skin
    } else if name == "ogloss2lights_pm.ssh" {
        Effect::Gloss
    } else if let Some(effect) = film {
        effect
    } else if has_vertex_color {
        Effect::Baked
    } else {
        Effect::Lit
    };
    Material {
        texture: base,
        detail: if water { base } else { detail },
        third: 0,
        effect,
        alpha_cut: if name.contains("clip")
            || name.contains("spikeplants")
            || name.contains("lfxhair")
            || name.contains("eyelash")
        {
            0.35
        } else {
            0.0
        },
        strength: if foam {
            1.0
        } else if effect == Effect::ReflectiveCrystal {
            0.4
        } else if name.contains("dot3")
            || name.contains("bumpy")
            || matches!(
                effect,
                Effect::Skin | Effect::IridescentWing | Effect::LightningGlass | Effect::Gloss
            )
        {
            0.65
        } else {
            0.0
        },
        blend: glass || teleporter || distorted_sprite || film.is_some(),
        tint: match name.as_str() {
            "senvcrystal.ssh" => [0.5, 0.0, 0.5, 0.5],
            "scrystaltree.ssh" => [0.2, 0.5, 1.0, 0.25],
            _ if glass => [1.0, 1.0, 1.0, 0.5],
            _ if waterfall => [0.55, 0.76, 0.82, 1.0],
            _ => [1.0; 4],
        },
    }
}

fn read_scene(scene: &mut Scene, data: &[u8]) -> Result<()> {
    let mut r = Reader::new(data);
    r.magic(0x20, MAGIC)?;
    let header = r.bytes(0x18a)?;
    let mut table = |offset, stride| r.records(count_at(header, offset), stride);
    table(0x9a, 0x90)?;
    let cells = table(0xa2, 0x111)?;
    let groups = table(0xb2, 0x10)?;
    table(0xba, 4)?;
    table(0xc2, 4)?;
    let geometry = table(0xca, 0x18)?;
    let draws = table(0xd2, 0x32)?;
    let lights = table(0xda, 0x19a)?;
    let tex_records = table(0xe2, 0x11f)?;
    let shaders = table(0xea, 0x36)?;
    let portals = table(0xf2, 0x282)?;
    let particles = table(0x102, 0xe6)?;
    table(0x16a, 0x755)?;
    let extra_textures = table(0x172, 0x123)?;
    table(0x17a, 0x36e)?;
    table(0x182, 0x104)?;

    for cell in cells {
        skip(&mut r, cell, 0xed, 4)?;
        skip(&mut r, cell, 0xfd, 4)?;
        skip(&mut r, cell, 0x105, 0x36e)?;
    }
    let mut batches = DrawRanges::new();
    group_items(&mut r, groups, Some(&mut batches))?;
    let mut streams = Vec::new();
    for geo in &geometry {
        let records = r.records(count_at(geo, 4), 0x24)?;
        let mut buffers = Vec::new();
        for record in records {
            let vertices = r.bytes(count_at(record, 8))?;
            let indices = r.u16s(count_at(record, 0x14))?;
            let count = count_at(record, 0);
            buffers.push((vertices, indices, vertices.len() / count.max(1)));
        }
        streams.push(buffers);
        skip(&mut r, geo, 0xc, 0x36e)?;
    }
    for draw in &draws {
        skip(&mut r, draw, 0x26, 0x36e)?;
    }
    for light in lights {
        visibility(&mut r, light, 0x140, None)?;
        visibility(&mut r, light, 0x158, None)?;
        skip(&mut r, light, 0x18e, 0x36e)?;
    }
    let tex = textures(scene, &mut r, &tex_records)?;
    for portal in portals {
        // The first portal list draws the water surface itself. The remaining
        // lists contain reflected and refracted scene geometry.
        visibility(&mut r, portal, 0x216, Some(&mut batches))?;
        visibility(&mut r, portal, 0x22e, None)?;
        visibility(&mut r, portal, 0x246, None)?;
    }
    for particle in particles {
        skip(&mut r, particle, 0xd2, 0x58)?;
        skip(&mut r, particle, 0xda, 0x4c)?;
    }
    textures(
        scene,
        &mut r,
        &extra_textures.iter().map(|t| &t[4..]).collect::<Vec<_>>(),
    )?;
    r.finish()?;

    for ((draw, buffer), mut ranges) in batches {
        let draw = draws.get(draw).ok_or("invalid scene draw")?;
        let geo = count_at(draw, 0);
        let geometry_record = geometry.get(geo).ok_or("invalid scene geometry")?;
        let layout = geometry_record[2];
        let (vertex_bytes, indices, stride) = streams
            .get(geo)
            .and_then(|s| s.get(buffer))
            .ok_or("invalid scene vertex buffer")?;
        let shader = shaders
            .get(count_at(draw, 4))
            .map_or(String::new(), |s| cstr(&s[..32]));
        let texture = |slot| {
            tex.get(u32_at(draw, 9 + slot * 5) as usize)
                .copied()
                .unwrap_or(0)
        };
        let color = !matches!(layout, 18 | 19 | 27 | 41 | 46 | 50);
        let mut mat = material(&shader, texture(0usize), texture(1usize), color);
        mat.third = texture(2usize);
        if shader.eq_ignore_ascii_case("rFallsLowerFoam.ssh") {
            mat.third = texture(3usize);
        }
        let vertex_data = static_vertices(vertex_bytes, *stride, layout);
        ranges.sort_unstable();
        ranges.dedup();
        let mut remap = vec![u32::MAX; vertex_data.len()];
        let mut vertices = Vec::new();
        let mut out = Vec::new();
        for (start, count) in ranges {
            let part = indices
                .get(start..start.saturating_add(count))
                .ok_or("invalid index range")?;
            for &i in part {
                let i = usize::from(i);
                let dest = remap.get_mut(i).ok_or("scene index outside vertices")?;
                if *dest == u32::MAX {
                    *dest = vertices.len() as u32;
                    vertices.push(vertex_data[i]);
                }
                out.push(*dest);
            }
        }
        if !out.is_empty() {
            scene.meshes.push(Mesh {
                vertices,
                indices: out,
                material: mat,
                motion: None,
                showcase: 0,
            });
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Stream<'a> {
    layout: u32,
    encoding: u32,
    stride: usize,
    data: &'a [u8],
}

fn streams<'a>(r: &mut Reader<'a>) -> ReadResult<Vec<Stream<'a>>> {
    let header = r.bytes(8)?;
    (0..count_at(header, 0))
        .map(|_| {
            let h = r.bytes(0x28)?;
            let stride = count_at(h, 4);
            let size = stride.saturating_mul(count_at(h, 8));
            Ok(Stream {
                layout: u32_at(h, 0x20),
                encoding: u32_at(h, 0),
                stride,
                data: r.bytes(size)?,
            })
        })
        .collect()
}

struct IndexData<'a> {
    streams: [usize; 2],
    indices: Vec<u16>,
    commands: Vec<&'a [u8]>,
}

fn index_sets<'a>(r: &mut Reader<'a>) -> ReadResult<Vec<Vec<IndexData<'a>>>> {
    let count = r.count()?;
    (0..count)
        .map(|_| {
            let header = r.bytes(8)?;
            (0..count_at(header, 0))
                .map(|_| {
                    let h = r.bytes(0x34)?;
                    Ok(IndexData {
                        streams: [4, 8].map(|o| u32_at(h, o) as usize),
                        indices: r.u16s(count_at(h, 0x1c))?,
                        commands: r.records(count_at(h, 0x28), 0x10)?,
                    })
                })
                .collect()
        })
        .collect()
}

#[derive(Clone)]
pub(crate) struct Track {
    pub duration: f32,
    matrices: Arc<[Mat4]>,
    stride: usize,
}

impl Track {
    pub fn animation_time(&self, time: f32) -> f32 {
        if self.duration > 0.0 {
            time.rem_euclid(self.duration)
        } else {
            0.0
        }
    }

    pub fn matrix(&self, time: f32, joint: usize) -> Mat4 {
        if self.stride == 0 || joint >= self.stride {
            return Mat4::IDENTITY;
        }
        let frames = self.matrices.len() / self.stride;
        if frames == 0 {
            return Mat4::IDENTITY;
        }
        let phase = (self.animation_time(time) / self.duration.max(0.001)) * (frames - 1) as f32;
        let a = phase as usize;
        let b = (a + 1).min(frames - 1);
        let a = self.matrices[a * self.stride + joint];
        let b = self.matrices[b * self.stride + joint];
        a * (1.0 - phase.fract()) + b * phase.fract()
    }
}

struct Animation<'a> {
    track: Track,
    frames: Vec<Vec<Stream<'a>>>,
    sets: Vec<Vec<IndexData<'a>>>,
}

fn animation(data: &[u8]) -> Result<Animation<'_>> {
    let mut r = Reader::new(data);
    r.magic(0x18, "SOA RUN00 v00.01.11")?;
    r.magic(0x20, MAGIC)?;
    let flags = r.u32()?;
    let head = r.bytes(0xc)?;
    let frames = (0..count_at(head, 4))
        .map(|_| streams(&mut r))
        .collect::<ReadResult<_>>()?;
    let head = r.bytes(0x10)?;
    let duration = f32_at(head, 0);
    let count = count_at(head, 4);
    let stride = count_at(head, 8);
    if flags & 0x20000 == 0 {
        return r.bad("expected matrix animation").map_err(Into::into);
    }
    let rows = count
        .checked_mul(stride)
        .ok_or("animation matrix count overflow")?;
    let matrices = r.records(rows, 64)?.into_iter().map(matrix).collect();
    let sets = index_sets(&mut r)?;
    r.bytes(4)?; // render flags
    let bounds = r.count()?;
    r.records(bounds, 0x28)?;
    r.bytes(0x28)?; // union bounds
    r.finish()?;
    Ok(Animation {
        track: Track {
            duration,
            matrices,
            stride,
        },
        frames,
        sets,
    })
}

fn matrix(bytes: &[u8]) -> Mat4 {
    Mat4::from_cols_array(&std::array::from_fn(|i| f32_at(bytes, i * 4)))
}

#[derive(Clone, Copy)]
pub(crate) struct Skin {
    pub weights: [f32; 4],
    pub joints: [usize; 4],
}

fn unpack_short(bytes: &[u8], offset: usize) -> f32 {
    f32::from(i16::from_le_bytes([bytes[offset], bytes[offset + 1]])) / 32767.0
}

fn packed(
    stream: &Stream<'_>,
    aux: Option<&Stream<'_>>,
    i: usize,
    scale: Vec3,
    bias: Vec3,
) -> Vertex {
    let b = &stream.data[i * stream.stride..][..stream.stride];
    if stream.encoding < 83 || stream.encoding == 88 {
        if stream.encoding == 37 {
            let uv_offset = if stream.layout == 43 { 16 } else { 12 };
            let uv = aux
                .and_then(|s| s.data.get(i * s.stride..(i + 1) * s.stride))
                .filter(|b| b.len() >= uv_offset + 8)
                .map_or([0.0; 2], |b| uv(b, uv_offset));
            return Vertex::new(
                vec3(b, 0).into(),
                vec3(b, 12).normalize_or_zero().into(),
                uv,
            );
        }
        return object_vertex(stream, i);
    }
    let pos = Vec3::from_array([0, 2, 4].map(|o| unpack_short(b, o))) * scale + bias;
    let (n, tex) = if stream.stride == 24 {
        // 86: SHORT4 position, UBYTE4 weights, UBYTE4 normal, SHORT2 UV, UBYTE4 tangent.
        (&b[12..16], &b[16..20])
    } else {
        let tex = aux
            .and_then(|a| a.data.get(i * a.stride + 4..i * a.stride + 8))
            .unwrap_or(&[0; 4]);
        (&b[8..12], tex)
    };
    let normal = Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32) / 127.5 - Vec3::ONE;
    Vertex::new(
        pos.to_array(),
        normal.normalize_or_zero().to_array(),
        [unpack_short(tex, 0), unpack_short(tex, 2)],
    )
}

fn object_skin(
    stream: &Stream<'_>,
    aux: Option<&Stream<'_>>,
    i: usize,
    palette: &[Option<usize>; 256],
) -> Result<Skin> {
    let bytes = &stream.data[i * stream.stride..][..stream.stride];
    let aux_bytes = aux.and_then(|s| s.data.get(i * s.stride..(i + 1) * s.stride));
    let mut slots = [0, 1, 2, 3];
    let weights = match stream.encoding {
        86 => bytes[8..12].try_into().map(byte_weights)?,
        83 => {
            let aux = aux_bytes.ok_or("missing packed skin attributes")?;
            aux[..4].try_into().map(byte_weights)?
        }
        37 => {
            let aux = aux_bytes.ok_or("missing float skin attributes")?;
            if stream.layout == 43 {
                slots = [12, 13, 14, 15].map(|o| usize::from(aux[o]));
            }
            float_weights(aux, 0)
        }
        88 => {
            slots = [24, 25, 26, 27].map(|o| usize::from(bytes[o]));
            float_weights(bytes, 12)
        }
        _ => float_weights(bytes, 12),
    };
    let mut joints = [0; 4];
    for (i, (&slot, &weight)) in slots.iter().zip(&weights).enumerate() {
        if weight > 0.00001 {
            joints[i] = palette[slot].ok_or_else(|| format!("unbound skin palette slot {slot}"))?;
        }
    }
    Ok(Skin { weights, joints })
}

fn byte_weights(bytes: [u8; 4]) -> [f32; 4] {
    bytes.map(|w| f32::from(w) / 255.0)
}

fn float_weights(bytes: &[u8], offset: usize) -> [f32; 4] {
    let [x, y, z] = [0, 4, 8].map(|o| f32_at(bytes, offset + o).clamp(0.0, 1.0));
    [x, y, z, (1.0 - x - y - z).max(0.0)]
}

fn object_vertex(stream: &Stream<'_>, index: usize) -> Vertex {
    let v = &stream.data[index * stream.stride..][..stream.stride];
    let (uv_offset, normal) = match stream.layout {
        3 => (24, Vec3::Z),
        9 | 19 | 37 | 48 => (36, vec3(v, 24)),
        31 => (40, vec3(v, 24)),
        43 => (40, vec3(v, 28)),
        _ => (stream.stride.saturating_sub(8), Vec3::Z),
    };
    Vertex::new(
        vec3(v, 0).to_array(),
        normal.normalize_or_zero().to_array(),
        uv(v, uv_offset),
    )
}

fn object(
    scene: &mut Scene,
    model: &[u8],
    anim: &Animation<'_>,
    start: f32,
    origin: Mat4,
    model_name: &str,
) -> Result<()> {
    let mut r = Reader::new(model);
    r.magic(0x18, "SOL RUN00 v00.01.11")?;
    r.magic(0x20, MAGIC)?;
    let flags = r.u32()?;
    let count = r.count()?;
    let materials = r.records(count, 0x26)?;
    let count = r.count()?;
    let tex_records = r.records(count, 0x11f)?;
    let tex = textures(scene, &mut r, &tex_records)?;
    let static_streams = streams(&mut r)?;
    let sets = index_sets(&mut r)?;
    let joints = r.count()?;
    r.records(joints, if flags & 0x20000 == 0 { 28 } else { 64 })?;
    let bounds = r.bytes(40)?;
    let lo = vec3(bounds, 0);
    let hi = vec3(bounds, 12);
    let scale = (hi - lo) * 0.5;
    let bias = (hi + lo) * 0.5;
    r.finish()?;

    let animated = !anim.frames.is_empty();
    let sets = if animated { &anim.sets } else { &sets };
    let Some(draws) = sets.iter().find(|set| !set.is_empty()) else {
        return Err(format!("{model_name} contains no indexed draws").into());
    };
    for draw in draws {
        let [main, aux] = draw.streams;
        let streams = anim.frames.first().unwrap_or(&static_streams);
        let stream = streams.get(main).ok_or("invalid object stream")?;
        let mut shader = 0;
        let mut texture_slots = [0; 6];
        let mut palette = [None; 256];
        let mut meshes = BTreeMap::<(usize, [usize; 6]), (Vec<u32>, Vec<Skin>)>::new();
        for command in &draw.commands {
            let a = u32_at(command, 4) as usize;
            let b = u32_at(command, 8) as usize;
            match u32_at(command, 0) {
                1 if a < materials.len() => shader = a,
                2 if a < palette.len() => palette[a] = Some(b),
                3 if a < texture_slots.len() => texture_slots[a] = tex.get(b).copied().unwrap_or(0),
                0 => {
                    let indices = draw.indices.get(a..=b).ok_or("invalid object draw range")?;
                    let (out, skin) = meshes.entry((shader, texture_slots)).or_default();
                    for &index in indices {
                        if usize::from(index) >= stream.data.len() / stream.stride.max(1) {
                            return Err("object index outside vertex stream".into());
                        }
                        out.push(u32::from(index));
                        skin.push(object_skin(
                            stream,
                            streams.get(aux),
                            usize::from(index),
                            &palette,
                        )?);
                    }
                }
                _ => {}
            }
        }
        for ((shader, slots), (indices, skin)) in meshes {
            let label = cstr(&materials.get(shader).ok_or("object material missing")?[..32]);
            if label.to_ascii_lowercase().contains("shadow") {
                continue;
            }
            let mut mat = material(&label, slots[0], slots[1], false);
            mat.third = slots[2];
            if scene.demo == Demo::Dolphin {
                mat.effect = Effect::Caustics;
                mat.detail = if scene.textures.len() > 2 { 2 } else { 0 };
                mat.strength = 0.22;
            }
            if label.eq_ignore_ascii_case("oLFXEyeCornea.ssh") {
                mat.tint[3] = 0.10;
            }
            if label.eq_ignore_ascii_case("oLFXHair.ssh") {
                mat.alpha_cut = 0.06;
            }
            // A source vertex can appear in several draw commands whose
            // palette slots point to different joints.
            let mut sources = Vec::new();
            let mut weights = Vec::new();
            let mut remap = BTreeMap::new();
            let indices = indices
                .into_iter()
                .zip(skin)
                .map(|(index, skin)| {
                    *remap.entry((index, skin.joints)).or_insert_with(|| {
                        let dest = sources.len() as u32;
                        sources.push(index as usize);
                        weights.push(skin);
                        dest
                    })
                })
                .collect();
            let (vertices, motion) = if animated {
                let count = stream.data.len() / stream.stride.max(1);
                let mut poses: Vec<Arc<[Vertex]>> = Vec::with_capacity(anim.frames.len());
                for frame in &anim.frames {
                    let updates = frame.get(main).ok_or("missing animated stream")?;
                    if updates.data.is_empty()
                        && let Some(last) = poses.last()
                    {
                        poses.push(last.clone());
                        continue;
                    }
                    let previous = updates;
                    if previous.data.len() / previous.stride.max(1) != count {
                        return Err("animated vertex count changed".into());
                    }
                    let vertices = sources
                        .iter()
                        .map(|&i| packed(previous, streams.get(aux), i, scale, bias))
                        .collect();
                    poses.push(vertices);
                }
                let vertices = poses[0].to_vec();
                (
                    vertices,
                    Motion::Morph {
                        origin,
                        start,
                        track: anim.track.clone(),
                        frames: poses,
                        skin: weights,
                    },
                )
            } else {
                let vertices = sources.iter().map(|&i| object_vertex(stream, i)).collect();
                let pn_level = if scene.demo == Demo::Dolphin { 3 } else { 1 };
                (
                    vertices,
                    Motion::Skeletal {
                        origin,
                        start,
                        track: anim.track.clone(),
                        skin: weights,
                        pn_level,
                    },
                )
            };
            let index = scene.motions.len();
            scene.motions.push(motion);
            scene.meshes.push(Mesh {
                vertices,
                indices,
                material: mat,
                motion: Some(index),
                showcase: 0,
            });
        }
    }
    Ok(())
}

pub(crate) fn load(demo: Demo, files: &Archive) -> Result<Scene> {
    let mut scene = Scene::new(demo);
    let mut archive = None;
    if let Some(bytes) = files.get("sushi.ati") {
        archive = Some(Archive::open(bytes)?);
    }
    let content = archive.as_ref().unwrap_or(files);
    let (scene_name, horizon, default_fov, clear) = match demo {
        Demo::Dolphin => ("dolphin.2su", None, 75.0, [0.008, 0.045, 0.075]),
        Demo::Nature => ("falls.2su", Some("sky.2su"), 65.0, [0.22, 0.35, 0.50]),
        Demo::Rachel => ("rachel.2su", None, 25.0, [0.016, 0.016, 0.022]),
        Demo::Island1 => ("island1.ati", Some("horizon.ati"), 65.0, [0.18, 0.27, 0.34]),
        Demo::Island2 => ("island2.ati", Some("horizon.ati"), 65.0, [0.18, 0.27, 0.34]),
        Demo::Island3 => ("island3.ati", Some("horizon.ati"), 65.0, [0.18, 0.27, 0.34]),
        _ => unreachable!(),
    };
    let stem = scene_name.split_once('.').unwrap().0;
    scene.fov = default_fov;
    scene.initial_time = match demo {
        Demo::Island1 => 12.0,
        Demo::Island2 | Demo::Island3 => 6.0,
        _ => 0.0,
    };
    scene.clear = clear;
    scene.path = read_path(std::str::from_utf8(asset(
        files,
        &format!("flypaths/{stem}.pth"),
    )?)?)?;
    if scene.path.frames.is_empty() || scene.path.step <= 0.0 {
        return Err("empty camera path".into());
    }
    if let Some(horizon) = horizon {
        read_scene(&mut scene, asset(content, &format!("scenes/{horizon}"))?)?;
    }
    read_scene(&mut scene, asset(content, &format!("scenes/{scene_name}"))?)?;

    let text = std::str::from_utf8(asset(content, &format!("objects/{stem}.ini"))?)?;
    let mut animations = BTreeMap::new();
    for line in text.lines().skip(1).filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 || !matches!(fields[0], "Global" | "Local") {
            return Err(format!("invalid object entry {line:?}").into());
        }
        let origin = if fields[0] == "Local" {
            let values = fields[4..]
                .iter()
                .map(|x| x.parse::<f32>())
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Mat4::from_cols_array(&values.try_into().map_err(|_| "invalid object placement")?)
        } else {
            Mat4::IDENTITY
        };
        let model = asset(content, &format!("objects/{}", fields[2]))?;
        let animation_name = fields[3].to_ascii_lowercase();
        let anim = match animations.entry(animation_name) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let bytes = asset(content, &format!("objects/{}", entry.key()))?;
                entry.insert(animation(bytes).map_err(|err| format!("{}: {err}", fields[3]))?)
            }
        };
        object(
            &mut scene,
            model,
            anim,
            fields[1].parse()?,
            origin,
            fields[2],
        )
        .map_err(|err| format!("{}: {err}", fields[2]))?;
    }
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_models_resolve_draw_palettes_for_indexed_and_morph_streams() {
        let mut palette = [None; 256];
        palette[5] = Some(14);
        palette[19] = Some(23);

        let mut rigid = [0; 80];
        rigid[12..16].copy_from_slice(&0.75f32.to_le_bytes());
        rigid[24..28].copy_from_slice(&[19, 0, 0, 5]);
        let indexed = Stream {
            encoding: 88,
            layout: 43,
            stride: 80,
            data: &rigid,
        };

        let positions = [0; 24];
        let mut attributes = [0; 56];
        attributes[..12].copy_from_slice(&rigid[12..24]);
        attributes[12..16].copy_from_slice(&rigid[24..28]);
        let morph = Stream {
            encoding: 37,
            layout: 43,
            stride: 24,
            data: &positions,
        };
        let aux = Stream {
            encoding: 89,
            layout: 43,
            stride: 56,
            data: &attributes,
        };

        for (stream, aux) in [(&indexed, None), (&morph, Some(&aux))] {
            let skin = object_skin(stream, aux, 0, &palette).unwrap();
            assert_eq!(skin.weights, [0.75, 0.0, 0.0, 0.25]);
            assert_eq!(skin.joints, [23, 0, 0, 14]);
        }
    }

    #[test]
    fn wet_terrain_and_ocean_uvs_follow_the_original_vertex_declarations() {
        for (layout, stride, uv_offset) in [(47, 92, 32), (26, 60, 28)] {
            let mut bytes = vec![0; stride];
            bytes[uv_offset..uv_offset + 4].copy_from_slice(&0.25f32.to_le_bytes());
            bytes[uv_offset + 4..uv_offset + 8].copy_from_slice(&0.75f32.to_le_bytes());
            assert_eq!(static_vertices(&bytes, stride, layout)[0].uv, [0.25, 0.75]);
        }
    }

    #[test]
    fn rachel_combined_stream_keeps_weights_normals_and_uvs_distinct() {
        let bytes = [
            0, 0, 0, 0, 255, 127, 255, 127, // SHORT4 position
            192, 63, 0, 0, // UBYTE4 blend weights
            127, 127, 255, 0, // UBYTE4 normal
            0, 64, 255, 127, // SHORT2 texture coordinates
            255, 127, 127, 0, // UBYTE4 tangent
        ];
        let stream = Stream {
            layout: 24,
            encoding: 86,
            stride: bytes.len(),
            data: &bytes,
        };
        let vertex = packed(&stream, None, 0, Vec3::ONE, Vec3::ZERO);
        assert!(vertex.normal[2] > 0.99);
        assert!((vertex.uv[0] - 0.5).abs() < 0.001);
        assert_eq!(vertex.uv[1], 1.0);
        let mut palette = [None; 256];
        palette[0] = Some(3);
        palette[1] = Some(4);
        assert_eq!(
            object_skin(&stream, None, 0, &palette).unwrap().weights,
            [192.0 / 255.0, 63.0 / 255.0, 0.0, 0.0]
        );
    }
}
