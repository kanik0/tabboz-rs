// The dialog frame engine: lays out a DIALOG template, paints the Win95
// window chrome (title bar + menu bar) in BWCC colors, renders every
// control of the original game and tracks mouse interaction.
//
// One Frame instance drives one window; the app (tabboz-app) owns the
// winit window and softbuffer surface and feeds events in.

use std::collections::{HashMap, HashSet};

use crate::dialogs::{self, Control, ControlKind, Dialog};
use crate::font::Fonts;
use crate::img;
use crate::menus::{self, MenuItem};
use crate::pixel::{Image, Pixel, Surface};
use crate::theme::*;

/// Tabboz layers shown by BMPView (tabimg.c defaults).
/// The game updates these when clothes change; the port will do the same.
#[derive(Clone, Debug)]
pub struct TabbozView {
    pub sesso: char,
    pub testa: i32,
    pub giubbotto: i32,
    pub pantaloni: i32,
    pub scarpe: i32,
}

impl Default for TabbozView {
    fn default() -> Self {
        TabbozView { sesso: 'M', testa: 0, giubbotto: 0, pantaloni: 0, scarpe: 0 }
    }
}

impl TabbozView {
    /// Composes the whole tabboz on the original 143x275 canvas,
    /// in the exact layer order of tabimg.c WMPaint.
    pub fn compose(&self) -> Image {
        const W: i32 = 143;
        const H: i32 = 275;
        let mut out = Image { w: W, h: H, data: vec![0; (W * H) as usize] };

        let female = self.sesso == 'F';
        let sfondo = if female { 1561 } else { 1560 };
        let testa = self.testa + if female { 1370 } else { 1390 };
        let giubbotto = self.giubbotto + if female { 1507 } else { 1500 };
        let pantaloni = self.pantaloni + if female { 1525 } else { 1520 };
        let scarpe = self.scarpe + 1540;

        let mut layer = |id: i32, x: i32, y: i32| {
            if let Some(img) = asset_bitmap(&id.to_string()) {
                blit_yellow_transparent(&mut out, &img, x, y);
            }
        };

        layer(sfondo, 0, 0);
        // pantaloni (IMG_X_INC = 10, IMG_Y_INC = 0)
        if female {
            layer(pantaloni, 33, 120);
        } else {
            layer(pantaloni, 28, 93);
        }
        layer(scarpe, -6, 205);
        if female {
            layer(giubbotto, -17, 39);
            layer(testa, 40, 1);
        } else {
            layer(giubbotto, -20, 18);
            layer(testa, 42, 3);
        }
        out
    }
}

/// Blit treating pure yellow (255,255,0) as transparent, like the original.
fn blit_yellow_transparent(dst: &mut Image, src: &Image, x: i32, y: i32) {
    for yy in 0..src.h {
        for xx in 0..src.w {
            let px = src.data[(yy * src.w + xx) as usize];
            let (r, g, b) = ((px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF);
            if r == 0xFF && g == 0xFF && b == 0x00 {
                continue;
            }
            let dx = x + xx;
            let dy = y + yy;
            if dx >= 0 && dy >= 0 && dx < dst.w && dy < dst.h {
                dst.data[(dy * dst.w + dx) as usize] = 0xFF00_0000 | (px & 0x00FF_FFFF);
            }
        }
    }
}

fn asset_bitmap(name: &str) -> Option<Image> {
    let a = crate::assets::BITMAPS.iter().find(|a| a.name == name)?;
    img::decode_bmp(a.data).ok()
}

fn asset_icon(name: &str) -> Option<Image> {
    let a = crate::assets::ICONS.iter().find(|a| a.name == name)?;
    img::decode_ico(a.data).ok()
}

/// Events surfaced from the UI to the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEvent {
    /// A menu bar command (menu_id of the .rc menu).
    Command(u16),
    /// A push button was clicked (control id: 130 = scooter, ...).
    Button(i32),
    /// The window close button.
    Close,
    /// The minimize button (main window, WS_MINIMIZEBOX).
    Minimize,
}

pub struct Frame {
    dlg: &'static Dialog,
    tabboz: TabbozView,
    state: FrameState,
}

#[derive(Default)]
struct FrameState {
    mouse: (i32, i32),
    pressed_control: Option<usize>,
    pressed_menu: Option<usize>, // popup idx while the menubar item is held
    open_menu: Option<usize>,    // popup whose dropdown is open
    hover_menu_item: Option<usize>,
    pressed_sys_close: bool,
    pressed_sys_min: bool,
    /// The Edit control that owns the keyboard focus (caret).
    focused_edit: Option<usize>,
    /// Decoded-image cache keyed by asset name.
    cache: HashMap<String, Image>,
    /// Composed tabboz (recomputed when the view changes).
    composed: Option<Image>,
    /// SetDlgItemText overrides (control id -> text).
    text: HashMap<i32, String>,
    /// BM_SETCHECK state (radio/checkbox control ids).
    checked: HashSet<i32>,
    /// Render offset of this frame inside the window (modal dialogs).
    offset: (i32, i32),
    /// Which tipa bitmap BMPTipa shows (0 normal, 1 Babbo Natale, 2 mare).
    tipa_img: i32,
    /// Position of the last released click, relative to the clicked control.
    last_click: Option<(i32, i32)>,
}

