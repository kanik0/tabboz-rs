// Text rendering with the bundled MS Sans Serif fonts.
//
// The bundled ms_sans_serif.woff is 98.css "Pixelated MS Sans Serif": the
// outlines are traced from the original bitmap but the shapes sit half a
// pixel off-grid, and ab_glyph (like macOS CoreText in browsers) does not
// run TrueType hinting. The faithful rendering is therefore the same the
// web port shows in a browser: grayscale antialiasing at the physical
// resolution, with glyphs rasterized at TEXT_SIZE * scale on integer
// positions. A contrast curve deepens the strokes and removes the
// fringe, which keeps the text as crisp as the design allows.

use ab_glyph::{Font, FontArc, Glyph, PxScale, PxScaleFont, ScaleFont, point};
use std::collections::HashMap;

use crate::pixel::{Pixel, Surface};

/// Logical text size of the original dialogs (MS Sans Serif 10pt @96dpi).
pub const TEXT_SIZE: f32 = 11.0;

/// Contrast curve calibrated on this font: the outlines sit half a pixel
/// off-grid, so true strokes carry ~0.45-0.75 coverage while the fringe
/// stays below ~0.2. The curve maps 0.5+ to full ink, keeps a soft ramp
/// for curved edges (0.2-0.5) and drops the fringe entirely (<0.1).
fn contrast(c: f32) -> u8 {
    (((c - 0.1) / 0.4).clamp(0.0, 1.0) * 255.0) as u8
}

pub struct Face {
    font: PxScaleFont<FontArc>,
    /// Regular face used for glyphs the main face lacks (bold fallback).
    fallback: Option<PxScaleFont<FontArc>>,
    cache: HashMap<char, CachedGlyph>,
    /// Integer upscale factor: glyphs are rasterized at TEXT_SIZE * scale.
    scale: i32,
    /// Ascent/descent at the rasterization size (physical pixels).
    ascent: f32,
    descent: f32,
}

struct CachedGlyph {
    /// Alpha coverage after the contrast curve, w*h (physical pixels).
    bitmap: Vec<u8>,
    w: i32,
    h: i32,
    /// Offset of the bitmap top-left from the glyph origin (physical ints).
    x: i32,
    y: i32,
    /// Advance to the next character (physical pixels).
    advance: f32,
}

impl Face {
    pub fn new(data: &[u8], scale: i32) -> Self {
        Self::with_fallback(data, None, scale)
    }

    /// The Pixelated MS Sans Serif bold face shipped by 98.css lacks most
    /// punctuation (" ' * + , . / < = > ? \ _ |); browsers paper over this
    /// with the CSS font-family fallback chain, so the port rasterizes
    /// missing glyphs from the regular face instead of the .notdef box.
    pub fn with_fallback(data: &[u8], fallback_data: Option<&[u8]>, scale: i32) -> Self {
        let font = FontArc::try_from_vec(data.to_vec()).expect("valid font");
        let size = TEXT_SIZE * scale as f32;
        let scaled: PxScaleFont<FontArc> = font.into_scaled(PxScale::from(size));
        let ascent = scaled.ascent();
        let descent = scaled.descent();
        let fallback = fallback_data.map(|d| {
            let fb = FontArc::try_from_vec(d.to_vec()).expect("valid font");
            fb.into_scaled(PxScale::from(size))
        });
        Face {
            font: scaled,
            fallback,
            cache: HashMap::new(),
            scale,
            ascent,
            descent,
        }
    }

    /// Logical line height (matches the original dialog text rows).
    pub fn line_height(&self) -> i32 {
        ((self.ascent - self.descent) / self.scale as f32).ceil() as i32
    }

