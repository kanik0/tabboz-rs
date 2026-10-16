// Text rendering with the bundled MS Sans Serif fonts.
//
// The web port ships MS Sans Serif as WOFF; the build script converts it to
// TTF and embeds it (see build.rs). The font is designed on an 11px pixel
// grid, so every glyph is rasterized at TEXT_SIZE with BINARY coverage
// (no antialiasing, exactly like the bitmap font of the original) and then
// each ink pixel is expanded into a scale x scale physical block. This is
// pixel-perfect at any scale: the same hard edges the original Win95 had.

use ab_glyph::{Font, FontArc, Glyph, PxScale, PxScaleFont, ScaleFont, point};
use std::collections::HashMap;

use crate::pixel::{Pixel, Surface};

/// Logical text size of the original dialogs (MS Sans Serif 10pt @96dpi).
pub const TEXT_SIZE: f32 = 11.0;

pub struct Face {
    font: PxScaleFont<FontArc>,
    cache: HashMap<char, CachedGlyph>,
    /// Integer upscale factor applied to the rasterized glyphs.
    scale: i32,
    /// Ascent/descent at the design size (logical pixels).
    ascent: f32,
    descent: f32,
}

#[derive(Clone)]
struct CachedGlyph {
    /// Binary coverage (0 or 255) on the 11px design grid.
    bitmap: Vec<u8>,
    w: i32,
    h: i32,
    /// Offset of the bitmap top-left from the glyph origin (design ints).
    x: i32,
    y: i32,
    /// Advance to the next character (design pixels).
    advance: f32,
}

impl Face {
    pub fn new(data: &[u8], scale: i32) -> Self {
        let font = FontArc::try_from_vec(data.to_vec()).expect("valid font");
        // Rasterize at the DESIGN size: the font is pixel-perfect there.
        let scaled: PxScaleFont<FontArc> = font.into_scaled(PxScale::from(TEXT_SIZE));
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
        (self.ascent - self.descent).ceil() as i32
    }

    fn glyph(&mut self, c: char) -> &CachedGlyph {
        let font = &self.font;
        self.cache.entry(c).or_insert_with(|| {
            let glyph_id = font.glyph_id(c);
            let advance = font.h_advance(glyph_id);
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
                // Binary coverage on the design grid: MS Sans Serif is a
                // bitmap-styled font and Win95 rendered it without any
                // antialiasing.
                outlined.draw(|x, y, coverage| {
                    if x < w as u32 && y < h as u32 {
                        bitmap[(y * w as u32 + x) as usize] = if coverage >= 0.5 { 255 } else { 0 };
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
        x.ceil() as i32
    }

    /// Draws one glyph's ink, expanding every design pixel to a
    /// scale x scale physical block (pixel-perfect upscaling).
    fn stamp(&self, surf: &mut Surface, g: &CachedGlyph, gx: i32, gy: i32, color: Pixel) {
        let s = self.scale;
        for yy in 0..g.h {
            for xx in 0..g.w {
                if g.bitmap[(yy * g.w + xx) as usize] > 0 {
                    surf.rect_phys((gx + xx) * s, (gy + yy) * s, s, s, color);
                }
            }
        }
    }

    /// Draws a single line at a LOGICAL (x = left, y = top) position.
    fn draw_line(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let baseline = y + self.ascent.round() as i32;
        let mut pen = x;
        for c in s.chars() {
            let g = self.glyph(c).clone();
            self.stamp(surf, &g, pen + g.x, baseline + g.y, color);
            // Snap the pen to the design grid: no subpixel accumulation.
            pen = (pen as f32 + g.advance).round() as i32;
        }
    }

    pub fn draw_text(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        self.draw_line(surf, s, x, y, color);
    }

    /// Draws text underlining the character after '&' (Windows accelerators).
    pub fn draw_accel_text(&mut self, surf: &mut Surface, s: &str, x: i32, y: i32, color: Pixel) {
        let baseline = y + self.ascent.round() as i32;
        let mut pen = x;
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
            let g = self.glyph(c).clone();
            self.stamp(surf, &g, pen + g.x, baseline + g.y, color);
            if underline_next {
                let uw = g.advance.round() as i32;
                surf.rect(pen, baseline + 1, uw.max(1), 1, color);
                underline_next = false;
            }
            pen = (pen as f32 + g.advance).round() as i32;
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

    /// Crispness contract: glyphs are rasterized with binary coverage on
    /// the design grid and upscaled in exact blocks, so the ink contains
    /// NO partial (antialiased) pixels at any scale.
    #[test]
    fn text_is_pixel_perfect_at_scale_2() {
        for scale in [1, 2] {
            let mut f = Fonts::new(scale);
            let w = f.regular.text_width("Testo nitido");
            let mut buf = vec![0xFFFFFFu32; (w as usize * scale as usize) * (20 * scale as usize)];
            let mut s = Surface::new(&mut buf, w, 20, scale);
            f.regular.draw_text(&mut s, "Testo nitido", 0, 3, 0x000000);

            let mut ink = 0usize;
            let mut partial = 0usize;
            for p in buf.iter() {
                if *p != 0xFFFFFF {
                    ink += 1;
                    // Any value that is neither the exact text color nor
                    // the exact background is an antialiasing artifact.
                    if *p != 0x000000 {
                        partial += 1;
                    }
                }
            }
            assert!(ink > 40 * (scale as usize * scale as usize), "too little ink at scale {scale}: {ink}");
            assert_eq!(
                partial, 0,
                "antialiased pixels found at scale {scale}: {partial} of {ink}"
            );
        }
    }

    /// The scale-2 rendering must contain every scale-1 ink pixel expanded
    /// to a 2x2 block: same shape, pixel-doubled.
    #[test]
    fn scale_2_is_exact_2x_of_scale_1() {
        let mut f1 = Fonts::new(1);
        let mut f2 = Fonts::new(2);
        let text = "Tabboz";
        let w = f1.regular.text_width(text);
        let mut buf1 = vec![0xFFFFFFu32; (w as usize) * 20];
        let mut s1 = Surface::new(&mut buf1, w, 20, 1);
        f1.regular.draw_text(&mut s1, text, 0, 3, 0x000000);
        let mut buf2 = vec![0xFFFFFFu32; (w as usize * 2) * 40];
        let mut s2 = Surface::new(&mut buf2, w, 20, 2);
        f2.regular.draw_text(&mut s2, text, 0, 3, 0x000000);

        for y in 0..20 {
            for x in 0..w {
                let expected = buf1[(y * w + x) as usize];
                for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                    let got = buf2[((2 * y + dy) * 2 * w + 2 * x + dx) as usize];
                    assert_eq!(
                        got, expected,
                        "pixel ({x},{y}) not doubled correctly at scale 2"
                    );
                }
            }
        }
    }
}
