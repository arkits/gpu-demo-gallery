//! DirectX .X files as the demo ships them: text ("txt") or binary ("bin"), either of them
//! compressed with MSZIP ("tzip", "bzip"), 32-bit floats. The result is a tree of data
//! objects whose numbers and strings are kept in order; `Mesh` and the rest read it.
use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};

use crate::Error;

/// A value of a data object, in file order.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Num(f64),
    Str(String),
}

#[derive(Debug, Clone, Default)]
pub struct Node {
    pub kind: String,
    pub name: String,
    pub values: Vec<Val>,
    pub children: Vec<Node>,
    /// `{ Name }` references inside the object.
    pub refs: Vec<String>,
}

impl Node {
    pub fn child(&self, kind: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.kind == kind)
    }
    pub fn all<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.kind == kind)
    }
    pub fn nums(&self) -> Vec<f64> {
        self.values
            .iter()
            .filter_map(|v| match v {
                Val::Num(n) => Some(*n),
                Val::Str(_) => None,
            })
            .collect()
    }
    pub fn strings(&self) -> Vec<&str> {
        self.values
            .iter()
            .filter_map(|v| match v {
                Val::Str(s) => Some(s.as_str()),
                Val::Num(_) => None,
            })
            .collect()
    }
}

/// Reads a whole file into its top-level data objects (templates are skipped).
pub fn parse(d: &[u8]) -> Result<Vec<Node>, Error> {
    if d.len() < 16 || &d[..4] != b"xof " {
        return Err("not a .X file".into());
    }
    let format = &d[8..12];
    let body = match format {
        b"tzip" | b"bzip" => mszip(&d[16..])?,
        _ => d[16..].to_vec(),
    };
    match format {
        b"txt " | b"tzip" => Text::new(&body).objects(),
        b"bin " | b"bzip" => Binary { d: &body, at: 0 }.objects(),
        f => Err(format!("X format {:?}", String::from_utf8_lossy(f)).into()),
    }
}

/// MSZIP: the size, then blocks of (u16 size out, u16 size in, "CK", deflate), each block
/// able to refer back into the previous.
fn mszip(d: &[u8]) -> Result<Vec<u8>, Error> {
    let total = u32::from_le_bytes(d.get(..4).ok_or("cut short")?.try_into()?) as usize;
    let mut out = Vec::with_capacity(total);
    let mut at = 4;
    while at + 4 <= d.len() {
        let size_out = u16::from_le_bytes([d[at], d[at + 1]]) as usize;
        let size_in = u16::from_le_bytes([d[at + 2], d[at + 3]]) as usize;
        at += 4;
        let block = d.get(at..at + size_in).ok_or("cut short")?;
        if &block[..2] != b"CK" {
            return Err("MSZIP block without CK".into());
        }
        let start = out.len();
        inflate(&block[2..], &mut out, start)?;
        if out.len() - start != size_out {
            out.resize(start + size_out, 0);
        }
        at += size_in;
    }
    Ok(out)
}

struct Bits<'a> {
    d: &'a [u8],
    at: usize,
    buf: u64,
    n: u32,
}

