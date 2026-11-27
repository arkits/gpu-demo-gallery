//! Read the extracted assets or the same files in a losslessly compressed pack.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use crate::{Error, demo, tree};

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
        if [".nmb", ".nma", ".dds", ".tga", ".exr", ".txt", ".wav"]
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

    pub(crate) fn text(&self, name: &str) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(self.get(name)?).into_owned())
    }

    /// The scenes, and the textures, hair and terrain that they name.
    pub fn needed(&self) -> Result<Vec<String>, Error> {
        let mut names = BTreeSet::new();
        let scenes = [
            demo::TIMBURY,
            demo::ANIMATION,
            demo::HAIR,
            demo::GRASS,
            demo::BLUR,
            demo::BACKGROUND,
            demo::MENU,
            demo::SWARMBUG,
            demo::SWARMBUG_ANIM,
            demo::GRASS_TERRAIN,
        ];
        for file in scenes {
            names.insert((*file).to_owned());
            let data = self.get(file)?;
            let root = if file.ends_with(".nmb") {
                tree::read_binary(data)
            } else {
                tree::read_text(&String::from_utf8_lossy(data))
            }
            .map_err(|e| format!("{file}: {e}"))?;
            Self::referenced(&root, &mut names);
        }
        names.insert(demo::HAIR_CURVES.to_owned());
        names.insert(demo::SOUNDTRACK.to_owned());
        for n in &names {
            self.get(n)?;
        }
        Ok(names.into_iter().collect())
    }

    fn referenced(root: &tree::Node, names: &mut BTreeSet<String>) {
        for t in root.nodes("textures") {
            if !t.text("filename").is_empty() {
                names.insert(asset_name(t.text("filename")));
            }
        }
        for g in root.nodes("geometries") {
            for f in ["hairFile", "grassLifeMapTex", "grassTerrainGeom"] {
                if !g.text(f).is_empty() {
                    names.insert(asset_name(g.text(f)));
                }
            }
        }
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
            // Follows symlinks, so a linked asset folder works too.
            if !std::fs::metadata(item.path())?.is_dir()
                || !matches!(name.as_str(), "models" | "textures" | "ui" | "audio")
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
