//! What is particular to this demo's files.
use std::ops::Range;

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
pub const MODELS: &[&str] = &["zoltar.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const MUSIC: &str = "background_soft";
/// The button, as it is and pressed, and the logo.
pub const OVERLAYS: [&str; 3] = [
    "userinterface/gemotruth_up_256.dds",
    "userinterface/gemotruth_down_256.dds",
    "userinterface/nvidialogo.dds",
];
/// The texture that turns with the light (0x401920).
pub const FOLLOWS_LIGHT: &str = "phongE10_tex_e";
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
pub const DATA_ENV: &str = "ZOLTAR_DATA";

/// A scene of his head moving.
pub struct Clip {
    pub scene: &'static str,
    /// The voice, and seconds until it is heard.
    pub voice: Option<(&'static str, f64)>,
    /// Seconds, as in the scene.
    pub length: f32,
}

const fn clip(scene: &'static str, voice: Option<(&'static str, f64)>, length: f32) -> Clip {
    Clip {
        scene,
        voice,
        length,
    }
}

/// In the order of `greetingsAnimation.txt`, `fortuneAnimations.txt` and
/// `idleAnimations.txt`.
pub const CLIPS: &[Clip] = &[
    clip("greetings.nmb", Some(("greetings", 0.95)), 10.0),
    clip("pleasure.nmb", Some(("pleasure", 2.05)), 8.666),
    clip("bigman.nmb", Some(("bigman", 1.9)), 8.666),
    clip("getahead.nmb", Some(("getahead", 1.3)), 6.0),
    clip("inferior.nmb", Some(("inferior", 1.85)), 7.491),
    clip("benice.nmb", Some(("benice", 1.5)), 5.478),
    clip("dontpick.nmb", Some(("dontpick", 1.5)), 5.478),
    clip("psst.nmb", Some(("psst", 2.3)), 8.217),
    clip("approach.nmb", Some(("approach", 1.3)), 5.478),
    clip("bored.nmb", None, 13.497),
    clip("yawn.nmb", Some(("yawn", 1.75)), 8.666),
];
pub const GREETING: usize = 0;
pub const FORTUNES: Range<usize> = 1..8;
pub const IDLE: Range<usize> = 8..11;
