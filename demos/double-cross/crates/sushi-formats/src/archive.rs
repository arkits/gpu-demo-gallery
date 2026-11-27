use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::ops::Range;

use crate::reader::{Error, Reader, Result};

/// The demo's files: an original `Sushi.ati` (an ordinary ZIP), or the same files
/// repacked smaller (see [`pack`]) and compressed with xz. Names are matched
/// case-insensitively with either slash, as the original Windows paths were.
pub struct Archive {
    data: Vec<u8>,
    files: BTreeMap<String, Range<usize>>,
}

const PACK_MAGIC: &[u8; 8] = b"PDPACK1\0";
const XZ_MAGIC: &[u8; 6] = b"\xfd7zXZ\0";

fn key(name: &str) -> String {
    name.split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
        .to_ascii_lowercase()
}

fn bad(what: impl std::fmt::Display) -> Error {
    Error::Bad {
        at: 0,
        what: what.to_string(),
    }
}

impl Archive {
    pub fn open(data: &[u8]) -> Result<Self> {
        if data.starts_with(XZ_MAGIC) {
            let mut raw = Vec::new();
            // Verifies the checksums stored by xz.
            lzma_rust2::XzReader::new(data, false)
                .read_to_end(&mut raw)
                .map_err(|e| bad(format!("xz: {e}")))?;
            return Self::unpack(raw);
        }
        Self::unzip(data)
    }

    fn unzip(zip: &[u8]) -> Result<Self> {
        let err = |e: &dyn std::fmt::Display| bad(format!("zip: {e}"));
        let mut zip = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| err(&e))?;
        let (mut data, mut files) = (Vec::new(), BTreeMap::new());
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| err(&e))?;
            if f.is_dir() {
                continue;
            }
            let name = key(&f.name().map_err(|e| err(&e))?);
            let start = data.len();
            f.read_to_end(&mut data).map_err(|e| err(&e))?;
            files.insert(name, start..data.len());
        }
        Ok(Self { data, files })
    }

    fn unpack(data: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&data);
        if r.bytes(8)? != PACK_MAGIC {
            return r.bad("not a Sushi pack");
        }
        let mut sizes = Vec::new();
        for _ in 0..r.count()? {
            sizes.push((r.pstring()?, r.count()?));
        }
        let mut files = BTreeMap::new();
        for (name, size) in sizes {
            let start = r.pos();
            r.bytes(size)?;
            files.insert(key(&name), start..start + size);
        }
        r.finish()?;
        Ok(Self { data, files })
    }

    /// Uncompressed pack: magic, count, (name, size) table, then contents. Files
    /// of one kind are neighbors, which compresses much better than a ZIP.
    pub fn pack(&self) -> Vec<u8> {
        let mut order: Vec<&str> = self.names().collect();
        order.sort_by_key(|n| (n.rsplit('.').next(), *n));
        let mut out = PACK_MAGIC.to_vec();
        let int = |out: &mut Vec<u8>, n: usize| {
            out.extend(
                u32::try_from(n)
                    .expect("file too large for a pack")
                    .to_le_bytes(),
            );
        };
        int(&mut out, order.len());
        for n in &order {
            int(&mut out, n.len());
            out.extend(n.as_bytes());
            int(&mut out, self.files[*n].len());
        }
        for n in &order {
            out.extend(&self.data[self.files[*n].clone()]);
        }
        out
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(&key(name)).map(|r| &self.data[r.clone()])
    }

    pub fn keep(&mut self, retain: impl Fn(&str) -> bool) {
        let mut data = Vec::new();
        self.files.retain(|name, range| {
            if !retain(name) {
                return false;
            }
            let start = data.len();
            data.extend_from_slice(&self.data[range.clone()]);
            *range = start..data.len();
            true
        });
        self.data = data;
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Archive {
        let data = b"hello world".to_vec();
        let files = [
            ("a/x.ini".into(), 0..5),
            ("b.ssh".into(), 5..11),
            ("e.so2".into(), 3..3),
        ];
        Archive {
            data,
            files: files.into_iter().collect(),
        }
    }

    #[test]
    fn pack_round_trips() {
        let a = sample();
        let b = Archive::unpack(a.pack()).unwrap();
        assert_eq!(b.get("A\\X.INI"), Some(&b"hello"[..]));
        assert!(a.names().eq(b.names()));
        assert!(a.names().all(|n| a.get(n) == b.get(n)));
    }

    #[test]
    fn damaged_packs_are_errors() {
        let p = sample().pack();
        assert!(Archive::unpack(p[..p.len() - 1].to_vec()).is_err());
        assert!(Archive::unpack([&p[..], &[0]].concat()).is_err());
        assert!(Archive::open(b"\xfd7zXZ\0garbage").is_err());
        assert!(Archive::open(b"neither").is_err());
    }
}
