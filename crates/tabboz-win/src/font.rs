// Text rendering with the bundled MS Sans Serif fonts.
//
// The web port ships MS Sans Serif as WOFF; the build script converts it to
// TTF and embeds it (see build.rs). Glyphs are rasterized on demand and
// cached; positions are snapped to integer pixels so the pixel design of
// the font stays crisp, like the original BWCC dialogs.

use ab_glyph::{Font, FontArc, Glyph, PxScale, PxScaleFont, ScaleFont, point};
use std::collections::HashMap;

use crate::pixel::{Pixel, Surface};

pub struct Face {
    font: PxScaleFont<FontArc>,
    cache: HashMap<char, CachedGlyph>,
    pub size: f32,
    pub ascent: f32,
    pub descent: f32,
}

struct CachedGlyph {
    bitmap: Vec<u8>, // alpha coverage, w*h
    w: i32,
    h: i32,
    /// Offset of the bitmap top-left from the glyph origin (integers).
    x: i32,
    y: i32,
    /// Advance to the next character (float, snapped on use).
    advance: f32,
}

impl Face {
    pub fn new(data: &[u8], size: f32) -> Self {
        let font = FontArc::try_from_vec(data.to_vec()).expect("valid font");
        let scaled: PxScaleFont<FontArc> = font.into_scaled(PxScale::from(size));
        let ascent = scaled.ascent();
        let descent = scaled.descent();
        Face {
            font: scaled,
            cache: HashMap::new(),
            size,
            ascent,
            descent,
        }
    }

    /// Logical line height (matches the original dialog text rows).
    pub fn line_height(&self) -> i32 {
        (self.ascent - self.descent).ceil() as i32
    }

    fn glyph(&mut self, c: char) -> &CachedGlyph {
        let font = &self.font;
        self.cache.entry(c).or_insert_with(|| {
            let glyph_id = font.glyph_id(c);
            let advance = font.h_advance(glyph_id);
            // Snap the origin to integers for crisp rasterization.
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
                        bitmap[(y * w as u32 + x) as usize] = (coverage * 255.0) as u8;
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

    /// Width of a single-line string, in pixels.
    pub fn text_width(&mut self, s: &str) -> i32 {
        let mut x = 0.0f32;
        for c in s.chars() {
            x += self.glyph(c).advance;
        }
        x.ceil() as i32
    }

    /// Draws a single line of text at integer position (x = left, y = top).
    /// Returns the pen advance for the whole string.
    pub fn draw_text(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let baseline = y + self.ascent.round() as i32;
        let mut pen = x as f32;
        for c in s.chars() {
            let g = self.glyph(c);
            let gx = pen.round() as i32 + g.x;
            let gy = baseline + g.y;
            for yy in 0..g.h {
                for xx in 0..g.w {
                    let a = g.bitmap[(yy * g.w + xx) as usize];
                    if a > 0 {
                        surf.blend(gx + xx, gy + yy, color, a);
                    }
                }
            }
            pen += g.advance;
        }
    }

    /// Draws text underlining the character after '&' (Windows accelerators).
    /// Returns the visible string (with '&' removed) for width measuring.
    pub fn draw_accel_text(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let mut pen = x as f32;
        let baseline = y + self.ascent.round() as i32;
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
            let g = self.glyph(c);
            let gx = pen.round() as i32 + g.x;
            let gy = baseline + g.y;
            for yy in 0..g.h {
                for xx in 0..g.w {
                    let a = g.bitmap[(yy * g.w + xx) as usize];
                    if a > 0 {
                        surf.blend(gx + xx, gy + yy, color, a);
                    }
                }
            }
            if underline_next {
                let uw = g.advance.ceil() as i32;
                surf.hline(pen.round() as i32, baseline + 1, uw.max(1), color);
                underline_next = false;
            }
            pen += g.advance;
        }
    }
}

/// The two faces used by the game UI.
pub struct Fonts {
    pub regular: Face,
    pub bold: Face,
}

pub const TEXT_SIZE: f32 = 11.0;

impl Fonts {
    pub fn new() -> Self {
        Fonts {
            regular: Face::new(crate::fonts::FONT_REGULAR, TEXT_SIZE),
            bold: Face::new(crate::fonts::FONT_BOLD, TEXT_SIZE),
        }
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_load_and_measure() {
        let mut f = Fonts::new();
        let w = f.regular.text_width("Tabboz Simulator");
        assert!(w > 50, "title text suspiciously narrow: {w}");
        // ASCII-art sanity check: the caption must produce ink.
        let mut buf = vec![0xFFFFFFu32; w as usize * 20];
        let mut s = Surface::new(&mut buf, w, 20);
        f.regular.draw_text(&mut s, "Tabboz Simulator", 0, 3, 0x000000);
        assert!(buf.iter().any(|p| *p != 0xFFFFFF), "no pixels drawn");
    }

    #[test]
    fn accel_text_underlines_hotkey() {
        let mut f = Fonts::new();
        let mut buf = vec![0xFFFFFFu32; 100 * 20];
        let mut s = Surface::new(&mut buf, 100, 20);
        f.regular.draw_accel_text(&mut s, "&File", 0, 3, 0x000000);
        // The underline sits below the ascent line and to the left.
        assert!(buf.iter().any(|p| *p != 0xFFFFFF));
    }

    #[test]
    fn text_is_pixel_crisp() {
        // Pixel fonts render with (almost) binary coverage; if this test
        // fails the font is being scaled off its design grid.
        let mut f = Fonts::new();
        let w = f.regular.text_width("Text");
        let mut buf = vec![0xFFFFFFu32; w as usize * 20];
        let mut s = Surface::new(&mut buf, w, 20);
        f.regular.draw_text(&mut s, "Text", 0, 3, 0x000000);
        let dark = buf.iter().filter(|p| **p != 0xFFFFFF).count();
        assert!(dark > 20, "font rendered too light: {dark} ink pixels");
    }
}
