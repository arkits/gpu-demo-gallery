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
/// The first is the squid; plants are in the order of `SquidParams.txt`.
pub const MODELS: &[&str] = &[
    "squid.nmb",
    "kelp.nmb",
    "uglyfish.nmb",
    "plant3.nmb",
    "plant4.nmb",
    "plant1.nmb",
    "plant2.nmb",
    "plant0.nmb",
    "plant5.nmb",
];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "squid16m";
pub const DATA_ENV: &str = "SQUID_DATA";
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

pub const MORPH: &str = "morph";
pub const THRUST: &str = "sqthrust78b";
pub const BLINK: &str = "sq-eyem12mx";

/// Numbers of `SquidParams.txt`.
pub const FOG_DISTANCE: f32 = 400.0;
pub const FISH: usize = 30;
pub const BONES: usize = 20;
pub const SILT: u32 = 5120;
pub const BUBBLES: u32 = 1000;
pub const PLANTS_PER_TILE: usize = 100;

/// A kind of plant: size, and its sway along x and z as amplitude, factors of x, y, z
/// and time, and a constant.
pub struct Plant {
    pub width: f32,
    pub height: f32,
    pub sway: [[f32; 6]; 2],
}

const fn plant(width: f32, height: f32, amplitude: f32) -> Plant {
    Plant {
        width,
        height,
        sway: [
            [amplitude, 0.05, 0.1, 0.0, 1.0, 0.0],
            [amplitude, 0.0, 0.1, 0.025, 1.0, 1.5],
        ],
    }
}

pub const PLANTS: [Plant; 6] = [
    plant(12.0, 25.0, 8.0),
    plant(14.0, 28.0, 8.0),
    plant(10.0, 20.0, 8.0),
    plant(10.0, 20.0, 4.0),
    plant(15.0, 30.0, 6.0),
    plant(14.0, 28.0, 8.0),
];
