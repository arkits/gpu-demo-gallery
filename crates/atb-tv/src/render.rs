//! Draws ATB scenes as the ATB renderer (`atr*`, on Glide) did: each
//! material's own Glide state, per-vertex lighting on the CPU, and the
//! texture-coordinate modes of `atrMaterialRealize`. See `docs/ENGINE.md`.
use std::collections::HashMap;

use atb::bof::{Object, TriSet};
use atb::{Bof, Item, Material};
use glam::{Mat3, Mat4, Vec3, Vec4};

use crate::gl::{Depth, Gl, Gpu, Sampling, State, Surface, TextureId, Vertex, WHITE};
use crate::image::Image;

/// The fragment program index of `glide` in `PROGRAMS`.
pub const GLIDE: usize = 2;

/// An ATB camera (`rcamera.c`): its frame in the world, and Glide's
/// projection of horizontal field of view `fov` with x right, y up and z
/// into the screen.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub frame: Mat4,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(frame: Mat4) -> Self {
        // `0x420350`: 60 degrees, near 1, far 65535.
        Self {
            frame,
            fov: std::f32::consts::FRAC_PI_3,
            near: 1.0,
            far: 65_535.0,
        }
    }
    pub fn view(&self) -> Mat4 {
        self.frame.inverse()
    }
    pub fn projection(&self, aspect: f32) -> Mat4 {
        // Square pixels: the vertical view follows the window's shape.
        let fovy = 2.0 * ((self.fov * 0.5).tan() / aspect).atan();
        // x right, y up, z ahead; depth 0 at `near`, 1 at `far`.
        let f = 1.0 / (fovy * 0.5).tan();
        let (n, far) = (self.near, self.far.min(self.near * 1.0e5));
        let r = far / (far - n);
        Mat4::from_cols(
            Vec4::new(f / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, r, 1.0),
            Vec4::new(0.0, 0.0, -r * n, 0.0),
        )
    }
    pub fn eye(&self) -> Vec3 {
        self.frame.w_axis.truncate()
    }
}

/// ATB light kinds (`rlight.c`): 0 ambient, 1 directional, 2 point, 4
/// projector. A light's frame carries its direction (z axis) and position.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub kind: u32,
    pub color: Vec3,
    pub frame: Mat4,
    /// Projector half-angle tangents, x and y.
    pub spread: (f32, f32),
}

impl Light {
    pub fn ambient(color: Vec3) -> Self {
        Self {
            kind: 0,
            color,
            frame: Mat4::IDENTITY,
            spread: (1.0, 1.0),
        }
    }
    /// Shining along `dir`.
    pub fn directional(color: Vec3, dir: Vec3) -> Self {
        let z = dir.normalize_or_zero();
        let x = Vec3::Y.cross(z).try_normalize().unwrap_or(Vec3::X);
        let y = z.cross(x);
        Self {
            kind: 1,
            color,
            frame: Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), Vec4::W),
            spread: (1.0, 1.0),
        }
    }
}

/// Glide's linear fog on eye depth (`grFogTable` from `guFogGenerateLinear`).
#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub near: f32,
    pub far: f32,
    pub color: [f32; 3],
}

/// Everything a frame needs besides the shapes.
pub struct Frame<'a> {
    pub camera: Camera,
    pub lights: &'a [Light],
    pub fog: Option<Fog>,
    /// A picture shown by a texture instead of its own (`atrTextureSet`).
    pub texture_image: &'a dyn Fn(u32) -> Option<u32>,
    /// A material used in place of another, by id; ids above the file's are
    /// materials the demo made in code.
    pub extra: &'a [Material],
    /// A second pass for every shape (Demoroom's projector).
    pub pass: Option<u32>,
    pub wireframe: bool,
    pub textures: bool,
    pub bilinear: bool,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    material: u32,
    image: u32,
    sampling: (bool, bool, bool, bool),
}

/// Pictures made into textures, kept between frames.
/// Light-map places by geometry, strip and triangle: left, top, width,
/// height in the atlas.
type Places = HashMap<(u32, usize, usize), [f32; 4]>;

