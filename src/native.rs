use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use box_of_smoke::controls::Button;
use box_of_smoke::data::Files;
use box_of_smoke::{Error, Player};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

const PACK: &str = "box-of-smoke.xz";

#[derive(Default)]
struct Options {
    path: Option<PathBuf>,
    /// Run this many seconds in a hidden window, report, and quit.
    smoke: Option<f64>,
}

fn options() -> Result<Options, Error> {
    let mut o = Options::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--smoke") => {
                let v = args.next().and_then(|v| v.to_str()?.parse().ok());
                o.smoke = Some(v.ok_or("--smoke needs a number")?);
            }
            Some(f) if f.starts_with("--") => return Err(format!("unknown option {f}").into()),
            _ => o.path = Some(a.into()),
        }
    }
    Ok(o)
}

/// Remembers the chosen folder so the dialog appears at most once.
fn remembered() -> Option<PathBuf> {
    let var = |n: &str| std::env::var_os(n).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else if cfg!(windows) {
        var("APPDATA")?
    } else {
        var("XDG_CONFIG_HOME").or_else(|| Some(var("HOME")?.join(".config")))?
    };
    Some(base.join("box-of-smoke").join("data-path.txt"))
}

fn load(p: &Path) -> Result<Files, Error> {
    if p.is_dir() {
        return Files::from_dir(p);
    }
    let data = std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    Files::open(&data)
}

/// Finds the demo: argument, `BOX_OF_SMOKE_DATA`, beside or above the program, the
/// last choice, then a dialog.
fn find(o: &Options) -> Result<Option<Files>, Error> {
    let given = o
        .path
        .clone()
        .or_else(|| std::env::var_os("BOX_OF_SMOKE_DATA").map(Into::into));
    if let Some(p) = given {
        return load(&p).map(Some);
    }
    let exe = std::env::current_exe().ok();
    let mut near = vec![PathBuf::from(".")];
    near.extend(
        exe.iter()
            .flat_map(|e| e.ancestors().skip(1).take(6))
            .map(Path::to_path_buf),
    );
    for d in near {
        for p in [d.join(PACK), d.join("data"), d] {
            if p.is_file() || p.join("models/trapped.obj").is_file() {
                return load(&p).map(Some);
            }
        }
    }
    let saved = remembered();
    let last = saved.as_ref().and_then(|s| std::fs::read_to_string(s).ok());
    if let Some(files) = last.and_then(|p| load(Path::new(&p)).ok()) {
        return Ok(Some(files));
    }
    if o.smoke.is_some() {
        return Ok(None);
    }
    let Some(picked) = rfd::FileDialog::new()
        .set_title("Choose the unpacked Box of Smoke folder")
        .pick_folder()
    else {
        return Ok(None);
    };
    let files = load(&picked)?;
    if let Some(s) = saved {
        let _ = s.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(s, picked.to_string_lossy().as_bytes());
    }
    Ok(Some(files))
}

pub fn run() -> Result<(), Error> {
    let options = options()?;
    let Some(files) = find(&options)? else {
        return match options.smoke {
            Some(_) => Err("demo data not found".into()),
            None => Ok(()),
        };
    };
    let mut app = Shell {
        files,
        options,
        frames: (0, 0),
        window: None,
        player: None,
        start: Instant::now(),
        cursor: PhysicalPosition::default(),
        held: false,
        modifiers: ModifiersState::empty(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct Shell {
    files: Files,
    options: Options,
    /// Shown, and drawn offscreen while hidden.
    frames: (u32, u32),
    window: Option<Arc<Window>>,
    player: Option<Player>,
    start: Instant,
    cursor: PhysicalPosition<f64>,
    held: bool,
    modifiers: ModifiersState,
    error: Option<Error>,
}

impl Shell {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Error> {
        let shown = self.options.smoke.is_none();
        let attrs = Window::default_attributes()
            .with_title("Box of Smoke")
            .with_visible(shown)
            .with_active(shown)
            .with_inner_size(LogicalSize::new(1024, 768));
        let window = Arc::new(el.create_window(attrs)?);
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let size = (size.width, size.height);
        let player = Player::new(&instance, window.clone(), size, &self.files);
        let player = pollster::block_on(player)?;
        while !player.ready() {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.player = Some(player);
        if shown {
            window.focus_window();
        }
        self.start = Instant::now();
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn redraw(&mut self) {
        let Some(player) = &mut self.player else {
            return;
        };
        let now = self.start.elapsed().as_secs_f64();
        if player.frame(now) {
            self.frames.0 += 1;
            return;
        }
        // Hidden window: nothing paces us, so avoid spinning.
        std::thread::sleep(Duration::from_millis(16));
        if self.options.smoke.is_some() {
            player.frame_offscreen(now);
            self.frames.1 += 1;
        }
    }

    fn key(&mut self, el: &ActiveEventLoop, key: &Key) {
        let (Some(window), Some(player)) = (&self.window, &mut self.player) else {
            return;
        };
        match key {
            Key::Named(NamedKey::Escape) => el.exit(),
            Key::Named(NamedKey::Enter | NamedKey::F11) => {
                let full = window.fullscreen().is_none();
                window.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
            }
            Key::Named(NamedKey::Space) => _ = player.app.controls.key(" "),
            Key::Character(c) => _ = player.app.controls.key(c),
            _ => {}
        }
    }

    /// The cursor as a fraction of the window.
    fn at(&self) -> (f32, f32) {
        let s = self.window.as_ref().map(|w| w.inner_size());
        let s = s.unwrap_or_default();
        let part = |v: f64, n: u32| (v / f64::from(n.max(1))) as f32;
        (part(self.cursor.x, s.width), part(self.cursor.y, s.height))
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(e) = self.open(el)
        {
            self.error = Some(e);
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Resized(s) => {
                if let Some(p) = &mut self.player {
                    p.resize(s.width, s.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                let elapsed = self.start.elapsed().as_secs_f64();
                if self.options.smoke.is_some_and(|s| elapsed >= s) {
                    let (shown, hidden) = self.frames;
                    let total = shown + hidden;
                    println!("frames {total} (shown {shown}, offscreen {hidden})");
                    return el.exit();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => self.key(el, &logical_key),
            WindowEvent::MouseInput { state, button, .. } => {
                let at = self.at();
                let rotate = self.modifiers.control_key() || self.modifiers.shift_key();
                let Some(p) = &mut self.player else { return };
                self.held = state == ElementState::Pressed;
                match (self.held, button) {
                    (true, MouseButton::Left) => p.app.controls.press(Button::Left, rotate, at),
                    (true, _) => p.app.controls.press(Button::Other, rotate, at),
                    (false, _) => p.app.controls.release(),
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let at = self.at();
                if let (true, Some(p)) = (self.held, &mut self.player) {
                    p.app.controls.motion(at);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 0.1,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.004,
                };
                if let Some(p) = &mut self.player {
                    p.app.controls.zoom(amount);
                }
            }
            _ => {}
        }
    }
}
