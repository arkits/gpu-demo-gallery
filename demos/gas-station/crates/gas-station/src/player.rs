use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nv_engine::gl::Gpu;
use nv_engine::load::Assets;
use nv_engine::viewer::Viewer;
use wgpu::CurrentSurfaceTexture as Acquired;

use crate::Error;
use crate::demo::Demo;
use crate::present::Present;
use crate::ui::KeyResult;

/// The demo in a window or canvas. Times are seconds from the start.
pub struct Player {
    v: Viewer,
    pub demo: Demo,
    present: Present,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ready: Arc<AtomicBool>,
}

impl Player {
    pub async fn new(
        instance: &wgpu::Instance,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        (width, height): (u32, u32),
        files: &dyn Assets,
        seed: Option<u32>,
        progress: &mut dyn FnMut(&str),
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
        let mut config = surface.get_default_config(&adapter, width.max(1), height.max(1)).ok_or("surface not supported by this adapter")?;
        // The demo works in gamma space and the screen showed its values as they were.
        let caps = surface.get_capabilities(&adapter);
        config.format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(config.format);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        let present = Present::new(&gpu.device, config.format);
        let (v, demo) = crate::start(files, gpu, seed, progress)?;
        let mut player = Player { v, demo, present, surface, config, ready: Arc::new(AtomicBool::new(false)) };
        player.warm_up();
        Ok(player)
    }

    /// Draws once unseen, so that pipelines are made before the first shown frame.
    fn warm_up(&mut self) {
        self.v.update_transforms();
        if let Err(e) = self.v.draw(&mut self.demo) {
            eprintln!("{e}");
        }
        self.v.gl.submit(|_, _| {});
        let done = self.ready.clone();
        self.v.gl.gpu.queue.on_submitted_work_done(move || done.store(true, Ordering::Release));
    }

    /// True once the GPU has done the first frame. Poll before starting.
    pub fn ready(&self) -> bool {
        let _ = self.v.gl.gpu.device.poll(wgpu::PollType::Poll);
        self.ready.load(Ordering::Acquire)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let max = self.v.gl.gpu.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.clamp(1, max), height.clamp(1, max));
        if (width, height) != (self.config.width, self.config.height) {
            (self.config.width, self.config.height) = (width, height);
            self.surface.configure(&self.v.gl.gpu.device, &self.config);
        }
    }

    /// Advances to `now` and draws. Returns false if the surface was unavailable.
    pub fn frame(&mut self, now: f64) -> bool {
        self.step(now);
        let frame = match self.surface.get_current_texture() {
            Acquired::Success(t) | Acquired::Suboptimal(t) => t,
            Acquired::Outdated | Acquired::Lost => {
                self.surface.configure(&self.v.gl.gpu.device, &self.config);
                self.v.gl.submit(|_, _| {});
                return false;
            }
            _ => {
                self.v.gl.submit(|_, _| {});
                return false;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let rect = self.rect();
        let present = &self.present;
        self.v.gl.submit(|enc, gl| present.draw(&gl.gpu.device, enc, &gl.window_color.view, &view, rect));
        self.v.gl.gpu.queue.present(frame);
        true
    }

    /// Like `frame` but without showing it, for tests. Waits for the GPU.
    pub fn frame_offscreen(&mut self, now: f64) {
        self.step(now);
        self.v.gl.submit(|_, _| {});
        let _ = self.v.gl.gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }

    fn step(&mut self, now: f64) {
        self.demo.frame(&mut self.v, now);
        if let Err(e) = self.v.draw(&mut self.demo) {
            eprintln!("{e}");
        }
    }

    fn rect(&self) -> [f32; 4] {
        Present::rect(self.v.gl.window, self.config.width, self.config.height)
    }

    /// A point of the surface in pixels as a fraction of the demo's window, from its top left.
    fn at(&self, x: f64, y: f64) -> (f32, f32) {
        let r = self.rect();
        ((x as f32 - r[0]) / r[2], (y as f32 - r[1]) / r[3])
    }

    /// A typed character. True if it asks to quit.
    pub fn key(&mut self, c: char, now: f64) -> bool {
        matches!(self.demo.key(&mut self.v, c, now), KeyResult::Quit)
    }

    /// An arrow key: 0 up, 1 down, 2 left, 3 right.
    pub fn arrow(&mut self, which: usize, down: bool, now: f64) {
        self.demo.arrow(which, down, now);
    }

    /// A mouse button (0 left, 1 middle, 2 right) at a point of the surface in pixels.
    pub fn mouse_button(&mut self, button: u32, down: bool, x: f64, y: f64, now: f64) {
        let (x, y) = self.at(x, y);
        self.demo.mouse_button(&mut self.v, button, down, x, y, now);
    }

    /// The mouse moving to a point of the surface in pixels; `held` with a button down.
    pub fn mouse_motion(&mut self, x: f64, y: f64, held: bool, now: f64) {
        let (x, y) = self.at(x, y);
        self.demo.mouse_motion(&mut self.v, x, y, held, now);
    }

    pub fn playing(&self) -> bool {
        self.demo.playing
    }
}
