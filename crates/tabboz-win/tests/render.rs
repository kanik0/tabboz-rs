// Offscreen rendering tests for the main window (milestone 2).
//
// The expected geometry comes from the .rc template (dialog 1) and the
// BWCC palette of the Novantotto web port.

use tabboz_win::font::Fonts;
use tabboz_win::pixel::Surface;
use tabboz_win::render::{Frame, TabbozView, UiEvent, message_box, MsgBoxButtons};
use tabboz_win::theme::*;

/// Renders the main dialog and returns (buffer, w, h, frame, fonts).
fn render_main() -> (Vec<u32>, i32, i32, Frame, Fonts) {
    let mut frame = Frame::new(1);
    let mut fonts = Fonts::new(1);
    let (w, h) = frame.window_size();
    let mut buf = vec![0u32; (w * h) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 1);
        frame.render(&mut s, &mut fonts);
    }
    (buf, w, h, frame, fonts)
}

#[test]
fn main_window_size_matches_the_original() {
    // 204x185 dialog units * 2 px + 4 px frame + 18 title + 20 menu.
    let frame = Frame::new(1);
    assert_eq!(frame.window_size(), (412, 412));
}

#[test]
fn window_frame_and_title_bar_colors() {
    let (buf, w, h, _, _) = render_main();
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // Outer frame.
    assert_eq!(get(0, h / 2), INNER_LIGHT);
    assert_eq!(get(w - 1, h / 2), DKSHADOW);
    // Title bar (Borland navy) with white caption text somewhere.
    assert_eq!(get(200, 3), NAVY, "title bar strip above the caption text");
    let white_ink = (24..200).any(|x| (5..16).any(|y| get(x, y) == WHITE));
    assert!(white_ink, "no white pixels in the caption text");
}

#[test]
fn menubar_renders_labels() {
    let (buf, w, _, _, _) = render_main();
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // "File" sits at the left of the menu bar (y in 20..40).
    let ink = (2..60).any(|x| (22..38).any(|y| get(x, y) == BLACK));
    assert!(ink, "no black pixels where 'File' should be");
}

#[test]
fn scooter_button_is_a_win95_button() {
    let (buf, w, _, _, _) = render_main();
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // CONTROL "Scooter", 130, BUTTON, BS_DEFPUSHBUTTON..., 3, 4, 45, 9
    // client origin (2, 40); button rect = (8, 48, 90, 18).
    assert_eq!(get(8, 48), WHITE, "button top border");
    assert_eq!(get(8, 48 + 17), DKSHADOW, "button bottom border");
    assert_eq!(get(8 + 45, 48 + 9), FACE, "button face");
    // BS_DEFPUSHBUTTON gets the black focus outline at (7, 47).
    assert_eq!(get(7, 47), BLACK, "default button outline");
}

#[test]
fn tabboz_image_is_composed_and_drawn() {
    let (buf, w, _, _, _) = render_main();
    // BMPView control 103 at DLU (5, 40, 68, 138) -> px (12, 120, 136, 276).
    let mut ink = 0;
    for y in 121..395 {
        for x in 13..147 {
            if buf[(y * w + x) as usize] != FACE {
                ink += 1;
            }
        }
    }
    assert!(ink > 5000, "tabboz image barely drawn: {ink} ink pixels");
}

#[test]
fn composed_tabboz_matches_original_layout() {
    let img = TabbozView::default().compose();
    assert_eq!((img.w, img.h), (143, 275), "IMG_SIZEX x IMG_SIZEY of tabimg.c");
    let opaque = img.data.iter().filter(|p| **p >> 24 != 0).count();
    assert!(opaque > 5000, "composed tabboz too sparse: {opaque}");
    // The sfondo (background, 1560) is the first layer and covers most
    // of the canvas; the head (1390) starts at x=42.
    let head_ink = (0..img.h).any(|y| img.data[(y * img.w + 42) as usize] >> 24 != 0);
    assert!(head_ink, "no testa ink at the expected offset");
}

#[test]
fn dropdown_menu_opens_and_sends_commands() {
    let mut frame = Frame::new(1);
    let mut fonts = Fonts::new(1);

    // Press "&File" on the menubar, drag to "Chiudi...", release:
    // the classic Win95 menu gesture.
    frame.mouse_down(12, 30, &mut fonts);
    // Dropdown: Apri (42..58), Salva (58..74), separator (74..79), Chiudi (79..95).
    frame.mouse_move(20, 88, &mut fonts);
    let ev = frame.mouse_up(20, 88, &mut fonts);
    assert_eq!(ev, Some(UiEvent::Command(108)), "Chiudi... must send command 108");
}

