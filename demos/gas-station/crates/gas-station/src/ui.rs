//! Input: the menus' callbacks `ButtonCB` (`0x406740`) and `SliderCB` (`0x401350`), the
//! keyboard (`0x4079e0`) and the mouse (`0x42da70`, `0x42d950`, `0x404e80`).
//!
//! Buttons read "on" as available: the drawing mode in use and the camera in use are off.
//! Dragging with a button held takes the walking camera and stops the movie; keys do not.

use nv_engine::menu::{Event, Menus};
use nv_engine::viewer::Viewer;
use nv_engine::world::{self, Settings, Shader};

use crate::demo::{Demo, CLOUDS_RANGE, SOFTNESS_RANGE, SPEED_RANGE};

/// The wireframe override (`0x457f60`): no culling or depth test, lines, `_white`.
pub(crate) fn wire_settings(v: &mut Viewer) -> Settings {
    let mut state = |n: &str| {
        let w = &mut v.world;
        Some(match w.states.iter().position(|s| s.0 == n) {
            Some(i) => i,
            None => {
                w.states.push((n.to_string(), world::builtin(n).expect("built-in state")));
                w.states.len() - 1
            }
        })
    };
    let mut s = Settings {
        cull: state("_CullNoCull"),
        blend: state("_BlendDefault"),
        depth: state("_DepthNoTest"),
        alpha: state("_AlphaDefault"),
        stencil: state("_StencilDefault"),
        raster: state("_RasterLine"),
        ..Default::default()
    };
    if let Some(p) = v.builtins.get("_white").copied() {
        v.world.shaders.push(Shader { name: "_white".into(), filename: "_white.fp30".into(), profile: 6149, entry: "main".into(), params: vec![] });
        v.programs.push(Some(p));
        s.fragment = Some(v.world.shaders.len() - 1);
    }
    s
}

/// What a key asks of the player.
pub enum KeyResult {
    None,
    Quit,
}

impl Demo {
    /// The menus (`0x409600`…): the slider menu hidden, the clouds and shadow sliders at
    /// their values; the speed slider keeps its own.
    pub(crate) fn init_menus(&mut self, v: &Viewer) {
        let m = Menus::new(v, &|n| v.builtins.get(n).copied());
        let names = [
            "toplevel_menu", "nav_menu", "draw_menu", "draw_menu_norm", "draw_menu_wire", "nav_menu_play", "nav_menu_pause",
            "nav_menu_walk", "nav_menu_sliders", "slider_menu", "slider_menu_time", "slider_menu_shadow", "slider_menu_clouds",
        ];
        if names.iter().any(|n| m.find(n).is_none()) || self.nav_pause.is_none() || self.nav_cont.is_none() {
            eprintln!("UI not properly initialized");
            return;
        }
        let mut m = m;
        m.set_visible("slider_menu", false);
        m.set_value("slider_menu_clouds", (self.clouds.size - CLOUDS_RANGE.0) / (CLOUDS_RANGE.1 - CLOUDS_RANGE.0));
        m.set_value("slider_menu_shadow", (self.softness - SOFTNESS_RANGE.0) / (SOFTNESS_RANGE.1 - SOFTNESS_RANGE.0));
        self.menus = Some(m);
    }

    fn wire_on(&self, v: &Viewer) -> bool {
        self.scene_pass().is_some_and(|p| v.world.passes[p].override_settings.as_ref() == Some(&self.wire))
    }

    fn set_wire(&mut self, v: &mut Viewer, on: bool) {
        if let Some(p) = self.scene_pass() {
            v.world.passes[p].override_settings = on.then(|| self.wire.clone());
        }
    }

    /// The walking camera (`0x404cc0` with `0x5c0794`).
    fn take_walk(&mut self, v: &mut Viewer) {
        self.set_camera(v, self.walk_camera, None);
    }

    fn walking(&self, v: &Viewer) -> bool {
        self.camera(v) == self.walk_camera
    }

    fn on(&mut self, name: &str, on: bool) {
        if let Some(m) = &mut self.menus {
            m.set_on(name, on);
        }
    }

    fn is_on(&self, name: &str) -> bool {
        self.menus.as_ref().is_some_and(|m| m.is_on(name))
    }

    fn pause_image(&mut self) {
        let im = if self.playing { self.nav_pause } else { self.nav_cont };
        if let Some(m) = &mut self.menus {
            m.set_image("nav_menu_pause", im);
        }
    }

    /// `ButtonCB` (`0x406740`), after the button toggled.
    fn button(&mut self, v: &mut Viewer, name: &str, now: f64) {
        if let Some(m) = &mut self.menus {
            m.close("draw_menu");
            m.close("nav_menu");
        }
        match name {
            "draw_menu_norm" => {
                if self.wire_on(v) {
                    self.set_wire(v, false);
                }
                self.on("draw_menu_wire", true);
                self.on("draw_menu_norm", false);
            }
            "draw_menu_wire" => {
                if !self.wire_on(v) {
                    self.set_wire(v, true);
                }
                self.on("draw_menu_wire", false);
                self.on("draw_menu_norm", true);
            }
            "nav_menu_play" => {
                if !self.is_on("nav_menu_play") {
                    self.start_movie(v, now);
                } else {
                    self.on("nav_menu_play", false);
                }
            }
            "nav_menu_pause" => {
                self.playing = !self.playing;
                self.on("nav_menu_pause", true);
                self.pause_image();
                if !self.walking(v) {
                    self.on("nav_menu_play", false);
                }
            }
            "nav_menu_walk" => {
                if !self.walking(v) {
                    self.on("nav_menu_play", false);
                    self.idle_since = now;
                    self.take_walk(v);
                    self.on("nav_menu_walk", false);
                    self.on("nav_menu_play", true);
                }
            }
            "nav_menu_sliders" => {
                let show = !self.is_on("nav_menu_sliders");
                if let Some(m) = &mut self.menus {
                    m.set_visible("slider_menu", show);
                }
            }
            _ => {}
        }
    }

