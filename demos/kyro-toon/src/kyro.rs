//! The demo framework of STMicroelectronics' Kyro demos (2001), PowerVR's "D3DShell": a scene
//! exported from 3D Studio Max is compiled into the program as tables, read here from the
//! program itself. Addresses are those of the unpacked program; `docs/ENGINE.md` lists them.
use std::collections::HashMap;

use glam::{Mat4, Vec3};

use crate::Error;
use crate::image::Image;

/// A Windows program, read by the addresses its code uses.
pub struct Exe {
    data: Vec<u8>,
    base: u32,
    /// Address, size in memory, file offset, size in the file.
    sections: Vec<(u32, u32, u32, u32)>,
    resource_root: Option<u32>,
}

impl Exe {
    pub fn new(data: Vec<u8>) -> Result<Self, Error> {
        let u16_at = |o: usize| -> Option<u16> {
            Some(u16::from_le_bytes(data.get(o..o + 2)?.try_into().ok()?))
        };
        let u32_at = |o: usize| -> Option<u32> {
            Some(u32::from_le_bytes(data.get(o..o + 4)?.try_into().ok()?))
        };
        let pe = u32_at(0x3c).ok_or("not a program")? as usize;
        if data.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err("not a program".into());
        }
        let count = usize::from(u16_at(pe + 6).ok_or("cut short")?);
        let optional = usize::from(u16_at(pe + 20).ok_or("cut short")?);
        let base = u32_at(pe + 24 + 28).ok_or("cut short")?;
        let mut sections = Vec::new();
        for i in 0..count {
            let s = pe + 24 + optional + 40 * i;
            let f = |k| u32_at(s + k).ok_or("cut short");
            sections.push((f(12)?, f(8)?, f(20)?, f(16)?));
        }
        let resource_root = u32_at(pe + 24 + 96 + 16).filter(|r| *r != 0);
        if sections
            .iter()
            .any(|&(_, _, at, size)| at as usize + size as usize > data.len())
        {
            return Err("program is cut short; it may still be packed with UPX".into());
        }
        Ok(Self {
            data,
            base,
            sections,
            resource_root,
        })
    }

    fn offset(&self, rva: u32) -> Option<usize> {
        self.sections.iter().find_map(|&(va, vs, raw, rs)| {
            (va..va + vs.max(rs))
                .contains(&rva)
                .then(|| rva - va)
                .filter(|o| *o < rs)
                .map(|o| (raw + o) as usize)
        })
    }

    /// Bytes at an address, at most `n`.
    pub fn bytes(&self, va: u32, n: usize) -> &[u8] {
        let Some(o) = va.checked_sub(self.base).and_then(|r| self.offset(r)) else {
            return &[];
        };
        &self.data[o..(o + n).min(self.data.len())]
    }

    pub fn u32(&self, va: u32) -> u32 {
        let b = self.bytes(va, 4);
        b.try_into().map_or(0, u32::from_le_bytes)
    }
    pub fn i32(&self, va: u32) -> i32 {
        self.u32(va) as i32
    }
    pub fn f32(&self, va: u32) -> f32 {
        f32::from_bits(self.u32(va))
    }
    pub fn vec3(&self, va: u32) -> Vec3 {
        Vec3::new(self.f32(va), self.f32(va + 4), self.f32(va + 8))
    }
    pub fn floats(&self, va: u32, n: usize) -> Vec<f32> {
        (0..n as u32).map(|i| self.f32(va + 4 * i)).collect()
    }
    /// The text a pointer at `va` points to; none for a null pointer.
    pub fn text(&self, va: u32) -> Option<String> {
        let p = self.u32(va);
        (p != 0).then(|| self.cstr(p))
    }
    pub fn cstr(&self, va: u32) -> String {
        let b = self.bytes(va, 256);
        let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
        b[..end].iter().map(|&c| c as char).collect()
    }

    /// Resources by kind and name, both upper case: ("BITMAP", "FOO.BMP"), ("DDS", "FOO.DDS").
    pub fn resources(&self) -> HashMap<(String, String), &[u8]> {
        let mut out = HashMap::new();
        let Some(root) = self.resource_root.and_then(|r| self.offset(r)) else {
            return out;
        };
        let d = &self.data;
        let u16_at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]) as usize;
        let u32_at = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize;
        let name = |id: usize, numbered: &dyn Fn(usize) -> String| {
            if id >> 31 == 1 {
                let at = root + (id & 0x7fff_ffff);
                (0..u16_at(at))
                    .map(|k| char::from_u32(u16_at(at + 2 + 2 * k) as u32).unwrap_or('?'))
                    .collect::<String>()
                    .to_ascii_uppercase()
            } else {
                numbered(id)
            }
        };
        let entries = |at: usize| {
            let n = u16_at(at + 12) + u16_at(at + 14);
            (0..n).map(move |i| (u32_at(at + 16 + 8 * i), u32_at(at + 20 + 8 * i)))
        };
        let kind_of = |id: usize| match id {
            2 => "BITMAP".to_string(),
            10 => "RCDATA".to_string(),
            k => k.to_string(),
        };
        for (kid, kto) in entries(root) {
            if kto >> 31 == 0 {
                continue;
            }
            let kind = name(kid, &kind_of);
            for (nid, nto) in entries(root + (kto & 0x7fff_ffff)) {
                if nto >> 31 == 0 {
                    continue;
                }
                let n = name(nid, &|i| i.to_string());
                if let Some((_, leaf)) = entries(root + (nto & 0x7fff_ffff)).next() {
                    let leaf = root + (leaf & 0x7fff_ffff);
                    let (rva, size) = (u32_at(leaf) as u32, u32_at(leaf + 4));
                    if let Some(o) = self.offset(rva) {
                        out.insert((kind.clone(), n), &d[o..(o + size).min(d.len())]);
                    }
                }
            }
        }
        out
    }
}

