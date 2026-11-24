//! Port of NVIDIA's "Box of Smoke" (2006). See `docs/ENGINE.md`.
pub mod controls;
pub mod data;
pub mod gpu;
mod player;
pub mod scene;
pub mod sim;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::{App, Player};

pub type Error = Box<dyn std::error::Error>;
