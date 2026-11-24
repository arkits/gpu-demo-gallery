use crate::reader::*;

pub const TEXTURE_RECORD: usize = 0x1ed;

/// A 0x1ed texture record plus its texel blob (all mips, all faces).
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub raw: Vec<u8>,
    /// Empty when the data lives in another record with the same name, or the
    /// texture is supplied by the engine.
    pub texels: Vec<u8>,
}

/// Records are stored contiguously, followed by all blobs in order.
pub fn read_textures(r: &mut Reader, n: usize) -> Result<Vec<Texture>> {
    let recs = r.records(n, TEXTURE_RECORD)?;
    recs.into_iter().map(|raw| read_texels(r, raw)).collect()
}

pub fn read_texels(r: &mut Reader, raw: &[u8]) -> Result<Texture> {
    let texels = r.bytes(u32_at(raw, 0x1c4) as usize)?.to_vec();
    Ok(Texture {
        name: cstr(raw),
        raw: raw.to_vec(),
        texels,
    })
}