/// A picture as the framework loads it (0x4417b0): "name.bmp" is the resource NAME.DDS if
/// there is one, else the bitmap NAME.BMP with NAME_A.BMP as its alpha.
pub fn picture(res: &HashMap<(String, String), &[u8]>, name: &str) -> Result<Image, Error> {
    let upper = name.to_ascii_uppercase();
    let stem = upper.rsplit_once('.').map_or(upper.as_str(), |s| s.0);
    if let Some(d) = res.get(&("DDS".into(), format!("{stem}.DDS"))) {
        return crate::image::read(name, d);
    }
    let bitmap = |n: String| res.get(&("BITMAP".into(), n)).map(|d| dib(d));
    let mut image = bitmap(format!("{stem}.BMP")).ok_or_else(|| format!("no picture {name}"))??;
    if let Some(alpha) = bitmap(format!("{stem}_A.BMP")) {
        let alpha = alpha?;
        if (alpha.width, alpha.height) == (image.width, image.height) {
            for (p, a) in image
                .rgba
                .chunks_exact_mut(4)
                .zip(alpha.rgba.chunks_exact(4))
            {
                p[3] = a[0];
            }
        }
    }
    Ok(image)
}

/// A bitmap resource: the header and pixels of a BMP file without its first 14 bytes.
fn dib(d: &[u8]) -> Result<Image, Error> {
    let int = |o: usize| -> Result<u32, Error> {
        Ok(u32::from_le_bytes(
            d.get(o..o + 4).ok_or("cut short")?.try_into()?,
        ))
    };
    let (size, width, height) = (int(0)? as usize, int(4)?, int(8)? as i32);
    let bits = u16::from_le_bytes([d[14], d[15]]);
    if bits != 24 || int(16)? != 0 {
        return Err(format!("bitmap of {bits} bits").into());
    }
    let rows = height.unsigned_abs() as usize;
    let stride = (width as usize * 3).div_ceil(4) * 4;
    let mut rgba = Vec::with_capacity(width as usize * rows * 4);
    for r in 0..rows {
        let r = if height > 0 { rows - 1 - r } else { r };
        let row = d.get(size + r * stride..).ok_or("cut short")?;
        let row = row.get(..width as usize * 3).ok_or("cut short")?;
        rgba.extend(row.chunks_exact(3).flat_map(|p| [p[2], p[1], p[0], 255]));
    }
    Ok(Image {
        width,
        height: rows as u32,
        rgba,
    })
}

/// Values for each frame, or for some frames with the list of them.
#[derive(Debug, Clone, Default)]
pub struct Track<T> {
    pub values: Vec<T>,
    pub keys: Option<Vec<i32>>,
}

