use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use image::ImageFormat;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Fullscreen, Window, WindowId},
};

use crate::{
    Demo, Player, Result, Scene,
    gpu::{Controls, Renderer, device},
};

struct Audio {
    _device: rodio::MixerDeviceSink,
    player: rodio::Player,
    energy: Option<Arc<AtomicU32>>,
}

struct GeoAudio<S> {
    source: S,
    energy: Arc<AtomicU32>,
    low_pass: f32,
    envelope: f32,
}

impl<S: rodio::Source> Iterator for GeoAudio<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.source.next()?;
        self.low_pass += (sample - self.low_pass) * 0.009;
        self.envelope = self.low_pass.abs().max(self.envelope * 0.9998);
        self.energy.store(
            (self.envelope * 3.1).clamp(0.0, 1.0).to_bits(),
            Ordering::Relaxed,
        );
        Some(sample)
    }
}

impl<S: rodio::Source> rodio::Source for GeoAudio<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.source.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
    fn try_seek(
        &mut self,
        position: Duration,
    ) -> std::result::Result<(), rodio::source::SeekError> {
        self.source.try_seek(position)
    }
}

impl Audio {
    fn open(path: &Path, demo: Demo) -> Option<Self> {
        let root = path.parent()?.join(demo.slug());
        let (intro, looping) = match demo {
            Demo::Nature => (Some("fallsintro.mp3"), None),
            Demo::Rachel => (None, Some("rachelloop.mp3")),
            Demo::Island1 => (Some("island1intro.mp3"), Some("island1loop.mp3")),
            Demo::Island2 => (Some("island2intro.mp3"), Some("island2loop.mp3")),
            Demo::Island3 => (Some("island3intro.mp3"), Some("island3loop.mp3")),
            _ => return None,
        };
        let open = |name: &str| fs::File::open(root.join(name)).ok().map(BufReader::new);
        let intro = intro.and_then(open);
        let looping = looping.and_then(open);
        if intro.is_none() && looping.is_none() {
            return None;
        }
        let device = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
        let player = rodio::Player::connect_new(device.mixer());
        if let Some(source) = intro.and_then(|file| rodio::Decoder::new(file).ok()) {
            player.append(source);
        }
        if let Some(source) = looping.and_then(|file| rodio::Decoder::new_looped(file).ok()) {
            player.append(source);
        }
        Some(Self {
            _device: device,
            player,
            energy: None,
        })
    }

    fn geo(path: &Path) -> Result<Self> {
        let source = rodio::Decoder::new_looped(BufReader::new(fs::File::open(path)?))?;
        let device = rodio::DeviceSinkBuilder::open_default_sink()?;
        let player = rodio::Player::connect_new(device.mixer());
        let energy = Arc::new(AtomicU32::new(0));
        player.append(GeoAudio {
            source,
            energy: energy.clone(),
            low_pass: 0.0,
            envelope: 0.0,
        });
        Ok(Self {
            _device: device,
            player,
            energy: Some(energy),
        })
    }
}

