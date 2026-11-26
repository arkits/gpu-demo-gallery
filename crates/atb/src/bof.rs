//! 3dfx ATB `.BOF` scene files: the saved object graph of an ATB scene.
//!
//! Layout, from the loader in every ATB demo (`atsLoad`, `boftoats.c`; in
//! `wiz.exe` at `0x414620`, the per-object step at `0x411440`) and checked
//! against every staged `.BOF`, which this reader consumes to the last byte:
//!
//! * `BOFM`, then the big-endian version word 2.
//! * Objects follow until an id of 0. Each is its id (1, 2, 3, ... in order),
//!   its class name as a counted string, then the class's own fields. The
//!   stream is not padded: every number is a big-endian 32-bit word (a few are
//!   16-bit), wherever it falls.
//! * A counted string is a word holding the length including the NUL, then
//!   the bytes; length 0 is a missing string.
//! * References between objects are ids; 0 is none.
//!
//! The classes and their fields are in `docs/ENGINE.md`. A file is refused if
//! it uses a class or version this reader does not know, an id is out of
//! order, a reference points at nothing, or anything is left over.

use crate::Error;

/// Common fields of every scene-graph node (`node.c`, load at `0x413e50`).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub flags: [u32; 2],
    pub name: Option<String>,
    /// Bounding sphere: centre and radius.
    pub sphere: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub node: Node,
    pub children: Vec<u32>,
}

/// A group under a transform (`frame.c`).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub group: Group,
    pub mode: u32,
    /// Row-vector matrix: a point is `[x y z 1] * m`, translation in 12..15.
    pub matrix: [f32; 16],
}

/// A group that shows one child at a time (`seq.c`).
#[derive(Clone, Debug, PartialEq)]
pub struct Seq {
    pub group: Group,
    pub fields: [u32; 5],
}

/// A level-of-detail switch (`lod.c`).
#[derive(Clone, Debug, PartialEq)]
pub struct Lod {
    pub group: Group,
    pub center: [f32; 3],
    pub ranges: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Anim {
    pub group: Group,
    pub fields: (u16, u16, u32),
    pub names: Vec<Option<String>>,
}

/// Geometry with materials: `(material, geometry)` ids.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub node: Node,
    pub parts: Vec<(u32, u32)>,
}

/// A shape turned to face the viewer (`bboard.c`).
#[derive(Clone, Debug, PartialEq)]
pub struct Bboard {
    pub shape: Shape,
    pub mode: (u32, u32),
    pub axis: [f32; 3],
    pub points: Vec<[f32; 3]>,
}

/// A picture in a Glide texture format (`image.c`, `AtrImg`).
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// `GrTextureFormat_t`.
    pub format: u32,
    pub width: u32,
    pub height: u32,
    /// Mip levels stored, largest first.
    pub levels: u32,
    pub file: Option<String>,
    /// Texels as stored: bytes, or big-endian 16- or 32-bit words.
    pub data: Vec<u8>,
    /// The 256-entry table that palette and YIQ formats carry.
    pub table: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub handle: u32,
    /// Image ids; more than one is an animated texture.
    pub images: Vec<u32>,
}

/// An `atrMaterial`: Glide state for one pass, as 79 words in file order.
/// Named accessors are in [`crate::material`].
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub words: [u32; 79],
}

/// One strip of a triangle set: up to 62 vertices of 16 floats and triangles
/// of three byte indices.
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    pub flags: u32,
    pub vertices: Vec<[f32; 16]>,
    pub triangles: Vec<[u8; 3]>,
    /// A picture per triangle (light maps), or none.
    pub images: Vec<Image>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TriSet {
    pub header: [u32; 6],
    pub strips: Vec<Strip>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    /// The base class, saved as an opaque block of words.
    Raw(Vec<u8>),
    Dict(Vec<(Option<String>, u32)>),
    Group(Group),
    Frame(Frame),
    Seq(Seq),
    Lod(Lod),
    Anim(Anim),
    Shape(Shape),
    Bboard(Bboard),
    Image(Image),
    Texture(Texture),
    Material(Box<Material>),
    TriSet(TriSet),
    /// A collision mesh: header words and triangles of 16 floats.
    CMesh([u32; 4], Vec<[f32; 16]>),
}