#[test]
fn close_button_emits_close() {
    let mut frame = Frame::new(1);
    let mut fonts = Fonts::new(1);
    let (w, _) = frame.window_size();
    // The close button sits at (w-15, 4) 13x13.
    let (bx, by) = (w - 15, 4);
    frame.mouse_down(bx + 6, by + 6, &mut fonts);
    let ev = frame.mouse_up(bx + 6, by + 6, &mut fonts);
    assert_eq!(ev, Some(UiEvent::Close));
}

#[test]
fn clicking_face_does_nothing() {
    let mut frame = Frame::new(1);
    let mut fonts = Fonts::new(1);
    // Empty face area inside the client region.
    let ev = frame.mouse_up(200, 200, &mut fonts);
    assert_eq!(ev, None);
}

#[test]
fn empty_ok_cancel_buttons_render_their_bwcc_bitmaps() {
    // The .rc templates carry empty-text BorBtn for OK (id 1) and Cancel
    // (id 2): BWCC draws their faces from the chrome bitmaps 1001/1002,
    // exactly like the web port does with CSS backgrounds.
    let mut frame = Frame::new(7); // Scooter: has an empty id-1 BorBtn
    let mut fonts = Fonts::new(1);
    let (w, h) = frame.window_size();
    let mut buf = vec![0u32; (w * h) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 1);
        frame.render(&mut s, &mut fonts);
    }
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // The OK button: CONTROL "", 1, "BorBtn", ..., 105, 72, 33, 21
    // client origin (2, 22): button at (212, 166) size 66x42. The bitmap
    // face (silver with dark border) must be there, not an empty face box.
    let mut ink = 0;
    for y in 168..204 {
        for x in 214..278 {
            if get(x, y) != tabboz_win::theme::FACE {
                ink += 1;
            }
        }
    }
    assert!(ink > 300, "OK bitmap button not drawn: {ink} ink pixels");
}

#[test]
fn about_dialog_renders_the_1257_bitmap_button() {
    // Dialog 2 (About): CONTROL "", 257, "BorBtn", 20, 44, 146, 71 is a
    // bitmap button showing game bitmap 1257 (control id + 1000) stretched
    // to the control rect, like the web port <img> sizing.
    let mut frame = Frame::new(2);
    let mut fonts = Fonts::new(1);
    let (w, h) = frame.window_size();
    let mut buf = vec![0u32; (w * h) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 1);
        frame.render(&mut s, &mut fonts);
    }
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // Client origin (2, 22): image at (42, 110), size 292x142.
    let mut ink = 0;
    for y in 112..250 {
        for x in 44..332 {
            if get(x, y) != tabboz_win::theme::FACE {
                ink += 1;
            }
        }
    }
    assert!(ink > 5000, "About 1257 bitmap not drawn: {ink} ink pixels");
}

#[test]
fn compagnia_dialog_renders_the_rle4_photo() {
    // Dialog 6 (Compagnia): CONTROL "", 259, "BorBtn", ..., 4, 3, 144, 92
    // draws the RLE4-compressed photo 1259 stretched to the control rect.
    let mut frame = Frame::new(6);
    let mut fonts = Fonts::new(1);
    let (w, h) = frame.window_size();
    let mut buf = vec![0u32; (w * h) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 1);
        frame.render(&mut s, &mut fonts);
    }
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // Client origin (2, 22): image at (10, 28), size 288x184.
    let mut ink = 0;
    for y in 32..208 {
        for x in 14..294 {
            if get(x, y) != tabboz_win::theme::FACE {
                ink += 1;
            }
        }
    }
    assert!(ink > 10000, "Compagnia photo not drawn: {ink} ink pixels");
}

#[test]
fn bold_face_falls_back_to_regular_for_missing_punctuation() {
    // The 98.css bold face lacks " ' * + , . / < = > ? \ _ | ; the regular
    // face must supply those glyphs instead of the .notdef box.
    use tabboz_win::font::Fonts;
    use tabboz_win::pixel::Surface;
    use tabboz_win::theme::WHITE;
    let mut fonts = Fonts::new(2);
    let (w, h) = (40, 20);
    let mut buf = vec![0u32; (w * h * 4) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 2);
        fonts.bold.draw_text(&mut s, "...", 2, 2, WHITE);
    }
    let ink = buf.iter().filter(|&&p| p != 0).count();
    // Three dots, 2 physical px each at scale 2: tiny — the old .notdef
    // box drew ~150.
    assert!(ink <= 30, "bold dots drew a notdef box: {ink} ink pixels");
    assert!(ink > 0, "bold dots drew nothing");
}

#[test]
fn spegnimi_radios_check_like_the_original() {
    // Dialog 16 (Spegnimi): BUTTON BS_AUTORADIOBUTTON controls must behave
    // like radios: clicking 102 checks it and clears 101 (they render as
    // plain push buttons without the BS_* style dispatch).
    let mut frame = Frame::new(16);
    let mut fonts = Fonts::new(1);
    frame.check_radio(101); // WM_INITDIALOG default
    // Radio 102: CONTROL "&Spegni il computer ed esci di casa", 26, 36.
    let (x, y) = (2 + 26 * 2 + 8, 22 + 36 * 2 + 4);
    frame.mouse_down(x, y, &mut fonts);
    let ev = frame.mouse_up(x, y, &mut fonts);
    assert_eq!(ev, Some(UiEvent::Button(102)));
    assert!(frame.is_checked(102));
    assert!(!frame.is_checked(101));
}

