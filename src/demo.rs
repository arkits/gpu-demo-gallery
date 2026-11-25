#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Bugs,
    Wolfman,
    Squid,
    Chameleon,
    Tidepool,
    Zoltar,
}
pub const DEMO: Demo = Demo::Tidepool;
pub const MODELS: &[&str] = &["pool2.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "tidep46l";
pub const DATA_ENV: &str = "TIDEPOOL_DATA";
pub const CLIPS: &[&str] = &[];
