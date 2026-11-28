//! Camera clearance against the visible bedroom and workshop geometry.
use glam::{Mat4, Vec3};

use crate::model::{Blend, Geometry, Mesh, Model, Pass};

const CLEARANCE: f32 = 4.0;

pub(crate) struct Obstacle {
    shape: usize,
    // For a rigid mesh, the vertex shader applies this one bone to every vertex.
    bone: Option<(usize, Mat4)>,
    min: Vec3,
    max: Vec3,
    skinned: bool,
}

impl Obstacle {
    pub(crate) fn from_model(model: &Model) -> Vec<Self> {
        model
            .shapes
            .iter()
            .enumerate()
            .filter_map(|(index, shape)| {
                if !shape.primary
                    || shape.pass != Pass::Color
                    || shape.blend != Blend::None
                    || !model.ancestors(shape.transform).any(|name| {
                        matches!(
                            name,
                            "bedroom_set"
                                | "bedroom_set_animated"
                                | "workshop_set"
                                | "workshop_set_animated"
                        )
                    })
                {
                    return None;
                }
                let (Geometry::Mesh(mesh) | Geometry::Cage(mesh)) = &shape.geometry else {
                    return None;
                };
                let bone = match shape.bones.as_slice() {
                    [] => None,
                    &[(joint, bend)] => Some((
                        joint,
                        if bend {
                            model.rest()[joint].inverse()
                        } else {
                            Mat4::IDENTITY
                        },
                    )),
                    _ => None,
                };
                let (min, max) = bounds(mesh.moving.iter().map(|v| Vec3::from(v.position)));
                Some(Self {
                    shape: index,
                    bone,
                    min,
                    max,
                    skinned: shape.bones.len() > 1,
                })
            })
            .collect()
    }
}

/// Move the lens toward the anchor when solid geometry crosses its path.
/// Sample the center and corners so a wall can also catch the viewport edges.
pub(crate) fn keep_clear(
    obstacles: &[Obstacle],
    model: &Model,
    visible: &[bool],
    pose: &[Mat4],
    anchor: Vec3,
    camera: Mat4,
    radius: f32,
) -> Mat4 {
    let eye = camera.w_axis.truncate();
    let toward_eye = eye - anchor;
    let distance = toward_eye.length();
    if distance <= CLEARANCE {
        return camera;
    }
    // Check a short distance past the eye to retain clearance as it approaches a surface.
    let travel = toward_eye * ((distance + CLEARANCE) / distance);
    let right = camera.x_axis.truncate().normalize() * radius;
    let up = camera.y_axis.truncate().normalize() * radius;
    let offsets = [
        Vec3::ZERO,
        right,
        -right,
        up,
        -up,
        right + up,
        right - up,
        -right + up,
        -right - up,
    ];
    let mut stop: f32 = 1.0;
    for obstacle in obstacles.iter().filter(|mesh| visible[mesh.shape]) {
        let shape = &model.shapes[obstacle.shape];
        let (Geometry::Mesh(mesh) | Geometry::Cage(mesh)) = &shape.geometry else {
            continue;
        };
        let from_world = obstacle.bone.map_or(Mat4::IDENTITY, |(joint, bind)| {
            (pose[joint] * bind).inverse()
        });
        let skinned = obstacle.skinned.then(|| {
            let bones = model.bones(shape, pose);
            skin_positions(mesh, &bones)
        });
        let (min, max) = skinned
            .as_ref()
            .map_or((obstacle.min, obstacle.max), |p| bounds(p.iter().copied()));
        let delta = from_world.transform_vector3(travel);
        let start = from_world.transform_point3(anchor);
        let position = |i: u32| {
            let i = i as usize;
            skinned
                .as_ref()
                .map_or_else(|| Vec3::from(mesh.moving[i].position), |p| p[i])
        };
        for offset in offsets {
            let origin = start + from_world.transform_vector3(offset);
            if !intersects_bounds(origin, delta * stop, min, max) {
                continue;
            }
            for tri in mesh.indices.chunks_exact(3) {
                let points = std::array::from_fn(|i| position(tri[i]));
                if let Some(hit) = intersect_triangle(origin, delta, points)
                    && hit > 0.0
                    && hit < stop
                {
                    stop = hit;
                }
            }
        }
    }
    if stop == 1.0 {
        return camera;
    }
    let safe_distance = (stop * (distance + CLEARANCE) - CLEARANCE).clamp(0.5, distance);
    let mut place = camera;
    place.w_axis = (anchor + toward_eye * (safe_distance / distance)).extend(1.0);
    place
}

fn bounds(points: impl Iterator<Item = Vec3>) -> (Vec3, Vec3) {
    points.fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), p| (min.min(p), max.max(p)),
    )
}

// Match vs_main for deforming props like the bedroom door frame.
fn skin_positions(mesh: &Mesh, bones: &[Mat4]) -> Vec<Vec3> {
    mesh.moving
        .iter()
        .zip(&mesh.fixed)
        .map(|(vertex, fixed)| {
            let point = Vec3::from(vertex.position);
            let mut total = 0.0;
            let mut result = Vec3::ZERO;
            for (weight, joint) in fixed.weights.into_iter().zip(fixed.bones) {
                if weight != 0.0 {
                    total += weight;
                    result += bones[joint as usize].transform_point3(point) * weight;
                }
            }
            if total < 0.0001 {
                bones[0].transform_point3(point)
            } else {
                result
            }
        })
        .collect()
}

fn intersects_bounds(origin: Vec3, delta: Vec3, min: Vec3, max: Vec3) -> bool {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for axis in 0..3 {
        if delta[axis].abs() < 1e-6 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return false;
            }
        } else {
            let a = (min[axis] - origin[axis]) / delta[axis];
            let b = (max[axis] - origin[axis]) / delta[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    enter <= exit
}

/// Segment parameter at a two-sided triangle, using Möller–Trumbore.
fn intersect_triangle(origin: Vec3, delta: Vec3, [a, b, c]: [Vec3; 3]) -> Option<f32> {
    let edge1 = b - a;
    let edge2 = c - a;
    let cross = delta.cross(edge2);
    let determinant = edge1.dot(cross);
    if determinant.abs() < 1e-6 {
        return None;
    }
    let inv = determinant.recip();
    let from_a = origin - a;
    let u = from_a.dot(cross) * inv;
    let cross = from_a.cross(edge1);
    let v = delta.dot(cross) * inv;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0).then(|| edge2.dot(cross) * inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_segment_bounds_and_both_triangle_faces() {
        let triangle = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let origin = Vec3::new(0.2, 0.2, 2.0);
        let delta = -Vec3::Z * 4.0;
        assert!(intersects_bounds(
            origin,
            delta,
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 0.0)
        ));
        assert_eq!(intersect_triangle(origin, delta, triangle), Some(0.5));
        assert_eq!(
            intersect_triangle(origin + delta, -delta, triangle),
            Some(0.5)
        );
        assert!(!intersects_bounds(
            origin + Vec3::X,
            delta,
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 0.0)
        ));
    }
}
