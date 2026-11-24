//! Window-independent player; the native binary and the web page drive it.
mod player;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;
