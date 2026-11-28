//! Gas Station's own code: the day (`0x4061f0`), the Preetham sky (`0x405080`), sun and sky
//! colours (`0x405980`), soft shadows' time windows (`0x4055d0`), the plant rings' sort
//! sets (`0x405eb0`) and the animated cameras (`0x404d60`, `0x404cc0`).

use nv_engine::math::{add, dot, scale, Vec3, Vec4};
use nv_engine::viewer::{Hooks, Viewer};
use nv_engine::world::{Named, TransformKind};

/// Slider ranges (`0x57b46c`…`0x57b478`, `0x57b454`).
pub const SPEED_RANGE: (f32, f32) = (0.02, 5.0);
pub const SOFTNESS_RANGE: (f32, f32) = (0.025, 0.1);
pub const CLOUDS_RANGE: (f32, f32) = (50.0, 150.0);

pub struct Demo {
    /// The time of day (`0x5c0808`), seconds of the day's clock.
    pub day: f64,
    /// Seconds per second (`0x5ba2b0`).
    pub speed: f64,
    pub playing: bool,
    /// Shadow softness, the half-width of the rise and fall window (`0x5ba2b8`).
    pub softness: f32,
    pub turbidity: f32,
    /// Base sun and sky colours (`0x5c0868`, `0x5c0858`).
    sun_base: Vec3,
    sky_base: Vec3,
    /// Clock of the last frame, and when input last came (`0x5c0818`, `0x5c0820`).
    last: Option<f64>,
    pub idle_since: f64,
    pub frames: u64,
    /// Running camera animation (`0x5c0830`).
    pub anim: Option<usize>,
    scene_pass: Option<usize>,
    pub walk_camera: Option<usize>,
    cams: [(Option<usize>, Option<usize>); 2],
    sun_xform: Option<usize>,
    light: Option<usize>,
    /// The sun-derived values the clouds read (`clouds + 0x670`…).
    pub clouds: crate::clouds::Clouds,
    /// The menus, when the scene has all of them (`0x5c07fc`).
    pub menus: Option<nv_engine::menu::Menus>,
    pub walk: crate::walk::Walk,
    /// The wireframe override (`0x457f60`).
    pub(crate) wire: nv_engine::world::Settings,
    pub(crate) nav_pause: Option<usize>,
    pub(crate) nav_cont: Option<usize>,
}

impl Demo {
    pub fn new(v: &mut Viewer) -> Demo {
        Self::with_seed(v, None)
    }