pub trait Blend: Copy {
    fn blend(a: Self, b: Self, t: f32) -> Self;
}
impl Blend for f32 {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        (b - a) * t + a
    }
}
impl Blend for Vec3 {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        (b - a) * t + a
    }
}
/// Quaternions as (w, x, y, z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat(pub [f32; 4]);
impl Blend for Quat {
    /// D3DTMatrixQuaternionSlerp (0x445710).
    fn blend(a: Self, b: Self, t: f32) -> Self {
        let (a, mut b) = (a.0, b.0);
        let mut cos = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
        if cos < 0.0 {
            cos = -cos;
            b = b.map(|v| -v);
        }
        let (wa, wb) = if 1.0 - cos <= 0.5 {
            (1.0 - t, t)
        } else {
            let angle = cos.acos();
            let s = angle.sin();
            (((1.0 - t) * angle).sin() / s, (t * angle).sin() / s)
        };
        let q: [f32; 4] = std::array::from_fn(|i| wb * b[i] + wa * a[i]);
        let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        Quat(if n > 0.0 { q.map(|v| v / n) } else { q })
    }
}
/// Matrices are held, never blended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rows(pub [f32; 12]);
impl Blend for Rows {
    fn blend(a: Self, _: Self, _: f32) -> Self {
        a
    }
}

impl<T: Blend> Track<T> {
    /// The value at frame `cur` going on to `next` by `t` (0x403380).
    pub fn at(&self, cur: i32, next: i32, t: f32) -> Option<T> {
        let v = &self.values;
        match &self.keys {
            None => {
                let a = *v.get(cur as usize)?;
                Some(match v.get(next as usize) {
                    Some(&b) => T::blend(a, b, t),
                    None => a,
                })
            }
            Some(keys) => {
                let i = keys.iter().position(|&k| cur < k)?;
                let a = *v.get(i)?;
                if keys[i] == next
                    && let Some(&b) = v.get(i + 1)
                {
                    return Some(T::blend(a, b, t));
                }
                Some(a)
            }
        }
    }
}

fn track<T>(
    exe: &Exe,
    values: u32,
    keys: u32,
    count: usize,
    frames: usize,
    read: impl Fn(u32) -> T,
    size: u32,
) -> Option<Track<T>> {
    (values != 0).then(|| {
        let n = if keys != 0 { count } else { frames };
        Track {
            values: (0..n as u32).map(|i| read(values + i * size)).collect(),
            keys: (keys != 0).then(|| (0..count as u32).map(|i| exe.i32(keys + 4 * i)).collect()),
        }
    })
}

/// A mesh header (0x98 bytes).
#[derive(Debug, Clone)]
pub struct Mesh {
    pub name: String,
    pub material: usize,
    pub center: Vec3,
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub colors: Option<Vec<u32>>,
    /// Texture coordinates of the twelve kinds; kind 1 is the diffuse map's.
    pub uvs: [Option<Vec<[f32; 2]>>; 12],
    pub faces: Vec<[u16; 3]>,
    /// Header words 9, 10 (a float) and 11.
    pub shadow: bool,
    pub hidden: bool,
    pub drawn: bool,
}

pub fn meshes(exe: &Exe, table: u32, count: usize) -> Vec<Mesh> {
    (0..count as u32)
        .map(|i| {
            let a = table + 0x98 * i;
            let w = |k: u32| exe.u32(a + 4 * k);
            let n = w(1) as usize;
            let vec3s = |p: u32| (0..n as u32).map(|k| exe.vec3(p + 12 * k)).collect();
            Mesh {
                name: exe.cstr(w(0)),
                material: w(5) as usize,
                center: exe.vec3(a + 24),
                positions: vec3s(w(12)),
                normals: if w(25) != 0 {
                    vec3s(w(25))
                } else {
                    vec![Vec3::Y; n]
                },
                colors: (w(27) != 0)
                    .then(|| (0..n as u32).map(|k| exe.u32(w(27) + 4 * k)).collect()),
                uvs: std::array::from_fn(|k| {
                    let p = w(13 + k as u32);
                    (p != 0).then(|| {
                        (0..n as u32)
                            .map(|j| [exe.f32(p + 8 * j), exe.f32(p + 8 * j + 4)])
                            .collect()
                    })
                }),
                faces: (0..w(2))
                    .map(|f| {
                        let p = w(29) + 6 * f;
                        let b = exe.bytes(p, 6);
                        [0, 2, 4].map(|o| u16::from_le_bytes([b[o], b[o + 1]]))
                    })
                    .collect(),
                shadow: w(9) == 1,
                hidden: f32::from_bits(w(10)) == 1.0,
                drawn: w(11) == 1,
            }
        })
        .collect()
}

