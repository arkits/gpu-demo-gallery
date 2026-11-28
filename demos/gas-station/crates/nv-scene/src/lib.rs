//! Reads the binary scene files (`.nmb`) of NVIDIA's 2003 NVDemo viewers (Gas Station,
//! Vulcan, Time Machine, Toys).
//!
//! A file is one node: a kind id (u32), then fields until -4. A field is its index in the
//! kind's registration order (i32) and a value of the field's type; -3 is the node's name
//! (a string). Strings and arrays carry a u32 count. Kinds, field order and types differ
//! between the viewers, so each has its own table (`schema`).

pub mod schema;

use std::fmt;

/// Field value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Int,
    Bool,
    Float,
    Double,
    /// n floats.
    Vector(u32),
    /// n signed ints.
    IVec(u32),
    /// A count, then count × n floats.
    Floats(u32),
    /// A count, then count × n half floats.
    Halves(u32),
    /// An int and four floats.
    SetVec,
    Str,
    Strs,
    Ints,
    Shorts,
    /// A count, then 32-byte key frames.
    Keys,
    One,
    Many,
    /// Registered but never written by the demos' files.
    Unused,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(u32),
    Bool(bool),
    Float(f32),
    Double(f64),
    Vector(Vec<f32>),
    IVec(Vec<i32>),
    /// Values and components per element.
    Floats(Vec<f32>, u32),
    Halves(Vec<u16>, u32),
    SetVec(u32, [f32; 4]),
    Str(String),
    Strs(Vec<String>),
    Ints(Vec<u32>),
    Shorts(Vec<u16>),
    Keys(Vec<[u32; 8]>),
    One(Box<Node>),
    Many(Vec<Node>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: &'static str,
    pub name: String,
    /// Fields in file order.
    pub fields: Vec<(&'static str, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub at: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {:#x}: {}", self.at, self.message)
    }
}

impl std::error::Error for Error {}

/// Which viewer wrote the file: their kinds and fields differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    GasStation,
    Vulcan,
    TimeMachine,
    Toys,
}

impl Viewer {
    pub fn classes(self) -> &'static [schema::Class] {
        match self {
            Viewer::GasStation => schema::GAS_STATION,
            Viewer::Vulcan => schema::VULCAN,
            Viewer::TimeMachine => schema::TRUCK,
            Viewer::Toys => schema::DEPTH_OF_FIELD,
        }
    }
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
    classes: &'static [schema::Class],
}

