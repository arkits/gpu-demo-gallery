//! NVIDIA's 2002–2003 program assembly (NV_vertex_program 1.1/2.0, NV_fragment_program) to
//! WGSL. The NVDemo viewers compile Cg with the shipped Cg runtime at start; the compiled
//! listings (see the port's tools) or the hand-written ones are what this reads.
pub mod fp20;
pub mod parse;
pub mod wgsl;

pub use parse::{parse, Program, Stage};
