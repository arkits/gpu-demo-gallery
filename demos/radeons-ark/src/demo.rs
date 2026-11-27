//! Radeon's Ark (ATI, 2000-2002, Direct3D 7, Radeon): a flight through an ark of animal
//! exhibits along `fly.pth` to the music, drawn by the radiATIon engine. The world is one
//! `.ati` file of rooms whose vertices are in buckets by kind and whose triangles are command
//! lists; characters are Proteus objects skinned with up to four matrices.
use glam::{Mat4, Quat, Vec3, Vec4};

use crate::Error;
use crate::ati::{self, Room, World};
use crate::d3d::{self, BufferId, Device, Gpu, Light, Material, Source, TextureId, Vertex};
use crate::data::Files;
use crate::path::Path;
use crate::proteus::{Animation, Object};
use crate::water::Water;

/// The pools' springs step this often (the original steps once a frame).
const WATER_RATE: f64 = 60.0;

pub const FILES: &[&str] = &[];
pub const REQUIRED: &[&str] = &["radiation.ati", "fly.pth", "obj.ini"];
pub const PARTS: &[(&str, usize, usize)] = &[];
/// The music as shipped, and FLAC made from it for the browser version.
pub const SOUNDS: &[&str] = &["music.wav", "music.flac"];
pub const BUTTONS: &[(&str, &str)] = &[(" ", "Pause"), ("r", "Restart")];
pub const SLIDERS: &[(&str, f32, f32)] = &[];

/// gluPerspective as 0x402f90 sets it: 50 degrees down the screen, near 1, far 4000 in the
/// flight (0x43a444).
const FOV: f32 = 50.0;
const NEAR: f32 = 1.0;
const FAR: f32 = 4000.0;
/// The picture is 16:9 inside the window (letterbox, on by default, 0x43a450).
const ASPECT: f32 = 16.0 / 9.0;

/// D3D's FVF bits of the buckets' vertices.
const FVF_C2: u32 = 0xc2;
const FVF_1C2: u32 = 0x1c2;
const FVF_2C2: u32 = 0x2c2;
const FVF_3C2: u32 = 0x3c2;
const FVF_102: u32 = 0x102;
const FVF_52: u32 = 0x52;
const FVF_42: u32 = 0x42;

/// Each bucket's vertex format, or 0 where the engine never draws the bucket in the flight.
fn bucket_fvf(b: usize) -> u32 {
    match b {
        0 => FVF_C2,
        1 | 2 | 16 | 20 | 22 => FVF_1C2,
        3 | 5 | 6 | 21 => FVF_2C2,
        7 => FVF_102,
        8 => FVF_3C2,
        15 => FVF_52,
        25 => FVF_C2,
        26 => FVF_2C2,
        27 => FVF_42,
        _ => 0,
    }
}

fn stride(fvf: u32) -> usize {
    match fvf {
        FVF_C2 | FVF_102 => 20,
        FVF_1C2 | FVF_52 => 28,
        FVF_42 => 16,
        FVF_2C2 => 36,
        FVF_3C2 => 44,
        _ => 0,
    }
}

/// Bytes R, G, B, A in the file are D3D's A8R8G8B8 after the swap at load (0x412eb0).
fn rgba_color(b: &[u8]) -> u32 {
    u32::from(b[3]) << 24 | u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2])
}

fn vertices(bytes: &[u8], fvf: u32) -> Vec<Vertex> {
    let s = stride(fvf);
    let f = |v: &[u8], o: usize| ati::f32_at(v, o);
    bytes
        .chunks_exact(s)
        .map(|v| {
            let mut out = Vertex::at(Vec3::new(f(v, 0), f(v, 4), f(v, 8)));
            match fvf {
                FVF_102 => {
                    out = out.uv(0, f(v, 12), f(v, 16));
                }
                FVF_42 => {
                    out = out.color(rgba_color(&v[12..16]));
                }
                FVF_52 => {
                    out = out
                        .normal(Vec3::new(f(v, 12), f(v, 16), f(v, 20)))
                        .color(rgba_color(&v[24..28]));
                }
                _ => {
                    out = out
                        .color(rgba_color(&v[12..16]))
                        .specular(rgba_color(&v[16..20]));
                    let sets = (s - 20) / 8;
                    for k in 0..sets {
                        out = out.uv(k, f(v, 20 + 8 * k), f(v, 24 + 8 * k));
                    }
                }
            }
            out
        })
        .collect()
}

/// A bucket ready to draw: its vertices on the GPU and its command list.
struct Bucket {
    buffer: BufferId,
    commands: Vec<i16>,
    /// The vertices, kept for mirror surfaces whose alpha changes each frame.
    cpu: Vec<Vertex>,
}

struct RoomData {
    buckets: Vec<Option<Bucket>>,
    tanks: Vec<Tank>,
    waters: Vec<Water>,
}

/// An exhibit tank (a room's 0x120 records, drawn by 0x426490): a window in the room whose
/// contents (its own triangles and the characters that start inside its box) are seen
/// through the window's edges, bent as if by water.
struct Tank {
    bounds: (Vec3, Vec3),
    plane: Vec4,
    corners: [Vec3; 4],
    /// World to the window's frame, and back (D3D's row order, as glam).
    to_local: Mat4,
    to_world: Mat4,
    /// +0x18 and +0x1c: how far the caustics' projector sits from the window, across and up.
    reach: (f32, f32),
    bucket: Option<Bucket>,
}

