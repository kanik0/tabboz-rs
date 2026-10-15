// Pixel surface and drawing primitives.
//
// Pixels are u32 in 0x00RRGGBB form, matching what softbuffer expects
// on little-endian targets. All engine coordinates are logical pixels
// (1 dialog unit = 2 logical pixels, as in the original Borland dialogs
// rendered by the Novantotto web port).

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
pub struct Surface<'a> {
    pub w: i32,
    pub h: i32,
    buf: &'a mut [Pixel],
}

impl<'a> Surface<'a> {
    pub fn new(buf: &'a mut [Pixel], w: i32, h: i32) -> Self {
        assert_eq!(buf.len(), (w * h) as usize, "surface buffer size mismatch");
        Surface { w, h, buf }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Pixel {
        self.buf[(y * self.w + x) as usize]
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: Pixel) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.buf[(y * self.w + x) as usize] = c;
        }
    }

    /// Alpha-blends a pixel over the current content (a = 0..255).
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, color: Pixel, a: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = (y * self.w + x) as usize;
        if a >= 255 {
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

    pub fn fill(&mut self, c: Pixel) {
        self.buf.fill(c);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Pixel) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: Pixel) {
        for xx in x..x + w {
            self.set(xx, y, c);
        }
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, c: Pixel) {
        for yy in y..y + h {
            self.set(x, yy, c);
        }
    }

    /// 1px frame: one color on top/left, another on bottom/right.
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, tl: Pixel, br: Pixel) {
        self.hline(x, y, w, tl);
        self.vline(x, y, h, tl);
        self.hline(x, y + h - 1, w, br);
        self.vline(x + w - 1, y, h, br);
    }

    /// Blits an image; pixels with alpha 0 are skipped (transparent).
    pub fn blit(&mut self, img: &Image, x: i32, y: i32) {
        for yy in 0..img.h {
            for xx in 0..img.w {
                let px = img.data[(yy * img.w + xx) as usize];
                if px >> 24 != 0 {
                    self.set(x + xx, y + yy, px & 0x00FF_FFFF);
                }
            }
        }
    }

    /// Blits an image clipped to a rectangle (in surface coordinates).
    #[allow(clippy::too_many_arguments)]
    pub fn blit_clip(&mut self, img: &Image, x: i32, y: i32, cx: i32, cy: i32, cw: i32, ch: i32) {
        for yy in 0..img.h {
            for xx in 0..img.w {
                let px = img.data[(yy * img.w + xx) as usize];
                if px >> 24 == 0 {
                    continue;
                }
                let (sx, sy) = (x + xx, y + yy);
                if sx >= cx && sy >= cy && sx < cx + cw && sy < cy + ch {
                    self.set(sx, sy, px & 0x00FF_FFFF);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_primitives() {
        let mut buf = vec![0u32; 8 * 4];
        let mut s = Surface::new(&mut buf, 8, 4);
        s.rect(0, 0, 8, 4, 1);
        s.set(9, 9, 2); // out of bounds: ignored
        assert_eq!(s.get(0, 0), 1);
        s.frame(0, 0, 8, 4, 2, 3);
        assert_eq!(s.get(0, 0), 2);
        assert_eq!(s.get(7, 3), 3);
        assert_eq!(s.get(3, 2), 1, "interior keeps the fill");
    }

    #[test]
    fn blit_respects_transparency() {
        let img = Image {
            w: 2,
            h: 1,
            data: vec![0xFF00_0000, 0x0000_0000],
        };
        let mut buf = vec![9u32; 2];
        let mut s = Surface::new(&mut buf, 2, 1);
        s.blit(&img, 0, 0);
        assert_eq!(s.get(0, 0), 0);
        assert_eq!(s.get(1, 0), 9);
    }
}
