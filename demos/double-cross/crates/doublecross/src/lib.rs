//! Window-independent player; the native binary and the web page drive it.
mod player;
mod show;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;
pub use show::Show;

pub const TEN_BIT: bool = false;
pub const TITLE: &str = "Double Cross";
pub const SOUNDTRACK: &str = "SceneIntro.mp3";
