//! The viewers' menus (`NVStaticMenu`, `NVDropDownMenu`, `NVOnOffButton`, `NVTexSlider`,
//! `NVIcon`), drawn last in the window pass (`0x42b820`).
//!
//! Each widget is a rectangle in fractions of the window from the bottom left, drawn with
//! the built-in programs `_scr_tex` and `_tex_mod_color` (buttons) or `_texture` (sliders,
//! icons), alpha blending, no culling, no depth test (their `AddToViewer` methods), through
//! the device's immediate quad (`0x41a0c0`: clip (2x - 1, 2y - 1), z 1).
//!
//! Buttons draw their on colour when on and their off colour when off (`0x44a8f0`); a click
//! toggles them and calls their callback (`0x449290`). A drop-down has its own button at
//! its open position, off at first in 0.7 grey (`0x44a310`); clicking it opens or closes the
//! menu, and a click on an item closes it (`0x45dd40`). Sliders take the value where they
//! are clicked or dragged (`0x449330`, `0x449470`); the knob is as tall as the bar and as wide
//! as its picture's shape makes it (`0x44b670`). Icons keep their height at size·w/h.

use nv_asm::parse::Stage;

use crate::gl::{MeshArray, States};
use crate::viewer::Viewer;
use crate::world::{self, Widget as Node};

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Static,
    DropDown { open: bool, button: usize },
    OnOff { on: bool, on_color: [f32; 3], off_color: [f32; 3], image: Option<usize>, callback: String },
    Slider { value: f32, horizontal: bool, bar: Option<usize>, knob: Option<usize>, callback: String },
    Icon { image: Option<usize> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub name: String,
    pub kind: Kind,
    pub visible: bool,
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub children: Vec<usize>,
}

/// What a click or drag did, for the demo's callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Button { name: String, on: bool },
    Slider { name: String, value: f32 },
}

#[derive(Default)]
pub struct Menus {
    pub widgets: Vec<Widget>,
    pub roots: Vec<usize>,
    /// The slider being dragged.
    dragging: Option<usize>,
    vp: Option<usize>,
    fp_color: Option<usize>,
    fp_texture: Option<usize>,
}

impl Menus {
    /// Builds the widgets of the scenes' menus; `programs` finds a built-in program by name.
    pub fn new(v: &Viewer, programs: &dyn Fn(&str) -> Option<(Stage, usize)>) -> Menus {
        let mut m = Menus::default();
        let tex = |i: &Option<usize>| i.and_then(|k| v.world.widget_textures.get(k).copied().flatten()).and_then(|t| v.textures[t]);
        fn add(m: &mut Menus, n: &Node, tex: &dyn Fn(&Option<usize>) -> Option<usize>) -> usize {
            let at = m.widgets.len();
            let (name, kind, pos, size, kids): (String, Kind, [f32; 2], [f32; 2], Vec<&Node>) = match n {
                Node::Static { name, items } => (name.clone(), Kind::Static, [0.0; 2], [0.0; 2], items.iter().collect()),
                Node::DropDown { name, items, open_position, open_size, image, initially_open } => {
                    m.widgets.push(Widget {
                        name: name.clone(),
                        kind: Kind::DropDown { open: *initially_open, button: 0 },
                        visible: true,
                        pos: *open_position,
                        size: *open_size,
                        children: vec![],
                    });
                    let b = m.widgets.len();
                    m.widgets.push(Widget {
                        name: "___Button___".into(),
                        kind: Kind::OnOff {
                            on: false,
                            on_color: [1.0; 3],
                            off_color: [0.7, 0.7, 0.7],
                            image: tex(image),
                            callback: String::new(),
                        },
                        visible: true,
                        pos: *open_position,
                        size: *open_size,
                        children: vec![],
                    });
                    let kids: Vec<usize> = items.iter().map(|c| add(m, c, tex)).collect();
                    m.widgets[at].children = kids;
                    m.widgets[at].kind = Kind::DropDown { open: *initially_open, button: b };
                    return at;
                }
                Node::OnOff { name, callback, initial, position, size, on_color, off_color, image } => (
                    name.clone(),
                    Kind::OnOff { on: *initial, on_color: *on_color, off_color: *off_color, image: tex(image), callback: callback.clone() },
                    *position,
                    *size,
                    vec![],
                ),
                Node::TexSlider { name, callback, initial, alignment, position, size, slider, knob } => (
                    name.clone(),
                    Kind::Slider { value: *initial, horizontal: *alignment == 0, bar: tex(slider), knob: tex(knob), callback: callback.clone() },
                    *position,
                    *size,
                    vec![],
                ),
                Node::Icon { name, image, position, size } => (name.clone(), Kind::Icon { image: tex(image) }, *position, *size, vec![]),
                Node::Other { name, .. } => (name.clone(), Kind::Static, [0.0; 2], [0.0; 2], vec![]),
            };
            m.widgets.push(Widget { name, kind, visible: true, pos, size, children: vec![] });
            let ids: Vec<usize> = kids.into_iter().map(|c| add(m, c, tex)).collect();
            m.widgets[at].children = ids;
            at
        }
        for n in &v.world.menus {
            let r = add(&mut m, n, &tex);
            m.roots.push(r);
        }
        let prog = |n: &str, s: Stage| programs(n).filter(|p| p.0 == s).map(|p| p.1);
        m.vp = prog("_scr_tex", Stage::Vertex);
        m.fp_color = prog("_tex_mod_color", Stage::Fragment);
        m.fp_texture = prog("_texture", Stage::Fragment);
        m
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.widgets.iter().position(|w| w.name == name)
    }

