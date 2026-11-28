//! Valley of Ra: Anubis and Nefertiti fight before a pyramid. `fight.exe`
//! loads the nine scene parts of `fight.bof` (`0x4018a0`), plays each
//! fighter's moves as frame ranges of one long animation (`0x401ff0`),
//! keeps the camera on the two (`0x401a70`, `0x401bb0`) and draws shadows,
//! reflection, fighters, backdrop and scene (`0x401410`). See
//! `docs/ENGINE.md`.
use atb::bof::Object;
use atb::{Bof, Item, View};
use glam::{Mat4, Vec3};

use crate::Error;
use crate::data::Files;
use crate::gl::{Gl, Gpu};
use crate::pe::Pe;
use crate::render::{self, Camera, Fog, Frame, Light, Pictures, look_at};

pub const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &["plain", "textured", "glide"];
pub const BUTTONS: &[(&str, &str)] = &[
    ("ArrowUp", "Anubis jump"),
    ("8", "Anubis punch"),
    ("9", "Anubis high kick"),
    ("6", "Anubis low kick"),
    ("7", "Anubis block"),
    ("w", "Nefertiti jump"),
    ("i", "Nefertiti punch"),
    ("o", "Nefertiti high kick"),
    ("l", "Nefertiti low kick"),
    ("u", "Nefertiti block"),
    ("f", "Fog"),
    ("r", "Restart"),
];

