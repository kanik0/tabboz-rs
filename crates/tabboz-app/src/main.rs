// Tabboz Simulator - macOS port (pure Rust).
//
// Architecture: the winit event loop owns the window and the frame stack;
// the game logic runs on its own thread (like the original message loop)
// and drives the UI through UiRequest events, blocking on modal answers.
// Sounds play on a third thread (rodio), the state persists as JSON.

mod game;
mod save;
mod sound;
mod ui;

use std::num::NonZeroU32;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

use tabboz_win::font::Fonts;
use tabboz_win::pixel::Surface;
use tabboz_win::render::{message_box, Frame, MsgBoxButtons, TabbozView, UiEvent};
use ui::{AskKind, DialogAnswer, GameAction, MainSnapshot, UiRequest};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::ui::UiBridge;

/// An open frame: a game dialog, an event dialog or a message box.
struct OpenFrame {
    frame: Frame,
    /// Game dialog token (0 for ask/message frames).
    token: u64,
    /// Blocking request being answered by this frame, if any.
    ask: Option<Ask>,
}

struct Ask {
    reply: Sender<DialogAnswer>,
    yes_no: bool,
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>>,
    fonts: Fonts,
    /// Integer render scale (round of the window scale factor: 1 or 2).
    scale: i32,
    main: Frame,
    snapshot: Option<MainSnapshot>,
    frames: Vec<OpenFrame>,
    game_tx: Option<Sender<GameAction>>,
    proxy: Option<EventLoopProxy<UiRequest>>,
    sound_tx: Option<Sender<u16>>,
    /// Last physical mouse position (tracked from CursorMoved).
    mouse: (f64, f64),
}

impl App {
    fn new(proxy: EventLoopProxy<UiRequest>) -> Self {
        App {
            window: None,
            surface: None,
            fonts: Fonts::new(1),
            main: Frame::new(1), // MAIN
            snapshot: None,
            frames: Vec::new(),
            game_tx: None,
            proxy: Some(proxy),
            sound_tx: None,
            scale: 1,
            mouse: (0.0, 0.0),
        }
    }

    // ---- rendering -------------------------------------------------------

    /// Offset that centers a frame over the main window.
    fn center_offset(&self, frame: &Frame) -> (i32, i32) {
        let (mw, mh) = self_size(&self.main);
        let (fw, fh) = frame.window_size();
        (((mw - fw) / 2).max(0), ((mh - fh) / 2).max(0))
    }

    fn redraw(&mut self) {
        let Some(_window) = &self.window else { return };
        let Some(surface) = &mut self.surface else { return };

        // Physical-resolution rendering: the engine draws in logical
        // coordinates, the Surface upscales every primitive by `scale`,
        // so text is rasterized at 22px (crisp) on retina instead of
        // being nearest-doubled from 11px.
        let (lw, lh) = self_size(&self.main);
        let scale = self.scale;
        let (pw, ph) = (lw * scale, lh * scale);

        let mut physical = vec![0u32; (pw * ph) as usize];
        {
            let mut s = Surface::new(&mut physical, lw, lh, scale);
            self.main.render(&mut s, &mut self.fonts);
            for open in &mut self.frames {
                open.frame.render(&mut s, &mut self.fonts);
            }
        }

        if let (Some(pw_nz), Some(ph_nz)) = (NonZeroU32::new(pw as u32), NonZeroU32::new(ph as u32)) {
            surface.resize(pw_nz, ph_nz).expect("resize softbuffer surface");
            let mut buffer = surface.buffer_mut().expect("softbuffer buffer");
            buffer.copy_from_slice(&physical);
            buffer.present().expect("present");
        }
    }

    // ---- game requests ---------------------------------------------------

