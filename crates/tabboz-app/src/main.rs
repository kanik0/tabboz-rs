// Tabboz Simulator - macOS port (pure Rust).
//
// Architecture (faithful to the original): the game logic runs on its own
// thread (like the original message loop) and drives real OS windows —
// the main window, one window per game dialog, one per message box —
// through UiRequest events, blocking on modal answers. Like the original
// DialogBox(), every dialog is a separate window with its own size and
// window-relative mouse coordinates, which removes the whole class of
// offset/clipping bugs; and like the original SW_HIDE, the main window
// is hidden while a game dialog is open.

mod game;
mod save;
mod sound;
mod ui;

use std::collections::HashMap;
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

/// A dialog or message-box window, with its own frame and buffer.
struct WinCtx {
    /// Last mouse position in this window (logical), tracked from CursorMoved.
    last_mouse: (i32, i32),
    window: Arc<Window>,
    surface: softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>,
    frame: Frame,
    fonts: Fonts,
    scale: i32,
    /// Game dialog token (0 for message boxes / ask frames).
    token: u64,
    /// The main window is hidden while this dialog is open (SW_HIDE).
    hides_main: bool,
    /// Blocking request answered by this window, if any.
    ask: Option<Ask>,
}

struct Ask {
    reply: Sender<DialogAnswer>,
    yes_no: bool,
}

struct App {
    // Main window.
    main: Option<MainWin>,
    // Dialog windows, by winit WindowId.
    dialogs: HashMap<WindowId, WinCtx>,
    // The modal window on top of the stack (the only one accepting input
    // while the game thread is blocked on it).
    modal: Option<WindowId>,
    game_tx: Option<Sender<GameAction>>,
    sound_tx: Option<Sender<u16>>,
    proxy: Option<EventLoopProxy<UiRequest>>,
    /// Scratch buffer for rendering, reused across frames to avoid a
    /// per-frame allocation of the whole physical pixel buffer.
    render_buf: Vec<u32>,
}

struct MainWin {
    /// Last mouse position in this window (logical), tracked from CursorMoved.
    last_mouse: (i32, i32),
    window: Arc<Window>,
    surface: softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>,
    frame: Frame,
    fonts: Fonts,
    scale: i32,
}

impl App {
    fn new(proxy: EventLoopProxy<UiRequest>) -> Self {
        App {
            main: None,
            dialogs: HashMap::new(),
            modal: None,
            game_tx: None,
            sound_tx: None,
            proxy: Some(proxy),
            render_buf: Vec::new(),
        }
    }

    // ---- window helpers --------------------------------------------------

    /// Renders a frame into the softbuffer surface, reusing `buf` as the
    /// physical-pixel scratch buffer (resized as needed, never reallocated
    /// per frame after the first call).
    fn render_win(
        surface: &mut softbuffer::Surface<winit::event_loop::OwnedDisplayHandle, Arc<Window>>,
        frame: &mut Frame,
        fonts: &mut Fonts,
        scale: i32,
        buf: &mut Vec<u32>,
    ) {
        let (lw, lh) = frame.window_size();
        let (pw, ph) = (lw * scale, lh * scale);
        let needed = (pw * ph) as usize;
        buf.clear();
        buf.resize(needed, 0);
        {
            let mut s = Surface::new(buf, lw, lh, scale);
            frame.render(&mut s, fonts);
        }
        if let (Some(pw_nz), Some(ph_nz)) = (NonZeroU32::new(pw as u32), NonZeroU32::new(ph as u32)) {
            surface.resize(pw_nz, ph_nz).expect("resize softbuffer surface");
            let mut buffer = surface.buffer_mut().expect("softbuffer buffer");
            buffer.copy_from_slice(buf);
            buffer.present().expect("present");
        }
    }

