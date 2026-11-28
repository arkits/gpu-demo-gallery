//! Uncompressed true-color TGA files.
use crate::Error;

/// Width, height and RGBA. Rows stay as in the file, where the bottom one comes first: the
/// program gives them to OpenGL like that, and its DDS files agree.
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
        let o = &mut out[i * 4..][..4];
        (o[0], o[1], o[2]) = (p[2], p[1], p[0]);
        if n == 4 {
            o[3] = p[3];
        }
    }
    Ok((w, h, out))
}

/// Width, height and the bytes in file order (no BGR swap). The signed
/// `_norm` maps store x, y, z and the specular ramp in that order: byte 2 (z)
/// averages +1 in every one.
pub fn read_raw(d: &[u8]) -> Result<(usize, usize, Vec<u8>), Error> {
    let (w, h, mut px) = read(d)?;
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    Ok((w, h, px))
}

#[cfg(test)]
mod tests {
    #[test]
    fn rows_keep_their_order() {
        let mut d = vec![0; 18];
        (d[2], d[12], d[14], d[16]) = (2, 1, 2, 24);
        d.extend([1, 2, 3, 4, 5, 6]);
        let (w, h, px) = super::read(&d).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(px, [3, 2, 1, 255, 6, 5, 4, 255]);
    }
}
