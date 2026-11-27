//! Window-independent player; the native binary and the web page drive it.
pub mod music;
mod player;
mod show;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;
pub use show::Show;

/// `Sushi.ini` sets no `backBufferColorDepth`, so the back buffer has eight bits.
pub const TEN_BIT: bool = false;
pub const TITLE: &str = "Subsurface Scattering";
/// `Audio/SceneIntro.wav`, then `Audio/SceneLoop.wav` repeating.
pub const INTRO: &str = "SceneIntro.wav";
pub const LOOP: &str = "SceneLoop.wav";
/// `audioIntroOverlap` in the archive's `Sushi2.ini`.
pub const OVERLAP: f64 = 0.0;
