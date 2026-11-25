//! The bubble (`bubble.exe`; addresses are in it).
use std::collections::BTreeMap;

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{
    CLAMP, Depth, Gl, Gpu, State, Surface, TextureId, Vertex, from_opengl, perspective,
};
use crate::image::Image;

/// +X, -X, +Y, -Y, +Z, -Z.
pub const FILES: &[&str] = &[
    "right.png",
    "left.png",
    "top.png",
    "bottom.png",
    "front.png",
    "back.png",
];
pub const SOUNDS: &[&str] = &[];
pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "cubed"];
pub const BUTTONS: &[(&str, &str)] = &[
    (" ", "Poke"),
    ("c", "Cube map"),
    ("s", "Sphere map"),
    ("w", "Wireframe"),
    ("t", "Pictures"),
    ("2", "Coarse"),
    ("3", "Medium"),
    ("4", "Fine"),
];

const PLAIN: usize = 0;
const TEXTURED: usize = 1;
const CUBED: usize = 2;

const X: f32 = 0.525_731_1;
const Z: f32 = 0.850_650_8;
/// An icosahedron (0x40ebe0, 0x43d3c0).
const CORNERS: [[f32; 3]; 12] = [
    [-X, 0.0, Z],
    [X, 0.0, Z],
    [-X, 0.0, -Z],
    [X, 0.0, -Z],
    [0.0, Z, X],
    [0.0, Z, -X],
    [0.0, -Z, X],
    [0.0, -Z, -X],
    [Z, X, 0.0],
    [-Z, X, 0.0],
    [Z, -X, 0.0],
    [-Z, -X, 0.0],
];
const FACES: [[usize; 3]; 20] = [
    [1, 4, 0],
    [4, 9, 0],
    [4, 5, 9],
    [8, 5, 4],
    [1, 8, 4],
    [1, 10, 8],
    [10, 3, 8],
    [8, 3, 5],
    [3, 2, 5],
    [3, 7, 2],
    [3, 10, 7],
    [10, 6, 7],
    [6, 11, 7],
    [6, 0, 11],
    [6, 1, 0],
    [10, 1, 6],
    [11, 0, 9],
    [2, 11, 9],
    [5, 2, 9],
    [11, 2, 7],
];
/// Corners of the surroundings, and of each side as seen from within: top left, bottom
/// left, bottom right, top right (0x40d7a0, 0x43d330).
const BOX: [[f32; 3]; 8] = [
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
];
const SIDES: [[usize; 4]; 6] = [
    [3, 2, 6, 7],
    [4, 5, 1, 0],
    [4, 0, 3, 7],
    [1, 5, 6, 2],
    [7, 6, 5, 4],
    [0, 1, 2, 3],
];
/// Where `t` shows each side (0x43d108).
const PLACES: [[f32; 2]; 6] = [
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [2.0, 0.0],
    [0.0, 0.0],
];
/// From the eye to the middle of the bubble.
const DISTANCE: f32 = 5.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Show {
    Wires,
    Sphere,
    Cube,
}

#[derive(Clone)]
struct Point {
    rest: Vec3,
    position: Vec3,
    velocity: Vec3,
    /// The sum of the normals of the triangles around, not normalized.
    normal: Vec3,
    triangles: u32,
}

struct Edge {
    ends: [usize; 2],
    length: f32,
}

struct Mesh {
    points: Vec<Point>,
    faces: Vec<[usize; 3]>,
    edges: Vec<Edge>,
    /// What is left of the speed each step; grows to 0.99.
    damping: f32,
}

