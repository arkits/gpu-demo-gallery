//! Readers for 3dfx's Arcade Tool Box (ATB) scene files, shared by the ATB
//! demo ports.
//!
//! * [`bof`]: `.BOF` scene files, the saved ATB object graph.
//! * [`glide`]: the Glide texture formats BOF pictures use, and materials.
//! * [`graph`]: walking a scene to the shapes it draws.
//! * [`tex3df`]: loose `.3DF` texture files.
//! * [`mesh3ds`]: 3D Studio `.3DS` meshes, and [`tds`] makes them ATB objects.
//!
//! Every reader refuses what it does not know and never reads past the end.

pub mod bof;
pub mod glide;
pub mod graph;
pub mod mesh3ds;
pub mod tds;
pub mod tex3df;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub use bof::{Bof, Image, Material, Object, TriSet};
pub use glide::Rgba;
pub use graph::{Item, View, walk};
pub use mesh3ds::{Mesh, Scene3ds, read as read_mesh_3ds};
pub use tex3df::{Level, Texture3df, read as read_texture_3df};