fn prepare_room(gpu: &Gpu, dev: &mut Device, room: &Room) -> RoomData {
    let buckets = (0..ati::BUCKETS)
        .map(|b| {
            let fvf = bucket_fvf(b);
            let bytes = room.vertices(b);
            if fvf == 0 || bytes.is_empty() {
                return None;
            }
            let v = vertices(bytes, fvf);
            let cpu = if b == 26 { v.clone() } else { Vec::new() };
            Some(Bucket {
                buffer: dev.buffer(gpu, &v),
                commands: room.commands(b),
                cpu,
            })
        })
        .collect();
    let tanks = room
        .array(0x290)
        .chunks_exact(0x120)
        .enumerate()
        .map(|(j, e)| {
            let f = |o: usize| ati::f32_at(e, o);
            let v3 = |o: usize| Vec3::new(f(o), f(o + 4), f(o + 8));
            let sub = |off: usize| {
                room.sub
                    .iter()
                    .find(|s| s.0 == 0x290 && s.1 == j && s.2 == off)
                    .map_or(&[][..], |s| s.3.as_slice())
            };
            let m = |o: usize| d3d::d3d_matrix(std::array::from_fn(|k| f(o + 4 * k)));
            let verts = vertices(sub(0xf0), FVF_1C2);
            let commands: Vec<i16> = sub(0xfc)
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            let bucket = (!verts.is_empty()).then(|| Bucket {
                buffer: dev.buffer(gpu, &verts),
                commands,
                cpu: Vec::new(),
            });
            Tank {
                bounds: (v3(0), v3(0xc)),
                plane: Vec4::new(f(0x20), f(0x24), f(0x28), f(0x30)),
                corners: [v3(0x34), v3(0x40), v3(0x4c), v3(0x58)],
                to_local: m(0x70),
                to_world: m(0xb0),
                reach: (f(0x18), f(0x1c)),
                bucket,
            }
        })
        .collect();
    let waters = room
        .array(0x288)
        .chunks_exact(0x84)
        .enumerate()
        .filter_map(|(j, e)| {
            let sub = |off: usize| {
                room.sub
                    .iter()
                    .find(|s| s.0 == 0x288 && s.1 == j && s.2 == off)
                    .map_or(&[][..], |s| s.3.as_slice())
            };
            Water::new(e, sub(0x68), sub(0x70))
        })
        .collect();
    RoomData {
        buckets,
        tanks,
        waters,
    }
}

/// One character from `obj.ini`.
struct Character {
    object: usize,
    animation: Animation,
    /// Where it was put (0x414f70): a room's tank, else drawn with the rooms.
    tank: Option<(usize, usize)>,
    /// REFLECT in obj.ini: seen in mirrors.
    reflect: bool,
}

/// A Proteus object on the GPU: textures by material.
struct ObjectData {
    object: Object,
    textures: Vec<Option<TextureId>>,
}

pub struct Demo {
    world: World,
    rooms: Vec<RoomData>,
    /// Each mirror's copies of the rooms, already reflected.
    mirrors: Vec<Vec<RoomData>>,
    /// The room the camera is in (world + 8).
    room: usize,
    textures: Vec<TextureId>,
    path: Path,
    objects: Vec<ObjectData>,
    characters: Vec<Character>,
    time: f64,
    /// Water steps taken.
    steps: u64,
    paused: bool,
    still: bool,
    started: bool,
    cues: Vec<String>,
}

/// `obj.ini`: `name = { object, animation, portals, reflect, when }`.
fn object_list(text: &str) -> Vec<(String, String, String, bool, bool, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.split("//").next()?.trim();
            let (name, rest) = l.split_once('=')?;
            let inner = rest.trim().strip_prefix('{')?.strip_suffix('}')?;
            let f: Vec<&str> = inner.split(',').map(str::trim).collect();
            (f.len() == 5).then(|| {
                (
                    name.trim().to_string(),
                    f[0].to_ascii_lowercase(),
                    f[1].to_ascii_lowercase(),
                    f[2].starts_with('M'),
                    f[3] == "REFLECT",
                    f[4].to_string(),
                )
            })
        })
        .collect()
}

impl Demo {
    pub fn new(gpu: &Gpu, dev: &mut Device, files: &Files) -> Result<Self, Error> {
        let world = World::read(files.get("radiation.ati")?)?;
        let textures = world
            .textures
            .iter()
            .map(|t| dev.texture(gpu, &t.image, true))
            .collect();
        let rooms: Vec<RoomData> = world
            .rooms
            .iter()
            .map(|r| prepare_room(gpu, dev, r))
            .collect();
        let mirrors = world
            .mirrors
            .iter()
            .map(|m| m.rooms.iter().map(|r| prepare_room(gpu, dev, r)).collect())
            .collect();
        let path = Path::parse(&String::from_utf8_lossy(files.get("fly.pth")?)).ok_or("fly.pth")?;
        let mut objects: Vec<(String, ObjectData)> = Vec::new();
        let mut characters = Vec::new();
        for (_name, obj, pan, multi, reflect, when) in
            object_list(&String::from_utf8_lossy(files.get("obj.ini")?))
        {
            // The flight shows the characters marked ALWAYS and MAIN (0x414f70).
            if when == "!MAIN" {
                continue;
            }
            let Ok(anim) = files
                .get(&pan)
                .map_err(|e| e.to_string())
                .and_then(|d| Animation::read(d).map_err(|e| e.to_string()))
            else {
                continue;
            };
            let index = match objects.iter().position(|o| o.0 == obj) {
                Some(i) => i,
                None => {
                    let Ok(d) = files.get(&obj) else { continue };
                    let object = Object::read(d)?;
                    let textures = object
                        .materials
                        .iter()
                        .map(|m| m.texture.as_ref().map(|t| dev.texture(gpu, t, true)))
                        .collect();
                    objects.push((obj.clone(), ObjectData { object, textures }));
                    objects.len() - 1
                }
            };
            // The first corner of its box at frame 0 says where it lives: the room holding it,
            // and of that room's tanks the one whose box holds it.
            let o = &objects[index].1.object;
            let corner = (anim.root_at(0.0) * o.bounds.0.extend(1.0)).truncate();
            let inside =
                |lo: Vec3, hi: Vec3| (0..3).all(|k| lo[k] <= corner[k] && corner[k] < hi[k]);
            let mut tank = None;
            let room = world.rooms.iter().position(|r| {
                let (lo, hi) = r.bounds();
                inside(Vec3::from(lo), Vec3::from(hi))
            });
            if let (false, Some(r)) = (multi, room) {
                tank = rooms[r]
                    .tanks
                    .iter()
                    .position(|t| inside(t.bounds.0, t.bounds.1))
                    .map(|t| (r, t));
            }
            characters.push(Character {
                object: index,
                animation: anim,
                tank,
                reflect,
            });
        }
        Ok(Self {
            world,
            rooms,
            mirrors,
            room: 0,
            textures,
            path,
            objects: objects.into_iter().map(|o| o.1).collect(),
            characters,
            time: 0.0,
            steps: 0,
            paused: false,
            still: false,
            started: false,
            cues: Vec::new(),
        })
    }