impl Bits<'_> {
    fn need(&mut self, k: u32) {
        while self.n < k {
            let b = self.d.get(self.at).copied().unwrap_or(0);
            self.at += 1;
            self.buf |= u64::from(b) << self.n;
            self.n += 8;
        }
    }
    fn get(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        self.need(k);
        let v = (self.buf & ((1u64 << k) - 1)) as u32;
        self.buf >>= k;
        self.n -= k;
        v
    }
    fn align(&mut self) {
        let r = self.n % 8;
        self.get(r);
    }
    fn exhausted(&self) -> bool {
        self.at > self.d.len() + 4
    }
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut symbols = vec![0; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Self { counts, symbols }
    }
    fn decode(&self, b: &mut Bits) -> Result<u16, Error> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1) as i32;
            let count = i32::from(self.counts[len]);
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err("bad Huffman code".into())
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Raw deflate onto the end of `out`, which back-references may reach into. A stored block
/// whose second length does not match is read without it: two of the demo's files were
/// written so.
fn inflate(d: &[u8], out: &mut Vec<u8>, _start: usize) -> Result<(), Error> {
    let mut b = Bits {
        d,
        at: 0,
        buf: 0,
        n: 0,
    };
    loop {
        if b.exhausted() {
            return Err("deflate runs past its block".into());
        }
        let last = b.get(1);
        match b.get(2) {
            0 => {
                b.align();
                let len = b.get(16) as usize;
                let at = b.at - (b.n / 8) as usize;
                let nlen = u16::from_le_bytes([
                    d.get(at).copied().unwrap_or(0),
                    d.get(at + 1).copied().unwrap_or(0),
                ]);
                let skip = if nlen == !(len as u16) { 2 } else { 0 };
                let mut i = at + skip;
                b = Bits {
                    d,
                    at: i,
                    buf: 0,
                    n: 0,
                };
                for _ in 0..len {
                    out.push(d.get(i).copied().unwrap_or(0));
                    i += 1;
                }
                b.at = i;
            }
            kind @ (1 | 2) => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [0u8; 288];
                    l[..144].fill(8);
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    l[280..].fill(8);
                    (Huffman::new(&l), Huffman::new(&[5u8; 30]))
                } else {
                    let hlit = b.get(5) as usize + 257;
                    let hdist = b.get(5) as usize + 1;
                    let hclen = b.get(4) as usize + 4;
                    const ORDER: [usize; 19] = [
                        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
                    ];
                    let mut cl = [0u8; 19];
                    for &o in &ORDER[..hclen] {
                        cl[o] = b.get(3) as u8;
                    }
                    let ch = Huffman::new(&cl);
                    let mut lengths = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < hlit + hdist {
                        let s = ch.decode(&mut b)?;
                        match s {
                            0..=15 => {
                                lengths[i] = s as u8;
                                i += 1;
                            }
                            16 => {
                                let prev =
                                    *lengths.get(i.wrapping_sub(1)).ok_or("repeat at start")?;
                                for _ in 0..3 + b.get(2) {
                                    *lengths.get_mut(i).ok_or("too many lengths")? = prev;
                                    i += 1;
                                }
                            }
                            17 => i += 3 + b.get(3) as usize,
                            _ => i += 11 + b.get(7) as usize,
                        }
                    }
                    (
                        Huffman::new(&lengths[..hlit]),
                        Huffman::new(&lengths[hlit..]),
                    )
                };
                loop {
                    let s = lit.decode(&mut b)?;
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        let s = (s - 257) as usize;
                        let len = LEN_BASE.get(s).ok_or("bad length")?
                            + b.get(u32::from(LEN_EXTRA[s])) as u16;
                        let ds = dist.decode(&mut b)? as usize;
                        let back = DIST_BASE.get(ds).ok_or("bad distance")?
                            + b.get(u32::from(DIST_EXTRA[ds])) as u16;
                        let from = out
                            .len()
                            .checked_sub(back as usize)
                            .ok_or("distance too far back")?;
                        for k in 0..len as usize {
                            let v = out[from + k];
                            out.push(v);
                        }
                    }
                }
            }
            _ => return Err("bad deflate block".into()),
        }
        if last == 1 {
            return Ok(());
        }
    }
}

// ---------- text

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Num(f64),
    Str(String),
    Open,
    Close,
    Guid,
}

struct Text {
    toks: Vec<Tok>,
    at: usize,
}

