//! Deterministic playback of the filed rocket and tool particle emitters.
use glam::{Mat4, Vec3};

use crate::{anim::Animation, demo::TAKES, model::Model, tree::Node};

const HZ: f64 = 30.0;
pub(crate) const MAX_PARTICLES: usize = 3072;

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct Particle {
    pub from: [f32; 3],
    pub to: [f32; 3],
    // Width, opacity and age fraction.
    pub style: [f32; 3],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Smoke,
    Jet,
    Grinder,
    Welder,
}

struct Emitter {
    kind: Kind,
    joint: usize,
    rate: f32,
    life: f32,
    life_variance: f32,
    speed_variance: f32,
    spread: f32,
    min: Vec3,
    max: Vec3,
    velocity: Vec3,
    force: Vec3,
    drag: f32,
    streak: f32,
    brightness: f32,
}

#[derive(Default)]
pub(crate) struct Particles {
    pub smoke: Vec<Particle>,
    pub streaks: Vec<Particle>,
}

pub(crate) struct Effects {
    emitters: Vec<Emitter>,
    samples: Vec<Vec<Vec<Mat4>>>,
}

pub(crate) fn jet_active(take: usize, time: f32) -> bool {
    take == 8 || take == 0 && (12.0..19.0).contains(&time)
}

pub(crate) fn tool_active(take: usize, time: f32) -> bool {
    (take == 3 && (1.6..9.5).contains(&time)) || (take == 6 && (0.35..2.6).contains(&time))
}

impl Effects {
    pub(crate) fn new(model: &Model, root: &Node, animations: &[Animation]) -> Self {
        let mut emitters = Vec::new();
        for system in root
            .nodes("geometries")
            .iter()
            .filter(|n| n.kind == "ParticleSystem")
        {
            let kinds: &[Kind] = match system.name.as_str() {
                "jetSmoke" => &[Kind::Smoke],
                "jetParticles1" | "jetParticles2" => &[Kind::Jet],
                "sparkParticles" => &[Kind::Grinder, Kind::Welder],
                _ => continue,
            };
            for &kind in kinds {
                for node in system.nodes("emitters") {
                    let locator = match kind {
                        Kind::Grinder => "sparkSpawnPointGrinder",
                        Kind::Welder => "sparkSpawnPointWelder",
                        _ => node.text("transform"),
                    };
                    let Some(joint) = model.find(locator) else {
                        continue;
                    };
                    let vector = |n: &Node, field| n.vec::<3>(field).map_or(Vec3::ZERO, Vec3::from);
                    emitters.push(Emitter {
                        kind,
                        joint,
                        rate: node.float("rate") / if kind == Kind::Smoke { 4.0 } else { 1.0 },
                        life: node.float("lifespan"),
                        life_variance: node.float("lifespanVariance"),
                        speed_variance: node.float("speedVariance"),
                        spread: node.float("spread"),
                        min: vector(node, "randOffsetMinLS"),
                        max: vector(node, "randOffsetMaxLS"),
                        velocity: vector(node, "direction"),
                        force: vector(system, "force"),
                        drag: system.float("dragMultPerSec").clamp(0.01, 1.0),
                        streak: system.float("streakLength"),
                        brightness: node.float("brightScale"),
                    });
                }
            }
        }
        // The pose history is sampled from the animation so a seek at any time
        // produces the same trails as continuous playback.
        let samples = TAKES
            .iter()
            .zip(animations)
            .enumerate()
            .map(|(i, (take, animation))| {
                if !matches!(i, 0 | 3 | 6 | 8) {
                    return Vec::new();
                }
                let frames = (take.stop * HZ).ceil() as usize + 1;
                (0..frames)
                    .map(|frame| {
                        let time = take.start + frame as f64 / HZ * f64::from(animation.scale);
                        let pose = model.pose(Some((animation, time)), &[]);
                        emitters.iter().map(|e| pose[e.joint]).collect()
                    })
                    .collect()
            })
            .collect();
        Self { emitters, samples }
    }

