//! Autodesk 3D Studio R4 `.3DS` mesh files, used by every ATB demo for its
//! scene objects (`DEMOLITE.3DS`, `LUDWIG.3DS`, `GALLEON.3DS`, …).
//!
//! Observed chunk layout (all little-endian), confirmed byte-wise against
//! Racing, Flip and Grand Bleu meshes:
//!
//! * Every chunk is `[u16 id][u32 length][payload]`; `length` includes the
//!   6-byte header. Containers nest: `0x4d4d` (main) → `0x3d3d` (editor) →
//!   `0x4000` (object).
//! * A `0x4000` object begins with a NUL-terminated name, then trimesh chunks:
//!   - `0x4110` vertices: `[u16 count][xyz f32 × count]`
//!   - `0x4140` texcoords: `[u16 count][uv f32 × count]` (optional)
//!   - `0x4120` faces: `[u16 count][ (a,b,c,flags) u16 × count ]`, followed by
//!     material (`0x4130`) and smoothing (`0x4150`) sub-chunks.
//!
//! The reader is strict: it bounds-checks every chunk length, refuses a file
//! whose top-level chunk does not consume every byte, and never reads past the
//! end. It exposes geometry only; materials map to `.3DF` textures elsewhere.

use crate::Error;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub name: String,
    /// `[x, y, z]`, as stored.
    pub vertices: Vec<[f32; 3]>,
    /// `[u, v]`, parallel to vertices when present.
    pub texcoords: Vec<[f32; 2]>,
    /// Triangle vertex indices (the 4th u16 "flags" word is dropped).
    pub faces: Vec<[u16; 3]>,
    /// Material names from `0x4130` sub-chunks, in order.
    pub materials: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene3ds {
    pub meshes: Vec<Mesh>,
}

/// A chunk cursor, checked against the parent's end on every step.
struct Reader<'a> {
    d: &'a [u8],
    o: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8], start: usize, end: usize) -> Self {
        Self { d, o: start, end }
    }

    fn header(&mut self) -> Option<(u16, usize, usize)> {
        if self.o + 6 > self.end {
            return None;
        }
        let id = u16::from_le_bytes(self.d[self.o..self.o + 2].try_into().ok()?);
        let len = u32::from_le_bytes(self.d[self.o + 2..self.o + 6].try_into().ok()?) as usize;
        if len < 6 || self.o + len > self.end {
            return None;
        }
        let payload = self.o + 6;
        self.o += len;
        Some((id, payload, self.o)) // id, payload start, chunk end
    }

    fn u16(&self, o: usize) -> Option<u16> {
        Some(u16::from_le_bytes(self.d.get(o..o + 2)?.try_into().ok()?))
    }
    fn f32(&self, o: usize) -> Option<f32> {
        Some(f32::from_le_bytes(self.d.get(o..o + 4)?.try_into().ok()?))
    }
    fn cstring(&self, start: usize, end: usize) -> String {
        let mut e = start;
        while e < end && self.d[e] != 0 {
            e += 1;
        }
        String::from_utf8_lossy(&self.d[start..e]).into_owned()
    }
}

pub fn read(bytes: &[u8]) -> Result<Scene3ds, Error> {
    if bytes.len() < 6 {
        return Err("3DS file too small".into());
    }
    let mut top = Reader::new(bytes, 0, bytes.len());
    let (id, start, end) = top.header().ok_or("3DS top-level chunk is malformed")?;
    if id != 0x4d4d {
        return Err("3DS missing main (0x4d4d) chunk".into());
    }
    if end != bytes.len() {
        return Err("3DS trailing bytes after main chunk".into());
    }
    let mut scene = Scene3ds::default();
    walk(bytes, start, end, &mut scene)?;
    Ok(scene)
}

fn walk(bytes: &[u8], start: usize, end: usize, scene: &mut Scene3ds) -> Result<(), Error> {
    let mut r = Reader::new(bytes, start, end);
    while let Some((id, payload, chunk_end)) = r.header() {
        match id {
            0x3d3d | 0x4100 => walk(bytes, payload, chunk_end, scene)?,
            0x4000 => {
                // Object name is a NUL-terminated string, then trimesh chunks.
                let mut name_end = payload;
                while name_end < chunk_end && bytes[name_end] != 0 {
                    name_end += 1;
                }
                let name = String::from_utf8_lossy(&bytes[payload..name_end]).into_owned();
                let mesh_start = (name_end + 1).min(chunk_end);
                let mut mesh = Mesh {
                    name,
                    ..Default::default()
                };
                object(bytes, mesh_start, chunk_end, &mut mesh)?;
                scene.meshes.push(mesh);
            }
            _ => {}
        }
    }
    Ok(())
}