#[derive(Default)]
pub struct Pictures {
    made: HashMap<Key, TextureId>,
    /// Every triangle's own light map, packed into one picture: its id and
    /// each map's place, by geometry, strip and triangle.
    lightmaps: Option<(TextureId, Places)>,
}

/// The batch key that stands for the light-map atlas.
const LIGHTMAPS: u32 = u32::MAX;

/// Packs the light maps in rows, each with a one-texel border copied from
/// its edge so filtering stays inside it.
fn pack_lightmaps(bof: &Bof) -> (Image, Places) {
    const WIDTH: u32 = 2048;
    let mut maps = Vec::new();
    for (i, o) in bof.objects.iter().enumerate() {
        if let Object::TriSet(t) = o {
            for (s, strip) in t.strips.iter().enumerate() {
                for (k, img) in strip.images.iter().enumerate() {
                    if let Ok(p) = img.rgba() {
                        maps.push(((i as u32 + 1, s, k), p));
                    }
                }
            }
        }
    }
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    let mut places = Vec::new();
    for (_, p) in &maps {
        let (w, h) = (p.width + 2, p.height + 2);
        if x + w > WIDTH {
            x = 0;
            y += row;
            row = 0;
        }
        places.push((x, y));
        x += w;
        row = row.max(h);
    }
    let height = (y + row).max(1).next_power_of_two();
    let mut rgba = vec![0u8; (WIDTH * height * 4) as usize];
    let mut rects = HashMap::new();
    for ((key, p), (px, py)) in maps.iter().zip(places) {
        for ty in 0..p.height + 2 {
            for tx in 0..p.width + 2 {
                let sx = tx.saturating_sub(1).min(p.width - 1);
                let sy = ty.saturating_sub(1).min(p.height - 1);
                let src = ((sy * p.width + sx) * 4) as usize;
                let dst = (((py + ty) * WIDTH + px + tx) * 4) as usize;
                rgba[dst..dst + 4].copy_from_slice(&p.rgba[src..src + 4]);
            }
        }
        let (w, h) = (WIDTH as f32, height as f32);
        rects.insert(
            *key,
            [
                (px + 1) as f32 / w,
                (py + 1) as f32 / h,
                p.width as f32 / w,
                p.height as f32 / h,
            ],
        );
    }
    (
        Image {
            width: WIDTH,
            height,
            rgba,
        },
        rects,
    )
}

impl Pictures {
    fn lightmaps(&mut self, gpu: &Gpu, gl: &mut Gl, bof: &Bof) {
        if self.lightmaps.is_none() {
            let (image, rects) = pack_lightmaps(bof);
            let sampling = Sampling {
                clamp: [true; 2],
                mips: false,
                linear: true,
                anisotropy: 1,
            };
            self.lightmaps = Some((gl.texture(gpu, &image, sampling), rects));
        }
    }

    fn get(
        &mut self,
        gpu: &Gpu,
        gl: &mut Gl,
        bof: &Bof,
        image: u32,
        sampling: Sampling,
    ) -> TextureId {
        let key = Key {
            material: 0,
            image,
            sampling: (
                sampling.clamp[0],
                sampling.clamp[1],
                sampling.mips,
                sampling.linear,
            ),
        };
        if let Some(&t) = self.made.get(&key) {
            return t;
        }
        let t = match bof.image(image).and_then(|i| i.rgba().ok()) {
            Some(p) => gl.texture(
                gpu,
                &Image {
                    width: p.width,
                    height: p.height,
                    rgba: p.rgba,
                },
                sampling,
            ),
            None => WHITE,
        };
        self.made.insert(key, t);
        t
    }
}

fn blend_factor(f: u32, dst: bool) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as B;
    match (f, dst) {
        (0, _) => B::Zero,
        (1, _) => B::SrcAlpha,
        (2, false) => B::Dst,
        (2, true) => B::Src,
        (3, _) => B::DstAlpha,
        (4, _) => B::One,
        (5, _) => B::OneMinusSrcAlpha,
        (6, false) => B::OneMinusDst,
        (6, true) => B::OneMinusSrc,
        (7, _) => B::OneMinusDstAlpha,
        (15, false) => B::SrcAlphaSaturated,
        _ => B::One,
    }
}

