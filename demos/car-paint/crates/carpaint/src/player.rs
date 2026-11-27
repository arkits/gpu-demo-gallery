use sushi_engine::demo::Demo;
use sushi_engine::gpu::Gpu;
use sushi_engine::overlay::{Corner, Overlay};
use sushi_engine::targets::{Presenter, Targets};
use wgpu::CurrentSurfaceTexture as Frame;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::Show;

type Error = Box<dyn std::error::Error>;

pub struct Player {
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    targets: Targets,
    presenter: Presenter,
    overlay: Overlay,
    demo: Demo,
    pub show: Show,
    ready: Arc<AtomicBool>,
}

impl Player {
    pub async fn new(
        instance: &wgpu::Instance,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        (width, height): (u32, u32),
        archive: &[u8],
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
        let caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface not supported by this adapter")?;
        // The renderer works in gamma space, as the original did.
        let plain = caps.formats.iter().copied().find(|f| !f.is_srgb());
        config.format = plain.unwrap_or(config.format);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        let targets = Targets::new(&gpu, config.width, config.height);
        let presenter = Presenter::new(&gpu, config.format);
        let mut demo = Demo::load(&gpu, archive)?;
        let mut overlay = Overlay::new(&gpu);
        // Sizes and corners from 0x421c50.
        if !demo.chip_logo().is_empty() {
            overlay.add_logo(&gpu, demo.chip_logo(), Corner::BottomRight, 0.27);
        }
        overlay.set_font(&gpu, demo.font());
        let ready = Arc::new(AtomicBool::new(false));
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        demo.warm_up(&gpu, &mut enc, &targets);
        gpu.queue.submit([enc.finish()]);
        let done = ready.clone();
        gpu.queue
            .on_submitted_work_done(move || done.store(true, Ordering::Release));
        let show = demo.chapters().iter().map(|c| f64::from(c.duration()));
        let show = Show::new(show.collect());
        Ok(Self {
            gpu,
            surface,
            config,
            targets,
            presenter,
            overlay,
            demo,
            show,
            ready,
        })
    }

    pub fn titles(&self) -> Vec<&'static str> {
        self.demo.chapters().iter().map(|c| c.title).collect()
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
            self.targets = Targets::new(&self.gpu, width, height);
        }
    }

    /// Advances the show to wall-clock `now` and draws it. Returns false if the
    /// surface was unavailable and nothing was drawn.
    pub fn frame(&mut self, now: f64) -> bool {
        self.show.advance(now);
        let frame = match self.surface.get_current_texture() {
            Frame::Success(t) | Frame::Suboptimal(t) => t,
            Frame::Outdated | Frame::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let mut enc = self.render();
        let gpu = &self.gpu;
        let view = frame.texture.create_view(&Default::default());
        self.presenter.present(gpu, &mut enc, &self.targets, &view);
        gpu.queue.submit([enc.finish()]);
        gpu.queue.present(frame);
        true
    }

    /// Draws the current frame without showing it, for tests without a visible
    /// window. Waits for the GPU.
    pub fn frame_offscreen(&mut self, now: f64) {
        self.show.advance(now);
        let enc = self.render();
        self.gpu.queue.submit([enc.finish()]);
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }

    fn render(&mut self) -> wgpu::CommandEncoder {
        let (gpu, show) = (&self.gpu, &self.show);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        let (chapter, time) = (show.chapter(), show.time());
        let marks = self
            .demo
            .draw(gpu, &mut enc, &self.targets, chapter, time, show.camera());
        self.overlay.draw(
            gpu,
            &mut enc,
            &self.targets,
            &marks,
            show.logos,
            show.brightness(),
        );
        enc
    }
}
