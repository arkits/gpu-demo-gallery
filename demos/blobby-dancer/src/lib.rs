//! Port of the Blobby Dancer demo. See `docs/ENGINE.md`.
pub mod blobs;
pub mod data;
pub mod demo;
pub mod fp30;
pub mod gl;
pub mod image;
mod player;
pub mod tables;
pub mod vab;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
