//! Strict readers for ATI Sushi 2.0 data files as its later demos have them.
//!
//! Layouts were recovered from `SushiDX.exe`; the function address each reader
//! mirrors is noted beside it. Every reader must consume its input exactly.
pub mod anim;
pub mod archive;
pub mod mesh;
pub mod object;
pub mod particles;
pub mod path;
pub mod reader;
pub mod scene;
pub mod shader_header;
pub mod texture;
pub mod transfer;

pub use reader::{Error, Result};
