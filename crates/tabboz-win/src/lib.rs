// Tabboz Simulator - Win32/BWCC dialog engine.
//
// Milestone 1: this crate exposes the code-generated dialog templates
// (from resources/dialogs/dialogs.rc) and the embedded game assets.
// The actual widget engine, BWCC theme and message pump land in milestone 2.

/// Dialog templates generated at build time from dialogs.rc.
pub mod dialogs {
    include!(concat!(env!("OUT_DIR"), "/dialogs.rs"));
}

/// Game assets embedded at build time (bitmaps, icons, sounds, strings, menus).
pub mod assets {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}
