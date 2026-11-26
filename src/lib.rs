//! Port of NVIDIA's "Chameleon" demo (2001). See `docs/ENGINE.md`.
pub mod anim;
pub mod combiner;
pub mod data;
pub mod dds;
pub mod demo;
pub mod gpu;
pub mod model;
pub mod pass;
mod player;
pub mod scene;
pub mod show;
pub mod stage;
pub mod tga;
pub mod tree;
pub mod vp;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

pub type Error = Box<dyn std::error::Error>;