impl Frame {
    pub fn new(id: u32) -> Self {
        Frame {
            dlg: dialogs::find(id).expect("unknown dialog id"),
            tabboz: TabbozView::default(),
            state: FrameState::default(),
        }
    }

    /// Builds a frame from a runtime dialog template (message boxes).
    pub fn from_dialog(dlg: &'static Dialog) -> Self {
        Frame {
            dlg,
            tabboz: TabbozView::default(),
            state: FrameState::default(),
        }
    }

    /// SetDlgItemText: overrides the text of a control at runtime.
    pub fn set_text(&mut self, id: i32, s: impl Into<String>) {
        self.state.text.insert(id, s.into());
    }

    /// Effective text of a control (override or template).
    pub fn text(&self, c: &Control) -> &str {
        self.state
            .text
            .get(&c.id)
            .map(|s| s.as_str())
            .unwrap_or(c.text)
    }

    /// BM_SETCHECK.
    pub fn check(&mut self, id: i32, on: bool) {
        if on {
            self.state.checked.insert(id);
        } else {
            self.state.checked.remove(&id);
        }
    }

    /// Radio semantics: all the radios of a dialog form one group (exactly
    /// what the original dialogs do), so checking one clears the others.
    pub fn check_radio(&mut self, id: i32) {
        let radios: Vec<i32> = self
            .dlg
            .controls
            .iter()
            .filter(|c| is_radio(c))
            .map(|c| c.id)
            .collect();
        for r in radios {
            self.state.checked.remove(&r);
        }
        self.state.checked.insert(id);
    }

    /// Whether a radio/checkbox control is currently checked.
    pub fn is_checked(&self, id: i32) -> bool {
        self.state.checked.contains(&id)
    }

