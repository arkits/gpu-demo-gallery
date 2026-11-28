use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gas_station::files::Files;
use gas_station::{Error, Player};
use rodio::buffer::SamplesBuffer;
use rodio::{MixerDeviceSink, Source};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
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
    Some(base.join("gas-station").join("data-path.txt"))
}

/// The demo folder (its `asm/` made by `tools/nv_compile.py`) or a pack, and where the
/// sounds are.
fn load(p: &Path) -> Result<(Files, PathBuf), Error> {
    if p.is_file() {
        let files = Files::open(&std::fs::read(p)?)?;
        return Ok((files, p.parent().unwrap_or(Path::new(".")).join("sounds")));
    }
    let files = Files::from_dir(p)?;
    if files.get("asm/index.json").is_none() {
        return Err(format!(
            "{} has no compiled programs: run `python3 tools/nv_compile.py \"{0}\" \"{0}/asm\"` first",
            p.display()
        )
        .into());
    }
    Ok((files, p.join("sounds")))
}

fn is_demo(p: &Path) -> bool {
    p.join("bin/args.txt").is_file() && p.join("models/menu.nmb").is_file()
}

/// Finds the demo: argument, `GAS_STATION_DATA`, beside or above the program, the last
/// choice, then a dialog.
fn find(o: &Options) -> Result<Option<(Files, PathBuf)>, Error> {
    let given = o.path.clone().or_else(|| std::env::var_os("GAS_STATION_DATA").map(Into::into));
    if let Some(p) = given {
        return load(&p).map(Some);
    }
    let exe = std::env::current_exe().ok();
    let mut near = vec![PathBuf::from(".")];
    near.extend(exe.iter().flat_map(|e| e.ancestors().skip(1).take(6)).map(Path::to_path_buf));
    for d in near {
        for p in [d.join("data"), d] {
            if is_demo(&p) {
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
    let Some(picked) = rfd::FileDialog::new().set_title("Choose the Gas Station demo folder").pick_folder() else {
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
        sound: if options.smoke.is_none() { Sound::new(&sounds) } else { None },
        options,
        frames: (0, 0),
        window: None,
        player: None,
        start: Instant::now(),
        cursor: PhysicalPosition::default(),
        held: 0,
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
                let (kind, rate, 16) = format? else { return None };
                let channels = NonZero::new((kind >> 16) as u16)?;
                let end = body.checked_add(size)?.min(d.len());
                let samples: Vec<f32> = d[body..end].chunks_exact(2).map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0).collect();
                let pcm = kind & 0xffff == 1;
                return pcm.then(|| SamplesBuffer::new(channels, NonZero::new(rate).unwrap_or(NonZero::<u32>::MIN), samples));
            }
            _ => {}
        }
        at = body.checked_add(size)? + size % 2;
    }
}

/// Both sounds loop from the start at full volume, whatever the demo does.
struct Sound {
    _device: MixerDeviceSink,
    players: Vec<rodio::Player>,
    muted: bool,
}

impl Sound {
    fn new(dir: &Path) -> Option<Self> {
        let mut device = rodio::DeviceSinkBuilder::from_default_device()
            .ok()?
            .with_buffer_size(rodio::cpal::BufferSize::Default)
            .open_sink_or_fallback()
            .ok()?;
        device.log_on_drop(false);
        let mut players = vec![];
        for name in gas_station::SOUNDS {
            let data = std::fs::read(dir.join(format!("{name}.wav"))).ok();
            let Some(buffer) = data.and_then(|d| wav(&d)) else {
                eprintln!("cannot play {name}.wav");
                continue;
            };
            let p = rodio::Player::connect_new(device.mixer());
            p.append(buffer.repeat_infinite());
            players.push(p);
        }
        Some(Self { _device: device, players, muted: false })
    }

    fn mute(&mut self) {
        self.muted = !self.muted;
        for p in &self.players {
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
    /// Mouse buttons held, a bit each.
    held: u32,
    error: Option<Error>,
}

impl Shell {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Error> {
        let shown = self.options.smoke.is_none();
        let attrs = Window::default_attributes()
            .with_title(gas_station::TITLE)
            .with_visible(shown)
            .with_active(shown)
            .with_inner_size(LogicalSize::new(1024, 768));
        let window = Arc::new(el.create_window(attrs)?);
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let mut quiet = |_: &str| {};
        let player = pollster::block_on(Player::new(&instance, window.clone(), (size.width, size.height), &self.files, None, &mut quiet))?;
        while !player.ready() {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.player = Some(player);
        // The pack's bytes are on the GPU now.
        self.files = Files::default();
        if shown {
            window.focus_window();
        }
        self.start = Instant::now();
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn redraw(&mut self) {
        let now = self.now();
        let Some(player) = &mut self.player else { return };
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

    fn key(&mut self, el: &ActiveEventLoop, key: &Key, down: bool, repeat: bool) {
        let now = self.now();
        let (Some(window), Some(player)) = (&self.window, &mut self.player) else { return };
        let arrow = match key {
            Key::Named(NamedKey::ArrowUp) => Some(0),
            Key::Named(NamedKey::ArrowDown) => Some(1),
            Key::Named(NamedKey::ArrowLeft) => Some(2),
            Key::Named(NamedKey::ArrowRight) => Some(3),
            _ => None,
        };
        if let Some(a) = arrow {
            if !repeat {
                player.arrow(a, down, now);
            }
            return;
        }
        if !down {
            return;
        }
        match key {
            Key::Named(NamedKey::Enter | NamedKey::F11) if !repeat => {
                let full = window.fullscreen().is_none();
                window.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
            }
            Key::Named(NamedKey::Escape) => el.exit(),
            Key::Named(NamedKey::Space) => _ = player.key(' ', now),
            Key::Character(c) if c.as_str() == "m" || c.as_str() == "M" => {
                if !repeat && let Some(s) = &mut self.sound {
                    s.mute();
                }
            }
            Key::Character(c) => {
                for ch in c.chars() {
                    if player.key(ch, now) {
                        el.exit();
                    }
                }
            }
            _ => {}
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
                let elapsed = self.now();
                if self.options.smoke.is_some_and(|s| elapsed >= s) {
                    let (shown, hidden) = self.frames;
                    println!("frames {} (shown {shown}, offscreen {hidden})", shown + hidden);
                    return el.exit();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key, state, repeat, .. }, .. } => {
                self.key(el, &logical_key, state == ElementState::Pressed, repeat)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                    _ => return,
                };
                let down = state == ElementState::Pressed;
                if down {
                    self.held |= 1 << b;
                } else {
                    self.held &= !(1 << b);
                }
                let (now, c) = (self.now(), self.cursor);
                if let Some(p) = &mut self.player {
                    p.mouse_button(b, down, c.x, c.y, now);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                let (now, held) = (self.now(), self.held != 0);
                if let Some(p) = &mut self.player {
                    p.mouse_motion(position.x, position.y, held, now);
                }
            }
            _ => {}
        }
    }
}
