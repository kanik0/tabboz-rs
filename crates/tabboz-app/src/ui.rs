// UI <-> game protocol and the Interact bridge.
//
// The game logic runs on its own thread (like the original message loop):
// it owns the GameState and sends UI requests over a channel, blocking on
// answers for modal interactions (message boxes, yes/no questions, event
// dialogs) exactly like the original MessageBox/DialogBox calls did.

use std::sync::mpsc::Sender;

use tabboz_core::Interact;
use winit::event_loop::EventLoopProxy;

/// What the user answered to a modal request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAnswer {
    Ok,
    Yes,
    No,
}

/// Game -> UI requests.
/// The Ask variant carries a whole dialog template; boxing keeps the enum small.
#[allow(clippy::large_enum_variant)]
pub enum UiRequest {
    /// Opens a game dialog (non-modal for the engine, the game thread keeps
    /// the handler for its token).
    ShowFrame {
        token: u64,
        dlg_id: u32,
        texts: Vec<(i32, String)>,
        /// BorRadio ids to check at open.
        radios: Vec<i32>,
        /// BorCheck ids to check at open.
        checks: Vec<i32>,
    },
    CloseFrame { token: u64 },
    SetText { token: u64, id: i32, text: String },
    #[allow(dead_code)] // used by later milestones (radio groups in shop dialogs)
    SetRadio { token: u64, id: i32 },
    #[allow(dead_code)] // used by later milestones
    SetCheck { token: u64, id: i32, on: bool },
    /// Refresh of the main window fields.
    UpdateMain { snapshot: MainSnapshot },
    PlaySound { id: u16 },
    Quit,

    /// Blocking modal request with its dedicated answer channel.
    Ask {
        kind: AskKind,
        reply: Sender<DialogAnswer>,
    },
}

/// Blocking modal kinds (all map to original MessageBox/DialogBox calls).
pub enum AskKind {
    Message { title: String, text: String },
    Confirm { title: String, text: String },
    /// A dialog from the .rc with only an OK button (event dialogs).
    Dialog { id: u32, texts: Vec<(i32, String)> },
}

/// UI -> Game actions.
pub enum GameAction {
    /// A button/radio click on the frame with this token.
    Button { token: u64, id: i32 },
    /// A menu bar command (main window only).
    Command { id: u16 },
    /// The frame was closed (close button / OK / Cancel).
    FrameClosed { token: u64 },
}

/// Snapshot of everything the main window renders.
#[derive(Clone, Debug)]
pub struct MainSnapshot {
    pub nome: String,
    pub soldi: String,
    pub fama: String,
    pub reputazione: String,
    pub studio: String,
    pub nometipa: String,
    pub rapporti: String,
    pub scooter_nome: String,
    pub scooter_stato: String,
    pub calendario: String,
    pub tipa_label: String,
    pub rapporto_label: String,
    pub tabboz: TabbozSnapshot,
}

#[derive(Clone, Copy, Debug)]
pub struct TabbozSnapshot {
    pub sesso: char,
    pub testa: i32,
    pub giubbotto: i32,
    pub pantaloni: i32,
    pub scarpe: i32,
}

/// Implements Interact on the game thread by asking the UI thread.
/// Each blocking call ships its own reply channel inside the request.
pub struct UiBridge {
    pub tx: EventLoopProxy<UiRequest>,
}

impl Interact for UiBridge {
    fn message(&mut self, title: &str, text: &str) {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        let _ = self.tx.send_event(UiRequest::Ask {
            kind: AskKind::Message { title: title.into(), text: text.into() },
            reply: reply_tx,
        });
        let _ = reply_rx.recv(); // block until the user clicks OK
    }

    fn confirm(&mut self, title: &str, text: &str) -> bool {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        let _ = self.tx.send_event(UiRequest::Ask {
            kind: AskKind::Confirm { title: title.into(), text: text.into() },
            reply: reply_tx,
        });
        matches!(reply_rx.recv(), Ok(DialogAnswer::Yes))
    }

    fn dialog(&mut self, id: u32, set_text: &[(i32, String)]) {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        let _ = self.tx.send_event(UiRequest::Ask {
            kind: AskKind::Dialog { id, texts: set_text.to_vec() },
            reply: reply_tx,
        });
        let _ = reply_rx.recv();
    }

    fn play_sound(&mut self, id: u16) {
        let _ = self.tx.send_event(UiRequest::PlaySound { id });
    }
}