/// Four triangles for each, `n` times. New corners lie on the sphere (0x410fe0, 0x401350).
fn mesh(n: usize) -> Mesh {
    let mut at: Vec<Vec3> = CORNERS.iter().map(|&c| Vec3::from(c)).collect();
    let mut faces = FACES.to_vec();
    for _ in 0..n {
        let mut middles = BTreeMap::new();
        let mut next = Vec::with_capacity(faces.len() * 4);
        for [a, b, c] in faces {
            let mut middle = |a: usize, b: usize| {
                *middles.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    at.push(((at[a] + at[b]) * 0.5).normalize());
                    at.len() - 1
                })
            };
            let (p, q, r) = (middle(a, b), middle(b, c), middle(c, a));
            next.extend([[p, q, r], [a, p, r], [b, q, p], [c, r, q]]);
        }
        faces = next;
    }
    let mut edges = BTreeMap::new();
    for &[a, b, c] in &faces {
        for (a, b) in [(a, b), (b, c), (c, a)] {
            edges.entry((a.min(b), a.max(b))).or_insert([a, b]);
        }
    }
    let edges = edges.into_values().map(|ends| Edge {
        ends,
        length: at[ends[0]].distance(at[ends[1]]),
    });
    let mut mesh = Mesh {
        edges: edges.collect(),
        faces,
        damping: 0.7,
        points: at
            .iter()
            .map(|&p| Point {
                rest: p,
                position: p,
                velocity: Vec3::ZERO,
                normal: Vec3::ZERO,
                triangles: 0,
            })
            .collect(),
    };
    mesh.normals();
    mesh
}

impl Mesh {
    /// 0x4116a0.
    fn normals(&mut self) {
        for p in &mut self.points {
            (p.normal, p.triangles) = (Vec3::ZERO, 0);
        }
        for f in &self.faces {
            let [a, b, c] = f.map(|i| self.points[i].position);
            let n = (b - a).cross(c - a);
            for &i in f {
                self.points[i].normal += n;
                self.points[i].triangles += 1;
            }
        }
    }

    /// 0x411e20, 0x412170, 0x412240, 0x4120c0.
    fn step(&mut self) {
        // Back to where it was.
        for p in &mut self.points {
            p.velocity += (p.rest - p.position) * 0.005;
        }
        // Along the edges, to their lengths.
        for e in &self.edges {
            let [a, b] = e.ends;
            let d = self.points[b].position - self.points[a].position;
            let pull = (d - d / d.length() * e.length) * 0.6;
            self.points[a].velocity += pull;
            self.points[b].velocity -= pull;
        }
        // Pressure.
        for p in &mut self.points {
            p.velocity += p.normal / p.triangles as f32 * 0.5;
        }
        // Like the neighbors.
        let mut around = vec![(Vec3::ZERO, 0.0f32); self.points.len()];
        for e in &self.edges {
            let [a, b] = e.ends;
            around[a].0 += self.points[b].velocity;
            around[b].0 += self.points[a].velocity;
            around[a].1 += 1.0;
            around[b].1 += 1.0;
        }
        if self.damping < 0.99 {
            self.damping += 0.004;
        }
        for (p, (sum, n)) in self.points.iter_mut().zip(around) {
            p.velocity = p.velocity * 0.9 + sum / n * 0.1;
            p.position += p.velocity;
            p.velocity *= self.damping;
        }
        self.normals();
    }

    /// Pushes in around the point nearest to a line, of those that face along it (0x412550).
    fn poke(&mut self, from: Vec3, along: Vec3) {
        let away = |p: &&Point| {
            let t = (p.position - from).dot(along) / along.length_squared();
            (p.position - from - along * t).length_squared()
        };
        let front = self.points.iter().filter(|p| along.dot(p.normal) < 0.0);
        let Some(at) = front.min_by(|a, b| away(a).total_cmp(&away(b))) else {
            return;
        };
        let at = at.position;
        for p in &mut self.points {
            let near = 10f32.powf(-p.position.distance_squared(at));
            p.velocity -= p.position * near * 0.15;
        }
    }
}

/// Turned, so that the first row is the bottom one.
fn upside_down(i: &Image) -> Image {
    let row = i.width as usize * 4;
    Image {
        rgba: i.rgba.chunks_exact(row).rev().flatten().copied().collect(),
        ..i.clone()
    }
}

