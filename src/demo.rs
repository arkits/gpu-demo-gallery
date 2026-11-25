#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Bugs,
    Wolfman,
    Squid,
    Chameleon,
    Tidepool,
    Zoltar,
}

pub const DEMO: Demo = Demo::Bugs;
pub const MODELS: &[&str] = &["stage.nmb", "bug3.nmb", "bug2.nmb", "bug1.nmb", "bug0.nmb"];
pub const SOUND_DIR: &str = "sounds";
pub const INITIAL_MUSIC: &str = "wind_constant";
pub const DATA_ENV: &str = "BUGS_DATA";
pub const CLIPS: &[&str] = &[];
