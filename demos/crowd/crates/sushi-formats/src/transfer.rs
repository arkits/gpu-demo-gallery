//! Light transfer (`SOLADB_CPCA_v1.1` blocks; reader 0x45d0c0): light sampled along an
//! object's way, and what its vertices do with light.
use crate::reader::*;

/// Data block kinds.
pub const VECTOR: u32 = 0;
pub const TRANSFER: u32 = 1;
pub const WEIGHTS: u32 = 2;

/// A vector the engine hands shaders as `SOLVectorADB<index>` (SOLVectorADB.cpp,
/// 0x474350 in Crowd's SushiDX.exe): four floats, then the index.
pub fn read_vector(data: &[u8]) -> Result<(String, [f32; 4])> {
    let mut r = Reader::new(data);
    let v = r.f32s(4)?;
    let index = r.u32()?;
    r.finish()?;
    Ok((format!("SOLVectorADB{index}"), [v[0], v[1], v[2], v[3]]))
}

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
    /// Lighting projected into SH by the exporter, one RGB triple per light.
    pub projected_lights: Vec<[Vec<f32>; 3]>,
}

pub fn read_transfer(data: &[u8]) -> Result<Transfer> {
    let mut r = Reader::new(data);
    let compact = !data.starts_with(b"SOLADB_CPCA_v1.1");
    if data.starts_with(b"SOLADB_CPCA_v1.0") {
        return r.bad("obsolete SOLADB_CPCA_v1.0 block");
    }
    if !compact {
        r.magic(0x11, "SOLADB_CPCA_v1.1")?;
    }
    let n = r.count()?;
    let order = r.count()?;
    if !(1..=6).contains(&order) {
        return r.bad("invalid CPCA band count");
    }
    // Settings of the simulation that made this.
    r.bytes(if compact { 0x74 - 8 } else { 0x85 - 0x19 })?;
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
    if fixed > 32 || bands > 6 {
        return r.bad("invalid projected CPCA light dimensions");
    }
    let mut projected_lights = Vec::with_capacity(fixed);
    for _ in 0..fixed {
        let n = bands * bands;
        projected_lights.push([r.f32s(n)?, r.f32s(n)?, r.f32s(n)?]);
    }
    // The compact record (SushiDX.exe 0x45ce30) ends with projected lights.
    if compact {
        r.finish()?;
        return Ok(Transfer {
            order,
            clusters,
            lights,
            count: order * order,
            samples: Vec::new(),
            projected_lights,
        });
    }
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
        projected_lights,
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
