use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use crate::reader::{Error, Result};

/// `Sushi.ati`: an ordinary ZIP. Names are matched case-insensitively with
/// either slash, as the original Windows paths were.
pub struct Archive {
    files: BTreeMap<String, Vec<u8>>,
}

fn key(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

impl Archive {
    pub fn open(data: &[u8]) -> Result<Self> {
        let err = |e: &dyn std::fmt::Display| Error::Bad {
            at: 0,
            what: format!("zip: {e}"),
        };
        let mut zip = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| err(&e))?;
        let mut files = BTreeMap::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| err(&e))?;
            if f.is_dir() {
                continue;
            }
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| err(&e))?;
            let name = key(&f.name().map_err(|e| err(&e))?);
            files.insert(name, buf);
        }
        Ok(Self { files })
    }
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(&key(name)).map(Vec::as_slice)
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}