    /// `seed` replaces `time(NULL)` for the clouds (tests, screenshots).
    pub fn with_seed(v: &mut Viewer, seed: Option<u32>) -> Demo {
        let w = &mut v.world;
        let pass = |w: &nv_engine::world::World, n: &str| match w.find(n) {
            Some(Named::Pass(i)) => Some(i),
            _ => None,
        };
        let scene_pass = pass(w, "scenepass");
        // `cloud_group` joins the scene pass's shapes; the window pass clears to grey.
        if let (Some(p), Some(r)) = (scene_pass, w.shape_ref("cloud_group")) {
            w.passes[p].shapes.push(r);
        }
        if let Some(p) = w.passes.iter().position(|p| p.color.is_none() && p.depth.is_none()) {
            w.passes[p].clear_color = [0.5, 0.5, 0.5, 1.0];
        }
        let walk_camera = scene_pass.and_then(|p| w.passes[p].camera).or(if w.cameras.is_empty() { None } else { Some(0) });
        let anim = |w: &nv_engine::world::World, n: &str| match w.find(n) {
            Some(Named::Animation(i)) => Some(i),
            _ => None,
        };
        let cams = [
            (w.camera("gas_station_cam_sunsetShape"), anim(w, "gas_station_cam_sunset_anim")),
            (w.camera("gas_station_cam_detailShape"), anim(w, "gas_station_cam_detail_anim")),
        ];
        // Plant rings (0x405eb0): `Plantring<x>` draws in set 27 - (x - 'a').
        if let Some(nv_engine::world::ShapeRef::Group(g)) = w.shape_ref("plant_group") {
            for c in w.groups[g].children.clone() {
                if let nv_engine::world::ShapeRef::Shape(s) = c {
                    let name = w.shapes[s].name.clone();
                    if name.len() > 9 && name.starts_with("Plantring") {
                        let x = name.as_bytes()[9].to_ascii_lowercase();
                        if let Some(st) = w.shapes[s].settings.first_mut() {
                            st.sort_set = (0x1b - (x as i32 - b'a' as i32)) as u32;
                        }
                    }
                }
            }
        }
        // The clouds come first (0x40a830), seeded with time(NULL).
        let seed = seed.unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0)
        });
        let mut clouds = crate::clouds::Clouds::new(seed);
        let _ = &mut clouds;
        let light = match w.find("dirLightShape1").or(w.find("directionalLightShape1")) {
            Some(Named::Light(i)) => Some(i),
            _ => None,
        };
        let sun_xform = w.transform("sun_xform");
        clouds.attach(v);
        let wire = crate::ui::wire_settings(v);
        let image = |v: &Viewer, n: &str| v.world.texture(n).and_then(|t| v.textures[t]);
        let (nav_pause, nav_cont) = (image(v, "nav_pause"), image(v, "nav_cont"));
        let mut walk = crate::walk::Walk::new();
        walk.attach(v, walk_camera, 0.0);
        let mut demo = Demo {
            day: 0.0,
            speed: 1.0,
            playing: true,
            softness: 0.05,
            turbidity: 4.0,
            sun_base: [0.8, 0.8, 0.6],
            sky_base: [0.6, 0.6, 0.8],
            last: None,
            idle_since: 0.0,
            frames: 0,
            anim: None,
            scene_pass,
            walk_camera,
            cams,
            sun_xform,
            light,
            clouds,
            menus: None,
            walk,
            wire,
            nav_pause,
            nav_cont,
        };
        demo.init_menus(v);
        demo
    }

    pub fn set_collision(&mut self, collision1_tga: &[u8]) {
        self.walk.map = crate::walk::Map::read(collision1_tga);
    }

    /// The camera on the scene pass.
    pub fn camera(&self, v: &Viewer) -> Option<usize> {
        self.scene_pass.and_then(|p| v.world.passes[p].camera)
    }

    pub(crate) fn scene_pass(&self) -> Option<usize> {
        self.scene_pass
    }

    /// Puts a camera on the scene pass and starts its animation at the time of day (`0x404cc0`).
    pub fn set_camera(&mut self, v: &mut Viewer, camera: Option<usize>, anim: Option<usize>) {
        if let Some(p) = self.scene_pass {
            v.world.passes[p].camera = camera;
        }
        self.anim = anim;
        if let Some(a) = anim {
            v.world.animations[a].start = self.day;
        }
    }

    /// The animated camera (`0x404d60`): day and speed back to 0 and 1, and one of the two
    /// paths, by whether the clock's whole seconds are odd.
    pub fn start_movie(&mut self, v: &mut Viewer, now: f64) {
        self.playing = true;
        self.day = 0.0;
        self.speed = 1.0;
        let pause_image = self.nav_pause;
        if let Some(m) = &mut self.menus {
            m.close("nav_menu");
            m.close("draw_menu");
            m.set_on("nav_menu_walk", true);
            m.set_on("nav_menu_play", false);
            m.set_image("nav_menu_pause", pause_image);
            m.set_on("nav_menu_sliders", true);
        }
        let k = if (now as i32) & 1 != 0 { 0 } else { 1 };
        let (c, a) = self.cams[k];
        self.set_camera(v, c, a);
        self.idle_since = now;
    }

    /// One frame at clock time `now` (`0x4061f0`).
    pub fn frame(&mut self, v: &mut Viewer, now: f64) {
        let dt = match self.last {
            Some(l) => now - l,
            None => 0.1,
        };
        if self.last.is_none() {
            self.idle_since = now;
        }
        self.last = Some(now);
        self.walk.update(v, now);
        if self.anim.is_none() && now > self.idle_since + 30.0 {
            self.start_movie(v, now);
        }
        self.clouds.prev = self.clouds.time;
        self.clouds.time = self.day;
        if self.playing {
            self.day += self.speed * dt;
            let mut t = ((self.day * 0.016599999740719795) % 1.1000000014901161) as f32;
            while t < 0.0 {
                t += 1.1;
            }
            if t > 1.0 {
                t = (t - 1.0) * 10.0 + 1.0;
                if t > 1.5 {
                    t -= 2.0;
                }
            }
            let s = (std::f64::consts::PI * t as f64).sin() * 0.9f32 as f64 + 0.1f32 as f64;
            let intensity = if s >= 0.0 { s as f32 } else { 0.0 };
            if t > 1.0 && now > self.idle_since + 15.0 {
                self.start_movie(v, now);
            }
            let sun = sun_direction(t);
            self.shadow_windows(v, t);
            if let Some(l) = self.light {
                v.world.lights[l].direction = sun;
            }
            self.sky(v, sun);
            let p4 = intensity.powf(0.4);
            let p5 = intensity.powf(0.5);
            let sb = self.sun_base;
            let kb = self.sky_base;
            let sun_c = [sb[0] * p4, sb[1] * p5, sb[2] * p5, 1.0];
            let sky_c = [kb[0] * (p4 + 1.0) * 0.5, kb[1] * (p5 + 1.0) * 0.5, kb[2] * (p5 + 1.0) * 0.5, 1.0];
            self.colors(v, sun_c, sky_c);
            v.set_constant("sun_suncolor", [intensity.powf(0.3), intensity.powf(0.35), p4, 1.0]);
            // The clouds' light (0x406586…0x4066c7).
            let x = ((dot([1.0, 0.0, 0.0], sun) + 1.0) * 0.5).clamp(0.0, 1.0);
            let f = if t < 0.0 {
                (t + 0.2) * 5.0
            } else if t <= 1.0 {
                1.0
            } else {
                1.0 - (t - 1.0) * 5.0
            };
            let f = if f >= 1.0 { 1.0 } else if f > 0.0 { f } else { 0.0 };
            self.clouds.light = if t >= 1.0 { 1.0 } else if t < 0.0 { 0.0 } else { x };
            self.clouds.sun = sun;
            self.clouds.day = f;
            if let Some(a) = self.anim {
                v.animate(a, self.day);
            }
        }
        v.time = now;
        v.update_transforms();
        self.frames += 1;
    }

    /// `invDelta_N` and `timeRange_N` (`0x4055d0`): the window of rise and fall times that is
    /// half lit.
    fn shadow_windows(&self, v: &mut Viewer, t: f32) {
        let s = self.softness;
        let inv = [1.0 / s, 0.0, 0.0, 0.0];
        let range = [s * 0.5 + t, t - s * 0.5, 0.0, 0.0];
        for i in 1..=6 {
            v.set_constant(&format!("invDelta_{i}"), inv);
            v.set_constant(&format!("timeRange_{i}"), range);
            if i < 4 {
                v.set_constant(&format!("invDelta_t{i}"), inv);
                v.set_constant(&format!("timeRange_t{i}"), range);
            }
        }
    }

    /// `0x405980`.
    fn colors(&self, v: &mut Viewer, sun: Vec4, sky: Vec4) {
        let mid = [(sun[0] + sky[0]) * 0.5, (sun[1] + sky[1]) * 0.5, (sun[2] + sky[2]) * 0.5, 1.0];
        v.set_constant("p_lightcolor", mid);
        v.set_constant("ns_suncolor", sun);
        v.set_constant("ns_skycolor", sky);
        v.set_constant("ns_suncolor_t", sun);
        v.set_constant("ns_skycolor_t", sky);
        for i in 1..=6 {
            v.set_constant(&format!("ss_suncolor_{i}"), sun);
            v.set_constant(&format!("ss_skycolor_{i}"), sky);
            if i < 4 {
                v.set_constant(&format!("ss_suncolor_t{i}"), sun);
                v.set_constant(&format!("ss_skycolor_t{i}"), sky);
            }
        }
    }

    /// The Preetham sky's constants (`0x405080`) and the sun's place.
    fn sky(&self, v: &mut Viewer, sun: Vec3) {
        let t = self.turbidity;
        let y = if -0.1 < sun[1] || -0.1 == sun[1] { sun[1] } else { -0.1 };
        let theta = y.acos();
        let lin = |base: Vec3, slope: Vec3| add(base, scale(slope, t));
        let a = lin([-0.2592, -0.2608, -1.463], [-0.0193, -0.0167, 0.1787]);
        let b = scale(lin([0.0008, 0.0092, 0.4275], [-0.0665, -0.095, -0.3554]), 1.442695);
        let c = lin([0.2125, 0.2102, 5.3251], [-0.0004, -0.0079, -0.0227]);
        let d = scale(lin([-0.8989, -1.6537, -2.5771], [-0.0641, -0.0441, 0.1206]), 1.442695);
        let e = lin([0.0452, 0.0529, 0.3703], [-0.0033, -0.0109, -0.067]);
        let zenith = zenith(t, theta);
        // The Perez function at the zenith (0x401800, 0x4011f0), with exp as 2^x.
        let pf = |k: usize| {
            let ct = theta.cos();
            (1.0 + a[k] * 2f32.powf(b[k] * 1.0)) * (1.0 + c[k] * 2f32.powf(d[k] * theta) + e[k] * ct * ct)
        };
        let v3 = |x: Vec3| [x[0], x[1], x[2], 0.0];
        v.set_constant("zenithcolor", v3(zenith));
        v.set_constant("pf0", v3([pf(0), pf(1), pf(2)]));
        v.set_constant("constA", v3(a));
        v.set_constant("constB", v3(b));
        v.set_constant("constC", v3(c));
        v.set_constant("constD", v3(d));
        v.set_constant("constE", v3(e));
        v.set_constant("sundir", [sun[0], sun[1], sun[2], 1.0]);
        if let Some(x) = self.sun_xform {
            if let TransformKind::Maya { translation, .. } = &mut v.world.transforms[x].kind {
                *translation = scale(sun, 40.0);
            }
        }
    }
}

