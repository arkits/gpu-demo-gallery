use std::collections::HashMap;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::{MixerDeviceSink, Source};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};
use wolfman::data::Files;
use wolfman::show::{Action, Cue};
use wolfman::{Error, Player};

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
    Some(base.join("wolfman").join("data-path.txt"))
}

/// The files, and where the sounds are.
fn load(p: &Path) -> Result<(Files, PathBuf), Error> {
    Ok((Files::from_dir(p)?, p.join(wolfman::demo::SOUND_DIR)))
}

/// Finds the demo: argument, `WOLFMAN_DATA`, beside or above the program, the
/// last choice, then a dialog.
fn find(o: &Options) -> Result<Option<(Files, PathBuf)>, Error> {
    let given = o
        .path
        .clone()
        .or_else(|| std::env::var_os("WOLFMAN_DATA").map(Into::into));
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
        for p in [d.join("data"), d] {
            if p.join("models").join(wolfman::demo::MODELS[0]).is_file() {
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
        .set_title("Choose the unpacked Wolfman folder")
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
    let Some((files, sounds)) = find(&options)? else {
        return match options.smoke {
            Some(_) => Err("demo data not found".into()),
            None => Ok(()),
        };
    };
    let mut app = Shell {
        files,
        // Tests are silent.
        sound: if options.smoke.is_none() {
            Sound::new(sounds)
        } else {
            None
        },
        options,
        frames: (0, 0),
        window: None,
        player: None,
        start: Instant::now(),
        cursor: PhysicalPosition::default(),
        held: false,
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// 16-bit PCM in a WAV file.
fn wav(d: &[u8]) -> Option<SamplesBuffer> {
    let int = |o: usize| Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?));
    let (mut at, mut format) = (12, None);
    loop {
        let (name, size) = (d.get(at..at + 4)?, int(at + 4)? as usize);
        let body = at + 8;
        match name {
            b"fmt " => format = Some((int(body)?, int(body + 4)?, int(body + 12)? >> 16)),
            b"data" => {
                let (kind, rate, 16) = format? else {
                    return None;
                };
                let channels = NonZero::new((kind >> 16) as u16)?;
                let end = body.checked_add(size)?.min(d.len());
                let samples: Vec<f32> = d[body..end]
                    .chunks_exact(2)
                    .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                    .collect();
                let pcm = kind & 0xffff == 1;
                return pcm.then(|| {
                    SamplesBuffer::new(
                        channels,
                        NonZero::new(rate).unwrap_or(NonZero::<u32>::MIN),
                        samples,
                    )
                });
            }
            _ => {}
        }
        at = body.checked_add(size)? + size % 2;
    }
}

struct Sound {
    dir: PathBuf,
    device: MixerDeviceSink,
    /// By name, with how loud they should be.
    players: HashMap<&'static str, (rodio::Player, f32)>,
    muted: bool,
}

impl Sound {
    fn new(dir: PathBuf) -> Option<Self> {
        // rodio's preferred buffer size is too big for some devices, such as AirPods.
        let mut device = rodio::DeviceSinkBuilder::from_default_device()
            .ok()?
            .with_buffer_size(rodio::cpal::BufferSize::Default)
            .open_sink_or_fallback()
            .ok()?;
        device.log_on_drop(false);
        Some(Self {
            dir,
            device,
            players: HashMap::new(),
            muted: false,
        })
    }

    fn cue(&mut self, cue: Cue) {
        match cue.action {
            Action::Repeat => self.play(cue.name, 1.0, true),
            Action::Once => self.play(cue.name, 1.0, false),
            Action::Stop => _ = self.players.remove(cue.name),
            Action::Rate(rate) => {
                if let Some((player, _)) = self.players.get(cue.name) {
                    player.set_speed(rate);
                }
            }
        }
    }

    fn play(&mut self, name: &'static str, gain: f32, repeat: bool) {
        let filename = format!("{name}.wav");
        let file = std::fs::read_dir(&self.dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .find(|f| {
                f.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&filename)
            });
        let data = file.and_then(|file| std::fs::read(file.path()).ok());
        let Some(buffer) = data.and_then(|d| wav(&d)) else {
            return eprintln!("cannot play {name}.wav");
        };
        let player = rodio::Player::connect_new(self.device.mixer());
        player.set_volume(if self.muted { 0.0 } else { gain });
        match repeat {
            true => player.append(buffer.repeat_infinite()),
            false => player.append(buffer),
        }
        self.players.insert(name, (player, gain));
    }

    fn mute(&mut self) {
        self.muted = !self.muted;
        for (player, gain) in self.players.values() {
            player.set_volume(if self.muted { 0.0 } else { *gain });
        }
    }
}

struct Shell {
    files: Files,
    sound: Option<Sound>,
    options: Options,
    /// Shown, and drawn offscreen while hidden.
    frames: (u32, u32),
    window: Option<Arc<Window>>,
    player: Option<Player>,
    start: Instant,
    cursor: PhysicalPosition<f64>,
    held: bool,
    error: Option<Error>,
}

impl Shell {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Error> {
        let shown = self.options.smoke.is_none();
        let attrs = Window::default_attributes()
            .with_title("Wolfman")
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
        let drawn = player.frame(now);
        for cue in player.show.take_cues() {
            if let Some(s) = &mut self.sound {
                s.cue(cue);
            }
        }
        if drawn {
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
            Key::Named(NamedKey::Space) => _ = player.show.key(" "),
            Key::Named(NamedKey::ArrowUp) => _ = player.show.key("ArrowUp"),
            Key::Named(NamedKey::ArrowDown) => _ = player.show.key("ArrowDown"),
            Key::Named(NamedKey::ArrowLeft) => _ = player.show.key("ArrowLeft"),
            Key::Named(NamedKey::ArrowRight) => _ = player.show.key("ArrowRight"),
            Key::Named(NamedKey::Home) => _ = player.show.key("Home"),
            Key::Character(c) if c.eq_ignore_ascii_case("m") => {
                if let Some(s) = &mut self.sound {
                    s.mute();
                }
            }
            Key::Character(c) => _ = player.show.key(c),
            _ => {}
        }
    }

    /// The cursor as a fraction of the window.
    fn at(&self) -> (f64, f64) {
        let s = self.window.as_ref().map(|w| w.inner_size());
        let s = s.unwrap_or_default();
        let part = |v: f64, n: u32| v / f64::from(n.max(1));
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
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.at();
                let Some(p) = &mut self.player else { return };
                self.held = state == ElementState::Pressed;
                match self.held {
                    true => p.show.press(x, y),
                    false => p.show.release(),
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let (x, y) = self.at();
                if let (true, Some(p)) = (self.held, &mut self.player) {
                    p.show.motion(x, y);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 0.1,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.004,
                };
                if let Some(p) = &mut self.player {
                    p.show.zoom(amount);
                }
            }
            _ => {}
        }
    }
}