/// A mesh's animation (0x4c bytes).
#[derive(Debug, Clone, Default)]
pub struct MeshAnim {
    pub visibility: Option<Track<f32>>,
    pub matrix: Option<Track<Rows>>,
    pub position: Option<Track<Vec3>>,
    pub rotation: Option<Track<Quat>>,
    pub scale: Option<Track<Vec3>>,
}

pub fn mesh_anims(exe: &Exe, table: u32, count: usize, frames: usize) -> Vec<MeshAnim> {
    (0..count as u32)
        .map(|i| {
            let a = table + 0x4c * i;
            let w = |k: u32| exe.u32(a + 4 * k);
            let vec3 = |p| exe.vec3(p);
            MeshAnim {
                visibility: track(exe, w(4), w(5), w(6) as usize, frames, |p| exe.f32(p), 4),
                matrix: track(
                    exe,
                    w(7),
                    w(11),
                    w(15) as usize,
                    frames,
                    |p| Rows(std::array::from_fn(|k| exe.f32(p + 4 * k as u32))),
                    48,
                ),
                position: track(exe, w(8), w(12), w(16) as usize, frames, vec3, 12),
                // Stored x, y, z, w.
                rotation: track(
                    exe,
                    w(9),
                    w(13),
                    w(17) as usize,
                    frames,
                    |p| Quat([exe.f32(p + 12), exe.f32(p), exe.f32(p + 4), exe.f32(p + 8)]),
                    16,
                ),
                scale: track(exe, w(10), w(14), w(18) as usize, frames, vec3, 12),
            }
        })
        .collect()
}

/// A material of the exported scene (0x50 bytes).
#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    /// Words 1 to 12: ambient, diffuse, specular, (4), bump 2, light, opacity, detail, bump,
    /// environment, (11), (12) maps.
    pub maps: [Option<String>; 12],
    pub ambient: u32,
    pub diffuse: u32,
    pub specular: u32,
    pub shininess: f32,
    pub words: [u32; 20],
}

pub fn materials(exe: &Exe, table: u32, count: usize) -> Vec<Material> {
    (0..count as u32)
        .map(|i| {
            let a = table + 0x50 * i;
            let w = |k: u32| exe.u32(a + 4 * k);
            Material {
                name: exe.cstr(w(0)),
                maps: std::array::from_fn(|k| exe.text(a + 4 + 4 * k as u32)),
                ambient: w(13),
                diffuse: w(14),
                specular: w(15),
                shininess: f32::from_bits(w(16)),
                words: std::array::from_fn(|k| w(k as u32)),
            }
        })
        .collect()
}

/// A light (0x50 bytes): kind 0 point, 1 spot, 2 directional.
#[derive(Debug, Clone)]
pub struct LightSrc {
    pub name: String,
    pub kind: u32,
    pub position: Vec3,
    pub direction: Vec3,
    pub color: Vec3,
    pub theta: f32,
    pub phi: f32,
    pub range: f32,
    pub attenuation: [f32; 3],
    pub words: [u32; 20],
}

pub fn lights(exe: &Exe, table: u32, count: usize) -> Vec<LightSrc> {
    (0..count as u32)
        .map(|i| {
            let a = table + 0x50 * i;
            let w = |k: u32| exe.u32(a + 4 * k);
            let f = |k: u32| exe.f32(a + 4 * k);
            LightSrc {
                name: exe.cstr(w(0)),
                kind: w(1),
                position: exe.vec3(a + 8),
                direction: exe.vec3(a + 20),
                color: exe.vec3(a + 32),
                theta: f(12),
                phi: f(13),
                range: if f(14) < 0.0 { f32::MAX } else { f(14) },
                attenuation: [f(15), f(16), f(17)],
                words: std::array::from_fn(|k| w(k as u32)),
            }
        })
        .collect()
}

/// A camera (13 words): place, direction, up, field of view, near and far.
#[derive(Debug, Clone)]
pub struct CameraSrc {
    pub name: String,
    pub position: Vec3,
    pub direction: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone, Default)]
pub struct CameraAnim {
    pub position: Option<Track<Vec3>>,
    pub direction: Option<Track<Vec3>>,
    pub up: Option<Track<Vec3>>,
    pub fov: Option<Track<f32>>,
    pub far: Option<Track<f32>>,
}