fn mirrored(i: &Image) -> Image {
    let row = i.width as usize * 4;
    let turn = |r: &[u8]| {
        r.as_chunks::<4>()
            .0
            .iter()
            .rev()
            .flatten()
            .copied()
            .collect::<Vec<_>>()
    };
    Image {
        rgba: i.rgba.chunks_exact(row).flat_map(turn).collect(),
        ..i.clone()
    }
}

/// What lies in direction `d`, from pictures whose first rows are the bottom ones (0x40e160).
fn look(faces: &[Image; 6], d: Vec3) -> [u8; 4] {
    let a = d.abs();
    let (face, uv) = if a.z >= a.y && a.z >= a.x {
        match d.z > 0.0 {
            true => (4, Vec2::new(-d.x, d.y) / a.z),
            false => (5, Vec2::new(d.x, d.y) / a.z),
        }
    } else if a.x > a.y {
        match d.x > 0.0 {
            true => (0, Vec2::new(d.z, d.y) / a.x),
            false => (1, Vec2::new(-d.z, d.y) / a.x),
        }
    } else {
        match d.y > 0.0 {
            true => (2, Vec2::new(d.x, d.z) / a.y),
            false => (3, Vec2::new(d.x, -d.z) / a.y),
        }
    };
    let image = &faces[face];
    let size = Vec2::new(image.width as f32 - 1.0, image.height as f32 - 1.0);
    let at = (uv + 1.0) * 0.5 * size;
    let (low, part) = (at.floor(), at - at.floor());
    let texel = |x: f32, y: f32| {
        let x = (low.x + x).clamp(0.0, size.x) as usize;
        let y = (low.y + y).clamp(0.0, size.y) as usize;
        let i = (y * image.width as usize + x) * 4;
        std::array::from_fn::<f32, 4, _>(|c| f32::from(image.rgba[i + c]))
    };
    let (a, b, c, d) = (
        texel(0.0, 0.0),
        texel(1.0, 0.0),
        texel(0.0, 1.0),
        texel(1.0, 1.0),
    );
    std::array::from_fn(|i| {
        let low = a[i] * (1.0 - part.x) + b[i] * part.x;
        let high = c[i] * (1.0 - part.x) + d[i] * part.x;
        (low * (1.0 - part.y) + high * part.y + 0.5) as u8
    })
}

/// The surroundings as a mirror ball shows them from far away (0x40de60).
fn ball(faces: &[Image; 6]) -> Image {
    const N: u32 = 256;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for j in 0..N {
        for i in 0..N {
            let at = |k: u32| k as f32 / 128.0 - 1.0 + 1.0 / 512.0;
            let (x, y) = (at(i), at(j));
            let zz = 1.0 - x * x - y * y;
            rgba.extend(match zz >= 0.0 {
                true => {
                    let n = Vec3::new(x, y, zz.sqrt());
                    look(faces, n * 2.0 * n.z - Vec3::Z)
                }
                false => [0, 0, 0, 255],
            });
        }
    }
    Image {
        width: N,
        height: N,
        rgba,
    }
}

pub struct Demo {
    sides: [TextureId; 6],
    cube: TextureId,
    ball: TextureId,
    mesh: Mesh,
    fineness: usize,
    show: Show,
    pictures: bool,
    /// About y and x, and how fast.
    angles: Vec2,
    speed: Vec2,
    /// From the middle of the window, right and up, as parts of half of it.
    mouse: Vec2,
    aspect: f32,
    distance: f32,
    pressed: bool,
    pokes: u32,
    due: f64,
}

