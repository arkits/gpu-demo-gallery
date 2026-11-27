//! Read the extracted assets or the same files in a losslessly compressed pack.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use crate::{Error, demo, tree, tree::read_binary};

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
        if [
            ".nmb", ".nma", ".dds", ".tga", ".t3d", ".txt", ".fp30", ".vp30", ".fp20", ".vp20",
        ]
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

    pub fn text(&self, name: &str) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(self.get(name)?).into_owned())
    }

    pub fn binary(&self, name: &str) -> Result<tree::Node, Error> {
        read_binary(self.get(name)?).map_err(|e| format!("{name}: {e}").into())
    }

    pub fn scene(&self, name: &str) -> Result<tree::Node, Error> {
        // `particles.nma` ends with a corrupt two-token tail after its closed
        // Scene (see ENGINE.md); everything before it is strict.
        if asset_name(name) == "models/particles.nma" {
            let text = self.text(name)?;
            let (node, rest) = tree::read_text_first(&text)?;
            if rest != ["0", "o3"] {
                return Err(format!("{name}: unexpected tail {rest:?}").into());
            }
            return Ok(node);
        }
        let text = self.text(name)?;
        tree::read_text(&text).map_err(|e| format!("{name}: {e}").into())
    }

    /// The scenes, and the textures, shaders, hair and UI that they name.
    pub fn needed(&self) -> Result<Vec<String>, Error> {
        let mut names = BTreeSet::new();
        for file in demo::MODELS {
            let root = if file.ends_with(".nmb") {
                self.binary(file)?
            } else if *file == demo::PARTICLES {
                // `particles.nma` ends with a corrupt two-token tail after its
                // closed Scene; everything before it is strict (see ENGINE.md).
                let text = self.text(file)?;
                let (node, rest) = tree::read_text_first(&text)?;
                if rest != ["0", "o3"] {
                    return Err(format!("{file}: unexpected tail {rest:?}").into());
                }
                node
            } else {
                self.scene(file)?
            };
            names.insert((*file).to_owned());
            for t in root.nodes("textures") {
                if !t.text("filename").is_empty() {
                    names.insert(asset_name(t.text("filename")));
                }
            }
            for s in root.nodes("shaders") {
                if !s.text("filename").is_empty() {
                    names.insert(asset_name(s.text("filename")));
                }
            }
        }
        for extra in demo::HAIR_FILES.iter().chain(demo::TEXT_FILES.iter()) {
            names.insert((*extra).to_owned());
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
        // Split web packs are concatenated in order.
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
                || !matches!(
                    name.as_str(),
                    "models" | "textures" | "shaders" | "bin" | "ui" | "audio"
                )
            {
                continue;
            }
            if name == "audio" {
                continue;
            }
            for file in walk(item.path()) {
                let rel = file
                    .strip_prefix(dir)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(&rel, std::fs::read(&file)?);
            }
        }
        return Ok(files);

        #[cfg(not(target_arch = "wasm32"))]
        fn walk(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
            let mut out = Vec::new();
            let Ok(items) = std::fs::read_dir(&dir) else {
                return out;
            };
            for item in items.flatten() {
                let path = item.path();
                if path.is_dir() {
                    out.extend(walk(path));
                } else if path.is_file() {
                    out.push(path);
                }
            }
            out
        }
    }
}