    fn handle_request(&mut self, event_loop: &ActiveEventLoop, req: UiRequest) {
        match req {
            UiRequest::ShowFrame { token, dlg_id, texts, radios, checks } => {
                let mut frame = Frame::new(dlg_id);
                for (id, text) in texts {
                    frame.set_text(id, text);
                }
                for id in radios {
                    frame.check_radio(id);
                }
                for id in checks {
                    frame.check(id, true);
                }
                let (ox, oy) = self.center_offset(&frame);
                frame.set_offset(ox, oy);
                self.frames.push(OpenFrame { frame, token, ask: None });
            }
            UiRequest::CloseFrame { token } => {
                if let Some(pos) = self.frames.iter().rposition(|f| f.token == token) {
                    self.frames.remove(pos);
                }
            }
            UiRequest::SetText { token, id, text } => {
                if let Some(open) = self.frames.iter_mut().find(|f| f.token == token) {
                    open.frame.set_text(id, text);
                }
            }
            UiRequest::SetRadio { token, id } => {
                if let Some(open) = self.frames.iter_mut().find(|f| f.token == token) {
                    open.frame.check_radio(id);
                }
            }
            UiRequest::SetCheck { token, id, on } => {
                if let Some(open) = self.frames.iter_mut().find(|f| f.token == token) {
                    open.frame.check(id, on);
                }
            }
            UiRequest::UpdateMain { snapshot } => {
                self.apply_snapshot(snapshot);
            }
            UiRequest::PlaySound { id } => {
                if let Some(tx) = &self.sound_tx {
                    let _ = tx.send(id);
                }
            }
            UiRequest::Quit => {
                event_loop.exit();
            }
            UiRequest::Ask { kind, reply } => {
                let mut frame = match &kind {
                    AskKind::Message { title, text } => {
                        message_box(title, text, MsgBoxButtons::Ok)
                    }
                    AskKind::Confirm { title, text } => {
                        message_box(title, text, MsgBoxButtons::YesNo)
                    }
                    AskKind::Dialog { id, texts } => {
                        let mut f = Frame::new(*id);
                        for (cid, text) in texts {
                            f.set_text(*cid, text.clone());
                        }
                        f
                    }
                };
                let yes_no = matches!(kind, AskKind::Confirm { .. });
                let (ox, oy) = self.center_offset(&frame);
                frame.set_offset(ox, oy);
                self.frames.push(OpenFrame {
                    frame,
                    token: 0,
                    ask: Some(Ask { reply, yes_no }),
                });
            }
        }
        self.redraw();
    }

    /// AggiornaPrincipale applied to the main frame controls.
    fn apply_snapshot(&mut self, s: MainSnapshot) {
        let m = &mut self.main;
        m.set_text(102, s.nome.clone());
        m.set_text(105, s.soldi.clone());
        m.set_text(151, s.fama.clone());
        m.set_text(152, s.reputazione.clone());
        m.set_text(153, s.studio.clone());
        m.set_text(154, s.rapporti.clone());
        m.set_text(155, s.nometipa.clone());
        m.set_text(150, s.scooter_nome.clone());
        m.set_text(156, s.scooter_stato.clone());
        m.set_text(157, s.calendario.clone());
        m.set_text(133, s.tipa_label.clone());
        m.set_text(170, s.rapporto_label.clone());
        m.set_tabboz_view(TabbozView {
            sesso: s.tabboz.sesso,
            testa: s.tabboz.testa,
            giubbotto: s.tabboz.giubbotto,
            pantaloni: s.tabboz.pantaloni,
            scarpe: s.tabboz.scarpe,
        });
        self.snapshot = Some(s);
    }

    // ---- mouse routing ---------------------------------------------------

    /// True when (lx, ly) is on the title bar but not on a caption button:
    /// dragging the frameless window from there, like a native title bar.
    fn title_bar_drag_hit(frame: &Frame, lx: i32, ly: i32) -> bool {
        let (fw, _) = frame.window_size();
        let in_bar = (tabboz_win::theme::BORDER..tabboz_win::theme::BORDER + tabboz_win::theme::TITLE_H)
            .contains(&ly)
            && (tabboz_win::theme::BORDER..fw - tabboz_win::theme::BORDER).contains(&lx);
        if !in_bar {
            return false;
        }
        // Caption buttons sit on the right edge; clicks there are clicks.
        let buttons_w = if frame.dialog_style() & 0x0002_0000 != 0 {
            13 * 2 + 3
        } else {
            13
        };
        lx < fw - tabboz_win::theme::BORDER - buttons_w
    }