impl Reader<'_> {
    fn fail<T>(&self, message: impl Into<String>) -> Result<T, Error> {
        Err(Error { at: self.at, message: message.into() })
    }

    fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        if self.at.checked_add(n).is_none_or(|e| e > self.d.len()) {
            return self.fail(format!("{n} bytes past the end"));
        }
        let s = &self.d[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn count(&mut self, size: usize) -> Result<usize, Error> {
        let n = self.u32()? as usize;
        if n.saturating_mul(size) > self.d.len() - self.at {
            return self.fail(format!("count {n} too large"));
        }
        Ok(n)
    }

    fn f32s(&mut self, n: usize) -> Result<Vec<f32>, Error> {
        Ok(self.take(4 * n)?.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }

    fn text(&mut self) -> Result<String, Error> {
        let n = self.count(1)?;
        // Latin-1, as the viewers wrote them.
        Ok(self.take(n)?.iter().map(|&b| b as char).collect())
    }

    fn value(&mut self, ty: Ty) -> Result<Value, Error> {
        Ok(match ty {
            Ty::Int => Value::Int(self.u32()?),
            Ty::Bool => {
                let b = self.take(1)?[0];
                if b > 1 {
                    return self.fail(format!("bool {b}"));
                }
                Value::Bool(b == 1)
            }
            Ty::Float => Value::Float(self.f32s(1)?[0]),
            Ty::Double => Value::Double(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            Ty::Vector(n) => Value::Vector(self.f32s(n as usize)?),
            Ty::IVec(n) => Value::IVec(
                self.take(4 * n as usize)?
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                    .collect(),
            ),
            Ty::Floats(k) => {
                let n = self.count(4 * k as usize)?;
                Value::Floats(self.f32s(n * k as usize)?, k)
            }
            Ty::Halves(k) => {
                let n = self.count(2 * k as usize)?;
                let b = self.take(2 * n * k as usize)?;
                Value::Halves(b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(), k)
            }
            Ty::SetVec => {
                let i = self.u32()?;
                let v = self.f32s(4)?;
                Value::SetVec(i, [v[0], v[1], v[2], v[3]])
            }
            Ty::Str => Value::Str(self.text()?),
            Ty::Strs => {
                let n = self.count(4)?;
                Value::Strs((0..n).map(|_| self.text()).collect::<Result<_, _>>()?)
            }
            Ty::Ints => {
                let n = self.count(4)?;
                Value::Ints(
                    self.take(4 * n)?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect(),
                )
            }
            Ty::Shorts => {
                let n = self.count(2)?;
                Value::Shorts(self.take(2 * n)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect())
            }
            Ty::Keys => {
                let n = self.count(32)?;
                let b = self.take(32 * n)?;
                Value::Keys(
                    b.chunks_exact(32)
                        .map(|k| std::array::from_fn(|i| u32::from_le_bytes(k[4 * i..4 * i + 4].try_into().unwrap())))
                        .collect(),
                )
            }
            Ty::One => Value::One(Box::new(self.node(0)?)),
            Ty::Many => {
                let n = self.count(8)?;
                Value::Many((0..n).map(|_| self.node(0)).collect::<Result<_, _>>()?)
            }
            Ty::Unused => return self.fail("a field no file of these demos has"),
        })
    }

    fn node(&mut self, depth: u32) -> Result<Node, Error> {
        if depth > 64 {
            return self.fail("nested too deeply");
        }
        let kind = self.u32()?;
        let Some(&(_, kname, fields)) = self.classes.iter().find(|c| c.0 == kind) else {
            return self.fail(format!("unknown kind {kind:#x}"));
        };
        let mut node = Node { kind: kname, name: String::new(), fields: vec![] };
        loop {
            let tag = self.u32()? as i32;
            match tag {
                -4 => return Ok(node),
                -3 => node.name = self.text()?,
                t if t >= 0 && (t as usize) < fields.len() => {
                    let (fname, ty) = fields[t as usize];
                    let v = self.value(ty)?;
                    node.fields.push((fname, v));
                }
                t => return self.fail(format!("field {t} of {kname}")),
            }
        }
    }
}

/// Reads a whole file; anything after the root node is an error.
pub fn read(data: &[u8], viewer: Viewer) -> Result<Node, Error> {
    let mut r = Reader { d: data, at: 0, classes: viewer.classes() };
    let n = r.node(0)?;
    if r.at != data.len() {
        return r.fail(format!("{} bytes after the scene", data.len() - r.at));
    }
    Ok(n)
}

/// Converts IEEE half floats.
pub fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1f) as i32;
    let m = (h & 0x3ff) as f32;
    s * match e {
        0 => m * (2f32).powi(-24),
        31 => {
            if m == 0.0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => (1.0 + m / 1024.0) * (2f32).powi(e - 15),
    }
}

impl Node {
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.iter().find(|f| f.0 == field).map(|f| &f.1)
    }

    pub fn has(&self, field: &str) -> bool {
        self.get(field).is_some()
    }

    pub fn int(&self, field: &str) -> Option<u32> {
        match self.get(field)? {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn bool(&self, field: &str) -> Option<bool> {
        match self.get(field)? {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn float(&self, field: &str) -> Option<f32> {
        match self.get(field)? {
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn double(&self, field: &str) -> Option<f64> {
        match self.get(field)? {
            Value::Double(v) => Some(*v),
            _ => None,
        }
    }

    pub fn vector(&self, field: &str) -> Option<&[f32]> {
        match self.get(field)? {
            Value::Vector(v) => Some(v),
            _ => None,
        }
    }

    pub fn ivec(&self, field: &str) -> Option<&[i32]> {
        match self.get(field)? {
            Value::IVec(v) => Some(v),
            _ => None,
        }
    }

    pub fn str(&self, field: &str) -> Option<&str> {
        match self.get(field)? {
            Value::Str(v) => Some(v),
            _ => None,
        }
    }

    pub fn strs(&self, field: &str) -> &[String] {
        match self.get(field) {
            Some(Value::Strs(v)) => v,
            _ => &[],
        }
    }

    pub fn one(&self, field: &str) -> Option<&Node> {
        match self.get(field)? {
            Value::One(v) => Some(v),
            _ => None,
        }
    }

    pub fn many(&self, field: &str) -> &[Node] {
        match self.get(field) {
            Some(Value::Many(v)) => v,
            _ => &[],
        }
    }
}
