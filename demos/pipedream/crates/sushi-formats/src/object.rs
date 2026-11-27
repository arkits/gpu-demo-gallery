use crate::mesh::*;
use crate::reader::*;
use crate::texture::*;

pub const MATERIAL_MAGIC: &str = "* ATI Sushi 2.0 Material v18 *";

/// `.so2` object (reader 0x437d20).
#[derive(Debug, Clone)]
pub struct Object {
    pub textures: Vec<Texture>,
    pub meshes: Vec<Mesh>,
    pub bounds: Vec<u8>,
}

pub fn read_object(data: &[u8]) -> Result<Object> {
    let mut r = Reader::new(data);
    r.magic(0x18, "SOL RUN00 v00.02.01")?;
    r.magic(0x24, MATERIAL_MAGIC)?;
    let n = r.count()?;
    let textures = read_textures(&mut r, n)?;
    let meshes = read_meshes(&mut r)?;
    let bounds = r.bytes(0x28)?.to_vec();
    r.finish()?;
    Ok(Object {
        textures,
        meshes,
        bounds,
    })
}
