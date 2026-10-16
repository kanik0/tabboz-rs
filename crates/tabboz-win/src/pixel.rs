// Pixel surface and drawing primitives.
//
// The engine works in LOGICAL coordinates (1 dialog unit = 2 logical px,
// like the original Borland dialogs). The Surface owns an integer scale
// factor (1 on standard displays, 2 on retina) and renders every logical
// primitive directly at PHYSICAL resolution: 1 logical pixel becomes a
// scale x scale block (crisp Win95 borders), images blit nearest-neighbor,
// and text is rasterized at size * scale on the physical pixel grid.

/// A 0x00RRGGBB pixel.
pub type Pixel = u32;

/// RGBA image (0xAARRGGBB), produced by the BMP/ICO decoders.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: i32,
    pub h: i32,
    /// 0xAARRGGBB, row-major, alpha 0 = fully transparent.
    pub data: Vec<u32>,
}

impl Image {
    /// Nearest-neighbor 2x downscale (32px title icons -> 16px).
    pub fn half(&self) -> Image {
        let w = (self.w + 1) / 2;
        let h = (self.h + 1) / 2;
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push(self.data[(2 * y * self.w + 2 * x) as usize]);
            }
        }
        Image { w, h, data }
    }
}

/// A rectangular drawing target.
///
/// All coordinates in the public API are LOGICAL pixels; the surface maps
/// them onto its physical buffer at the given integer scale. The only
/// exceptions are the physical-pixel methods used by the font rasterizer.
pub struct Surface<'a> {
    /// Integer upscale factor (1 or 2).
    pub scale: i32,
    /// Logical size (what the engine sees).
    pub lw: i32,
    pub lh: i32,
    buf: &'a mut [Pixel],
}

impl<'a> Surface<'a> {
    pub fn new(buf: &'a mut [Pixel], lw: i32, lh: i32, scale: i32) -> Self {
        assert!(scale >= 1, "scale must be >= 1");
        let pw = lw * scale;
        let ph = lh * scale;
        assert_eq!(buf.len(), (pw * ph) as usize, "surface buffer size mismatch");
        Surface { scale, lw, lh, buf }
    }

    /// Physical pixel size of the buffer.
    pub fn physical_size(&self) -> (i32, i32) {
        (self.lw * self.scale, self.lh * self.scale)
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Pixel {
        let (px, py) = (x * self.scale, y * self.scale);
        self.buf[(py * self.lw * self.scale + px) as usize]
    }

    /// Sets one LOGICAL pixel (a scale x scale physical block).
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: Pixel) {
        let s = self.scale;
        self.rect_phys(x * s, y * s, s, s, c);
    }

    /// Sets one PHYSICAL pixel (font rasterizer only).
    #[inline]
    pub fn set_phys(&mut self, x: i32, y: i32, c: Pixel) {
        if x >= 0 && y >= 0 && x < self.lw * self.scale && y < self.lh * self.scale {
            self.buf[(y * self.lw * self.scale + x) as usize] = c;
        }
    }

    pub fn fill(&mut self, c: Pixel) {
        self.buf.fill(c);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Pixel) {
        let s = self.scale;
        self.rect_phys(x * s, y * s, w * s, h * s, c);
    }

    /// Raw physical-coordinate rectangle (font stamp and present only).
    pub fn rect_phys(&mut self, x: i32, y: i32, w: i32, h: i32, c: Pixel) {
        let (pw, ph) = self.physical_size();
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = ((x + w).min(pw), (y + h).min(ph));
        for yy in y0..y1.max(y0) {
            let row = (yy * pw) as usize;
            for xx in x0..x1.max(x0) {
                self.buf[row + xx as usize] = c;
            }
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: Pixel) {
        self.rect(x, y, w, 1, c);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, c: Pixel) {
        self.rect(x, y, 1, h, c);
    }

    /// 1px frame: one color on top/left, another on bottom/right.
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, tl: Pixel, br: Pixel) {
        self.hline(x, y, w, tl);
        self.vline(x, y, h, tl);
        self.hline(x, y + h - 1, w, br);
        self.vline(x + w - 1, y, h, br);
    }

