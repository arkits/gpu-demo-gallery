//! Loads every ATB asset a demo folder holds, through the strict readers.
//!
//! Each ATB demo has a different mix of `.BOF` (name tables / object graphs),
//! `.3DS` (meshes) and `.3DF` (texture pyramids). This module gathers whatever
//! is present, deterministically, so a port renders its *own* real files. The
//! BOF object graph after the dict is still opaque; ports draw the validated
//! name table, real meshes and real textures on reconstructed geometry.

use crate::Error;
use crate::bof::Bof;
use crate::mesh3ds::{self, Scene3ds};
use crate::tex3df::{self, Texture3df};

/// Supplies the demo's files (loose folder, pack, or browser uploads).
pub trait Source {
    /// Lower-case base names present.
    fn names(&self) -> Vec<String>;
    /// Raw bytes of a named file.
    fn get(&self, name: &str) -> Option<&[u8]>;
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// Names from the first `.bof` dict, in file order.
    pub dict: Vec<String>,
    /// `(name, parsed mesh scene)` for every `.3ds`.
    pub meshes: Vec<(String, Scene3ds)>,
    /// `(name, parsed texture)` for every `.3df`.
    pub textures: Vec<(String, Texture3df)>,
    /// Raw bytes of the first `.bof`, for ports that derive visuals from it.
    pub bof_bytes: Vec<u8>,
}

impl Scene {
    pub fn load(src: &impl Source) -> Result<Self, Error> {
        let mut names = src.names();
        names.sort();
        names.dedup();
        let mut scene = Scene::default();
        for name in names {
            let Some(bytes) = src.get(&name) else {
                continue;
            };
            // One unreadable asset must not break the whole scene: the .3DF
            // family includes compressed yiq/ayiq variants with a different
            // byte layout, and some demos ship a few truncated files. The
            // strict readers reject those; here we simply skip them.
            let ext = if name.ends_with(".bof") {
                "bof"
            } else if name.ends_with(".3ds") {
                "3ds"
            } else if name.ends_with(".3df") {
                "3df"
            } else {
                continue;
            };
            match ext {
                "bof" => {
                    if let Ok(bof) = Bof::parse(bytes)
                        && scene.dict.is_empty()
                    {
                        scene.dict = bof.names().to_vec();
                        scene.bof_bytes = bytes.to_vec();
                    }
                }
                "3ds" => {
                    if let Ok(mesh) = mesh3ds::read(bytes) {
                        scene.meshes.push((name, mesh));
                    }
                }
                "3df" => {
                    if let Ok(tex) = tex3df::read(bytes) {
                        scene.textures.push((name, tex));
                    }
                }
                _ => unreachable!(),
            }
        }
        Ok(scene)
    }

    /// Total drawable triangles across all meshes that have faces.
    pub fn triangle_count(&self) -> usize {
        self.meshes
            .iter()
            .flat_map(|(_, s)| &s.meshes)
            .map(|m| m.faces.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Mem(BTreeMap<String, Vec<u8>>);
    impl Source for Mem {
        fn names(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
        fn get(&self, name: &str) -> Option<&[u8]> {
            self.0.get(name).map(Vec::as_slice)
        }
    }

    #[test]
    fn empty_is_ok() {
        assert!(Scene::load(&Mem(BTreeMap::new())).unwrap().dict.is_empty());
    }
}