    fn states() -> States {
        let w = |n: &str| world::builtin(n);
        let (Some(world::State::Cull(cull)), Some(world::State::Blend(blend)), Some(world::State::Depth(depth))) =
            (w("_CullNoCull"), w("_BlendAlpha"), w("_DepthNoTest"))
        else {
            unreachable!()
        };
        States {
            cull,
            blend,
            depth,
            alpha: world::ALPHA_DEFAULT,
            stencil: world::STENCIL_DEFAULT,
            raster: world::RASTER_DEFAULT,
        }
    }

    fn quad(&self, v: &mut Viewer, fp: usize, rect: [f32; 4], texture: Option<usize>, color: Option<[f32; 4]>) -> Result<(), Box<dyn std::error::Error>> {
        let [x, y, w, h] = rect;
        let (cx, cy, cw, ch) = (x + x - 1.0, y + y - 1.0, w + w, h + h);
        let pos = vec![cx, cy, 1.0, cx + cw, cy, 1.0, cx + cw, cy + ch, 1.0, cx, cy + ch, 1.0];
        let uv = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let mesh = v.gl.mesh(
            &[("a2v.projCoord".into(), MeshArray::F32(pos, 3)), ("a2v.texcoord".into(), MeshArray::F32(uv, 2))],
            &[(3, vec![0, 1, 2, 0, 2, 3])],
            false,
            false,
        );
        v.gl.bind_texture(fp, "texture", texture);
        if let Some(c) = color {
            v.gl.fragment_programs[fp].set("color", c);
        }
        let Some(vp) = self.vp else { return Ok(()) };
        v.gl.bind_vertex_program(vp);
        v.gl.draw(vp, fp, &Self::states(), &mesh, 0)?;
        Ok(())
    }

    /// Draws the visible widgets (`0x45dc60`, `0x45e3b0`, `0x44a8f0`, `0x44b670`, `0x44cd20`).
    pub fn draw(&self, v: &mut Viewer) -> Result<(), Box<dyn std::error::Error>> {
        for &r in &self.roots {
            self.draw_widget(v, r)?;
        }
        Ok(())
    }

    fn draw_widget(&self, v: &mut Viewer, i: usize) -> Result<(), Box<dyn std::error::Error>> {
        let w = &self.widgets[i];
        if !w.visible {
            return Ok(());
        }
        match &w.kind {
            Kind::Static => {
                for &c in &w.children {
                    self.draw_widget(v, c)?;
                }
            }
            Kind::DropDown { open, button } => {
                self.draw_widget(v, *button)?;
                if *open {
                    for &c in &w.children {
                        self.draw_widget(v, c)?;
                    }
                }
            }
            Kind::OnOff { on, on_color, off_color, image, .. } => {
                let c = if *on { on_color } else { off_color };
                if let Some(fp) = self.fp_color {
                    self.quad(v, fp, [w.pos[0], w.pos[1], w.size[0], w.size[1]], *image, Some([c[0], c[1], c[2], 1.0]))?;
                }
            }
            Kind::Slider { value, horizontal, bar, knob, .. } => {
                let Some(fp) = self.fp_texture else { return Ok(()) };
                let [x, y] = w.pos;
                let [sw, sh] = w.size;
                self.quad(v, fp, [x, y, sw, sh], *bar, None)?;
                let dims = knob.and_then(|t| v.gl.textures[t].as_ref()).map(|t| (t.width as f32, t.height as f32)).unwrap_or((1.0, 1.0));
                let rect = if *horizontal {
                    let (kw, kh) = (sh * dims.0 / dims.1, sh);
                    [((value - 0.5) * ((sw - kw) / sw) + 0.5) * sw + x - kw * 0.5, y, kw, kh]
                } else {
                    let (kw, kh) = (sw, sw * dims.1 / dims.0);
                    [x, ((value - 0.5) * ((sh - kh) / sh) + 0.5) * sh + y - kh * 0.5, kw, kh]
                };
                self.quad(v, fp, rect, *knob, None)?;
            }
            Kind::Icon { image } => {
                let Some(fp) = self.fp_texture else { return Ok(()) };
                let (ww, wh) = (v.gl.window.0 as f32, v.gl.window.1 as f32);
                self.quad(v, fp, [w.pos[0], w.pos[1], w.size[0], ww / wh * w.size[1]], *image, None)?;
            }
        }
        Ok(())
    }

