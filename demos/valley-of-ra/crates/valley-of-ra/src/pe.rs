//! Reads numbers from a Windows program's data by virtual address, for the
//! few tables an ATB demo keeps in its `.exe` rather than its scene file.
use crate::Error;

pub struct Pe<'a> {
    d: &'a [u8],
    base: u32,
    /// Virtual size, virtual address, raw size, raw offset.
    sections: Vec<(u32, u32, u32, u32)>,
}

impl<'a> Pe<'a> {
    pub fn new(d: &'a [u8]) -> Result<Self, Error> {
        let word = |o: usize| -> Result<u32, Error> {
            Ok(u32::from_le_bytes(
                d.get(o..o + 4).ok_or("program cut short")?.try_into()?,
            ))
        };
        let half = |o: usize| -> Result<u16, Error> {
            Ok(u16::from_le_bytes(
                d.get(o..o + 2).ok_or("program cut short")?.try_into()?,
            ))
        };
        if d.get(..2) != Some(b"MZ") {
            return Err("not a Windows program".into());
        }
        let pe = word(0x3c)? as usize;
        if d.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err("not a Windows program".into());
        }
        let n = half(pe + 6)? as usize;
        let at = pe + 24 + half(pe + 20)? as usize;
        let base = word(pe + 52)?;
        let sections = (0..n)
            .map(|i| {
                let s = at + 40 * i;
                Ok((word(s + 8)?, word(s + 12)?, word(s + 16)?, word(s + 20)?))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { d, base, sections })
    }

    fn at(&self, va: u32, len: u32) -> Result<&'a [u8], Error> {
        let rva = va
            .checked_sub(self.base)
            .ok_or("address below the program")?;
        for &(_, start, raw, offset) in &self.sections {
            if rva >= start && rva - start + len <= raw {
                let o = (offset + rva - start) as usize;
                return self
                    .d
                    .get(o..o + len as usize)
                    .ok_or_else(|| "program cut short".into());
            }
        }
        Err("address outside the program's data".into())
    }

    pub fn word(&self, va: u32) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.at(va, 4)?.try_into()?))
    }

    pub fn float(&self, va: u32) -> Result<f32, Error> {
        Ok(f32::from_bits(self.word(va)?))
    }
}
