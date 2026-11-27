//! Condensation on the bottle (Procedural EMBM module, 0x430eb0 to 0x432a30): four drops run
//! down a 256 by 256 height map leaving trails, ten more sit still; each step the height map
//! becomes a bump map with luminance (L6V5U5) and a light map is stamped with dark spots
//! where the drops are. Pixels are 16 bits as on the Kyro.
use crate::image::Image;

/// The C library's rand() of Visual C++, seeded as at start.
#[derive(Debug, Clone)]
pub struct Rand(u32);

impl Default for Rand {
    fn default() -> Self {
        Self(1)
    }
}

impl Rand {
    pub fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7fff) as i32
    }
    /// 0x425920.
    pub fn unit(&mut self) -> f32 {
        self.rand() as f32 * 3.051_851e-5
    }
}

/// A picture in a 16-bit surface.
#[derive(Debug, Clone)]
pub struct Sprite {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u16>,
}

impl Sprite {
    /// A grey picture loaded as a height map: the height in both bytes.
    pub fn height(i: &Image) -> Self {
        let px = i
            .rgba
            .chunks_exact(4)
            .map(|p| u16::from(p[0]) << 8 | u16::from(p[0]))
            .collect();
        Self {
            w: i.width as usize,
            h: i.height as usize,
            px,
        }
    }
    /// A picture loaded as 5-6-5 colour.
    pub fn rgb565(i: &Image) -> Self {
        let px = i
            .rgba
            .chunks_exact(4)
            .map(|p| {
                (u16::from(p[0]) >> 3) << 11 | (u16::from(p[1]) >> 2) << 5 | u16::from(p[2]) >> 3
            })
            .collect();
        Self {
            w: i.width as usize,
            h: i.height as usize,
            px,
        }
    }
}

const TRAIL: usize = 50;

#[derive(Debug, Clone)]
struct Drop {
    x: i32,
    y: i32,
    trail: [(i32, i32); TRAIL],
    ages: [i32; TRAIL],
    speed: f32,
    /// 0 straight down, 1 to the left, 2 to the right.
    state: i32,
    active: bool,
}

pub struct Droplets {
    pub size: usize,
    height: Vec<u16>,
    light: Vec<u16>,
    drops: Vec<Drop>,
    statics: Vec<(i32, i32)>,
    /// Shared by all drops (0x587d3c), as in the original.
    count: usize,
    tick: u32,
    /// Steps between moves (0x5870dc, T and G keys in the original change others).
    pub period: u32,
    /// Trail strength (0x587ce0).
    pub trail: f32,
    /// Luminance where the glass is dry (0x587d2c): 100, or 0 in toon style.
    pub dry: i32,
    bumps: Vec<Sprite>,
    lights: Vec<Sprite>,
    static_bump: Sprite,
    static_light: Sprite,
    strip: Vec<u16>,
    rand: Rand,
}

impl Droplets {
    /// 0x431520 and 0x431250: drops at random places with random speeds.
    pub fn new(
        bumps: Vec<Sprite>,
        lights: Vec<Sprite>,
        static_bump: Sprite,
        static_light: Sprite,
    ) -> Self {
        let size = 256;
        let mut rand = Rand::default();
        let mut drops = Vec::new();
        for i in 0..4 {
            let x = rand.rand() % size as i32 + i * size as i32;
            let y = rand.rand() % size as i32;
            let mut speed;
            loop {
                speed = rand.unit() * 3.0;
                if speed >= 1.0 {
                    break;
                }
            }
            drops.push(Drop {
                x,
                y,
                trail: [(-1, -1); TRAIL],
                ages: [-1; TRAIL],
                speed,
                state: 0,
                active: true,
            });
        }
        let statics = (0..10)
            .map(|_| (rand.rand() % size as i32, rand.rand() % size as i32))
            .collect();
        // The trail's strip: half the first drop's width from its first row (0x431160).
        let first = &bumps[0];
        let half = first.w / 2;
        let strip = first.px[half / 2..half / 2 + half].to_vec();
        Self {
            size,
            height: vec![0; size * size],
            light: vec![0xffff; size * size],
            drops,
            statics,
            count: 0,
            tick: 0,
            period: 2,
            trail: 0.15,
            dry: 100,
            bumps,
            lights,
            static_bump,
            static_light,
            strip,
            rand,
        }
    }

    /// One call of 0x431900: move every `period` calls, then remake both maps.
    pub fn step(&mut self) {
        if self.period != 0 && self.tick.is_multiple_of(self.period) {
            self.drip();
            self.tick = 0;
        }
        self.tick += 1;
        self.heights();
        self.lights();
    }

