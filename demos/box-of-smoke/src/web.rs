use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::Player;
use crate::controls::Button;
use crate::data::Files;

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

/// The demo files as they arrive, by name relative to the demo folder.
#[wasm_bindgen]
#[derive(Default)]
pub struct WebFiles(Files);

#[wasm_bindgen]
impl WebFiles {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }
    /// Opens a pack made by `scripts/pack.sh`.
    pub fn open(pack: &[u8]) -> Result<WebFiles, JsError> {
        let files = Files::open(pack).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self(files))
    }
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.0.insert(name, data);
    }
    pub fn names() -> Vec<String> {
        crate::data::FILES.map(String::from).to_vec()
    }
}

/// Browser handle to a running player.
#[wasm_bindgen]
pub struct WebPlayer(Player);

#[wasm_bindgen]
impl WebPlayer {
    pub async fn create(canvas: HtmlCanvasElement, files: &WebFiles) -> Result<WebPlayer, JsError> {
        let size = (canvas.width(), canvas.height());
        let desc = wgpu::InstanceDescriptor::new_without_display_handle();
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let player = Player::new(&instance, target, size, &files.0).await;
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
    pub fn frame(&mut self, now: f64) {
        self.0.frame(now);
    }
    pub fn key(&mut self, key: &str) -> bool {
        self.0.app.controls.key(key)
    }
    /// Positions are fractions of the canvas, from its top left.
    pub fn press(&mut self, left: bool, rotate: bool, x: f32, y: f32) {
        let button = if left { Button::Left } else { Button::Other };
        self.0.app.controls.press(button, rotate, (x, y));
    }
    pub fn motion(&mut self, x: f32, y: f32) {
        self.0.app.controls.motion((x, y));
    }
    pub fn release(&mut self) {
        self.0.app.controls.release();
    }
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.0.app.controls.rotate(dx, dy);
    }
    pub fn zoom(&mut self, amount: f32) {
        self.0.app.controls.zoom(amount);
    }
}
