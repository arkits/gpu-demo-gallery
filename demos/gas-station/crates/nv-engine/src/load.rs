//! Loading a demo: its scene files into one world, its programs (the compiled listings in
//! `asm/`, see the port's tools), textures and render targets.

use std::collections::HashMap;

use nv_scene::Viewer as Schema;

use crate::gl::{Gl, Tex};
use crate::images;
use crate::viewer::Viewer;
use crate::world::{TextureSource, TextureState, World};

pub type Error = Box<dyn std::error::Error>;

/// The demo's installed files, by path from the demo's folder (`textures/x.dds`).
pub trait Assets {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

/// The viewers name files from their `bin` folder with backslashes (`..\textures\x.dds`).
pub fn path_of(name: &str) -> String {
    let n = name.replace('\\', "/");
    let mut parts: Vec<&str> = vec!["bin"];
    for p in n.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/")
}

/// Formats of render targets (`0x4152d0`'s table, this viewer's numbering).
pub fn target_format(format: u32, depth_formats: &[(u32, wgpu::TextureFormat)]) -> wgpu::TextureFormat {
    if let Some((_, f)) = depth_formats.iter().find(|d| d.0 == format) {
        return *f;
    }
    match format {
        19 | 23 => wgpu::TextureFormat::Rgba16Float,
        _ => wgpu::TextureFormat::Rgba8Unorm,
    }
}

pub struct LoadOptions<'a> {
    pub schema: Schema,
    /// Scene files from the demo's folder, in the order the viewer reads them.
    pub scenes: Vec<String>,
    pub window: (u32, u32),
    /// The viewer's depth formats by number.
    pub depth_formats: &'a [(u32, wgpu::TextureFormat)],
}

pub fn load(assets: &dyn Assets, gl: Gl, opts: &LoadOptions, progress: &mut dyn FnMut(&str)) -> Result<Viewer, Error> {
    let mut world = World::default();
    let mut pending = crate::world::PendingNames::default();
    for s in &opts.scenes {
        progress(s);
        let data = assets.read(s).ok_or_else(|| format!("missing {s}"))?;
        let node = nv_scene::read(&data, opts.schema).map_err(|e| format!("{s}: {e}"))?;
        world.add_scene(&node, &mut pending);
    }
    world.resolve(pending);
    let mut v = Viewer::new(world, gl);
    // Programs: the listing the shipped Cg runtime gave for (file, entry, profile).
    let index: HashMap<String, String> = {
        let d = assets.read("asm/index.json").ok_or("missing asm/index.json (see tools/nv_compile.py)")?;
        parse_index(&String::from_utf8_lossy(&d))
    };
    let profile = |p: u32| match p {
        6146 => "vp20",
        6147 => "fp20",
        6148 => "vp30",
        6149 => "fp30",
        _ => "?",
    };
    for i in 0..v.world.shaders.len() {
        let s = &v.world.shaders[i];
        let entry = if s.entry.is_empty() { "main" } else { &s.entry };
        let key = format!("{}|{}|{}", s.filename, entry, profile(s.profile));
        let Some(file) = index.get(&key) else { return Err(format!("no listing for {key}").into()) };
        let text = assets.read(&format!("asm/{file}")).ok_or_else(|| format!("missing asm/{file}"))?;
        let name = s.name.clone();
        let p = v.gl.add_program(&String::from_utf8_lossy(&text), &name)?;
        v.programs[i] = Some(p);
    }
    // Built-in programs: listings the scenes do not name (`_scr_tex.vp20` …).
    for (key, file) in &index {
        let name = key.split('|').next().unwrap_or("").replace('\\', "/");
        let base = name.rsplit('/').next().unwrap_or("");
        if !base.starts_with('_') {
            continue;
        }
        let stem = base.split('.').next().unwrap_or("").to_string();
        if v.builtins.contains_key(&stem) {
            continue;
        }
        if let Some(text) = assets.read(&format!("asm/{file}")) {
            let p = v.gl.add_program(&String::from_utf8_lossy(&text), &stem)?;
            v.builtins.insert(stem, p);
        }
    }
    // Textures.
    for i in 0..v.world.textures.len() {
        let t = v.world.textures[i].clone();
        if i % 16 == 0 {
            progress(&t.name);
        }
        let state = t.state.unwrap_or_default();
        let tex: Option<Tex> = match &t.source {
            TextureSource::File(f) => {
                let p = path_of(f);
                match assets.read(&p) {
                    Some(d) => {
                        let img = images::load_2d(&d, &p, v.gl.gpu.bc).map_err(|e| format!("{p}: {e}"))?;
                        Some(v.gl.upload(&img, state, &t.name))
                    }
                    None => None,
                }
            }
            TextureSource::Cube(faces) => {
                let mut fs = vec![];
                for f in faces {
                    let p = path_of(f);
                    fs.push((assets.read(&p).ok_or_else(|| format!("missing {p}"))?, p));
                }
                let img = images::load_cube(&fs, v.gl.gpu.bc)?;
                Some(v.gl.upload(&img, state, &t.name))
            }
            TextureSource::Proc { width, height, samples } => {
                let format = target_format(t.format, opts.depth_formats);
                let mut x = Gl::target_tex(&v.gl.gpu, *width, *height, format, if *samples > 1 { 4 } else { 1 }, 1);
                x.state = state;
                Some(x)
            }
            TextureSource::ProcCube { .. } | TextureSource::File3D(_) => None,
        };
        let id = v.gl.add_texture(tex);
        v.textures[i] = Some(id);
    }
    progress("geometry");
    v.build_meshes();
    let _ = TextureState::default();
    Ok(v)
}

/// `asm/index.json`: an object of "file|entry|profile" to listing file names.
fn parse_index(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut strings = vec![];
    let mut cur = String::new();
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if in_str {
            if esc {
                cur.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
                strings.push(std::mem::take(&mut cur));
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_str = true;
        }
    }
    for kv in strings.chunks_exact(2) {
        out.insert(kv[0].clone(), kv[1].clone());
    }
    out
}
