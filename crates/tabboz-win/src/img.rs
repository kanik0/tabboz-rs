// Pure-Rust decoders for the original game images:
// Windows BMP (1/4/8/24/32 bpp, uncompressed or RLE8) and ICO icons.
//
// These are the only image formats the game resources use; a general
// image library would be dead weight here.

use crate::pixel::Image;

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn i32le(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn rgba(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Palette entry: BGRX -> RGBA.
fn palette(d: &[u8], o: usize) -> u32 {
    rgba(d[o + 2], d[o + 1], d[o])
}

/// Decodes a Windows BMP into an RGBA image.
pub fn decode_bmp(d: &[u8]) -> Result<Image, String> {
    if d.len() < 54 || &d[0..2] != b"BM" {
        return Err("not a BMP".to_string());
    }
    let data_offset = u32le(d, 10) as usize;
    let dib = u32le(d, 14) as usize;
    if dib < 40 {
        return Err(format!("unsupported DIB header ({dib} bytes)"));
    }
    let w = i32le(d, 18);
    let raw_h = i32le(d, 22);
    let top_down = raw_h < 0;
    let h = raw_h.unsigned_abs() as i32;
    let bpp = u16le(d, 28) as usize;
    let compression = u32le(d, 30);
    let clr_used = u32le(d, 46) as usize;

    if compression != 0 && !(compression == 1 && bpp == 8) {
        return Err(format!("unsupported BMP compression {compression} (bpp {bpp})"));
    }

    // Palette for indexed formats.
    let mut pal: Vec<u32> = Vec::new();
    if bpp <= 8 {
        let n = if clr_used > 0 { clr_used } else { 1 << bpp };
        let base = 14 + dib;
        for i in 0..n {
            pal.push(palette(d, base + i * 4));
        }
    }

    let mut data = vec![0u32; (w * h) as usize];

    let put = |data: &mut Vec<u32>, x: i32, y: i32, c: u32| {
        let yy = if top_down { y } else { h - 1 - y };
        data[(yy * w + x) as usize] = c;
    };

    match bpp {
        1 | 4 | 8 if compression == 0 => {
            let row_bytes = ((w as usize * bpp + 31) / 32) * 4;
            for y in 0..h {
                let row = data_offset + y as usize * row_bytes;
                for x in 0..w {
                    let (byte, shift, mask) = match bpp {
                        1 => (d[row + x as usize / 8], 7 - (x % 8), 1),
                        4 => (d[row + x as usize / 2], 4 * (1 - x % 2), 0xF),
                        _ => (d[row + x as usize], 0, 0xFF),
                    };
                    let idx = ((byte >> shift) & mask) as usize;
                    put(&mut data, x, y, pal[idx.min(pal.len() - 1)]);
                }
            }
        }
        8 if compression == 1 => {
            // RLE8: <count,value> runs; escapes 0x00 00/01/02.
            let mut pos = data_offset;
            let (mut x, mut y) = (0i32, 0i32);
            while pos + 1 < d.len() {
                let count = d[pos] as usize;
                let value = d[pos + 1];
                pos += 2;
                if count > 0 {
                    for _ in 0..count {
                        if x < w {
                            put(&mut data, x, y, pal[value as usize]);
                        }
                        x += 1;
                    }
                } else {
                    match value {
                        0 => { // end of line
                            x = 0;
                            y += 1;
                        }
                        1 => break, // end of bitmap
                        2 => { // delta
                            if pos + 1 >= d.len() {
                                break;
                            }
                            x += d[pos] as i32;
                            y += d[pos + 1] as i32;
                            pos += 2;
                        }
                        n => { // literal run
                            let n = n as i32;
                            for i in 0..n {
                                if x < w {
                                    put(&mut data, x, y, pal[d[pos + i as usize] as usize]);
                                }
                                x += 1;
                            }
                            pos += n as usize;
                            if n % 2 == 1 {
                                pos += 1; // word padding
                            }
                        }
                    }
                }
            }
        }
        24 => {
            let row_bytes = (w as usize * 3).div_ceil(4) * 4;
            for y in 0..h {
                let row = data_offset + y as usize * row_bytes;
                for x in 0..w {
                    let o = row + x as usize * 3;
                    put(&mut data, x, y, rgba(d[o + 2], d[o + 1], d[o]));
                }
            }
        }
        32 => {
            for y in 0..h {
                let row = data_offset + y as usize * (w as usize * 4);
                for x in 0..w {
                    let o = row + x as usize * 4;
                    put(&mut data, x, y, rgba(d[o + 2], d[o + 1], d[o]));
                }
            }
        }
        other => return Err(format!("unsupported BMP bpp {other}")),
    }

    Ok(Image { w, h, data })
}

/// Decodes the largest image of an ICO icon file into an RGBA image.
pub fn decode_ico(d: &[u8]) -> Result<Image, String> {
    if d.len() < 6 || u16le(d, 0) != 0 || u16le(d, 2) != 1 {
        return Err("not an ICO".to_string());
    }
    let count = u16le(d, 4) as usize;
    if count == 0 {
        return Err("empty ICO".to_string());
    }

    // Pick the largest image (the game icons have a single image each).
    let mut best: Option<(usize, u8, u8, u16)> = None; // (offset, w, h, bpp)
    for i in 0..count {
        let e = 6 + i * 16;
        let w = d[e];
        let h = d[e + 1];
        let bpp = u16le(d, e + 6);
        let off = u32le(d, e + 12);
        let area = w as u32 * h as u32;
        let better = match best {
            None => true,
            Some((_, bw, bh, _)) => area > bw as u32 * bh as u32,
        };
        if better {
            best = Some((off as usize, w, h, bpp));
        }
    }
    let (off, ico_w, ico_h, _declared_bpp) = best.unwrap();
    let w = if ico_w == 0 { 256i32 } else { ico_w as i32 };
    let h = if ico_h == 0 { 256i32 } else { ico_h as i32 };

    // XOR image: BITMAPINFOHEADER with height = 2 * real height.
    let dib = u32le(d, off) as usize;
    let xor_h = i32le(d, off + 8) / 2;
    let bpp = u16le(d, off + 14) as usize;

    let mut pal: Vec<u32> = Vec::new();
    if bpp <= 8 {
        let n = 1 << bpp;
        let base = off + dib;
        for i in 0..n {
            pal.push(palette(d, base + i * 4));
        }
    }

    let xor_off = off + dib + if bpp <= 8 { (1 << bpp) * 4 } else { 0 };
    let row_bytes = ((w as usize * bpp + 31) / 32) * 4;
    let and_row_bytes = ((w as usize + 31) / 32) * 4;
    let and_off = xor_off + row_bytes * xor_h as usize;

    let mut data = vec![0u32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (byte, shift, mask) = match bpp {
                1 => (d[xor_off + y as usize * row_bytes + x as usize / 8], 7 - (x % 8), 1),
                4 => (d[xor_off + y as usize * row_bytes + x as usize / 2], 4 * (1 - x % 2), 0xF),
                8 => (d[xor_off + y as usize * row_bytes + x as usize], 0, 0xFF),
                24 => {
                    let o = xor_off + y as usize * row_bytes + x as usize * 3;
                    data[((h - 1 - y) * w + x) as usize] = rgba(d[o + 2], d[o + 1], d[o]);
                    continue;
                }
                32 => {
                    let o = xor_off + y as usize * row_bytes + x as usize * 4;
                    let px = 0xFF00_0000
                        | (d[o + 2] as u32) << 16
                        | (d[o + 1] as u32) << 8
                        | d[o] as u32;
                    data[((h - 1 - y) * w + x) as usize] = px;
                    continue;
                }
                other => return Err(format!("unsupported ICO bpp {other}")),
            };
            let idx = ((byte >> shift) & mask) as usize;
            let abit = d[and_off + (h - 1 - y) as usize * and_row_bytes + x as usize / 8]
                >> (7 - x % 8)
                & 1;
            let c = if abit == 1 { 0 } else { pal[idx.min(pal.len() - 1)] };
            data[((h - 1 - y) * w + x) as usize] = c;
        }
    }

    Ok(Image { w, h, data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets;

    fn bmp(name: &str) -> Image {
        let a = assets::BITMAPS.iter().find(|a| a.name == name).expect("bitmap");
        decode_bmp(a.data).unwrap()
    }

    #[test]
    fn decodes_1bpp_tabboz_layer() {
        // 1201: shop preview bitmap, 71x96 1-bit.
        let img = bmp("1201");
        assert_eq!((img.w, img.h), (71, 96));
        assert!(img.data.iter().all(|p| (p >> 24) == 0xFF));
    }

    #[test]
    fn decodes_4bpp_testa_layer() {
        // 1390: testa (head) of the tabboz, 48x64 4-bit with palette.
        let img = bmp("1390");
        assert_eq!((img.w, img.h), (48, 64));
    }

    #[test]
    fn decodes_8bpp_and_rle8_layers() {
        // 1560: sfondo 8-bit uncompressed; 1520: pantaloni 8-bit RLE.
        let sfondo = bmp("1560");
        assert_eq!((sfondo.w, sfondo.h), (135, 275));
        let pantaloni = bmp("1520");
        assert_eq!((pantaloni.w, pantaloni.h), (84, 153));
        assert!(pantaloni.data.iter().any(|p| *p != pantaloni.data[0]));
    }

    #[test]
    fn decodes_game_icons() {
        for name in ["ZARROSIM", "SCOOTER", "REDBABY"] {
            let a = assets::ICONS.iter().find(|a| a.name == name).expect("icon");
            let img = decode_ico(a.data).unwrap();
            assert!(img.w == 32 && img.h == 32, "{name}: {}x{}", img.w, img.h);
        }
    }
}