pub fn cameras(
    exe: &Exe,
    table: u32,
    anims: u32,
    count: usize,
    frames: usize,
) -> Vec<(CameraSrc, CameraAnim)> {
    (0..count as u32)
        .map(|i| {
            let a = table + 52 * i;
            let n = anims + 24 * i;
            let w = |k: u32| exe.u32(n + 4 * k);
            let len = (w(0) as usize).max(frames);
            let v3 = |p| track(exe, p, 0, 0, len, |q| exe.vec3(q), 12);
            let f1 = |p| track(exe, p, 0, 0, len, |q| exe.f32(q), 4);
            (
                CameraSrc {
                    name: exe.cstr(exe.u32(a)),
                    position: exe.vec3(a + 4),
                    direction: exe.vec3(a + 16).normalize_or_zero(),
                    up: exe.vec3(a + 28).normalize_or_zero(),
                    fov: exe.f32(a + 40),
                    near: exe.f32(a + 44),
                    far: exe.f32(a + 48),
                },
                CameraAnim {
                    position: v3(w(1)),
                    direction: v3(w(2)),
                    up: v3(w(3)),
                    fov: f1(w(4)),
                    far: f1(w(5)),
                },
            )
        })
        .collect()
}

/// The camera at a moment: its view and projection (0x4067d0, 0x445070), in D3D's layout.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub direction: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn at(src: &CameraSrc, anim: &CameraAnim, cur: i32, next: i32, t: f32) -> Self {
        let get3 = |tr: &Option<Track<Vec3>>, d: Vec3| {
            tr.as_ref().and_then(|tr| tr.at(cur, next, t)).unwrap_or(d)
        };
        let get1 = |tr: &Option<Track<f32>>, d: f32| {
            tr.as_ref().and_then(|tr| tr.at(cur, next, t)).unwrap_or(d)
        };
        Self {
            position: get3(&anim.position, src.position),
            direction: get3(&anim.direction, src.direction).normalize_or_zero(),
            up: get3(&anim.up, src.up).normalize_or_zero(),
            fov: get1(&anim.fov, src.fov),
            near: src.near,
            far: get1(&anim.far, src.far),
        }
    }

    /// The axes as the program has them: x = up × direction, y = up, z = direction.
    pub fn view(&self) -> Mat4 {
        let (z, y) = (self.direction, self.up);
        let x = y.cross(z);
        let p = self.position;
        Mat4::from_cols_array_2d(&[
            [x.x, y.x, z.x, 0.0],
            [x.y, y.y, z.y, 0.0],
            [x.z, y.z, z.z, 0.0],
            [-x.dot(p), -y.dot(p), -z.dot(p), 1.0],
        ])
    }

    /// The field of view is across; it is 0.8 of that up and down, for 4:3. Other window
    /// shapes widen or narrow the view across instead of stretching it.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        let x = 1.0 / (self.fov * 0.5).tan();
        let y = 1.0 / (self.fov * 0.8 * 0.5).tan();
        let q = self.far / (self.far - self.near);
        let x = x * (4.0 / 3.0) / aspect;
        Mat4::from_cols_array_2d(&[
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, q, 1.0],
            [0.0, 0.0, -q * self.near, 0.0],
        ])
    }
}

