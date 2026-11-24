use std::sync::Arc;
use std::time::Instant;

use pipedream::Player;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Fullscreen, Window, WindowId},
};

type Error = Box<dyn std::error::Error>;

const DEFAULT: &str = "ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream/Sushi.ati";

pub fn run() -> Result<(), Error> {
    let path = std::env::args().nth(1).unwrap_or_else(|| DEFAULT.into());
    let data = std::fs::read(&path).map_err(|e| {
        format!("cannot read {path}: {e}\nusage: pipedream <path to your Sushi.ati>")
    })?;
    let mut app = App {
        data,
        window: None,
        player: None,
        start: Instant::now(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct App {
    data: Vec<u8>,
    window: Option<Arc<Window>>,
    player: Option<Player>,
    start: Instant,
    error: Option<Error>,
}

impl App {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), Error> {
        let attrs = Window::default_attributes()
            .with_title("Pipe Dream")
            .with_inner_size(winit::dpi::LogicalSize::new(1024, 768));
        let window = Arc::new(el.create_window(attrs)?);
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let player = Player::new(
            &instance,
            window.clone(),
            (size.width, size.height),
            &self.data,
        );
        self.player = Some(pollster::block_on(player)?);
        self.start = Instant::now();
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(e) = self.open(el)
        {
            self.error = Some(e);
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let (Some(window), Some(player)) = (&self.window, &mut self.player) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => player.resize(s.width, s.height),
            WindowEvent::RedrawRequested => {
                let t = self.start.elapsed().as_secs_f32() % player.duration();
                player.frame(t);
                window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => match logical_key {
                Key::Named(NamedKey::Escape) => el.exit(),
                Key::Character(c) if c == "f" => {
                    let full = window.fullscreen().is_none();
                    window.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
                }
                _ => {}
            },
            _ => {}
        }
    }
}
