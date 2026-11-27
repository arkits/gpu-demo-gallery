//! Port of ATI's Radeon's Ark demo. See `docs/ENGINE.md`.
pub mod ati;
pub mod d3d;
pub mod data;
pub mod demo;
pub mod image;
pub mod path;
mod player;
pub mod proteus;
pub mod water;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
