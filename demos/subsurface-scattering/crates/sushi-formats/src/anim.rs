use crate::mesh::*;
use crate::object::{BLOCKS, MATERIAL_MAGIC};
use crate::reader::*;

/// `.sa2` animation (reader 0x438350).
#[derive(Debug, Clone)]
pub struct Animation {
    pub flags: u32,
    pub bound_gaps: Vec<f64>,
    pub blocks: Vec<Blocks>,
    pub unk0: u32,
    /// Seconds; sample time is taken modulo this (0x438d10).
    pub duration: f32,
    pub bones: Vec<Track>,
    pub morph_header: Vec<u8>,
    pub morph_meshes: Vec<Mesh>,
    pub params: Vec<ParamTrack>,
    pub max_time: f32,
    pub bounds: Vec<Vec<u8>>,
    pub total_bounds: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ParamTrack {
    pub raw: Vec<u8>,
    pub name: String,
    /// Seconds to next key, then four components.
    pub keys: Vec<[f32; 5]>,
}

/// What a bone's records hold (0x46b860), by the type's number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    /// 4x4 matrix.
    Matrix,
    /// 3x3 rows and position.
    Affine,
    /// The first record holds the position; the rest 3x3 rows.
    FixedPosition,
    /// The first three records hold 3x3 rows; the rest positions.
    FixedRotation,
    /// Quaternion and position.
    QuatPosition,
    /// The first record holds the position; the rest quaternions.
    QuatFixedPosition,
    /// Position and two more numbers (shear).
    Position5,
}

/// One moment of a bone. Pose, then visibility, frame and number of frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// Row-vector matrix.
    pub matrix: [f32; 16],
    pub visible: f32,
    pub frame: f32,
    pub frames: f32,
}

/// Keys of one bone, with the seconds from each to the next; the last has -1.
#[derive(Debug, Clone)]
pub struct Track {
    pub kind: u32,
    pub pose: Pose,
    pub gaps: Vec<f64>,
    pub keys: Vec<Key>,
}

fn keys<const N: usize>(r: &mut Reader, n: usize) -> Result<Vec<[f32; N]>> {
    let recs = r.records(n, N * 4)?;
    let of = |b: &&[u8]| std::array::from_fn(|i| f32_at(b, i * 4));
    Ok(recs.iter().map(of).collect())
}

fn matrix3(r: &[f32], t: &[f32]) -> [f32; 16] {
    [
        r[0], r[1], r[2], 0.0, r[3], r[4], r[5], 0.0, r[6], r[7], r[8], 0.0, t[0], t[1], t[2], 1.0,
    ]
}

/// Same convention as the engine's quaternion to matrix (0x46d122).
fn quaternion(q: &[f32], t: &[f32]) -> [f32; 16] {
    let [x, y, z, w] = [q[0], q[1], q[2], q[3]];
    let r = [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    ];
    matrix3(&r, t)
}

