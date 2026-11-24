use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::Player;
use crate::music::Music;

#[wasm_bindgen(start)]
fn init() {
    console_error_panic_hook::set_once();
}

/// Decoded music for the Web Audio API.
#[wasm_bindgen]
pub struct WebMusic(Music);

#[wasm_bindgen]
impl WebMusic {
    #[wasm_bindgen(constructor)]
    pub fn new(intro: &[u8], repeat: &[u8]) -> Result<WebMusic, JsError> {
        Music::new(intro, repeat)
            .map(WebMusic)
            .map_err(|e| JsError::new(&e))
    }
    pub fn rate(&self) -> u32 {
        self.0.intro.rate
    }
    pub fn channels(&self) -> usize {
        self.0.intro.channels
    }
    /// Seconds from the start of the intro to the start of the loop.
    pub fn repeat_start(&self) -> f64 {
        self.0.repeat_start() as f64 / f64::from(self.0.intro.rate)
    }
    /// One channel of the intro, or of the loop.
    pub fn channel(&self, repeat: bool, index: usize) -> Vec<f32> {
        let p = if repeat {
            &self.0.repeat
        } else {
            &self.0.intro
        };
        let samples = p.samples.iter().skip(index).step_by(p.channels);
        samples.copied().collect()
    }
}

/// Browser handle to a running player.
#[wasm_bindgen]
pub struct WebPlayer(Player);

#[wasm_bindgen]
impl WebPlayer {
    /// `archive` is the user's `Sushi.ati` or its repacked form.
    pub async fn create(canvas: HtmlCanvasElement, archive: Vec<u8>) -> Result<WebPlayer, JsError> {
        let size = (canvas.width(), canvas.height());
        let desc = wgpu::InstanceDescriptor::new_without_display_handle();
        let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        let target = wgpu::SurfaceTarget::Canvas(canvas);
        let player = Player::new(&instance, target, size, &archive).await;
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
    /// `now` in seconds.
    pub fn frame(&mut self, now: f64) {
        self.0.frame(now);
    }
    pub fn titles(&self) -> Vec<String> {
        self.0.titles().into_iter().map(String::from).collect()
    }
    /// Draws one fixed moment of a chapter, counted from 0.
    pub fn freeze(&mut self, chapter: usize, time: f64) {
        self.0.show.go(chapter);
        self.0.show.set_time(time);
        if !self.0.show.paused() {
            self.0.show.key(" ");
        }
    }
    pub fn go(&mut self, chapter: usize) {
        self.0.show.go(chapter);
    }
    pub fn chapter(&self) -> usize {
        self.0.show.chapter()
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
    pub fn take_restart(&mut self) -> bool {
        self.0.show.take_restart()
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
