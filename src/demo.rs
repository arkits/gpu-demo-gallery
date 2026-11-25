#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Bugs,
    Wolfman,
    Squid,
    Chameleon,
    Tidepool,
    Zoltar,
}
pub const DEMO: Demo = Demo::Squid;
pub const MODELS: &[&str] = &["squid.nmb", "kelp.nmb", "uglyfish.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "squid16m";
pub const DATA_ENV: &str = "SQUID_DATA";
pub const CLIPS: &[&str] = &[];
