//! What is particular to this demo, from `bin/args_ultra.txt` and the program.

pub const FAIRY: &str = "models/fairy_ultra.nmb";
pub const BRANCH: &str = "models/branch.nmb";
pub const SUNNING: &str = "models/sunning.nmb";
pub const MODELS: &[&str] = &[FAIRY, BRANCH, SUNNING];
pub const BADGE: &str = "textures/nv_badge.dds";
pub const SOUND_DIR: &str = "audio";
pub const DATA_ENV: &str = "DAWN_DATA";

/// A dance: `FUN_00411530` fills these lists.
pub struct Scene {
    pub name: &'static str,
    pub sound: &'static str,
    /// Seconds after its start at which the white begins to fade, and at which it has
    /// returned and the scene starts again.
    pub start: f64,
    pub stop: f64,
    /// Where the branches are.
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
}
pub const SCENES: [Scene; 2] = [
    Scene {
        name: "Dance",
        sound: "flirt",
        start: 1.0,
        stop: 43.0,
        translation: [0.0; 3],
        rotation: [0.0; 3],
    },
    Scene {
        name: "Sunning",
        sound: "sunning",
        start: 1.0,
        stop: 70.0,
        translation: [-1.119, 8.521, 0.0],
        rotation: [0.0, -1.399, 0.0],
    },
];
/// Seconds that each fade takes.
pub const FADE: f64 = 3.0;
/// Seconds without input after which the movie returns.
pub const IDLE: f64 = 120.0;

/// Morph targets that one mesh can use at a time, and the least weight that counts
/// (`FUN_00408ed0`).
pub const MORPHS: usize = 5;
pub const LEAST: f32 = 0.01;

/// Five sliders: morph targets and where their knobs start (`FUN_0040c0c0`), and their
/// labels (`0x5483b0`).
pub struct Expression {
    pub name: &'static str,
    pub targets: [usize; MORPHS],
    pub weights: [f32; MORPHS],
    pub labels: [&'static str; MORPHS],
}
pub const EXPRESSIONS: [Expression; 8] = [
    Expression {
        name: "Happy",
        targets: [20, 26, 29, 35, 47],
        weights: [0.3, 0.4, 0.16, 1.0, 0.07],
        labels: ["right brow", "wink", "eyes", "smile", "left brow"],
    },
    Expression {
        name: "Disgust",
        targets: [19, 23, 38, 41, 45],
        weights: [0.27, 1.0, 0.14, 1.0, 0.14],
        labels: ["left eye", "eyes", "eyebrows", "ears", "eyebrows 2"],
    },
    Expression {
        name: "Angry",
        targets: [22, 33, 34, 37, 41],
        weights: [0.25, 1.0, 0.11, 0.2, 1.0],
        labels: ["eyes", "eyes and mouth", "mouth", "squint", "ears"],
    },
    Expression {
        name: "Sad",
        targets: [21, 39, 40, 41, 42],
        weights: [0.28, 1.0, 0.4, 1.0, 0.76],
        labels: ["eyebrows", "eyebrows 2", "mouth", "ears", "ears 2"],
    },
    Expression {
        name: "Disbelief",
        targets: [16, 17, 18, 42, 50],
        weights: [0.3, 0.23, 1.0, 0.39, 0.08],
        labels: ["eyes", "mouth", "eyebrows", "ears", "eyebrows 2"],
    },
    Expression {
        name: "Pucker",
        targets: [7, 11, 20, 37, 43],
        weights: [0.25, 0.34, 0.56, 0.41, 1.0],
        labels: ["blink", "eyebrows", "eyes and mouth", "squint", "pucker"],
    },
    Expression {
        name: "Frightened",
        targets: [14, 15, 30, 42, 45],
        weights: [1.0, 0.12, 0.5, 0.77, 0.15],
        labels: ["eyes and mouth", "squint", "nostrils", "ears", "eyebrows"],
    },
    Expression {
        name: "Grin",
        targets: [3, 4, 35, 37, 42],
        weights: [0.14, 0.57, 0.46, 0.27, 0.29],
        labels: ["smile", "smile 2", "smile 3", "eyes", "ears"],
    },
];
/// The expression that the program selects first.
pub const FIRST: usize = 7;
