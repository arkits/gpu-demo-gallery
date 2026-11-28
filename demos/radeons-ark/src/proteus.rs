//! Proteus objects (`.obj`, 0x40f140) and animations (`.pan`, 0x40f7d0): characters skinned
//! with up to four matrices a mesh, as the Radeon blends vertices.
use glam::{Mat4, Vec3};

use crate::Error;
use crate::d3d::d3d_matrix;
use crate::image::Image;

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let b = self
            .d
            .get(self.at..self.at + n)
            .ok_or("Proteus file cut short")?;
        self.at += n;
        Ok(b)
    }
    fn i32(&mut self) -> Result<i32, Error> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn count(&mut self) -> Result<usize, Error> {
        let n = self.i32()?;
        if !(0..1 << 24).contains(&n) {
            return Err(format!("Proteus count {n}").into());
        }
        Ok(n as usize)
    }
    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.i32()? as u32))
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], Error> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }
    fn matrix(&mut self) -> Result<Mat4, Error> {
        Ok(d3d_matrix(self.floats::<16>()?))
    }
    fn magic(&mut self, kind: &[u8]) -> Result<(), Error> {
        if self.take(8)? != b"Proteus\0" || !self.take(8)?.starts_with(kind) {
            return Err("not a Proteus file of this kind".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Material {
    pub texture: Option<Image>,
    /// Ambient, diffuse and specular as stored (0x40ea50).
    pub colors: [[f32; 3]; 3],
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub material: usize,
    /// Bones blended, in the order of the vertices' weights.
    pub bones: Vec<usize>,
    /// Place, four weights, normal, texture coordinates.
    pub vertices: Vec<[f32; 12]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Clone)]
pub struct Object {
    pub name: String,
    pub bounds: (Vec3, Vec3),
    /// Each bone's name and the inverse of its rest matrix (0x40a5c0).
    pub bones: Vec<(String, Mat4)>,
    pub materials: Vec<Material>,
    pub meshes: Vec<Mesh>,
}

impl Object {
    pub fn read(d: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { d, at: 0 };
        r.magic(b"Object")?;
        r.take(8)?;
        let lo = Vec3::from(r.floats::<3>()?);
        let hi = Vec3::from(r.floats::<3>()?);
        r.take(16)?;
        let name = cstr(r.take(64)?);
        let mut bones = Vec::new();
        for _ in 0..r.count()? {
            let n = cstr(r.take(64)?);
            bones.push((n, r.matrix()?.inverse()));
        }
        let mut materials = Vec::new();
        for _ in 0..r.count()? {
            let texture = if r.take(1)?[0] != 0 {
                let b = r.take(5)?;
                let (w, h, bpp) = (
                    u16::from_le_bytes([b[0], b[1]]) as u32,
                    u16::from_le_bytes([b[2], b[3]]) as u32,
                    b[4] as usize,
                );
                let px = r.take((w * h) as usize * bpp)?;
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for p in px.chunks_exact(bpp.max(1)) {
                    match bpp {
                        3 => rgba.extend([p[0], p[1], p[2], 255]),
                        4 => rgba.extend([p[0], p[1], p[2], p[3]]),
                        _ => rgba.extend([p[0], p[0], p[0], 255]),
                    }
                }
                Some(Image {
                    width: w.max(1),
                    height: h.max(1),
                    rgba,
                })
            } else {
                None
            };
            let c = r.floats::<9>()?;
            materials.push(Material {
                texture,
                colors: [[c[0], c[1], c[2]], [c[3], c[4], c[5]], [c[6], c[7], c[8]]],
            });
        }
        let mut meshes = Vec::new();
        for _ in 0..r.count()? {
            let (nv, nt, material, nb) = (r.count()?, r.count()?, r.count()?, r.count()?);
            let mut mbones = Vec::with_capacity(nb);
            for _ in 0..nb {
                mbones.push(r.i32()?.max(0) as usize);
            }
            let mut vertices = Vec::with_capacity(nv);
            for _ in 0..nv {
                vertices.push(r.floats::<12>()?);
            }
            let mut triangles = Vec::with_capacity(nt);
            for _ in 0..nt {
                let t = [r.i32()?, r.i32()?, r.i32()?];
                triangles.push(t.map(|i| i.clamp(0, nv.max(1) as i32 - 1) as u32));
            }
            meshes.push(Mesh {
                material,
                bones: mbones,
                vertices,
                triangles,
            });
        }
        Ok(Self {
            name,
            bounds: (lo, hi),
            bones,
            materials,
            meshes,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Animation {
    /// Seconds the whole takes (+0x104).
    pub length: f32,
    /// The object's own matrix in each frame (+0x118), and each bone's (+0x114).
    pub root: Vec<Mat4>,
    pub bones: Vec<Vec<Mat4>>,
    /// Version 2: (start, end, kind) of each event, in seconds.
    pub events: Vec<(f32, f32, i32)>,
}

impl Animation {
    pub fn read(d: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { d, at: 0 };
        r.magic(b"Animate")?;
        let h = r.take(8)?;
        let version = u16::from_le_bytes([h[2], h[3]]);
        let length = r.f32()?;
        r.take(0x24 + 0x40)?;
        let frames = r.count()?;
        let mut root = Vec::with_capacity(frames);
        for _ in 0..frames {
            root.push(r.matrix()?);
        }
        let nb = r.count()?;
        let mut bones = Vec::with_capacity(nb);
        for _ in 0..nb {
            let mut list = Vec::with_capacity(frames);
            for _ in 0..frames {
                list.push(r.matrix()?);
            }
            bones.push(list);
        }
        let mut events = Vec::new();
        if version == 2 {
            for _ in 0..r.count()? {
                let (a, b, k) = (r.f32()?, r.f32()?, r.i32()?);
                events.push((a, b, k));
            }
        }
        Ok(Self {
            length,
            root,
            bones,
            events,
        })
    }

    /// Frame position at `t` seconds (0x40a880): fmod by the length, frames spread over it.
    fn frame(&self, t: f32) -> (usize, usize, f32) {
        let n = self.root.len().max(1);
        let t = if self.length > 0.0 {
            t % self.length
        } else {
            0.0
        };
        let f = if self.length > 0.0 {
            t / self.length * (n as f32 - 1.0)
        } else {
            0.0
        };
        let i = (f.max(0.0) as usize).min(n - 1);
        (i, (i + 1).min(n - 1), f - i as f32)
    }

    /// Matrices are lerped element by element, as the original does.
    fn lerp(a: &Mat4, b: &Mat4, k: f32) -> Mat4 {
        Mat4::from_cols_array(&std::array::from_fn(|i| {
            a.to_cols_array()[i] * (1.0 - k) + b.to_cols_array()[i] * k
        }))
    }

    /// The event under way at `t` and how far through it is (0x40a880): of those whose span
    /// holds the wrapped time, the last.
    pub fn event(&self, t: f32) -> Option<(i32, f32)> {
        let t = if self.length > 0.0 {
            t % self.length
        } else {
            0.0
        };
        self.events
            .iter()
            .rfind(|e| e.0 <= t && t <= e.1)
            .map(|e| (e.2, (t - e.0) / (e.1 - e.0)))
    }

    pub fn root_at(&self, t: f32) -> Mat4 {
        let (i, j, k) = self.frame(t);
        Self::lerp(&self.root[i], &self.root[j], k)
    }

    /// Each bone's blend matrix at `t`: the inverse rest matrix, then the bone's animated one
    /// (0x40a240), as glam matrices.
    pub fn bones_at(&self, t: f32, object: &Object) -> Vec<Mat4> {
        let (i, j, k) = self.frame(t);
        self.bones
            .iter()
            .enumerate()
            .map(|(b, list)| {
                let anim = Self::lerp(&list[i], &list[j], k);
                let inv = object.bones.get(b).map_or(Mat4::IDENTITY, |x| x.1);
                anim * inv
            })
            .collect()
    }
}

fn cstr(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap_or(&[])).into_owned()
}
