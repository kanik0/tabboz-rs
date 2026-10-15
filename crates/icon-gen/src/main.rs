// icon-gen: converts the original ZARROSIM.ico into a macOS iconset
// (PNGs at every required size, hand-rolled PNG encoder, no dependencies).
//
// Build tool for bundle.sh; iconutil then packs the iconset into an .icns.

use std::env;
use std::fs;
use std::path::Path;

use tabboz_win::img::decode_ico;
use tabboz_win::pixel::Image;

fn main() {
    let args: Vec<String> = env::args().collect();
    let out_dir = args.get(1).expect("usage: icon-gen <iconset-dir>");
    let out_dir = Path::new(out_dir);
    fs::create_dir_all(out_dir).expect("create iconset dir");

    let asset = tabboz_win::assets::ICONS
        .iter()
        .find(|a| a.name == "ZARROSIM")
        .expect("ZARROSIM.ico");
    let ico = decode_ico(asset.data).expect("decode icon");

    // iconutil set: 16, 16@2x (32), 32, 32@2x (64), 128, 128@2x (256),
    // 256, 256@2x (512), 512, 512@2x (1024).
    for size in [16, 32, 64, 128, 256, 512] {
        let scaled = scale_nearest(&ico, size, size);
        let name = format!("icon_{size}x{size}.png");
        let path = out_dir.join(&name);
        fs::write(&path, encode_png(&scaled)).expect("write png");
        println!("wrote {}", path.display());
    }
}

/// Nearest-neighbor scale (pixel-art faithful, like the original icon).
fn scale_nearest(src: &Image, w: i32, h: i32) -> Image {
    let mut out = Image { w, h, data: vec![0; (w * h) as usize] };
    for y in 0..h {
        for x in 0..w {
            let sx = src.w as i64 * x as i64 / w as i64;
            let sy = src.h as i64 * y as i64 / h as i64;
            out.data[(y * w + x) as usize] = src.data[(sy as i32 * src.w + sx as i32) as usize];
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Minimal PNG encoder: 8-bit RGBA, filter 0, stored (uncompressed) deflate.
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, e) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// zlib stream with stored (uncompressed) deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header: deflate, no compression
    let mut chunks = data.chunks(65535).peekable();
    if data.is_empty() {
        out.push(0x01);
        out.extend_from_slice(&[0, 0, 0, 0]);
    }
    while let Some(block) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(if last { 1 } else { 0 });
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

pub fn encode_png(img: &Image) -> Vec<u8> {
    let mut raw = Vec::with_capacity((img.w * img.h * 5) as usize);
    for y in 0..img.h {
        raw.push(0); // filter type 0 (None)
        for x in 0..img.w {
            let px = img.data[(y * img.w + x) as usize];
            raw.push(((px >> 16) & 0xFF) as u8); // R
            raw.push(((px >> 8) & 0xFF) as u8); // G
            raw.push((px & 0xFF) as u8); // B
            raw.push(((px >> 24) & 0xFF) as u8); // A
        }
    }

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR: width, height, 8bpp, color type 6 (RGBA), no interlace.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(img.w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(img.h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);

    chunk(&mut png, b"IDAT", &zlib_stored(&raw));
    chunk(&mut png, b"IEND", &[]);
    png
}
