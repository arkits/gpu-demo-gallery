//! What is particular to this demo's files.

/// Versions of the scene format and its viewer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// GeForce 3: shorter texture nodes, eight spare matrices.
    First,
    /// GeForce 4.
    Second,
    /// As the second, but height fields repeat textures in three directions.
    Third,
}

pub const DIALECT: Dialect = Dialect::Second;
pub const MODELS: &[&str] = &["wolf_final.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const DATA_ENV: &str = "WOLFMAN_DATA";
/// Another pass for a shape, which its scene does not list.
pub struct Swap {
    pub shape: &'static str,
    pub program: &'static str,
    pub shader: &'static str,
    pub textures: &'static [&'static str],
}
pub const SWAPS: &[Swap] = &[];
/// Pixel shaders whose constant colors the program sets.
pub const DYNAMIC: &[&str] = &[];
pub const CLIPS: &[&str] = &[];

/// The music, which repeats (0x405320).
pub const MUSIC: &str = "wolf 47b lr";

/// A stretch of the animation, in frames at 30 a second, and its sound (0x4010c0).
pub struct Segment {
    pub frames: (u32, u32),
    /// Its sound repeats; segments that loop are where actions end.
    pub repeats: bool,
    pub sound: &'static str,
}

const fn segment(start: u32, end: u32, repeats: bool, sound: &'static str) -> Segment {
    Segment {
        frames: (start, end),
        repeats,
        sound,
    }
}

pub const SCRATCH: usize = 0;
pub const HOWL: usize = 1;
pub const BREATH_TO_WALK: usize = 2;
pub const WALK: usize = 3;
pub const SIDEWAYS: usize = 4;
pub const WALK_TO_RUN: usize = 5;
pub const RUN: usize = 6;
pub const RUN_TO_WALK: usize = 7;
pub const WALK_TO_BREATH: usize = 8;
pub const BREATHE: usize = 9;

pub const SEGMENTS: [Segment; 10] = [
    segment(0, 103, false, "scratching82"),
    segment(103, 315, false, "howling 3"),
    segment(315, 362, false, "brwalk 3"),
    segment(362, 407, true, "2lwalk loop 3"),
    segment(407, 632, false, "sideways 3b"),
    segment(632, 754, false, "w2run 3"),
    segment(754, 779, true, "brun loop 3"),
    segment(779, 880, false, "brun2w 3"),
    segment(880, 925, false, "w2b 3"),
    segment(925, 967, true, "breathloop8"),
];

/// What plays until a key is pressed, again and again (0x401760).
pub const SCRIPT: [usize; 22] = [
    BREATHE,
    BREATHE,
    SCRATCH,
    HOWL,
    BREATH_TO_WALK,
    WALK,
    WALK,
    WALK,
    SIDEWAYS,
    WALK,
    WALK_TO_RUN,
    RUN,
    RUN,
    RUN,
    RUN,
    RUN,
    RUN,
    RUN,
    RUN_TO_WALK,
    WALK_TO_BREATH,
    BREATHE,
    BREATHE,
];

/// What keys 1 to 6 ask for (0x402870).
pub const KEYS: [usize; 6] = [BREATHE, SCRATCH, HOWL, WALK, SIDEWAYS, RUN];
