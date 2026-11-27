//! Port of Still Life. See `docs/ENGINE.md`.
pub mod d3d;
pub mod data;
pub mod demo;
pub mod image;
pub mod kyro;
mod player;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
