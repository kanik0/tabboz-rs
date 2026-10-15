// Tabboz Simulator - Win32/BWCC dialog engine.
//
// - dialogs/assets/fonts/menus/strings: data code-generated at build time
//   from the original game resources (see build.rs)
// - pixel/img/font: pure-Rust rendering primitives and asset decoders
// - theme/render: the BWCC look and the dialog widget engine

pub mod dialogs {
    include!(concat!(env!("OUT_DIR"), "/dialogs.rs"));
}

pub mod assets {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

pub mod fonts {
    include!(concat!(env!("OUT_DIR"), "/fonts.rs"));
}

pub mod menus {
    include!(concat!(env!("OUT_DIR"), "/menus.rs"));
}

pub mod strings {
    include!(concat!(env!("OUT_DIR"), "/strings.rs"));
}

pub mod font;
pub mod img;
pub mod pixel;
pub mod render;
pub mod theme;