    /// 0x431a20.
    fn drip(&mut self) {
        for d in &mut self.drops {
            if self.count == TRAIL {
                for k in 0..TRAIL - 1 {
                    d.trail[k] = d.trail[k + 1];
                    d.ages[k] = d.ages[k + 1];
                    d.ages[k + 1] += 1;
                }
                self.count = TRAIL - 1;
            }
            for a in &mut d.ages[..self.count] {
                *a += 1;
            }
            d.trail[self.count] = (d.x, d.y);
            d.ages[self.count] = 1;
            self.count += 1;
            d.y += d.speed as i32;
            match d.state {
                1 => d.x -= 1,
                2 => d.x += 1,
                _ => {}
            }
        }
        // 0x432920: a walk that mostly goes straight; slow drops never turn.
        for d in &mut self.drops {
            let r = self.rand.rand() % 100;
            d.state = if d.speed < 2.0 {
                0
            } else {
                let (left, straight) = match d.state {
                    0 => (10, 90),
                    1 => (15, 95),
                    2 => (5, 85),
                    _ => (-1, -1),
                };
                if left < 0 {
                    0
                } else if r < left {
                    1
                } else if r < straight {
                    0
                } else {
                    2
                }
            };
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// 0x432190: a sprite into a surface at (x, y), wrapping. Modes: 0 copy, 3 keep the
    /// larger. With a scale other than 1, the sprite's high byte times it in both bytes.
    fn stamp(
        target: &mut [u16],
        size: usize,
        s: &[u16],
        (w, h): (usize, usize),
        x: i32,
        y: i32,
        scale: f32,
        mode: u32,
    ) {
        let (x, y) = ((x as u32 as usize) % size, (y as u32 as usize) % size);
        for r in 0..h {
            let row = (y + r) % size;
            for c in 0..w {
                let at = row * size + (x + c) % size;
                let src = s[r * w + c];
                let value = if scale == 1.0 {
                    src
                } else {
                    let b = (f32::from((src >> 8) as u8) * scale) as i32 as u16 & 0xff;
                    b << 8 | b
                };
                match mode {
                    0 => target[at] = value,
                    3 if target[at] < src => target[at] = value,
                    _ => {}
                }
            }
        }
    }

    /// 0x431ba0: the height map, cleared, then trails, drops and still drops.
    fn heights(&mut self) {
        let size = self.size;
        self.height.fill(0);
        let half = self.bumps[0].w / 2;
        let strip_w = self.strip.len();
        for (i, d) in self.drops.iter().enumerate() {
            let step = self.trail * 0.02 / d.speed;
            let n = d.speed as i32;
            for (j, &(x, y)) in d.trail.iter().enumerate() {
                if x == -1 {
                    continue;
                }
                for k in 0..n {
                    let scale = k as f32 * step + (j as i32 * n) as f32 * step;
                    let sx = x + (half as i32 - strip_w as i32 / 2);
                    Self::stamp(
                        &mut self.height,
                        size,
                        &self.strip,
                        (strip_w, 1),
                        sx,
                        y + k,
                        scale,
                        0,
                    );
                }
            }
            if d.active {
                let s = &self.bumps[i % self.bumps.len()];
                Self::stamp(&mut self.height, size, &s.px, (s.w, s.h), d.x, d.y, 1.0, 3);
            }
        }
        let s = &self.static_bump;
        for &(x, y) in &self.statics {
            Self::stamp(&mut self.height, size, &s.px, (s.w, s.h), x, y, 1.0, 3);
        }
    }

    /// 0x431f10: the light map, white, with the drops' spots copied in.
    fn lights(&mut self) {
        let size = self.size;
        self.light.fill(0xffff);
        for (i, d) in self.drops.iter().enumerate() {
            if d.active {
                let s = &self.lights[i % self.lights.len()];
                Self::stamp(&mut self.light, size, &s.px, (s.w, s.h), d.x, d.y, 1.0, 0);
            }
        }
        let s = &self.static_light;
        for &(x, y) in &self.statics {
            Self::stamp(&mut self.light, size, &s.px, (s.w, s.h), x, y, 1.0, 0);
        }
    }

    /// 0x4327a0: differences across and down of the heights' low byte, five bits each, and
    /// six bits of luminance, full where wet and `dry` where the height is under 10. As a
    /// picture for the bump stage: red and green the signed offsets around 128, blue the
    /// luminance.
    pub fn bump_map(&self) -> Image {
        let n = self.size;
        let h = |x: usize, y: usize| i32::from(self.height[(y % n) * n + x % n] as u8);
        let mut rgba = Vec::with_capacity(n * n * 4);
        for y in 0..n {
            for x in 0..n {
                let c = h(x, y);
                let du = c - h(x + 1, y);
                let dv = c - h(x, y + 1);
                let l = if c < 10 { self.dry } else { 0xff };
                let u5 = (du >> 3) & 0x1f;
                let v5 = (dv >> 3) & 0x1f;
                let l6 = (l >> 2) & 0x3f;
                let signed = |v: i32| if v >= 16 { v - 32 } else { v };
                let enc = |v: i32| {
                    (signed(v) as f32 / 16.0 * 127.0 + 128.0)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                rgba.extend([enc(u5), enc(v5), (l6 * 255 / 63) as u8, 255]);
            }
        }
        Image {
            width: n as u32,
            height: n as u32,
            rgba,
        }
    }

    pub fn light_map(&self) -> Image {
        let rgba = self
            .light
            .iter()
            .flat_map(|&p| {
                let r = (p >> 11) & 31;
                let g = (p >> 5) & 63;
                let b = p & 31;
                [
                    (r * 255 / 31) as u8,
                    (g * 255 / 63) as u8,
                    (b * 255 / 31) as u8,
                    255,
                ]
            })
            .collect();
        Image {
            width: self.size as u32,
            height: self.size as u32,
            rgba,
        }
    }
}