    /// Creates a dialog window: its own OS window sized to the dialog,
    /// centered over the main window (or the screen if the main is hidden),
    /// like the original DialogBox().
    #[allow(clippy::too_many_arguments)]
    fn open_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        frame: Frame,
        token: u64,
        hides_main: bool,
        ask: Option<Ask>,
    ) {
        let scale = self
            .main
            .as_ref()
            .map(|m| m.window.scale_factor().round().max(1.0) as i32)
            .unwrap_or(2);
        let (lw, lh) = frame.window_size();
        let mut attrs = Window::default_attributes()
            .with_title("Tabboz Simulator")
            .with_inner_size(winit::dpi::LogicalSize::new(lw as f64, lh as f64))
            .with_resizable(false)
            .with_decorations(false); // the BWCC chrome is drawn by the engine
        // Center over the main window, like a modal dialog over its owner.
        if let Some(m) = &self.main {
            if let Ok(pos) = m.window.outer_position() {
                let size = m.window.inner_size();
                let logical = size.to_logical::<f64>(m.window.scale_factor());
                let (dx, dy) = (
                    ((logical.width - lw as f64) / 2.0).round() as i32,
                    ((logical.height - lh as f64) / 2.0).round() as i32,
                );
                attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                    pos.x + dx * scale,
                    pos.y + dy * scale,
                ));
            }
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create dialog window"));
        // DialogBox activates the new dialog over its owner: without this
        // the dialog can open BEHIND the still-visible main window.
        window.focus_window();
        let ctx: softbuffer::Context<winit::event_loop::OwnedDisplayHandle> =
            softbuffer::Context::new(event_loop.owned_display_handle()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&ctx, window.clone()).expect("softbuffer surface");
        let fonts = Fonts::new(scale);
        let id = window.id();
        let is_ask = ask.is_some();
        self.dialogs.insert(
            id,
            WinCtx { last_mouse: (0, 0), window, surface, frame, fonts, scale, token, hides_main, ask },
        );
        // Modal semantics: while a game dialog or an ask window is open,
        // only the top of the stack accepts input (DialogBox disables its
        // owner); and the main window is hidden for game dialogs (SW_HIDE).
        if is_ask {
            self.modal = Some(id);
        }
        self.refresh_main_visibility();
        self.redraw_dialog(id);
    }

    /// SW_HIDE / SW_SHOW of the original: the main window is hidden while
    /// a game dialog is open. When it comes back, it also regains focus,
    /// like an owner window after DialogBox returns.
    fn refresh_main_visibility(&mut self) {
        if let Some(m) = &self.main {
            let hide = self.dialogs.values().any(|c| c.hides_main);
            if hide {
                m.window.set_visible(false);
            } else {
                let was_hidden = !m.window.is_visible().unwrap_or(false);
                m.window.set_visible(true);
                if was_hidden {
                    m.window.focus_window();
                }
            }
        }
    }

    /// When the last dialog closes, activation goes back to the owner
    /// window (covers dialogs that never hid the main, e.g. About).
    fn focus_main_if_forgotten(&mut self) {
        if self.dialogs.is_empty() {
            if let Some(m) = &self.main {
                m.window.focus_window();
            }
        }
    }

    fn redraw_dialog(&mut self, id: WindowId) {
        let Some(ctx) = self.dialogs.get_mut(&id) else { return };
        // Take the scratch buffer out temporarily: ctx borrows self.dialogs
        // mutably, so we cannot borrow self.render_buf at the same time.
        let mut buf = std::mem::take(&mut self.render_buf);
        Self::render_win(&mut ctx.surface, &mut ctx.frame, &mut ctx.fonts, ctx.scale, &mut buf);
        self.render_buf = buf;
    }

    fn redraw_main(&mut self) {
        let Some(m) = &mut self.main else { return };
        let mut buf = std::mem::take(&mut self.render_buf);
        Self::render_win(&mut m.surface, &mut m.frame, &mut m.fonts, m.scale, &mut buf);
        self.render_buf = buf;
    }

    /// Closes a dialog window (by winit id): dropping the context destroys
    /// the OS window.
    fn close_dialog_id(&mut self, id: WindowId) {
        if self.modal == Some(id) {
            self.modal = None;
        }
        self.dialogs.remove(&id);
        self.refresh_main_visibility();
        self.focus_main_if_forgotten();
    }

    /// Closes the dialog window carrying this game token.
    fn close_dialog_token(&mut self, token: u64) {
        let id = self
            .dialogs
            .iter()
            .find(|(_, c)| c.token == token)
            .map(|(id, _)| *id);
        if let Some(id) = id {
            self.close_dialog_id(id);
        }
    }

    fn answer_and_close(&mut self, id: WindowId, answer: DialogAnswer) {
        if let Some(ctx) = self.dialogs.remove(&id) {
            if let Some(ask) = ctx.ask {
                let _ = ask.reply.send(answer);
            }
        }
        if self.modal == Some(id) {
            self.modal = None;
        }
        self.refresh_main_visibility();
        self.focus_main_if_forgotten();
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
                // Every game dialog hides the main window
                // (ShowWindow(WIN_PICCOLO)); without this macOS can leave
                // the dialog BEHIND the main, and a modal-blocked main
                // would make the app unusable (About, PersonalInfo,
                // Config, Logo included).
                self.open_window(event_loop, frame, token, true, None);
            }
            UiRequest::CloseFrame { token } => {
                self.close_dialog_token(token);
            }
            UiRequest::SetText { token, id, text } => {
                let win = self
                    .dialogs
                    .iter_mut()
                    .find(|(_, c)| c.token == token)
                    .map(|(_, c)| (c.frame.set_text(id, text), c.window.id()));
                if let Some((_, id)) = win {
                    self.redraw_dialog(id);
                }
            }
            UiRequest::SetRadio { token, id } => {
                let win = self
                    .dialogs
                    .iter_mut()
                    .find(|(_, c)| c.token == token)
                    .map(|(_, c)| (c.frame.check_radio(id), c.window.id()));
                if let Some((_, id)) = win {
                    self.redraw_dialog(id);
                }
            }
            UiRequest::SetCheck { token, id, on } => {
                let win = self
                    .dialogs
                    .iter_mut()
                    .find(|(_, c)| c.token == token)
                    .map(|(_, c)| (c.frame.check(id, on), c.window.id()));
                if let Some((_, id)) = win {
                    self.redraw_dialog(id);
                }
            }
            UiRequest::UpdateMain { snapshot } => {
                self.apply_snapshot(snapshot);
                self.redraw_main();
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
                let (frame, yes_no) = match &kind {
                    AskKind::Message { title, text } => {
                        (message_box(title, text, MsgBoxButtons::Ok), false)
                    }
                    AskKind::Confirm { title, text } => {
                        (message_box(title, text, MsgBoxButtons::YesNo), true)
                    }
                    AskKind::Dialog { id, texts } => {
                        let mut f = Frame::new(*id);
                        for (cid, text) in texts {
                            f.set_text(*cid, text.clone());
                        }
                        (f, false)
                    }
                };
                // A message box opened while the main window is visible must
                // hide it (macOS can stack the box BEHIND the still-visible
                // main, and the modal-blocked main would freeze the app —
                // File->Apri/Salva con nome). Boxes opened from a game
                // dialog leave visibility alone: the main is already
                // hidden and the dialog owns the screen.
                let hides_main = self
                    .main
                    .as_ref()
                    .map(|m| m.window.is_visible().unwrap_or(false))
                    .unwrap_or(false);
                self.open_window(event_loop, frame, 0, hides_main, Some(Ask { reply, yes_no }));
            }
        }
    }

    /// AggiornaPrincipale applied to the main frame controls.
    fn apply_snapshot(&mut self, s: MainSnapshot) {
        let m = &mut match &mut self.main {
            Some(m) => m,
            None => return,
        };
        m.frame.set_text(102, s.nome.clone());
        m.frame.set_text(105, s.soldi.clone());
        m.frame.set_text(151, s.fama.clone());
        m.frame.set_text(152, s.reputazione.clone());
        m.frame.set_text(153, s.studio.clone());
        m.frame.set_text(154, s.rapporti.clone());
        m.frame.set_text(155, s.nometipa.clone());
        m.frame.set_text(150, s.scooter_nome.clone());
        m.frame.set_text(156, s.scooter_stato.clone());
        m.frame.set_text(157, s.calendario.clone());
        m.frame.set_text(133, s.tipa_label.clone());
        m.frame.set_text(170, s.rapporto_label.clone());
        m.frame.set_tabboz_view(TabbozView {
            sesso: s.tabboz.sesso,
            testa: s.tabboz.testa,
            giubbotto: s.tabboz.giubbotto,
            pantaloni: s.tabboz.pantaloni,
            scarpe: s.tabboz.scarpe,
        });
    }

    fn send_game(&self, action: GameAction) -> bool {
        match &self.game_tx {
            Some(tx) => tx.send(action).is_ok(),
            None => false,
        }
    }
}

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

