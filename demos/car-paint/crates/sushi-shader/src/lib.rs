//! Sushi surface shaders (`.ssh`): parsing, pass state and translation to WGSL.
pub mod asm;
pub mod parse;
pub mod wgsl;

pub use parse::*;
