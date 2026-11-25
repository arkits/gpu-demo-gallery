//! 3dfx `.BOF` scene/object databases (`BOFM`).
//!
//! Observed layout, validated against every staged ATB demo (Grand Bleu 38 dict
//! entries, Race 7, Wizards 17, Valley-of-Ra 9, Grand Bleu 2, Flip 1, plus all
//! Grand Bleu model BOFs):
//!
//! * `BOFM` magic, then three big-endian version words (e.g. 2, 1, 5).
//! * A `Dict` tag at offset 0x10.
//! * A little-endian u32 entry count at `Dict + 12`.
//! * From `Dict + 16`, one record per entry: `[u8 len][name incl NUL]`, followed
//!   by a small fixed trailer (id/flags; the exact semantics are unresolved).
//!   Names are ASCII: `helpscreen`, `0000.3DF`, `demolite`, `start`, …
//!
//! The full object graph after the dict is still opaque (raw memory dumps with
//! baked pointers). This reader exposes the validated name table and rejects
//! anything that does not match the observed framing; it does not fabricate a
//! scene. Fields beyond the dict are marked `[I]` in the docs.

use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bof {
    /// The three version words after `BOFM` (endianness as stored).
    pub version: [u32; 3],
    /// Dict names, in file order, without NUL terminators.
    pub names: Vec<String>,
    /// Full original bytes, for later object-graph parsing.
    pub data: Vec<u8>,
}

impl Bof {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 32 || &bytes[..4] != b"BOFM" {
            return Err("not a BOFM file".into());
        }
        let word = |o: usize| u32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
        let version = [word(4), word(8), word(12)];

        let dict = bytes[..bytes.len().min(64)]
            .windows(4)
            .position(|w| w == b"Dict")
            .ok_or("BOFM Dict tag not found")?;
        let count_at = dict + 4 + 8;
        let count = u32::from_le_bytes(
            bytes
                .get(count_at..count_at + 4)
                .ok_or("BOFM cut short in Dict header")?
                .try_into()?,
        ) as usize;
        // Sanity bound: no staged file has more than a few hundred names.
        if count > 4096 {
            return Err(format!("implausible Dict count {count}").into());
        }

        let mut names = Vec::with_capacity(count);
        let mut o = dict + 4 + 12;
        while names.len() < count && o < bytes.len() {
            let len = bytes[o] as usize;
            if (3..=63).contains(&len) {
                let raw = bytes.get(o + 1..o + 1 + len).ok_or("BOFM name overruns")?;
                if let Some(end) = raw.iter().rposition(|&c| c != 0) {
                    let text = &raw[..=end];
                    if text.iter().all(|&c| (32..127).contains(&c)) {
                        names.push(String::from_utf8_lossy(text).into_owned());
                        o += 1 + len;
                        continue;
                    }
                }
            }
            o += 1; // skip trailer bytes / id words between records
        }
        if names.len() != count {
            return Err(format!("BOFM Dict: expected {count} names, found {}", names.len()).into());
        }
        Ok(Self {
            version,
            names,
            data: bytes.to_vec(),
        })
    }

    /// Index of a name (case-insensitive, as on Windows), if present.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n.eq_ignore_ascii_case(name))
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_be(v: u32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }
    fn record(name: &str) -> Vec<u8> {
        let mut r = vec![(name.len() + 1) as u8];
        r.extend_from_slice(name.as_bytes());
        r.push(0); // NUL
        r.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0]); // observed trailer
        r
    }

    fn fixture(names: &[&str]) -> Vec<u8> {
        let mut d = b"BOFM".to_vec();
        for v in [2u32, 1, 5] {
            d.extend(word_be(v));
        }
        d.extend_from_slice(b"Dict");
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&1u32.to_be_bytes());
        d.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for n in names {
            d.extend(record(n));
        }
        d
    }

    #[test]
    fn reads_dict() {
        let b = Bof::parse(&fixture(&["helpscreen", "0000.3DF", "demolite"])).unwrap();
        assert_eq!(b.version, [2, 1, 5]);
        assert_eq!(b.names, ["helpscreen", "0000.3DF", "demolite"]);
        assert_eq!(b.find("DEMOLITE"), Some(2));
        assert_eq!(b.find("nope"), None);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(Bof::parse(b"nope").is_err());
        assert!(Bof::parse(b"BOFM").is_err());
        // A declared count larger than the names present is refused.
        let mut d = fixture(&["one"]);
        let count_at = 16 + 4 + 8;
        d[count_at..count_at + 4].copy_from_slice(&9u32.to_le_bytes());
        assert!(Bof::parse(&d).is_err());
    }
}