impl Text {
    fn new(d: &[u8]) -> Self {
        let mut toks = Vec::new();
        let mut i = 0;
        while i < d.len() {
            let c = d[i];
            match c {
                b'{' => {
                    toks.push(Tok::Open);
                    i += 1;
                }
                b'}' => {
                    toks.push(Tok::Close);
                    i += 1;
                }
                b'"' => {
                    let end = d[i + 1..]
                        .iter()
                        .position(|&x| x == b'"')
                        .map_or(d.len(), |p| i + 1 + p);
                    toks.push(Tok::Str(
                        String::from_utf8_lossy(&d[i + 1..end]).into_owned(),
                    ));
                    i = end + 1;
                }
                b'<' => {
                    let end = d[i..]
                        .iter()
                        .position(|&x| x == b'>')
                        .map_or(d.len(), |p| i + p);
                    toks.push(Tok::Guid);
                    i = end + 1;
                }
                b'/' if d.get(i + 1) == Some(&b'/') => {
                    while i < d.len() && d[i] != b'\n' {
                        i += 1;
                    }
                }
                b'#' => {
                    while i < d.len() && d[i] != b'\n' {
                        i += 1;
                    }
                }
                b'-' | b'+' | b'.' | b'0'..=b'9' => {
                    let s = i;
                    i += 1;
                    while i < d.len()
                        && matches!(d[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'-' | b'+')
                    {
                        i += 1;
                    }
                    let t = std::str::from_utf8(&d[s..i]).unwrap_or("0");
                    toks.push(Tok::Num(t.parse().unwrap_or(0.0)));
                }
                c if c.is_ascii_alphabetic() || c == b'_' => {
                    let s = i;
                    while i < d.len()
                        && (d[i].is_ascii_alphanumeric() || matches!(d[i], b'_' | b'-' | b'.'))
                    {
                        i += 1;
                    }
                    toks.push(Tok::Word(String::from_utf8_lossy(&d[s..i]).into_owned()));
                }
                _ => i += 1,
            }
        }
        Self { toks, at: 0 }
    }

    fn objects(mut self) -> Result<Vec<Node>, Error> {
        let mut out = Vec::new();
        while self.at < self.toks.len() {
            if let Some(n) = self.object()? {
                out.push(n);
            }
        }
        Ok(out)
    }

    /// A data object or template at the cursor; templates give none.
    fn object(&mut self) -> Result<Option<Node>, Error> {
        let Some(Tok::Word(kind)) = self.toks.get(self.at).cloned() else {
            self.at += 1;
            return Ok(None);
        };
        self.at += 1;
        let mut name = String::new();
        if let Some(Tok::Word(n)) = self.toks.get(self.at) {
            name = n.clone();
            self.at += 1;
        }
        if self.toks.get(self.at) != Some(&Tok::Open) {
            return Ok(None);
        }
        self.at += 1;
        let mut node = Node {
            kind: kind.clone(),
            name,
            ..Default::default()
        };
        while self.at < self.toks.len() {
            match self.toks[self.at].clone() {
                Tok::Close => {
                    self.at += 1;
                    break;
                }
                Tok::Num(v) => {
                    node.values.push(Val::Num(v));
                    self.at += 1;
                }
                Tok::Str(s) => {
                    node.values.push(Val::Str(s));
                    self.at += 1;
                }
                Tok::Guid => self.at += 1,
                Tok::Open => {
                    // { Reference }
                    self.at += 1;
                    if let Some(Tok::Word(r)) = self.toks.get(self.at).cloned() {
                        node.refs.push(r);
                        self.at += 1;
                    }
                    if self.toks.get(self.at) == Some(&Tok::Close) {
                        self.at += 1;
                    }
                }
                Tok::Word(_) => {
                    let save = self.at;
                    match self.object()? {
                        Some(c) => node.children.push(c),
                        None => {
                            if self.at == save {
                                self.at += 1;
                            }
                        }
                    }
                }
            }
        }
        Ok((kind != "template").then_some(node))
    }
}

// ---------- binary

struct Binary<'a> {
    d: &'a [u8],
    at: usize,
}

const T_NAME: u16 = 1;
const T_STRING: u16 = 2;
const T_INTEGER: u16 = 3;
const T_GUID: u16 = 5;
const T_INT_LIST: u16 = 6;
const T_FLOAT_LIST: u16 = 7;
const T_OBRACE: u16 = 10;
const T_CBRACE: u16 = 11;
const T_TEMPLATE: u16 = 31;

