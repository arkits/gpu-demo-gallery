//! Shared ATB runtime readers for 3dfx ATB demo ports.
//!
//! This is the pioneer crate that the other ATB-family demos build on. It holds
//! strict, bounds-checked readers for the observed formats; it does **not**
//! redistribute any 3dfx SDK binaries or demo content. See `docs/` at the repo
//! root for provenance and format notes.
//!
//! * [`bof`] — `.BOF` (`BOFM`) scene name tables.
//! * [`tex3df`] — `.3DF` (3df v1.1, rgb565) texture pyramids.
//! * [`mesh3ds`] — Autodesk `.3DS` scene meshes.
//! * [`scene`] — gathers all assets in a demo folder.
//! * [`db`] — a thin facade combining them.

pub mod bof;
pub mod db;
pub mod mesh3ds;
pub mod scene;
pub mod tex3df;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub use bof::Bof;
pub use db::Db;
pub use mesh3ds::{Mesh, Scene3ds, read as read_mesh_3ds};
pub use scene::{Scene, Source};
pub use tex3df::{Level, Texture3df, read as read_texture_3df};
