// Offscreen rendering tests for the main window (milestone 2).
//
// The expected geometry comes from the .rc template (dialog 1) and the
// BWCC palette of the Novantotto web port.

use tabboz_win::font::Fonts;
use tabboz_win::pixel::Surface;
use tabboz_win::render::{Frame, TabbozView, UiEvent};
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
