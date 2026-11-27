//! Port of the Matrox Parhelia Reef demo. See `docs/ENGINE.md`.
pub mod d3d;
pub mod data;
pub mod demo;
pub mod image;
mod player;
pub mod script;
pub mod shader;
#[cfg(target_arch = "wasm32")]
mod web;
pub mod xfile;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
