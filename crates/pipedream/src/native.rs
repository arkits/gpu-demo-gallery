use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pipedream::{Data, Player};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

type Error = Box<dyn std::error::Error>;

const DEMO: &str = "ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream/Sushi.ati";

/// Finds `Sushi.ati`: argument, `PIPEDREAM_DATA`, nearby folders, then a dialog.
fn find_archive() -> Option<PathBuf> {
    let from = |p: PathBuf| if p.is_dir() { p.join("Sushi.ati") } else { p };
    let arg = std::env::args_os().skip(1).find(|a| a != "--mute");
    if let Some(p) = arg.or_else(|| std::env::var_os("PIPEDREAM_DATA")) {
        return Some(from(p.into()));
    }
    let exe = std::env::current_exe().ok();
    let exe_dir = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    let mut near = vec![PathBuf::from("."), PathBuf::from("assets")];
    near.extend(exe_dir.iter().cloned());
    // Beside a macOS bundle: Name.app/Contents/MacOS/pipedream.
    near.extend(
        exe_dir
            .iter()
            .filter_map(|d| d.ancestors().nth(3).map(Path::to_path_buf)),
    );
    let found = near
        .iter()
        .flat_map(|d| [d.join("Sushi.ati"), d.join(DEMO)]);
    found.into_iter().find(|p| p.is_file()).or_else(|| {
        rfd::FileDialog::new()
            .set_title("Choose Sushi.ati from the original Pipe Dream demo")
            .add_filter("Sushi.ati", &["ati"])
            .pick_file()
    })
}

struct Music {
    player: rodio::Player,
    _device: rodio::MixerDeviceSink,
}

impl Music {
    fn open(data: Vec<u8>) -> Result<Self, Error> {
        let device = rodio::DeviceSinkBuilder::open_default_sink()?;
        let player = rodio::Player::connect_new(device.mixer());
        let len = data.len() as u64;
        let decoder = rodio::Decoder::builder()
            .with_data(Cursor::new(data))
            .with_byte_len(len)
            .with_seekable(true)
            .build()?;
        player.append(decoder);
        Ok(Self {
            player,
            _device: device,
        })
    }
}

pub fn run() -> Result<(), Error> {
    let Some(path) = find_archive() else {
        return Ok(());
    };
    let archive =
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let data = Data {
        archive,
        logo: std::fs::read(dir.join("Logo2.tga")).ok(),
    };
    let mp3 = dir.join("Audio").join("AnimusicIntro.mp3");
    let mp3 = if mp3.is_file() {
        mp3
    } else {
        dir.join("AnimusicIntro.mp3")
    };
    let mp3 = std::fs::read(&mp3).map_err(Error::from);
    let music = mp3.and_then(Music::open);
    let music = music
        .inspect_err(|e| eprintln!("playing without music: {e}"))
        .ok();
    let mut app = App {
        data,
        music,
        muted: std::env::args().any(|a| a == "--mute"),
        window: None,
        player: None,
        start: Instant::now(),
        drag: None,
        cursor: PhysicalPosition::default(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    data: Data,
    music: Option<Music>,
    muted: bool,
    window: Option<Arc<Window>>,
    player: Option<Player>,
    start: Instant,
    drag: Option<PhysicalPosition<f64>>,
    cursor: PhysicalPosition<f64>,
    error: Option<Error>,
}

impl App {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Error> {
        let attrs = Window::default_attributes()
            .with_title("Pipe Dream")
            .with_inner_size(LogicalSize::new(1024, 768));
        let window = Arc::new(el.create_window(attrs)?);
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let player = Player::new(
            &instance,
            window.clone(),
            (size.width, size.height),
            &self.data,
        );
        let player = pollster::block_on(player)?;
        while !player.ready() {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.player = Some(player);
        self.start = Instant::now();
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn redraw(&mut self) {
        let Some(player) = &mut self.player else {
            return;
        };
        let mut audio = None;
        if let Some(m) = &self.music {
            if let Some(t) = player.show.take_seek()
                && let Err(e) = m.player.try_seek(Duration::from_secs_f64(t))
            {
                eprintln!("seek: {e}");
            }
            match player.show.paused() {
                true => m.player.pause(),
                false => m.player.play(),
            }
            m.player.set_volume(if self.muted {
                0.0
            } else {
                player.show.brightness()
            });
            audio = (!m.player.empty() && !m.player.is_paused())
                .then(|| m.player.get_pos().as_secs_f64());
        }
        player.frame(self.start.elapsed().as_secs_f64(), audio);
    }

    fn key(&mut self, el: &ActiveEventLoop, key: &Key) {
        let (Some(window), Some(player)) = (&self.window, &mut self.player) else {
            return;
        };
        let name = match key {
            Key::Named(NamedKey::Escape) => return el.exit(),
            Key::Named(NamedKey::Enter | NamedKey::F11) => {
                let full = window.fullscreen().is_none();
                window.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
                window.set_cursor_visible(!full);
                return;
            }
            Key::Named(NamedKey::Space) => " ",
            Key::Named(NamedKey::ArrowLeft) => "ArrowLeft",
            Key::Named(NamedKey::ArrowRight) => "ArrowRight",
            Key::Named(NamedKey::Home) => "Home",
            Key::Character(c) if c.eq_ignore_ascii_case("m") => {
                self.muted = !self.muted;
                return;
            }
            Key::Character(c) => c.as_str(),
            _ => return,
        };
        player.show.key(name);
    }
}

impl ApplicationHandler for App {
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
            WindowEvent::Resized(s) => {
                if let Some(p) = &mut self.player {
                    p.resize(s.width, s.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        repeat,
                        ..
                    },
                ..
            } if !repeat || matches!(logical_key, Key::Named(_)) => self.key(el, &logical_key),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.drag = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if let (Some(last), Some(p)) = (self.drag.as_mut(), &mut self.player) {
                    let (dx, dy) = (position.x - last.x, position.y - last.y);
                    p.show.drag(dx as f32 * 0.2, dy as f32 * 0.2);
                    *last = position;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 0.05,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.002,
                };
                if let Some(p) = &mut self.player {
                    p.show.zoom(amount);
                }
            }
            _ => {}
        }
    }
}
