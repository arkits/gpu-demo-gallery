//! Clear Sailing (`PirateShip.exe`, 2004): a pirate ship at sunset with a choppy
//! ocean, rope rigging, cannon smoke and an HDR pipeline. See `docs/ENGINE.md`.
use std::collections::BTreeMap;

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::Error;
use crate::data::Files;
use crate::gl::{self, Depth, Gl, Gpu, State, Surface, Vertex, from_opengl, look_at, perspective};
use crate::nma;
use crate::nmb::{self, Node};
use crate::scene;

/// Files that are parts of a program: none, the demo reads loose files.
pub const PARTS: &[(&str, usize, usize)] = &[];
/// Sounds made into FLAC for the web; the small ones ship in the packs.
pub const SOUNDS: &[&str] = &["ambient.wav", "soundtrack.wav"];
pub const FILES: &[&str] = &[
    "ps_03_19.nmb",
    "sail_animation.nmb",
    "foamy_ocean.nma",
    "hdr.nma",
    "menu.nma",
    "pirates.nma",
    "reflection.nma",
    "ropes.nma",
    "smoke.nma",
    "args.txt",
    "sliders.txt",
    "about.dds",
    "about_box.dds",
    "actions.dds",
    "camera.dds",
    "cannon_door_mizzen_bump_norm.tga",
    "cannon_door_mizzen_color.dds",
    "clouds.tga",
    "clouds_norm.tga",
    "cloudy_sunset_vcross_spec100.tga",
    "drawstyle.dds",
    "frontmainleft1_bmp_norm.tga",
    "frontmainleft1_color.dds",
    "froth3.tga",
    "genericflag_bmp_norm.tga",
    "genericflag_color.dds",
    "hull_backmainleft1_bmp_norm.tga",
    "hull_backmainleft1_color.dds",
    "hull_midmainleft1_color.dds",
    "hull_midmainleft_bmp_norm.tga",
    "knob.dds",
    "light.dds",
    "logo.dds",
    "mount_fore_shipswheel_bmp_norm.tga",
    "mount_fore_shipswheel_color.dds",
    "navigation.dds",
    "new_cloudy_sunset_xneg.dds",
    "new_cloudy_sunset_xpos.dds",
    "new_cloudy_sunset_yneg.dds",
    "new_cloudy_sunset_ypos.dds",
    "new_cloudy_sunset_zneg.dds",
    "new_cloudy_sunset_zpos.dds",
    "pause.dds",
    "quit.dds",
    "returnfire.dds",
    "ribs_front_bmp_norm.tga",
    "ribs_front_color.dds",
    "rudder_bmp_norm.tga",
    "rudder_color.dds",
    "shaded.dds",
    "ship_anchor_bmp_norm.tga",
    "ship_anchor_color.dds",
    "ship_barrels_bmp_norm.tga",
    "ship_barrels_color.dds",
    "ship_cannon_door_mizzen_color.dds",
    "ship_cannons_right_bmp_norm.tga",
    "ship_cannons_right_color.dds",
    "ship_capstan_belfry_bmp_norm.tga",
    "ship_capstan_belfry_color.dds",
    "ship_deck_maina_bmp_norm.tga",
    "ship_deck_maina_color.dds",
    "ship_deck_mainb_bmp_norm.tga",
    "ship_deck_mainb_color.dds",
    "ship_deck_mainc_bmp_norm.tga",
    "ship_deck_mainc_color.dds",
    "ship_deck_rear_low_bmp_norm.tga",
    "ship_deck_rear_low_color.dds",
    "ship_deck_rear_top_bmp_norm.tga",
    "ship_deck_rear_top_color.dds",
    "ship_flag_mizzen_bmp_norm.tga",
    "ship_flag_mizzen_color.dds",
    "ship_flag_sword_bmp_norm.tga",
    "ship_flag_sword_color.dds",
    "ship_fore_right_bmp_norm.tga",
    "ship_fore_right_color.dds",
    "ship_hold_main_bmp_norm.tga",
    "ship_hold_main_color.dds",
    "ship_hold_rear_bmp_norm.tga",
    "ship_hold_rear_color.dds",
    "ship_lantern_bmp_norm.tga",
    "ship_lantern_color.dds",
    "ship_lifeboat_bmp_norm.tga",
    "ship_lifeboat_color.dds",
    "ship_mast_fore3_bmp_norm.tga",
    "ship_mast_fore3_color.dds",
    "ship_mast_jib_bmp_norm.tga",
    "ship_mast_jib_color.dds",
    "ship_mast_main_bmp_norm.tga",
    "ship_mast_main_color.dds",
    "ship_mast_mizzen_bmp_norm.tga",
    "ship_mast_mizzen_color.dds",
    "ship_masthead_fore_bmp_norm.tga",
    "ship_masthead_fore_color.dds",
    "ship_masthead_rear_bmp_norm.tga",
    "ship_masthead_rear_color.dds",
    "ship_rail_aft_bmp_norm.tga",
    "ship_rail_aft_color.dds",
    "ship_rail_mizzen_bmp_norm.tga",
    "ship_rail_mizzen_color.dds",
    "ship_rails_main_bmp_norm.tga",
    "ship_rails_main_color.dds",
    "ship_rig_cannon_rear_left_bmp_norm.tga",
    "ship_rig_cannon_rear_left_color.dds",
    "ship_sail_fore_course_bmp_norm.tga",
    "ship_sail_fore_course_color.dds",
    "ship_sail_fore_gallant_bmp_norm.tga",
    "ship_sail_fore_gallant_color.dds",
    "ship_sail_fore_top_bmp_norm.tga",
    "ship_sail_fore_top_color.dds",
    "ship_sail_fore_tri_low_bmp_norm.tga",
    "ship_sail_fore_tri_low_color.dds",
    "ship_sail_fore_tri_up_bmp_norm.tga",
    "ship_sail_fore_tri_up_color.dds",
    "ship_sail_jib_low_bmp_norm.tga",
    "ship_sail_jib_low_color.dds",
    "ship_sail_jib_up_bmp_norm.tga",
    "ship_sail_jib_up_color.dds",
    "ship_sail_main_course_bmp_norm.tga",
    "ship_sail_main_course_color.dds",
    "ship_sail_main_gallant_bmp_norm.tga",
    "ship_sail_main_gallant_color.dds",
    "ship_sail_main_top_bmp_norm.tga",
    "ship_sail_main_top_color.dds",
    "ship_sail_main_tri_bmp_norm.tga",
    "ship_sail_main_tri_color.dds",
    "ship_sail_mizzen_gallant_bmp_norm.tga",
    "ship_sail_mizzen_gallant_color.dds",
    "ship_sail_mizzen_tri_bmp_norm.tga",
    "ship_sail_mizzen_tri_color.dds",
    "ship_side_ribs_right_bmp_norm.tga",
    "ship_side_ribs_right_color.dds",
    "ship_stairs_mizzen_bmp_norm.tga",
    "ship_stairs_mizzen_color.dds",
    "ship_supports_left_bmp_norm.tga",
    "ship_supports_left_color.dds",
    "ship_windows_left_bmp_norm.tga",
    "ship_windows_left_color.dds",
    "smoke.vol",
    "smoke040.tga",
    "stair_aft_left_bmp_norm.tga",
    "stair_aft_left_color.dds",
    "underattack.dds",
    "wireframe.dds",
    "far_cannon0.wav",
    "far_cannon1.wav",
    "far_cannon2.wav",
    "far_cannon3.wav",
    "near_cannon0.wav",
    "near_cannon1.wav",
    "near_cannon2.wav",
    "near_cannon3.wav",
    "ship_impact0.wav",
    "ship_impact1.wav",
    "ship_impact2.wav",
    "ship_impact3.wav",
    "ship_impact4.wav",
    "ship_impact5.wav",
    "ship_impact6.wav",
    "water_impact0.wav",
    "water_impact1.wav",
    "water_impact2.wav",
    "water_impact3.wav",
];
const SHADER: &str = include_str!("../shaders/demo.wgsl");
pub const PROGRAMS: &[&str] = &[
    "sky", "ocean", "ship", "sail", "rope", "smoke", "spray", "bright", "blur", "comp", "ui",
];
pub const BUTTONS: &[(&str, &str)] = &[("f", "Fire"), ("h", "HDR"), ("u", "Menus"), (" ", "Pause")];

