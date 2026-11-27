//! The demo's own files: OBJ models and raw images, from the unpacked folder or a pack.
use std::collections::{BTreeMap, HashMap};
use std::io::Read;

use bytemuck::{Pod, Zeroable};

use crate::Error;

/// Every file the port needs, relative to the demo folder.
pub const FILES: [&str; 15] = [
    "models/stand.obj",
    "models/cushion.obj",
    "models/shadow.obj",
    "models/glass.obj",
    "models/trapped.obj",
    "textures/logo.raw",
    "textures/velvet.raw",
    "textures/shadows.raw",
    "textures/noise.raw",
    "textures/left.raw",
    "textures/right.raw",
    "textures/bottom.raw",
    "textures/top.raw",
    "textures/back.raw",
    "textures/front.raw",
];

const MAGIC: &[u8; 8] = b"BOSPACK1";

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

impl Files {
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.0.insert(name.to_owned(), data);
    }
    pub fn get(&self, name: &str) -> Result<&[u8], Error> {
        let f = self.0.get(name).map(Vec::as_slice);
        f.ok_or_else(|| format!("missing {name}").into())
    }
    /// Magic, count, then a (name, size) table, then contents.
    pub fn pack(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend((self.0.len() as u32).to_le_bytes());
        for (n, d) in &self.0 {
            out.extend((n.len() as u32).to_le_bytes());
            out.extend(n.as_bytes());
            out.extend((d.len() as u32).to_le_bytes());
        }
        self.0.values().for_each(|d| out.extend(d));
        out
    }
    /// Opens a pack, plain or compressed with xz.
    pub fn open(data: &[u8]) -> Result<Self, Error> {
        if data.starts_with(b"\xfd7zXZ\0") {
            let mut raw = Vec::new();
            lzma_rust2::XzReader::new(data, false).read_to_end(&mut raw)?;
            return Self::unpack(&raw);
        }
        Self::unpack(data)
    }
    fn unpack(mut d: &[u8]) -> Result<Self, Error> {
        fn take<'a>(d: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
            let (a, b) = d.split_at_checked(n).ok_or("pack is cut short")?;
            *d = b;
            Ok(a)
        }
        fn int(d: &mut &[u8]) -> Result<usize, Error> {
            Ok(u32::from_le_bytes(take(d, 4)?.try_into()?) as usize)
        }
        if take(&mut d, 8)? != MAGIC {
            return Err("not a Box of Smoke pack".into());
        }
        let mut table = Vec::new();
        for _ in 0..int(&mut d)? {
            let n = int(&mut d)?;
            let name = std::str::from_utf8(take(&mut d, n)?)?.to_owned();
            table.push((name, int(&mut d)?));
        }
        let mut files = Self::default();
        for (name, size) in table {
            let data = take(&mut d, size)?.to_vec();
            files.0.insert(name, data);
        }
        if !d.is_empty() {
            return Err("pack has extra bytes".into());
        }
        Ok(files)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, Error> {
        let mut files = Self::default();
        for n in FILES {
            let p = dir.join(n);
            let d = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            files.insert(n, d);
        }
        Ok(files)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Triangles of a Wavefront OBJ file.
pub fn read_obj(text: &str) -> Result<Mesh, Error> {
    let (mut v, mut vt, mut vn) = (Vec::new(), Vec::new(), Vec::new());
    let mut seen: HashMap<[usize; 3], u32> = HashMap::new();
    let mut mesh = Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for line in text.lines() {
        let mut w = line.split_whitespace();
        match w.next() {
            Some("v") => v.push(floats::<3>(&mut w)?),
            Some("vt") => vt.push(floats::<2>(&mut w)?),
            Some("vn") => vn.push(floats::<3>(&mut w)?),
            Some("f") => {
                let mut face = Vec::new();
                for corner in w {
                    let mut key = [0; 3];
                    for (k, s) in key.iter_mut().zip(corner.split('/')) {
                        *k = if s.is_empty() { 0 } else { s.parse()? };
                    }
                    let vertex = Vertex {
                        pos: *pick(&v, key[0])?,
                        // The cushion and the glass have faces with texture index 0: none.
                        uv: match key[1] {
                            0 => [0.0; 2],
                            i => *pick(&vt, i)?,
                        },
                        normal: *pick(&vn, key[2])?,
                    };
                    let next = mesh.vertices.len() as u32;
                    let i = *seen.entry(key).or_insert(next);
                    if i == next {
                        mesh.vertices.push(vertex);
                    }
                    face.push(i);
                }
                let &[a, b, c] = face.as_slice() else {
                    return Err("only triangles are supported".into());
                };
                mesh.indices.extend([a, b, c]);
            }
            _ => {}
        }
    }
    Ok(mesh)
}

/// The vertices of an OBJ curve, in order. The autopilot follows `trapped.obj`.
pub fn read_path(text: &str) -> Result<Vec<[f32; 3]>, Error> {
    let lines = text.lines().filter_map(|l| l.trim().strip_prefix("v "));
    lines.map(|l| floats(&mut l.split_whitespace())).collect()
}

fn pick<T>(list: &[T], i: usize) -> Result<&T, Error> {
    let v = i.checked_sub(1).and_then(|i| list.get(i));
    v.ok_or_else(|| "OBJ index out of range".into())
}

fn floats<'a, const N: usize>(w: &mut impl Iterator<Item = &'a str>) -> Result<[f32; N], Error> {
    let mut out = [0.0; N];
    for o in &mut out {
        *o = w.next().ok_or("OBJ line is cut short")?.parse()?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_shares_vertices() {
        let t = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvt 0 0\nvn 0 0 1\n\
                 f 1/1/1 2/1/1 3/1/1\nf 2/1/1 4/1/1 3/1/1\n";
        let m = read_obj(t).unwrap();
        assert_eq!(m.indices, [0, 1, 2, 1, 3, 2]);
        assert_eq!(m.vertices.len(), 4);
        assert!(read_obj("f 1/1/1 2/1/1 3/1/1").is_err());
        assert_eq!(read_path("v 1 2 3\nvn 0 0 0\nv 4 5 6").unwrap().len(), 2);
    }

    #[test]
    fn pack_round_trips() {
        let mut f = Files::default();
        f.insert("a", vec![1, 2]);
        f.insert("b", vec![]);
        let p = f.pack();
        let g = Files::open(&p).unwrap();
        assert_eq!(g.get("a").unwrap(), [1, 2]);
        assert!(g.get("b").unwrap().is_empty() && g.get("c").is_err());
        assert!(Files::open(&p[..p.len() - 1]).is_err());
        assert!(Files::open(&[&p[..], &[0]].concat()).is_err());
    }
}