    pub fn step(&mut self, dt: f64) {
        if !self.started {
            self.started = true;
            self.cues.push("music".into());
        }
        if self.paused || self.still {
            return;
        }
        self.time += dt;
        if self.time >= f64::from(self.path.length()) {
            self.time = 0.0;
            self.cues.push("music".into());
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.time = seconds;
        for k in keys.chars() {
            self.key(&k.to_string(), true);
        }
        self.still = true;
        self.started = true;
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return false;
        }
        match key {
            " " => self.paused = !self.paused,
            "r" => {
                self.time = 0.0;
                self.cues.push("music".into());
            }
            _ => return false,
        }
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}
    pub fn wheel(&mut self, _amount: f64) {}
    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }
    pub fn slider(&self, _i: usize) -> f32 {
        0.0
    }
    pub fn set_slider(&mut self, _i: usize, _v: f32) {}

    pub fn report(&self) -> String {
        let at = self.path.at(self.time as f32).map(|p| p.0);
        format!("time {:.2} at {at:?}", self.time)
    }

    /// Runs a bucket's command list (0x4186d0): 0, 1 and 2 put a texture on a stage, 3 draws
    /// that many indexed triangles, 4 sets the bump matrix, 5 ends.
    fn run(&self, dev: &mut Device, b: &Bucket, fvf: u32) {
        let c = &b.commands;
        let mut i = 0;
        while i < c.len() {
            match c[i] {
                0..=2 => {
                    let stage = c[i] as usize;
                    let t = c.get(i + 1).copied().unwrap_or(-1);
                    if stage < 3 {
                        dev.set_texture(
                            stage,
                            usize::try_from(t)
                                .ok()
                                .and_then(|t| self.textures.get(t))
                                .copied(),
                        );
                    }
                    i += 2;
                }
                3 => {
                    let n = c.get(i + 1).copied().unwrap_or(0).max(0) as usize;
                    let list: Vec<u16> = c
                        .get(i + 2..i + 2 + n)
                        .unwrap_or(&[])
                        .iter()
                        .map(|&x| x as u16)
                        .collect();
                    if !list.is_empty() {
                        dev.draw(
                            d3d::TRIANGLELIST,
                            fvf,
                            Source::Buffer(b.buffer, 0),
                            Some(&list),
                        );
                    }
                    i += 2 + n;
                }
                4 => i += 7,
                5 => break,
                _ => i += 1,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stage(dev: &mut Device, s: usize, op: u32, a1: u32, a2: u32, aop: u32, aa1: u32, aa2: u32) {
        dev.set_texture_stage_state(s, d3d::COLOROP, op);
        dev.set_texture_stage_state(s, d3d::COLORARG1, a1);
        dev.set_texture_stage_state(s, d3d::COLORARG2, a2);
        dev.set_texture_stage_state(s, d3d::ALPHAOP, aop);
        dev.set_texture_stage_state(s, d3d::ALPHAARG1, aa1);
        dev.set_texture_stage_state(s, d3d::ALPHAARG2, aa2);
    }

    fn off(dev: &mut Device, from: usize) {
        for s in from..3 {
            dev.set_texture_stage_state(s, d3d::COLOROP, d3d::DISABLE);
            dev.set_texture_stage_state(s, d3d::ALPHAOP, d3d::DISABLE);
            dev.set_texture(s, None);
        }
    }

    /// The opaque buckets of a room in the flight's state (0x4274c0, 0x428600, 0x423ed0,
    /// 0x424df0): the Radeon has three texture stages, so the three-texture bucket replaces
    /// its two-pass fallbacks.
    fn draw_opaque(&self, dev: &mut Device, r: &RoomData, view: Mat4) {
        use d3d::{
            MODULATE, MODULATE2X, SELECTARG1, SELECTARG2, TA_CURRENT, TA_DIFFUSE, TA_SPECULAR,
            TA_TEXTURE,
        };
        for (b, bucket) in r.buckets.iter().enumerate() {
            let Some(bucket) = bucket else { continue };
            let fvf = bucket_fvf(b);
            Self::off(dev, 0);
            for s in 0..3 {
                dev.set_texture_stage_state(s, d3d::TEXCOORDINDEX, s as u32);
            }
            match b {
                // Untextured: the colour is the vertices' specular (0x4274c0).
                0 => Self::stage(
                    dev,
                    0,
                    SELECTARG2,
                    TA_TEXTURE,
                    TA_SPECULAR,
                    SELECTARG2,
                    TA_TEXTURE,
                    TA_DIFFUSE,
                ),
                1 | 2 => Self::stage(
                    dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
                ),
                3 => {
                    Self::stage(
                        dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, SELECTARG1, TA_TEXTURE,
                        TA_DIFFUSE,
                    );
                    Self::stage(
                        dev, 1, MODULATE, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE, TA_CURRENT,
                    );
                }
                5 => {
                    Self::stage(
                        dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
                    );
                    Self::stage(
                        dev, 1, MODULATE2X, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE,
                        TA_CURRENT,
                    );
                }
                8 => {
                    Self::stage(
                        dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
                    );
                    Self::stage(
                        dev, 1, MODULATE, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE, TA_CURRENT,
                    );
                    Self::stage(
                        dev, 2, MODULATE2X, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE,
                        TA_CURRENT,
                    );
                }
                15 => {
                    // Dual paraboloid environment maps (0x424df0): the reflection vector
                    // through each half's matrix, the back half blended in by its alpha.
                    Self::stage(
                        dev, 0, SELECTARG1, TA_TEXTURE, TA_DIFFUSE, SELECTARG1, TA_TEXTURE,
                        TA_DIFFUSE,
                    );
                    Self::stage(
                        dev, 1, 13, TA_TEXTURE, TA_CURRENT, SELECTARG1, TA_TEXTURE, TA_CURRENT,
                    );
                    for (s, side) in [(0, -1.0), (1, 1.0)] {
                        dev.set_texture_stage_state(s, d3d::TEXCOORDINDEX, 0x30000);
                        dev.set_texture_stage_state(s, d3d::TEXTURETRANSFORMFLAGS, 0x103);
                        dev.set_texture_transform(s, paraboloid(side, view, true));
                    }
                }
                16 => {
                    // Decals added over (0x424df0): ONE, SRCALPHA.
                    dev.set_render_state(d3d::ALPHABLENDENABLE, 1);
                    dev.set_render_state(d3d::SRCBLEND, 2);
                    dev.set_render_state(d3d::DESTBLEND, 5);
                    Self::stage(
                        dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
                    );
                }
                _ => continue,
            }
            self.run(dev, bucket, fvf);
            dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
            for s in 0..2 {
                dev.set_texture_stage_state(s, d3d::TEXTURETRANSFORMFLAGS, 0);
            }
        }
    }

    /// The translucent buckets (0x427b00), drawn last, farthest room first, with
    /// SRCALPHA, INVSRCALPHA and no depth writes.
    fn draw_translucent(&self, dev: &mut Device, r: &RoomData) {
        use d3d::{MODULATE, TA_CURRENT, TA_DIFFUSE, TA_TEXTURE};
        for b in [20, 21, 22] {
            let Some(bucket) = &r.buckets[b] else {
                continue;
            };
            Self::off(dev, 0);
            dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0);
            dev.set_texture_stage_state(1, d3d::TEXCOORDINDEX, 1);
            Self::stage(
                dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
            );
            if b == 21 {
                Self::stage(
                    dev, 1, MODULATE, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE, TA_CURRENT,
                );
            }
            self.run(dev, bucket, bucket_fvf(b));
        }
    }

    /// Mirrors seen from the camera's room (0x415b30 with 0x418e30, 0x419090 and 0x419f50):
    /// those not drawn to textures (+8 zero), facing the eye, in the room's list. Each first
    /// fills its area with its rooms' backdrop (bucket 27, no depth writes), then draws the
    /// rooms it reflects (stored already reflected), framed mirrors (+4 one) cut by the planes
    /// through the eye and their edges, then its surface: Fresnel-weighted over the reflection.
    fn draw_mirrors(&self, dev: &mut Device, eye: Vec3, view: Mat4) {
        use d3d::{MODULATE, SELECTARG2, TA_CURRENT, TA_DIFFUSE, TA_TEXTURE};
        let room = &self.world.rooms[self.room];
        for (j, entry) in room.array(0xb4).chunks_exact(0x14).enumerate() {
            let m = ati::i32_at(entry, 0).max(0) as usize;
            let (Some(rec), Some(mrooms)) = (self.world.mirrors.get(m), self.mirrors.get(m)) else {
                continue;
            };
            let r = &rec.raw;
            let (kind, flag) = (ati::i32_at(r, 4), ati::i32_at(r, 8));
            if kind == 2 || flag != 0 {
                continue;
            }
            let f = |o: usize| ati::f32_at(r, o);
            let n = Vec3::new(f(0x24), f(0x28), f(0x2c));
            if eye.dot(n) + f(0x34) < 0.0 {
                continue;
            }
            let list: Vec<usize> = room
                .sub
                .iter()
                .find(|s| s.0 == 0xb4 && s.1 == j)
                .map(|s| {
                    s.3.chunks_exact(4)
                        .map(|c| ati::i32_at(c, 0).max(0) as usize)
                        .collect()
                })
                .unwrap_or_default();
            let visible: Vec<&RoomData> = list.iter().filter_map(|&i| mrooms.get(i)).collect();
            if visible.is_empty() {
                continue;
            }
            // The backdrop (0x429310).
            dev.set_render_state(d3d::ZWRITEENABLE, 0);
            for mr in &visible {
                if let Some(b) = &mr.buckets[27] {
                    Self::off(dev, 0);
                    Self::stage(
                        dev, 0, SELECTARG2, TA_TEXTURE, TA_DIFFUSE, SELECTARG2, TA_TEXTURE,
                        TA_DIFFUSE,
                    );
                    self.run(dev, b, FVF_42);
                }
            }
            dev.set_render_state(d3d::ZWRITEENABLE, 1);
            if kind == 1 {
                let c = |k: usize| {
                    Vec3::new(f(0x78 + 16 * k), f(0x7c + 16 * k), f(0x80 + 16 * k)) - eye
                };
                for k in 0..4 {
                    let p = c((k + 1) % 4).cross(c(k)).normalize_or_zero();
                    dev.set_clip_plane(k, [p.x, p.y, p.z, -p.dot(eye)]);
                }
                dev.set_render_state(d3d::CLIPPLANEENABLE, 0xf);
            }
            for mr in &visible {
                self.draw_opaque(dev, mr, view);
            }
            dev.set_render_state(d3d::ALPHABLENDENABLE, 1);
            dev.set_render_state(d3d::SRCBLEND, 5);
            dev.set_render_state(d3d::DESTBLEND, 6);
            dev.set_render_state(d3d::ZWRITEENABLE, 0);
            for mr in visible.iter().rev() {
                self.draw_translucent(dev, mr);
            }
            dev.set_render_state(d3d::ZWRITEENABLE, 1);
            dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
            // Characters marked REFLECT, through the mirror's matrix (+0x38, as columns) with
            // the winding turned (0x419090), where their reflection lies behind the plane.
            let refl = Mat4::from_cols_array(&std::array::from_fn(|k| f(0x38 + 4 * k))).transpose();
            let t = self.time as f32;
            dev.set_render_state(d3d::CULLMODE, 3);
            self.draw_some(dev, t, refl, |c| {
                c.reflect && c.tank.is_none() && {
                    let p = (refl * c.animation.root_at(t).w_axis).truncate();
                    p.dot(n) + f(0x34) < 0.0
                }
            });
            dev.set_render_state(d3d::CULLMODE, 2);
            dev.set_render_state(d3d::ALPHABLENDENABLE, 1);
            dev.set_render_state(d3d::CLIPPLANEENABLE, 0);
            // The surface: ONE, SRCALPHA (ONE, ONE where +0x20 is 9999999).
            dev.set_render_state(d3d::SRCBLEND, 2);
            dev.set_render_state(d3d::DESTBLEND, if f(0x20) == 9_999_999.0 { 2 } else { 5 });
            let tint = f(0x18);
            for mr in &visible {
                if let Some(b) = &mr.buckets[25] {
                    Self::off(dev, 0);
                    Self::stage(
                        dev, 0, SELECTARG2, TA_TEXTURE, TA_DIFFUSE, SELECTARG2, TA_TEXTURE,
                        TA_DIFFUSE,
                    );
                    self.run(dev, b, FVF_C2);
                }
                if let Some(b) = &mr.buckets[26] {
                    // Alpha by Fresnel: (1 - v.n)^2 between the tint's alpha and one, colour white.
                    let verts: Vec<Vertex> = b
                        .cpu
                        .iter()
                        .map(|v| {
                            let p = Vec3::new(v.pos[0], v.pos[1], v.pos[2]);
                            let c = (eye - p).normalize_or_zero().dot(n);
                            let a = ((1.0 - c) * (1.0 - c) * (1.0 - tint) + tint) * 255.0;
                            let mut v = *v;
                            v.diffuse = (a.clamp(0.0, 255.0) as u32) << 24 | 0x00ff_ffff;
                            v
                        })
                        .collect();
                    Self::off(dev, 0);
                    dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0);
                    dev.set_texture_stage_state(1, d3d::TEXCOORDINDEX, 1);
                    Self::stage(
                        dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
                    );
                    Self::stage(
                        dev, 1, MODULATE, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE, TA_CURRENT,
                    );
                    self.run_data(dev, b, &verts, FVF_2C2);
                }
            }
            dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
        }
    }

    /// An exhibit tank (0x426490), facing the eye: its contents through the window's four
    /// edges, moved by M70 · L · Mb0, where L, in the window's frame, stretches depth by how
    /// square on the eye looks and shears it sideways by the eye's slant, like water.
    fn draw_tank(&self, dev: &mut Device, t: f32, eye: Vec3, at: (usize, usize), tank: &Tank) {
        if eye.dot(tank.plane.truncate()) + tank.plane.w < 0.0 {
            return;
        }
        for k in 0..4 {
            let (a, b) = (tank.corners[(k + 1) % 4] - eye, tank.corners[k] - eye);
            let p = a.cross(b).normalize_or_zero();
            dev.set_clip_plane(k, [p.x, p.y, p.z, -p.dot(eye)]);
        }
        dev.set_render_state(d3d::CLIPPLANEENABLE, 0xf);
        let dir = tank
            .corners
            .iter()
            .fold(Vec3::ZERO, |s, c| s + (eye - *c).normalize_or_zero() * 0.25)
            .normalize_or_zero();
        // The eye's direction in the window's frame (rows of M70's turn).
        let local = tank.to_local.transform_vector3(dir);
        let sy = dir.dot(tank.plane.truncate());
        let sx = local.x / (local.x * local.x + local.y * local.y).sqrt().max(1e-6) * -0.75;
        let sz = local.z / (local.z * local.z + local.y * local.y).sqrt().max(1e-6) * -0.25;
        // Row 1 of D3D's L is (sx, sy, sz, 0); as glam that is column 1.
        let l = Mat4::from_cols(Vec4::X, Vec4::new(sx, sy, sz, 0.0), Vec4::Z, Vec4::W);
        let world = tank.to_world * l * tank.to_local;
        dev.set_world(world);
        if let Some(b) = &tank.bucket {
            use d3d::{MODULATE, TA_CURRENT, TA_DIFFUSE, TA_TEXTURE};
            Self::off(dev, 0);
            dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0);
            Self::stage(
                dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, MODULATE, TA_TEXTURE, TA_DIFFUSE,
            );
            // Two layers of caustics (texture 4) cast from above the tank through a 25 degree
            // frustum onto the tank's own (unbent) places, scrolling and breathing in time.
            let caustics = self.textures.get(4).copied();
            let mut back = tank.to_local;
            back.w_axis.y += tank.reach.0 * 0.25;
            back.w_axis.z -= tank.reach.1 * 1.4;
            let cot = 1.0 / 25f32.to_radians().tan();
            let proj = Mat4::from_cols(
                Vec4::new(cot, 0.0, 0.0, 0.0),
                Vec4::new(0.0, cot, 0.0, 0.0),
                Vec4::new(0.0, 0.0, -2.0005, -2.0005),
                Vec4::ZERO,
            );
            let undo = world.inverse() * dev.view().inverse();
            let x1 = t * 0.2;
            let f1 = x1 - x1.trunc();
            let mut s1 = Mat4::IDENTITY;
            s1.w_axis.x = f1;
            s1.y_axis.y = (t * 0.5).sin() * 0.5 + 1.5;
            s1.w_axis.y = -f1;
            let x2 = t * 0.3;
            let mut s2 = Mat4::IDENTITY;
            s2.w_axis.x = -(x2 - x2.trunc());
            s2.y_axis.y = -(t.sin() * 0.3 + 1.5);
            for (stage, sm) in [(1, s1), (2, s2)] {
                dev.set_texture(stage, caustics);
                Self::stage(
                    dev, stage, MODULATE, TA_TEXTURE, TA_CURRENT, MODULATE, TA_TEXTURE, TA_CURRENT,
                );
                dev.set_texture_stage_state(stage, d3d::TEXCOORDINDEX, 0x20000);
                dev.set_texture_stage_state(stage, d3d::TEXTURETRANSFORMFLAGS, 0x103);
                dev.set_texture_transform(stage, sm * proj * back * undo);
            }
            self.run_fixed_textures(dev, b, FVF_1C2);
            for stage in 1..3 {
                dev.set_texture_stage_state(stage, d3d::TEXTURETRANSFORMFLAGS, 0);
                dev.set_texture_stage_state(stage, d3d::TEXCOORDINDEX, stage as u32);
            }
            Self::off(dev, 1);
        }
        self.draw_characters(dev, t, Some(at), world);
        dev.set_world(Mat4::IDENTITY);
        dev.set_render_state(d3d::CLIPPLANEENABLE, 0);
    }

    /// As `run`, but leaving stages 1 and 2 as set (the tank's list only names stage 0's).
    fn run_fixed_textures(&self, dev: &mut Device, b: &Bucket, fvf: u32) {
        let c = &b.commands;
        let mut i = 0;
        while i < c.len() {
            match c[i] {
                0 => {
                    let t = c.get(i + 1).copied().unwrap_or(-1);
                    dev.set_texture(
                        0,
                        usize::try_from(t)
                            .ok()
                            .and_then(|t| self.textures.get(t))
                            .copied(),
                    );
                    i += 2;
                }
                1 | 2 => i += 2,
                3 => {
                    let n = c.get(i + 1).copied().unwrap_or(0).max(0) as usize;
                    let list: Vec<u16> = c
                        .get(i + 2..i + 2 + n)
                        .unwrap_or(&[])
                        .iter()
                        .map(|&x| x as u16)
                        .collect();
                    if !list.is_empty() {
                        dev.draw(
                            d3d::TRIANGLELIST,
                            fvf,
                            Source::Buffer(b.buffer, 0),
                            Some(&list),
                        );
                    }
                    i += 2 + n;
                }
                4 => i += 7,
                5 => break,
                _ => i += 1,
            }
        }
    }

    /// A pool (0x425ee0): the environment by its paraboloid (reflection vector, no axis
    /// swap), times the vertices' shade, plus the pool's box picture at the bent ray's hit.
    fn draw_water(&self, dev: &mut Device, w: &Water, view: Mat4) {
        use d3d::{MODULATE, SELECTARG2, TA_CURRENT, TA_DIFFUSE, TA_TEXTURE};
        let tex = |i: i32| {
            usize::try_from(i)
                .ok()
                .and_then(|i| self.textures.get(i))
                .copied()
        };
        let verts: Vec<Vertex> = (0..w.pos.len())
            .map(|k| {
                let s = u32::from(w.shade[k]);
                Vertex::at(w.pos[k])
                    .normal(w.normal[k])
                    .color(s << 24 | s << 16 | s << 8 | s)
                    .uv(0, w.uv[k][0], w.uv[k][1])
            })
            .collect();
        Self::off(dev, 0);
        dev.set_texture(0, tex(w.reflection));
        Self::stage(
            dev, 0, MODULATE, TA_TEXTURE, TA_DIFFUSE, SELECTARG2, TA_TEXTURE, TA_DIFFUSE,
        );
        dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0x30000);
        dev.set_texture_stage_state(0, d3d::TEXTURETRANSFORMFLAGS, 0x103);
        dev.set_texture_transform(0, paraboloid(1.0, view, false));
        dev.set_texture(1, tex(w.pool));
        Self::stage(
            dev,
            1,
            d3d::ADD,
            TA_TEXTURE,
            TA_CURRENT,
            SELECTARG2,
            TA_TEXTURE,
            TA_CURRENT,
        );
        dev.set_texture_stage_state(1, d3d::TEXCOORDINDEX, 0);
        dev.draw(
            d3d::TRIANGLELIST,
            0x152,
            Source::Data(&verts),
            Some(&w.indices),
        );
        dev.set_texture_stage_state(0, d3d::TEXTURETRANSFORMFLAGS, 0);
        dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0);
        dev.set_texture_stage_state(1, d3d::TEXCOORDINDEX, 1);
        Self::off(dev, 0);
    }

    /// As `run`, from vertices made this frame.
    fn run_data(&self, dev: &mut Device, b: &Bucket, verts: &[Vertex], fvf: u32) {
        let c = &b.commands;
        let mut i = 0;
        while i < c.len() {
            match c[i] {
                0..=2 => {
                    let stage = c[i] as usize;
                    let t = c.get(i + 1).copied().unwrap_or(-1);
                    dev.set_texture(
                        stage,
                        usize::try_from(t)
                            .ok()
                            .and_then(|t| self.textures.get(t))
                            .copied(),
                    );
                    i += 2;
                }
                3 => {
                    let n = c.get(i + 1).copied().unwrap_or(0).max(0) as usize;
                    let list: Vec<u16> = c
                        .get(i + 2..i + 2 + n)
                        .unwrap_or(&[])
                        .iter()
                        .map(|&x| x as u16)
                        .collect();
                    if !list.is_empty() {
                        dev.draw(d3d::TRIANGLELIST, fvf, Source::Data(verts), Some(&list));
                    }
                    i += 2 + n;
                }
                4 => i += 7,
                5 => break,
                _ => i += 1,
            }
        }
    }

    fn draw_characters(&self, dev: &mut Device, t: f32, tank: Option<(usize, usize)>, world: Mat4) {
        self.draw_some(dev, t, world, |c| c.tank == tank);
    }

    fn draw_some(&self, dev: &mut Device, t: f32, world: Mat4, keep: impl Fn(&Character) -> bool) {
        // Two white directional lights (0x40aaf0); the first gives a fifth of ambient.
        let light = |dir: Vec3, ambient: f32| Light {
            kind: d3d::LIGHT_DIRECTIONAL,
            diffuse: Vec4::ONE,
            specular: Vec4::ONE,
            ambient: Vec4::new(ambient, ambient, ambient, 1.0),
            direction: dir,
            ..Default::default()
        };
        dev.set_light(0, light(Vec3::new(1.0, -1.0, -1.0), 0.2));
        dev.set_light(1, light(Vec3::new(-1.0, 1.0, 0.0), 0.0));
        dev.light_enable(0, true);
        dev.light_enable(1, true);
        dev.set_render_state(d3d::LIGHTING, 1);
        dev.set_render_state(d3d::COLORVERTEX, 0);
        dev.set_world(Mat4::IDENTITY);
        for ch in self.characters.iter().filter(|c| keep(c)) {
            let o = &self.objects[ch.object];
            let root = ch.animation.root_at(t);
            let bones: Vec<Mat4> = ch
                .animation
                .bones_at(t, &o.object)
                .into_iter()
                .map(|b| world * root * b)
                .collect();
            for mesh in &o.object.meshes {
                let m = o.object.materials.get(mesh.material);
                let tex = o.textures.get(mesh.material).copied().flatten();
                // 0x40aa50: textured materials are a light grey; others their own colours.
                let material = match (m, tex) {
                    (Some(m), None) => Material {
                        ambient: Vec4::new(m.colors[0][0], m.colors[0][1], m.colors[0][2], 1.0),
                        diffuse: Vec4::new(m.colors[1][0], m.colors[1][1], m.colors[1][2], 1.0),
                        specular: Vec4::new(m.colors[2][0], m.colors[2][1], m.colors[2][2], 1.0),
                        emissive: Vec4::ZERO,
                        power: 64.0,
                    },
                    _ => Material {
                        ambient: Vec4::new(0.8, 0.8, 0.8, 1.0),
                        diffuse: Vec4::new(0.8, 0.8, 0.8, 1.0),
                        specular: Vec4::ONE,
                        emissive: Vec4::ZERO,
                        power: 64.0,
                    },
                };
                dev.set_material(material);
                Self::off(dev, 0);
                dev.set_texture_stage_state(0, d3d::TEXCOORDINDEX, 0);
                if let Some(t) = tex {
                    dev.set_texture(0, Some(t));
                    Self::stage(
                        dev,
                        0,
                        d3d::MODULATE,
                        d3d::TA_TEXTURE,
                        d3d::TA_DIFFUSE,
                        d3d::SELECTARG1,
                        d3d::TA_TEXTURE,
                        d3d::TA_DIFFUSE,
                    );
                } else {
                    Self::stage(
                        dev,
                        0,
                        d3d::SELECTARG2,
                        d3d::TA_TEXTURE,
                        d3d::TA_DIFFUSE,
                        d3d::SELECTARG2,
                        d3d::TA_TEXTURE,
                        d3d::TA_DIFFUSE,
                    );
                }
                // Blended on the CPU exactly as the Radeon blends: the last weight is one less
                // the others.
                let verts: Vec<Vertex> = mesh
                    .vertices
                    .iter()
                    .map(|v| {
                        let p = Vec3::new(v[0], v[1], v[2]).extend(1.0);
                        let n = Vec3::new(v[7], v[8], v[9]).extend(0.0);
                        let k = mesh.bones.len().clamp(1, 4);
                        let mut w = [v[3], v[4], v[5], 0.0];
                        let sum: f32 = w[..k - 1].iter().sum();
                        w[k - 1] = 1.0 - sum;
                        let (mut pp, mut nn) = (Vec4::ZERO, Vec4::ZERO);
                        #[allow(clippy::needless_range_loop)]
                        for j in 0..k {
                            let m = mesh
                                .bones
                                .get(j)
                                .and_then(|&b| bones.get(b))
                                .copied()
                                .unwrap_or(Mat4::IDENTITY);
                            pp += m * p * w[j];
                            nn += m * n * w[j];
                        }
                        Vertex::at(pp.truncate())
                            .normal(nn.truncate().normalize_or_zero())
                            .uv(0, v[10], v[11])
                    })
                    .collect();
                let idx: Vec<u16> = mesh.triangles.iter().flatten().map(|&i| i as u16).collect();
                dev.draw(
                    d3d::TRIANGLELIST,
                    d3d::FVF_XYZ | d3d::FVF_NORMAL | 0x100,
                    Source::Data(&verts),
                    Some(&idx),
                );
            }
        }
        dev.set_render_state(d3d::LIGHTING, 0);
        dev.light_enable(0, false);
        dev.light_enable(1, false);
    }

    pub fn draw(&mut self, _gpu: &Gpu, dev: &mut Device) {
        let (w, h) = dev.size();
        let (w, h) = (w as f32, h as f32);
        // Letterboxed to 16:9 (0x402ec0).
        let vh = (w / ASPECT).min(h);
        let vw = vh * ASPECT;
        dev.clear(7, 0, 1.0, 0);
        dev.set_viewport([(w - vw) / 2.0, (h - vh) / 2.0, vw, vh, 0.0, 1.0]);
        let t = self.time as f32;
        let (eye, q) = self.path.at(t).unwrap_or((Vec3::ZERO, Quat::IDENTITY));
        let view = Mat4::from_quat(q) * Mat4::from_translation(-eye);
        let cot = 1.0 / (FOV.to_radians() / 2.0).tan();
        let proj = d3d::d3d_matrix([
            cot / ASPECT,
            0.0,
            0.0,
            0.0,
            0.0,
            cot,
            0.0,
            0.0,
            0.0,
            0.0,
            (NEAR + FAR) / (NEAR - FAR),
            -1.0,
            0.0,
            0.0,
            2.0 * NEAR * FAR / (NEAR - FAR),
            0.0,
        ]);
        dev.set_view(view);
        dev.set_projection(proj);
        dev.set_world(Mat4::IDENTITY);
        dev.set_render_state(d3d::ZENABLE, 1);
        dev.set_render_state(d3d::ZWRITEENABLE, 1);
        dev.set_render_state(d3d::LIGHTING, 0);
        dev.set_render_state(d3d::CULLMODE, 2);
        // The pools under the eye ripple on (0x425430 each frame they are seen).
        let want = (self.time * WATER_RATE) as u64;
        if want < self.steps {
            for r in &mut self.rooms {
                for w in &mut r.waters {
                    w.reset();
                }
            }
            self.steps = 0;
        }
        let todo = want - self.steps;
        for r in &mut self.rooms {
            for w in &mut r.waters {
                if eye.z >= w.level() {
                    for _ in 0..todo.min(20_000) {
                        w.step();
                    }
                }
            }
        }
        self.steps = want;
        // The camera's room: the first whose box holds the eye (0x415b30).
        if let Some(i) = self.world.rooms.iter().position(|r| {
            let (lo, hi) = r.bounds();
            (0..3).all(|k| lo[k] <= eye[k] && eye[k] < hi[k])
        }) {
            self.room = i;
        }
        self.draw_mirrors(dev, eye, view);
        for r in &self.rooms {
            self.draw_opaque(dev, r, view);
        }
        let mut waters: Vec<(usize, usize)> = Vec::new();
        for (ri, r) in self.rooms.iter().enumerate() {
            for (wi, w) in r.waters.iter().enumerate() {
                if eye.z >= w.level() {
                    waters.push((ri, wi));
                }
            }
        }
        for (ri, wi) in waters {
            self.rooms[ri].waters[wi].shade_for(eye);
            self.draw_water(dev, &self.rooms[ri].waters[wi], view);
        }
        self.draw_characters(dev, t, None, Mat4::IDENTITY);
        for (ri, r) in self.rooms.iter().enumerate() {
            for (ti, tank) in r.tanks.iter().enumerate() {
                self.draw_tank(dev, t, eye, (ri, ti), tank);
            }
        }
        dev.set_render_state(d3d::ALPHABLENDENABLE, 1);
        dev.set_render_state(d3d::SRCBLEND, 5);
        dev.set_render_state(d3d::DESTBLEND, 6);
        dev.set_render_state(d3d::ZWRITEENABLE, 0);
        for r in self.rooms.iter().rev() {
            self.draw_translucent(dev, r);
        }
        dev.set_render_state(d3d::ZWRITEENABLE, 1);
        dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
        Self::off(dev, 0);
        // The flight fades in over its first second and out over its last (0x415b30): the
        // picture times black's alpha, blended ZERO, SRCALPHA.
        let left = self.path.length() - t;
        let k = t.min(left).clamp(0.0, 1.0);
        if k < 1.0 {
            let a = ((k * 255.0) as u32) << 24;
            let (x0, y0) = ((w - vw) / 2.0, (h - vh) / 2.0);
            let quad = [
                Vertex::screen(x0, y0, 0.0, 1.0).color(a),
                Vertex::screen(x0 + vw, y0, 0.0, 1.0).color(a),
                Vertex::screen(x0, y0 + vh, 0.0, 1.0).color(a),
                Vertex::screen(x0 + vw, y0 + vh, 0.0, 1.0).color(a),
            ];
            dev.set_render_state(d3d::ZENABLE, 0);
            dev.set_render_state(d3d::ALPHABLENDENABLE, 1);
            dev.set_render_state(d3d::SRCBLEND, 1);
            dev.set_render_state(d3d::DESTBLEND, 5);
            Self::stage(
                dev,
                0,
                d3d::SELECTARG2,
                d3d::TA_TEXTURE,
                d3d::TA_DIFFUSE,
                d3d::SELECTARG2,
                d3d::TA_TEXTURE,
                d3d::TA_DIFFUSE,
            );
            dev.draw(
                d3d::TRIANGLESTRIP,
                d3d::FVF_XYZRHW | d3d::FVF_DIFFUSE,
                Source::Data(&quad),
                None,
            );
            dev.set_render_state(d3d::ALPHABLENDENABLE, 0);
            dev.set_render_state(d3d::ZENABLE, 1);
        }
    }
}

