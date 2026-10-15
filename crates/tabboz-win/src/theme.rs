// BWCC (Borland Windows Custom Controls) look: palette and widget painting.
//
// Colors follow the Novantotto web port of the game (resources/css),
// which in turn reproduces the original Borland dialogs:
//   face #d4d0c8, title bar #0a246a, light #ffffff, shadow #808080,
//   dark outline #0a0a0a, inner light #dfdfdf.

use crate::font::Fonts;
use crate::pixel::{Pixel, Surface};

pub const FACE: Pixel = 0x00D4D0C8;
pub const WHITE: Pixel = 0x00FFFFFF;
pub const INNER_LIGHT: Pixel = 0x00DFDFDF;
pub const SHADOW: Pixel = 0x00808080;
pub const DKSHADOW: Pixel = 0x000A0A0A;
pub const NAVY: Pixel = 0x000A246A;
pub const BLACK: Pixel = 0x00000000;
pub const TEAL: Pixel = 0x00008080;
pub const GRAY_TEXT: Pixel = 0x00808080;

/// 1 dialog unit = 2 logical pixels (same scale as the web port).
pub const DLU: i32 = 2;

/// Window frame: 1px outer #dfdfdf (l/t) / #0a0a0a (r/b), 1px white/gray,
/// matching the 98.css window shadow stack used by the web port.
pub const BORDER: i32 = 2;

/// Title bar height (logical px).
pub const TITLE_H: i32 = 18;

/// Menu bar height (logical px).
pub const MENU_H: i32 = 20;

pub fn sunken_field(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    // Classic Win95 sunken field (edit boxes).
    s.frame(x, y, w, h, SHADOW, DKSHADOW);
    s.frame(x + 1, y + 1, w - 2, h - 2, WHITE, SHADOW);
}

pub fn raised_button(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    // Classic Win95 push button.
    s.frame(x, y, w, h, WHITE, DKSHADOW);
    s.frame(x + 1, y + 1, w - 2, h - 2, INNER_LIGHT, SHADOW);
    s.rect(x + 2, y + 2, w - 4, h - 4, FACE);
}

pub fn pressed_button(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    s.frame(x, y, w, h, DKSHADOW, WHITE);
    s.frame(x + 1, y + 1, w - 2, h - 2, SHADOW, INNER_LIGHT);
    s.rect(x + 2, y + 2, w - 4, h - 4, FACE);
}

/// BorShade 0x8001: thin inset frame (top/left shadow, bottom/right white).
pub fn shade_box(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    if w < 1 || h < 1 {
        return;
    }
    s.frame(x, y, w, h, SHADOW, WHITE);
}

/// BorShade 0x2: sunken horizontal dip.
pub fn shade_hdip(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    s.rect(x, y, w, h.min(1), SHADOW);
    if h > 1 {
        s.rect(x, y + 1, w, h - 1, WHITE);
    }
}

/// BorShade 0x3: sunken vertical dip.
pub fn shade_vdip(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    s.rect(x, y, w.min(1), h, SHADOW);
    if w > 1 {
        s.rect(x + 1, y, w - 1, h, WHITE);
    }
}

/// BorShade 0x5: raised vertical bump.
pub fn shade_vbump(s: &mut Surface, x: i32, y: i32, w: i32, h: i32) {
    s.rect(x, y, w.min(1), h, WHITE);
    if w > 1 {
        s.rect(x + 1, y, w - 1, h, SHADOW);
    }
}

/// Win95 radio button circle, centered in a 12x12 box.
pub fn radio(s: &mut Surface, x: i32, y: i32, checked: bool) {
    let (cx, cy) = (x + 5, y + 5);
    // Outer ring.
    for (dx, dy, c) in [
        (2, 0, SHADOW),
        (3, 0, DKSHADOW),
        (1, 1, DKSHADOW),
        (0, 2, DKSHADOW),
        (0, 3, DKSHADOW),
    ] {
        for (sx, sy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            s.set(cx + sx * dx, cy + sy * dy, c);
            s.set(cx + sx * dy, cy + sy * dx, c);
        }
    }
    // Inner white field.
    for yy in -3..=3 {
        for xx in -3..=3 {
            if xx * xx + yy * yy <= 8 {
                s.set(cx + xx, cy + yy, WHITE);
            }
        }
    }
    if checked {
        for yy in -2..=2 {
            for xx in -2..=2 {
                if xx * xx + yy * yy <= 4 {
                    s.set(cx + xx, cy + yy, BLACK);
                }
            }
        }
    }
}

/// Win95 check box, 13x13.
pub fn checkbox(s: &mut Surface, x: i32, y: i32, checked: bool) {
    sunken_field(s, x, y, 13, 13);
    if checked {
        // Diagonal check mark.
        for i in 0..5 {
            let dy = i;
            s.set(x + 3 + i, y + 5 + dy, BLACK);
            s.set(x + 7 - i, y + 5 + dy, BLACK);
            s.set(x + 2 + i, y + 5 + dy, DKSHADOW);
            s.set(x + 8 - i, y + 5 + dy, DKSHADOW);
        }
    }
}

/// Title bar of an active BWCC window.
#[allow(clippy::too_many_arguments)]
pub fn title_bar(
    s: &mut Surface,
    fonts: &mut Fonts,
    x: i32,
    y: i32,
    w: i32,
    title: &str,
    icon: Option<&crate::pixel::Image>,
    show_min: bool,
) {
    s.rect(x, y, w, TITLE_H, NAVY);
    let mut tx = x + 3;
    if let Some(img) = icon {
        s.blit(img, tx, y + 1);
        tx += 16 + 3;
    }
    fonts.bold.draw_text(s, title, tx, y + 3, WHITE);
    // Caption buttons: minimize and close for the main window (WS_MINIMIZEBOX),
    // close only for modal dialogs.
    if show_min {
        draw_sys_button(s, x + w - 13 - 17, y + 2, SysButton::Minimize);
    }
    draw_sys_button(s, x + w - 13 - 2, y + 2, SysButton::Close);
}

pub enum SysButton {
    Minimize,
    Close,
}

pub fn sys_button_rect(x: i32, y: i32) -> (i32, i32, i32, i32) {
    (x, y, 13, 13)
}

pub fn draw_sys_button(s: &mut Surface, x: i32, y: i32, kind: SysButton) {
    s.frame(x, y, 13, 13, WHITE, DKSHADOW);
    s.frame(x + 1, y + 1, 11, 11, INNER_LIGHT, SHADOW);
    s.rect(x + 2, y + 2, 9, 9, FACE);
    match kind {
        SysButton::Minimize => {
            s.rect(x + 3, y + 8, 6, 2, BLACK);
        }
        SysButton::Close => {
            for i in 0..7 {
                s.set(x + 3 + i, y + 3 + i, BLACK);
                s.set(x + 9 - i, y + 3 + i, BLACK);
            }
        }
    }
}
