use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Eof { at: usize, want: usize, len: usize },
    Trailing { at: usize, len: usize },
    Bad { at: usize, what: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Eof { at, want, len } => {
                write!(f, "read of {want} bytes at {at:#x} passes end {len:#x}")
            }
            Error::Trailing { at, len } => write!(f, "{} unread bytes at {at:#x}", len - at),
            Error::Bad { at, what } => write!(f, "at {at:#x}: {what}"),
        }
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Little-endian cursor over a byte slice.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn bad<T>(&self, what: impl Into<String>) -> Result<T> {
        Err(Error::Bad {
            at: self.pos,
            what: what.into(),
        })
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        let Some(end) = end else {
            return Err(Error::Eof {
                at: self.pos,
                want: n,
                len: self.data.len(),
            });
        };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32_at(self.bytes(4)?, 0))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    /// A count field: the engine treats it as signed and skips when < 1.
    pub fn count(&mut self) -> Result<usize> {
        let n = self.u32()? as i32;
        Ok(n.max(0) as usize)
    }
    /// `n` fixed-size records.
    pub fn records(&mut self, n: usize, stride: usize) -> Result<Vec<&'a [u8]>> {
        let Some(total) = n.checked_mul(stride) else {
            return self.bad("record table size overflow");
        };
        Ok(self.bytes(total)?.chunks_exact(stride.max(1)).collect())
    }
    pub fn u16s(&mut self, n: usize) -> Result<Vec<u16>> {
        Ok(self
            .records(n, 2)?
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect())
    }
    pub fn u32s(&mut self, n: usize) -> Result<Vec<u32>> {
        Ok(self.records(n, 4)?.iter().map(|b| u32_at(b, 0)).collect())
    }
    /// Length-prefixed string (shader header encoding).
    pub fn pstring(&mut self) -> Result<String> {
        let n = self.count()?;
        Ok(self.bytes(n)?.iter().map(|&b| b as char).collect())
    }
    pub fn magic(&mut self, size: usize, want: &str) -> Result<()> {
        let at = self.pos;
        let got = cstr(self.bytes(size)?);
        if got == want {
            Ok(())
        } else {
            Err(Error::Bad {
                at,
                what: format!("magic {got:?}, expected {want:?}"),
            })
        }
    }
    pub fn finish(self) -> Result<()> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(Error::Trailing {
                at: self.pos,
                len: self.data.len(),
            })
        }
    }
}

pub fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"))
}
pub fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}
pub fn count_at(b: &[u8], o: usize) -> usize {
    (u32_at(b, o) as i32).max(0) as usize
}
pub fn cstr(b: &[u8]) -> String {
    b.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_eof_and_trailing() {
        let mut r = Reader::new(&[1, 0, 0, 0, 9]);
        assert_eq!(r.u32().unwrap(), 1);
        assert!(matches!(
            r.u32(),
            Err(Error::Eof {
                at: 4,
                want: 4,
                len: 5
            })
        ));
        assert_eq!(r.finish(), Err(Error::Trailing { at: 4, len: 5 }));
    }

    #[test]
    fn negative_count_is_zero() {
        assert_eq!(Reader::new(&[0xff; 4]).count().unwrap(), 0);
    }

    #[test]
    fn huge_record_table_errors_instead_of_allocating() {
        assert!(Reader::new(&[0; 8]).records(usize::MAX, 8).is_err());
    }
}
