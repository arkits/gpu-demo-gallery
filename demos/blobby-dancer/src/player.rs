use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wgpu::CurrentSurfaceTexture as Acquired;

use crate::Error;
use crate::data::Files;
use crate::demo::{self, Demo, PROGRAMS};
use crate::gl::{Gl, Gpu};

/// The demo in a window or canvas.
pub struct Player {
    gpu: Gpu,
    gl: Gl,
    pub demo: Demo,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    last: Option<f64>,
    still: bool,
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
        let mut gl = Gl::new(&gpu, &demo::shader(files)?, PROGRAMS, config.format);
        let demo = Demo::new(&gpu, &mut gl, files)?;
        let mut player = Self {
            gpu,
            gl,
            demo,
            surface,
            config,
            last: None,
            still: false,
            ready: Arc::new(AtomicBool::new(false)),
        };
        player.warm_up();
        Ok(player)
    }

    /// Draws once unseen: pipelines may be compiled on first use.
    fn warm_up(&mut self) {
        let t = self.gpu.target((64, 64), self.config.format, 1);
        self.draw(&t.create_view(&Default::default()), (64, 64));
        let done = self.ready.clone();
        self.gpu
            .queue
            .on_submitted_work_done(move || done.store(true, Ordering::Release));
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

    fn advance(&mut self, now: f64) {
        let dt = self.last.map_or(0.0, |l| (now - l).clamp(0.0, 0.1));
        self.last = Some(now);
        if !self.still {
            self.demo.step(dt);
        }
    }

    /// Advances to wall-clock `now` seconds and draws. Returns false if the
    /// surface was unavailable.
    pub fn frame(&mut self, now: f64) -> bool {
        self.advance(now);
        let frame = match self.surface.get_current_texture() {
            Acquired::Success(t) | Acquired::Suboptimal(t) => t,
            Acquired::Outdated | Acquired::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let view = frame.texture.create_view(&Default::default());
        self.draw(&view, (self.config.width, self.config.height));
        self.gpu.queue.present(frame);
        true
    }

    /// Like `frame` but without showing it, for tests. Waits for the GPU.
    pub fn frame_offscreen(&mut self, now: f64) {
        self.advance(now);
        let size = (self.config.width, self.config.height);
        let t = self.gpu.target(size, self.config.format, 1);
        self.draw(&t.create_view(&Default::default()), size);
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// Goes to a moment and stays there, for checking frames.
    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.demo.freeze(seconds, keys);
        self.still = true;
    }

    fn draw(&mut self, view: &wgpu::TextureView, size: (u32, u32)) {
        self.demo.draw(&self.gpu, &mut self.gl, size);
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.gl.finish(&self.gpu, &mut enc, view, size);
        self.gpu.queue.submit([enc.finish()]);
    }
}
