//! What is particular to this demo, from `bin/args.txt` and the program.

// The scene the demo opens, and the eight animation files it merges after it
// (`args.txt`, `NVDemoTest: Merging scene`).
pub const SCENE: &str = "models/boss_scene_nv31.nmb";
pub const BOSS_SUBD: &str = "models/boss_subd_anim_occlusion.nmb";
pub const BOSS_VEST: &str = "models/boss_vest_subd_anim.nmb";
pub const BOSS_ANIMS: &str = "models/boss_anims.nmb";
pub const CREEP_A: &str = "models/creepA_anim_h.nmb";
pub const CREEP_B: &str = "models/creepB_anim_h.nmb";
pub const CREEP_BX: &str = "models/creep_Bx_anim_h.nmb";
pub const CREEP_C: &str = "models/creepC_anim_h.nmb";
pub const CREEP_CX: &str = "models/creepCx_anim_h.nmb";
pub const MODELS: &[&str] = &[
    SCENE, BOSS_SUBD, BOSS_VEST, BOSS_ANIMS, CREEP_A, CREEP_B, CREEP_BX, CREEP_C, CREEP_CX,
];
pub const BADGE: &str = "textures/nv_badge.tga";
pub const SOUND: &str = "OgreWurly";
pub const SOUND_DIR: &str = "audio";
pub const DATA_ENV: &str = "OGRE_DATA";

/// The demo opens at 1024 by 768 (`args.txt`).
pub const WIDTH: u32 = 1024;
pub const HEIGHT: u32 = 768;

/// Seconds without input after which the movie returns to its start [I].
pub const IDLE: f64 = 120.0;
