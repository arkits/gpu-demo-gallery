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

    pub fn model(&self, name: &str) -> Result<Model, Error> {
        let load = || -> Result<Model, Error> {
            let root = read_binary(self.get(&format!("models/{name}"))?)?;
            let mut model = Model::new(&root)?;
            for shape in &mut model.shapes {
                for path in shape.textures.iter_mut().flatten() {
                    if let Some(stem) = path.strip_suffix(".tga")
                        && self.get(path).is_err()
                    {
                        let dds = format!("{stem}.dds");
                        if self.get(&dds).is_ok() {
                            *path = dds;
                        }
                    }
                }
            }
            Ok(model)
        };
        load().map_err(|e| format!("{name}: {e}").into())
    }

    pub fn faces(&self, name: &str) -> Result<[String; 6], Error> {
        let paths: Vec<_> = std::str::from_utf8(self.get(name)?)?
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(asset_name)
            .collect();
        paths
            .try_into()
            .map_err(|_| format!("{name}: expected six cube faces").into())
    }

    pub fn needed(&self) -> Result<Vec<String>, Error> {
        let mut names = BTreeSet::from(["userinterface/nvidialogo.dds".to_owned()]);
        for file in demo::MODELS {
            names.insert(format!("models/{file}"));
            let model = self.model(file)?;
            names.extend(model.environment);
            for shape in model.shapes {
                names.extend(shape.textures.into_iter().flatten());
                names.extend(shape.cube);
            }
        }
        let faces = names
            .iter()
            .filter(|f| f.ends_with(".ifl"))
            .map(|f| self.faces(f))
            .collect::<Result<Vec<_>, _>>()?;
        names.extend(faces.into_iter().flatten());
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