    fn hit(w: &Widget, x: f32, y: f32) -> bool {
        w.pos[0] <= x && x < w.pos[0] + w.size[0] && w.pos[1] <= y && y <= w.pos[1] + w.size[1]
    }

    /// A press at (x, y), fractions of the window from the top left. Returns whether a widget
    /// took it, and what it did.
    pub fn press(&mut self, x: f32, y: f32) -> (bool, Vec<Event>) {
        let y = 1.0 - y;
        let mut events = vec![];
        let mut took = false;
        for r in self.roots.clone() {
            if self.press_widget(r, x, y, &mut events) {
                took = true;
                break;
            }
        }
        (took, events)
    }

    fn press_widget(&mut self, i: usize, x: f32, y: f32, ev: &mut Vec<Event>) -> bool {
        if !self.widgets[i].visible {
            return false;
        }
        match self.widgets[i].kind.clone() {
            Kind::Static => {
                for c in self.widgets[i].children.clone() {
                    if self.press_widget(c, x, y, ev) {
                        return true;
                    }
                }
                false
            }
            Kind::DropDown { open, button } => {
                if self.press_widget(button, x, y, ev) {
                    self.widgets[i].kind = Kind::DropDown { open: !open, button };
                    return true;
                }
                if open {
                    for c in self.widgets[i].children.clone() {
                        if self.press_widget(c, x, y, ev) {
                            self.widgets[i].kind = Kind::DropDown { open: false, button };
                            return true;
                        }
                    }
                }
                false
            }
            Kind::OnOff { on, on_color, off_color, image, callback } => {
                let w = &self.widgets[i];
                if Self::hit(w, x, y) {
                    let name = w.name.clone();
                    self.widgets[i].kind = Kind::OnOff { on: !on, on_color, off_color, image, callback: callback.clone() };
                    if !callback.is_empty() {
                        ev.push(Event::Button { name, on: !on });
                    }
                    return true;
                }
                false
            }
            Kind::Slider { .. } => {
                if Self::hit(&self.widgets[i], x, y) {
                    self.dragging = Some(i);
                    self.slide(i, x, y, ev);
                    return true;
                }
                false
            }
            Kind::Icon { .. } => false,
        }
    }

    fn slide(&mut self, i: usize, x: f32, y: f32, ev: &mut Vec<Event>) {
        let w = &mut self.widgets[i];
        if let Kind::Slider { value, horizontal, callback, .. } = &mut w.kind {
            let v = if *horizontal { (x - w.pos[0]) / w.size[0] } else { (y - w.pos[1]) / w.size[1] };
            *value = if v < 0.0 { 0.0 } else if v > 1.0 { 1.0 } else { v };
            if !callback.is_empty() {
                ev.push(Event::Slider { name: w.name.clone(), value: *value });
            }
        }
    }

    /// The mouse moving with the button down.
    pub fn drag(&mut self, x: f32, y: f32) -> Vec<Event> {
        let mut ev = vec![];
        if let Some(i) = self.dragging {
            self.slide(i, x, 1.0 - y, &mut ev);
        }
        ev
    }

    /// Whether a slider has the mouse.
    pub fn dragging(&self) -> bool {
        self.dragging.is_some()
    }

    pub fn release(&mut self) {
        self.dragging = None;
    }

    pub fn set_on(&mut self, name: &str, on: bool) {
        if let Some(i) = self.find(name) {
            if let Kind::OnOff { on: o, .. } = &mut self.widgets[i].kind {
                *o = on;
            }
        }
    }

    pub fn is_on(&self, name: &str) -> bool {
        self.find(name).is_some_and(|i| matches!(self.widgets[i].kind, Kind::OnOff { on: true, .. }))
    }

    pub fn set_image(&mut self, name: &str, image: Option<usize>) {
        if let Some(i) = self.find(name) {
            if let Kind::OnOff { image: im, .. } = &mut self.widgets[i].kind {
                *im = image;
            }
        }
    }

    pub fn set_visible(&mut self, name: &str, visible: bool) {
        if let Some(i) = self.find(name) {
            self.widgets[i].visible = visible;
        }
    }

    pub fn set_value(&mut self, name: &str, v: f32) {
        if let Some(i) = self.find(name) {
            if let Kind::Slider { value, .. } = &mut self.widgets[i].kind {
                *value = v;
            }
        }
    }

    /// Closes a drop-down (`0x45dd00`).
    pub fn close(&mut self, name: &str) {
        if let Some(i) = self.find(name) {
            if let Kind::DropDown { button, .. } = self.widgets[i].kind {
                self.widgets[i].kind = Kind::DropDown { open: false, button };
            }
        }
    }
}