struct App {
    demo: Demo,
    scene: Option<Scene>,
    window: Option<Arc<Window>>,
    player: Option<Player>,
    started: Instant,
    audio: Option<Audio>,
    drag: bool,
    mouse: Option<(f64, f64)>,
    result: Result<()>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(scene) = self.scene.take() else {
            return;
        };
        let result = (|| {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title(self.demo.title())
                        .with_inner_size(LogicalSize::new(960, 720)),
                )?,
            );
            let size = window.inner_size();
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let player = pollster::block_on(Player::new(
                &instance,
                window.clone(),
                (size.width, size.height),
                scene,
            ))?;
            self.window = Some(window);
            self.player = Some(player);
            self.started = Instant::now();
            Ok(())
        })();
        if result.is_err() {
            self.result = result;
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let (Some(window), Some(player)) = (&self.window, &mut self.player) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Some(energy) = self.audio.as_ref().and_then(|a| a.energy.as_ref()) {
                    player.controls.audio = Some(f32::from_bits(energy.load(Ordering::Relaxed)));
                }
                player.frame(self.started.elapsed().as_secs_f64());
                window.request_redraw();
            }
            WindowEvent::Resized(size) => {
                player.resize(size.width, size.height);
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.drag = state == ElementState::Pressed;
                self.mouse = None;
            }
            WindowEvent::CursorMoved { position, .. } if self.drag => {
                if let Some((x, y)) = self.mouse.replace((position.x, position.y)) {
                    player.controls.yaw += (position.x - x) as f32 * 0.003;
                    player.controls.pitch =
                        (player.controls.pitch + (position.y - y) as f32 * 0.003).clamp(-1.4, 1.4);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let step = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.025,
                };
                player.controls.zoom = (player.controls.zoom - step * 0.07).clamp(-1.0, 0.9);
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state.is_pressed() && !event.repeat =>
            {
                let key = match &event.logical_key {
                    Key::Named(NamedKey::Escape) => return event_loop.exit(),
                    Key::Named(NamedKey::Space) => " ",
                    Key::Named(NamedKey::ArrowLeft) => "ArrowLeft",
                    Key::Named(NamedKey::ArrowRight) => "ArrowRight",
                    Key::Character(c) => c.as_str(),
                    _ => return,
                };
                player.controls.key(key);
                if key == "f" {
                    window.set_fullscreen(
                        window
                            .fullscreen()
                            .is_none()
                            .then_some(Fullscreen::Borderless(None)),
                    );
                }
                if let Some(audio) = &self.audio {
                    if key.eq_ignore_ascii_case("r") {
                        let _ = audio.player.try_seek(Duration::ZERO);
                    }
                    if player.controls.paused {
                        audio.player.pause();
                    } else {
                        audio.player.play();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

async fn screenshot(scene: Scene, seconds: f32, path: &Path) -> Result<()> {
    let (width, height) = (1024, 768);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance.request_adapter(&Default::default()).await?;
    let (device, queue) = device(&adapter).await?;
    let renderer = Renderer::new(device, queue, scene, format, (width, height));
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: u64::from(width * height * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let mut controls = Controls::default();
    controls.time = seconds;
    renderer.draw(
        &mut encoder,
        &texture.create_view(&Default::default()),
        &controls,
    );
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
        },
        size,
    );
    renderer.queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (send, recv) = std::sync::mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = send.send(result);
    });
    renderer.device.poll(wgpu::PollType::wait_indefinitely())?;
    recv.recv()??;
    image::save_buffer_with_format(
        path,
        &slice.get_mapped_range()?,
        width,
        height,
        image::ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    Ok(())
}

pub fn run_native() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let demos = Demo::ALL.map(Demo::slug).join(" | ");
    let help = format!(
        "radeon-8500 <{demos}> [pack.zip] [--inspect | --shot SECONDS FILE.png] [--audio MP3_OR_WAV]"
    );
    let Some(first) = args.next() else {
        println!("{help}");
        return Ok(());
    };
    if matches!(first.as_str(), "--help" | "-h") {
        println!("{help}");
        return Ok(());
    }
    let demo = Demo::from_slug(&first)?;
    let mut pack = PathBuf::from("data").join(format!("{}.zip", demo.slug()));
    let mut inspect = false;
    let mut shot = None;
    let mut source = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--inspect" => inspect = true,
            "--audio" if demo == Demo::GeoForms => {
                source = Some(PathBuf::from(
                    args.next().ok_or("--audio needs a file path")?,
                ));
            }
            "--shot" => {
                let time = args
                    .next()
                    .ok_or("--shot needs seconds and output path")?
                    .parse::<f32>()?;
                if !time.is_finite() || time < 0.0 {
                    return Err("invalid screenshot time".into());
                }
                let path = PathBuf::from(args.next().ok_or("--shot needs output path")?);
                shot = Some((time, path));
            }
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option {unknown}").into());
            }
            path => pack = path.into(),
        }
    }
    let bytes = fs::read(&pack).map_err(|e| format!("{}: {e}", pack.display()))?;
    let scene = demo
        .scene(&bytes)
        .map_err(|e| format!("{}: {e}", demo.title()))?;
    if inspect {
        let (meshes, tris, tex) = scene.geometry_counts();
        println!(
            "{} | {meshes} meshes | {tris} triangles | {tex} textures",
            demo.title()
        );
        return Ok(());
    }
    if let Some((seconds, path)) = shot {
        return pollster::block_on(screenshot(scene, seconds, &path));
    }
    let audio = if let Some(path) = source {
        Some(Audio::geo(&path)?)
    } else {
        Audio::open(&pack, demo)
    };
    let mut app = App {
        demo,
        scene: Some(scene),
        window: None,
        player: None,
        started: Instant::now(),
        audio,
        drag: false,
        mouse: None,
        result: Ok(()),
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.result
}