    /// The dialog template this frame renders.
    pub fn dlg(&self) -> &'static Dialog {
        self.dlg
    }

    /// Whether an edit of this frame owns the keyboard focus.
    pub fn has_edit_focus(&self) -> bool {
        self.state.focused_edit.is_some()
    }

    /// ES_AUTOHSCROLL input: appends a character to the focused edit.
    pub fn type_char(&mut self, c: char) {
        let Some(idx) = self.state.focused_edit else { return };
        let id = self.dlg.controls[idx].id;
        let cur = self.state.text.get(&id).cloned().unwrap_or_else(|| self.dlg.controls[idx].text.to_string());
        if c >= ' ' && cur.chars().count() < 64 {
            let mut s = cur;
            s.push(c);
            self.set_text(id, s);
        }
    }

    /// Backspace into the focused edit.
    pub fn backspace(&mut self) {
        let Some(idx) = self.state.focused_edit else { return };
        let id = self.dlg.controls[idx].id;
        let mut cur = self.state.text.get(&id).cloned().unwrap_or_else(|| self.dlg.controls[idx].text.to_string());
        if cur.pop().is_some() {
            self.set_text(id, cur);
        }
    }

    /// All edit fields with their current text (the game reads them when
    /// the dialog closes, like the original GetDlgItemText calls).
    pub fn edit_texts(&self) -> Vec<(i32, String)> {
        self.dlg
            .controls
            .iter()
            .filter(|c| c.kind == ControlKind::Edit)
            .map(|c| (c.id, self.text(c).to_string()))
            .collect()
    }

    /// Position of this frame inside the window surface (modal dialogs).
    pub fn set_offset(&mut self, x: i32, y: i32) {
        self.state.offset = (x, y);
    }

    /// Render offset of this frame (where it is drawn in the window).
    pub fn offset(&self) -> (i32, i32) {
        self.state.offset
    }

    /// Selects the tipa bitmap of BMPTipa (current_tipa + 1204).
    pub fn set_tipa_image(&mut self, n: i32) {
        self.state.tipa_img = n;
    }

    /// Click position relative to the last clicked control (frame-local),
    /// consumed by the caller; used by the BMPTipa "palpatina" easter egg.
    pub fn take_last_click(&mut self) -> Option<(i32, i32)> {
        self.state.last_click.take()
    }

    pub fn dialog_id(&self) -> u32 {
        self.dlg.id
    }

    /// Raw dialog/window style bits (WS_MINIMIZEBOX etc.).
    pub fn dialog_style(&self) -> u32 {
        self.dlg.style
    }

    /// Total window size in logical pixels, chrome included.
    pub fn window_size(&self) -> (i32, i32) {
        let menu = if self.dlg.menu.is_some() { MENU_H } else { 0 };
        (
            self.dlg.w * DLU + 2 * BORDER,
            self.dlg.h * DLU + 2 * BORDER + TITLE_H + menu,
        )
    }

    pub fn set_tabboz_view(&mut self, v: TabbozView) {
        if self.tabboz.testa != v.testa
            || self.tabboz.giubbotto != v.giubbotto
            || self.tabboz.pantaloni != v.pantaloni
            || self.tabboz.scarpe != v.scarpe
            || self.tabboz.sesso != v.sesso
        {
            self.state.composed = None; // invalidate
        }
        self.tabboz = v;
    }

    // -- geometry ----------------------------------------------------------

    /// Client area origin in frame coordinates.
    fn origin(&self) -> (i32, i32) {
        let menu = if self.dlg.menu.is_some() { MENU_H } else { 0 };
        (BORDER, BORDER + TITLE_H + menu)
    }

    fn control_rect(&self, c: &Control) -> (i32, i32, i32, i32) {
        let (ox, oy) = self.origin();
        let (fx, fy) = self.state.offset;
        (fx + ox + c.x * DLU, fy + oy + c.y * DLU, c.w * DLU, c.h * DLU)
    }

    fn menubar_item_rect(&self, idx: usize, fonts: &mut Fonts) -> (i32, i32, i32, i32) {
        let mut x = BORDER + 4;
        for (i, popup) in menus::MENUBAR.iter().enumerate() {
            let label = strip_accel(popup.label);
            let w = fonts.regular.text_width(&label) + 12;
            if i == idx {
                return (x, BORDER + TITLE_H, w, MENU_H);
            }
            x += w;
        }
        (x, BORDER + TITLE_H, 0, MENU_H)
    }

    fn dropdown_rect(&self, popup_idx: usize, fonts: &mut Fonts) -> (i32, i32, i32, i32) {
        let popup = &menus::MENUBAR[popup_idx];
        let (mut w, mut h) = (0, 2 * 2); // 2px outer+inner border
        for item in popup.items {
            if item.separator {
                h += 5;
            } else {
                h += 16;
                w = w.max(fonts.regular.text_width(&strip_accel(item.label)) + 24);
            }
        }
        let (ix, iy, iw, _) = self.menubar_item_rect(popup_idx, fonts);
        let mut x = ix;
        if ix + w > BORDER + self.dlg.w * DLU {
            x = (BORDER + self.dlg.w * DLU - w).max(BORDER + 4);
        }
        (x, iy + MENU_H, w.min(iw.max(w)), h)
    }

    fn dropdown_item_rect(&self, popup_idx: usize, item_idx: usize, fonts: &mut Fonts) -> (i32, i32, i32, i32) {
        let (dx, dy, _, _) = self.dropdown_rect(popup_idx, fonts);
        let popup = &menus::MENUBAR[popup_idx];
        let mut y = dy + 2;
        for (i, item) in popup.items.iter().enumerate() {
            let h = if item.separator { 5 } else { 16 };
            if i == item_idx {
                let w = fonts
                    .regular
                    .text_width(&strip_accel(item.label))
                    .max(0)
                    + 24;
                return (dx + 2, y, w.max(10), h);
            }
            y += h;
        }
        (dx, dy, 0, 0)
    }

    fn hit_test(&self, x: i32, y: i32, fonts: &mut Fonts) -> Option<usize> {
        for (i, c) in self.dlg.controls.iter().enumerate() {
            if !is_clickable(c) {
                continue;
            }
            let (cx, cy, cw, ch) = self.control_rect(c);
            if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
                return Some(i);
            }
        }
        let _ = fonts;
        None
    }

    fn hit_menu_item(&self, x: i32, y: i32, fonts: &mut Fonts) -> Option<(usize, usize)> {
        let popup = self.state.open_menu?;
        let items = menus::MENUBAR[popup].items;
        for i in 0..items.len() {
            let (ix, iy, iw, ih) = self.dropdown_item_rect(popup, i, fonts);
            if x >= ix && x < ix + iw && y >= iy && y < iy + ih {
                return Some((popup, i));
            }
        }
        None
    }

    fn hit_menubar(&self, x: i32, y: i32, fonts: &mut Fonts) -> Option<usize> {
        for i in 0..menus::MENUBAR.len() {
            let (ix, iy, iw, ih) = self.menubar_item_rect(i, fonts);
            if x >= ix && x < ix + iw && y >= iy && y < iy + ih {
                return Some(i);
            }
        }
        None
    }

    // -- events ----------------------------------------------------------

    /// Returns true when a redraw is needed.
    pub fn mouse_move(&mut self, x: i32, y: i32, fonts: &mut Fonts) -> bool {
        let old_hover_item = self.state.hover_menu_item;
        self.state.mouse = (x, y);

        if self.state.open_menu.is_some() {
            self.state.hover_menu_item = self.hit_menu_item(x, y, fonts)
                .filter(|(_, i)| !menus::MENUBAR[self.state.open_menu.unwrap()].items[*i].separator)
                .map(|(_, i)| i);
            // Moving over another menubar title while a dropdown is open switches it.
            if let Some(idx) = self.hit_menubar(x, y, fonts) {
                if Some(idx) != self.state.open_menu {
                    self.state.open_menu = Some(idx);
                    self.state.hover_menu_item = None;
                    return true;
                }
            }
            return old_hover_item != self.state.hover_menu_item;
        }
        false
    }

    pub fn mouse_leave(&mut self) -> bool {
        self.state.hover_menu_item.take().is_some()
    }

    pub fn mouse_down(&mut self, x: i32, y: i32, fonts: &mut Fonts) -> bool {
        // Menubar / dropdown.
        if let Some((popup, item)) = self.hit_menu_item(x, y, fonts) {
            let _ = popup;
            self.state.pressed_menu = self.state.open_menu;
            self.state.hover_menu_item = Some(item);
            return true;
        }
        if let Some(idx) = self.hit_menubar(x, y, fonts) {
            let already = self.state.open_menu == Some(idx);
            self.state.open_menu = if already { None } else { Some(idx) };
            return true;
        }
        if self.state.open_menu.is_some() {
            // Click outside an open dropdown closes it.
            self.state.open_menu = None;
            return true;
        }
        // Close button.
        let (ww, _) = self.window_size();
        let (bx, by, bw, bh) = sys_button_rect(BORDER + ww - 2 * BORDER - 13 - 2, BORDER + 2);
        if x >= bx && x < bx + bw && y >= by && y < by + bh {
            self.state.pressed_sys_close = true;
            return true;
        }
        // Minimize button (WS_MINIMIZEBOX).
        if self.dlg.style & 0x0002_0000 != 0 {
            let (mx, my, mw, mh) = sys_button_rect(BORDER + ww - 2 * BORDER - 13 - 17 - 2, BORDER + 2);
            if x >= mx && x < mx + mw && y >= my && y < my + mh {
                self.state.pressed_sys_min = true;
                return true;
            }
        }
        // Dialog controls.
        if let Some(idx) = self.hit_test(x, y, fonts) {
            self.state.pressed_control = Some(idx);
            // Clicking an edit gives it the keyboard focus; anything else
            // takes the focus away (Win95 single-focus dialogs).
            self.state.focused_edit =
                if self.dlg.controls[idx].kind == ControlKind::Edit { Some(idx) } else { None };
            return true;
        }
        self.state.focused_edit = None;
        false
    }

    pub fn mouse_up(&mut self, x: i32, y: i32, fonts: &mut Fonts) -> Option<UiEvent> {
        if self.state.pressed_sys_min {
            self.state.pressed_sys_min = false;
            let (ww, _) = self.window_size();
            let (mx, my, mw, mh) = sys_button_rect(BORDER + ww - 2 * BORDER - 13 - 17 - 2, BORDER + 2);
            if x >= mx && x < mx + mw && y >= my && y < my + mh {
                return Some(UiEvent::Minimize);
            }
            return None;
        }
        if self.state.pressed_sys_close {
            self.state.pressed_sys_close = false;
            let (ww, _) = self.window_size();
            let (bx, by, bw, bh) = sys_button_rect(BORDER + ww - 2 * BORDER - 13 - 2, BORDER + 2);
            if x >= bx && x < bx + bw && y >= by && y < by + bh {
                return Some(UiEvent::Close);
            }
            return None;
        }

        if let Some((popup, item)) = self.hit_menu_item(x, y, fonts) {
            self.state.open_menu = None;
            self.state.hover_menu_item = None;
            let MenuItem { menu_id, separator, .. } = menus::MENUBAR[popup].items[item];
            if !separator {
                return Some(UiEvent::Command(menu_id));
            }
            return None;
        }

        if let Some(idx) = self.state.pressed_control.take() {
            if self.hit_test(x, y, fonts) == Some(idx) {
                let c = &self.dlg.controls[idx];
                let (cx, cy, _, _) = self.control_rect(c);
                self.state.last_click = Some((x - cx, y - cy));
                // BUTTON-class radios/checkboxes pick their behavior from
                // the BS_* style nibble, not from the control kind.
                if is_radio(c) {
                    // Radio: checking one clears all the others (single
                    // group per dialog, like the original templates).
                    self.check_radio(c.id);
                    return Some(UiEvent::Button(c.id));
                }
                if is_checkbox(c) {
                    let on = !self.state.checked.contains(&c.id);
                    self.check(c.id, on);
                    return Some(UiEvent::Button(c.id));
                }
                match c.kind {
                    ControlKind::Button | ControlKind::BorBtn => return Some(UiEvent::Button(c.id)),
                    ControlKind::BorRadio => {
                        self.check_radio(c.id);
                        return Some(UiEvent::Button(c.id));
                    }
                    ControlKind::BorCheck => {
                        let on = !self.state.checked.contains(&c.id);
                        self.check(c.id, on);
                        return Some(UiEvent::Button(c.id));
                    }
                    ControlKind::BmpView => return Some(UiEvent::Button(c.id)),
                    _ => {}
                }
                return None;
            }
            return None; // redraw is driven by the caller via needs_redraw()
        }
        None
    }

    /// Whether the frame wants a repaint (after mouse_up state changes).
    pub fn needs_redraw(&self) -> bool {
        self.state.pressed_control.is_none() || self.state.open_menu.is_some()
    }

    // -- rendering --------------------------------------------------------

    pub fn render(&mut self, surf: &mut Surface, fonts: &mut Fonts) {
        let (ww, wh) = self.window_size();
        let (fx, fy) = self.state.offset;

        // Window frame + background.
        surf.rect(fx, fy, ww, wh, FACE);
        surf.frame(fx, fy, ww, wh, INNER_LIGHT, DKSHADOW);
        surf.frame(fx + 1, fy + 1, ww - 2, wh - 2, WHITE, SHADOW);

        // Title bar (minimize button only on windows with WS_MINIMIZEBOX).
        let title = self.dlg.caption.unwrap_or("");
        let icon = self.cached_icon16("ZARROSIM");
        let show_min = self.dlg.style & 0x0002_0000 != 0; // WS_MINIMIZEBOX
        title_bar(
            surf,
            fonts,
            fx + BORDER,
            fy + BORDER,
            ww - 2 * BORDER,
            title,
            icon.as_ref(),
            show_min,
        );

        // Menu bar (only the main window has one).
        if self.dlg.menu.is_some() {
            self.render_menubar(surf, fonts);
        }

        // Client area.
        let (ox, oy) = self.origin();
        let (cw, ch) = (self.dlg.w * DLU, self.dlg.h * DLU);
        surf.rect(ox, oy, cw, ch, FACE);

        for (idx, c) in self.dlg.controls.iter().enumerate() {
            self.render_control(surf, fonts, c, idx);
        }

        // Dropdown menu (on top of everything).
        if let Some(popup) = self.state.open_menu {
            self.render_dropdown(surf, fonts, popup);
        }

        let _ = wh;
    }

    fn cached_icon(&mut self, name: &str) -> Option<Image> {
        if !self.state.cache.contains_key(name) {
            let img = asset_icon(name)?;
            self.state.cache.insert(name.to_string(), img);
        }
        self.state.cache.get(name).cloned()
    }

    /// 16x16 title-bar variant of an icon (Windows small icon).
    fn cached_icon16(&mut self, name: &str) -> Option<Image> {
        let key = format!("{name}16");
        if !self.state.cache.contains_key(&key) {
            let img = self.cached_icon(name)?.half();
            self.state.cache.insert(key.clone(), img);
        }
        self.state.cache.get(&key).cloned()
    }

    /// Decoded bitmap by resource name (used from milestone 4 for shop previews).
    fn cached_bitmap(&mut self, name: &str) -> Option<Image> {
        if !self.state.cache.contains_key(name) {
            let img = asset_bitmap(name)?;
            self.state.cache.insert(name.to_string(), img);
        }
        self.state.cache.get(name).cloned()
    }

    /// Chrome bitmap (novantotto/: OK/Cancel faces, glyphs) by name.
    fn cached_chrome(&mut self, name: &str) -> Option<Image> {
        if !self.state.cache.contains_key(name) {
            let a = crate::assets::CHROME_BITMAPS.iter().find(|a| a.name == name)?;
            let img = img::decode_bmp(a.data).ok()?;
            self.state.cache.insert(name.to_string(), img);
        }
        self.state.cache.get(name).cloned()
    }

    fn composed_tabboz(&mut self) -> Image {
        if self.state.composed.is_none() {
            self.state.composed = Some(self.tabboz.compose());
        }
        self.state.composed.clone().unwrap()
    }

    fn render_menubar(&mut self, surf: &mut Surface, fonts: &mut Fonts) {
        let my = BORDER + TITLE_H;
        surf.rect(BORDER, my, self.window_size().0 - 2 * BORDER, MENU_H, FACE);
        for i in 0..menus::MENUBAR.len() {
            let (ix, iy, iw, ih) = self.menubar_item_rect(i, fonts);
            let open = self.state.open_menu == Some(i);
            let hover = self.state.mouse.1 >= iy
                && self.state.mouse.1 < iy + ih
                && self.state.mouse.0 >= ix
                && self.state.mouse.0 < ix + iw;
            if open || hover {
                surf.rect(ix + 1, iy + 1, iw - 2, ih - 2, NAVY);
                fonts.regular.draw_accel_text(surf, menus::MENUBAR[i].label, ix + 5, iy + 3, WHITE);
            } else {
                fonts.regular.draw_accel_text(surf, menus::MENUBAR[i].label, ix + 5, iy + 3, BLACK);
            }
        }
    }

    fn render_dropdown(&mut self, surf: &mut Surface, fonts: &mut Fonts, popup_idx: usize) {
        let (dx, dy, dw, dh) = self.dropdown_rect(popup_idx, fonts);
        // Shadow under the dropdown.
        surf.rect(dx + 2, dy + dh, dw, 1, SHADOW);
        surf.rect(dx + dw, dy + 2, 1, dh, SHADOW);
        // Panel.
        surf.rect(dx, dy, dw, dh, FACE);
        surf.frame(dx, dy, dw, dh, WHITE, DKSHADOW);
        surf.frame(dx + 1, dy + 1, dw - 2, dh - 2, INNER_LIGHT, SHADOW);
        for i in 0..menus::MENUBAR[popup_idx].items.len() {
            let item = menus::MENUBAR[popup_idx].items[i];
            let (ix, iy, _iw, ih) = self.dropdown_item_rect(popup_idx, i, fonts);
            let (iw, ih) = (dw - 4, ih);
            if item.separator {
                surf.hline(ix, iy + 2, iw, SHADOW);
                surf.hline(ix, iy + 3, iw, WHITE);
            } else {
                let hover = self.state.hover_menu_item == Some(i);
                if hover {
                    surf.rect(ix, iy, iw, ih, NAVY);
                    fonts.regular.draw_accel_text(surf, item.label, ix + 10, iy + 2, WHITE);
                } else {
                    fonts.regular.draw_accel_text(surf, item.label, ix + 10, iy + 2, BLACK);
                }
            }
        }
    }

    fn render_control(&mut self, surf: &mut Surface, fonts: &mut Fonts, c: &Control, idx: usize) {
        let (x, y, w, h) = self.control_rect(c);
        if w <= 0 || h <= 0 {
            return;
        }
        match c.kind {
            ControlKind::Button | ControlKind::BorBtn => {
                // BWCC bitmap buttons. OK/Cancel BorBtns (ids 1/2) ALWAYS
                // draw the chrome faces 1001/1002 — the .rc carries junk
                // text on them ("Button", "OK") and the web port uses the
                // bitmap for all of them. BS_OWNERDRAW (0x8000) BorBtns —
                // empty OR junk text — show the game bitmap whose resource
                // id is the control id + 1000 (shop previews, firm logos,
                // phone photos, trucca images, the About picture...),
                // stretched to the control rect: the same id mapping and
                // sizing the web port uses.
                let text = self.text(c);
                if c.kind == ControlKind::BorBtn
                    && (matches!(c.id, 1 | 2) || text.is_empty() || c.style & 0x8000 != 0)
                {
                    if matches!(c.id, 1 | 2) {
                        let bmp = if c.id == 1 { "1001" } else { "1002" };
                        if let Some(img) = self.cached_chrome(bmp) {
                            let (ox, oy) = ((w - img.w) / 2, (h - img.h) / 2);
                            surf.blit(&img, x + ox, y + oy);
                        }
                    } else if let Some(img) = self.cached_bitmap(&(c.id + 1000).to_string()) {
                        surf.blit_stretched(&img, x, y, w, h);
                    } else {
                        self.render_button(surf, fonts, c, x, y, w, h, false);
                    }
                    return;
                }
                let pressed = self.state.pressed_control == Some(idx);
                // Plain BUTTON-class controls pick their look from the
                // BS_* style nibble, like the web port (input type=radio /
                // type=checkbox / fieldset).
                if is_radio(c) {
                    let checked = self.state.checked.contains(&c.id);
                    radio(surf, x, y + (h - 12) / 2, checked);
                    let text = self.text(c).to_string(); // borrow dance: self.text() borrows self
                    fonts.regular.draw_accel_text(surf, &text, x + 16, y + 1, BLACK);
                    return;
                }
                if is_checkbox(c) {
                    let checked = self.state.checked.contains(&c.id);
                    checkbox(surf, x, y + 1, checked);
                    let text = self.text(c).to_string();
                    fonts.regular.draw_accel_text(surf, &text, x + 18, y + 1, BLACK);
                    return;
                }
                if is_groupbox(c) {
                    group_box(surf, x, y, w, h);
                    let text = self.text(c).to_string();
                    fonts.regular.draw_accel_text(surf, &text, x + 8, y + 1, BLACK);
                    return;
                }
                self.render_button(surf, fonts, c, x, y, w, h, pressed);
            }
            ControlKind::BorRadio => {
                let checked = self.state.checked.contains(&c.id);
                radio(surf, x, y + (h - 12) / 2, checked);
                let text = self.text(c).to_string(); // borrow dance: self.text() borrows self
                fonts.regular.draw_accel_text(surf, &text, x + 16, y + 1, BLACK);
            }
            ControlKind::BorCheck => {
                let checked = self.state.checked.contains(&c.id);
                checkbox(surf, x, y + 1, checked);
                let text = self.text(c).to_string();
                fonts.regular.draw_accel_text(surf, &text, x + 18, y + 1, BLACK);
            }
            ControlKind::BorStatic => {
                self.render_text(surf, fonts, c, x, y, w, h, BLACK);
            }
            ControlKind::Static => {
                if c.style & 0xF == 3 {
                    // SS_ICON: the control text names the icon resource.
                    let icon_name = self.text(c).to_string();
                    if let Some(icon) = self.cached_icon(&icon_name) {
                        surf.blit(&icon, x, y);
                    }
                } else {
                    self.render_text(surf, fonts, c, x, y, w, h, BLACK);
                }
            }
            ControlKind::Edit => {
                surf.rect(x, y, w, h, WHITE);
                sunken_field(surf, x, y, w, h);
                // The edit shows its text (ES_AUTOHSCROLL: the tail scrolls
                // into view) and a caret while focused, like Win95.
                let text = self.text(c).to_string();
                let mut shown = text.as_str();
                while !shown.is_empty() && fonts.regular.text_width(shown) > w - 5 {
                    shown = &shown[shown.char_indices().nth(1).map(|(i, _)| i).unwrap_or(shown.len())..];
                }
                fonts.regular.draw_text(surf, shown, x + 2, y + 1, BLACK);
                if self.state.focused_edit == Some(idx) {
                    let tx = x + 2 + fonts.regular.text_width(shown);
                    surf.rect(tx, y + 2, 1, (h - 4).max(1), BLACK);
                }
            }
            ControlKind::BorShade => match c.style & 0xF {
                1 => shade_box(surf, x, y, w, h),
                2 => shade_hdip(surf, x, y, w, h),
                3 => shade_vdip(surf, x, y, w, h),
                5 => shade_vbump(surf, x, y, w, h),
                _ => {}
            },
            ControlKind::BmpView => {
                let img = self.composed_tabboz();
                surf.blit_clip(&img, x, y, x, y, w, h);
            }
            ControlKind::BmpTipa => {
                // BMPTipa: single bitmap 1204 + current_tipa (tabimg.c).
                let name = (1204 + self.state.tipa_img).to_string();
                if let Some(img) = self.cached_bitmap(&name) {
                    surf.blit_clip(&img, x, y, x, y, w, h);
                }
            }
            ControlKind::Combobox => {
                surf.rect(x, y, w, h, WHITE);
                sunken_field(surf, x, y, w, h);
            }
            ControlKind::MsctlsProgress => {
                surf.rect(x, y, w, h, FACE);
                sunken_field(surf, x, y, w, h);
                surf.rect(x + 2, y + 2, (w - 4) / 2, h - 4, NAVY);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_button(
        &mut self,
        surf: &mut Surface,
        fonts: &mut Fonts,
        c: &Control,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        pressed: bool,
    ) {
        let is_default = c.style & 0x1 != 0; // BS_DEFPUSHBUTTON
        if is_default && !pressed {
            surf.frame(x - 1, y - 1, w + 2, h + 2, BLACK, BLACK);
        }
        if pressed {
            pressed_button(surf, x, y, w, h);
        } else {
            raised_button(surf, x, y, w, h);
        }
        let text = self.text(c).to_string();
        let label = strip_accel(&text);
        let tw = fonts.regular.text_width(&label);
        let tx = x + (w - tw) / 2 + pressed as i32;
        let ty = y + (h - fonts.regular.line_height()) / 2 + pressed as i32;
        fonts.regular.draw_accel_text(surf, &text, tx, ty, BLACK);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_text(
        &mut self,
        surf: &mut Surface,
        fonts: &mut Fonts,
        c: &Control,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        color: Pixel,
    ) {
        let align_center = c.style & 0xF == 1; // SS_CENTER
        let align_right = c.style & 0xF == 2; // SS_RIGHT
        let no_prefix = c.style & 0x80 != 0; // SS_NOPREFIX
        let text = self.text(c).to_string();
        // Win32 static controls word-wrap by default: the long shop-item
        // descriptions (Bau House &c.) fit their SS_CENTER rect.
        let mut yy = y;
        'outer: for para in text.split('\n') {
            let mut wrapped: Vec<String> = vec![String::new()];
            for word in para.split(' ') {
                let cur = wrapped.last_mut().unwrap();
                let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                let measured = if no_prefix {
                    candidate.clone()
                } else {
                    strip_accel(&candidate)
                };
                if !cur.is_empty() && fonts.regular.text_width(&measured) > w {
                    wrapped.push(word.to_string());
                } else {
                    *cur = candidate;
                }
            }
            for line in &wrapped {
                let label = if no_prefix { line.clone() } else { strip_accel(line) };
                let tw = fonts.regular.text_width(&label);
                let tx = if align_center {
                    x + (w - tw) / 2
                } else if align_right {
                    x + w - tw
                } else {
                    x
                };
                if no_prefix {
                    fonts.regular.draw_text(surf, line, tx, yy, color);
                } else {
                    fonts.regular.draw_accel_text(surf, line, tx, yy, color);
                }
                yy += fonts.regular.line_height();
                if yy >= y + h {
                    break 'outer;
                }
            }
        }
    }
}

/// Removes '&' hotkey markers from a label.
fn strip_accel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// BS_* flavor of a BUTTON-class control (low nibble of the style).
fn bs_style(c: &Control) -> u32 {
    c.style & 0xF
}

/// A radio button: BorRadio class, or BUTTON with BS_AUTORADIOBUTTON (9).
fn is_radio(c: &Control) -> bool {
    c.kind == ControlKind::BorRadio || (c.kind == ControlKind::Button && bs_style(c) == 9)
}

/// A checkbox: BorCheck class, or BUTTON with BS_CHECKBOX (2),
/// BS_AUTOCHECKBOX (3), BS_3STATE (5), BS_AUTO3STATE (6).
fn is_checkbox(c: &Control) -> bool {
    c.kind == ControlKind::BorCheck || (c.kind == ControlKind::Button && matches!(bs_style(c), 2 | 3 | 5 | 6))
}

/// A group box: BUTTON with BS_GROUPBOX (7) — etched frame with a label.
fn is_groupbox(c: &Control) -> bool {
    c.kind == ControlKind::Button && bs_style(c) == 7
}

fn is_clickable(c: &Control) -> bool {
    if is_groupbox(c) || c.style & 0x0800_0000 != 0 {
        // BS_GROUPBOX is decoration; WS_DISABLED controls ignore input.
        return false;
    }
    matches!(
        c.kind,
        ControlKind::Button
            | ControlKind::BorBtn
            | ControlKind::BorRadio
            | ControlKind::BorCheck
            | ControlKind::Edit
            | ControlKind::BmpView
    )
}

/// Buttons of a message box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgBoxButtons {
    Ok,
    YesNo,
}

/// Win95-style message box: title bar, text, buttons.
///
/// Templates are synthesized at runtime and cached by content: the game
/// repeats the same messages over a session, so the set stays bounded.
pub fn message_box(title: &str, text: &str, buttons: MsgBoxButtons) -> Frame {
    use std::cell::RefCell;

    thread_local! {
        static CACHE: RefCell<HashMap<(String, String, bool), &'static Dialog>> = RefCell::new(HashMap::new());
    }

    CACHE.with(|cache| {
        let key = (title.to_string(), text.to_string(), buttons == MsgBoxButtons::YesNo);
        if let Some(dlg) = cache.borrow().get(&key) {
            return Frame::from_dialog(dlg);
        }

        // The original MessageBox wraps text to the box width; measure with
        // the real face (MS Sans Serif 11px) instead of estimating per
        // character, and word-wrap long lines (Norme di utilizzo, the
        // Progresso joke, ...) so they never overflow the window.
        let mut face = crate::font::Face::new(crate::fonts::FONT_REGULAR, 1);
        // Wrap to the BODY width: the text control is x=14, w-26 DLU, so a
        // 160-DLU-clamped box gives (160-26)*2 = 268 px of usable width —
        // wrapping to the window width would still overflow the body.
        const BODY_W_PX: i32 = (160 - 26) * 2;
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            let mut cur = String::new();
            for word in para.split(' ') {
                let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if !cur.is_empty() && face.text_width(&candidate) > BODY_W_PX {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = candidate;
                }
            }
            lines.push(cur);
        }
        let longest = lines.iter().map(|l| face.text_width(l)).max().unwrap_or(10);
        // Body = w-26 DLU must hold the longest wrapped line, with the
        // text control starting at x=14 DLU (12 DLU right margin).
        let w = ((longest + 1) / 2 + 26).clamp(90, 160);
        let h = (lines.len() as i32 * 13 + 60) / 2 + 6;

        let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };

        let mut controls: Vec<Control> = Vec::new();
        // Text lines (like the original MessageBox body).
        for (i, line) in lines.iter().enumerate() {
            controls.push(Control {
                id: -(i as i32) - 10,
                kind: ControlKind::BorStatic,
                class: "BorStatic",
                text: leak(line.as_str()),
                style: 0x4000_0000 | 0x1000_0000 | 0x0002_0000, // SS_LEFT(0)|WS_CHILD|WS_VISIBLE|WS_GROUP
                x: 14,
                y: 8 + i as i32 * 7,
                w: w - 26,
                h: 7,
            });
        }
        match buttons {
            MsgBoxButtons::Ok => {
                controls.push(ok_button(w / 2 - 16, h - 16));
            }
            MsgBoxButtons::YesNo => {
                controls.push(Control {
                    id: 6, // IDYES
                    kind: ControlKind::BorBtn,
                    class: "BorBtn",
                    text: "Si'",
                    style: 0x1 | 0x4000_0000 | 0x1000_0000 | 0x0001_0000,
                    x: w / 2 - 34,
                    y: h - 16,
                    w: 33,
                    h: 11,
                });
                controls.push(Control {
                    id: 7, // IDNO
                    kind: ControlKind::BorBtn,
                    class: "BorBtn",
                    text: "No",
                    style: 0x4000_0000 | 0x1000_0000 | 0x0001_0000,
                    x: w / 2 + 1,
                    y: h - 16,
                    w: 33,
                    h: 11,
                });
            }
        }

        let dlg: &'static Dialog = Box::leak(Box::new(Dialog {
            id: 0xFFFF, // runtime-only dialog
            style: 0x40 | 0x80 | 0x8000_0000 | 0x00C0_0000 | 0x0008_0000, // DS_SETFONT|DS_MODALFRAME|WS_POPUP|WS_CAPTION|WS_SYSMENU
            caption: Some(leak(title)),
            menu: None,
            class: Some("Bordlg"),
            font_size: Some(10),
            font_name: Some("MS Sans Serif"),
            x: 0,
            y: 0,
            w,
            h,
            controls: Box::leak(controls.into_boxed_slice()),
        }));
        cache.borrow_mut().insert(key, dlg);
        Frame::from_dialog(dlg)
    })
}

/// The OK button of a message box (BorBtn, id 1 = IDOK).
fn ok_button(x: i32, y: i32) -> Control {
    Control {
        id: 1,
        kind: ControlKind::BorBtn,
        class: "BorBtn",
        text: "OK",
        style: 0x1 | 0x4000_0000 | 0x1000_0000 | 0x0001_0000, // BS_DEFPUSHBUTTON|...
        x,
        y,
        w: 33,
        h: 11,
    }
}
