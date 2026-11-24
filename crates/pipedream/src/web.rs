use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::Player;

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

/// Browser handle to a running player.
#[wasm_bindgen]
pub struct WebPlayer(Player);

#[wasm_bindgen]
impl WebPlayer {
    /// `data` is the user's `Sushi.ati`.
    pub async fn create(canvas: HtmlCanvasElement, data: Vec<u8>) -> Result<WebPlayer, JsError> {
        let size = (canvas.width(), canvas.height());
        let desc = wgpu::InstanceDescriptor::new_without_display_handle();
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let player = Player::new(&instance, target, size, &data).await;
        player
            .map(WebPlayer)
            .map_err(|e| JsError::new(&e.to_string()))
    }
    pub fn duration(&self) -> f32 {
        self.0.duration()
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.0.resize(width, height);
    }
    pub fn frame(&mut self, time: f32) {
        self.0.frame(time);
    }
}