fn argb(c: u32) -> [f32; 4] {
    let b = c.to_be_bytes();
    [b[1], b[2], b[3], b[0]].map(|v| f32::from(v) / 255.0)
}

/// What `glide` in `demo.wgsl` reads from the parameters.
fn surface(m: &Material, frame: &Frame, texture: TextureId, projected: bool) -> Surface {
    let f = |v: u32| v as f32;
    let c = m.color_combine();
    let a = m.alpha_combine();
    let mut t = m.tex_combine();
    if !frame.textures || m.tmu_mode()[0] == 0 {
        t = [0; 6];
    }
    let (test, reference) = m.alpha_test();
    let chroma = m.chromakey();
    let fog = frame.fog.filter(|_| m.blend() != (0, 2));
    Surface::textures(&[texture])
        .param(0, [f(c[0]), f(c[1]), f(c[2]), f(c[3])])
        .param(1, [f(c[4]), f(a[0]), f(a[1]), f(a[2])])
        .param(2, [f(a[3]), f(a[4]), f(t[0]), f(t[1])])
        .param(3, [f(t[2]), f(t[3]), f(t[4]), f(t[5])])
        .param(4, argb(m.constant()))
        .param(
            5,
            [
                f(test),
                f32::from(reference) / 255.0,
                if chroma.is_some() { 1.0 } else { 0.0 },
                if projected { 1.0 } else { 0.0 },
            ],
        )
        .param(6, argb(chroma.unwrap_or(0)))
        .param(
            7,
            match fog {
                Some(fog) => [1.0, fog.near, fog.far, 0.0],
                None => [0.0; 4],
            },
        )
        .param(8, {
            let c = fog.map_or([0.0; 3], |f| f.color);
            [c[0], c[1], c[2], 1.0]
        })
}

fn state(m: &Material, frame: &Frame) -> State {
    let (src, dst) = m.blend();
    let mut s = State::new(GLIDE).depth(Depth::LessEqual(m.depth_write()));
    if (src, dst) != (4, 0) {
        s = s.blend(blend_factor(src, false), blend_factor(dst, true));
    }
    if frame.wireframe {
        s = s.lines();
    }
    s
}

/// Per-vertex light, as the `atrLight*` routines compute it.
fn light(m: &Material, frame: &Frame, p: Vec3, n: Vec3) -> Vec3 {
    let mut c = Vec3::from(m.ambient());
    let diffuse = Vec3::from(m.diffuse());
    let specular = Vec3::from(m.specular());
    // The shininess is the word the demos set with the specular color. [I]
    let shine = m.words[10] as f32;
    let eye = (frame.camera.eye() - p).normalize_or_zero();
    let spec = |to: Vec3| -> Vec3 {
        if specular == Vec3::ZERO || n.dot(to) <= 0.0 {
            return Vec3::ZERO;
        }
        let r = n * 2.0 * n.dot(to) - to;
        specular * r.dot(eye).max(0.0).powf(shine.max(1.0))
    };
    for l in frame.lights {
        match l.kind {
            0 => c += l.color * diffuse,
            1 => {
                let to = -l.frame.z_axis.truncate().normalize_or_zero();
                let d = n.dot(to);
                let d = if m.two_sided() { d.abs() } else { d.max(0.0) };
                c += l.color * (diffuse * d + spec(to));
            }
            2 | 3 => {
                let to = (l.frame.w_axis.truncate() - p).normalize_or_zero();
                let d = n.dot(to);
                let d = if m.two_sided() { d.abs() } else { d.max(0.0) };
                c += l.color * (diffuse * d + spec(to));
            }
            _ => {}
        }
    }
    c.clamp(Vec3::ZERO, Vec3::ONE)
}

