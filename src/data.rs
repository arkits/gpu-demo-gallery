//! Read the extracted assets or the same files in a losslessly compressed pack.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use crate::{Error, demo, model::Model, tree::read_binary};

const MAGIC: &[u8; 8] = b"NVVIEWR1";

/// Paths in scene files are relative to the original viewer executable.
pub(crate) fn asset_name(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("../")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

impl Files {
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        let name = asset_name(name);
        if [".nmb", ".dds", ".tga", ".ifl", ".png"]
            .iter()
            .any(|e| name.ends_with(e))
        {
            self.0.insert(name, data);
        }
    }

    pub fn get(&self, name: &str) -> Result<&[u8], Error> {
        self.0
            .get(&asset_name(name))
            .map(Vec::as_slice)
            .ok_or_else(|| format!("missing {name}").into())
    }

    pub(crate) fn resolve(&self, name: &str) -> Result<String, Error> {
        if self.get(name).is_ok() {
            return Ok(name.to_owned());
        }
        if let Some((stem, ext)) = name.rsplit_once('.') {
            for alternate in ["dds", "tga"] {
                if alternate != ext {
                    let candidate = format!("{stem}.{alternate}");
                    if self.get(&candidate).is_ok() {
                        return Ok(candidate);
                    }
                }
            }
        }
        Err(format!("missing {name}").into())
    }

    pub fn model(&self, name: &str) -> Result<Model, Error> {
        let load = || -> Result<Model, Error> {
            let root = read_binary(self.get(&format!("models/{name}"))?)?;
            Model::new(&root, self)
        };
        load().map_err(|e| format!("{name}: {e}").into())
    }

    /// The IFL cube files are in +X, -X, +Y, -Y, +Z, -Z order.
    pub fn faces(&self, name: &str) -> Result<[String; 6], Error> {
        let paths: Vec<_> = std::str::from_utf8(self.get(name)?)?
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(asset_name)
            .map(|p| self.resolve(&p))
            .collect::<Result<_, _>>()?;
        paths
            .try_into()
            .map_err(|_| format!("{name}: expected six cube faces").into())
    }

    pub fn needed(&self) -> Result<Vec<String>, Error> {
        let mut names: BTreeSet<String> = self
            .0
            .keys()
            .filter(|name| name.starts_with("textures/"))
            .cloned()
            .collect();
        names.insert("userinterface/nvidialogo.dds".to_owned());
        for file in demo::MODELS.iter().chain(demo::CLIPS) {
            let name = format!("models/{file}");
            read_binary(self.get(&name)?).map_err(|e| format!("{name}: {e}"))?;
            names.insert(name);
        }
        for n in &names {
            self.get(n)?;
        }
        Ok(names.into_iter().collect())
    }

    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let names = self.needed()?;
        let mut out = MAGIC.to_vec();
        out.extend(u32::try_from(names.len())?.to_le_bytes());
        for n in &names {
            out.extend(u32::try_from(n.len())?.to_le_bytes());
            out.extend(n.as_bytes());
            out.extend(u32::try_from(self.get(n)?.len())?.to_le_bytes());
        }
        for n in &names {
            out.extend(self.get(n)?);
        }
        Ok(out)
    }

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
            return Err("unrecognized pack".into());
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
            if files.0.insert(name, data).is_some() {
                return Err("duplicate entry in asset pack".into());
            }
        }
        if !d.is_empty() {
            return Err("pack has extra bytes".into());
        }
        Ok(files)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, Error> {
        let mut files = Self::default();
        for item in std::fs::read_dir(dir)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().to_ascii_lowercase();
            if !item.file_type()?.is_dir()
                || !(name == "models" || name == "userinterface" || name.starts_with("textures"))
            {
                continue;
            }
            for file in std::fs::read_dir(item.path())? {
                let file = file?;
                if file.file_type()?.is_file() {
                    files.insert(
                        &format!("{name}/{}", file.file_name().to_string_lossy()),
                        std::fs::read(file.path())?,
                    );
                }
            }
        }
        Ok(files)
    }
}
