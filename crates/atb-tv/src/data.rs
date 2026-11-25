//! The demo's own files: an unpacked folder, or a packed `.xz`. Names are
//! matched without regard to case, as on Windows. Nothing here is committed:
//! the demo's files are 3dfx's.
use std::collections::BTreeMap;
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::Error;
use crate::demo::{FILES, SOUNDS};
use crate::image::{self, Image};

const MAGIC: &[u8; 8] = b"DEMOPAK1";

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

impl atb::Source for Files {
    fn names(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
    fn get(&self, name: &str) -> Option<&[u8]> {
        self.0.get(name).map(Vec::as_slice)
    }
}

impl Files {
    /// Keeps only files the demo can use, by their names without directories.
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        let name = name.rsplit(['\\', '/']).next().unwrap_or("");
        let name = name.to_ascii_lowercase();
        if FILES.iter().chain(SOUNDS).any(|f| *f == name) {
            self.0.insert(name, data);
        }
    }

    pub fn get(&self, name: &str) -> Result<&[u8], Error> {
        self.0
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
            .ok_or_else(|| format!("missing {name}").into())
    }

    pub fn image(&self, name: &str) -> Result<Image, Error> {
        image::read(name, self.get(name)?)
    }

    /// Fails with the name of what is missing.
    pub fn check(&self) -> Result<(), Error> {
        FILES.iter().try_for_each(|f| self.get(f).map(|_| ()))
    }

    /// Magic, count, then a (name, size) table, then contents.
    pub fn pack(&self) -> Result<Vec<u8>, Error> {
        let mut out = MAGIC.to_vec();
        out.extend((FILES.len() as u32).to_le_bytes());
        for &n in FILES {
            let name: &str = n;
            out.extend((name.len() as u32).to_le_bytes());
            out.extend(name.as_bytes());
            out.extend((self.get(name)?.len() as u32).to_le_bytes());
        }
        for &n in FILES {
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
            files.0.insert(name.to_ascii_lowercase(), data);
        }
        if !d.is_empty() {
            return Err("pack has extra bytes".into());
        }
        Ok(files)
    }

    /// A folder with everything below it (up to a few levels), or a pack.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_path(path: &Path) -> Result<Self, Error> {
        if path.extension().is_some_and(|e| e == "pack" || e == "xz") {
            return Self::open(&std::fs::read(path)?);
        }
        if !path.is_dir() {
            return Err(format!("{} is not a directory", path.display()).into());
        }
        let mut files = Self::default();
        collect(&mut files, path, 0)?;
        files.check()?;
        Ok(files)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn collect(files: &mut Files, path: &Path, depth: usize) -> Result<(), Error> {
    if depth > 4 {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect(files, &p, depth + 1)?;
        } else if p.is_file() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let lower = name.to_ascii_lowercase();
            if FILES.iter().chain(SOUNDS).any(|f| *f == lower) {
                files.insert(&lower, std::fs::read(&p)?);
            }
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn find_data() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ATB_TV_DATA") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let p = PathBuf::from("data");
    if p.is_dir() { Some(p) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Files {
        let mut f = Files::default();
        // Drive the fixture from this port's own FILES so the pack test is
        // meaningful for every ATB demo, not just one.
        for (i, n) in FILES.iter().enumerate() {
            f.insert(n, vec![i as u8; 16]);
        }
        f
    }

    #[test]
    fn pack_roundtrip() {
        let f = sample();
        f.check().unwrap();
        let packed = f.pack().unwrap();
        let again = Files::open(&packed).unwrap();
        for n in FILES {
            assert_eq!(f.get(n).unwrap(), again.get(n).unwrap(), "{n}");
        }
    }

    #[test]
    fn rejects_bad_pack() {
        assert!(Files::open(b"nope").is_err());
        let mut d = sample().pack().unwrap();
        d.truncate(d.len() - 2);
        assert!(Files::open(&d).is_err());
    }
}