    /// `SliderCB` (`0x401350`).
    fn slider(&mut self, name: &str, value: f32) {
        match name {
            "slider_menu_time" => self.speed = ((SPEED_RANGE.1 - SPEED_RANGE.0) * value + SPEED_RANGE.0) as f64,
            "slider_menu_shadow" => self.softness = (SOFTNESS_RANGE.1 - SOFTNESS_RANGE.0) * value + SOFTNESS_RANGE.0,
            "slider_menu_clouds" => self.clouds.size = (CLOUDS_RANGE.1 - CLOUDS_RANGE.0) * value + CLOUDS_RANGE.0,
            _ => {}
        }
    }

    fn events(&mut self, v: &mut Viewer, events: Vec<Event>, now: f64) {
        for e in events {
            match e {
                Event::Button { name, .. } => self.button(v, &name, now),
                Event::Slider { name, value } => self.slider(&name, value),
            }
        }
    }

    fn speed_slider(&mut self) {
        let v = (self.speed as f32 - SPEED_RANGE.0) / (SPEED_RANGE.1 - SPEED_RANGE.0);
        if let Some(m) = &mut self.menus {
            m.set_value("slider_menu_time", v);
        }
    }

    fn softness_slider(&mut self) {
        let v = (self.softness - SOFTNESS_RANGE.0) / (SOFTNESS_RANGE.1 - SOFTNESS_RANGE.0);
        if let Some(m) = &mut self.menus {
            m.set_value("slider_menu_shadow", v);
        }
    }

    /// A typed character (`0x4079e0`).
    pub fn key(&mut self, v: &mut Viewer, c: char, now: f64) -> KeyResult {
        let (lo, hi) = (SPEED_RANGE.0 as f64, SPEED_RANGE.1 as f64);
        match c {
            '\u{1b}' => return KeyResult::Quit,
            ' ' => {
                self.playing = !self.playing;
                if self.menus.is_some() {
                    self.on("nav_menu_pause", true);
                    self.pause_image();
                    if !self.walking(v) {
                        self.on("nav_menu_play", false);
                    }
                }
            }
            '*' | '8' => {
                if self.speed.abs() < hi {
                    self.speed *= 1.5;
                    if hi <= self.speed {
                        self.speed = hi;
                    }
                }
                self.speed_slider();
            }
            '-' | '_' => self.speed = -self.speed,
            '/' | '?' => {
                if lo < self.speed.abs() {
                    self.speed *= 0.6666666666666666;
                    if self.speed <= lo {
                        self.speed = lo;
                    }
                }
                self.speed_slider();
            }
            'C' | 'c' => {
                if self.menus.is_none() || self.is_on("nav_menu_play") {
                    self.start_movie(v, now);
                }
            }
            'S' => {
                if SOFTNESS_RANGE.0 < self.softness {
                    self.softness *= 0.909090877;
                    if self.softness <= SOFTNESS_RANGE.0 {
                        self.softness = SOFTNESS_RANGE.0;
                    }
                }
                self.softness_slider();
            }
            's' => {
                if self.softness < SOFTNESS_RANGE.1 {
                    self.softness *= 1.1;
                    if SOFTNESS_RANGE.1 <= self.softness {
                        self.softness = SOFTNESS_RANGE.1;
                    }
                }
                self.softness_slider();
            }
            'W' | 'w' => {
                let wire = !self.wire_on(v);
                self.set_wire(v, wire);
                if self.menus.is_some() {
                    self.on("draw_menu_norm", wire);
                    self.on("draw_menu_wire", !wire);
                }
            }
            _ => {}
        }
        KeyResult::None
    }

    /// An arrow key: 0 up, 1 down, 2 left, 3 right.
    pub fn arrow(&mut self, which: usize, down: bool, now: f64) {
        self.walk.key(which, down, now);
    }

    /// A mouse button (`0x42da70`): 0 left, 1 middle, 2 right; x and y are fractions of the
    /// window from its top left.
    pub fn mouse_button(&mut self, v: &mut Viewer, button: u32, down: bool, x: f32, y: f32, now: f64) {
        let m = &mut self.walk.mouse;
        m.handled = false;
        m.prev = m.now;
        m.now = [x, y];
        match button {
            0 => {
                m.left = down;
                if down {
                    m.press = [x, y];
                    if let Some(menus) = &mut self.menus {
                        let (took, events) = menus.press(x, y);
                        if took {
                            self.walk.mouse.handled = true;
                            self.events(v, events, now);
                        }
                    }
                    return;
                }
            }
            1 => m.middle = down,
            _ => m.right = down,
        }
        if !down {
            if let Some(menus) = &mut self.menus {
                menus.release();
            }
        }
    }

    /// The mouse moving (`0x42d950`); `held` when a button is down (`0x404e80`).
    pub fn mouse_motion(&mut self, v: &mut Viewer, x: f32, y: f32, held: bool, now: f64) {
        if held {
            self.idle_since = now;
            self.take_walk(v);
            if self.menus.is_some() {
                self.on("nav_menu_walk", false);
                self.on("nav_menu_play", true);
            }
        }
        let m = &mut self.walk.mouse;
        m.handled = false;
        m.prev = m.now;
        m.now = [x, y];
        if m.left {
            if let Some(menus) = &mut self.menus {
                if menus.dragging() {
                    let events = menus.drag(x, y);
                    self.walk.mouse.handled = true;
                    self.events(v, events, now);
                    return;
                }
            }
        }
        self.walk.look(v);
    }
}
