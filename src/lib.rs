//! Port of the Small Pond demo. See `docs/ENGINE.md`.
pub mod data;
pub mod demo;
mod frames;
pub mod gl;
pub mod image;
mod player;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