fn object(bytes: &[u8], start: usize, end: usize, mesh: &mut Mesh) -> Result<(), Error> {
    let mut r = Reader::new(bytes, start, end);
    while let Some((id, payload, chunk_end)) = r.header() {
        match id {
            0x4100 => object(bytes, payload, chunk_end, mesh)?, // some files wrap trimesh
            0x4110 => {
                let n = r.u16(payload).ok_or("3DS vertex count missing")? as usize;
                ensure(
                    payload + 2 + n * 12 <= chunk_end,
                    "3DS vertex array overruns",
                )?;
                for i in 0..n {
                    let o = payload + 2 + i * 12;
                    mesh.vertices.push([
                        r.f32(o).unwrap(),
                        r.f32(o + 4).unwrap(),
                        r.f32(o + 8).unwrap(),
                    ]);
                }
            }
            0x4140 => {
                let n = r.u16(payload).ok_or("3DS texcoord count missing")? as usize;
                ensure(
                    payload + 2 + n * 8 <= chunk_end,
                    "3DS texcoord array overruns",
                )?;
                for i in 0..n {
                    let o = payload + 2 + i * 8;
                    mesh.texcoords
                        .push([r.f32(o).unwrap(), r.f32(o + 4).unwrap()]);
                }
            }
            0x4120 => {
                let n = r.u16(payload).ok_or("3DS face count missing")? as usize;
                ensure(payload + 2 + n * 8 <= chunk_end, "3DS face array overruns")?;
                for i in 0..n {
                    let o = payload + 2 + i * 8;
                    mesh.faces.push([
                        r.u16(o).unwrap(),
                        r.u16(o + 2).unwrap(),
                        r.u16(o + 4).unwrap(),
                    ]);
                }
                // Material / smoothing sub-chunks follow the face array.
                let mut sub = Reader::new(bytes, payload + 2 + n * 8, chunk_end);
                while let Some((sid, sp, se)) = sub.header() {
                    if sid == 0x4130 {
                        mesh.materials.push(sub.cstring(sp, se));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn ensure(ok: bool, msg: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(msg.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: u16, payload: &[u8]) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&id.to_le_bytes());
        c.extend_from_slice(&((6 + payload.len()) as u32).to_le_bytes());
        c.extend_from_slice(payload);
        c
    }

    /// A minimal valid 3DS with one triangle mesh.
    fn fixture() -> Vec<u8> {
        let mut verts = vec![0u8; 2];
        verts.extend_from_slice(&1u16.to_le_bytes()); // count=1 ... wait, one triangle needs 3
        verts.clear();
        verts.extend_from_slice(&3u16.to_le_bytes());
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for f in v {
                verts.extend_from_slice(&f.to_le_bytes());
            }
        }
        let mut faces = 1u16.to_le_bytes().to_vec();
        for i in [0u16, 1, 2, 0] {
            faces.extend_from_slice(&i.to_le_bytes());
        }
        let mut mat = b"WOOD\0".to_vec();
        mat.extend_from_slice(&1u16.to_le_bytes());
        mat.extend_from_slice(&0u16.to_le_bytes());
        faces.extend(chunk(0x4130, &mat));

        let mut obj = b"triangle\0".to_vec();
        obj.extend(chunk(0x4110, &verts));
        obj.extend(chunk(0x4120, &faces));

        let mut editor = chunk(0x4000, &obj);
        editor.extend(chunk(0x3d3e, &1u32.to_le_bytes())); // version chunk, ignored
        chunk(0x4d4d, &chunk(0x3d3d, &editor))
    }

    #[test]
    fn reads_triangle() {
        let s = read(&fixture()).unwrap();
        assert_eq!(s.meshes.len(), 1);
        let m = &s.meshes[0];
        assert_eq!(m.name, "triangle");
        assert_eq!(m.vertices.len(), 3);
        assert_eq!(m.vertices[1], [1.0, 0.0, 0.0]);
        assert_eq!(m.faces, [[0, 1, 2]]);
        assert_eq!(m.materials, ["WOOD"]);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(read(b"no").is_err());
        assert!(read(b"XXXX\x06\x00\x00\x00").is_err()); // wrong magic
        let mut d = fixture();
        d.truncate(d.len() - 2); // truncated
        assert!(read(&d).is_err());
    }
}
