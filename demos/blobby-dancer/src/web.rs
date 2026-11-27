use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::Player;
use crate::data::Files;
use crate::demo::BUTTONS;

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

fn failed(e: crate::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// The demo files as they arrive.
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
        Files::open(pack).map(Self).map_err(failed)
    }
    pub fn insert(&mut self, name: &str, data: Vec<u8>) {
        self.0.insert(name, data);
    }
    /// Fails with the name of what is missing.
    pub fn check(&self) -> Result<(), JsError> {
        self.0.check().map_err(failed)
    }
    pub fn sound(&self, name: &str) -> Option<Vec<u8>> {
        let wav = self.0.get(&format!("{name}.wav"));
        let any = wav.or_else(|_| self.0.get(&format!("{name}.mp3")));
        any.ok().map(<[u8]>::to_vec)
    }
}

/// Keys and labels, in turn.
#[wasm_bindgen]
pub fn buttons() -> Vec<String> {
    let all = BUTTONS.iter().flat_map(|b| [b.0, b.1]);
    all.map(String::from).collect()
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
        player.map(WebPlayer).map_err(failed)
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
    pub fn key(&mut self, key: &str, down: bool) -> bool {
        self.0.demo.key(key, down)
    }
    /// Position as fractions of the canvas from its top left; buttons 1 left, 2 right,
    /// 4 middle.
    pub fn pointer(&mut self, x: f64, y: f64, buttons: u8) {
        self.0.demo.pointer(x, y, buttons);
    }
    pub fn wheel(&mut self, amount: f64) {
        self.0.demo.wheel(amount);
    }
    /// Sounds to start: a name, with "*" if it repeats; "-" before it stops it.
    pub fn cues(&mut self) -> Vec<String> {
        self.0.demo.cues()
    }
    /// What tests look at.
    pub fn report(&self) -> String {
        self.0.demo.report()
    }
    /// Shows one moment, for checking frames.
    pub fn freeze(&mut self, seconds: f64, keys: &str) {
        self.0.freeze(seconds, keys);
    }
}