    /// Blits an image nearest-neighbor scaled by the surface scale;
    /// pixels with alpha 0 are skipped (transparent).
    pub fn blit(&mut self, img: &Image, x: i32, y: i32) {
        let s = self.scale;
        let (pw, ph) = self.physical_size();
        for yy in 0..img.h * s {
            for xx in 0..img.w * s {
                let px = img.data[(yy / s * img.w + xx / s) as usize];
                if px >> 24 != 0 {
                    let (dx, dy) = (x * s + xx, y * s + yy);
                    if dx >= 0 && dy >= 0 && dx < pw && dy < ph {
                        self.buf[(dy * pw + dx) as usize] = px & 0x00FF_FFFF;
                    }
                }
            }
        }
    }

    /// Blits an image clipped to a logical rectangle.
    /// The clip rect mirrors the shape of the Win32 bitblt call.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_clip(&mut self, img: &Image, x: i32, y: i32, cx: i32, cy: i32, cw: i32, ch: i32) {
        let s = self.scale;
        let (pw, ph) = self.physical_size();
        for yy in 0..img.h * s {
            for xx in 0..img.w * s {
                let px = img.data[(yy / s * img.w + xx / s) as usize];
                if px >> 24 == 0 {
                    continue;
                }
                let (lx, ly) = (x + xx / s, y + yy / s);
                if !(lx >= cx && ly >= cy && lx < cx + cw && ly < cy + ch) {
                    continue;
                }
                let (dx, dy) = (x * s + xx, y * s + yy);
                if dx >= 0 && dy >= 0 && dx < pw && dy < ph {
                    self.buf[(dy * pw + dx) as usize] = px & 0x00FF_FFFF;
                }
            }
        }
    }

    /// Alpha-blends a PHYSICAL pixel over the current content (a = 0..255).
    #[inline]
    pub fn blend_phys(&mut self, x: i32, y: i32, color: Pixel, a: u8) {
        let (pw, ph) = self.physical_size();
        if x < 0 || y < 0 || x >= pw || y >= ph {
            return;
        }
        let i = (y * pw + x) as usize;
        if a == 255 {
            self.buf[i] = color;
            return;
        }
        let dst = self.buf[i];
        let (dr, dg, db) = ((dst >> 16) & 0xFF, (dst >> 8) & 0xFF, dst & 0xFF);
        let (sr, sg, sb) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
        let f = a as u32;
        let r = (dr * (255 - f) + sr * f) / 255;
        let g = (dg * (255 - f) + sg * f) / 255;
        let b = (db * (255 - f) + sb * f) / 255;
        self.buf[i] = r << 16 | g << 8 | b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_primitives() {
        let mut buf = vec![0u32; 16 * 8];
        let mut s = Surface::new(&mut buf, 16, 8, 1);
        s.rect(0, 0, 16, 8, 1);
        s.set(20, 20, 2); // out of bounds: ignored
        assert_eq!(s.get(0, 0), 1);
        s.frame(0, 0, 16, 8, 2, 3);
        assert_eq!(s.get(0, 0), 2);
        assert_eq!(s.get(15, 7), 3);
        assert_eq!(s.get(7, 6), 1, "interior keeps the fill");
    }

    #[test]
    fn scaled_surface_makes_blocks() {
        // 1 logical pixel = 2x2 physical at scale 2.
        let mut buf = vec![0u32; 8 * 4];
        let mut s = Surface::new(&mut buf, 4, 2, 2);
        assert_eq!(s.physical_size(), (8, 4));
        s.set(1, 0, 9);
        for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
            assert_eq!(s.buf[(y * 8 + x) as usize], 9, "block pixel {x},{y}");
        }
        assert_eq!(s.buf[0], 0);
    }

    #[test]
    fn blit_respects_transparency() {
        let img = Image { w: 2, h: 1, data: vec![0xFF00_0000, 0x0000_0000] };
        let mut buf = vec![9u32; 2];
        let mut s = Surface::new(&mut buf, 2, 1, 1);
        s.blit(&img, 0, 0);
        assert_eq!(s.get(0, 0), 0);
        assert_eq!(s.get(1, 0), 9);
    }
}