const SKY: usize = 0;
const OCEAN: usize = 1;
const SHIP: usize = 2;
const SAIL: usize = 3;
const ROPE: usize = 4;
const SMOKE: usize = 5;
const SPRAY: usize = 6;
const BRIGHT: usize = 7;
const BLUR: usize = 8;
const COMP: usize = 9;
const UI: usize = 10;

/// The shader: hand-written WGSL, no translation at startup.
pub fn shader(_files: &Files) -> Result<String, Error> {
    Ok(SHADER.to_string())
}

/// One rigid mesh of the ship, baked to world space in a GPU buffer.
struct Part {
    mesh: usize,
    tris: usize,
    color: usize,
    bump: usize,
    sail: Option<usize>,
    sail_cpu: Option<SailCpu>,
}

/// Base mesh of a morphing sail for the CPU interpolation.
struct SailCpu {
    tris: Vec<[usize; 3]>,
    uvs: Vec<Vec2>,
    world: Mat4,
}

/// One morphing sail: absolute targets in ship space.
struct Sail {
    times: Vec<f32>,
    pos: Vec<Vec<Vec3>>,
}

struct Particle {
    at: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    spray: bool,
}

/// Settings from `args.txt` and `sliders.txt`.
struct Settings {
    hdr: bool,
    scene: (u32, u32),
    reflection: (u32, u32),
    sliders: BTreeMap<String, f32>,
    #[allow(dead_code)]
    order: Vec<String>,
}

impl Settings {
    fn get(&self, name: &str) -> f32 {
        self.sliders.get(name).copied().unwrap_or(0.0)
    }
}

/// Sliders changed with `v`, `-` and `=`.
const FEATURED: &[&str] = &[
    "amplitude_slider",
    "displacement_slider",
    "foaminess_slider",
    "time_slider",
    "water_dampening_slider",
    "smoke_size_slider",
    "smoke_age_slider",
    "fog_near",
];

pub struct Demo {
    settings: Settings,
    parts: Vec<Part>,
    sails: Vec<usize>,
    sail_anims: Vec<Sail>,
    ropes: usize,
    rope_count: usize,
    splinters: Vec<(usize, usize)>,
    sky: usize,
    smoke_sprite: usize,
    foam: usize,
    turbulence: usize,
    logo: usize,
    actions: usize,
    ocean_waves: Vec<(Vec2, f32, f32, f32)>,
    ocean_size: f32,
    ocean_tess: usize,
    bow: f32,
    stern: f32,
    bounds: (Vec3, f32),
    smoke: Vec<Particle>,
    time: f32,
    sail_scale: f32,
    paused: bool,
    hdr: bool,
    ui: bool,
    rotate: bool,
    yaw: f32,
    pitch: f32,
    dist: f32,
    at: Vec3,
    last: Option<(f64, f64)>,
    slider_at: usize,
    fire_cool: f32,
    fire_side: f32,
    burst: f32,
    cues: Vec<String>,
    frames: u64,
    tris: usize,
    refl: usize,
    scene_target: usize,
    bright: usize,
    blur_a: usize,
    blur_b: usize,
    rand: u32,
}

