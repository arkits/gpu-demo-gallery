//! The demo's files by path from its folder (`textures/x.dds`), read from the folder or from
//! a pack made by `scripts/pack.sh` (optionally xz-compressed). Paths are matched without
//! regard to case, as on the Windows the demo was made for.
use std::collections::BTreeMap;
use std::io::Read;

use nv_engine::load::Assets;

pub type Error = Box<dyn std::error::Error>;

const MAGIC: &[u8; 8] = b"NVVIEWR1";

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

fn key(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches("./").to_ascii_lowercase()
}

impl Assets for Files {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.get(&key(path)).cloned()
    }
}

impl Files {
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.0.insert(key(name), data);
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.0.get(&key(name)).map(Vec::as_slice)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    /// A pack of the named files.
    pub fn pack(&self, names: &[String]) -> Result<Vec<u8>, Error> {
        let mut out = MAGIC.to_vec();
        out.extend(u32::try_from(names.len())?.to_le_bytes());
        for n in names {
            let d = self.get(n).ok_or_else(|| format!("missing {n}"))?;
            let k = key(n);
            out.extend(u32::try_from(k.len())?.to_le_bytes());
            out.extend(k.as_bytes());
            out.extend(u32::try_from(d.len())?.to_le_bytes());
        }
        for n in names {
            out.extend(self.get(n).unwrap_or_default());
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
                return Err("duplicate entry in pack".into());
            }
        }
        if !d.is_empty() {
            return Err("pack has extra bytes".into());
        }
        Ok(files)
    }

    /// Every file under a folder, but sounds (they are played from the folder).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, Error> {
        fn walk(files: &mut Files, root: &std::path::Path, dir: &std::path::Path) -> Result<(), Error> {
            for item in std::fs::read_dir(dir)? {
                let item = item?;
                let path = item.path();
                let meta = std::fs::metadata(&path)?;
                let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
                if meta.is_dir() {
                    if !rel.eq_ignore_ascii_case("sounds") {
                        walk(files, root, &path)?;
                    }
                } else if !rel.to_ascii_lowercase().ends_with(".exe") && !rel.to_ascii_lowercase().ends_with(".dll") {
                    files.insert(&rel, std::fs::read(&path)?);
                }
            }
            Ok(())
        }
        let mut files = Self::default();
        walk(&mut files, dir, dir)?;
        Ok(files)
    }
}

/// Records what is read, to pack only that.
pub struct Recorder<'a> {
    pub files: &'a Files,
    pub read: std::cell::RefCell<std::collections::BTreeSet<String>>,
}

impl Assets for Recorder<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let d = self.files.read(path)?;
        self.read.borrow_mut().insert(key(path));
        Some(d)
    }
}