    fn handle_mouse(&mut self, event_loop: &ActiveEventLoop, action: MouseAction) {
        // Mouse coordinates are PHYSICAL pixels; the engine is logical.
        let scale = self.scale as f64;
        let (lx, ly) = ((self.mouse.0 / scale) as i32, (self.mouse.1 / scale) as i32);

        // Modals on top intercept everything.
        let Some(top) = self.frames.last_mut() else {
            // No modal: route to the main window.
            match action {
                MouseAction::Move => {
                    self.main.mouse_move(lx, ly, &mut self.fonts);
                }
                MouseAction::Down => {
                    // Drag the frameless window from its title bar.
                    if Self::title_bar_drag_hit(&self.main, lx, ly) {
                        if let Some(w) = &self.window {
                            let _ = w.drag_window();
                        }
                        return;
                    }
                    self.main.mouse_down(lx, ly, &mut self.fonts);
                }
                MouseAction::Up => {
                    if let Some(ev) = self.main.mouse_up(lx, ly, &mut self.fonts) {
                        self.on_main_event(event_loop, ev);
                    }
                }
            }
            self.redraw();
            return;
        };

        // Coordinates local to the modal frame (it is centered on the main).
        let (mw, mh) = self_size(&self.main);
        let (fw, fh) = top.frame.window_size();
        let (fx, fy) = (((mw - fw) / 2).max(0), ((mh - fh) / 2).max(0));
        let (lx, ly) = (lx - fx, ly - fy);

        let is_ask = top.ask.is_some();
        let token = top.token;
        match action {
            MouseAction::Move => {
                top.frame.mouse_move(lx, ly, &mut self.fonts);
            }
            MouseAction::Down => {
                // Drag modal frames from their title bar too.
                if Self::title_bar_drag_hit(&top.frame, lx, ly) {
                    let window = self.window.clone();
                    if let Some(w) = &window {
                        let _ = w.drag_window();
                    }
                } else {
                    top.frame.mouse_down(lx, ly, &mut self.fonts);
                }
            }
            MouseAction::Up => {
                if let Some(ev) = top.frame.mouse_up(lx, ly, &mut self.fonts) {
                    // Click position relative to the clicked control
                    // (BMPTipa palpatina coordinates).
                    let pos = top.frame.take_last_click().unwrap_or((-1, -1));
                    match (ev, is_ask) {
                        (UiEvent::Button(id), true) => {
                            let ask = self.frames.pop().and_then(|f| f.ask);
                            if let Some(ask) = ask {
                                let answer = match id {
                                    6 => DialogAnswer::Yes, // IDYES
                                    7 => DialogAnswer::No,  // IDNO
                                    _ => DialogAnswer::Ok,
                                };
                                let _ = ask.reply.send(answer);
                            }
                        }
                        (UiEvent::Button(id), false) => {
                            let _ = self.send_game(GameAction::Button {
                                token,
                                id,
                                x: pos.0,
                                y: pos.1,
                            });
                        }
                        (UiEvent::Command(_), _) => {}
                        (UiEvent::Close, _) => {
                            // Modal close button: answer the ask or close the
                            // game dialog, like EndDialog.
                            let ask = self.frames.pop().and_then(|f| f.ask);
                            if let Some(ask) = ask {
                                let answer =
                                    if ask.yes_no { DialogAnswer::No } else { DialogAnswer::Ok };
                                let _ = ask.reply.send(answer);
                            } else if token != 0 {
                                let _ = self.send_game(GameAction::FrameClosed { token });
                            }
                        }
                    }
                }
            }
        }
        self.redraw();
    }