impl Demo {
    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let mut loaded = Vec::new();
        for n in FILES {
            loaded.push(upside_down(&files.image(n)?));
        }
        let faces: [Image; 6] = loaded.try_into().map_err(|_| "six sides")?;
        // Sides of a cube map other than top and bottom are seen from outside (0x40d250).
        let cube: [Image; 6] = std::array::from_fn(|i| match i {
            2 | 3 => faces[i].clone(),
            _ => mirrored(&upside_down(&faces[i])),
        });
        Ok(Self {
            sides: std::array::from_fn(|i| gl.texture(gpu, &faces[i], CLAMP)),
            cube: gl.cube(gpu, &cube),
            ball: gl.texture(gpu, &ball(&faces), CLAMP),
            mesh: mesh(4),
            fineness: 4,
            show: Show::Cube,
            pictures: false,
            angles: Vec2::ZERO,
            speed: Vec2::ZERO,
            mouse: Vec2::ZERO,
            aspect: 4.0 / 3.0,
            distance: DISTANCE,
            pressed: false,
            pokes: 0,
            due: 0.0,
        })
    }

    fn view(&self) -> Mat3 {
        Mat3::from_quat(Quat::from_rotation_x(self.angles.y) * Quat::from_rotation_y(self.angles.x))
    }

    /// The original does this once a frame, which is taken as 60 a second (0x406400).
    pub fn step(&mut self, dt: f64) {
        self.due += dt * 60.0;
        while self.due >= 1.0 {
            self.due -= 1.0;
            if self.pictures {
                self.speed -= self.angles * 0.001;
            } else {
                self.speed += self.mouse * 0.001;
                self.speed.y -= self.angles.y * 0.0006;
            }
            self.speed *= 0.94;
            self.angles += self.speed;
            let turn = std::f32::consts::TAU;
            if self.angles.x > turn / 2.0 {
                self.angles.x -= turn;
            } else if self.angles.x < -turn / 2.0 {
                self.angles.x += turn;
            }
            if !self.pictures {
                self.mesh.step();
            }
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        (0..(seconds * 60.0).round() as usize).for_each(|_| self.step(1.0 / 60.0));
    }

    /// 0x4060f0.
    fn poke(&mut self, at: Vec2) {
        let back = self.view().transpose();
        let along = back * Vec3::new(at.x * self.aspect, at.y, -2.0);
        self.mesh.poke(back * (Vec3::Z * self.distance), along);
        self.pokes += 1;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key.to_ascii_lowercase().as_str() {
            " " => self.poke(Vec2::new(0.1, 0.1)),
            "c" => self.show = Show::Cube,
            "s" => self.show = Show::Sphere,
            "w" => self.show = Show::Wires,
            "t" => self.pictures = !self.pictures,
            "2" | "3" | "4" => {
                self.fineness = usize::from(key.as_bytes()[0] - b'0');
                self.mesh = mesh(self.fineness);
            }
            _ => return false,
        }
        true
    }

    /// The farther from the middle, the faster the view turns; the left button pokes.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        self.mouse = Vec2::new(x as f32 * 2.0 - 1.0, 1.0 - y as f32 * 2.0);
        let pressed = buttons & 1 != 0;
        if pressed && !self.pressed && !self.pictures {
            self.poke(self.mouse);
        }
        self.pressed = pressed;
    }

    /// Not in the original.
    pub fn wheel(&mut self, amount: f64) {
        self.distance = (self.distance * (1.0 - amount as f32)).clamp(3.0, 12.0);
    }

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let show = match (self.pictures, self.show) {
            (true, Show::Sphere) => "sphere picture",
            (true, _) => "cube pictures",
            (_, Show::Wires) => "wires",
            (_, Show::Sphere) => "sphere map",
            (_, Show::Cube) => "cube map",
        };
        format!(
            "{show}, {} triangles, {} pokes",
            self.mesh.faces.len(),
            self.pokes
        )
    }

    pub fn draw(&mut self, _: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        self.aspect = w as f32 / h as f32;
        let view = self.view();
        // Half as high as far, from 2 to 40 (0x404fa0).
        let fov = 2.0 * 0.5f32.atan().to_degrees();
        let all = from_opengl(perspective(fov, self.aspect, 0.5, 60.0)) * Mat4::from_mat3(view);
        let white = [1.0; 4];
        const UV: [[f32; 2]; 4] = [[0.001, 0.999], [0.001; 2], [0.999, 0.001], [0.999; 2]];
        for (side, t) in SIDES.iter().zip(self.sides) {
            let v = std::array::from_fn(|i| {
                let p = Vec3::from(BOX[side[i]]) * 20.0;
                Vertex::new(all * p.extend(1.0), white).uv(0, UV[i][0], UV[i][1])
            });
            gl.quad(State::new(TEXTURED), &Surface::textures(&[t]), v);
        }
        // Ahead of the eye, wherever it looks.
        let middle = view.transpose() * Vec3::NEG_Z * self.distance;
        if self.pictures {
            let mut panel = |x: f32, scale: f32, y: f32, t| {
                let v = std::array::from_fn(|i| {
                    let at = [[-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0], [1.0, 1.0]][i];
                    let p = middle + Vec3::new(x + at[0], y + at[1], 0.0) * scale;
                    let uv = [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]][i];
                    Vertex::new(all * p.extend(1.0), white).uv(0, uv[0], uv[1])
                });
                gl.quad(State::new(TEXTURED), &Surface::textures(&[t]), v);
            };
            if self.show == Show::Sphere {
                panel(0.0, 1.3, 0.0, self.ball);
            } else {
                for (place, t) in PLACES.iter().zip(self.sides) {
                    panel(place[0] * 2.1 - 1.05, 0.7, place[1] * 2.1, t);
                }
            }
            return;
        }
        let points: Vec<Vertex> = self
            .mesh
            .points
            .iter()
            .map(|p| {
                let at = p.position + middle;
                let v = Vertex::new(all * at.extend(1.0), white);
                // As OpenGL makes them, in a space that only moves with the eye.
                let (u, n) = (at.normalize(), p.normal.normalize());
                let r = u - n * 2.0 * n.dot(u);
                if self.show == Show::Sphere {
                    let m = 2.0 * (r + Vec3::Z).length();
                    v.uv(0, r.x / m + 0.5, r.y / m + 0.5)
                } else {
                    v.uvw(0, [r.x, r.y, r.z, 0.0])
                }
            })
            .collect();
        if self.show == Show::Wires {
            let lines: Vec<Vertex> = self
                .mesh
                .edges
                .iter()
                .flat_map(|e| e.ends)
                .map(|i| points[i])
                .collect();
            gl.draw(State::new(PLAIN).lines(), &Surface::default(), &lines);
            return;
        }
        let v: Vec<Vertex> = self
            .mesh
            .faces
            .iter()
            .flatten()
            .map(|&i| points[i])
            .collect();
        let (program, surface) = match self.show {
            Show::Sphere => (TEXTURED, Surface::textures(&[self.ball])),
            _ => (CUBED, Surface::default().cube(self.cube)),
        };
        gl.draw(State::new(program).depth(Depth::Less(true)), &surface, &v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshes_are_closed() {
        for n in 0..4 {
            let m = mesh(n);
            assert_eq!(m.faces.len(), 20 << (2 * n));
            assert_eq!(m.points.len() + m.faces.len() - m.edges.len(), 2);
            for p in &m.points {
                assert!(p.normal.dot(p.position) > 0.0);
            }
        }
    }

    #[test]
    fn settles_and_comes_back_from_a_poke() {
        let mut m = mesh(2);
        (0..600).for_each(|_| m.step());
        let still: Vec<Vec3> = m.points.iter().map(|p| p.position).collect();
        m.poke(Vec3::Z * 5.0, Vec3::NEG_Z);
        (0..5).for_each(|_| m.step());
        let far = |m: &Mesh| {
            let d = m
                .points
                .iter()
                .zip(&still)
                .map(|(p, s)| p.position.distance(*s));
            d.fold(0.0, f32::max)
        };
        assert!(far(&m) > 0.1, "{}", far(&m));
        (0..4000).for_each(|_| m.step());
        assert!(far(&m) < 0.05, "{}", far(&m));
    }

    #[test]
    fn looks_at_the_right_side() {
        let faces: [Image; 6] = std::array::from_fn(|i| Image {
            width: 2,
            height: 2,
            rgba: vec![i as u8; 16],
        });
        let d = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ];
        for (i, d) in d.into_iter().enumerate() {
            assert_eq!(look(&faces, d)[0], i as u8);
        }
    }
}
