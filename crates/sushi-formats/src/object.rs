use crate::mesh::*;
use crate::reader::*;
use crate::texture::*;

pub const MATERIAL_MAGIC: &str = "* ATI Sushi 2.0 Material v21 *";

/// Header flag: data blocks follow.
pub const BLOCKS: u32 = 0x20_0000;

/// `.so2` object (reader 0x437d20).
#[derive(Debug, Clone)]
pub struct Object {
    pub textures: Vec<Texture>,
    pub meshes: Vec<Mesh>,
    pub bounds: Vec<u8>,
    pub flags: u32,
    pub blocks: Vec<Blocks>,
}

pub fn read_object(data: &[u8]) -> Result<Object> {
    let mut r = Reader::new(data);
    r.magic(0x18, "SOL RUN00 v00.02.01")?;
    r.magic(0x20, MATERIAL_MAGIC)?;
    let flags = r.u32()?;
    let n = r.count()?;
    let textures = read_textures(&mut r, n)?;
    let meshes = read_meshes(&mut r)?;
    let bounds = r.bytes(0x28)?.to_vec();
    let blocks = match flags & BLOCKS {
        0 => Vec::new(),
        _ => read_blocks(&mut r)?,
    };
    r.finish()?;
    Ok(Object {
        textures,
        meshes,
        bounds,
        flags,
        blocks,
    })
}
