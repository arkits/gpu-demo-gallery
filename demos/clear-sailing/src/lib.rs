//! Port of the Clear Sailing demo. See `docs/ENGINE.md`.
pub mod data;
pub mod demo;
pub mod gl;
pub mod image;
pub mod nma;
pub mod nmb;
mod player;
pub mod scene;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
