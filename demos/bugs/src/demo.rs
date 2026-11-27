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
/// The stage, and bugs in falling detail. The last is for shadows only.
pub const MODELS: &[&str] = &["stage.nmb", "bug3.nmb", "bug2.nmb", "bug1.nmb", "bug0.nmb"];
pub const SOUND_DIR: &str = "sounds";
pub const INITIAL_MUSIC: &str = "wind_constant";
pub const DATA_ENV: &str = "BUGS_DATA";
pub const CLIPS: &[&str] = &[];
/// Another pass for a shape, which its scene does not list.
pub struct Swap {
    pub shape: &'static str,
    pub program: &'static str,
    pub shader: &'static str,
    pub textures: &'static [&'static str],
}
pub const SWAPS: &[Swap] = &[];
/// Pixel shaders whose constant colors the program sets.
pub const DYNAMIC: &[&str] = &["BugShader", "GroundShader2"];

/// From `bugData.txt`: how many bugs, and up to where the first two models are drawn.
pub const BUGS: usize = 60;
pub const DETAIL: [f32; 2] = [180.0, 400.0];

pub const VOICES: &[&str] = &[
    "bug_clack1",
    "bug_clack2",
    "bug_clack3",
    "bug_screech1",
    "bug_screech2",
    "bug_screech3",
];