/// Rotation of a quaternion (w, x, y, z) as the program builds it (0x445a50), D3D layout.
pub fn rotation(q: Quat) -> Mat4 {
    let [w, x, y, z] = q.0;
    Mat4::from_cols_array_2d(&[
        [
            1.0 - 2.0 * y * y - 2.0 * z * z,
            2.0 * x * y - 2.0 * z * w,
            2.0 * y * w + 2.0 * x * z,
            0.0,
        ],
        [
            2.0 * z * w + 2.0 * x * y,
            1.0 - 2.0 * x * x - 2.0 * z * z,
            2.0 * y * z - 2.0 * x * w,
            0.0,
        ],
        [
            2.0 * x * z - 2.0 * y * w,
            2.0 * x * w + 2.0 * y * z,
            1.0 - 2.0 * x * x - 2.0 * y * y,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// The frame and the way to the next at `seconds` (0x403380): `fps` frames a second,
/// `frames` in a loop.
pub fn frame_at(seconds: f64, fps: f64, frames: i32) -> (i32, i32, f32, i32) {
    let f = (seconds * fps).floor();
    let t = (seconds * fps - f) as f32;
    let f = f as i32;
    (f.rem_euclid(frames), (f + 1).rem_euclid(frames), t, f)
}

/// The state of one mesh at a moment (0x41c880): its world matrix and whether shown.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub world: Mat4,
    pub hidden: bool,
}

impl MeshAnim {
    /// With matrices off (DAT_00638744 = 0, 0x424d40): place, turn and size.
    pub fn pose(&self, mesh: &Mesh, cur: i32, next: i32, t: f32) -> Pose {
        let mut hidden = mesh.hidden;
        if let Some(v) = self.visibility.as_ref().and_then(|v| v.at(cur, next, t)) {
            hidden = v == 1.0;
        }
        let animated = self.position.is_some() || self.rotation.is_some() || self.scale.is_some();
        if !animated {
            return Pose {
                world: Mat4::IDENTITY,
                hidden,
            };
        }
        let p = self
            .position
            .as_ref()
            .and_then(|v| v.at(cur, next, t))
            .unwrap_or(Vec3::ZERO);
        let q = self
            .rotation
            .as_ref()
            .and_then(|v| v.at(cur, next, t))
            .unwrap_or(Quat([1.0, 0.0, 0.0, 0.0]));
        let s = self
            .scale
            .as_ref()
            .and_then(|v| v.at(cur, next, t))
            .unwrap_or(Vec3::ONE);
        // The program multiplies rotation by scale and then scale by rotation into the same
        // place: the second stands. In D3D's row order that is scale, then rotation.
        let m = rotation(q) * Mat4::from_scale(s);
        let mut world = m;
        world.w_axis = p.extend(1.0);
        Pose { world, hidden }
    }
}

/// `Frame N { Trigger Fade frames from to colour "picture" }` lines of the ini resource.
#[derive(Debug, Clone, PartialEq)]
pub struct Fade {
    pub frame: i32,
    pub frames: i32,
    pub from: f32,
    pub to: f32,
    pub color: u32,
    pub picture: Option<String>,
}

fn number(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok().map(|v| v as f64);
    }
    s.parse().ok()
}

pub fn fades(ini: &str) -> Vec<Fade> {
    let mut out = Vec::new();
    let mut frame = 0;
    for line in ini.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["Frame", n, ..] => frame = number(n).unwrap_or(0.0) as i32,
            ["Trigger", "Fade", frames, from, to, color, rest @ ..] => {
                let clamp = |v: f64| v.clamp(0.0, 1.0) as f32;
                out.push(Fade {
                    frame,
                    frames: number(frames).unwrap_or(0.0) as i32,
                    from: clamp(number(from).unwrap_or(0.0)),
                    to: clamp(number(to).unwrap_or(0.0)),
                    color: number(color).unwrap_or(0.0) as u32,
                    picture: rest.first().map(|p| p.trim_matches('"').to_string()),
                });
            }
            _ => {}
        }
    }
    out
}

/// A light's tracks (16 bytes each, 0x420690): place, direction, colour, and whether on.
#[derive(Debug, Clone, Default)]
pub struct LightAnim {
    pub position: Option<Track<Vec3>>,
    pub direction: Option<Track<Vec3>>,
    pub color: Option<Track<Vec3>>,
    pub on: Option<Track<f32>>,
}

pub fn light_anims(exe: &Exe, table: u32, count: usize, frames: usize) -> Vec<LightAnim> {
    if table == 0 {
        return vec![LightAnim::default(); count];
    }
    (0..count as u32)
        .map(|i| {
            let a = table + 16 * i;
            let w = |k: u32| exe.u32(a + 4 * k);
            let v3 = |p| track(exe, p, 0, 0, frames, |q| exe.vec3(q), 12);
            LightAnim {
                position: v3(w(0)),
                direction: v3(w(1)),
                color: v3(w(2)),
                on: track(exe, w(3), 0, 0, frames, |q| exe.f32(q), 4),
            }
        })
        .collect()
}

impl LightSrc {
    /// The light at a frame, with its tracks applied (callbacks 0x421150 to 0x421760).
    pub fn at(&self, anim: &LightAnim, cur: i32, next: i32, t: f32) -> (LightSrc, bool) {
        let mut l = self.clone();
        let get = |tr: &Option<Track<Vec3>>| tr.as_ref().and_then(|tr| tr.at(cur, next, t));
        if let Some(p) = get(&anim.position) {
            l.position = p;
        }
        if let Some(d) = get(&anim.direction) {
            l.direction = d;
        }
        if let Some(c) = get(&anim.color) {
            l.color = c;
        }
        let on = anim
            .on
            .as_ref()
            .and_then(|tr| tr.at(cur, next, t))
            .is_none_or(|v| v == 1.0);
        (l, on)
    }
}
