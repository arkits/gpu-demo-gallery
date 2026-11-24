use crate::reader::*;

/// Compiled shader header embedded in meshes and scenes (0x460710).
#[derive(Debug, Clone, Default)]
pub struct ShaderHeader {
    pub misc: Vec<(String, String)>,
    pub streams: Vec<Stream>,
    pub stream_maps: Vec<(String, Vec<u32>)>,
    pub textures: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Stream {
    pub name: String,
    pub usage: String,
    pub elements: Vec<Element>,
    pub unk30: u32,
    pub unk34: u32,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub source: String,
    pub unk1c: u32,
    pub unk20: u32,
    pub unk24: u8,
}

pub fn read_shader_header(r: &mut Reader) -> Result<ShaderHeader> {
    r.magic(0x20, "* ATI Shader File v2 *")?;
    let mut h = ShaderHeader::default();
    for _ in 0..r.count()? {
        h.misc.push((r.pstring()?, r.pstring()?));
    }
    for _ in 0..r.count()? {
        let (name, usage) = (r.pstring()?, r.pstring()?);
        let mut elements = Vec::new();
        for _ in 0..r.count()? {
            elements.push(Element {
                source: r.pstring()?,
                unk1c: r.u32()?,
                unk20: r.u32()?,
                unk24: r.u8()?,
            });
        }
        h.streams.push(Stream {
            name,
            usage,
            elements,
            unk30: r.u32()?,
            unk34: r.u32()?,
        });
    }
    for _ in 0..r.count()? {
        let name = r.pstring()?;
        let n = r.count()?;
        h.stream_maps.push((name, r.u32s(n)?));
    }
    for _ in 0..r.count()? {
        h.textures.push((r.pstring()?, r.pstring()?));
    }
    Ok(h)
}
