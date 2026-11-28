use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::Player;
use crate::files::Files;

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

/// The demo's files: a pack made by `scripts/pack.sh`.
#[wasm_bindgen]
pub struct WebFiles(Files);

#[wasm_bindgen]
impl WebFiles {
    pub fn open(pack: &[u8]) -> Result<WebFiles, JsError> {
        Files::open(pack).map(WebFiles).map_err(|e| JsError::new(&e.to_string()))
    }
}

/// Browser handle to a running player. Times are seconds from the start; points are canvas
/// pixels.
#[wasm_bindgen]
pub struct WebPlayer(Player);

#[wasm_bindgen]
impl WebPlayer {
    /// `seed`: the clock's seconds, for the clouds (`time(NULL)`).
    pub async fn create(canvas: HtmlCanvasElement, files: WebFiles, seed: u32) -> Result<WebPlayer, JsError> {
        let size = (canvas.width(), canvas.height());
        let desc = wgpu::InstanceDescriptor::new_without_display_handle();
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let mut quiet = |_: &str| {};
        let player = Player::new(&instance, target, size, &files.0, Some(seed), &mut quiet).await;
        drop(files);
        player.map(WebPlayer).map_err(|e| JsError::new(&e.to_string()))
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
    /// A typed character; true if it asks to quit.
    pub fn key(&mut self, key: &str, now: f64) -> bool {
        key.chars().fold(false, |q, c| self.0.key(c, now) || q)
    }
    /// 0 up, 1 down, 2 left, 3 right.
    pub fn arrow(&mut self, which: usize, down: bool, now: f64) {
        self.0.arrow(which, down, now);
    }
    pub fn mouse_button(&mut self, button: u32, down: bool, x: f64, y: f64, now: f64) {
        self.0.mouse_button(button, down, x, y, now);
    }
    pub fn mouse_motion(&mut self, x: f64, y: f64, held: bool, now: f64) {
        self.0.mouse_motion(x, y, held, now);
    }
    pub fn playing(&self) -> bool {
        self.0.playing()
    }
    /// The sounds, by file name without extension; each loops from the start.
    pub fn sounds() -> Vec<String> {
        crate::SOUNDS.iter().map(|s| (*s).to_owned()).collect()
    }
}
