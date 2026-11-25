//! Reads `.ssh` shader files and turns their HLSL into WGSL.
pub mod hlsl;
mod parse;
pub mod wgsl;

pub use parse::*;
