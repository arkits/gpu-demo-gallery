//! NVIDIA's Last Chance Gas (2003, GeForce FX): a gas station in the desert through a day,
//! soft shadows from precomputed rise and fall times, sky, clouds and HDR glow.
// Constants are the program's own (1.442695 is its log2(e), not std's).
#![allow(clippy::approx_constant, clippy::excessive_precision)]
pub mod args;
pub mod clouds;
pub mod demo;
pub mod files;
mod player;
mod present;
pub mod ui;
pub mod walk;
#[cfg(target_arch = "wasm32")]
mod web;

pub use player::Player;

use nv_engine::gl::{Gl, Gpu};
use nv_engine::load::{self, Assets};
use nv_engine::viewer::Viewer;

pub type Error = Box<dyn std::error::Error>;

pub const TITLE: &str = "Last Chance Gas";
/// Both play from the start, over and over, at full volume (`0x4083b0`).
pub const SOUNDS: [&str; 2] = ["GasBMG3", "Windy2m2"];
/// The viewer's depth formats (`0x4152d0`): 14 is 16-bit, 15 is 24-bit.
pub const DEPTH_FORMATS: &[(u32, wgpu::TextureFormat)] =
    &[(14, wgpu::TextureFormat::Depth16Unorm), (15, wgpu::TextureFormat::Depth24Plus)];

/// Loads the scenes and starts the demo; `seed` replaces the clock for the clouds.
pub fn start(assets: &dyn Assets, gpu: Gpu, seed: Option<u32>, progress: &mut dyn FnMut(&str)) -> Result<(Viewer, demo::Demo), Error> {
    let text = assets.read("bin/args.txt").ok_or("missing bin/args.txt")?;
    let args = args::parse(&String::from_utf8_lossy(&text)).ok_or("bin/args.txt is not readable")?;
    let scenes = args.scenes.iter().map(|s| load::path_of(s)).collect();
    let gl = Gl::new(gpu, args.window);
    let opts = load::LoadOptions { schema: nv_scene::Viewer::GasStation, scenes, window: args.window, depth_formats: DEPTH_FORMATS };
    let mut v = load::load(assets, gl, &opts, progress)?;
    let mut demo = demo::Demo::with_seed(&mut v, seed);
    // Without the map, walking is free (0x402bf0).
    if let Some(c) = assets.read("textures/collision1.tga") {
        demo.set_collision(&c);
    }
    Ok((v, demo))
}
