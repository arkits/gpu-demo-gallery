//! The demo's own files, from the unpacked folder or a pack. Names are matched
//! without regard to case, as on Windows.
use std::collections::BTreeMap;
use std::io::Read;

use crate::Error;
use crate::anim;
use crate::model::{Animation, Model};
use crate::tree::read_binary;

const MAGIC: &[u8; 8] = b"CREPACK1";
/// Cube map faces (`underwater.ifl`): +X, -X, +Y, -Y, +Z, -Z.
pub const FACES: [&str; 6] = ["xpos", "xneg", "ypos", "yneg", "zpos", "zneg"];
pub const CAUSTICS: usize = 32;

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

impl Files {
    /// Keeps files the demo can use, by their names without directories.
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        let name = name
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if [".nmb", ".nab", ".dds", ".tga"]
            .iter()
            .any(|e| name.ends_with(e))
        {
            self.0.insert(name, data);
        }
    }
    pub fn get(&self, name: &str) -> Result<&[u8], Error> {
        let f = self.0.get(&name.to_ascii_lowercase()).map(Vec::as_slice);
        f.ok_or_else(|| format!("missing {name}").into())
    }

    pub fn model(&self, name: &str) -> Result<Model, Error> {
        let root = read_binary(self.get(name)?).map_err(|e| format!("{name}: {e}"))?;
        Model::new(&root).map_err(|e| format!("{name}: {e}").into())
    }

    pub fn animation(&self, model: &Model) -> Result<Animation, Error> {
        let name = "creature.nab";
        let root = read_binary(self.get(name)?).map_err(|e| format!("{name}: {e}"))?;
        Animation::new(model, anim::read(&root)?)
    }

    /// Everything the demo needs, and nothing else.
    pub fn needed(&self) -> Result<Vec<String>, Error> {
        let mut names: Vec<String> = vec!["creature.nab".into(), "nvidialogo.dds".into()];
        for m in ["creature.nmb", "fish.nmb", "beam.nmb"] {
            names.push(m.into());
            let model = self.model(m)?;
            names.extend(model.shapes.into_iter().flat_map(|s| s.textures));
        }
        names.extend(FACES.map(|f| format!("underwater_{f}.tga")));
        names.extend((0..CAUSTICS).map(|i| format!("caust{i:02}.tga")));
        names.sort_unstable();
        names.dedup();
        Ok(names)
    }

    /// Magic, count, then a (name, size) table, then contents.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let names = self.needed()?;
        let mut out = MAGIC.to_vec();
        out.extend((names.len() as u32).to_le_bytes());
        for n in &names {
            out.extend((n.len() as u32).to_le_bytes());
            out.extend(n.as_bytes());
            out.extend((self.get(n)?.len() as u32).to_le_bytes());
        }
        for n in &names {
            out.extend(self.get(n)?);
        }
        Ok(out)
    }

    /// Opens a pack, plain or compressed with xz.
    pub fn open(data: &[u8]) -> Result<Self, Error> {
        if data.starts_with(b"\xfd7zXZ\0") {
            let mut raw = Vec::new();
            lzma_rust2::XzReader::new(data, false).read_to_end(&mut raw)?;
            return Self::unpack(&raw);
        }
        Self::unpack(data)
    }

    fn unpack(mut d: &[u8]) -> Result<Self, Error> {
        fn take<'a>(d: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
            let (a, b) = d.split_at_checked(n).ok_or("pack is cut short")?;
            *d = b;
            Ok(a)
        }
        fn int(d: &mut &[u8]) -> Result<usize, Error> {
            Ok(u32::from_le_bytes(take(d, 4)?.try_into()?) as usize)
        }
        if take(&mut d, 8)? != MAGIC {
            return Err("not a Creature pack".into());
        }
        let mut table = Vec::new();
        for _ in 0..int(&mut d)? {
            let n = int(&mut d)?;
            let name = std::str::from_utf8(take(&mut d, n)?)?.to_owned();
            table.push((name, int(&mut d)?));
        }
        let mut files = Self::default();
        for (name, size) in table {
            let data = take(&mut d, size)?.to_vec();
            files.0.insert(name, data);
        }
        if !d.is_empty() {
            return Err("pack has extra bytes".into());
        }
        Ok(files)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, Error> {
        let mut files = Self::default();
        for sub in ["models", "animations", "textures", "UserInterface"] {
            let sub = dir.join(sub);
            let list = std::fs::read_dir(&sub).map_err(|e| format!("{}: {e}", sub.display()))?;
            for f in list {
                let f = f?;
                files.insert(&f.file_name().to_string_lossy(), std::fs::read(f.path())?);
            }
        }
        Ok(files)
    }
}