/// The sun (`0x401ed0`): a circle tilted from the vertical, half a turn per day.
pub fn sun_direction(t: f32) -> Vec3 {
    let a = t * 3.14159012f32;
    let (c3, s3) = ((3.0f64).cos() as f32, (3.0f64).sin() as f32);
    let (s5, c5) = ((0.5f64).sin() as f32, (0.5f64).cos() as f32);
    let v1 = [c3, 0.0, s3];
    let v2 = [s5 * -s3, c5 + s5 * 0.0, s5 * c3];
    add(scale(v2, a.sin()), scale(v1, a.cos()))
}

/// Preetham's zenith chromaticity and luminance (`0x401c60`).
fn zenith(t: f32, theta: f32) -> Vec3 {
    let th = [theta * theta * theta, theta * theta, theta, 1.0];
    let row = |r: [f32; 4]| r[0] * th[0] + r[1] * th[1] + r[2] * th[2] + r[3] * th[3];
    let tt = [t * t, t, 1.0];
    let x = tt[0] * row([0.00166, -0.00375, 0.00209, 0.0]) + tt[1] * row([-0.02903, 0.06377, -0.03202, 0.00394]) + tt[2] * row([0.11693, -0.21196, 0.06052, 0.25886]);
    let y = tt[0] * row([0.00275, -0.0061, 0.00317, 0.0]) + tt[1] * row([-0.04214, 0.0897, -0.04153, 0.00516]) + tt[2] * row([0.15346, -0.26756, 0.0667, 0.26688]);
    let lum = ((0.444444448f32 - t * 0.00833333377) * (3.14159012f32 - (theta + theta))).tan() * (t * 4.0453 - 4.971) - t * 0.2155 + 2.4192;
    [x, y, lum]
}

impl Hooks for Demo {
    fn menus(&mut self, v: &mut Viewer) {
        if let Some(m) = &self.menus {
            if let Err(e) = m.draw(v) {
                eprintln!("menus: {e}");
            }
        }
    }

    fn after_shape(&mut self, v: &mut Viewer, shape: usize) {
        if self.clouds.shape == Some(shape) {
            if let Err(e) = self.clouds.update_and_draw(v) {
                eprintln!("clouds: {e}");
            }
        }
    }
}
