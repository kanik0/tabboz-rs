// Tabboz Simulator - macOS port (pure Rust).
//
// Milestone 1: build verification stub. Lists the code-generated dialog
// templates and embedded assets; the window opens in milestone 2.

use tabboz_win::{assets, dialogs};

fn main() {
    println!("Tabboz Simulator {} (Rust port, M1)", tabboz_core::VERSION);
    println!(
        "Dialoghi generati da dialogs.rc: {} ({} controlli)",
        dialogs::DIALOGS.len(),
        dialogs::control_count()
    );
    println!(
        "Asset incorporati: {} ({:.1} MB)",
        assets::asset_count(),
        assets::asset_bytes() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "  bitmap: {}, chrome: {}, icone: {}, suoni: {}, stringhe: {}, menu: {}",
        assets::BITMAPS.len(),
        assets::CHROME_BITMAPS.len(),
        assets::ICONS.len(),
        assets::SOUNDS.len(),
        assets::STRINGS.len(),
        assets::MENUS.len(),
    );
}