#[test]
fn configuration_checkbox_click_toggles() {
    // Dialog 14 (Configuration): BUTTON BS_AUTOCHECKBOX 107 (Enable Euro)
    // at (6, 103) toggles its check and emits the click.
    let mut frame = Frame::new(14);
    let mut fonts = Fonts::new(1);
    let (x, y) = (2 + 6 * 2 + 4, 22 + 103 * 2 + 4);
    frame.mouse_down(x, y, &mut fonts);
    let ev = frame.mouse_up(x, y, &mut fonts);
    assert_eq!(ev, Some(UiEvent::Button(107)));
    assert!(frame.is_checked(107));
}

#[test]
fn message_box_wraps_long_lines_to_the_window() {
    // A single very long paragraph must be word-wrapped into several
    // lines inside the clamped box width, like the original MessageBox.
    let frame = message_box(
        "Norme di utilizzo",
        "Il biglietto e' valido solo dopo la convalida. Il biglietto deve essere conservato per tutta la durata del viaggio. Il diritto a viaggiare cessa al termine della tratta corrispondente al valore del biglietto.",
        MsgBoxButtons::Ok,
    );
    let (w, _h) = frame.window_size();
    // Clamp: 160 DLU client + borders -> at most 332 px wide.
    assert!(w <= 332, "box too wide: {w}");
    // Wrapped into more than one text line.
    let line_count = frame
        .dlg()
        .controls
        .iter()
        .filter(|c| c.kind == tabboz_win::dialogs::ControlKind::BorStatic && c.id < -9)
        .count();
    assert!(line_count >= 2, "text not wrapped: {line_count} lines");
}

#[test]
fn bau_house_renders_previews_and_wrapped_descriptions() {
    // Dialog 80 (Bau House): BorBtn BS_OWNERDRAW with junk "Button" text
    // must draw the id+1000 preview bitmap; the SS_CENTER descriptions
    // must word-wrap inside their rect (they used to overflow the card).
    let mut frame = Frame::new(80);
    let mut fonts = Fonts::new(1);
    let (w, h) = frame.window_size();
    let mut buf = vec![0u32; (w * h) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 1);
        frame.render(&mut s, &mut fonts);
    }
    let get = |x: i32, y: i32| buf[(y * w + x) as usize];
    // Card 502: (96, 3) 88x62 DLU -> px (194, 28) 176x124. The bitmap
    // (vestito) fills the card; a plain "Button" face would leave it flat.
    let mut ink = 0;
    for y in 40..140 {
        for x in 200..360 {
            if get(x, y) != tabboz_win::theme::FACE {
                ink += 1;
            }
        }
    }
    assert!(ink > 3000, "card 502 preview not drawn: {ink} ink pixels");
    // First description: (12, 78) 69x56 DLU -> rect ends at px x=164.
    // The band between that rect and the shade line at x=188 must stay
    // clean: overflowing description text would cross it.
    for y in 182..286 {
        for x in 166..186 {
            assert_eq!(get(x, y), tabboz_win::theme::FACE, "description leaks at ({x},{y})");
        }
    }
}

#[test]
fn message_box_text_stays_inside_the_body() {
    // The Norme di utilizzo message: every synthesized line must fit the
    // BODY control (x=14, w-26 DLU) so the template height matches what
    // render_text draws - the old wrap limit (window width) made lines
    // the render-time wrap had to re-break, clipping the last one.
    let text = "Il biglietto e' valido solo dopo la convalida. Il biglietto deve essere conservato per tutta la durata del viaggio. Il diritto a viaggiare cessa al termine della tratta corrispondente al valore del biglietto. Il passeggero che al controllo non fosse in grado di presentare il biglietto o lo presentasse irriconoscibile, o comunque non valido, verra' abbattuto. La notifica del decesso verra' inviata ai parenti solo previo pagamento delle spese postali.";
    let frame = message_box("Norme di utilizzo", text, MsgBoxButtons::Ok);
    // Window width in DLU: px = DLU*2 + 4.
    let (ww, _) = frame.window_size();
    let w_dlu = (ww - 4) / 2;
    let body_px = (w_dlu - 26) * 2;
    let mut face = tabboz_win::font::Face::new(tabboz_win::fonts::FONT_REGULAR, 1);
    for c in frame.dlg().controls.iter().filter(|c| c.id < -9) {
        let tw = face.text_width(c.text);
        assert!(
            tw <= body_px,
            "line of {tw}px wider than body {body_px}px: {:?}",
            c.text
        );
    }
}
