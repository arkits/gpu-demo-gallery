use crate::mesh::*;
use crate::object::MATERIAL_MAGIC;
use crate::reader::*;

/// `.sa2` animation (reader 0x438350).
#[derive(Debug, Clone)]
pub struct Animation {
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

/// Bone keys. Every record starts with the seconds until the next key.
/// `Static*` variants store the shared part in leading records.
#[derive(Debug, Clone)]
pub enum Track {
    /// Type 1: dt + 4x4 matrix.
    Matrix(Vec<[f32; 17]>),
    /// Type 2: dt + 3x3 rows + translation.
    Affine(Vec<[f32; 13]>),
    /// Type 3: record 0 holds the translation; the rest dt + 3x3.
    RotationFixedPos(Vec<[f32; 10]>),
    /// Type 4: records 0..3 hold 3x3 rows; the rest dt + translation.
    TranslationFixedRot(Vec<[f32; 4]>),
    /// Type 5: dt + quaternion + translation.
    QuatPos(Vec<[f32; 8]>),
    /// Type 6: record 0 holds the translation; the rest dt + quaternion.
    QuatFixedPos(Vec<[f32; 5]>),
}

fn keys<const N: usize>(r: &mut Reader, n: usize) -> Result<Vec<[f32; N]>> {
    let recs = r.records(n, N * 4)?;
    Ok(recs
        .iter()
        .map(|b| std::array::from_fn(|i| f32_at(b, i * 4)))
        .collect())
}

impl Track {
    fn read(r: &mut Reader) -> Result<Self> {
        let (n, kind) = (r.count()?, r.u32()?);
        r.u32()?; // runtime pointer
        Ok(match kind {
            1 => Track::Matrix(keys(r, n)?),
            2 => Track::Affine(keys(r, n)?),
            3 => Track::RotationFixedPos(keys(r, n)?),
            4 => Track::TranslationFixedRot(keys(r, n)?),
            5 => Track::QuatPos(keys(r, n)?),
            6 => Track::QuatFixedPos(keys(r, n)?),
            k => return r.bad(format!("unknown bone track type {k}")),
        })
    }

    /// Per-key times, excluding shared records.
    pub fn dts(&self) -> Vec<f32> {
        fn f<const N: usize>(k: &[[f32; N]], skip: usize) -> Vec<f32> {
            k.iter().skip(skip).map(|k| k[0]).collect()
        }
        match self {
            Track::Matrix(k) => f(k, 0),
            Track::Affine(k) => f(k, 0),
            Track::RotationFixedPos(k) => f(k, 1),
            Track::TranslationFixedRot(k) => f(k, 3),
            Track::QuatPos(k) => f(k, 0),
            Track::QuatFixedPos(k) => f(k, 1),
        }
    }

    pub fn kind(&self) -> u32 {
        match self {
            Track::Matrix(_) => 1,
            Track::Affine(_) => 2,
            Track::RotationFixedPos(_) => 3,
            Track::TranslationFixedRot(_) => 4,
            Track::QuatPos(_) => 5,
            Track::QuatFixedPos(_) => 6,
        }
    }
}

pub fn read_animation(data: &[u8]) -> Result<Animation> {
    let mut r = Reader::new(data);
    r.magic(0x18, "SOA RUN00 v00.02.01")?;
    r.magic(0x24, MATERIAL_MAGIC)?;
    let (unk0, duration) = (r.u32()?, r.f32()?);
    let n = r.count()?;
    r.u32()?; // runtime pointer
    let bones = (0..n).map(|_| Track::read(&mut r)).collect::<Result<_>>()?;
    let morph_header = r.bytes(0x10)?.to_vec();
    let morph_meshes = read_meshes(&mut r)?;
    let mut params = Vec::new();
    for _ in 0..r.count()? {
        let raw = r.bytes(0x5c)?;
        let n = u32_at(raw, 0x54) as usize;
        params.push(ParamTrack {
            raw: raw.to_vec(),
            name: cstr(raw),
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
    let total_bounds = r.bytes(0x28)?.to_vec();
    r.finish()?;
    Ok(Animation {
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
