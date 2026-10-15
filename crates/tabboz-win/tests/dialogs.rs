// Conformity tests for the dialogs.rc code generation (milestone 1).
//
// The expected values come from a manual audit of resources/dialogs/dialogs.rc.

use tabboz_win::dialogs::{self, ControlKind};

#[test]
fn all_81_dialogs_are_generated() {
    assert_eq!(dialogs::DIALOGS.len(), 81);
}

#[test]
fn all_1412_controls_are_generated() {
    assert_eq!(dialogs::control_count(), 1412);
}

#[test]
fn dialog_ids_are_unique_and_sorted() {
    let mut ids: Vec<u32> = dialogs::DIALOGS.iter().map(|d| d.id).collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "duplicate dialog ids");
}

#[test]
fn main_dialog_matches_the_original() {
    let main = dialogs::find(1).expect("dialog 1 (MAIN) missing");
    assert_eq!(main.caption, Some("Tabboz Simulator"));
    assert_eq!(main.menu, Some("zarrosim"));
    assert_eq!(main.class, Some("bordlg"));
    assert_eq!(main.font_name, Some("MS Sans Serif"));
    assert_eq!(main.font_size, Some(10));
    // 1 DIALOG 48, 22, 204, 185
    assert_eq!((main.x, main.y, main.w, main.h), (48, 22, 204, 185));
    // STYLE DS_SETFONT | WS_MINIMIZEBOX | WS_POPUP | WS_VISIBLE | WS_CAPTION | WS_SYSMENU
    assert_eq!(
        main.style,
        0x40 | 0x0002_0000 | 0x8000_0000 | 0x1000_0000 | 0x00C0_0000 | 0x0008_0000
    );
}

#[test]
fn main_dialog_default_button_is_scooter() {
    let main = dialogs::find(1).expect("dialog 1 (MAIN) missing");
    // CONTROL "Scooter", 130, BUTTON, BS_DEFPUSHBUTTON | ..., 3, 4, 45, 9
    let scooter = main.controls.iter().find(|c| c.id == 130).expect("id 130");
    assert_eq!(scooter.kind, ControlKind::Button);
    assert_eq!(scooter.text, "Scooter");
    assert_eq!((scooter.x, scooter.y, scooter.w, scooter.h), (3, 4, 45, 9));
    assert_eq!(scooter.style & 0x1, 0x1, "BS_DEFPUSHBUTTON");
    assert_eq!(scooter.style & 0x0001_0000, 0x0001_0000, "WS_TABSTOP");
}

#[test]
fn custom_classes_survive_code_generation() {
    // The main dialog hosts both custom BWCC/Borland control classes.
    let main = dialogs::find(1).expect("dialog 1 (MAIN) missing");
    assert!(main.controls.iter().any(|c| c.kind == ControlKind::BorShade));
    assert!(main.controls.iter().any(|c| c.kind == ControlKind::BmpView));
    // Every other dialog in the file uses Borland classes too.
    assert!(dialogs::DIALOGS.iter().any(|d| {
        d.controls.iter().any(|c| c.kind == ControlKind::BorRadio)
    }));
}

#[test]
fn dialogs_known_by_the_game_all_exist() {
    // Dialog resource ids referenced by zarrosim.h that exist in dialogs.rc.
    // Not present in the .rc: 3 (WARNING, a MessageBox), 20 (PROMPT, compiled
    // out), 93 (ESCICONLATIPA) and 122 (VENDICELLULAR).
    for id in [
        1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, // base dialogs
        70, 71, 72, 73, // scooter
        80, 81, 82, 83, 84, 85, 86, // negozi
        88, 89, // tabacchi, palestra
        91, 92, 95, // tipa
        120, 121, 123, // telefonino
    ] {
        assert!(dialogs::find(id).is_some(), "dialog {id} referenced by the game is missing");
    }
}

#[test]
fn no_control_has_invalid_geometry() {
    for d in dialogs::DIALOGS {
        assert!(d.w > 0 && d.h > 0, "dialog {} has empty size", d.id);
        for c in d.controls {
            assert!(c.w >= 0 && c.h >= 0, "dialog {} control {} has negative size", d.id, c.id);
        }
    }
}
