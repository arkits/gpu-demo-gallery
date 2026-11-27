//! The demo's own files: an unpacked folder, a program's resources, or a pack. Names are
//! matched without regard to case, as on Windows.
use std::collections::BTreeMap;
use std::io::Read;

use crate::Error;
use crate::demo::{FILES, PARTS, SOUNDS};
use crate::image::{self, Image};

const MAGIC: &[u8; 8] = b"DEMOPAK1";

#[derive(Default)]
pub struct Files(BTreeMap<String, Vec<u8>>);

impl Files {
    /// Keeps files the demo can use, by their names without directories. Programs are
    /// opened for the pictures and sounds in them.
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        let name = name.rsplit(['\\', '/']).next().unwrap_or("");
        let name = name.to_ascii_lowercase();
        if name.ends_with(".exe") && !FILES.contains(&name.as_str()) {
            let parts = parts(&data).unwrap_or_default();
            let resources = resources(&data).unwrap_or_default();
            for (name, data) in parts.into_iter().chain(resources) {
                self.insert(&name, data);
            }
        } else if FILES.iter().chain(SOUNDS).any(|f| *f == name) {
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
        let mut out = MAGIC.to_vec();
        out.extend((FILES.len() as u32).to_le_bytes());
        for n in FILES {
            out.extend((n.len() as u32).to_le_bytes());
            out.extend(n.as_bytes());
            out.extend((self.get(n)?.len() as u32).to_le_bytes());
        }
        for n in FILES {
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
            files.0.insert(name, data);
        }
        if !d.is_empty() {
            return Err("pack has extra bytes".into());
        }
        Ok(files)
    }

    /// A folder with everything below it, a program, or a pack.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_path(path: &std::path::Path) -> Result<Self, Error> {
        fn walk(files: &mut Files, path: &std::path::Path, depth: usize) -> Result<(), Error> {
            let failed = |e| format!("{}: {e}", path.display());
            if !path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let known = FILES.iter().chain(SOUNDS);
                let wanted = known.chain(&[".exe"]).any(|f| {
                    let name = name.to_ascii_lowercase();
                    name == *f || f.starts_with('.') && name.ends_with(f)
                });
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

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn int(d: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// Where the headers of a Windows program begin.
fn header(d: &[u8]) -> Option<usize> {
    let pe = int(d, 0x3c)?;
    (d.get(pe..pe + 4)? == b"PE\0\0" && u16_at(d, pe + 24)? == 0x10b).then_some(pe)
}

/// Where in the file of a program something is, by its address from the program's base.
fn place(d: &[u8], address: usize) -> Option<usize> {
    let pe = header(d)?;
    let sections = pe + 24 + usize::from(u16_at(d, pe + 20)?);
    (0..usize::from(u16_at(d, pe + 6)?)).find_map(|i| {
        let s = sections + 40 * i;
        let (size, start) = (int(d, s + 8)?.max(int(d, s + 16)?), int(d, s + 12)?);
        (start..start + size)
            .contains(&address)
            .then_some(address - start + int(d, s + 20)?)
    })
}

fn parts(d: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let base = int(d, header(d)? + 24 + 28)?;
    let part = |&(name, address, length): &(&str, usize, usize)| {
        let at = place(d, address.checked_sub(base)?)?;
        Some((name.to_string(), d.get(at..at + length)?.to_vec()))
    };
    PARTS.iter().map(part).collect()
}

/// Pictures and sounds of a Windows program, named `back.png` or `spark.wav`.
fn resources(d: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let pe = header(d)?;
    let (u16_at, int) = (|o| u16_at(d, o), |o| int(d, o));
    let place = |address| place(d, address);
    let root = place(int(pe + 24 + 96 + 16)?)?;
    // Entries of a directory: name if it has one, whether a directory, offset.
    let list = |at: usize| -> Option<Vec<(String, bool, usize)>> {
        let n = usize::from(u16_at(at + 12)?) + usize::from(u16_at(at + 14)?);
        (0..n)
            .map(|i| {
                let (name, to) = (int(at + 16 + 8 * i)?, int(at + 20 + 8 * i)?);
                let mut text = String::new();
                if name >> 31 == 1 {
                    let at = root + (name & 0x7fff_ffff);
                    for k in 0..usize::from(u16_at(at)?) {
                        text.push(char::from_u32(u16_at(at + 2 + 2 * k)?.into())?);
                    }
                }
                Some((text, to >> 31 == 1, root + (to & 0x7fff_ffff)))
            })
            .collect()
    };
    let mut out = Vec::new();
    for (kind, _, at) in list(root)? {
        let extension = match kind.as_str() {
            "PNG" => "png",
            "WAVE" => "wav",
            _ => continue,
        };
        for (name, _, at) in list(at)? {
            for (_, _, leaf) in list(at)? {
                let start = place(int(leaf)?)?;
                let data = d.get(start..start + int(leaf + 4)?)?.to_vec();
                let name = name.trim_matches('"').to_ascii_lowercase();
                out.push((format!("{name}.{extension}"), data));
            }
        }
    }
    Some(out)
}
