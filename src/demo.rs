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

pub const DIALECT: Dialect = Dialect::First;
pub const MODELS: &[&str] = &["chameleon.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "chabgm3";
pub const DATA_ENV: &str = "CHAMELEON_DATA";
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

/// Milliseconds that a skin stays, and the head start of the first (0x402409).
pub const PERIOD: f64 = 9918.0;
pub const LEAD: f64 = 2500.0;
/// How fast one skin gives way to the next, per millisecond (0x4bb038).
pub const FADE: f64 = 0.0003;
pub const SKINS: usize = 7;
/// Sound that comes with a new skin (0x4d1a88).
pub const CHANGE: &str = "chafx2";
