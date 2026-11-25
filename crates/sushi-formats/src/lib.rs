//! Strict readers for ATI Sushi 2.0 data files (Pipe Dream demo).
//!
//! Layouts were recovered from `SushiDX.exe`; the function address each reader
//! mirrors is noted beside it. Every reader must consume its input exactly.
pub mod anim;
pub mod archive;
pub mod mesh;
pub mod object;
pub mod path;
pub mod reader;
pub mod scene;
pub mod shader_header;
pub mod texture;

pub use reader::{Error, Result};