impl Binary<'_> {
    fn u16(&mut self) -> Result<u16, Error> {
        let v = self.d.get(self.at..self.at + 2).ok_or("cut short")?;
        self.at += 2;
        Ok(u16::from_le_bytes([v[0], v[1]]))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        let v = self.d.get(self.at..self.at + 4).ok_or("cut short")?;
        self.at += 4;
        Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }
    fn peek(&self) -> Option<u16> {
        self.d
            .get(self.at..self.at + 2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
    }
    fn name(&mut self) -> Result<String, Error> {
        let n = self.u32()? as usize;
        let s = self.d.get(self.at..self.at + n).ok_or("cut short")?;
        self.at += n;
        Ok(String::from_utf8_lossy(s).into_owned())
    }

    fn objects(mut self) -> Result<Vec<Node>, Error> {
        let mut out = Vec::new();
        while self.at + 2 <= self.d.len() {
            match self.peek() {
                Some(T_TEMPLATE) => self.skip_template()?,
                Some(T_NAME) => out.push(self.object()?),
                _ => self.at += 2,
            }
        }
        Ok(out)
    }

    fn skip_template(&mut self) -> Result<(), Error> {
        let mut depth = 0;
        self.u16()?;
        loop {
            let t = self.u16()?;
            match t {
                T_NAME | T_STRING => {
                    self.name()?;
                    if t == T_STRING {
                        self.u16()?;
                    }
                }
                T_INTEGER => {
                    self.u32()?;
                }
                T_GUID => self.at += 16,
                T_INT_LIST => {
                    let n = self.u32()? as usize;
                    self.at += 4 * n;
                }
                T_FLOAT_LIST => {
                    let n = self.u32()? as usize;
                    self.at += 4 * n;
                }
                T_OBRACE => depth += 1,
                T_CBRACE => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn object(&mut self) -> Result<Node, Error> {
        self.u16()?;
        let kind = self.name()?;
        let mut name = String::new();
        if self.peek() == Some(T_NAME) {
            self.u16()?;
            name = self.name()?;
        }
        if self.u16()? != T_OBRACE {
            return Err(format!("{kind}: no brace").into());
        }
        if self.peek() == Some(T_GUID) {
            self.u16()?;
            self.at += 16;
        }
        let mut node = Node {
            kind,
            name,
            ..Default::default()
        };
        loop {
            match self.peek().ok_or("cut short")? {
                T_CBRACE => {
                    self.u16()?;
                    return Ok(node);
                }
                T_NAME => node.children.push(self.object()?),
                T_OBRACE => {
                    self.u16()?;
                    if self.peek() == Some(T_NAME) {
                        self.u16()?;
                        node.refs.push(self.name()?);
                    }
                    if self.peek() == Some(T_GUID) {
                        self.u16()?;
                        self.at += 16;
                    }
                    if self.peek() == Some(T_CBRACE) {
                        self.u16()?;
                    }
                }
                T_INT_LIST => {
                    self.u16()?;
                    let n = self.u32()?;
                    for _ in 0..n {
                        node.values.push(Val::Num(f64::from(self.u32()?)));
                    }
                }
                T_FLOAT_LIST => {
                    self.u16()?;
                    let n = self.u32()?;
                    for _ in 0..n {
                        node.values
                            .push(Val::Num(f64::from(f32::from_bits(self.u32()?))));
                    }
                }
                T_STRING => {
                    self.u16()?;
                    node.values.push(Val::Str(self.name()?));
                    self.u16()?;
                }
                T_INTEGER => {
                    self.u16()?;
                    node.values.push(Val::Num(f64::from(self.u32()?)));
                }
                _ => {
                    self.u16()?;
                }
            }
        }
    }
}

// ---------- meshes, frames and animations

#[derive(Debug, Clone, Default)]
pub struct XMaterial {
    pub name: String,
    pub diffuse: [f32; 4],
    pub power: f32,
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub texture: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Skin {
    pub bone: String,
    pub vertices: Vec<u32>,
    pub weights: Vec<f32>,
    pub offset: Mat4,
}

#[derive(Debug, Clone, Default)]
pub struct XMesh {
    pub name: String,
    /// The frame the mesh hangs in.
    pub frame: String,
    pub positions: Vec<Vec3>,
    /// Triangles, fanned from the file's polygons, with the face they came from.
    pub triangles: Vec<([u32; 3], usize)>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    /// Every texture coordinate set that has coordinates, in file order (`uvs` is the first).
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub colors: Vec<[f32; 4]>,
    pub face_materials: Vec<u32>,
    pub materials: Vec<XMaterial>,
    pub skins: Vec<Skin>,
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub name: String,
    pub parent: Option<usize>,
    pub matrix: Mat4,
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub frame: String,
    pub rotation: Vec<(f32, Quat)>,
    pub scale: Vec<(f32, Vec3)>,
    pub position: Vec<(f32, Vec3)>,
    pub matrix: Vec<(f32, Mat4)>,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub frames: Vec<Frame>,
    pub meshes: Vec<XMesh>,
    pub tracks: Vec<Track>,
    pub materials: HashMap<String, XMaterial>,
}

fn mat(v: &[f64]) -> Mat4 {
    let f: [f32; 16] = std::array::from_fn(|i| v.get(i).copied().unwrap_or(0.0) as f32);
    Mat4::from_cols_array(&f)
}

fn material(n: &Node) -> XMaterial {
    let v = n.nums();
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0) as f32;
    XMaterial {
        name: n.name.clone(),
        diffuse: [g(0), g(1), g(2), g(3)],
        power: g(4),
        specular: [g(5), g(6), g(7)],
        emissive: [g(8), g(9), g(10)],
        texture: n
            .child("TextureFilename")
            .or_else(|| n.child("TextureMapFilename"))
            .and_then(|t| t.strings().first().map(|s| s.to_string())),
    }
}

impl Model {
    pub fn read(d: &[u8]) -> Result<Self, Error> {
        let top = parse(d)?;
        let mut m = Model::default();
        for n in &top {
            if n.kind == "Material" {
                m.materials.insert(n.name.clone(), material(n));
            }
        }
        for n in &top {
            match n.kind.as_str() {
                "Frame" => m.frame(n, None)?,
                "Mesh" => {
                    let mesh = m.mesh(n, "")?;
                    m.meshes.push(mesh);
                }
                "AnimationSet" => m.animations(n),
                "Animation" => m.animation(n),
                _ => {}
            }
        }
        Ok(m)
    }

    fn frame(&mut self, n: &Node, parent: Option<usize>) -> Result<(), Error> {
        let matrix = n
            .child("FrameTransformMatrix")
            .map_or(Mat4::IDENTITY, |t| mat(&t.nums()));
        let me = self.frames.len();
        self.frames.push(Frame {
            name: n.name.clone(),
            parent,
            matrix,
        });
        for c in &n.children {
            match c.kind.as_str() {
                "Frame" => self.frame(c, Some(me))?,
                "Mesh" => {
                    let mesh = self.mesh(c, &n.name)?;
                    self.meshes.push(mesh);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn mesh(&self, n: &Node, frame: &str) -> Result<XMesh, Error> {
        let v = n.nums();
        let mut at = 0;
        let mut next = |k: usize| -> Result<&[f64], Error> {
            let s = v.get(at..at + k).ok_or("mesh is cut short")?;
            at += k;
            Ok(s)
        };
        let nv = next(1)?[0] as usize;
        let p = next(nv * 3)?.to_vec();
        let positions: Vec<Vec3> = p
            .chunks_exact(3)
            .map(|c| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32))
            .collect();
        let nf = next(1)?[0] as usize;
        let mut faces = Vec::new();
        for _ in 0..nf {
            let k = next(1)?[0] as usize;
            faces.push(next(k)?.iter().map(|&i| i as u32).collect::<Vec<_>>());
        }
        let mut triangles = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 2..f.len() {
                triangles.push(([f[0], f[k - 1], f[k]], fi));
            }
        }
        let mut mesh = XMesh {
            name: n.name.clone(),
            frame: frame.into(),
            positions,
            triangles,
            ..Default::default()
        };
        if let Some(nn) = n.child("MeshNormals") {
            let v = nn.nums();
            let k = v.first().copied().unwrap_or(0.0) as usize;
            let normals: Vec<Vec3> = v[1..1 + k * 3]
                .chunks_exact(3)
                .map(|c| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32))
                .collect();
            // Per vertex when the face indices match the positions', as the exporter writes.
            mesh.normals = if normals.len() == mesh.positions.len() {
                normals
            } else {
                let mut per = vec![Vec3::ZERO; mesh.positions.len()];
                let mut at = 1 + k * 3;
                let nfn = v.get(at).copied().unwrap_or(0.0) as usize;
                at += 1;
                for f in faces.iter().take(nfn) {
                    let c = v.get(at).copied().unwrap_or(0.0) as usize;
                    for (j, &vi) in f.iter().enumerate().take(c) {
                        let ni = v.get(at + 1 + j).copied().unwrap_or(0.0) as usize;
                        if let (Some(dst), Some(src)) = (per.get_mut(vi as usize), normals.get(ni))
                        {
                            *dst = *src;
                        }
                    }
                    at += 1 + c;
                }
                per
            };
        }
        // Empty sets are dropped as D3DX does; the rest become texture sets 0, 1, 2...
        for t in n.all("MeshTextureCoords") {
            let v = t.nums();
            let k = v.first().copied().unwrap_or(0.0) as usize;
            if k > 0 && v.len() > k * 2 {
                mesh.uv_sets.push(
                    v[1..1 + k * 2]
                        .chunks_exact(2)
                        .map(|c| [c[0] as f32, c[1] as f32])
                        .collect(),
                );
            }
        }
        mesh.uvs = mesh.uv_sets.first().cloned().unwrap_or_default();
        if let Some(c) = n.child("MeshVertexColors") {
            let v = c.nums();
            let k = v.first().copied().unwrap_or(0.0) as usize;
            mesh.colors = vec![[1.0; 4]; mesh.positions.len()];
            for e in v[1..].chunks_exact(5).take(k) {
                if let Some(dst) = mesh.colors.get_mut(e[0] as usize) {
                    *dst = [e[1] as f32, e[2] as f32, e[3] as f32, e[4] as f32];
                }
            }
        }
        if let Some(ml) = n.child("MeshMaterialList") {
            let v = ml.nums();
            let nfi = v.get(1).copied().unwrap_or(0.0) as usize;
            mesh.face_materials = v
                .get(2..2 + nfi)
                .unwrap_or(&[])
                .iter()
                .map(|&x| x as u32)
                .collect();
            for c in &ml.children {
                if c.kind == "Material" {
                    mesh.materials.push(material(c));
                }
            }
            for r in &ml.refs {
                if let Some(m) = self.materials.get(r) {
                    mesh.materials.push(m.clone());
                }
            }
        }
        for s in n.all("SkinWeights") {
            let bone = s
                .strings()
                .first()
                .map(|x| x.to_string())
                .unwrap_or_default();
            let v = s.nums();
            let k = v.first().copied().unwrap_or(0.0) as usize;
            let vertices = v[1..1 + k].iter().map(|&x| x as u32).collect();
            let weights = v[1 + k..1 + 2 * k].iter().map(|&x| x as f32).collect();
            let offset = mat(&v[1 + 2 * k..]);
            mesh.skins.push(Skin {
                bone,
                vertices,
                weights,
                offset,
            });
        }
        Ok(mesh)
    }

    fn animations(&mut self, set: &Node) {
        for a in set.all("Animation") {
            self.animation(a);
        }
    }

    fn animation(&mut self, a: &Node) {
        let mut t = Track {
            frame: a
                .refs
                .first()
                .cloned()
                .or_else(|| a.child("Frame").map(|f| f.name.clone()))
                .unwrap_or_default(),
            ..Default::default()
        };
        for k in a.all("AnimationKey") {
            let v = k.nums();
            let kind = v.first().copied().unwrap_or(-1.0) as i32;
            let n = v.get(1).copied().unwrap_or(0.0) as usize;
            let mut at = 2;
            for _ in 0..n {
                let Some(&time) = v.get(at) else { break };
                let count = v.get(at + 1).copied().unwrap_or(0.0) as usize;
                let vals = v.get(at + 2..at + 2 + count).unwrap_or(&[]);
                let f = |i: usize| vals.get(i).copied().unwrap_or(0.0) as f32;
                let time = time as f32;
                match kind {
                    // The four numbers in memory order are D3DXQUATERNION's x, y, z, w: so read, the
                    // first key of every frame is the frame's own pose (see examples/xquat.rs).
                    0 => t
                        .rotation
                        .push((time, Quat::from_xyzw(f(0), f(1), f(2), f(3)))),
                    1 => t.scale.push((time, Vec3::new(f(0), f(1), f(2)))),
                    2 => t.position.push((time, Vec3::new(f(0), f(1), f(2)))),
                    4 => t.matrix.push((time, mat(vals))),
                    _ => {}
                }
                at += 2 + count;
            }
        }
        self.tracks.push(t);
    }
}
