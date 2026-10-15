// Tabboz Simulator - macOS port (pure Rust).
//
// Milestone 2: opens the real main window (dialog 1) rendered by the
// tabboz-win engine: Win95 chrome in BWCC colors, working menu bar,
// pressable buttons and the composed tabboz image.
// Game logic wiring arrives with the following milestones.

use std::num::NonZeroU32;
use std::sync::Arc;

use tabboz_win::font::Fonts;
use tabboz_win::render::{Frame, UiEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>>,
    frame: Frame,
    fonts: Fonts,
    /// Last physical mouse position (tracked from MouseMoved).
    mouse: (f64, f64),
}

impl App {
    fn new() -> Self {
        App {
            window: None,
            surface: None,
            frame: Frame::new(1), // MAIN
            fonts: Fonts::new(),
            mouse: (0.0, 0.0),
        }
    }

    /// Renders the frame into a logical-size buffer and presents it
    /// integer-scaled to the physical window surface.
    fn redraw(&mut self) {
        let Some(window) = &self.window else { return };
        let Some(surface) = &mut self.surface else { return };

        let (lw, lh) = self.frame.window_size();
        let scale = window.scale_factor().round().max(1.0) as i32;
        let (pw, ph) = (lw * scale, lh * scale);

        let mut logical = vec![0u32; (lw * lh) as usize];
        {
            let mut s = tabboz_win::pixel::Surface::new(&mut logical, lw, lh);
            self.frame.render(&mut s, &mut self.fonts);
        }

        if let (Some(pw_nz), Some(ph_nz)) = (NonZeroU32::new(pw as u32), NonZeroU32::new(ph as u32)) {
            surface.resize(pw_nz, ph_nz).expect("resize softbuffer surface");
            let mut buffer = surface.buffer_mut().expect("softbuffer buffer");
            for y in 0..ph {
                let row = (y / scale * lw) as usize;
                for x in 0..pw {
                    buffer[(y * pw + x) as usize] = logical[row + (x / scale) as usize];
                }
            }
            buffer.present().expect("present");
        }
    }

    fn handle_event(&mut self, event_loop: &ActiveEventLoop, ev: Option<UiEvent>) {
        match ev {
            Some(UiEvent::Close) => event_loop.exit(),
            Some(UiEvent::Command(108)) => event_loop.exit(), // Chiudi
            Some(UiEvent::Command(id)) => println!("menu command {id}"),
            Some(UiEvent::Button(id)) => println!("button {id}"),
            None => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let (lw, lh) = self.frame.window_size();
        let attrs = winit::window::Window::default_attributes()
            .with_title("Tabboz Simulator")
            .with_inner_size(winit::dpi::LogicalSize::new(lw as f64, lh as f64))
            .with_resizable(false);
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let ctx: softbuffer::Context<winit::event_loop::OwnedDisplayHandle> =
            softbuffer::Context::new(event_loop.owned_display_handle()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&ctx, window.clone()).expect("softbuffer surface");
        self.window = Some(window);
        self.surface = Some(surface);
        self.redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let scale = self
            .window
            .as_ref()
            .map(|w| w.scale_factor().round().max(1.0))
            .unwrap_or(1.0);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x, position.y);
                let (x, y) = (position.x / scale, position.y / scale);
                self.frame.mouse_move(x as i32, y as i32, &mut self.fonts);
                self.redraw();
            }
            WindowEvent::CursorLeft { device_id: _ } => {
                self.frame.mouse_leave();
                self.redraw();
            }
            WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                let (x, y) = (self.mouse.0 / scale, self.mouse.1 / scale);
                match state {
                    winit::event::ElementState::Pressed => {
                        self.frame.mouse_down(x as i32, y as i32, &mut self.fonts);
                        self.redraw();
                    }
                    winit::event::ElementState::Released => {
                        let ev = self.frame.mouse_up(x as i32, y as i32, &mut self.fonts);
                        self.handle_event(event_loop, ev);
                        self.redraw();
                    }
                }
            }
            WindowEvent::Resized(_) | WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.run_app(&mut App::new()).expect("run");
}
