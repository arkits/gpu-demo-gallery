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

pub const DIALECT: Dialect = Dialect::Third;
pub const MODELS: &[&str] = &["pool2.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "tidep46l";
pub const DATA_ENV: &str = "TIDEPOOL_DATA";
/// Another pass for a shape, which its scene does not list.
pub struct Swap {
    pub shape: &'static str,
    pub program: &'static str,
    pub shader: &'static str,
    pub textures: &'static [&'static str],
}
/// The pool with caustics on it. The program swaps these in.
pub const SWAPS: &[Swap] = &[Swap {
    shape: "TidePoolShape",
    program: "ProgramMCB_directionalLightcaustics",
    shader: "ColorDiffBumpCaustics",
    textures: &[
        "blinn6_tex_c",
        "blinn6_tex_b",
        "target",
        "tidepoolcolor_light",
    ],
}];
/// Pixel shaders whose constant colors the program sets.
pub const DYNAMIC: &[&str] = &[];
pub const CLIPS: &[&str] = &[];

pub const DRIP: &str = "tpdrip3";
