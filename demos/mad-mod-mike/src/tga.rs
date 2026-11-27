//! Uncompressed and RLE true-color TGA files.
use crate::Error;

/// Width, height and RGBA. Rows stay in the file, where the bottom one comes first: the
/// program gives them to OpenGL like that, and its DDS files agree.
pub fn read(d: &[u8]) -> Result<(usize, usize, Vec<u8>), Error> {
    if d.len() < 18 || d[1] != 0 || (d[2] != 2 && d[2] != 10) || (d[16] != 24 && d[16] != 32) {
        return Err("unsupported TGA file".into());
    }
    let int = |o: usize| usize::from(u16::from_le_bytes([d[o], d[o + 1]]));
    let (w, h, n) = (int(12), int(14), usize::from(d[16]) / 8);
    if w == 0 || h == 0 || w * h > 1 << 26 {
        return Err("bad TGA size".into());
    }
    let body = d
        .get(18 + usize::from(d[0])..)
        .ok_or("TGA file is cut short")?;
    let pixels: Vec<u8> = if d[2] == 2 {
        body.get(..w * h * n)
            .ok_or("TGA file is cut short")?
            .to_vec()
    } else {
        // Run-length packets: a repeat count and maybe one pixel.
        let mut out = Vec::with_capacity(w * h * n);
        let mut at = 0;
        while out.len() < w * h * n {
            let head = *body.get(at).ok_or("TGA file is cut short")?;
            at += 1;
            let count = usize::from(head & 0x7f) + 1;
            if head & 0x80 != 0 {
                let px = body.get(at..at + n).ok_or("TGA file is cut short")?;
                at += n;
                for _ in 0..count {
                    out.extend_from_slice(px);
                }
            } else {
                let px = body
                    .get(at..at + count * n)
                    .ok_or("TGA file is cut short")?;
                at += count * n;
                out.extend_from_slice(px);
            }
        }
        out.truncate(w * h * n);
        out
    };
    let mut out = vec![255; w * h * 4];
    for (i, p) in pixels.chunks_exact(n).enumerate() {
        let o = &mut out[i * 4..][..4];
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
    fn rows_keep_their_order() {
        let mut d = vec![0; 18];
        (d[2], d[12], d[14], d[16]) = (2, 1, 2, 24);
        d.extend([1, 2, 3, 4, 5, 6]);
        let (w, h, px) = super::read(&d).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(px, [3, 2, 1, 255, 6, 5, 4, 255]);
        assert!(super::read(&d[..d.len() - 1]).is_err());
    }

    #[test]
    fn runs_expand() {
        let mut d = vec![0; 18];
        (d[2], d[12], d[14], d[16]) = (10, 2, 1, 24);
        // One raw pixel, then one pixel repeated once.
        d.extend([0, 1, 2, 3, 0x80, 7, 8, 9]);
        let (w, h, px) = super::read(&d).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(px, [3, 2, 1, 255, 9, 8, 7, 255]);
        assert!(super::read(&d[..d.len() - 2]).is_err());
    }
}
