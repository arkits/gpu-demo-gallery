//! The NVDemo framework of NVIDIA's 2003 demo viewers, ported: scenes, transforms,
//! animation, cameras, lights, shader parameters, render passes and render lists, drawn
//! with wgpu through a small OpenGL-like state layer.
pub mod anim;
pub mod gl;
pub mod images;
pub mod load;
pub mod math;
pub mod menu;
pub mod sort;
pub mod viewer;
pub mod world;