impl Object {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Raw(_) => "Object",
            Self::Dict(_) => "Dict",
            Self::Group(_) => "Group",
            Self::Frame(_) => "Frame",
            Self::Seq(_) => "Seq",
            Self::Lod(_) => "LOD",
            Self::Anim(_) => "Anim",
            Self::Shape(_) => "Shape",
            Self::Bboard(_) => "Bboard",
            Self::Image(_) => "Image",
            Self::Texture(_) => "Texture",
            Self::Material(_) => "Material",
            Self::TriSet(_) => "TriSet",
            Self::CMesh(..) => "CMesh",
        }
    }

    /// The group part of grouping objects.
    pub fn group(&self) -> Option<&Group> {
        match self {
            Self::Group(g) => Some(g),
            Self::Frame(f) => Some(&f.group),
            Self::Seq(s) => Some(&s.group),
            Self::Lod(l) => Some(&l.group),
            Self::Anim(a) => Some(&a.group),
            _ => None,
        }
    }

    pub fn node(&self) -> Option<&Node> {
        match self {
            Self::Shape(s) => Some(&s.node),
            Self::Bboard(b) => Some(&b.shape.node),
            _ => self.group().map(|g| &g.node),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bof {
    /// Object `id` is at index `id - 1`.
    pub objects: Vec<Object>,
    /// The first object's `Dict`: names to ids.
    pub dict: Vec<(String, u32)>,
}

impl Bof {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { d: bytes, at: 0 };
        if r.take(4)? != b"BOFM" {
            return Err("not a BOF file".into());
        }
        if r.word()? != 2 {
            return Err("unknown BOF version".into());
        }
        let mut objects = Vec::new();
        loop {
            let id = r.word()?;
            if id == 0 {
                break;
            }
            if id as usize != objects.len() + 1 {
                return Err(format!("BOF object {id} out of order").into());
            }
            let class = r.string()?.ok_or("BOF object without a class")?;
            objects.push(r.object(&class)?);
        }
        if r.at != bytes.len() {
            return Err("BOF has bytes after its last object".into());
        }
        let dict = match objects.first() {
            Some(Object::Dict(d)) => d
                .iter()
                .filter_map(|(n, id)| n.clone().map(|n| (n, *id)))
                .collect(),
            _ => Vec::new(),
        };
        let bof = Self { objects, dict };
        bof.check_references()?;
        Ok(bof)
    }

    pub fn get(&self, id: u32) -> Option<&Object> {
        (id as usize)
            .checked_sub(1)
            .and_then(|i| self.objects.get(i))
    }

    /// The id a dictionary name stands for (any case, as on Windows).
    pub fn find(&self, name: &str) -> Option<u32> {
        self.dict
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|&(_, id)| id)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.dict.iter().map(|(n, _)| n.as_str())
    }

    pub fn image(&self, id: u32) -> Option<&Image> {
        match self.get(id)? {
            Object::Image(i) => Some(i),
            _ => None,
        }
    }

    pub fn material(&self, id: u32) -> Option<&Material> {
        match self.get(id)? {
            Object::Material(m) => Some(m),
            _ => None,
        }
    }

    /// A texture's first picture.
    pub fn texture_image(&self, texture: u32) -> Option<&Image> {
        match self.get(texture)? {
            Object::Texture(t) => self.image(*t.images.first()?),
            _ => None,
        }
    }

    fn check_references(&self) -> Result<(), Error> {
        let n = self.objects.len() as u32;
        let ok = |id: u32| id <= n;
        for (i, o) in self.objects.iter().enumerate() {
            let bad = match o {
                Object::Dict(d) => d.iter().any(|&(_, id)| !ok(id)),
                Object::Shape(s) => s.parts.iter().any(|&(m, g)| !ok(m) || !ok(g)),
                Object::Bboard(b) => b.shape.parts.iter().any(|&(m, g)| !ok(m) || !ok(g)),
                Object::Texture(t) => t.images.iter().any(|&id| !ok(id)),
                Object::Material(m) => [14, 15, 25].iter().any(|&w| !ok(m.words[w])),
                o => o
                    .group()
                    .is_some_and(|g| g.children.iter().any(|&c| !ok(c))),
            };
            if bad {
                return Err(format!("BOF object {} refers to a missing object", i + 1).into());
            }
        }
        Ok(())
    }
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).ok_or("BOF size overflow")?;
        let s = self.d.get(self.at..end).ok_or("BOF is cut short")?;
        self.at = end;
        Ok(s)
    }
    fn word(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn half(&mut self) -> Result<u16, Error> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    fn float(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.word()?))
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], Error> {
        let mut a = [0.0; N];
        for v in &mut a {
            *v = self.float()?;
        }
        Ok(a)
    }
    /// A count that must fit in what is left, at `size` bytes an item.
    fn count(&mut self, size: usize) -> Result<usize, Error> {
        let n = self.word()? as usize;
        if n.saturating_mul(size) > self.d.len() - self.at {
            return Err("BOF count runs past the end".into());
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<Option<String>, Error> {
        let n = self.count(1)?;
        if n == 0 {
            return Ok(None);
        }
        let b = self.take(n)?;
        let (last, text) = b.split_last().ok_or("BOF empty string")?;
        if *last != 0 || text.contains(&0) || !text.is_ascii() {
            return Err("BOF string is not NUL-terminated ASCII".into());
        }
        Ok(Some(String::from_utf8(text.to_vec())?))
    }
    /// A NUL-terminated tag of known text, with no count before it.
    fn tag(&mut self, t: &str) -> Result<(), Error> {
        let b = self.take(t.len() + 1)?;
        if &b[..t.len()] != t.as_bytes() || b[t.len()] != 0 {
            return Err(format!("BOF expected {t}").into());
        }
        Ok(())
    }
    fn version(&mut self, want: u32, class: &str) -> Result<(), Error> {
        if self.word()? != want {
            return Err(format!("unknown {class} version").into());
        }
        Ok(())
    }
    fn ids(&mut self, n: usize) -> Result<Vec<u32>, Error> {
        (0..n).map(|_| self.word()).collect()
    }

    fn node(&mut self) -> Result<Node, Error> {
        Ok(Node {
            flags: [self.word()?, self.word()?],
            name: self.string()?,
            sphere: self.floats()?,
        })
    }
    fn group(&mut self) -> Result<Group, Error> {
        let node = self.node()?;
        self.version(1, "Group")?;
        let n = self.half()? as usize;
        Ok(Group {
            node,
            children: self.ids(n)?,
        })
    }
    fn shape(&mut self) -> Result<Shape, Error> {
        let node = self.node()?;
        self.version(1, "Shape")?;
        let n = self.count(8)?;
        let parts = (0..n)
            .map(|_| Ok((self.word()?, self.word()?)))
            .collect::<Result<_, Error>>()?;
        Ok(Shape { node, parts })
    }

    fn image(&mut self) -> Result<Image, Error> {
        self.version(1, "Image")?;
        self.tag("AtrImg")?;
        let (format, width, height, levels) =
            (self.word()?, self.word()?, self.word()?, self.word()?);
        let file = self.string()?;
        let size = match format {
            0..=7 => 1,
            8..=15 => 2,
            _ => 4,
        };
        let n = self.count(size)?;
        let data = self.take(n * size)?.to_vec();
        let table = match format {
            1 | 9 | 5 | 14 => Some(self.take(0x400)?.to_vec()),
            _ => None,
        };
        Ok(Image {
            format,
            width,
            height,
            levels,
            file,
            data,
            table,
        })
    }

    fn triset(&mut self) -> Result<TriSet, Error> {
        self.version(1, "TriSet")?;
        self.tag("atrTriSet")?;
        let mut header = [0; 6];
        for w in &mut header {
            *w = self.word()?;
        }
        let mut strips = Vec::new();
        while self.word()? != 0 {
            let head = self.word()?;
            let n = (head & 0x3ff_ffff) as usize;
            if n > 255 || n * 64 > self.d.len() - self.at {
                return Err("TriSet strip too large".into());
            }
            let vertices = (0..n)
                .map(|_| self.floats())
                .collect::<Result<Vec<_>, _>>()?;
            let count = self.count(4)?;
            let mut triangles = Vec::with_capacity(count);
            for _ in 0..count {
                let w = self.word()?.to_be_bytes();
                let t = [w[1], w[2], w[3]];
                if w[0] != 0 || t.iter().any(|&i| i as usize >= n) {
                    return Err("TriSet triangle out of range".into());
                }
                triangles.push(t);
            }
            let k = self.count(1)?;
            if k != 0 && k != count {
                return Err("TriSet light maps do not match triangles".into());
            }
            let images = (0..k).map(|_| self.image()).collect::<Result<_, _>>()?;
            strips.push(Strip {
                flags: head >> 26,
                vertices,
                triangles,
                images,
            });
        }
        Ok(TriSet { header, strips })
    }

    fn object(&mut self, class: &str) -> Result<Object, Error> {
        Ok(match class {
            "Object" => {
                let n = self.count(1)?;
                Object::Raw(self.take(n / 4 * 4)?.to_vec())
            }
            "Dict" => {
                self.version(1, class)?;
                let n = self.count(8)?;
                let entries = (0..n)
                    .map(|_| Ok((self.string()?, self.word()?)))
                    .collect::<Result<_, Error>>()?;
                Object::Dict(entries)
            }
            "Group" => Object::Group(self.group()?),
            "Frame" => {
                let group = self.group()?;
                self.version(1, class)?;
                Object::Frame(Frame {
                    group,
                    mode: self.word()?,
                    matrix: self.floats()?,
                })
            }
            "Seq" => {
                let group = self.group()?;
                self.version(1, class)?;
                let mut fields = [0; 5];
                for f in &mut fields {
                    *f = self.word()?;
                }
                Object::Seq(Seq { group, fields })
            }
            "LOD" => {
                let group = self.group()?;
                self.version(0, class)?;
                let center = self.floats()?;
                let n = self.count(4)?;
                let ranges = (0..n).map(|_| self.float()).collect::<Result<_, _>>()?;
                Object::Lod(Lod {
                    group,
                    center,
                    ranges,
                })
            }
            "Anim" => {
                let group = self.group()?;
                self.version(1, class)?;
                let fields = (self.half()?, self.half()?, self.word()?);
                let n = self.count(4)?;
                let names = (0..n).map(|_| self.string()).collect::<Result<_, _>>()?;
                Object::Anim(Anim {
                    group,
                    fields,
                    names,
                })
            }
            "Shape" => Object::Shape(self.shape()?),
            "Bboard" => {
                let shape = self.shape()?;
                self.version(1, class)?;
                let mode = (self.word()?, self.word()?);
                let axis = self.floats()?;
                let n = self.count(12)?;
                let points = (0..n).map(|_| self.floats()).collect::<Result<_, _>>()?;
                Object::Bboard(Bboard {
                    shape,
                    mode,
                    axis,
                    points,
                })
            }
            "Image" => Object::Image(self.image()?),
            "Texture" => {
                self.version(2, class)?;
                let handle = self.word()?;
                let n = self.count(4)?;
                Object::Texture(Texture {
                    handle,
                    images: self.ids(n)?,
                })
            }
            "Material" => {
                self.version(1, class)?;
                self.tag("atrMaterial")?;
                let mut words = [0; 79];
                for (i, w) in words.iter_mut().enumerate() {
                    // Word 46 is the alpha-test reference, stored as one byte.
                    *w = if i == 46 {
                        u32::from(self.take(1)?[0])
                    } else {
                        self.word()?
                    };
                }
                Object::Material(Box::new(Material { words }))
            }
            "TriSet" => Object::TriSet(self.triset()?),
            "CMesh" => {
                self.version(1, class)?;
                self.tag("atmCmesh")?;
                let head = [self.word()?, self.word()?, self.word()?, self.word()?];
                let _bounds: [f32; 6] = self.floats()?;
                let _ = self.word()?;
                let n = self.count(64)?;
                let tris = (0..n).map(|_| self.floats()).collect::<Result<_, _>>()?;
                Object::CMesh(head, tris)
            }
            _ => return Err(format!("unknown BOF class {class}").into()),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds BOF bytes the way `atsSave` writes them.
    #[derive(Default)]
    pub struct Writer(pub Vec<u8>, u32);
    impl Writer {
        pub fn new() -> Self {
            let mut w = Self(b"BOFM".to_vec(), 0);
            w.word(2);
            w
        }
        pub fn word(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_be_bytes());
            self
        }
        pub fn float(&mut self, v: f32) -> &mut Self {
            self.word(v.to_bits())
        }
        pub fn string(&mut self, s: &str) -> &mut Self {
            self.word(s.len() as u32 + 1);
            self.0.extend(s.as_bytes());
            self.0.push(0);
            self
        }
        pub fn object(&mut self, class: &str) -> &mut Self {
            self.1 += 1;
            let id = self.1;
            self.word(id).string(class)
        }
        pub fn node(&mut self, name: &str) -> &mut Self {
            self.word(0).word(u32::MAX).string(name);
            for _ in 0..4 {
                self.float(0.0);
            }
            self
        }
        pub fn end(&mut self) -> Vec<u8> {
            self.word(0);
            std::mem::take(&mut self.0)
        }
    }

    fn sample() -> Vec<u8> {
        let mut w = Writer::new();
        w.object("Dict").word(1).word(1).string("thing").word(2);
        w.object("Shape")
            .node("thing")
            .word(1)
            .word(1)
            .word(3)
            .word(4);
        w.object("Material").word(1);
        w.0.extend(b"atrMaterial\0");
        for i in 0..79 {
            if i == 46 {
                w.0.push(0);
            } else {
                w.word(0);
            }
        }
        w.object("TriSet").word(1);
        w.0.extend(b"atrTriSet\0");
        for _ in 0..6 {
            w.word(0);
        }
        w.word(1).word(3);
        for i in 0..3 {
            for j in 0..16 {
                w.float((i * 16 + j) as f32);
            }
        }
        w.word(1).word(0x0000_0102).word(0).word(0);
        w.end()
    }

    #[test]
    fn reads_objects() {
        let b = Bof::parse(&sample()).unwrap();
        assert_eq!(b.find("THING"), Some(2));
        let Some(Object::Shape(s)) = b.get(2) else {
            panic!()
        };
        assert_eq!(s.parts, [(3, 4)]);
        let Some(Object::TriSet(t)) = b.get(4) else {
            panic!()
        };
        assert_eq!(t.strips[0].triangles, [[0, 1, 2]]);
        assert_eq!(t.strips[0].vertices[2][15], 47.0);
    }

    #[test]
    fn refuses_bad_files() {
        assert!(Bof::parse(b"nope").is_err());
        let good = sample();
        for cut in [5, 30, good.len() - 1] {
            assert!(Bof::parse(&good[..cut]).is_err(), "cut at {cut}");
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(Bof::parse(&extra).is_err());
        // A triangle naming a vertex the strip does not have.
        let mut bad = good.clone();
        let at = bad.len() - 16;
        bad[at + 3] = 9;
        assert!(Bof::parse(&bad).is_err());
    }
}