/// `fight.exe` is read for the two move tables; nothing else of it.
pub const FILES: &[&str] = &["fight.bof", "fight.exe"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const RATE: f64 = 60.0;

/// Input slots `0x401ff0` reads, as `0x402970` fills them from key.dat's
/// GCI bits: 0 up (jump), 1 down, 2 right, 3 left, 4 a reset on letting go
/// of R, 6 A (block), 8 B (punch), 10 C (high kick), 11 D (low kick).
#[derive(Clone, Copy, Default)]
struct Pad([bool; 12]);

struct Fighter {
    anim: u32,
    shadow: u32,
    /// Children of the animation, and the one whose x is the fighter's step.
    count: usize,
    horizontal: usize,
    /// The upper body the camera follows (`0x4019c0`).
    body: usize,
    /// Twelve moves of (first, contact, last) frames (`0x45c1c8`, `0x45c268`).
    moves: Vec<[u32; 3]>,
    state: usize,
    frame: u32,
    x: f32,
    /// Bounding-sphere radius of each animation child (`0x401000`).
    radius: Vec<f32>,
    /// Starts at 1; each hit taken costs 0.1 (`0x402650`). Nothing shows it.
    health: f32,
    start: f32,
    pad: Pad,
    last: Pad,
    /// Pressed since the last tick, and of those, already let go: a tap
    /// shorter than a tick (an on-screen button) still counts once.
    unseen: Pad,
    gone: Pad,
}

impl Fighter {
    fn new(
        bof: &Bof,
        pe: &Pe,
        anim: u32,
        shadow: u32,
        who: char,
        table: u32,
        x: f32,
    ) -> Result<Self, Error> {
        let Some(Object::Anim(a)) = bof.get(anim) else {
            return Err("fighter is not an animation".into());
        };
        let child = |name: String| {
            a.names
                .iter()
                .position(|n| n.as_deref() == Some(name.as_str()))
                .ok_or_else(|| format!("fight.bof has no {name}"))
        };
        let horizontal = child(format!("{who}-HORZNTL"))?;
        let body = child(format!("{who}-Body-T"))?;
        let moves = (0..12)
            .map(|i| {
                let at = table + i * 12;
                Ok([pe.word(at)?, pe.word(at + 4)?, pe.word(at + 8)?])
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if moves
            .iter()
            .any(|m| m[0] > m[2] || u32::from(a.fields.1) <= m[2])
        {
            return Err("fight.exe move table out of range".into());
        }
        let radius = a
            .group
            .children
            .iter()
            .map(|&c| {
                bof.get(c)
                    .and_then(Object::node)
                    .map_or(0.0, |n| n.sphere[3])
            })
            .collect();
        Ok(Self {
            anim,
            shadow,
            radius,
            health: 1.0,
            start: x,
            count: a.group.children.len(),
            horizontal,
            body,
            frame: moves[0][0],
            moves,
            state: 0,
            x,
            pad: Pad::default(),
            last: Pad::default(),
            unseen: Pad::default(),
            gone: Pad::default(),
        })
    }

    /// The step child's position at a frame.
    fn step(&self, bof: &Bof, frame: u32) -> Vec3 {
        self.child(bof, frame, self.horizontal)
    }

    fn child(&self, bof: &Bof, frame: u32, child: usize) -> Vec3 {
        let Some(Object::Anim(a)) = bof.get(self.anim) else {
            return Vec3::ZERO;
        };
        let Some(Object::Raw(raw)) = bof.get(a.fields.2) else {
            return Vec3::ZERO;
        };
        let at = (frame as usize * self.count + child) * 64 + 48;
        let f = |o: usize| {
            raw.get(at + o..at + o + 4)
                .map_or(0.0, |b| f32::from_be_bytes(b.try_into().unwrap()))
        };
        Vec3::new(f(0), f(4), f(8))
    }

    /// Back to the opening stance (`0x402700`, `0x402730`).
    fn reset(&mut self) {
        self.state = 0;
        self.frame = self.moves[0][0];
        self.x = self.start;
        self.health = 1.0;
    }

    /// What a move is doing at this frame (`0x4025a0`): 0 striking (an
    /// attack from its contact frame on), 1 reeling from a hit, 2 neither,
    /// 3 blocking.
    fn kind(&self) -> u8 {
        match self.state {
            1 | 2 | 6 | 9 if self.moves[self.state][1] <= self.frame => 0,
            3 | 4 | 5 | 10 => 1,
            11 => 3,
            _ => 2,
        }
    }

    /// Reels from the other's attack (`0x402630`, `0x402650`): hit by the
    /// low kick (1), high kick (2) or punch.
    fn hit_by(&mut self, attack: usize) {
        self.state = match attack {
            1 => 3,
            2 => 4,
            _ => 5,
        };
        self.frame = self.moves[self.state][0];
        self.health -= 0.1;
    }

    /// One frame of the move machine (`0x401ff0`). Moves 1, 2 and 6 start
    /// on a fresh press; 7 walks, 8 jumps, 11 holds a block; 3 to 5 are hit
    /// reactions that play through.
    fn tick(&mut self, bof: &Bof) {
        let p = self.pad.0;
        let fresh = |i: usize| p[i] && !self.last.0[i];
        let m = self.moves[self.state];
        let travel = |s: &Self, from: u32, to: u32| s.step(bof, to).x - s.step(bof, from).x;
        let mut go = None;
        match self.state {
            7 if p[0] => go = Some(8),
            0 | 7 if fresh(11) => go = Some(1),
            0 | 7 if fresh(10) => go = Some(2),
            0 | 7 if fresh(8) => go = Some(6),
            0 if p[2] || p[3] => go = Some(7),
            0 if p[6] => go = Some(11),
            0 if p[0] => go = Some(8),
            0 => {
                self.frame += 1;
                if self.frame > m[2] {
                    self.frame = m[0];
                }
            }
            1..=6 => {
                self.frame += 1;
                if self.frame > m[2] {
                    go = Some(0);
                }
            }
            7 if p[2] => {
                // The step's travel is banked when the walk cycle wraps.
                self.frame += 1;
                if self.frame > m[2] {
                    self.x += travel(self, m[0], m[2]);
                    self.frame = m[0];
                }
            }
            7 if p[3] => {
                if self.frame <= m[0] {
                    self.x += travel(self, m[2], m[0]);
                    self.frame = m[2];
                } else {
                    self.frame -= 1;
                }
            }
            7 => {
                self.x += travel(self, m[0], self.frame);
                go = Some(0);
            }
            8 => {
                self.frame += 1;
                if self.frame > m[2] {
                    self.x += travel(self, m[0], m[2]);
                    go = Some(0);
                }
            }
            11 => {
                if p[6] {
                    if self.frame < m[2] {
                        self.frame += 1;
                    }
                } else if self.frame > m[0] {
                    self.frame -= 1;
                } else {
                    go = Some(0);
                }
            }
            _ => go = Some(0),
        }
        if let Some(state) = go {
            self.state = state;
            self.frame = self.moves[state][0];
        }
        self.x = self.x.clamp(-130.0, 100.0);
        self.last = self.pad;
        for i in 0..self.pad.0.len() {
            if self.gone.0[i] {
                self.pad.0[i] = false;
            }
        }
        (self.unseen, self.gone) = Default::default();
    }

    fn press(&mut self, slot: usize, down: bool) {
        if down {
            self.pad.0[slot] = true;
            self.unseen.0[slot] = true;
            self.gone.0[slot] = false;
        } else if self.unseen.0[slot] {
            self.gone.0[slot] = true;
        } else {
            self.pad.0[slot] = false;
        }
    }

    /// Where the upper body is.
    fn world(&self, bof: &Bof) -> Vec3 {
        let s = self.child(bof, self.frame, self.body);
        Vec3::new(self.x + s.x, s.y, s.z)
    }

    /// Whether any part's bounding sphere meets one of the other's
    /// (`0x401000`).
    fn touches(&self, other: &Self, bof: &Bof) -> bool {
        let at = |f: &Self, i: usize| f.child(bof, f.frame, i) + Vec3::X * f.x;
        (0..self.count).any(|i| {
            let p = at(self, i);
            (0..other.count).any(|j| {
                let reach = self.radius[i] + other.radius[j];
                p.distance_squared(at(other, j)) < reach * reach
            })
        })
    }
}

/// When the two touch (`0x4023f0`, called with Nefertiti first): a strike
/// on a fighter who is neither striking, reeling nor blocking makes them
/// reel, and they are pushed a unit apart, or only one of them moves.
fn clash(b: &mut Fighter, a: &mut Fighter) {
    let (kb, ka) = (b.kind(), a.kind());
    let (sb, sa) = (b.state, a.state);
    let (move_b, move_a) = match (kb, ka) {
        (0, 0) => {
            b.hit_by(sa);
            a.hit_by(sb);
            (true, true)
        }
        (0, 2) => {
            a.hit_by(sb);
            (false, true)
        }
        (0, _) => (false, true),
        (2, 0) => {
            b.hit_by(sa);
            (true, false)
        }
        (_, 0) => (true, false),
        _ => (true, true),
    };
    if !move_b {
        b.x -= 1.0;
    } else if move_a {
        b.x += 1.0;
        a.x -= 1.0;
    } else {
        b.x += 1.0;
    }
}

/// A critically damped follower: `0x401a70` and `0x401bb0` keep a velocity,
/// pull it toward the goal and damp it each frame.
#[derive(Clone, Copy, Default)]
struct Spring {
    at: Vec3,
    speed: Vec3,
}

impl Spring {
    fn pull(&mut self, goal: Vec3, k: f32, damp: f32) {
        self.speed = (self.speed + (goal - self.at) * k) * damp;
        self.at += self.speed;
    }
}

pub struct Demo {
    bof: Bof,
    parts: [u32; 4],
    fighters: [Fighter; 2],
    pictures: Pictures,
    target: Spring,
    eye: Spring,
    fog: bool,
    wireframe: bool,
    due: f64,
    frames: u64,
}

impl Demo {
    pub fn new(_gpu: &Gpu, _gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let bof = Bof::parse(files.get("fight.bof")?)?;
        let pe = Pe::new(files.get("fight.exe")?)?;
        let find = |n: &str| bof.find(n).ok_or_else(|| format!("fight.bof has no {n}"));
        let anubis = Fighter::new(
            &bof,
            &pe,
            find("an_moves")?,
            find("an_moves_shadow")?,
            'M',
            0x45c1c8,
            -50.0,
        )?;
        let nefertiti = Fighter::new(
            &bof,
            &pe,
            find("nef_moves")?,
            find("nef_moves_shadow")?,
            'W',
            0x45c268,
            -27.0,
        )?;
        Ok(Self {
            parts: [
                find("refl")?,
                find("top")?,
                find("background")?,
                find("scene")?,
            ],
            fighters: [anubis, nefertiti],
            bof,
            pictures: Pictures::default(),
            target: Spring::default(),
            eye: Spring {
                at: Vec3::new(0.0, 4000.0, 0.0),
                ..Spring::default()
            },
            fog: false,
            wireframe: false,
            due: 0.0,
            frames: 0,
        })
    }

    /// The opening: both fighters in place, the camera springs at rest and
    /// the eye 4000 above the floor, so the view drops in (`0x4013c0`,
    /// `0x4019c0`).
    fn reset(&mut self) {
        for f in &mut self.fighters {
            f.reset();
        }
        self.target = Spring::default();
        self.eye = Spring {
            at: Vec3::new(0.0, 4000.0, 0.0),
            ..Spring::default()
        };
    }

    fn tick(&mut self) {
        // Letting go of R starts the fight again (`0x401410`).
        if self.fighters[0].pad.0[4] {
            self.fighters[0].pad.0[4] = false;
            self.reset();
        }
        let [a, b] = &mut self.fighters;
        if a.touches(b, &self.bof) {
            clash(b, a);
        }
        b.tick(&self.bof);
        a.tick(&self.bof);
        // The camera looks between them, ten below their middle, and stands
        // off by seven times their separation (`0x401a70`, `0x401bb0`).
        let (wa, wb) = (a.world(&self.bof), b.world(&self.bof));
        let mid = (wa + wb) * 0.5;
        self.target
            .pull(Vec3::new(mid.x, mid.y - 10.0, mid.z), 0.09, 0.7);
        // The eye (`0x401bb0`) takes Nefertiti's body through Anubis's
        // child index, and stands off from that pair's middle: sideways by
        // seven times her height above it, back by seven times their spread.
        let nb = b.child(&self.bof, b.frame, a.body) + Vec3::X * b.x;
        let m = (wa + nb) * 0.5;
        let low = m.y - 5.0;
        let mut up = 30.0 - self.target.at.y;
        if self.target.at.y < 30.0 {
            up *= 15.0;
        }
        let eye = Vec3::new(
            (nb.y - low) * 7.0 + m.x - 10.0,
            up + 30.0,
            (wa.x - m.x) * 7.0 + m.z,
        );
        self.eye.pull(eye, 0.1, 0.55);
        self.frames += 1;
    }

    pub fn step(&mut self, dt: f64) {
        self.due = (self.due + dt * RATE).min(8.0);
        while self.due >= 1.0 {
            self.due -= 1.0;
            self.tick();
        }
    }

    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        for k in keys.chars() {
            let k = k.to_string();
            self.key(&k, true);
            self.tick();
            self.key(&k, false);
        }
        for _ in 0..(seconds * RATE).round() as usize {
            self.tick();
        }
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        // The README's keys: Anubis on the arrows and keypad, Nefertiti on
        // letters.
        let (who, slot) = match key {
            // key.dat: arrows GCI_*_4 and keypad GCI_*_2 for Anubis,
            // letters GCI_*_1 for Nefertiti; A to D are block, punch, high
            // kick and low kick (`0x402970`).
            "ArrowUp" => (0, 0),
            "ArrowDown" => (0, 1),
            "ArrowRight" => (0, 2),
            "ArrowLeft" => (0, 3),
            "7" => (0, 6),
            "8" => (0, 8),
            "9" => (0, 10),
            "6" => (0, 11),
            "w" | "W" => (1, 0),
            "s" | "S" => (1, 2),
            "a" | "A" => (1, 3),
            "u" | "U" => (1, 6),
            "i" | "I" => (1, 8),
            "o" | "O" => (1, 10),
            "l" | "L" => (1, 11),
            // R (GCI_START_1) restarts the fight when let go.
            "r" | "R" => {
                if !down {
                    self.fighters[0].pad.0[4] = true;
                }
                return true;
            }
            "f" | "F" => {
                if down {
                    self.fog = !self.fog;
                }
                return true;
            }
            "x" | "X" => {
                if down {
                    self.wireframe = !self.wireframe;
                }
                return true;
            }
            _ => return false,
        };
        self.fighters[who].press(slot, down);
        true
    }

    pub fn pointer(&mut self, _x: f64, _y: f64, _buttons: u8) {}

    pub fn wheel(&mut self, _amount: f64) {}

    pub fn cues(&mut self) -> Vec<String> {
        Vec::new()
    }

    pub fn report(&self) -> String {
        let [a, b] = &self.fighters;
        format!(
            "anubis {} {} x {:.0} health {:.1} nefertiti {} {} x {:.0} health {:.1} frame {}",
            a.state, a.frame, a.x, a.health, b.state, b.frame, b.x, b.health, self.frames
        )
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        // 40 degrees, far 5000 (`0x401170`).
        let mut camera = Camera::new(look_at(self.target.at, self.eye.at, Vec3::Y));
        camera.fov = 0.698_131_7;
        camera.far = 5000.0;
        let eye = camera.eye();
        let frames: Vec<(u32, usize)> = self
            .fighters
            .iter()
            .flat_map(|f| [(f.anim, f.frame as usize), (f.shadow, f.frame as usize)])
            .collect();
        let pick = move |id: u32| frames.iter().find(|&&(a, _)| a == id).map(|&(_, f)| f);
        let walk = |root: u32, shift: f32, out: &mut Vec<Item>| {
            let mut part = Vec::new();
            atb::walk(
                &self.bof,
                root,
                &View {
                    eye,
                    frame: &pick,
                    ..View::default()
                },
                &mut part,
            );
            for mut i in part {
                i.matrix = Mat4::from_translation(Vec3::new(shift, 0.0, 0.0)) * i.matrix;
                out.push(i);
            }
        };
        // Shadows, reflection, top and fighters, under a white light along
        // +x (a frame turned a quarter about y).
        let mut lit = Vec::new();
        for f in &self.fighters {
            walk(f.shadow, f.x, &mut lit);
        }
        walk(self.parts[0], 0.0, &mut lit);
        walk(self.parts[1], 0.0, &mut lit);
        for f in &self.fighters {
            walk(f.anim, f.x, &mut lit);
        }
        walk(self.parts[3], 0.0, &mut lit);
        // The backdrop under a white ambient light alone.
        let mut back = Vec::new();
        walk(self.parts[2], 0.0, &mut back);
        let fog = self.fog.then_some(Fog {
            near: 320.0,
            far: 400.0,
            color: [0.5; 3],
        });
        let key = [Light::directional(Vec3::ONE, Vec3::X)];
        let flat = [Light::ambient(Vec3::ONE)];
        gl.begin([0.0, 0.0, 0.0, 1.0]);
        for (items, lights) in [(&lit, &key[..]), (&back, &flat[..])] {
            let frame = Frame {
                camera,
                lights,
                fog,
                texture_image: &|_| None,
                extra: &[],
                pass: None,
                wireframe: self.wireframe,
                textures: true,
                bilinear: true,
            };
            render::draw(
                gpu,
                gl,
                &mut self.pictures,
                &self.bof,
                items,
                &frame,
                aspect,
            );
        }
    }
}
