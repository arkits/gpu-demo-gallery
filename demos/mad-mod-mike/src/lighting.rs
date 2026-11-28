//! Storyboard lighting and bindings from geom.nmb.
use glam::{Mat4, Vec3};

use crate::{demo::Set, model::Model, tree::Node};

#[derive(Clone, Copy)]
struct Point {
    joint: Option<usize>,
    position: Vec3,
    color: Vec3,
}

impl Point {
    fn read(root: &Node, model: &Model, name: &str) -> Self {
        let node = root.nodes("lights").iter().find(|node| node.name == name);
        let v = |field| {
            node.and_then(|node| node.vec::<3>(field))
                .map_or(Vec3::ZERO, Vec3::from)
        };
        Self {
            joint: node.and_then(|node| model.find(node.text("transform"))),
            position: v("position"),
            color: v("color") * node.map_or(0.0, |node| node.float("intensity")),
        }
    }

    fn position(self, pose: &[Mat4]) -> Vec3 {
        self.joint.map_or(self.position, |joint| {
            pose[joint].transform_point3(self.position)
        })
    }
}

pub(crate) struct Rig {
    moon: Point,
    jet: Point,
    bedroom: Point,
    workshop: Point,
    display: Point,
    grinder: Point,
    welder: Point,
}

impl Rig {
    pub(crate) fn read(root: &Node, model: &Model) -> Self {
        let point = |name| Point::read(root, model, name);
        Self {
            moon: point("moonLightShape"),
            jet: point("omniLightShape"),
            bedroom: point("bedroomLampLightShape"),
            workshop: point("shopLampLightShape"),
            display: point("mermaidLightShape"),
            grinder: point("sparkSpawnPointGrinderShape"),
            welder: point("sparkSpawnPointWelderShape"),
        }
    }

    pub(crate) fn frame(
        &self,
        pose: &[Mat4],
        take: usize,
        set: Set,
        seconds: f32,
        fire: f32,
        spark: f32,
    ) -> [[f32; 4]; 16] {
        // Direct/radiance percentages (flags 10/11) of the original cuts.
        // The port keeps the unused scene6 for manual playback.
        let (direct, bounce) = match take {
            0 => (1.20, 0.31),
            1 => (1.35, 1.00),
            3 => (1.30, 0.45),
            4 => (0.85, 0.99),
            5 => (1.26, 0.52),
            6 => (1.07, 0.70),
            7 if seconds < 16.3667 => (1.30, 0.55),
            7 => (0.63, 2.00),
            8 => (1.15, 1.15),
            9 => (0.87, 2.00),
            _ => (1.00, 1.00),
        };
        let lamp = if set == Set::Workshop {
            self.workshop
        } else {
            self.bedroom
        };
        let tool = if take == 6 { self.welder } else { self.grinder };
        let mut out = [[0.0; 4]; 16];
        for (i, (point, gain, radius)) in [
            (
                self.moon,
                if take == 0 && seconds < 9.0 {
                    1.04 * direct
                } else {
                    0.72 * direct
                },
                0.0,
            ),
            (
                lamp,
                if set == Set::Workshop { 2.9 } else { 0.46 } * direct,
                130.0,
            ),
            (self.jet, 2.3 * fire * direct, 92.0),
            (
                self.display,
                if set == Set::NewBedroom {
                    0.64 * direct
                } else {
                    0.0
                },
                145.0,
            ),
            (tool, 0.76 * spark * direct, 92.0),
        ]
        .into_iter()
        .enumerate()
        {
            out[i * 2] = point.color.extend(gain).into();
            out[i * 2 + 1] = point.position(pose).extend(radius).into();
        }
        if set == Set::Workshop {
            out[10] = [0.30, 0.26, 0.19, bounce];
        } else {
            out[10] = [0.075, 0.10, 0.16, bounce];
        }
        out[11] = [fire, spark, 0.0, 0.0];
        // Hallway practical behind the hinged bedroom door.
        let hall = if take == 0 {
            ((seconds - 12.0) / 1.2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out[12] = [1.0, 0.72, 0.26, hall * 2.6];
        out[13] = [136.0, 167.0, 260.0, 70.0];
        out
    }
}
