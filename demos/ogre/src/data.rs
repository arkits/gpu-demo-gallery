//! Read the extracted assets or the same files in a losslessly compressed pack.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use crate::{Error, demo, tree::read_binary};

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
        if [".nmb", ".dds", ".tga", ".txt"]
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

    /// The scenes, and the textures that they name. Interface pictures and the
    /// normalization cube are left out: the port draws no menus and normalizes
    /// in its shaders instead. The animation files name older `.tga` copies of
    /// pictures that only ship as `.dds`; those names are skipped [V].
    pub fn needed(&self) -> Result<Vec<String>, Error> {
        let mut names = BTreeSet::new();
        for file in demo::MODELS {
            let root = read_binary(self.get(file)?).map_err(|e| format!("{file}: {e}"))?;
            names.insert((*file).to_owned());
            for t in root.nodes("textures") {
                let file = t.text("filename");
                if !file.is_empty()
                    && !file.contains("ui\\")
                    && !file.contains("ui/")
                    && !t.name.starts_with("normalize")
                    && self.get(file).is_ok()
                {
                    names.insert(asset_name(file));
                }
            }
        }
        // The ports sample their textures upside down relative to the files [V];
        // keep every named picture that is actually there.
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
            if files.0.insert(asset_name(&name), data).is_some() {
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
            if !item.file_type()?.is_dir() || !matches!(name.as_str(), "models" | "textures") {
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
