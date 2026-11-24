//! The whole demo: loaded data plus everything needed to draw one frame.
use sushi_formats::{
    Error, Result,
    archive::Archive,
    path::{self, FlyPath},
    scene,
};

use crate::{camera::Camera, gpu::Gpu, scene_renderer::SceneRenderer, targets::*};

pub struct Demo {
    path: FlyPath,
    scene: SceneRenderer,
}

impl Demo {
    /// `data` is the contents of the user's `Sushi.ati`.
    pub fn load(gpu: &Gpu, data: &[u8]) -> Result<Self> {
        let archive = Archive::open(data)?;
        let file = |name: &str| {
            archive.get(name).ok_or_else(|| Error::Bad {
                at: 0,
                what: format!("{name} not in archive"),
            })
        };
        let text = String::from_utf8_lossy(file("FlyPaths/Animusic.pth")?).into_owned();
        let scene = scene::read_scene(file("Scenes/Animusic.ati")?)?;
        Ok(Self {
            path: path::read_path(&text)?,
            scene: SceneRenderer::new(gpu, &scene, SAMPLES)?,
        })
    }

    /// Seconds covered by the camera path.
    pub fn duration(&self) -> f32 {
        self.path.frames.len() as f32 * self.path.step
    }

    pub fn draw(&self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, targets: &Targets, time: f32) {
        let size = targets.resolved.size();
        let aspect = size.width as f32 / size.height as f32;
        let camera = Camera::on_path(&self.path, time);
        self.scene
            .prepare(gpu, Camera::projection(aspect) * camera.view());
        let mut pass = targets.begin(enc);
        self.scene.draw(&mut pass);
    }
}