impl Track {
    fn read(r: &mut Reader, long: bool) -> Result<Self> {
        let (n, kind) = (r.count()?, r.u32()?);
        // A pointer; then, if the records are long, 30 and numbers of frames.
        r.u32()?;
        if long {
            r.bytes(0x10)?;
        }
        if !(1..=39).contains(&kind) {
            return r.bad(format!("unknown bone track type {kind}"));
        }
        // Sevens: plain, double times, visibility, both, frame numbers, both.
        // Those with double times lack the matrix.
        let (family, base) = match kind {
            1..=7 => (0, kind - 1),
            8..=13 => (1, kind - 8 + 1),
            14..=20 => (2, kind - 14),
            21..=26 => (3, kind - 21 + 1),
            27..=33 => (4, kind - 27),
            _ => (5, kind - 34 + 1),
        };
        let pose = [
            Pose::Matrix,
            Pose::Affine,
            Pose::FixedPosition,
            Pose::FixedRotation,
            Pose::QuatPosition,
            Pose::QuatFixedPosition,
            Pose::Position5,
        ][base as usize];
        let width = [16, 12, 9, 3, 7, 4, 5][base as usize];
        let skip = if family % 2 == 1 { 2 } else { 1 };
        let extra = [0, 1, 3][(family / 2) as usize];
        let stride = (skip + width + extra) * 4;
        let recs = r.records(n, stride)?;
        let lead = match pose {
            Pose::FixedPosition | Pose::QuatFixedPosition => 1,
            Pose::FixedRotation => 3,
            _ => 0,
        };
        if recs.len() < lead + 1 {
            return r.bad("bone track without keys");
        }
        let floats =
            |b: &[u8]| -> Vec<f32> { (0..width).map(|i| f32_at(b, (skip + i) * 4)).collect() };
        let shared: Vec<f32> = recs[..lead]
            .iter()
            .flat_map(|b| floats(b)[..3].to_vec())
            .collect();
        let (mut gaps, mut keys) = (Vec::new(), Vec::new());
        for b in &recs[lead..] {
            gaps.push(match skip {
                2 => f64::from_le_bytes(b[..8].try_into().expect("8 bytes")),
                _ => f64::from(f32_at(b, 0)),
            });
            let v = floats(b);
            let matrix = match pose {
                Pose::Matrix => std::array::from_fn(|i| v[i]),
                Pose::Affine => matrix3(&v, &v[9..]),
                Pose::FixedPosition => matrix3(&v, &shared),
                Pose::FixedRotation => matrix3(&shared, &v),
                Pose::QuatPosition => quaternion(&v, &v[4..]),
                Pose::QuatFixedPosition => quaternion(&v, &shared),
                Pose::Position5 => {
                    let mut m = matrix3(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], &v);
                    (m[3], m[7]) = (v[3], v[4]);
                    m
                }
            };
            let at = (skip + width) * 4;
            let int = |o: usize| u32_at(b, o) as i32 as f32;
            keys.push(Key {
                matrix,
                visible: if extra > 0 { int(at) } else { 1.0 },
                frame: if extra > 1 { f32_at(b, at + 4) } else { 0.0 },
                frames: if extra > 1 { int(at + 8) } else { 1.0 },
            });
        }
        Ok(Self {
            kind,
            pose,
            gaps,
            keys,
        })
    }
}

pub fn read_animation(data: &[u8]) -> Result<Animation> {
    let mut r = Reader::new(data);
    r.magic(0x18, "SOA RUN00 v00.02.01")?;
    r.magic(0x20, MATERIAL_MAGIC)?;
    let flags = r.u32()?;
    let (unk0, duration) = (r.u32()?, r.f32()?);
    let n = r.count()?;
    r.u32()?; // runtime pointer
    let bones = (0..n)
        .map(|_| Track::read(&mut r, unk0 & 1 != 0))
        .collect::<Result<_>>()?;
    let morph_header = r.bytes(0x10)?.to_vec();
    let morph_meshes = read_meshes(&mut r)?;
    let mut params = Vec::new();
    for _ in 0..r.count()? {
        let raw = r.bytes(0x5c)?;
        let n = u32_at(raw, 0x54) as usize;
        params.push(ParamTrack {
            raw: raw.to_vec(),
            name: cstr(&raw[4..0x44]),
            keys: keys(&mut r, n)?,
        });
    }
    let max_time = r.f32()?;
    let nb = r.count()?;
    let bounds = r
        .records(nb, 0x28)?
        .into_iter()
        .map(<[u8]>::to_vec)
        .collect();
    // Seconds from each to the next (0x466a80).
    let bound_gaps = if flags & 0x100_0000 != 0 {
        let times = r.records(nb, 8)?;
        let of = |b: &&[u8]| f64::from_le_bytes((*b).try_into().expect("8 bytes"));
        times.iter().map(of).collect()
    } else if flags & 0x40_0000 != 0 {
        r.f32s(nb)?.into_iter().map(f64::from).collect()
    } else {
        vec![f64::from(max_time) / nb.max(1) as f64; nb]
    };
    let total_bounds = r.bytes(0x28)?.to_vec();
    let blocks = match flags & BLOCKS {
        0 => Vec::new(),
        _ => read_blocks(&mut r)?,
    };
    r.finish()?;
    Ok(Animation {
        flags,
        bound_gaps,
        blocks,
        unk0,
        duration,
        bones,
        morph_header,
        morph_meshes,
        params,
        max_time,
        bounds,
        total_bounds,
    })
}
