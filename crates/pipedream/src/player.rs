use sushi_engine::{
    demo::Demo,
    gpu::Gpu,
    targets::{Presenter, Targets},
};
use wgpu::CurrentSurfaceTexture as Frame;

type Error = Box<dyn std::error::Error>;

pub struct Player {
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    targets: Targets,
    presenter: Presenter,
    demo: Demo,
}

impl Player {
    pub async fn new(
        instance: &wgpu::Instance,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        (width, height): (u32, u32),
        data: &[u8],
    ) -> Result<Self, Error> {
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let gpu = Gpu::from_adapter(&adapter).await?;
        let caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or("surface not supported by this adapter")?;
        // The renderer works in gamma space, as the original did.
        config.format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(config.format);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        let targets = Targets::new(&gpu, config.width, config.height);
        let presenter = Presenter::new(&gpu, config.format);
        let demo = Demo::load(&gpu, data)?;
        Ok(Self {
            gpu,
            surface,
            config,
            targets,
            presenter,
            demo,
        })
    }

    pub fn duration(&self) -> f32 {
        self.demo.duration()
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

    /// Draws the frame for `time` seconds. Skips if the surface is unavailable.
    pub fn frame(&mut self, time: f32) {
        let frame = match self.surface.get_current_texture() {
            Frame::Success(t) | Frame::Suboptimal(t) => t,
            Frame::Outdated | Frame::Lost => {
                self.surface.configure(&self.gpu.device, &self.config);
                return;
            }
            _ => return,
        };
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.demo.draw(&self.gpu, &mut enc, &self.targets, time);
        let view = frame.texture.create_view(&Default::default());
        self.presenter
            .present(&self.gpu, &mut enc, &self.targets, &view);
        self.gpu.queue.submit([enc.finish()]);
        self.gpu.queue.present(frame);
    }
}