    pub(crate) fn at(&self, take: usize, time: f32, eye: Vec3) -> Particles {
        let mut out = Particles::default();
        let history = &self.samples[take];
        if history.is_empty() || time < 0.0 {
            return out;
        }
        for (i, e) in self.emitters.iter().enumerate() {
            let active = match e.kind {
                Kind::Smoke | Kind::Jet => jet_active,
                Kind::Grinder if take == 3 => tool_active,
                Kind::Welder if take == 6 => tool_active,
                _ => continue,
            };
            let first = ((time - e.life - e.life_variance).max(0.0) * e.rate).ceil() as u32;
            let last = (time * e.rate) as u32;
            for id in first..=last {
                let birth = id as f32 / e.rate;
                if !active(take, birth) {
                    continue;
                }
                let age = time - birth;
                let rand = |channel| noise(id ^ (i as u32).wrapping_mul(0x517cc1b7) ^ channel);
                let life = (e.life + (rand(0) * 2.0 - 1.0) * e.life_variance).max(0.01);
                if age >= life {
                    continue;
                }
                let frame = birth * HZ as f32;
                let index = (frame as usize).min(history.len() - 1);
                let a = history[index][i];
                let b = history[(index + 1).min(history.len() - 1)][i];
                let mix = |p: Vec3, vector: bool| {
                    if vector {
                        a.transform_vector3(p)
                            .lerp(b.transform_vector3(p), frame.fract())
                    } else {
                        a.transform_point3(p)
                            .lerp(b.transform_point3(p), frame.fract())
                    }
                };
                let offset = e.min + (e.max - e.min) * Vec3::new(rand(3), rand(5), rand(7));
                let origin = mix(offset, false);
                let jitter = Vec3::new(rand(11), rand(13), rand(17)) * 2.0 - Vec3::ONE;
                let speed = 1.0 + (rand(23) * 2.0 - 1.0) * e.speed_variance;
                let velocity = mix(
                    (e.velocity + jitter * e.spread * e.velocity.length()) * speed,
                    true,
                );
                let position = |t: f32| {
                    let distance = if e.drag > 0.999 {
                        t
                    } else {
                        (e.drag.powf(t) - 1.0) / e.drag.ln()
                    };
                    let mut position = origin + velocity * distance + e.force * (0.5 * t * t);
                    if matches!(e.kind, Kind::Grinder | Kind::Welder)
                        && (105.0..168.3).contains(&position.x)
                        && (-26.6..98.9).contains(&position.z)
                        && position.y < 71.71
                    {
                        position.y = 71.71 + (71.71 - position.y) * 0.35;
                    }
                    position
                };
                let fraction = age / life;
                let to = position(age);
                if e.kind == Kind::Smoke {
                    out.smoke.push(Particle {
                        from: to.into(),
                        to: to.into(),
                        style: [3.5 + 18.0 * fraction, (1.0 - fraction) * 0.14, fraction],
                    });
                } else {
                    let from = position((age - e.streak.min(age) * 0.52).max(0.0));
                    out.streaks.push(Particle {
                        from: from.into(),
                        to: to.into(),
                        style: [
                            if e.kind == Kind::Jet { 0.42 } else { 0.29 },
                            e.brightness * (1.0 - fraction) * 0.6,
                            fraction,
                        ],
                    });
                }
            }
        }
        out.smoke.sort_by(|a, b| {
            Vec3::from(b.to)
                .distance_squared(eye)
                .total_cmp(&Vec3::from(a.to).distance_squared(eye))
        });
        out.smoke.truncate(MAX_PARTICLES);
        out.streaks.truncate(MAX_PARTICLES);
        out
    }
}

fn noise(mut i: u32) -> f32 {
    i = (i ^ (i >> 16)).wrapping_mul(0x7feb352d);
    i = (i ^ (i >> 15)).wrapping_mul(0x846ca68b);
    ((i ^ (i >> 16)) >> 8) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, data::Files, demo};

    #[test]
    fn original_emitters_follow_the_effect_cues() -> Result<(), Error> {
        let Some(folder) = std::env::var_os(demo::DATA_ENV) else {
            return Ok(());
        };
        let files = Files::from_dir(std::path::Path::new(&folder))?;
        let world = Model::new(&files.binary(demo::GEOM)?, &files)?;
        let takes = TAKES
            .iter()
            .map(|take| {
                let root = files.binary(take.file)?;
                Animation::read(&root.nodes("animations")[0])
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let effects = Effects::new(&world, &files.scene(demo::PARTICLES)?, &takes);
        assert!(effects.at(0, 2.0, Vec3::ZERO).smoke.is_empty());
        let jet = effects.at(8, 2.0, Vec3::ZERO);
        assert!(!jet.smoke.is_empty() && !jet.streaks.is_empty());
        for take in [3, 6] {
            assert!(effects.at(take, 0.0, Vec3::ZERO).streaks.is_empty());
            assert!(!effects.at(take, 2.0, Vec3::ZERO).streaks.is_empty());
        }
        Ok(())
    }
}