impl ApplicationHandler<UiRequest> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Game + sound threads with their channels.
        let (game_tx, game_rx) = channel::<GameAction>();
        let (sound_tx, sound_rx) = channel::<u16>();
        let proxy = self.proxy.clone().expect("proxy");
        self.game_tx = Some(game_tx);
        self.sound_tx = Some(sound_tx);

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

        // Main window: exactly the size of the main dialog, like the
        // original modeless main window.
        let frame = Frame::new(1); // MAIN
        let (lw, lh) = frame.window_size();
        let attrs = Window::default_attributes()
            .with_title("Tabboz Simulator")
            .with_inner_size(winit::dpi::LogicalSize::new(lw as f64, lh as f64))
            .with_resizable(false)
            .with_decorations(false) // the BWCC chrome is drawn by the engine
            // Hidden at boot: the startup Logo shows first (the original
            // opens it before the main dialog); refresh_main_visibility()
            // reveals the main when the last dialog closes.
            .with_visible(false);
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let ctx: softbuffer::Context<winit::event_loop::OwnedDisplayHandle> =
            softbuffer::Context::new(event_loop.owned_display_handle()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&ctx, window.clone()).expect("softbuffer surface");
        let scale = window.scale_factor().round().max(1.0) as i32;
        self.main = Some(MainWin {
            last_mouse: (0, 0),
            window,
            surface,
            frame,
            fonts: Fonts::new(scale),
            scale,
        });
        self.redraw_main();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, req: UiRequest) {
        self.handle_request(event_loop, req);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // ---- main window ---------------------------------------------------
        if self.main.as_ref().map(|m| m.window.id()) == Some(id) {
            // Modality: while a dialog is open, the original DialogBox has
            // disabled its owner, so main-window input is ignored.
            if !self.dialogs.is_empty() {
                return;
            }
            let scale = self.main.as_ref().unwrap().scale as f64;
            let Some(m) = &mut self.main else { return };
            match event {
                WindowEvent::CloseRequested => {
                    let _ = self.send_game(GameAction::Command { id: 108 });
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let (lx, ly) = ((position.x / scale) as i32, (position.y / scale) as i32);
                    m.last_mouse = (lx, ly);
                    m.frame.mouse_move(lx, ly, &mut m.fonts);
                    self.redraw_main();
                }
                WindowEvent::CursorLeft { .. } => {
                    m.frame.mouse_leave();
                    self.redraw_main();
                }
                WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                    match state {
                        winit::event::ElementState::Pressed => {
                            if title_bar_drag_hit(&m.frame, m.last_mouse.0, m.last_mouse.1) {
                                let _ = m.window.drag_window();
                            } else {
                                m.frame.mouse_down(m.last_mouse.0, m.last_mouse.1, &mut m.fonts);
                            }
                        }
                        winit::event::ElementState::Released => {
                            if let Some(ev) = m.frame.mouse_up(m.last_mouse.0, m.last_mouse.1, &mut m.fonts) {
                                let pos = m.frame.take_last_click().unwrap_or((-1, -1));
                                match ev {
                                    UiEvent::Button(bid) => {
                                        let _ = self.send_game(GameAction::Button {
                                            token: 0,
                                            id: bid,
                                            x: pos.0,
                                            y: pos.1,
                                        });
                                    }
                                    UiEvent::Command(cid) => {
                                        let _ = self.send_game(GameAction::Command { id: cid });
                                    }
                                    UiEvent::Close => {
                                        // SC_CLOSE opens the Spegnimi dialog.
                                        let _ = self.send_game(GameAction::Command { id: 108 });
                                    }
                                    UiEvent::Minimize => {
                                        m.window.set_minimized(true);
                                    }
                                }
                            }
                        }
                    }
                    self.redraw_main();
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    let scale = m.window.scale_factor().round().max(1.0) as i32;
                    m.scale = scale;
                    m.fonts = Fonts::new(scale);
                    self.redraw_main();
                }
                WindowEvent::RedrawRequested => {
                    self.redraw_main();
                }
                _ => {}
            }
            return;
        }

        // ---- dialog windows --------------------------------------------------
        let Some(ctx) = self.dialogs.get_mut(&id) else { return };
        // Modality: while an ask window is modal, other dialogs ignore input
        // (the game thread is blocked waiting for the answer).
        if let Some(modal) = self.modal {
            if modal != id {
                return;
            }
        }
        let scale = ctx.scale as f64;
        let token = ctx.token;
        let is_ask = ctx.ask.is_some();
        match event {
            WindowEvent::CloseRequested => {
                // The dialog close button: EndDialog / cancel answer.
                if is_ask {
                    let yes_no = ctx.ask.as_ref().map(|a| a.yes_no).unwrap_or(false);
                    let answer = if yes_no { DialogAnswer::No } else { DialogAnswer::Ok };
                    self.answer_and_close(id, answer);
                } else {
                    let edits = ctx.frame.edit_texts();
                    self.close_dialog_id(id);
                    let _ = self.send_game(GameAction::FrameClosed { token, edits });
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (lx, ly) = ((position.x / scale) as i32, (position.y / scale) as i32);
                ctx.last_mouse = (lx, ly);
                ctx.frame.mouse_move(lx, ly, &mut ctx.fonts);
                self.redraw_dialog(id);
            }
            WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                let (lx, ly) = ctx.last_mouse;
                match state {
                    winit::event::ElementState::Pressed => {
                        if title_bar_drag_hit(&ctx.frame, lx, ly) {
                            let window = ctx.window.clone();
                            let _ = window.drag_window();
                        } else {
                            ctx.frame.mouse_down(lx, ly, &mut ctx.fonts);
                        }
                    }
                    winit::event::ElementState::Released => {
                        if let Some(ev) = ctx.frame.mouse_up(lx, ly, &mut ctx.fonts) {
                            let pos = ctx.frame.take_last_click().unwrap_or((-1, -1));
                            match ev {
                                UiEvent::Button(bid) => {
                                    if is_ask {
                                        let answer = match bid {
                                            6 => DialogAnswer::Yes, // IDYES
                                            7 => DialogAnswer::No,  // IDNO
                                            _ => DialogAnswer::Ok,
                                        };
                                        self.answer_and_close(id, answer);
                                    } else {
                                        let _ = self.send_game(GameAction::Button {
                                            token,
                                            id: bid,
                                            x: pos.0,
                                            y: pos.1,
                                        });
                                    }
                                }
                                UiEvent::Close => {
                                    if is_ask {
                                        let yes_no = ctx.ask.as_ref().map(|a| a.yes_no).unwrap_or(false);
                                        let answer = if yes_no { DialogAnswer::No } else { DialogAnswer::Ok };
                                        self.answer_and_close(id, answer);
                                    } else {
                                        let edits = ctx.frame.edit_texts();
                                        self.close_dialog_id(id);
                                        let _ = self.send_game(GameAction::FrameClosed { token, edits });
                                    }
                                }
                                UiEvent::Command(_) => {}
                                UiEvent::Minimize => {}
                            }
                        }
                    }
                }
                self.redraw_dialog(id);
            }
            // Keyboard input into the focused edit (Personal Information
            // fields); Enter/Escape are the dialog IDOK/IDCANCEL.
            WindowEvent::KeyboardInput { event, .. } => {
                let named = |n| event.logical_key == winit::keyboard::Key::Named(n);
                if ctx.frame.has_edit_focus()
                    && (named(winit::keyboard::NamedKey::Enter)
                        || named(winit::keyboard::NamedKey::Escape))
                {
                    let edits = ctx.frame.edit_texts();
                    self.close_dialog_id(id);
                    let _ = self.send_game(GameAction::FrameClosed { token, edits });
                } else if named(winit::keyboard::NamedKey::Backspace) {
                    ctx.frame.backspace();
                    self.redraw_dialog(id);
                } else if let Some(text) = event.text.as_ref() {
                    for c in text.chars() {
                        ctx.frame.type_char(c);
                    }
                    self.redraw_dialog(id);
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let scale = ctx.window.scale_factor().round().max(1.0) as i32;
                ctx.scale = scale;
                ctx.fonts = Fonts::new(scale);
                self.redraw_dialog(id);
            }
            WindowEvent::RedrawRequested => {
                self.redraw_dialog(id);
            }
            _ => {}
        }
        let _ = event_loop;
    }
}

fn main() {
    let event_loop: EventLoop<UiRequest> = EventLoop::with_user_event()
        .build()
        .expect("event loop");
    let proxy = event_loop.create_proxy();
    event_loop.run_app(&mut App::new(proxy)).expect("run");
}
