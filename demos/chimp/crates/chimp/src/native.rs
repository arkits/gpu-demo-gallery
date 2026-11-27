use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chimp::Player;
use chimp::music::Music as Score;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

type Error = Box<dyn std::error::Error>;

const DEMO: &str = "data/Sushi.ati";

#[derive(Default)]
struct Options {
    path: Option<PathBuf>,
    mute: bool,
    /// Chapter to start with, from 1.
    chapter: Option<usize>,
    /// Run this many seconds in a hidden window, report, and quit.
    smoke: Option<f64>,
}

fn options() -> Result<Options, Error> {
    let mut o = Options::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        let mut number = || -> Result<f64, Error> {
            let v = args
                .next()
                .and_then(|v| v.to_str().and_then(|v| v.parse().ok()));
            v.ok_or_else(|| format!("{} needs a number", a.to_string_lossy()).into())
        };
        match a.to_str() {
            Some("--mute") => o.mute = true,
            Some("--chapter") => o.chapter = Some(number()? as usize),
            Some("--smoke") => o.smoke = Some(number()?),
            Some(f) if f.starts_with("--") => return Err(format!("unknown option {f}").into()),
            _ => o.path = Some(a.into()),
        }
    }
    Ok(o)
}

/// Remembers the chosen archive so the dialog appears at most once.
fn remembered() -> Option<PathBuf> {
    let var = |n: &str| std::env::var_os(n).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else if cfg!(windows) {
        var("APPDATA")?
    } else {
        var("XDG_CONFIG_HOME").or_else(|| Some(var("HOME")?.join(".config")))?
    };
    Some(base.join("chimp").join("archive-path.txt"))
}

/// Finds `Sushi.ati`: argument, `CHIMP_DATA`, beside or above the program,
/// the last choice, then a dialog.
fn find_archive(o: &Options) -> Option<PathBuf> {
    let from = |p: PathBuf| if p.is_dir() { p.join("Sushi.ati") } else { p };
    if let Some(p) = o
        .path
        .clone()
        .or_else(|| std::env::var_os("CHIMP_DATA").map(Into::into))
    {
        return Some(from(p));
    }
    let exe = std::env::current_exe().ok();
    let mut near = vec![PathBuf::from("."), PathBuf::from("assets")];
    // Covers a bundle or build inside the repository.
    near.extend(
        exe.iter()
            .flat_map(|e| e.ancestors().skip(1).take(6))
            .map(Path::to_path_buf),
    );
    let mut found = near
        .iter()
        .flat_map(|d| [d.join("Sushi.ati"), d.join(DEMO)]);
    if let Some(p) = found.find(|p| p.is_file()) {
        return Some(p);
    }
    let saved = remembered();
    let last = saved
        .as_ref()
        .and_then(|s| std::fs::read_to_string(s).ok())
        .map(PathBuf::from);
    if let Some(p) = last.filter(|p| p.is_file()) {
        return Some(p);
    }
    if o.smoke.is_some() {
        return None;
    }
    let picked = rfd::FileDialog::new()
        .set_title("Choose Sushi.ati from the original Chimp demo")
        .add_filter("Sushi.ati", &["ati"])
        .pick_file()?;
    if let Some(s) = saved {
        let _ = s.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(s, picked.to_string_lossy().as_bytes());
    }
    Some(picked)
}

struct Music {
    player: rodio::Player,
    score: Arc<Score>,
    _device: rodio::MixerDeviceSink,
}

/// Plays the score from its start, without end.
struct Playing(Arc<Score>, usize);

impl Iterator for Playing {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.1 += 1;
        Some(self.0.sample(self.1 - 1))
    }
}

impl rodio::Source for Playing {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(self.0.intro.channels as u16).expect("checked while decoding")
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(self.0.intro.rate).expect("checked while decoding")
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Music {
    fn open(score: Score) -> Result<Self, Error> {
        // rodio's preferred buffer size is too big for some devices, such as AirPods.
        let mut device = rodio::DeviceSinkBuilder::from_default_device()?
            .with_buffer_size(rodio::cpal::BufferSize::Default)
            .open_sink_or_fallback()?;
        device.log_on_drop(false);
        Ok(Self {
            player: rodio::Player::connect_new(device.mixer()),
            score: Arc::new(score),
            _device: device,
        })
    }

    fn restart(&self) {
        self.player.clear();
        self.player.append(Playing(self.score.clone(), 0));
        self.player.play();
    }
}

pub fn run() -> Result<(), Error> {
    let options = options()?;
    let Some(path) = find_archive(&options) else {
        return match options.smoke {
            Some(_) => Err("Sushi.ati not found".into()),
            None => Ok(()),
        };
    };
    let archive =
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let wav = |name: &str| -> Result<Vec<u8>, Error> {
        let name = format!("Chimp{name}.wav");
        let near = std::fs::read(dir.join("Audio").join(&name));
        Ok(near.or_else(|_| std::fs::read(dir.join(&name)))?)
    };
    let music =
        || -> Result<Music, Error> { Music::open(Score::new(&wav("Intro")?, &wav("Loop")?)?) };
    let music = music()
        .inspect_err(|e| eprintln!("playing without music: {e}"))
        .ok();
    let mut app = App {
        archive,
        music,
        muted: options.mute || options.smoke.is_some(),
        options,
        frames: (0, 0),
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
    archive: Vec<u8>,
    music: Option<Music>,
    muted: bool,
    options: Options,
    /// Shown, and drawn offscreen while hidden.
    frames: (u32, u32),
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
            .with_title("Chimp")
            .with_visible(self.options.smoke.is_none())
            .with_active(self.options.smoke.is_none())
            .with_inner_size(LogicalSize::new(1024, 768));
        let window = Arc::new(el.create_window(attrs)?);
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let player = Player::new(
            &instance,
            window.clone(),
            (size.width, size.height),
            &self.archive,
        );
        let mut player = pollster::block_on(player)?;
        while !player.ready() {
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(c) = self.options.chapter {
            player.show.go(c.max(1) - 1);
        }
        self.player = Some(player);
        if self.options.smoke.is_none() {
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
        if let Some(m) = &self.music {
            if player.show.take_restart() {
                m.restart();
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
        }
        let now = self.start.elapsed().as_secs_f64();
        if !player.frame(now) {
            // Hidden window: nothing paces us, so avoid spinning.
            std::thread::sleep(Duration::from_millis(16));
            if self.options.smoke.is_some() {
                player.frame_offscreen(now);
                self.frames.1 += 1;
            }
        } else {
            self.frames.0 += 1;
        }
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
                if self
                    .options
                    .smoke
                    .is_some_and(|s| self.start.elapsed().as_secs_f64() >= s)
                {
                    let time = self.player.as_ref().map_or(0.0, |p| p.show.time());
                    let (shown, hidden) = self.frames;
                    let music = if self.music.is_some() {
                        "open"
                    } else {
                        "unavailable"
                    };
                    println!(
                        "frames {} (shown {shown}, offscreen {hidden}) time {time:.2} music {music}",
                        shown + hidden
                    );
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
