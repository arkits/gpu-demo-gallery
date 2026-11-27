//! What is particular to this demo, from `bin/args.txt` and the program.
//!
//! `args.txt` (and the identical `args_lite.txt` but for the window size and the
//! lite flag) lists the scene files in order: Timbury, his hair, the animation,
//! grass, the blur/glow passes, the background, the menu, the swarm bug model,
//! whether to combine bug geometry with animation, the swarm bug animation, and
//! how many bugs are in the swarm.
use crate::Error;
use crate::tree::Node;

/// The scene files, in `args.txt` order.
pub const TIMBURY: &str = "models/test.nmb";
pub const HAIR: &str = "models/hair.nma";
pub const ANIMATION: &str = "models/animation.nmb";
pub const GRASS: &str = "models/grass.nma";
pub const BLUR: &str = "models/blur.nma";
pub const BACKGROUND: &str = "models/bkg_split.nma";
pub const MENU: &str = "models/menu.nma";
pub const SWARMBUG: &str = "models/swarmbug.nma";
pub const SWARMBUG_ANIM: &str = "models/swarmbug_anim.nma";
pub const GRASS_TERRAIN: &str = "models/grass_terrain_geom.nma";
pub const HAIR_CURVES: &str = "models/hair_cvs.txt";
pub const SOUNDTRACK: &str = "audio/soundtrack.wav";
pub const SOUND_DIR: &str = "audio";
pub const DATA_ENV: &str = "TIMBURY_DATA";

/// How many bugs are in the swarm (`args.txt`, 40).
pub const BUGS: usize = 40;
/// The Lite version uses smaller render targets; the port always uses full size.
pub const WINDOW: (u32, u32) = (1024, 768);

/// Seconds without input after which the movie returns [I].
pub const IDLE: f64 = 120.0;

/// The swarm-bug loader (`FUN_004194f0`) reads the bug model and its animation.
pub struct Args {
    /// Files in order, from `args.txt`.
    pub files: Vec<String>,
    /// Whether bug geometry is combined with animation (1), and bug count.
    pub combine: bool,
    pub bugs: usize,
}

impl Args {
    pub fn read(text: &str) -> Result<Self, Error> {
        let mut files: Vec<String> = Vec::new();
        for line in text.lines() {
            let line = line.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            files.push(line.to_owned());
        }
        if files.len() < 11 {
            return Err("args.txt is cut short".into());
        }
        Ok(Self {
            files: files[..9].to_vec(),
            combine: files[9].trim() != "0",
            bugs: files[10].trim().parse()?,
        })
    }
}

/// A menu callback from `menu.nma` (`FUN_0041d550` names these).
pub const CAMERA_1: &str = "Camera1CB";
pub const CAMERA_2: &str = "Camera2CB";

/// Morph weight streams in `test.nmb` are named `morphWeightN`.
pub fn morph_name(n: usize) -> String {
    format!("morphWeight{n}")
}

/// The animation in `animation.nmb` holds the dance: body motion channels and
/// the body's morph weights.
pub fn animation(root: &Node) -> Result<crate::anim::Animation, Error> {
    let a = root.nodes("animations").first().ok_or("no animation")?;
    crate::anim::Animation::read(a)
}
