//! Light transfer (`SOLADB_CPCA_v1.1` blocks; reader 0x45d0c0): light sampled along an
//! object's way, and what its vertices do with light.
use crate::reader::*;

/// Data block kinds.
pub const TRANSFER: u32 = 1;
pub const WEIGHTS: u32 = 2;

#[derive(Debug, Clone)]
pub struct Sample {
    pub time: f64,
    pub position: [f32; 3],
    /// Spherical harmonics: red, green and blue.
    pub light: Vec<f32>,
    /// How they change with the position, three numbers each.
    pub slopes: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct Transfer {
    /// Bands of spherical harmonics.
    pub order: usize,
    /// Means and principal components of the vertices' transfer, by cluster.
    pub clusters: Vec<f32>,
    /// Lights added while running.
    pub lights: Vec<String>,
    /// Coefficients per color.
    pub count: usize,
    pub samples: Vec<Sample>,
}

pub fn read_transfer(data: &[u8]) -> Result<Transfer> {
    let mut r = Reader::new(data);
    r.magic(0x11, "SOLADB_CPCA_v1.1")?;
    let n = r.count()?;
    let order = r.count()?;
    // Settings of the simulation that made this.
    r.bytes(0x85 - 0x19)?;
    let clusters = r.f32s(n)?;
    let mut lights = Vec::new();
    for _ in 0..r.count()? {
        let mut name = String::new();
        loop {
            match r.u8()? {
                0 => break,
                c => name.push(c as char),
            }
        }
        lights.push(name);
    }
    let fixed = r.count()?;
    let bands = r.count()?;
    r.f32s(fixed * bands * bands * 3)?;
    let samples = r.count()?;
    let count = r.count()?;
    let flags = r.bytes(6)?;
    let (colors, slopes) = (if flags[0] != 0 { 3 } else { 1 }, flags[5] != 0);
    let mut out = Vec::new();
    for _ in 0..samples {
        let position = [r.f32()?, r.f32()?, r.f32()?];
        let time = f64::from_le_bytes(r.bytes(8)?.try_into().expect("8 bytes"));
        let mut light = r.f32s(count * colors)?;
        let mut change = if slopes {
            r.f32s(count * colors * 3)?
        } else {
            Vec::new()
        };
        if colors == 1 {
            light = light.repeat(3);
            change = change.repeat(3);
        }
        out.push(Sample {
            time,
            position,
            light,
            slopes: change,
        });
    }
    r.finish()?;
    Ok(Transfer {
        order,
        clusters,
        lights,
        count,
        samples: out,
    })
}

/// Weights of a mesh's shapes over time.
#[derive(Debug, Clone)]
pub struct Weights {
    pub shapes: usize,
    /// Most shapes at once.
    pub most: usize,
    pub duration: f32,
    /// `keys[frame][shape]`.
    pub keys: Vec<Vec<f32>>,
    /// Seconds to the next key.
    pub gaps: Vec<f32>,
}

pub fn read_weights(data: &[u8]) -> Result<Weights> {
    let mut r = Reader::new(data);
    let (shapes, frames, duration, most) = (r.count()?, r.count()?, r.f32()?, r.count()?);
    let all = r.f32s(shapes * frames)?;
    let keys = all
        .chunks_exact(shapes.max(1))
        .map(<[f32]>::to_vec)
        .collect();
    let gaps = r.f32s(frames)?;
    r.finish()?;
    Ok(Weights {
        shapes,
        most,
        duration,
        keys,
        gaps,
    })
}