    fn on_main_event(&mut self, event_loop: &ActiveEventLoop, ev: UiEvent) {
        match ev {
            UiEvent::Button(id) => {
                let pos = self.main.take_last_click().unwrap_or((-1, -1));
                let _ = self.send_game(GameAction::Button { token: 0, id, x: pos.0, y: pos.1 });
            }
            UiEvent::Command(id) => {
                let _ = self.send_game(GameAction::Command { id });
            }
            UiEvent::Close => {
                // SC_CLOSE opens the Spegnimi dialog, like the original.
                let _ = self.send_game(GameAction::Command { id: 108 });
            }
        }
        let _ = event_loop;
    }

    fn send_game(&self, action: GameAction) -> bool {
        match &self.game_tx {
            Some(tx) => tx.send(action).is_ok(),
            None => false,
        }
    }
}

fn self_size(f: &Frame) -> (i32, i32) {
    f.window_size()
}

enum MouseAction {
    Move,
    Down,
    Up,
}

impl ApplicationHandler<UiRequest> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Game + sound threads with their channels.
        let (game_tx, game_rx) = channel::<GameAction>();
        let (sound_tx, sound_rx) = channel::<u16>();
        let proxy: EventLoopProxy<UiRequest> = self.proxy.clone().expect("proxy");

        // Boot the state: load the autosave, or start fresh (CaricaTutto).
        let (state, rng) = match save::load() {
            Some(state) => (state, tabboz_core::Rng::randomize()),
            None => {
                let mut rng = tabboz_core::Rng::randomize();
                let state = tabboz_core::GameState::reset(true, &mut rng);
                (state, rng)
            }
        };

        let bridge = UiBridge { tx: proxy };
        std::thread::spawn(move || {
            game::Game::new(state, rng, bridge, game_rx).run();
        });
        std::thread::spawn(move || sound::run(sound_rx));

        self.game_tx = Some(game_tx);
        self.sound_tx = Some(sound_tx);

        let (lw, lh) = self_size(&self.main);
        let attrs = Window::default_attributes()
            .with_title("Tabboz Simulator")
            .with_inner_size(winit::dpi::LogicalSize::new(lw as f64, lh as f64))
            .with_resizable(false)
            .with_decorations(false); // the BWCC chrome is drawn by the engine
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let ctx: softbuffer::Context<winit::event_loop::OwnedDisplayHandle> =
            softbuffer::Context::new(event_loop.owned_display_handle()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&ctx, window.clone()).expect("softbuffer surface");
        // Physical-resolution rendering needs the display scale.
        self.scale = window.scale_factor().round().max(1.0) as i32;
        self.fonts = Fonts::new(self.scale);
        self.window = Some(window);
        self.surface = Some(surface);
        self.redraw();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, req: UiRequest) {
        self.handle_request(event_loop, req);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                let _ = self.send_game(GameAction::Command { id: 108 });
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x, position.y);
                self.handle_mouse(event_loop, MouseAction::Move);
            }
            WindowEvent::CursorLeft { .. } => {
                self.main.mouse_leave();
                if let Some(top) = self.frames.last_mut() {
                    let (mw, mh) = self_size(&self.main);
                    let (fw, fh) = top.frame.window_size();
                    let _ = (mw, mh, fw, fh);
                }
                self.redraw();
            }
            WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                // self.mouse stays in PHYSICAL pixels; handle_mouse converts.
                match state {
                    winit::event::ElementState::Pressed => {
                        self.handle_mouse(event_loop, MouseAction::Down);
                    }
                    winit::event::ElementState::Released => {
                        self.handle_mouse(event_loop, MouseAction::Up);
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // The window moved between displays: re-rasterize the fonts.
                let scale = self
                    .window
                    .as_ref()
                    .map(|w| w.scale_factor().round().max(1.0) as i32)
                    .unwrap_or(1);
                if scale != self.scale {
                    self.scale = scale;
                    self.fonts = Fonts::new(scale);
                }
                self.redraw();
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop: EventLoop<UiRequest> = EventLoop::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    event_loop.run_app(&mut App::new(proxy)).expect("run");
}
