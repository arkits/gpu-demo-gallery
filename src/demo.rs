#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Bugs,
    Wolfman,
    Squid,
    Chameleon,
    Tidepool,
    Zoltar,
}
pub const DEMO: Demo = Demo::Chameleon;
pub const MODELS: &[&str] = &["chameleon.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "chabgm3";
pub const DATA_ENV: &str = "CHAMELEON_DATA";
pub const CLIPS: &[&str] = &[];
