use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::Player;
use crate::data::Files;
use crate::demo;

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
    /// Fails with the name of what is missing.
    pub fn check(&self) -> Result<(), JsError> {
        let names = self.0.needed().map_err(|e| JsError::new(&e.to_string()))?;
        for n in names {
            self.0.get(&n).map_err(|e| JsError::new(&e.to_string()))?;
        }
        Ok(())
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
        self.0.show.key(key)
    }
    /// Sounds to start: a file name without extension, with "*" if it repeats.
    pub fn cues(&mut self) -> Vec<String> {
        self.0
            .show
            .take_cues()
            .into_iter()
            .map(|cue| format!("{}{}", cue.name, if cue.repeat { "*" } else { "" }))
            .collect()
    }
    /// Shows one moment, for checking frames.
    pub fn freeze(&mut self, seconds: f64, yaw: f64, pitch: f64, key: &str) {
        self.0.show.freeze(seconds, yaw, pitch, key);
    }
    /// How loud the sound is.
    pub fn level(&self) -> f32 {
        self.0.show.level()
    }
    pub fn paused(&self) -> bool {
        self.0.show.paused
    }
    /// "Movie", "Expression" or "Free".
    pub fn mode(&self) -> String {
        format!("{:?}", self.0.show.mode())
    }
    pub fn scene(&self) -> usize {
        self.0.show.scene()
    }
    pub fn expression(&self) -> usize {
        self.0.show.expression()
    }
    pub fn expressions() -> Vec<String> {
        demo::EXPRESSIONS
            .iter()
            .map(|e| e.name.to_owned())
            .collect()
    }
    pub fn express(&mut self, i: usize) {
        self.0.show.express(i);
    }
    /// The labels of the expression's sliders, and where they are.
    pub fn labels(&self) -> Vec<String> {
        let e = &demo::EXPRESSIONS[self.0.show.expression()];
        e.labels.iter().map(|l| (*l).to_owned()).collect()
    }
    pub fn sliders(&self) -> Vec<f32> {
        self.0.show.sliders().to_vec()
    }
    pub fn slide(&mut self, i: usize, value: f32) {
        self.0.show.slide(i, value);
    }
    pub fn pan(&mut self, amount: f32) {
        self.0.show.pan(amount);
    }
    /// Positions are fractions of the canvas, from its top left.
    pub fn press(&mut self, x: f64, y: f64) {
        self.0.show.press(x, y);
    }
    pub fn motion(&mut self, x: f64, y: f64) {
        self.0.show.motion(x, y);
    }
    pub fn release(&mut self) {
        self.0.show.release();
    }
    pub fn zoom(&mut self, amount: f32) {
        self.0.show.zoom(amount);
    }
}
