use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wgpu::CurrentSurfaceTexture as Acquired;

use crate::Error;
use crate::data::Files;
use crate::gpu::Gpu;
use crate::scene::Scene;
use crate::show::Show;

/// The demo in a window or canvas.
pub struct Player {
    gpu: Gpu,
    scene: Scene,
    pub show: Show,
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
        let scene = Scene::new(&gpu, files, config.format)?;
        let show = scene.show();
        let mut player = Self {
            gpu,
            scene,
            show,
            surface,
            config,
            ready: Arc::new(AtomicBool::new(false)),
        };
        player.warm_up();
        Ok(player)
    }

    /// Draws everything once unseen: pipelines may be compiled on first use.
    fn warm_up(&mut self) {
        let t = self.gpu.target(64, 64, self.config.format, 1);
        let view = t.create_view(&Default::default());
        let frame = self.show.frame();
        let mut enc = self.encoder();
        self.scene
            .draw(&self.gpu, &mut enc, &frame, &view, (64, 64));
        self.gpu.queue.submit([enc.finish()]);
        self.scene.reset_exposure();
        let done = self.ready.clone();
        self.gpu
            .queue
            .on_submitted_work_done(move || done.store(true, Ordering::Release));
    }

    fn encoder(&self) -> wgpu::CommandEncoder {
        self.gpu.device.create_command_encoder(&Default::default())
    }

    /// True once the GPU has compiled everything. Poll before starting.
    pub fn ready(&self) -> bool {
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        self.ready.load(Ordering::Acquire)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let max = self.gpu.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.clamp(1, max), height.clamp(1, max));
        if (width, height) != (self.config.width, self.config.height) {
            (self.config.width, self.config.height) = (width, height);
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    /// Advances to wall-clock `now` seconds and draws. Returns false if the
    /// surface was unavailable.
    pub fn frame(&mut self, now: f64) -> bool {
        self.show.advance(now);
        let frame = match self.surface.get_current_texture() {
            Acquired::Success(t) | Acquired::Suboptimal(t) => t,
            Acquired::Outdated | Acquired::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let view = frame.texture.create_view(&Default::default());
        self.draw(&view);
        self.gpu.queue.present(frame);
        true
    }

    /// Like `frame` but without showing it, for tests. Waits for the GPU.
    pub fn frame_offscreen(&mut self, now: f64) {
        self.show.advance(now);
        let (w, h) = (self.config.width, self.config.height);
        let t = self.gpu.target(w, h, self.config.format, 1);
        self.draw(&t.create_view(&Default::default()));
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }

    fn draw(&mut self, view: &wgpu::TextureView) {
        let mut enc = self.encoder();
        let size = (self.config.width, self.config.height);
        let frame = self.show.frame();
        self.scene.draw(&self.gpu, &mut enc, &frame, view, size);
        self.gpu.queue.submit([enc.finish()]);
    }
}