/// The texture coordinates a material's TMU 0 takes (`texgen`).
fn texcoord(
    mode: u32,
    v: &[f32; 16],
    p: Vec3,
    n: Vec3,
    view: Mat4,
    projector: Option<&Light>,
) -> [f32; 4] {
    match mode {
        0x1000 => [v[6], v[7], 0.0, 1.0],
        0x2000 => [v[12], v[13], 0.0, 1.0],
        0x3000 => [v[14], v[15], 0.0, 1.0],
        0x4000 => {
            // Sphere map from the reflected eye ray (`0x43c650`).
            let e = view.transform_point3(p).normalize_or_zero();
            let n = view.transform_vector3(n).normalize_or_zero();
            let r = e - n * 2.0 * n.dot(e);
            let len = (r.x * r.x + r.y * r.y + (r.z - 1.0) * (r.z - 1.0))
                .sqrt()
                .max(1e-6);
            [0.5 + 0.5 * r.x / len, 0.5 - 0.5 * r.y / len, 0.0, 1.0]
        }
        0x6000 | 0x7000 => {
            let Some(l) = projector else {
                return [0.0, 0.0, 0.0, 1.0];
            };
            let q = l.frame.inverse().transform_point3(p);
            let w = if q.z.abs() < 1e-3 { 1e-3 } else { q.z };
            let s = 0.5 * q.x / l.spread.0 + 0.5 * w;
            let t = -0.5 * q.y / l.spread.1 + 0.5 * w;
            [s, t, 0.0, w]
        }
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// Draws the shape parts, with each material's second pass after it.
pub fn draw(
    gpu: &Gpu,
    gl: &mut Gl,
    pictures: &mut Pictures,
    bof: &Bof,
    items: &[Item],
    frame: &Frame,
    aspect: f32,
) -> usize {
    let view = frame.camera.view();
    let vp = frame.camera.projection(aspect) * view;
    let projector = frame.lights.iter().find(|l| l.kind == 4);
    let needs_maps = items.iter().any(|i| {
        let next = bof.material(i.material).map_or(0, |m| m.next_pass());
        [i.material, next]
            .iter()
            .any(|&id| bof.material(id).is_some_and(|m| m.texgen()[0] == 0x5000))
    });
    if needs_maps {
        pictures.lightmaps(gpu, gl, bof);
    }
    let material = |id: u32| -> Option<&Material> {
        let n = bof.objects.len() as u32;
        if id > n {
            frame.extra.get((id - n - 1) as usize)
        } else {
            bof.material(id)
        }
    };
    // Opaque passes first, in the order met, then blended ones.
    let mut batches: Vec<(u32, u32, Vec<Vertex>)> = Vec::new();
    let mut index: HashMap<(u32, u32), usize> = HashMap::new();
    let mut triangles = 0;
    for item in items {
        let Some(Object::TriSet(set)) = bof.get(item.geometry) else {
            continue;
        };
        let mut passes = vec![item.material];
        if let Some(m) = material(item.material)
            && m.next_pass() != 0
        {
            passes.push(m.next_pass());
        }
        if let Some(p) = frame.pass {
            passes.push(p);
        }
        for pass in passes {
            let Some(m) = material(pass) else { continue };
            let texture = m.textures()[0];
            let image = if m.texgen()[0] == 0x5000 {
                LIGHTMAPS
            } else if m.tmu_mode()[0] == 0 || texture == 0 {
                0
            } else {
                (frame.texture_image)(texture)
                    .or_else(|| match bof.get(texture) {
                        Some(Object::Texture(t)) => t.images.first().copied(),
                        _ => None,
                    })
                    .unwrap_or(0)
            };
            let key = (pass, image);
            let slot = *index.entry(key).or_insert_with(|| {
                batches.push((pass, image, Vec::new()));
                batches.len() - 1
            });
            let maps = pictures.lightmaps.as_ref().map(|(_, r)| (r, item.geometry));
            triangles += emit(
                set,
                m,
                item.matrix,
                vp,
                view,
                frame,
                projector,
                maps,
                &mut batches[slot].2,
            );
        }
    }
    let opaque = |pass: u32| material(pass).is_some_and(|m| m.blend() == (4, 0));
    let order: Vec<usize> = (0..batches.len())
        .filter(|&i| opaque(batches[i].0))
        .chain((0..batches.len()).filter(|&i| !opaque(batches[i].0)))
        .collect();
    for i in order {
        let (pass, image, verts) = &batches[i];
        let m = material(*pass).expect("batched");
        let texture = if *image == LIGHTMAPS {
            pictures.lightmaps.as_ref().map_or(WHITE, |l| l.0)
        } else if *image == 0 || !frame.textures {
            WHITE
        } else {
            let mips = m.mipmap() != 0;
            pictures.get(
                gpu,
                gl,
                bof,
                *image,
                Sampling {
                    clamp: m.clamp(),
                    mips,
                    linear: frame.bilinear && m.bilinear(),
                    anisotropy: 1,
                },
            )
        };
        let projected = matches!(m.texgen()[0], 0x6000 | 0x7000);
        let state = state(m, frame);
        if frame.wireframe {
            let lines: Vec<Vertex> = verts
                .chunks_exact(3)
                .flat_map(|t| [t[0], t[1], t[1], t[2], t[2], t[0]])
                .collect();
            gl.draw(state, &surface(m, frame, texture, projected), &lines);
        } else {
            gl.draw(state, &surface(m, frame, texture, projected), verts);
        }
    }
    triangles
}

#[allow(clippy::too_many_arguments)]
fn emit(
    set: &TriSet,
    m: &Material,
    model: Mat4,
    vp: Mat4,
    view: Mat4,
    frame: &Frame,
    projector: Option<&Light>,
    lightmaps: Option<(&Places, u32)>,
    out: &mut Vec<Vertex>,
) -> usize {
    let normal = Mat3::from_mat4(model).inverse().transpose();
    let mode = m.texgen()[0];
    let mut count = 0;
    for (si, strip) in set.strips.iter().enumerate() {
        let verts: Vec<Vertex> = strip
            .vertices
            .iter()
            .map(|v| {
                let p = model.transform_point3(Vec3::new(v[0], v[1], v[2]));
                let n = (normal * Vec3::new(v[3], v[4], v[5])).normalize_or_zero();
                let rgb = match m.lighting() {
                    1 => light(m, frame, p, n),
                    2 => Vec3::new(v[8], v[9], v[10]).clamp(Vec3::ZERO, Vec3::ONE),
                    _ => Vec3::ONE,
                };
                let alpha = if m.vertex_alpha() { v[11] } else { 1.0 };
                let depth = view.transform_point3(p).z;
                Vertex::new(vp * p.extend(1.0), [rgb.x, rgb.y, rgb.z, alpha])
                    .uvw(0, texcoord(mode, v, p, n, view, projector))
                    .uv(1, depth, 0.0)
            })
            .collect();
        if mode == 0x5000 {
            // Each triangle's own light map, its corners at the material's
            // three fixed places (`0x44ab40`).
            let Some((rects, geometry)) = lightmaps else {
                continue;
            };
            let corner = |k: usize| {
                [
                    f32::from_bits(m.words[19 + 2 * k]),
                    f32::from_bits(m.words[20 + 2 * k]),
                ]
            };
            for (ti, t) in strip.triangles.iter().enumerate() {
                let Some(r) = rects.get(&(geometry, si, ti)) else {
                    continue;
                };
                for (k, &i) in t.iter().enumerate() {
                    let [u, v] = corner(k);
                    out.push(verts[i as usize].uv(0, r[0] + u * r[2], r[1] + v * r[3]));
                }
                count += 1;
            }
            continue;
        }
        for t in &strip.triangles {
            out.extend(t.map(|i| verts[i as usize]));
            count += 1;
        }
    }
    count
}

/// An ATB look-at frame (`0x4238c0` in `race.exe`, `0x4206a0` in
/// `atbtv.exe`): placed at `eye`, its z axis towards `target`, x from
/// `up × z`.
pub fn look_at(target: Vec3, eye: Vec3, up: Vec3) -> Mat4 {
    let z = (target - eye).normalize_or_zero();
    let x = up.cross(z).try_normalize().unwrap_or(Vec3::X);
    let y = z.cross(x);
    Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), eye.extend(1.0))
}
