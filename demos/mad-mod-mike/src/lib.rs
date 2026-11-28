//! Port of NVIDIA's "Mad Mod Mike" demo (2002). See `docs/ENGINE.md`.
pub mod anim;
mod camera_collision;
pub mod data;
pub mod dds;
pub mod demo;
mod effects;
pub mod gpu;
pub mod hair;
mod lighting;
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
