use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::{Demo, Player};

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct WebPlayer(Player);

#[wasm_bindgen]
impl WebPlayer {
    pub async fn create(
        canvas: HtmlCanvasElement,
        slug: &str,
        content: &[u8],
    ) -> Result<Self, JsError> {
        let error = |e: crate::Error| JsError::new(&e.to_string());
        let demo = Demo::from_slug(slug).map_err(error)?;
        let scene = demo.scene(content).map_err(error)?;
        let size = (canvas.width(), canvas.height());
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        Player::new(&instance, target, size, scene)
            .await
            .map(Self)
            .map_err(error)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.0.resize(width, height);
    }

    pub fn frame(&mut self, now: f64) -> bool {
        self.0.frame(now)
    }

    pub fn key(&mut self, key: &str) -> bool {
        self.0.controls.key(key)
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.0.controls.yaw += dx * 0.003;
        self.0.controls.pitch = (self.0.controls.pitch + dy * 0.003).clamp(-1.4, 1.4);
    }

    pub fn zoom(&mut self, step: f32) {
        self.0.controls.zoom = (self.0.controls.zoom - step * 0.07).clamp(-1.0, 0.9);
    }

    pub fn audio(&mut self, level: Option<f32>) {
        self.0.controls.audio = level.map(|level| level.clamp(0.0, 1.0));
    }
}
