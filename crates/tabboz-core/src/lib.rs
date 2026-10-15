// Tabboz Simulator - game logic core.
//
// Pure-Rust port of the original C game logic (zarrosim.c, tempo.c,
// eventi.c): no UI, no I/O, fully testable headless.
//
// The UI-facing entry point is events::evento(), the "game turn" the
// original calls after almost every player action.

pub mod calendar;
pub mod events;
pub mod money;
pub mod rng;
pub mod state;

/// ATTESAMAX of zarrosim.h.
pub const ATTESA_MAX: i32 = 5;

/// Version of the original game this port tracks (zarrosim.h, VERSION).
pub const VERSION: &str = "0.92q";

/// Sccsid string of the original game, kept for the About dialog.
pub const COPYRIGHT: &str =
    "Copyright (c) 1997-2001 Andrea Bonomi, Emanuele Caccialanza, Daniele Gazzarari.";

pub use events::Interact;
pub use rng::Rng;
pub use state::GameState;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_original() {
        assert_eq!(VERSION, "0.92q");
    }
}
