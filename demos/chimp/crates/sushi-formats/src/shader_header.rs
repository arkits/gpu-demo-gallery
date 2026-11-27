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
    /// 0-3 float1-4, 4 D3DCOLOR, 5 ubyte4.
    pub kind: u32,
    /// Vertex register class; see [`Element::semantic`].
    pub usage: u32,
    pub usage_index: u8,
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
                kind: r.u32()?,
                usage: r.u32()?,
                usage_index: r.u8()?,
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

impl Element {
    pub fn ty(&self) -> u32 {
        self.kind
    }
    pub fn size(&self) -> usize {
        match self.kind {
            0..=3 => 4 * (self.kind as usize + 1),
            // SHORT4, SHORT4N, USHORT4N, FLOAT16_4.
            7 | 10 | 12 | 16 => 8,
            _ => 4,
        }
    }
    /// Semantic name as used by `dcl_*`, with its index.
    pub fn semantic(&self) -> (&'static str, u32) {
        let name = match self.usage {
            0 => "position",
            1 => "blendweight",
            2 => "blendindices",
            3 => "normal",
            5 | 12 => "color",
            6 | 13 => return ("color", 1),
            7 => "texcoord",
            8 => "tangent",
            9 => "binormal",
            _ => "other",
        };
        (name, u32::from(self.usage_index))
    }
}

impl ShaderHeader {
    pub fn element(&self, semantic: &str, index: u32) -> Option<&Element> {
        self.streams
            .iter()
            .flat_map(|s| &s.elements)
            .find(|e| e.semantic() == (semantic, index))
    }
}
