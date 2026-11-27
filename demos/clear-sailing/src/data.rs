//! The demo's own files: an unpacked folder or a pack. Names are matched without
//! regard to case, as on Windows.
use std::collections::BTreeMap;
use std::io::Read;

use crate::Error;
use crate::demo::{FILES, SOUNDS};
use crate::image::{self, Image};

const MAGIC: &[u8; 8] = b"DEMOPAK2";

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

impl Files {
    /// Keeps files the demo can use, by their names without directories.
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        let name = name.rsplit(['\\', '/']).next().unwrap_or("");
        let name = name.to_ascii_lowercase();
        if FILES.iter().chain(SOUNDS).any(|f| *f == name) {
            self.0.insert(name, data);
        }
    }

    pub fn get(&self, name: &str) -> Result<&[u8], Error> {
        let f = self.0.get(&name.to_ascii_lowercase()).map(Vec::as_slice);
        f.ok_or_else(|| format!("missing {name}").into())
    }

    pub fn image(&self, name: &str) -> Result<Image, Error> {
        image::read(name, self.get(name)?)
    }

    /// Fails with the name of what is missing. The demo goes on without sounds.
    pub fn check(&self) -> Result<(), Error> {
        FILES.iter().try_for_each(|f| self.get(f).map(|_| ()))
    }

    /// Magic, count, then a (name, size) table, then contents.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        self.pack_some(FILES)
    }

    pub fn pack_some(&self, names: &[&str]) -> Result<Vec<u8>, Error> {
        let mut out = MAGIC.to_vec();
        out.extend((names.len() as u32).to_le_bytes());
        for n in names {
            out.extend((n.len() as u32).to_le_bytes());
            out.extend(n.as_bytes());
            out.extend((self.get(n)?.len() as u32).to_le_bytes());
        }
        for n in names {
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

    /// Adds every file of another pack. Packs are split for the web.
    pub fn extend(&mut self, data: &[u8]) -> Result<(), Error> {
        for (name, data) in Self::open(data)?.0 {
            self.0.insert(name, data);
        }
        Ok(())
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
            return Err("not a pack of this demo".into());
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

    /// A folder with everything below it, or a pack.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_path(path: &std::path::Path) -> Result<Self, Error> {
        fn walk(files: &mut Files, path: &std::path::Path, depth: usize) -> Result<(), Error> {
            let failed = |e| format!("{}: {e}", path.display());
            if !path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let mut known = FILES.iter().chain(SOUNDS);
                let wanted = known.any(|f| name.to_ascii_lowercase() == *f);
                if wanted {
                    files.insert(&name, std::fs::read(path).map_err(failed)?);
                }
                return Ok(());
            }
            if depth > 6 {
                return Ok(());
            }
            for f in std::fs::read_dir(path).map_err(failed)? {
                walk(files, &f?.path(), depth + 1)?;
            }
            Ok(())
        }
        if path.extension().is_some_and(|e| e == "pack" || e == "xz") {
            return Self::open(&std::fs::read(path)?);
        }
        let mut files = Self::default();
        walk(&mut files, path, 0)?;
        files.check()?;
        Ok(files)
    }
}
