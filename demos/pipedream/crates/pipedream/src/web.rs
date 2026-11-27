use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::{Data, Player};

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

/// Browser handle to a running player.
#[wasm_bindgen]
pub struct WebPlayer(Player);

#[wasm_bindgen]
impl WebPlayer {
    /// `archive` is the user's `Sushi.ati`; `logo` is `Logo2.tga`.
    pub async fn create(
        canvas: HtmlCanvasElement,
        archive: Vec<u8>,
        logo: Option<Vec<u8>>,
    ) -> Result<WebPlayer, JsError> {
        let size = (canvas.width(), canvas.height());
        let desc = wgpu::InstanceDescriptor::new_without_display_handle();
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let player = Player::new(&instance, target, size, &Data { archive, logo }).await;
        player
            .map(WebPlayer)
            .map_err(|e| JsError::new(&e.to_string()))
    }
    pub fn ready(&self) -> bool {
        self.0.ready()
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.0.resize(width, height);
    }
    /// `now` in seconds; `audio` is the music position while it plays.
    pub fn frame(&mut self, now: f64, audio: Option<f64>) {
        self.0.frame(now, audio);
    }
    /// Draws one fixed moment of the scene.
    pub fn freeze(&mut self, time: f32) {
        self.0.show.set_time(time);
        if !self.0.show.paused() {
            self.0.show.key(" ");
        }
    }
    pub fn paused(&self) -> bool {
        self.0.show.paused()
    }
    pub fn brightness(&self) -> f32 {
        self.0.show.brightness()
    }
    pub fn time(&self) -> f32 {
        self.0.show.time()
    }
    /// Music position to move to, once after each jump.
    pub fn take_seek(&mut self) -> Option<f64> {
        self.0.show.take_seek()
    }
    pub fn key(&mut self, key: &str) -> bool {
        self.0.show.key(key)
    }
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.0.show.drag(dx, dy);
    }
    pub fn zoom(&mut self, amount: f32) {
        self.0.show.zoom(amount);
    }
}
