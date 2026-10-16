// Text rendering with the bundled MS Sans Serif fonts.
//
// The web port ships MS Sans Serif as WOFF; the build script converts it to
// TTF and embeds it (see build.rs). Glyphs are rasterized on demand and
// cached at PHYSICAL resolution (size * scale): on retina the text is
// rendered at 22px on the 2x pixel grid instead of being upscaled from
// 11px, which keeps every stroke crisp. All public methods take LOGICAL
// coordinates and convert with the face scale.

use ab_glyph::{Font, FontArc, Glyph, PxScale, PxScaleFont, ScaleFont, point};
use std::collections::HashMap;

use crate::pixel::{Pixel, Surface};

/// Logical text size of the original dialogs (MS Sans Serif 10pt).
pub const TEXT_SIZE: f32 = 11.0;

pub struct Face {
    font: PxScaleFont<FontArc>,
    cache: HashMap<char, CachedGlyph>,
    /// Integer upscale factor; glyph bitmaps are rasterized at size * scale.
    scale: i32,
    /// Scaled ascent/descent (physical pixels).
    ascent: f32,
    descent: f32,
}

struct CachedGlyph {
    bitmap: Vec<u8>, // alpha coverage, w*h (physical)
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
        let font = FontArc::try_from_vec(data.to_vec()).expect("valid font");
        let size = TEXT_SIZE * scale as f32;
        let scaled: PxScaleFont<FontArc> = font.into_scaled(PxScale::from(size));
        let ascent = scaled.ascent();
        let descent = scaled.descent();
        Face {
            font: scaled,
            cache: HashMap::new(),
            scale,
            ascent,
            descent,
        }
    }

    /// Logical line height.
    pub fn line_height(&self) -> i32 {
        ((self.ascent - self.descent) / self.scale as f32).ceil() as i32
    }

    fn glyph(&mut self, c: char) -> &CachedGlyph {
        let font = &self.font;
        let scale = self.scale;
        self.cache.entry(c).or_insert_with(|| {
            let glyph_id = font.glyph_id(c);
            let advance = font.h_advance(glyph_id);
            // Snap the origin to the physical pixel grid for crisp strokes.
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
                // Binary coverage: MS Sans Serif is a bitmap-styled font and
                // the original rendered it with no antialiasing at all.
                outlined.draw(|x, y, coverage| {
                    if x < w as u32 && y < h as u32 {
                        bitmap[(y * w as u32 + x) as usize] = if coverage >= 0.5 { 255 } else { 0 };
                    }
                });
                // Snap the glyph origin to multiples of the scale so the
                // pixel grid of the font matches the physical grid.
                let snap = |v: f32| -> i32 { ((v / scale as f32).round() * scale as f32) as i32 };
                CachedGlyph {
                    bitmap,
                    w,
                    h,
                    x: snap(bb.min.x),
                    y: snap(bb.min.y),
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

    /// Draws a single line at a LOGICAL (x = left, y = top) position.
    fn draw_line(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let scale = self.scale;
        let baseline = y * scale + self.ascent.round() as i32;
        // Snap the pen to multiples of the scale so every glyph lands on
        // the physical grid exactly like at 1x.
        let mut pen = x * scale;
        for c in s.chars() {
            let g = self.glyph(c);
            let gx = pen + g.x;
            let gy = baseline + g.y;
            for yy in 0..g.h {
                for xx in 0..g.w {
                    let a = g.bitmap[(yy * g.w + xx) as usize];
                    if a > 0 {
                        surf.blend_phys(gx + xx, gy + yy, color, a);
                    }
                }
            }
            pen = ((pen as f32 + g.advance) / scale as f32).round() as i32 * scale;
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
            let g = self.glyph(c);
            let gx = pen + g.x;
            let gy = baseline + g.y;
            for yy in 0..g.h {
                for xx in 0..g.w {
                    let a = g.bitmap[(yy * g.w + xx) as usize];
                    if a > 0 {
                        surf.blend_phys(gx + xx, gy + yy, color, a);
                    }
                }
            }
            if underline_next {
                let uw = g.advance.round() as i32;
                let uy = baseline + 1;
                for u in 0..uw.max(1) {
                    surf.blend_phys(pen + u, uy, color, 255);
                }
                underline_next = false;
            }
            pen = ((pen as f32 + g.advance) / scale as f32).round() as i32 * scale;
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
            bold: Face::new(crate::fonts::FONT_BOLD, scale),
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

    #[test]
    fn fonts_load_and_measure() {
        let mut f = Fonts::new(1);
        let w = f.regular.text_width("Tabboz Simulator");
        assert!(w > 50, "title text suspiciously narrow: {w}");
        let mut buf = vec![0xFFFFFFu32; w as usize * 20];
        let mut s = Surface::new(&mut buf, w, 20, 1);
        f.regular.draw_text(&mut s, "Tabboz Simulator", 0, 3, 0x000000);
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

    /// The M7 crispness contract: at scale 2 the text must be rasterized on
    /// the physical grid, so the ink of every glyph is made of full-strength
    /// pixels (a 2x-designed font at 2x renders with near-binary coverage).
    #[test]
    fn text_at_scale_2_is_crisp() {
        let mut f = Fonts::new(2);
        let w = f.regular.text_width("Testo nitido");
        let mut buf = vec![0xFFFFFFu32; (w as usize * 2) * 40];
        let mut s = Surface::new(&mut buf, w, 20, 2);
        f.regular.draw_text(&mut s, "Testo nitido", 0, 3, 0x000000);

        let mut ink = 0usize;
        let mut partial = 0usize;
        for p in buf.iter() {
            if *p != 0xFFFFFF {
                ink += 1;
                let (r, g, b) = ((*p >> 16) & 0xFF, (*p >> 8) & 0xFF, *p & 0xFF);
                if r > 40 && r < 215 && g > 40 && g < 215 && b > 40 && b < 215 {
                    partial += 1;
                }
            }
        }
        assert!(ink > 100, "too little ink: {ink}");
        let partial_ratio = partial as f32 / ink as f32;
        assert!(
            partial_ratio < 0.25,
            "text looks antialiased/blurry at scale 2: {partial} of {ink} pixels are partial"
        );
    }
}
