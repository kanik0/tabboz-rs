// Tabboz Simulator - game logic core.
//
// This crate is a pure-Rust port of the original C game logic
// (zarrosim.c, tempo.c, eventi.c, scooter.c, ...).
// It contains no UI code and no I/O: everything is testable headless.
//
// Milestone 1: only the version constant and the module skeleton live here.
// The state structs and game modules land in milestone 3.

/// Version of the original game this port tracks (zarrosim.h, VERSION).
pub const VERSION: &str = "0.92q";

/// Sccsid string of the original game, kept for the About dialog.
pub const COPYRIGHT: &str =
    "Copyright (c) 1997-2001 Andrea Bonomi, Emanuele Caccialanza, Daniele Gazzarri.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_original() {
        assert_eq!(VERSION, "0.92q");
    }
}
