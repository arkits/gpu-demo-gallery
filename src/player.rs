use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wgpu::CurrentSurfaceTexture as Acquired;

use crate::Error;
use crate::controls::{Command, Controls, STEP_MS};
use crate::data::{Files, read_path};
use crate::gpu::Gpu;
use crate::scene::{Frame, Scene};
use crate::sim::Sim;

/// Catching up is limited, so a slow machine runs slowly instead of stalling.
const MAX_STEPS: u32 = 2;

/// The demo without a window.
pub struct App {
    pub gpu: Gpu,
    pub controls: Controls,
    pub sim: Sim,
    scene: Scene,
    path: Vec<[f32; 3]>,
    last: Option<f64>,
    behind: f64,
}

impl App {
    pub fn new(gpu: Gpu, files: &Files, window: wgpu::TextureFormat) -> Result<Self, Error> {
        let path = read_path(std::str::from_utf8(files.get("models/trapped.obj")?)?)?;
        Ok(Self {
            sim: Sim::new(&gpu, files)?,
            scene: Scene::new(&gpu, files, window)?,
            controls: Controls::new(path.clone()),
            path,
            gpu,
            last: None,
            behind: 0.0,
        })
    }

    /// Returns to the state at startup.
    fn restart(&mut self) {
        self.controls = Controls::new(self.path.clone());
        self.controls.key("s");
        self.sim.rewind();
        self.last = None;
        self.behind = 0.0;
    }

    /// Runs the original's frames that are due at `now` seconds.
    pub fn advance(&mut self, now: f64, enc: &mut wgpu::CommandEncoder) {
        let elapsed = now - self.last.replace(now).unwrap_or(now - STEP_MS / 1000.0);
        self.behind += elapsed * 1000.0 / STEP_MS;
        // Steady 60 Hz displays get exactly one step a frame despite jitter.
        let due = (self.behind + 0.25).floor().max(0.0);
        self.behind = (self.behind - due).clamp(-0.5, 0.5);
        self.sim.begin();
        for _ in 0..(due as u32).min(MAX_STEPS) {
            self.step(enc);
        }
    }

    pub fn step(&mut self, enc: &mut wgpu::CommandEncoder) {
        for c in self.controls.take_commands() {
            let water = self.controls.settings.water;
            self.sim.reset(&self.gpu, enc, water);
            if c == Command::Logo && !water {
                self.sim.stamp_logo(&self.gpu, enc);
            }
        }
        if self.controls.runs() {
            let c = &self.controls;
            self.sim.step(&self.gpu, enc, &c.settings, &c.inject);
        }
        self.controls.step();
    }

    pub fn draw(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        let c = &self.controls;
        let frame = Frame {
            view: c.view(),
            eye: c.eye(),
            water: c.settings.water,
            supersample: c.supersample,
        };
        let density = self.sim.density();
        self.scene.draw(&self.gpu, enc, &frame, density, view, size);
    }
}

/// The demo in a window or canvas.
pub struct Player {
    pub app: App,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ready: Arc<AtomicBool>,
}

impl Player {
    pub async fn new(
        instance: &wgpu::Instance,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        (width, height): (u32, u32),
        files: &Files,
    ) -> Result<Self, Error> {
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        let gpu = Gpu::from_adapter(&adapter).await?;
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface not supported by this adapter")?;
        let caps = surface.get_capabilities(&adapter);
        let plain = caps.formats.iter().copied().find(|f| !f.is_srgb());
        config.format = plain.unwrap_or(config.format);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        let app = App::new(gpu, files, config.format)?;
        let mut player = Self {
            app,
            surface,
            config,
            ready: Arc::new(AtomicBool::new(false)),
        };
        player.warm_up();
        Ok(player)
    }

    /// Runs every pass once unseen: pipelines may be compiled on first use.
    fn warm_up(&mut self) {
        let size = (64, 64);
        let format = self.config.format;
        let target = self.app.gpu.target(size.0, size.1, format);
        let view = target.create_view(&Default::default());
        for key in ["n", "w", "s"] {
            self.app.controls.key(key);
            let mut enc = self.encoder();
            self.app.sim.begin();
            self.app.step(&mut enc);
            self.app.draw(&mut enc, &view, size);
            self.app.gpu.queue.submit([enc.finish()]);
        }
        // Forget the steps taken here.
        self.app.restart();
        let done = self.ready.clone();
        self.app
            .gpu
            .queue
            .on_submitted_work_done(move || done.store(true, Ordering::Release));
    }

    fn encoder(&self) -> wgpu::CommandEncoder {
        let device = &self.app.gpu.device;
        device.create_command_encoder(&Default::default())
    }

    /// True once the GPU has compiled everything. Poll before starting.
    pub fn ready(&self) -> bool {
        let _ = self.app.gpu.device.poll(wgpu::PollType::Poll);
        self.ready.load(Ordering::Acquire)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let max = self.app.gpu.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.clamp(1, max), height.clamp(1, max));
        if (width, height) != (self.config.width, self.config.height) {
            (self.config.width, self.config.height) = (width, height);
            self.surface.configure(&self.app.gpu.device, &self.config);
        }
    }

    /// Advances to wall-clock `now` seconds and draws. Returns false if the
    /// surface was unavailable.
    pub fn frame(&mut self, now: f64) -> bool {
        let frame = match self.surface.get_current_texture() {
            Acquired::Success(t) | Acquired::Suboptimal(t) => t,
            Acquired::Outdated | Acquired::Lost => {
                self.surface.configure(&self.app.gpu.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let mut enc = self.encoder();
        self.app.advance(now, &mut enc);
        let view = frame.texture.create_view(&Default::default());
        let size = (self.config.width, self.config.height);
        self.app.draw(&mut enc, &view, size);
        self.app.gpu.queue.submit([enc.finish()]);
        self.app.gpu.queue.present(frame);
        true
    }

    /// Like `frame` but without showing it, for tests. Waits for the GPU.
    pub fn frame_offscreen(&mut self, now: f64) {
        let size = (self.config.width, self.config.height);
        let target = self.app.gpu.target(size.0, size.1, self.config.format);
        let mut enc = self.encoder();
        self.app.advance(now, &mut enc);
        self.app
            .draw(&mut enc, &target.create_view(&Default::default()), size);
        self.app.gpu.queue.submit([enc.finish()]);
        let _ = self
            .app
            .gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely());
    }
}
