#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Bugs,
    Wolfman,
    Squid,
    Chameleon,
    Tidepool,
    Zoltar,
}
pub const DEMO: Demo = Demo::Wolfman;
pub const MODELS: &[&str] = &["wolf_final.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "breathloop8";
pub const DATA_ENV: &str = "WOLFMAN_DATA";
pub const CLIPS: &[&str] = &[];
