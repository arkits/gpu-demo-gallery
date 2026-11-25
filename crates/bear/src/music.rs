//! The soundtrack: one loop.

pub struct Pcm {
    pub rate: u32,
    pub channels: usize,
    /// Interleaved, -1 to 1.
    pub samples: Vec<f32>,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
}

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const NEXT: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// IMA ADPCM in a WAV file, which browsers do not play.
pub fn decode_wav(d: &[u8]) -> Result<Pcm, String> {
    let u16_at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut format, mut body, mut length) = (None, None, None);
    let mut at = 12;
    while at + 8 <= d.len() {
        let (name, size) = (&d[at..at + 4], u32_at(at + 4) as usize);
        let end = (at + 8)
            .checked_add(size)
            .ok_or("bad chunk size")?
            .min(d.len());
        match name {
            b"fmt " if end - at >= 8 + 20 => format = Some(at + 8),
            b"fact" if end - at >= 8 + 4 => length = Some(u32_at(at + 8) as usize),
            b"data" => body = Some(&d[at + 8..end]),
            _ => {}
        }
        at = end + size % 2;
    }
    let f = format.ok_or("no format chunk")?;
    let body = body.ok_or("no data chunk")?;
    if u16_at(f) != 0x11 || u16_at(f + 14) != 4 {
        return Err("not IMA ADPCM".into());
    }
    let channels = usize::from(u16_at(f + 2));
    let block = usize::from(u16_at(f + 12));
    if !(1..=2).contains(&channels) || block <= 4 * channels || u32_at(f + 4) == 0 {
        return Err("unsupported ADPCM layout".into());
    }
    let mut samples = Vec::new();
    for b in body.chunks(block).filter(|b| b.len() > 4 * channels) {
        let mut state = Vec::new();
        for c in 0..channels {
            let h = &b[c * 4..];
            let value = i32::from(i16::from_le_bytes([h[0], h[1]]));
            state.push((value, usize::from(h[2]).min(88)));
            samples.push(value as f32 / 32768.0);
        }
        // Eight samples of each channel in turn, low nibble first.
        for group in b[4 * channels..].chunks_exact(4 * channels) {
            let mut out = [[0.0; 2]; 8];
            for (c, bytes) in group.chunks_exact(4).enumerate() {
                for (i, o) in out.iter_mut().enumerate() {
                    let n = i32::from(bytes[i / 2] >> (4 * (i % 2))) & 15;
                    o[c] = step(&mut state[c], n) as f32 / 32768.0;
                }
            }
            for o in out {
                samples.extend(&o[..channels]);
            }
        }
    }
    if let Some(n) = length {
        samples.truncate(n * channels);
    }
    Ok(Pcm {
        rate: u32_at(f + 4),
        channels,
        samples,
    })
}

fn step(state: &mut (i32, usize), nibble: i32) -> i32 {
    let size = STEPS[state.1];
    let mut diff = size >> 3;
    for (bit, shift) in [(4, 0), (2, 1), (1, 2)] {
        if nibble & bit != 0 {
            diff += size >> shift;
        }
    }
    let sign = if nibble & 8 != 0 { -1 } else { 1 };
    state.0 = (state.0 + sign * diff).clamp(-32768, 32767);
    state.1 = (state.1 as i32 + NEXT[(nibble & 7) as usize]).clamp(0, 88) as usize;
    state.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(block: &[u8], frames: u32) -> Vec<u8> {
        let mut w = b"RIFF\0\0\0\0WAVEfmt \x14\0\0\0".to_vec();
        w.extend([0x11, 0, 1, 0, 0x44, 0xac, 0, 0, 0, 0, 0, 0]);
        w.extend((block.len() as u16).to_le_bytes());
        w.extend([4, 0, 2, 0, 9, 0]);
        w.extend(b"fact\x04\0\0\0");
        w.extend(frames.to_le_bytes());
        w.extend(b"data");
        w.extend((block.len() as u32).to_le_bytes());
        w.extend(block);
        w
    }

    #[test]
    fn decodes_known_steps() {
        // Starts at 100 with step size 7; nibbles 7, 0, 15.
        let w = wav(&[100, 0, 0, 0, 0x07, 0x0f, 0, 0], 4);
        let p = decode_wav(&w).unwrap();
        let v: Vec<i32> = p.samples.iter().map(|s| (s * 32768.0) as i32).collect();
        // 7>>3 + 7 + 3 + 1 = 11; then step 16: 2; then step 14: -(1+14+7+3).
        assert_eq!(v, [100, 111, 113, 88]);
        assert!(decode_wav(&w[..30]).is_err());
    }

    #[test]
    fn the_demo_music_decodes() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/Audio/");
        let Ok(file) = std::fs::read(format!("{dir}BearLoop.wav")) else {
            eprintln!("skipped: no music in data/Audio");
            return;
        };
        let p = decode_wav(&file).unwrap();
        let seconds = p.frames() as f64 / f64::from(p.rate);
        assert!((seconds - 7.924).abs() < 0.05, "{seconds}");
        // Wrongly decoded ADPCM drifts to the rails.
        let rails = p.samples.iter().filter(|s| s.abs() > 0.99).count();
        assert!(rails * 200 < p.samples.len(), "{rails}");
        let mean = p.samples.iter().map(|s| f64::from(*s)).sum::<f64>();
        let mean = mean / p.samples.len() as f64;
        assert!(mean.abs() < 0.02, "{mean}");
    }
}