fn find<'a>(n: &'a Node, kind: &str, name: &str) -> Option<&'a Node> {
    if n.kind == kind && n.name == name {
        return Some(n);
    }
    for f in n.fields.values() {
        match f {
            nmb::Value::Node(m) => {
                if let Some(r) = find(m, kind, name) {
                    return Some(r);
                }
            }
            nmb::Value::Nodes(ms) => {
                for m in ms {
                    if let Some(r) = find(m, kind, name) {
                        return Some(r);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// A texture by its scene name, trying the demo's naming habits.
fn resolve(files: &Files, name: &str) -> Result<String, Error> {
    let base = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let lower = base.to_ascii_lowercase();
    if files.get(&lower).is_ok() {
        return Ok(lower);
    }
    for alt in [
        format!("{lower}.dds"),
        format!("{lower}.tga"),
        format!("{lower}_norm.tga"),
        format!("{lower}_color.dds"),
    ] {
        if files.get(&alt).is_ok() {
            return Ok(alt);
        }
    }
    Err(format!("missing texture {name}").into())
}

fn settings(files: &Files) -> Result<Settings, Error> {
    // `args.txt`: window, fullscreen, HDR, scene and reflection sizes, then files.
    let args = std::str::from_utf8(files.get("args.txt")?)?;
    let stripped: Vec<&str> = args
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect();
    let mut at_line = 0;
    let mut next = || {
        let line = stripped.get(at_line).unwrap_or(&"").to_string();
        at_line += 1;
        line
    };
    let nums = |line: &str, n: usize| {
        line.split_whitespace()
            .filter_map(|w| w.parse::<u32>().ok())
            .take(n)
            .collect::<Vec<_>>()
    };
    let window = nums(&next(), 3);
    let fullscreen = next().contains("true");
    let hdr = next().contains("true");
    let scene = nums(&next(), 3);
    let reflection = nums(&next(), 3);
    let _ = (window, fullscreen);
    let sliders = std::str::from_utf8(files.get("sliders.txt")?)?;
    let mut map = BTreeMap::new();
    let mut order = Vec::new();
    for line in sliders.lines() {
        let mut w = line.split_whitespace();
        if let (Some(name), Some(value)) = (w.next(), w.next())
            && let Ok(v) = value.parse::<f32>()
        {
            map.insert(name.to_owned(), v);
            order.push(name.to_owned());
        }
    }
    let at = |v: &[u32], i: usize, d: u32| v.get(i).copied().unwrap_or(d);
    Ok(Settings {
        hdr,
        scene: (at(&scene, 0, 1024), at(&scene, 1, 512)),
        reflection: (at(&reflection, 0, 512), at(&reflection, 1, 256)),
        sliders: map,
        order,
    })
}

/// World-baked, non-indexed triangles in a GPU buffer. The motion shader
/// moves them rigidly; normals and frames are baked rotated already.
fn bake(gl: &mut Gl, gpu: &Gpu, mesh: &scene::MeshData, world: &Mat4) -> (usize, usize) {
    let rv = |v: Vec3| {
        let r = world.transform_vector3(v);
        if r.length_squared() > 1e-12 {
            r.normalize()
        } else {
            r
        }
    };
    let mut verts = Vec::with_capacity(mesh.idx.len());
    for i in mesh.idx.iter() {
        let i = *i as usize;
        let w = world.transform_point3(mesh.pos[i]);
        let (n, t, b) = (rv(mesh.nor[i]), rv(mesh.tan[i]), rv(mesh.bin[i]));
        verts.push(Vertex {
            position: [w.x, w.y, w.z, 1.0],
            color: [mesh.uv[i].x, mesh.uv[i].y, 1.0, 1.0],
            uv: [
                [w.x, w.y, w.z, 1.0],
                [n.x, n.y, n.z, 0.0],
                [t.x, t.y, t.z, 0.0],
                [b.x, b.y, b.z, 0.0],
            ],
        });
    }
    let tris = verts.len() / 3;
    (gl.mesh(gpu, &verts), tris)
}
impl Demo {
    /// Rigid motion of the whole ship: heave, pitch and roll.
    fn motion(&self) -> Mat4 {
        let t = self.time;
        let damp = 1.0 - 0.6 * self.slider("water_dampening_slider");
        let mass = 0.5 + self.slider("mass_slider");
        let heave = 3.0 * damp * (t * 0.5).sin();
        let pitch = 0.022 * damp * mass * (t * 0.43 + 1.0).sin();
        let roll = 0.030 * damp * mass * (t * 0.37).sin();
        let center = Vec3::new(
            0.0,
            self.slider("center_of_mass_y_slider") * 100.0 - 50.0,
            self.slider("center_of_mass_z_slider") * 100.0 - 50.0,
        );
        Mat4::from_translation(center + Vec3::new(0.0, heave, 0.0))
            * Mat4::from_rotation_x(pitch)
            * Mat4::from_rotation_z(roll)
            * Mat4::from_translation(-center)
    }

    fn eye(&self) -> Vec3 {
        self.at
            + Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.cos() * self.pitch.cos(),
            ) * self.dist
    }

    fn camera(&self, aspect: f32, mirror: bool) -> (Vec3, Mat4, Mat4) {
        let eye = self.eye();
        let (eye, at, up) = match mirror {
            false => (eye, self.at, Vec3::Y),
            true => (
                Vec3::new(eye.x, -eye.y, eye.z),
                Vec3::new(self.at.x, -self.at.y, self.at.z),
                -Vec3::Y,
            ),
        };
        let view = look_at(eye, at, up);
        let proj = from_opengl(perspective(54.0, aspect, 1.0, 100000.0));
        (eye, view, proj)
    }

    fn base_surface(&self, eye: Vec3, refl: Mat4) -> Surface {
        Surface::default()
            .param(0, [eye.x, eye.y, eye.z, f32::from(self.hdr)])
            .param(1, [-0.6, 0.18, 0.45, self.slider("sunLight") * 3.0 + 0.5])
            .param(
                2,
                [
                    self.slider("water_fog_red"),
                    self.slider("water_fog_green"),
                    self.slider("water_fog_blue"),
                    self.slider("water_fog_amount"),
                ],
            )
            .param(
                3,
                [
                    self.slider("fog_near") * 4000.0,
                    self.slider("fog_far") * 8000.0 + 100.0,
                    self.time,
                    self.slider("foaminess_slider") * 2.0,
                ],
            )
            .param(
                4,
                [
                    self.slider("sunLight") * 2.0 + 0.3,
                    self.slider("cubemapLight") * 2.0,
                    self.slider("waterLight") * 2.0,
                    self.slider("ambientLight") * 2.0 + 0.2,
                ],
            )
            .param(
                5,
                [
                    self.slider("sailsSunLight") * 2.0 + 0.3,
                    self.slider("sailsCubemapLight") * 2.0,
                    self.slider("sailsWaterLight") * 2.0,
                    self.slider("sailsAmbientLight") * 2.0 + 0.2,
                ],
            )
            .param(6, [0.75, 0.72, 0.70, 0.85])
            .param(
                7,
                [
                    self.slider("ropeColor") + 0.15,
                    self.slider("ropeColor") + 0.15,
                    self.slider("ropeColor") + 0.15,
                    0.9,
                ],
            )
            .param(8, refl.row(0).to_array())
            .param(9, refl.row(1).to_array())
            .param(10, refl.row(2).to_array())
            .param(11, refl.row(3).to_array())
    }

    /// Surface for the rigid meshes: the view and the ship's motion.
    fn moved(&self, base: &Surface, view_proj: Mat4, motion: Mat4) -> Surface {
        let base: Surface = *base;
        base.param(12, view_proj.row(0).to_array())
            .param(13, view_proj.row(1).to_array())
            .param(14, view_proj.row(2).to_array())
            .param(15, view_proj.row(3).to_array())
            .param(16, motion.row(0).to_array())
            .param(17, motion.row(1).to_array())
            .param(18, motion.row(2).to_array())
    }

    pub fn new(gpu: &Gpu, gl: &mut Gl, files: &Files) -> Result<Self, Error> {
        let settings = settings(files)?;
        // Ship: hierarchy, meshes, and the shapes of the main pass.
        let ship = nmb::read_binary(files.get("ps_03_19.nmb")?)?;
        let mut worlds = BTreeMap::new();
        scene::worlds(&ship, &mut worlds)?;
        let mut meshes = BTreeMap::new();
        for m in ship.nodes("geometries") {
            if m.kind == "IndexedFaceSet" {
                meshes.insert(m.name.clone(), scene::mesh(m)?);
            }
        }
        // Sails: morph targets by mesh name.
        let sail_file = nmb::read_binary(files.get("sail_animation.nmb")?)?;
        let mut sail_anims: BTreeMap<String, Sail> = BTreeMap::new();
        let mut sail_scale = 1.0;
        for a in sail_file.nodes("animations") {
            sail_scale = a.float("timeScale");
            for c in a.nodes("channels") {
                if c.kind != "GeometryChannel" {
                    continue;
                }
                let mut times = Vec::new();
                let mut pos = Vec::new();
                for k in c.nodes("keyframes") {
                    times.push(k.float("time"));
                    let coord = k.node("coord").ok_or("keyframe without mesh")?;
                    pos.push(
                        coord
                            .floats("value")
                            .chunks_exact(3)
                            .map(|p| Vec3::new(p[0], p[1], p[2]))
                            .collect(),
                    );
                }
                if !times.is_empty() {
                    sail_anims.insert(c.text("target").to_owned(), Sail { times, pos });
                }
            }
        }
        let mut seen_textures = BTreeMap::new();
        let mut tex = |gl: &mut Gl, name: &str| -> usize {
            let key = name.to_ascii_lowercase();
            if let Some(id) = seen_textures.get(&key) {
                return *id;
            }
            let id = resolve(files, name)
                .ok()
                .and_then(|n| files.image(&n).ok())
                .map(|image| gl.texture(gpu, &image, crate::gl::REPEAT))
                .unwrap_or(gl::WHITE);
            seen_textures.insert(key, id);
            id
        };
        // Sky cube and shared pictures from the ocean scene.
        let ocean_scene = nma::read_text(files.get("foamy_ocean.nma")?)?;
        let sky = {
            let bg = ocean_scene
                .nodes("textures")
                .iter()
                .find(|t| t.kind == "CubicTexture" && t.name.ends_with("bgTex"))
                .ok_or("no sky")?;
            let faces = ["xpos", "xneg", "ypos", "yneg", "zpos", "zneg"].map(|f| {
                let name = resolve(files, bg.text(f))?;
                files.image(&name)
            });
            let faces: [crate::image::Image; 6] = faces
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| "six sky faces")?;
            gl.cube(gpu, &faces)
        };
        let pic = |gl: &mut Gl, name: &str| -> usize {
            resolve(files, name)
                .ok()
                .and_then(|n| files.image(&n).ok())
                .map(|image| gl.texture(gpu, &image, crate::gl::REPEAT))
                .unwrap_or(gl::WHITE)
        };
        let ocean_tex = |kind: &str| {
            ocean_scene
                .nodes("textures")
                .iter()
                .find(|t| t.kind == kind)
                .map(|t| t.text("filename").to_owned())
                .unwrap_or_default()
        };
        let turbulence = pic(gl, &ocean_tex("turbulenceTex"));
        let foam = pic(gl, "froth3.tga");
        let smoke_sprite = pic(gl, "smoke040.tga");
        let logo = pic(gl, "logo.dds");
        let actions = pic(gl, "actions.dds");
        // Parts: every shape of the main group with its world matrix and pictures.
        let group = find(&ship, "Group", "nvPrimaryRenderPass_group").ok_or("no main group")?;
        let mut parts = Vec::new();
        let mut sails = Vec::new();
        let mut sail_list: Vec<Sail> = Vec::new();
        let mut sail_index: BTreeMap<String, usize> = BTreeMap::new();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for shape in group.nodes("children") {
            if shape.kind != "Shape" {
                continue;
            }
            let world = worlds
                .get(shape.text("transform"))
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            let mut color = gl::WHITE;
            let mut bump = gl::WHITE;
            if let Some(rs) = shape.nodes("renderSettings").first() {
                for p in rs.nodes("fragmentShaderParams") {
                    let (name, file) = (p.text("paramName"), p.text("texture"));
                    if file.is_empty() {
                        continue;
                    }
                    if name == "colorTex" {
                        color = tex(gl, file);
                    } else if name == "bumpTex" {
                        bump = tex(gl, &format!("{file}_norm"));
                    }
                }
            }
            for g in shape.texts("geometries") {
                let mesh = meshes.remove(g.as_str()).ok_or(format!("no mesh {g}"))?;
                for v in &mesh.pos {
                    let w = world.transform_point3(*v);
                    lo = lo.min(w);
                    hi = hi.max(w);
                }
                let sail = match sail_anims.remove(g) {
                    Some(s) => {
                        let id = *sail_index.entry(g.to_owned()).or_insert_with(|| {
                            sail_list.push(s);
                            sail_list.len() - 1
                        });
                        Some(id)
                    }
                    None => None,
                };
                let sail_cpu = match sail {
                    Some(_) => {
                        let tris = mesh
                            .idx
                            .chunks_exact(3)
                            .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
                            .collect();
                        Some(SailCpu {
                            tris,
                            uvs: mesh.uv.clone(),
                            world,
                        })
                    }
                    None => None,
                };
                if sail.is_some() {
                    sails.push(parts.len());
                }
                let (mesh_id, tris) = bake(gl, gpu, &mesh, &world);
                parts.push(Part {
                    mesh: mesh_id,
                    tris,
                    color,
                    bump,
                    sail,
                    sail_cpu,
                });
            }
        }
        // Ropes: static line strips in ship space.
        let ropes_nma = nma::read_text(files.get("ropes.nma")?)?;
        let mut rworlds = BTreeMap::new();
        scene::tworlds(&ropes_nma, &mut rworlds)?;
        let mut rope_verts = Vec::new();
        for shape in ropes_nma.nodes("shapes") {
            let world = rworlds
                .get(shape.text("transform"))
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            for g in shape.strs("geometries") {
                let mesh = ropes_nma
                    .nodes("geometries")
                    .iter()
                    .find(|m| m.name == g)
                    .ok_or(format!("no rope {g}"))?;
                let data = scene::tmesh(mesh)?;
                for w in data.idx.chunks_exact(2) {
                    for i in w {
                        let p = world.transform_point3(data.pos[*i as usize]);
                        lo = lo.min(p);
                        hi = hi.max(p);
                        rope_verts.push(Vertex {
                            position: [p.x, p.y, p.z, 1.0],
                            color: [0.35, 0.35, 0.35, 1.0],
                            uv: [[0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]],
                        });
                    }
                }
            }
        }
        let ropes = gl.mesh(gpu, &rope_verts);
        let rope_count = rope_verts.len() as u32;
        // Splinters: white debris meshes from the pirates scene.
        let pirates = nma::read_text(files.get("pirates.nma")?)?;
        let mut pworlds = BTreeMap::new();
        scene::tworlds(&pirates, &mut pworlds)?;
        let mut splinters = Vec::new();
        for shape in pirates.nodes("shapes") {
            if !shape.name.starts_with("Splinters") {
                continue;
            }
            let world = pworlds
                .get(shape.text("transform"))
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            for g in shape.strs("geometries") {
                let mesh = pirates
                    .nodes("geometries")
                    .iter()
                    .find(|m| m.name == g)
                    .ok_or(format!("no splinter {g}"))?;
                let (id, tris) = bake(gl, gpu, &scene::tmesh(mesh)?, &world);
                splinters.push((id, tris));
            }
        }
        // Ocean: grid and waves from the foamy ocean scene.
        let ocean = ocean_scene
            .nodes("geometries")
            .iter()
            .find(|g| g.kind == "Ocean")
            .ok_or("no ocean")?;
        let num = |f: &str, i: usize| ocean.num(f, i) as f32;
        let ocean_tess = num("tesselation", 0).clamp(32.0, 160.0) as usize;
        let mut ocean_waves = Vec::new();
        let mut rng = 1234567u32;
        let mut rand = || {
            rng = rng.wrapping_mul(214_013).wrapping_add(2_531_011);
            (rng >> 16) as f32 / 32768.0 - 0.5
        };
        for k in 0..8 {
            let f = k as f32 / 7.0;
            let len = 20.0 * (30.0f32).powf(f);
            let dir = Vec2::new(rand(), rand()).normalize_or_zero();
            ocean_waves.push((
                dir,
                len,
                0.6 + 0.8 * rand().abs(),
                rand() * std::f32::consts::TAU,
            ));
        }
        let center = (lo + hi) * 0.5;
        let radius = (hi - lo).length() * 0.5;
        let at = Vec3::new(center.x, center.y * 0.35, center.z);
        // Render targets: reflection, scene, and the HDR chain.
        let (rw, rh) = settings.reflection;
        let (sw, sh) = settings.scene;
        let refl = gl.target(gpu, (rw.max(1), rh.max(1)), crate::gl::CLAMP);
        let scene_target = gl.target(gpu, (sw.max(1), sh.max(1)), crate::gl::CLAMP);
        let bright = gl.target(gpu, (sw.max(2) / 2, sh.max(2) / 2), crate::gl::CLAMP);
        let blur_a = gl.target(gpu, (sw.max(2) / 2, sh.max(2) / 2), crate::gl::CLAMP);
        let blur_b = gl.target(gpu, (sw.max(2) / 2, sh.max(2) / 2), crate::gl::CLAMP);
        let hdr_on = settings.hdr;
        Ok(Self {
            settings,
            parts,
            sails,
            sail_anims: sail_list,
            ropes,
            rope_count: rope_count as usize,
            splinters,
            sky,
            smoke_sprite,
            foam,
            turbulence,
            logo,
            actions,
            ocean_waves,
            ocean_size: 2400.0,
            ocean_tess,
            bow: hi.z,
            stern: lo.z,
            bounds: (at, radius),
            smoke: Vec::new(),
            time: 0.0,
            sail_scale,
            paused: false,
            hdr: hdr_on,
            ui: true,
            rotate: true,
            yaw: 0.7,
            pitch: 0.14,
            dist: radius * 1.5,
            at,
            last: None,
            slider_at: 0,
            fire_cool: 0.0,
            fire_side: 1.0,
            burst: 0.0,
            cues: vec!["ambient*".into(), "soundtrack*".into()],
            frames: 0,
            tris: 0,
            refl,
            scene_target,
            bright,
            blur_a,
            blur_b,
            rand: 0x12345678,
        })
    }
}
impl Demo {
    fn slider(&self, name: &str) -> f32 {
        self.settings.get(name)
    }

    fn rand(&mut self) -> f32 {
        self.rand = self.rand.wrapping_mul(214_013).wrapping_add(2_531_011);
        (self.rand >> 16) as f32 / 32768.0 - 0.5
    }

    fn emit_smoke(&mut self, at: Vec3, vel: Vec3, spray: bool) {
        let life = match spray {
            true => 0.9 + self.rand().abs(),
            false => 1.5 + self.slider("smoke_age_slider") * 3.0,
        };
        let size = match spray {
            true => 3.0 + self.rand().abs() * 6.0,
            false => 6.0 + self.slider("smoke_size_slider") * 60.0,
        };
        self.smoke.push(Particle {
            at,
            vel,
            age: 0.0,
            life,
            size,
            spray,
        });
        if self.smoke.len() > 2400 {
            self.smoke.remove(0);
        }
    }

    fn fire(&mut self) {
        if self.fire_cool > 0.0 {
            return;
        }
        self.fire_cool = 0.6;
        self.fire_side = -self.fire_side;
        let (_, radius) = self.bounds;
        let guns = 4;
        for k in 0..guns {
            let z = -radius * 0.3 + 2.0 * radius * 0.3 * k as f32 / (guns - 1) as f32;
            let at = Vec3::new(self.fire_side * radius * 0.32, 26.0, z);
            let motion = self.motion();
            let at = motion.transform_point3(at);
            for _ in 0..8 {
                let (r1, r2, r3) = (self.rand(), self.rand(), self.rand());
                let (r4, r5, r6) = (self.rand(), self.rand(), self.rand());
                self.emit_smoke(
                    at + Vec3::new(r1 * 4.0, r2 * 4.0, r3 * 4.0),
                    Vec3::new(
                        self.fire_side * (20.0 + r4.abs() * 20.0),
                        4.0 + r5.abs() * 6.0,
                        r6 * 8.0,
                    ),
                    false,
                );
            }
            for _ in 0..4 {
                let (r1, r2) = (self.rand(), self.rand());
                self.emit_smoke(
                    Vec3::new(at.x * 1.6, 1.0, at.z + r1 * 10.0),
                    Vec3::new(self.fire_side * 10.0, 8.0 + r2.abs() * 8.0, 0.0),
                    true,
                );
            }
        }
        self.cues.push("far_cannon0".into());
        self.cues.push("near_cannon1".into());
        self.cues.push("ship_impact2".into());
        self.burst = 1.0;
    }

    pub fn step(&mut self, dt: f64) {
        let dt = dt.clamp(0.0, 0.1) as f32;
        if self.rotate && !self.paused {
            self.yaw += dt * 0.05;
        }
        self.fire_cool = (self.fire_cool - dt).max(0.0);
        self.burst = (self.burst - dt * 1.2).max(0.0);
        if self.paused {
            return;
        }
        self.time += dt;
        // Continuous smoke from the ship, and spray at the bow.
        if self.smoke.iter().filter(|p| !p.spray).count() < 900 && self.rand() > -0.2 {
            let motion = self.motion();
            let stack = motion.transform_point3(Vec3::new(6.0, 60.0, -20.0));
            let (r1, r2, r3) = (self.rand(), self.rand(), self.rand());
            self.emit_smoke(
                stack,
                Vec3::new(2.0 + r1 * 2.0, 7.0 + r2 * 3.0, r3 * 2.0),
                false,
            );
        }
        if self.rand() > 0.7 {
            let motion = self.motion();
            let bow = motion.transform_point3(Vec3::new(0.0, 1.0, self.bow - 8.0));
            let (r1, r2, r3) = (self.rand(), self.rand(), self.rand());
            self.emit_smoke(
                bow + Vec3::new(r1 * 14.0, 0.0, r2 * 6.0),
                Vec3::new(r3 * 4.0, 3.0 + r3.abs() * 4.0, -4.0),
                true,
            );
        }
        let damp = (1.0 - 0.5 * self.slider("smoke_damping_slider")).clamp(0.2, 1.0);
        let wind = self.slider("smoke_wind_slider") * 40.0;
        let grow = self.slider("smoke_grow_slider") * 60.0;
        let mut jr = self.rand;
        for p in &mut self.smoke {
            jr = jr.wrapping_mul(214_013).wrapping_add(2_531_011);
            let j = (jr >> 16) as f32 / 32768.0 - 0.5;
            p.age += dt;
            p.vel.y *= damp.powf(dt * 3.0);
            p.vel.x += (wind + j * 2.0) * dt;
            p.at += p.vel * dt;
            p.size += grow * dt * (1.0 - p.age / p.life).max(0.0);
        }
        self.rand = jr;
        self.smoke.retain(|p| p.age < p.life);
    }

    /// Goes to a moment and stays there, for checking frames.
    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.time = 0.0;
        self.smoke.clear();
        self.cues.clear();
        for c in keys.chars() {
            self.key(&c.to_string(), true);
        }
        let mut t = 0.0;
        while t < seconds as f32 {
            self.step(1.0 / 60.0);
            t += 1.0 / 60.0;
        }
        for c in keys.chars() {
            self.key(&c.to_string(), false);
        }
    }

    pub fn key(&mut self, key: &str, down: bool) -> bool {
        if !down {
            return matches!(key, " " | "f" | "h" | "u" | "r" | "v" | "-" | "=");
        }
        match key {
            " " => self.paused = !self.paused,
            "f" => self.fire(),
            "h" => self.hdr = !self.hdr,
            "u" => self.ui = !self.ui,
            "r" => self.rotate = !self.rotate,
            "v" => self.slider_at = (self.slider_at + 1) % FEATURED.len(),
            "-" => self.bump_slider(-0.05),
            "=" => self.bump_slider(0.05),
            _ => return false,
        }
        true
    }

    fn bump_slider(&mut self, by: f32) {
        if let Some(name) = FEATURED.get(self.slider_at)
            && let Some(v) = self.settings.sliders.get_mut(*name)
        {
            *v = (*v + by).clamp(0.0, 1.0);
        }
    }

    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        if buttons & 1 == 1
            && let Some((lx, ly)) = self.last
        {
            self.yaw -= (x - lx) as f32 * 3.0;
            self.pitch = (self.pitch + (y - ly) as f32 * 3.0).clamp(-0.2, 1.2);
        }
        self.last = Some((x, y));
    }

    pub fn wheel(&mut self, amount: f64) {
        self.dist = (self.dist * (1.0 - amount as f32 * 0.5))
            .clamp(self.bounds.1 * 0.4, self.bounds.1 * 4.0);
    }

    /// Sounds to start: a name, with "*" if it repeats; "-" before it stops it.
    pub fn cues(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cues)
    }

    /// What tests look at.
    pub fn report(&self) -> String {
        let name = FEATURED.get(self.slider_at).copied().unwrap_or("?");
        let paused = match self.paused {
            true => " paused",
            false => "",
        };
        format!(
            "t {:.1} ship {} sails {} smoke {} slider {name}={:.3} triangles {}{paused}",
            self.time,
            self.parts.len(),
            self.sails.len(),
            self.smoke.iter().filter(|p| !p.spray).count(),
            self.slider(name),
            self.tris,
        )
    }

    fn sky_quad(&self, gl: &mut Gl, params: [[f32; 4]; 32], eye: Vec3, at: Vec3) {
        let fwd = (at - eye).normalize_or_zero();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(fwd);
        let (tan_v, tan_h) = (0.51f32, 0.51f32 * 16.0 / 9.0);
        let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        let verts = corners.map(|(x, y)| {
            let d = (fwd + right * x * tan_h + up * y * tan_v).normalize_or_zero();
            Vertex {
                position: [x, y, 0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [[d.x, d.y, d.z, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]],
            }
        });
        gl.quad(
            State::new(SKY),
            &Surface {
                textures: [0, 0, 0, 0],
                cube: self.sky,
                params,
            },
            verts,
        );
    }

    fn full_quad(&self, gl: &mut Gl, program: usize, surface: &Surface) {
        let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let verts = corners.map(|(x, y)| Vertex {
            position: [2.0 * x - 1.0, 1.0 - 2.0 * y, 0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [[x, y, 0.0, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]],
        });
        gl.quad(State::new(program), surface, verts);
    }

    fn emit_ocean(&mut self, gl: &mut Gl, view_proj: &Mat4, refl: &Mat4, base: &Surface) {
        let n = self.ocean_tess;
        let half = self.ocean_size * 0.5;
        let amp = 2.0 + self.slider("amplitude_slider") * 10.0;
        let disp = self.slider("displacement_slider") * 3.0;
        let t = self.time * (0.2 + self.slider("time_slider"));
        let uni = 0.5 + self.slider("universe_scale_slider");
        let mut grid: Vec<(Vec3, Vec3, f32, f32)> = Vec::with_capacity((n + 1) * (n + 1));
        for j in 0..=n {
            for i in 0..=n {
                let (mut x, mut z) = (
                    -half + 2.0 * half * i as f32 / n as f32,
                    -half + 2.0 * half * j as f32 / n as f32,
                );
                let (mut y, mut dx, mut dz) = (0.0, 0.0, 0.0);
                for (dir, len, amp_k, phase) in &self.ocean_waves {
                    let k = std::f32::consts::TAU / (len * uni);
                    let a = amp * amp_k / 8.0;
                    let p = (dir.x * x + dir.y * z) * k + t * (0.5 + 8.0 / len) + phase;
                    let (s, c) = p.sin_cos();
                    y += a * s;
                    let q = disp * 0.6 / 8.0;
                    x += q * a * dir.x * c;
                    z += q * a * dir.y * c;
                    dx += a * k * dir.x * c;
                    dz += a * k * dir.y * c;
                }
                let normal = Vec3::new(-dx, 1.0, -dz).normalize_or_zero();
                let chop = (dx * dx + dz * dz).sqrt();
                let wake = match z < self.stern {
                    true => 1.0 - (self.stern - z) / 250.0,
                    false => 0.0,
                }
                .max(0.0)
                    * (1.0 - x.abs() / 120.0).max(0.0);
                grid.push((Vec3::new(x, y, z), normal, chop, wake));
            }
        }
        let at = |i: usize, j: usize| &grid[j * (n + 1) + i];
        let mut verts = Vec::with_capacity(n * n * 6);
        for j in 0..n {
            for i in 0..n {
                for (a, b, c) in [
                    (at(i, j), at(i + 1, j), at(i, j + 1)),
                    (at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)),
                ] {
                    for (w, nr, chop, wake) in [a, b, c] {
                        let clip = view_proj * Vec4::new(w.x, w.y, w.z, 1.0);
                        let rc = *refl * Vec4::new(w.x, w.y, w.z, 1.0);
                        verts.push(Vertex {
                            position: clip.to_array(),
                            color: [1.0, 1.0, 1.0, 1.0],
                            uv: [
                                [w.x, w.y, w.z, 1.0],
                                [nr.x, nr.y, nr.z, *chop],
                                [*wake, 0.0, 0.0, 0.0],
                                [rc.x, rc.y, rc.z, rc.w],
                            ],
                        });
                    }
                }
            }
        }
        let surface = Surface {
            textures: [self.refl, self.turbulence, self.foam, 0],
            cube: self.sky,
            params: base.params,
        };
        self.tris += verts.len() / 3;
        gl.draw(State::new(OCEAN).depth(Depth::Less(true)), &surface, &verts);
    }

    fn emit_ship(
        &mut self,
        gl: &mut Gl,
        view_proj: Mat4,
        motion: Mat4,
        base: &Surface,
        mirrored: bool,
        sails: bool,
    ) {
        for part in &self.parts {
            let is_sail = part.sail.is_some();
            if is_sail != sails {
                continue;
            }
            let program = match sails {
                true => SAIL,
                false => SHIP,
            };
            let mut state = State::new(program)
                .vertex("vsmotion")
                .depth(Depth::Less(true));
            if mirrored {
                state = state.cull(wgpu::Face::Front);
            }
            let surface = Surface {
                textures: [part.color, part.bump, 0, 0],
                cube: self.sky,
                params: self.moved(base, view_proj, motion).params,
            };
            self.tris += part.tris;
            gl.draw_mesh(state, &surface, part.mesh);
        }
    }

    fn emit_sails(&mut self, gl: &mut Gl, view_proj: &Mat4, base: &Surface) {
        let motion = self.motion();
        for pi in self.sails.clone() {
            let (sail_id, cpu) = {
                let part = &self.parts[pi];
                (
                    part.sail,
                    part.sail_cpu
                        .as_ref()
                        .map(|c| (c.tris.clone(), c.uvs.clone(), c.world)),
                )
            };
            let (Some(si), Some((tris, uvs, world))) = (sail_id, cpu) else {
                continue;
            };
            let sail = &self.sail_anims[si];
            let max = sail.times.last().copied().unwrap_or(1.0).max(0.01);
            let t = (self.time * self.sail_scale) % max;
            let k = sail
                .times
                .partition_point(|k| *k <= t)
                .clamp(1, sail.times.len() - 1);
            let (t0, t1) = (sail.times[k - 1], sail.times[k]);
            let u = ((t - t0) / (t1 - t0).max(1e-6)).clamp(0.0, 1.0);
            let world = motion * world;
            let mut verts = Vec::with_capacity(tris.len() * 3);
            for tri in &tris {
                for idx in tri {
                    let a = &sail.pos[k - 1][*idx];
                    let b = &sail.pos[k][*idx];
                    let p = *a + (*b - *a) * u;
                    let w = world.transform_point3(p);
                    verts.push(Vertex {
                        position: (*view_proj * Vec4::new(w.x, w.y, w.z, 1.0)).to_array(),
                        color: [uvs[*idx].x, uvs[*idx].y, 1.0, 1.0],
                        uv: [
                            [w.x, w.y, w.z, 1.0],
                            [0.0, 1.0, 0.0, 0.0],
                            [1.0, 0.0, 0.0, 0.0],
                            [0.0, 0.0, 1.0, 0.0],
                        ],
                    });
                }
            }
            let part = &self.parts[pi];
            let surface = Surface {
                textures: [part.color, part.bump, 0, 0],
                cube: self.sky,
                params: base.params,
            };
            self.tris += verts.len() / 3;
            gl.draw(State::new(SAIL).depth(Depth::Less(true)), &surface, &verts);
        }
    }

    fn emit_particles(&mut self, gl: &mut Gl, view_proj: &Mat4, eye: Vec3, base: &Surface) {
        let fwd = (self.at - eye).normalize_or_zero();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(fwd);
        let mut smoke = Vec::new();
        let mut spray = Vec::new();
        for p in &self.smoke {
            let a = 1.0 - p.age / p.life;
            let s = p.size * (0.5 + 0.5 * (p.age / p.life));
            let quad = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
            let q = quad.map(|(u, v)| {
                let w = p.at + right * (2.0 * u - 1.0) * s + up * (2.0 * v - 1.0) * s;
                Vertex {
                    position: (*view_proj * Vec4::new(w.x, w.y, w.z, 1.0)).to_array(),
                    color: [1.0, 1.0, 1.0, a.max(0.0)],
                    uv: [[u, v, p.age / p.life, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]],
                }
            });
            match p.spray {
                true => spray.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]),
                false => smoke.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]),
            }
        }
        if !smoke.is_empty() {
            let surface = Surface {
                textures: [self.smoke_sprite, 0, 0, 0],
                cube: self.sky,
                params: base.params,
            };
            self.tris += smoke.len() / 3;
            gl.draw(
                State::new(SMOKE).depth(Depth::Less(false)).blend(
                    wgpu::BlendFactor::SrcAlpha,
                    wgpu::BlendFactor::OneMinusSrcAlpha,
                ),
                &surface,
                &smoke,
            );
        }
        if !spray.is_empty() {
            let surface = Surface {
                textures: [self.foam, 0, 0, 0],
                cube: self.sky,
                params: base.params,
            };
            self.tris += spray.len() / 3;
            gl.draw(
                State::new(SPRAY).depth(Depth::Less(false)).blend(
                    wgpu::BlendFactor::SrcAlpha,
                    wgpu::BlendFactor::OneMinusSrcAlpha,
                ),
                &surface,
                &spray,
            );
        }
    }

    pub fn draw(&mut self, gpu: &Gpu, gl: &mut Gl, (w, h): (u32, u32)) {
        self.tris = 0;
        let debug = std::env::var("CLEAR_SAIL_DEBUG").unwrap_or_default();
        if debug.contains("nohdr") {
            self.hdr = false;
        }
        let no = |s: &str| debug.contains(s);
        let aspect = w.max(1) as f32 / h.max(1) as f32;
        let (eye, view, proj) = self.camera(aspect, false);
        let view_proj = proj * view;
        let (reye, rview, rproj) = self.camera(aspect, true);
        let refl = rproj * rview;
        let base = self.base_surface(eye, refl);
        let motion = self.motion();
        let _ = gpu;
        gl.begin([0.06, 0.05, 0.09, 1.0]);
        // Reflection: the ship mirrored in the water plane.
        gl.to(Some(self.refl), Some([0.0, 0.0, 0.0, 0.0]));
        {
            let base = self.base_surface(reye, refl);
            self.sky_quad(
                gl,
                base.params,
                reye,
                Vec3::new(self.at.x, -self.at.y, self.at.z),
            );
            self.emit_ship(gl, refl, motion, &base, true, false);
            self.emit_ship(gl, refl, motion, &base, true, true);
        }
        // Main scene, in a target for the HDR chain or straight to the window.
        match self.hdr {
            true => gl.to(Some(self.scene_target), Some([0.06, 0.05, 0.09, 1.0])),
            false => gl.to(None, Some([0.06, 0.05, 0.09, 1.0])),
        }
        self.sky_quad(gl, base.params, eye, self.at);
        if !no("noocean") {
            self.emit_ocean(gl, &view_proj, &refl, &base);
        }
        if !no("noship") {
            self.emit_ship(gl, view_proj, motion, &base, false, false);
            self.emit_sails(gl, &view_proj, &base);
        }
        {
            let surface = self.moved(&base, view_proj, motion);
            let surface = Surface {
                textures: [0, 0, 0, 0],
                cube: self.sky,
                params: surface.params,
            };
            self.tris += self.rope_count / 2;
            gl.draw_mesh(
                State::new(ROPE)
                    .vertex("vsmotion")
                    .depth(Depth::Less(false))
                    .lines()
                    .blend(
                        wgpu::BlendFactor::SrcAlpha,
                        wgpu::BlendFactor::OneMinusSrcAlpha,
                    ),
                &surface,
                self.ropes,
            );
        }
        for (mesh, tris) in self.splinters.clone() {
            let surface = self.moved(&base, view_proj, motion);
            let surface = Surface {
                textures: [0, 0, 0, 0],
                cube: self.sky,
                params: surface.params,
            };
            gl.draw_mesh(
                State::new(SHIP).vertex("vsmotion").depth(Depth::Less(true)),
                &surface,
                mesh,
            );
            self.tris += tris;
        }
        self.emit_particles(gl, &view_proj, eye, &base);
        // HDR: bright, blur across and down, screen blend.
        if no("sceneonly") {
            gl.to(None, Some([0.0, 0.0, 0.0, 1.0]));
            let surface = Surface {
                textures: [self.refl, 0, 0, 0],
                cube: self.sky,
                params: base.params,
            };
            self.full_quad(gl, UI, &surface);
        }
        if self.hdr {
            let quad = |gl: &mut Gl,
                        program: usize,
                        t0: usize,
                        t1: usize,
                        target: Option<usize>,
                        step: [f32; 4]| {
                gl.to(target, Some([0.0, 0.0, 0.0, 1.0]));
                let mut params = base.params;
                params[19] = step;
                let surface = Surface {
                    textures: [t0, t1, 0, 0],
                    cube: self.sky,
                    params,
                };
                self.full_quad(gl, program, &surface);
            };
            quad(
                gl,
                BRIGHT,
                self.scene_target,
                0,
                Some(self.bright),
                [0.0; 4],
            );
            quad(
                gl,
                BLUR,
                self.bright,
                0,
                Some(self.blur_a),
                [2.0 / 512.0, 0.0, 0.0, 0.0],
            );
            quad(
                gl,
                BLUR,
                self.blur_a,
                0,
                Some(self.blur_b),
                [0.0, 0.0, 0.0, 2.0 / 256.0],
            );
            quad(gl, COMP, self.scene_target, self.blur_b, None, [0.0; 4]);
        }
        // Menus on top.
        if self.ui {
            gl.to(None, None);
            for (tex, x, y, wq, hq) in [
                (self.logo, 0.86, 0.02, 0.12, 0.09),
                (self.actions, 0.01, 0.92, 0.10, 0.06),
            ] {
                // Menu pictures are stored mirrored, `logo.dds` upside down [I].
                let (fu, fv) = match tex == self.logo {
                    true => (false, true),
                    false => (true, false),
                };
                // The menu pictures are stored mirrored; the demo flips them back.
                let uu = |u: f32| match fu {
                    true => 1.0 - u,
                    false => u,
                };
                let vv = |v: f32| match fv {
                    true => 1.0 - v,
                    false => v,
                };
                let corners = [
                    (x, y, uu(0.0), vv(0.0)),
                    (x + wq, y, uu(1.0), vv(0.0)),
                    (x + wq, y + hq, uu(1.0), vv(1.0)),
                    (x, y + hq, uu(0.0), vv(1.0)),
                ];
                let v = corners.map(|(x, y, u, v)| Vertex {
                    position: [2.0 * x - 1.0, 1.0 - 2.0 * y, 0.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [[u, v, 0.0, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]],
                });
                let surface = Surface {
                    textures: [tex, 0, 0, 0],
                    cube: self.sky,
                    params: base.params,
                };
                gl.quad(
                    State::new(UI).blend(
                        wgpu::BlendFactor::SrcAlpha,
                        wgpu::BlendFactor::OneMinusSrcAlpha,
                    ),
                    &surface,
                    v,
                );
            }
        }
        self.frames += 1;
    }
}
