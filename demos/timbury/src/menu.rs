//! The menu overlay: logo, action buttons and HDR sliders from `menu.nma`.
use glam::Vec2;

use crate::Error;
use crate::data::Files;
use crate::tree::Node;

/// A button from the actions menu.
#[derive(Default, Clone)]
pub struct Button {
    pub callback: &'static str,
    pub image: String,
    pub position: Vec2,
    pub size: Vec2,
}

/// A slider from the slider menu.
#[derive(Default, Clone)]
pub struct Slider {
    pub index: usize,
    pub position: Vec2,
    pub size: Vec2,
}

#[derive(Default, Clone)]
pub struct MenuDef {
    pub logo: (String, Vec2, Vec2),
    pub about_box: (String, Vec2, Vec2),
    pub buttons: Vec<Button>,
    pub sliders: Vec<Slider>,
    pub bar: String,
    pub knob: String,
}

fn pair(n: &Node, field: &str) -> Vec2 {
    let v = n.floats(field);
    Vec2::new(*v.first().unwrap_or(&0.0), *v.get(1).unwrap_or(&0.0))
}

fn walk(
    items: &[Node],
    buttons: &mut Vec<Button>,
    sliders: &mut Vec<Slider>,
    logo: &mut Option<(String, Vec2, Vec2)>,
    about_box: &mut Option<(String, Vec2, Vec2)>,
) {
    for item in items {
        match item.kind {
            "Icon" => {
                let icon = (
                    item.text("image").to_owned(),
                    pair(item, "position"),
                    pair(item, "size"),
                );
                if item.name.contains("logo") {
                    *logo = Some(icon);
                } else {
                    *about_box = Some(icon);
                }
            }
            "DropDownMenu" | "StaticMenu" => {
                walk(item.nodes("menuItems"), buttons, sliders, logo, about_box);
            }
            "OnOffButton" => {
                let callback = match item.text("callback") {
                    "Camera1CB" => "Camera1CB",
                    "Camera2CB" => "Camera2CB",
                    "SlidersCB" => "SlidersCB",
                    "ResetCB" => "ResetCB",
                    "PauseCB" => "PauseCB",
                    "RestartCB" => "RestartCB",
                    "QuitCB" => "QuitCB",
                    "AboutCB" => "AboutCB",
                    _ => continue,
                };
                buttons.push(Button {
                    callback,
                    image: item.text("buttonImage").to_owned(),
                    position: pair(item, "position"),
                    size: pair(item, "size"),
                });
            }
            "Slider" => {
                sliders.push(Slider {
                    // Order in the file is the slider index.
                    index: sliders.len(),
                    position: pair(item, "position"),
                    size: pair(item, "size"),
                });
            }
            _ => {}
        }
    }
}

impl MenuDef {
    pub fn read(root: &Node) -> Result<Self, Error> {
        let mut buttons = Vec::new();
        let mut sliders = Vec::new();
        let (mut logo, mut about_box) = (None, None);
        for menu in root.nodes("menus") {
            walk(
                menu.nodes("menuItems"),
                &mut buttons,
                &mut sliders,
                &mut logo,
                &mut about_box,
            );
        }
        let (Some(logo), Some(about_box)) = (logo, about_box) else {
            return Err("menu is missing its pictures".into());
        };
        Ok(Self {
            logo,
            about_box,
            buttons,
            sliders,
            bar: "sliderbar_texture".to_owned(),
            knob: "sliderbutton_texture".to_owned(),
        })
    }

    pub fn load(files: &Files, name: &str) -> Result<Self, Error> {
        let data = files.get(name)?;
        let root = crate::tree::read_text(&String::from_utf8_lossy(data))?;
        Self::read(&root).map_err(|e| format!("{name}: {e}").into())
    }
}

/// Whether the slider panel and the about box show.
#[derive(Default)]
pub struct Menu {
    pub sliders: bool,
    pub about_until: f64,
}

impl Menu {
    /// Handles a press in fractions of the window; returns the callback hit, if any.
    pub fn press(
        &mut self,
        def: &MenuDef,
        x: f32,
        y: f32,
        _now: f64,
        n_sliders: usize,
    ) -> Option<Press> {
        let y = 1.0 - y;
        let inside =
            |(p, s): (Vec2, Vec2)| x >= p.x && x <= p.x + s.x && y >= p.y && y <= p.y + s.y;
        for b in &def.buttons {
            if inside((b.position, b.size)) {
                return Some(Press::Button(b.callback));
            }
        }
        if self.sliders {
            for s in &def.sliders {
                if s.index < n_sliders && inside((s.position, s.size)) {
                    return Some(Press::Slider(
                        s.index,
                        ((x - s.position.x) / s.size.x).clamp(0.0, 1.0),
                    ));
                }
            }
        }
        None
    }

    /// Dragging on a slider moves its knob.
    pub fn drag(
        &mut self,
        def: &MenuDef,
        x: f32,
        y: f32,
        n_sliders: usize,
    ) -> Option<(usize, f32)> {
        let y = 1.0 - y;
        for s in &def.sliders {
            if s.index < n_sliders
                && x >= s.position.x - 0.02
                && x <= s.position.x + s.size.x + 0.02
                && y >= s.position.y - 0.02
                && y <= s.position.y + s.size.y + 0.02
            {
                return Some((s.index, ((x - s.position.x) / s.size.x).clamp(0.0, 1.0)));
            }
        }
        None
    }
}

pub enum Press {
    Button(&'static str),
    Slider(usize, f32),
}
