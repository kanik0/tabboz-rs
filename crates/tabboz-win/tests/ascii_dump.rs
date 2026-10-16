// Manual visual check: renders the main window and prints it as ASCII art.
// Run with: cargo test -p tabboz-win --test ascii_dump -- --ignored --nocapture

use tabboz_win::font::Fonts;
use tabboz_win::pixel::Surface;
use tabboz_win::render::Frame;
use tabboz_win::theme::*;

#[test]
#[ignore = "manual visual check"]
fn dump_main_frame_ascii() {
    let mut frame = Frame::new(1);
    let mut fonts = Fonts::new(1);
    let (w, h) = frame.window_size();
    let mut buf = vec![0u32; (w * h) as usize];
    {
        let mut s = Surface::new(&mut buf, w, h, 1);
        frame.render(&mut s, &mut fonts);
    }

    let ch = |p: u32| -> char {
        match p {
            NAVY => '#',
            FACE => '.',
            WHITE => ' ',
            BLACK | DKSHADOW => '@',
            SHADOW => 's',
            INNER_LIGHT => 'l',
            0x00FF_8000..=0x00FF_FFFF => 'c', // colored image pixels
            0x0080_0000..=0x00FF_7FFF => 'c',
            _ => 'o',
        }
    };

    // Downsample 4x vertically, 2x horizontally to fit the terminal.
    for y in (0..h).step_by(4) {
        let mut line = String::with_capacity(w as usize / 2);
        for x in (0..w).step_by(2) {
            let p = buf[(y * w + x) as usize];
            line.push(ch(p));
        }
        println!("{line}");
    }
}
