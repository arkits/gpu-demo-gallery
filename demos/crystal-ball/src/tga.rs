//! Uncompressed true-color TGA files.
use crate::Error;

/// Width, height and RGBA, top row first.
pub fn read(d: &[u8]) -> Result<(usize, usize, Vec<u8>), Error> {
    if d.len() < 18 || d[1] != 0 || d[2] != 2 || (d[16] != 24 && d[16] != 32) {
        return Err("unsupported TGA file".into());
    }
    let int = |o: usize| usize::from(u16::from_le_bytes([d[o], d[o + 1]]));
    let (w, h, n) = (int(12), int(14), usize::from(d[16]) / 8);
    let at = 18 + usize::from(d[0]);
    let body = d.get(at..at + w * h * n).ok_or("TGA file is cut short")?;
    let mut out = vec![255; w * h * 4];
    for (i, p) in body.chunks_exact(n).enumerate() {
        let y = if d[17] & 0x20 == 0 {
            h - 1 - i / w
        } else {
            i / w
        };
        let o = &mut out[(y * w + i % w) * 4..][..4];
        (o[0], o[1], o[2]) = (p[2], p[1], p[0]);
        if n == 4 {
            o[3] = p[3];
        }
    }
    Ok((w, h, out))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rows_run_upward() {
        let mut d = vec![0; 18];
        (d[2], d[12], d[14], d[16]) = (2, 1, 2, 24);
        d.extend([1, 2, 3, 4, 5, 6]);
        let (w, h, px) = super::read(&d).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(px, [6, 5, 4, 255, 3, 2, 1, 255]);
    }
}