    fn glyph(&mut self, c: char) -> &CachedGlyph {
        let main = self.font.clone();
        let fallback = self.fallback.clone();
        self.cache.entry(c).or_insert_with(|| {
            let missing = main.glyph_id(c).0 == 0;
            let font = if missing { fallback.unwrap_or(main) } else { main };
            let glyph_id = font.glyph_id(c);
            let advance = font.h_advance(glyph_id);
            // Rasterize at the origin on the physical grid; the draw
            // position snaps to integers so every glyph lands where the
            // grid expects it.
            let glyph = Glyph {
                id: glyph_id,
                scale: font.scale,
                position: point(0.0, 0.0),
            };
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bb = outlined.px_bounds();
                let w = bb.width().ceil() as i32;
                let h = bb.height().ceil() as i32;
                let mut bitmap = vec![0u8; (w * h).max(0) as usize];
                outlined.draw(|x, y, coverage| {
                    if x < w as u32 && y < h as u32 {
                        bitmap[(y * w as u32 + x) as usize] = contrast(coverage);
                    }
                });
                CachedGlyph {
                    bitmap,
                    w,
                    h,
                    x: bb.min.x.round() as i32,
                    y: bb.min.y.round() as i32,
                    advance,
                }
            } else {
                CachedGlyph { bitmap: Vec::new(), w: 0, h: 0, x: 0, y: 0, advance }
            }
        })
    }

    /// Width of a single-line string, in LOGICAL pixels.
    pub fn text_width(&mut self, s: &str) -> i32 {
        let mut x = 0.0f32;
        for c in s.chars() {
            x += self.glyph(c).advance;
        }
        (x / self.scale as f32).ceil() as i32
    }

    /// Draws one glyph's ink with alpha blending (physical coordinates).
    fn stamp(&self, surf: &mut Surface, g: &CachedGlyph, gx: i32, gy: i32, color: Pixel) {
        for yy in 0..g.h {
            for xx in 0..g.w {
                let a = g.bitmap[(yy * g.w + xx) as usize];
                if a > 0 {
                    surf.blend_phys(gx + xx, gy + yy, color, a);
                }
            }
        }
    }

    /// Draws a single line at a LOGICAL (x = left, y = top) position.
    fn draw_line(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let scale = self.scale;
        let baseline = y * scale + self.ascent.round() as i32;
        let mut pen = x * scale;
        for c in s.chars() {
            // Copy what stamp needs so self is free again.
            let (bitmap, w, h, gx, gy, advance) = {
                let g = self.glyph(c);
                (g.bitmap.clone(), g.w, g.h, pen + g.x, baseline + g.y, g.advance)
            };
            let g = CachedGlyph { bitmap, w, h, x: 0, y: 0, advance };
            self.stamp(surf, &g, gx, gy, color);
            // Snap the pen to the physical grid: no subpixel drift.
            pen = (pen as f32 + advance).round() as i32;
        }
    }

    pub fn draw_text(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        self.draw_line(surf, s, x, y, color);
    }

    /// Draws text underlining the character after '&' (Windows accelerators).
    pub fn draw_accel_text(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let scale = self.scale;
        let baseline = y * scale + self.ascent.round() as i32;
        let mut pen = x * scale;
        let mut chars = s.chars().peekable();
        let mut underline_next = false;
        while let Some(c) = chars.next() {
            if c == '&' {
                if chars.peek() == Some(&'&') {
                    chars.next(); // '&&' = literal '&'
                } else {
                    underline_next = true;
                    continue;
                }
            }
            let (bitmap, w, h, gx, gy, advance) = {
                let g = self.glyph(c);
                (g.bitmap.clone(), g.w, g.h, pen + g.x, baseline + g.y, g.advance)
            };
            let g = CachedGlyph { bitmap, w, h, x: 0, y: 0, advance };
            self.stamp(surf, &g, gx, gy, color);
            if underline_next {
                let uw = advance.round() as i32;
                surf.rect((pen / scale).max(0), baseline / scale, (uw / scale).max(1), 1, color);
                underline_next = false;
            }
            pen = (pen as f32 + advance).round() as i32;
        }
    }
}

/// The two faces used by the game UI.
pub struct Fonts {
    pub regular: Face,
    pub bold: Face,
    pub scale: i32,
}

impl Fonts {
    pub fn new(scale: i32) -> Self {
        Fonts {
            regular: Face::new(crate::fonts::FONT_REGULAR, scale),
            bold: Face::with_fallback(crate::fonts::FONT_BOLD, Some(crate::fonts::FONT_REGULAR), scale),
            scale,
        }
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(text: &str, scale: i32) -> (Vec<u32>, i32, i32) {
        let mut f = Fonts::new(scale);
        let w = f.regular.text_width(text);
        let h = 20;
        let mut buf = vec![0xFFFFFFu32; (w as usize * scale as usize) * (h as usize * scale as usize)];
        let mut s = Surface::new(&mut buf, w, h, scale);
        f.regular.draw_text(&mut s, text, 0, 3, 0x000000);
        (buf, w, h)
    }

    #[test]
    fn fonts_load_and_measure() {
        let mut f = Fonts::new(1);
        let w = f.regular.text_width("Tabboz Simulator");
        assert!(w > 50, "title text suspiciously narrow: {w}");
        let (buf, _, _) = render("Tabboz Simulator", 1);
        assert!(buf.iter().any(|p| *p != 0xFFFFFF), "no pixels drawn");
    }

    #[test]
    fn accel_text_underlines_hotkey() {
        let mut f = Fonts::new(1);
        let mut buf = vec![0xFFFFFFu32; 100 * 20];
        let mut s = Surface::new(&mut buf, 100, 20, 1);
        f.regular.draw_accel_text(&mut s, "&File", 0, 3, 0x000000);
        assert!(buf.iter().any(|p| *p != 0xFFFFFF));
    }

    /// The strokes must be dark: the contrast curve has to keep the core
    /// of every glyph near full ink, or the text reads as washed out.
    #[test]
    fn strokes_are_dark() {
        for scale in [1, 2] {
            let (buf, _, _) = render("Profitto scolastico", scale);
            let mut ink = 0usize;
            let mut dark = 0usize;
            for p in &buf {
                if *p != 0xFFFFFF {
                    ink += 1;
                    let r = (*p >> 16) & 0xFF;
                    if r < 80 {
                        dark += 1;
                    }
                }
            }
            assert!(ink > 100 * scale as usize, "too little ink: {ink}");
            let ratio = dark as f32 / ink as f32;
            assert!(
                ratio > 0.5,
                "strokes look washed out at scale {scale}: {dark}/{ink} dark"
            );
        }
    }

    /// The fringe must be gone: the lightest text pixels are still clearly
    /// darker than the background (no 1-off faint halos).
    #[test]
    fn fringe_is_removed() {
        let (buf, _, _) = render("Scooter", 1);
        let faint = buf
            .iter()
            .filter(|p| **p != 0xFFFFFF && (**p >> 16) & 0xFF > 170)
            .count();
        let ink = buf.iter().filter(|p| **p != 0xFFFFFF).count();
        assert!(
            (faint as f32 / ink as f32) < 0.3,
            "too much faint fringe: {faint} of {ink}"
        );
    }
}
