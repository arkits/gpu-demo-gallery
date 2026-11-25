#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Bugs,
    Wolfman,
    Squid,
    Chameleon,
    Tidepool,
    Zoltar,
}
pub const DEMO: Demo = Demo::Zoltar;
pub const MODELS: &[&str] = &["zoltar.nmb"];
pub const SOUND_DIR: &str = "audio";
pub const INITIAL_MUSIC: &str = "background";
pub const DATA_ENV: &str = "ZOLTAR_DATA";
pub const CLIPS: &[&str] = &[
    "greetings.nmb",
    "pleasure.nmb",
    "bigman.nmb",
    "getahead.nmb",
    "inferior.nmb",
    "benice.nmb",
    "dontpick.nmb",
    "psst.nmb",
    "approach.nmb",
    "yawn.nmb",
    "bored.nmb",
];
