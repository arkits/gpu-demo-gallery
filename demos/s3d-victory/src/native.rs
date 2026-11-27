use std::collections::HashMap;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::{MixerDeviceSink, Source};
use s3d_victory::data::Files;
use s3d_victory::{Error, Player};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

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
    Some(base.join("s3d-victory").join("data-path.txt"))
}

/// Finds the demo: argument, `VICTORY_DATA`, `data` beside or above the program, the
/// last choice, then a dialog.
fn find(o: &Options) -> Result<Option<Files>, Error> {
    let given = o
        .path
        .clone()
        .or_else(|| std::env::var_os("VICTORY_DATA").map(Into::into));
    if let Some(p) = given {
        return Files::from_path(&p).map(Some);
    }
    let exe = std::env::current_exe().ok();
    let above = exe.iter().flat_map(|e| e.ancestors().skip(1).take(6));
    let mut near = std::iter::once(Path::new(".")).chain(above);
    if let Some(files) = near.find_map(|d| Files::from_path(&d.join("data")).ok()) {
        return Ok(Some(files));
    }
    let saved = remembered();
    let last = saved.as_ref().and_then(|s| std::fs::read_to_string(s).ok());
    if let Some(files) = last.and_then(|p| Files::from_path(Path::new(&p)).ok()) {
        return Ok(Some(files));
    }
    if o.smoke.is_some() {
        return Ok(None);
    }
    let dialog = rfd::FileDialog::new().set_title("Choose the unpacked S3D Victory folder");
    let Some(picked) = dialog.pick_folder() else {
        return Ok(None);
    };
    let files = Files::from_path(&picked)?;
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
        // Tests are silent.
        sound: match options.smoke {
            None => Sound::new(),
            Some(_) => None,
        },
        files,
        options,
        frames: (0, 0),
        window: None,
        player: None,
        start: Instant::now(),
        cursor: PhysicalPosition::default(),
        buttons: 0,
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// PCM in a WAV file.
fn wav(d: &[u8]) -> Option<SamplesBuffer> {
    let int = |o: usize| Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?));
    let (mut at, mut format) = (12, None);
    loop {
        let (name, size) = (d.get(at..at + 4)?, int(at + 4)? as usize);
        let body = at + 8;
        match name {
            b"fmt " => format = Some((int(body)?, int(body + 4)?, int(body + 12)? >> 16)),
            b"data" => {
                let (kind, rate, bits) = format?;
                let end = body.checked_add(size)?.min(d.len());
                let samples: Vec<f32> = match (kind & 0xffff, bits) {
                    (1, 16) => d[body..end]
                        .chunks_exact(2)
                        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                        .collect(),
                    (1, 8) => d[body..end]
                        .iter()
                        .map(|b| (f32::from(*b) - 128.0) / 128.0)
                        .collect(),
                    _ => return None,
                };
                let channels = NonZero::new((kind >> 16) as u16)?;
                return Some(SamplesBuffer::new(channels, NonZero::new(rate)?, samples));
            }
            _ => {}
        }
        at = body.checked_add(size)? + size % 2;
    }
}

struct Sound {
    device: MixerDeviceSink,
    playing: HashMap<String, rodio::Player>,
    muted: bool,
}

impl Sound {
    fn new() -> Option<Self> {
        // rodio's preferred buffer size is too big for some devices, such as AirPods.
        let mut device = rodio::DeviceSinkBuilder::from_default_device()
            .ok()?
            .with_buffer_size(rodio::cpal::BufferSize::Default)
            .open_sink_or_fallback()
            .ok()?;
        device.log_on_drop(false);
        Some(Self {
            device,
            playing: HashMap::new(),
            muted: false,
        })
    }

    fn cue(&mut self, files: &Files, cue: &str) {
        if let Some(name) = cue.strip_prefix('-') {
            self.playing.remove(name);
            return;
        }
        let name = cue.trim_end_matches('*');
        let data = files.get(&format!("{name}.wav"));
        let Some(buffer) = data.ok().and_then(wav) else {
            return eprintln!("cannot play {name}.wav");
        };
        let player = rodio::Player::connect_new(self.device.mixer());
        player.set_volume(if self.muted { 0.0 } else { 1.0 });
        match cue.ends_with('*') {
            true => player.append(buffer.repeat_infinite()),
            false => player.append(buffer),
        }
        self.playing.insert(name.to_string(), player);
    }

    fn mute(&mut self) {
        self.muted = !self.muted;
        for p in self.playing.values() {
            p.set_volume(if self.muted { 0.0 } else { 1.0 });
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
    buttons: u8,
    error: Option<Error>,
}

impl Shell {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Error> {
        let shown = self.options.smoke.is_none();
        let attrs = Window::default_attributes()
            .with_title("S3D Victory")
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
        for cue in player.demo.cues() {
            if let Some(s) = &mut self.sound {
                s.cue(&self.files, &cue);
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

    fn key(&mut self, el: &ActiveEventLoop, key: &Key, down: bool) {
        let (Some(window), Some(player)) = (&self.window, &mut self.player) else {
            return;
        };
        let name = match key {
            Key::Named(NamedKey::Escape) => return el.exit(),
            Key::Named(NamedKey::Enter | NamedKey::F11) => {
                if down {
                    let full = window.fullscreen().is_none();
                    window.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
                }
                return;
            }
            Key::Named(NamedKey::Space) => " ".to_string(),
            // The same names as in browsers.
            Key::Named(n) => format!("{n:?}"),
            Key::Character(c) => c.to_string(),
            _ => return,
        };
        if !player.demo.key(&name, down)
            && down
            && name.eq_ignore_ascii_case("m")
            && let Some(s) = &mut self.sound
        {
            s.mute();
        }
    }

    fn pointer(&mut self) {
        let s = self.window.as_ref().map(|w| w.inner_size());
        let s = s.unwrap_or_default();
        let part = |v: f64, n: u32| v / f64::from(n.max(1));
        let (x, y) = (part(self.cursor.x, s.width), part(self.cursor.y, s.height));
        if let Some(p) = &mut self.player {
            p.demo.pointer(x, y, self.buttons);
        }
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
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => self.key(el, &logical_key, state == ElementState::Pressed),
            WindowEvent::MouseInput { state, button, .. } => {
                let bit = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    _ => 0,
                };
                match state {
                    ElementState::Pressed => self.buttons |= bit,
                    ElementState::Released => self.buttons &= !bit,
                }
                self.pointer();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                self.pointer();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y) * 0.1,
                    MouseScrollDelta::PixelDelta(p) => p.y * 0.004,
                };
                if let Some(p) = &mut self.player {
                    p.demo.wheel(amount);
                }
            }
            _ => {}
        }
    }
}
