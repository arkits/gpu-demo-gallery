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
];

/// `fight.exe` is read for the two move tables; nothing else of it.
pub const FILES: &[&str] = &["fight.bof", "fight.exe"];
pub const PARTS: &[(&str, usize, usize)] = &[];
pub const SOUNDS: &[&str] = &[];

const RATE: f64 = 60.0;

/// Input slots `0x401ff0` reads: jump, forward, back, block, low kick,
/// high kick, punch.
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
    pad: Pad,
    last: Pad,
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
        Ok(Self {
            anim,
            shadow,
            count: a.group.children.len(),
            horizontal,
            body,
            frame: moves[0][0],
            moves,
            state: 0,
            x,
            pad: Pad::default(),
            last: Pad::default(),
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

    /// One frame of the move machine (`0x401ff0`). Moves 1, 2 and 6 start
    /// on a fresh press the other fighter is not also making; 7 walks, 8
    /// jumps, 11 holds a block.
    fn tick(&mut self, bof: &Bof, other: Pad) {
        let p = self.pad.0;
        let fresh = |i: usize| p[i] && !self.last.0[i] && !other.0[i];
        let m = self.moves[self.state];
        let travel = |s: &Self, from: u32, to: u32| s.step(bof, to).x - s.step(bof, from).x;
        let mut go = None;
        match self.state {
            0 | 7 if fresh(11) => go = Some(1),
            0 | 7 if fresh(10) => go = Some(2),
            0 | 7 if fresh(8) => go = Some(6),
            0 if p[2] || p[3] => go = Some(7),
            0 if p[6] => go = Some(11),
            0 if p[0] => go = Some(8),
            7 if p[0] => go = Some(8),
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
    }

    /// Where the upper body is.
    fn world(&self, bof: &Bof) -> Vec3 {
        let s = self.child(bof, self.frame, self.body);
        Vec3::new(self.x + s.x, s.y, s.z)
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
            eye: Spring::default(),
            fog: false,
            wireframe: false,
            due: 0.0,
            frames: 0,
        })
    }

    fn tick(&mut self) {
        let [a, b] = &mut self.fighters;
        let (pa, pb) = (a.pad, b.pad);
        b.tick(&self.bof, pa);
        a.tick(&self.bof, pb);
        // The camera looks between them, ten below their middle, and stands
        // off by seven times their separation (`0x401a70`, `0x401bb0`).
        let (wa, wb) = (a.world(&self.bof), b.world(&self.bof));
        let mid = (wa + wb) * 0.5;
        self.target
            .pull(Vec3::new(mid.x, mid.y - 10.0, mid.z), 0.09, 0.7);
        let low = mid.y - 5.0;
        let mut up = 30.0 - self.target.at.y;
        if self.target.at.y < 30.0 {
            up *= 15.0;
        }
        let eye = Vec3::new(
            (wa.y - low) * 7.0 + mid.x - 10.0,
            up + 30.0,
            (wb.x - mid.x) * 7.0 + mid.z,
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
            "ArrowUp" => (0, 0),
            "ArrowRight" => (0, 2),
            "ArrowLeft" => (0, 3),
            "7" => (0, 6),
            "6" => (0, 8),
            "9" => (0, 10),
            "8" => (0, 11),
            "w" | "W" => (1, 0),
            "a" | "A" => (1, 2),
            "s" | "S" => (1, 3),
            "u" | "U" => (1, 6),
            "l" | "L" => (1, 8),
            "o" | "O" => (1, 10),
            "i" | "I" => (1, 11),
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
        self.fighters[who].pad.0[slot] = down;
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
            "anubis {} {} x {:.0} nefertiti {} {} x {:.0} frame {}",
            a.state, a.frame, a.x, b.state, b.frame, b.x, self.frames
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