/// The texture matrix of one half of the dual paraboloid map (0x4249e0 with the inverse of
/// the view's turn), in D3D's order: world reflection, y and z swapped, flipped and moved by
/// `side`, z made the divisor, scaled and biased into the texture, signed by `side`; its
/// third column is then its fourth, the divisor.
fn paraboloid(side: f32, view: Mat4, swap_axes: bool) -> Mat4 {
    let m = |a: [f32; 16]| d3d::d3d_matrix(a);
    let a1 = m([
        side, 0.0, 0.0, 0.0, 0.0, side, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, side,
    ]);
    let sb = m([
        0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, 0.0, 1.0,
    ]);
    let p = m([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0,
    ]);
    let flip = m([
        -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, side, 1.0,
    ]);
    let swap = m([
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    // The turn of the view, undone (camera space back to the world's).
    let turn = Mat4::from_mat3(glam::Mat3::from_mat4(view)).transpose();
    // D3D's product A·B is glam's B·A.
    let swap = if swap_axes { swap } else { Mat4::IDENTITY };
    let mut out = a1 * sb * p * flip * swap * turn;
    // Third output (row 2 in glam's order) becomes the fourth.
    let r3 = out.row(3);
    out = Mat4::from_cols(
        Vec4::new(out.x_axis.x, out.x_axis.y, r3.x, out.x_axis.w),
        Vec4::new(out.y_axis.x, out.y_axis.y, r3.y, out.y_axis.w),
        Vec4::new(out.z_axis.x, out.z_axis.y, r3.z, out.z_axis.w),
        Vec4::new(out.w_axis.x, out.w_axis.y, r3.w, out.w_axis.w),
    );
    out
}
