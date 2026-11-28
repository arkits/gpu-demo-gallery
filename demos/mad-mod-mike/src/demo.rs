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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Set {
    OldBedroom,
    Workshop,
    NewBedroom,
    None,
}

/// One take: its scene file, and how long the port plays it (`0x4...` loops
/// the animation; lengths below are the animation spans, see ENGINE.md).
pub struct Take {
    pub name: &'static str,
    pub file: &'static str,
    /// Master-timeline start in seconds (`args.txt` start frame at 30 fps;
    /// the curves count from there). Takes without a storyboard slot use
    /// their own curve start [I].
    pub start: f64,
    /// Seconds of animation, after which the take loops (`args.txt`
    /// length; takes without a slot use their key span [I]).
    pub stop: f64,
    /// The movie camera rig for this take (`args.txt` flag 6: `camera1`,
    /// `camera2`, `cornerCam`); empty when the take animates none [I].
    pub camera: &'static str,
    // Set and prop visibility from args.txt.
    pub set: Set,
    pub grinder: bool,
    pub welder: bool,
    pub door: bool,
    pub hinges: bool,
    pub graphics_card: bool,
}

pub const TAKES: &[Take] = &[
    Take {
        name: "Intro",
        file: "models/scene2.nmb",
        start: 175.0 / 30.0,
        stop: 20.5,
        camera: "camera1",
        set: Set::OldBedroom,
        grinder: false,
        welder: false,
        door: true,
        hinges: true,
        graphics_card: false,
    },
    Take {
        name: "Karate",
        file: "models/scene4.nmb",
        start: 793.0 / 30.0,
        stop: 5.16666666667,
        camera: "camera1",
        set: Set::OldBedroom,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: false,
    },
    Take {
        name: "Mermaid",
        file: "models/scene6.nmb",
        start: 28.4,
        stop: 3.2,
        camera: "",
        set: Set::OldBedroom,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: false,
    },
    Take {
        name: "Cutting open the case",
        file: "models/scene8.nmb",
        start: 1106.0 / 30.0,
        stop: 9.95833333333,
        camera: "camera1",
        set: Set::OldBedroom,
        grinder: true,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: false,
    },
    Take {
        name: "Tooth fairy",
        file: "models/scene10.nmb",
        start: 0.0,
        stop: 12.79166666667,
        camera: "camera1",
        set: Set::Workshop,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: false,
    },
    Take {
        name: "Pulling computer apart",
        file: "models/scene12.nmb",
        start: 1788.0 / 30.0,
        stop: 5.25,
        camera: "camera2",
        set: Set::OldBedroom,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: false,
    },
    Take {
        name: "Welding",
        file: "models/scene14.nmb",
        start: 1945.0 / 30.0,
        stop: 2.875,
        camera: "camera1",
        set: Set::NewBedroom,
        grinder: false,
        welder: true,
        door: false,
        hinges: false,
        graphics_card: false,
    },
    Take {
        name: "Posters",
        file: "models/scene161820.nmb",
        start: 2.0 / 30.0,
        stop: 21.41666666667,
        camera: "camera1",
        set: Set::NewBedroom,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: true,
    },
    Take {
        name: "Flies away",
        file: "models/scene24.nmb",
        start: 1.0 / 30.0,
        stop: 12.0,
        camera: "cornerCam",
        set: Set::None,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: true,
    },
    Take {
        name: "Mermaid",
        file: "models/scene26.nmb",
        start: 0.0,
        stop: 4.41666666667,
        camera: "camera1",
        set: Set::NewBedroom,
        grinder: false,
        welder: false,
        door: false,
        hinges: false,
        graphics_card: true,
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
