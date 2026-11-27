//! What is particular to this demo: the takes, cameras, expressions and files.
//!
//! Scene files are chosen by animation index (`keys.txt` plays animations
//! 01-15); camera presets come from `bin/vistas.txt`, `bin/effectshots.txt`
//! and `bin/defaultUserCamPositions.txt`; expressions from
//! `bin/BlendCombos.txt`.

pub const GEOM: &str = "models/geom.nmb";
pub const HAIR_MESH: &str = "models/hair_scalp_lite.nmb";
pub const ANIM_RESET: &str = "models/anim_reset.nma";
pub const POSES: &str = "models/poses.nma";
pub const QUADS: &str = "models/quads.nma";
pub const CARDS: &str = "models/cards.nma";
pub const MENU: &str = "models/menu.nma";
pub const PARTICLES: &str = "models/particles.nma";
pub const HULA: &str = "models/hula.nma";
pub const HAIR_CUSTOM: &str = "models/hair_custom.nma";
pub const CUBE_BLUR: &str = "models/cube_blur.nma";
pub const FORCE_INIT: &str = "models/_forceinitpbuffers.nma";
pub const ARTIST_MODE: &str = "models/_artistmode.nma";
pub const CUT1: &str = "models/cut1.nma";

pub const MODELS: &[&str] = &[
    GEOM,
    HAIR_MESH,
    "models/scene2.nmb",
    "models/scene4.nmb",
    "models/scene6.nmb",
    "models/scene8.nmb",
    "models/scene10.nmb",
    "models/scene12.nmb",
    "models/scene14.nmb",
    "models/scene161820.nmb",
    "models/scene24.nmb",
    "models/scene26.nmb",
    ANIM_RESET,
    POSES,
    QUADS,
    CARDS,
    MENU,
    PARTICLES,
    HULA,
    HAIR_CUSTOM,
    CUBE_BLUR,
    FORCE_INIT,
    ARTIST_MODE,
    CUT1,
];

/// Hair growth guides and parameters (the hair system reads `<name>_curves.txt`
/// with `<name>_params.txt`; `0x4...` builds the names, see ENGINE.md).
pub const HAIR_FILES: &[&str] = &[
    "bin/hair_curves.txt",
    "bin/hairparams.txt",
    "bin/hairclumpsparams.txt",
    "bin/gotti_curves.txt",
    "bin/gotti_params.txt",
    "bin/hairclumpsparams_gotti.txt",
    "bin/haircurves.txt",
    "bin/haircurves_gotti.txt",
    "bin/hair_color.tga",
    "bin/hair_density.tga",
    "bin/hair_kill.tga",
    "bin/hair_lenctrl.tga",
    "bin/hair_lenght.tga",
    "bin/hairbase.tga",
    "bin/hairlen.tga",
    "bin/gotti_color.tga",
    "bin/gotti_density.tga",
    "bin/gotti_kill.tga",
    "bin/gotti_lenctrl.tga",
    "bin/gotti_lenght.tga",
    "bin/hairclumpsparams.txt",
];

/// Camera presets and expressions, parsed at runtime.
pub const TEXT_FILES: &[&str] = &[
    "bin/blendcombos.txt",
    "bin/vistas.txt",
    "bin/effectshots.txt",
    "bin/defaultusercampositions.txt",
];

pub const SOUND_DIR: &str = "audio";
pub const DATA_ENV: &str = "MADMODMIKE_DATA";

/// One take: its scene file, and how long the port plays it (`0x4...` loops
/// the animation; lengths below are the animation spans, see ENGINE.md).
pub struct Take {
    pub name: &'static str,
    pub file: &'static str,
    /// Seconds of animation, after which the take loops.
    pub stop: f64,
}

pub const TAKES: &[Take] = &[
    Take {
        name: "Bedroom",
        file: "models/scene2.nmb",
        stop: 26.4,
    },
    Take {
        name: "Workshop",
        file: "models/scene4.nmb",
        stop: 30.0,
    },
    Take {
        name: "Mermaid",
        file: "models/scene6.nmb",
        stop: 24.0,
    },
    Take {
        name: "Lamp",
        file: "models/scene8.nmb",
        stop: 22.0,
    },
    Take {
        name: "Flames",
        file: "models/scene10.nmb",
        stop: 20.0,
    },
    Take {
        name: "Sparks",
        file: "models/scene12.nmb",
        stop: 18.0,
    },
    Take {
        name: "Moon",
        file: "models/scene14.nmb",
        stop: 28.0,
    },
    Take {
        name: "Posters",
        file: "models/scene161820.nmb",
        stop: 32.0,
    },
    Take {
        name: "Grinder",
        file: "models/scene24.nmb",
        stop: 26.0,
    },
    Take {
        name: "Welder",
        file: "models/scene26.nmb",
        stop: 26.0,
    },
];

/// Seconds that each fade takes.
pub const FADE: f64 = 2.0;
/// Seconds without input after which the movie returns.
pub const IDLE: f64 = 120.0;

/// Morph targets that one mesh can use at a time, and the least weight that counts.
pub const MORPHS: usize = 8;
pub const LEAST: f32 = 0.01;

/// A facial expression: the jaw angle and five blend-shape weights
/// (`bin/BlendCombos.txt`: target index, weight).
pub struct Expression {
    pub name: &'static str,
    pub jaw: f32,
    pub targets: [usize; 5],
    pub weights: [f32; 5],
}

pub const EXPRESSIONS: [Expression; 7] = [
    Expression {
        name: "Gentle smile",
        jaw: 0.0,
        targets: [19, 25, 26, 38, 63],
        weights: [0.019723, 0.019723, 0.515938, 0.019723, 1.0],
    },
    Expression {
        name: "Side smile",
        jaw: 0.0,
        targets: [2, 25, 31, 37, 46],
        weights: [0.019723, 0.302482, 0.840392, 0.720856, 0.019723],
    },
    Expression {
        name: "Angry",
        jaw: 0.308302,
        targets: [2, 19, 24, 30, 33],
        weights: [0.019723, 0.019723, 1.0, 0.618985, 0.222369],
    },
    Expression {
        name: "Yawning",
        jaw: 1.0,
        targets: [0, 3, 17, 32, 61],
        weights: [0.608863, 0.506404, 0.660092, 0.3271, 0.679303],
    },
    Expression {
        name: "Panic",
        jaw: 0.416752,
        targets: [4, 17, 19, 33, 84],
        weights: [0.858607, 1.0, 0.775358, 0.314293, 1.0],
    },
    Expression {
        name: "Smile with winks",
        jaw: 0.0,
        targets: [2, 12, 31, 46, 80],
        weights: [0.02, 0.02, 0.333504, 0.55123, 0.845799],
    },
    Expression {
        name: "Sad",
        jaw: 0.0,
        targets: [0, 3, 17, 33, 48],
        weights: [0.333504, 0.211834, 0.730533, 0.653689, 0.493596],
    },
];

/// The expression that the program selects first.
pub const FIRST: usize = 0;
