//! Port of NVIDIA's "Zoltar" demo (2001). See `docs/ENGINE.md`.
pub mod anim;
pub mod data;
pub mod dds;
pub mod demo;
pub mod gpu;
pub mod model;
mod player;
pub mod scene;
pub mod show;
pub mod tga;
pub mod tree;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
