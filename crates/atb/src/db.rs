//! A thin database facade over the observed ATB formats.
//!
//! A `.BOF` provides the scene's name table; `.3DF` files are the textures it
//! refers to. Both readers are strict and bounds-checked. Object-graph extraction
//! from BOF is still future work, so `texture()` takes loose bytes supplied by
//! the caller (e.g. a `.3DF` file loaded beside the BOF).

use crate::Error;
use crate::bof::Bof;
use crate::mesh3ds::{self, Scene3ds};
use crate::tex3df::{self, Texture3df};

#[derive(Clone, Debug)]
pub struct Db {
    pub bof: Bof,
}

impl Db {
    pub fn open(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            bof: Bof::parse(bytes)?,
        })
    }

    pub fn names(&self) -> &[String] {
        self.bof.names()
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.bof.find(name)
    }

    /// Decode a `.3DF` texture from its raw bytes.
    pub fn texture(&self, bytes: &[u8]) -> Result<Texture3df, Error> {
        tex3df::read(bytes)
    }

    /// Decode a `.3DS` scene mesh from its raw bytes.
    pub fn mesh(&self, bytes: &[u8]) -> Result<Scene3ds, Error> {
        mesh3ds::read(bytes)
    }
}

/// Decode a standalone `.3DF` file.
pub fn read_texture(bytes: &[u8]) -> Result<Texture3df, Error> {
    tex3df::read(bytes)
}

/// Decode a standalone `.3DS` file.
pub fn read_mesh(bytes: &[u8]) -> Result<Scene3ds, Error> {
    mesh3ds::read(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_bytes() {
        assert!(Db::open(b"garbage").is_err());
        assert!(read_texture(b"garbage").is_err());
    }
}
