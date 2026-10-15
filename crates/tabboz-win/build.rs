// Build script for tabboz-win.
//
// Parses resources/dialogs/dialogs.rc (the source of truth for all 81 game
// dialogs, already in machine-readable form thanks to the Novantotto export)
// and generates typed Rust code, plus embeds every game asset
// (bitmaps, icons, sounds, string tables, menus) via include_bytes!.
//
// Zero external dependencies: the .rc grammar used by this file is tiny and
// fully covered (only CONTROL statements are present in the file).

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Win32 style constants (subset actually used by dialogs.rc)
// ---------------------------------------------------------------------------

fn style_const(name: &str) -> Option<u32> {
    Some(match name {
        // window styles
        "WS_POPUP" => 0x8000_0000,
        "WS_CHILD" => 0x4000_0000,
        "WS_VISIBLE" => 0x1000_0000,
        "WS_DISABLED" => 0x0800_0000,
        "WS_BORDER" => 0x0080_0000,
        "WS_DLGFRAME" => 0x0040_0000,
        "WS_CAPTION" => 0x00C0_0000,
        "WS_SYSMENU" => 0x0008_0000,
        "WS_HSCROLL" => 0x0010_0000,
        "WS_VSCROLL" => 0x0020_0000,
        "WS_THICKFRAME" => 0x0004_0000,
        "WS_MINIMIZEBOX" => 0x0002_0000,
        "WS_GROUP" => 0x0002_0000,
        "WS_TABSTOP" => 0x0001_0000,
        // dialog styles
        "DS_ABSALIGN" => 0x0000_0001,
        "DS_SYSMODAL" => 0x0000_0002,
        "DS_SETFONT" => 0x0000_0040,
        "DS_MODALFRAME" => 0x0000_0080,
        "DS_CONTEXTHELP" => 0x0000_2000,
        // static styles
        "SS_LEFT" => 0x0000_0000,
        "SS_CENTER" => 0x0000_0001,
        "SS_RIGHT" => 0x0000_0002,
        "SS_ICON" => 0x0000_0003,
        "SS_NOPREFIX" => 0x0000_0080,
        // button styles
        "BS_PUSHBUTTON" => 0x0000_0000,
        "BS_DEFPUSHBUTTON" => 0x0000_0001,
        "BS_CHECKBOX" => 0x0000_0002,
        "BS_AUTOCHECKBOX" => 0x0000_0003,
        "BS_RADIOBUTTON" => 0x0000_0004,
        "BS_AUTORADIOBUTTON" => 0x0000_0009,
        "BS_LEFTTEXT" => 0x0000_0020,
        "BS_GROUPBOX" => 0x0000_0007,
        // edit styles
        "ES_LEFT" => 0x0000_0000,
        "ES_MULTILINE" => 0x0000_0004,
        "ES_PASSWORD" => 0x0000_0020,
        "ES_AUTOVSCROLL" => 0x0000_0040,
        "ES_AUTOHSCROLL" => 0x0000_0080,
        "ES_OEMCONVERT" => 0x0000_0400,
        "ES_CENTER" => 0x0000_0001,
        "ES_READONLY" => 0x0000_0800,
        "ES_NUMBER" => 0x0000_2000,
        // combobox styles
        "CBS_DROPDOWN" => 0x0000_0002,
        "CBS_DROPDOWNLIST" => 0x0000_0003,
        "CBS_SORT" => 0x0000_0100,
        _ => return None,
    })
}

fn eval_style(expr: &str, line: u32) -> u32 {
    let mut value: u32 = 0;
    for part in expr.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let v = if let Some(rest) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
            u32::from_str_radix(rest, 16)
                .unwrap_or_else(|_| panic!("line {line}: invalid hex style '{part}'"))
        } else if part.chars().all(|c| c.is_ascii_digit()) {
            part.parse::<u32>()
                .unwrap_or_else(|_| panic!("line {line}: invalid numeric style '{part}'"))
        } else {
            style_const(part).unwrap_or_else(|| panic!("line {line}: unknown style constant '{part}'"))
        };
        value |= v;
    }
    value
}

// ---------------------------------------------------------------------------
// .rc tokenizer
// ---------------------------------------------------------------------------

/// Splits `s` on top-level commas (commas inside "quoted strings" are kept).
fn split_top_level(s: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if !in_string => {
                in_string = true;
                current.push(c);
            }
            '"' if in_string => {
                in_string = false;
                current.push(c);
            }
            '\\' if in_string => {
                current.push(c);
                if let Some(&next) = chars.peek() {
                    current.push(next);
                    chars.next();
                }
            }
            ',' if !in_string => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        fields.push(current.trim().to_string());
    }
    fields
}

/// Parses a double-quoted .rc string literal, decoding RC escapes
/// (\n newline, \" quote, \\ backslash).
fn parse_rc_string(raw: &str, line: u32) -> String {
    let raw = raw.trim();
    let bytes = raw.as_bytes();
    if bytes.first() != Some(&b'"') || bytes.last() != Some(&b'"') || bytes.len() < 2 {
        panic!("line {line}: expected quoted string, got '{raw}'");
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Re-escapes a decoded string as a Rust string literal.
fn rust_lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Dialog model
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Control {
    id: i32,
    class: String, // raw class token: BUTTON, STATIC, "BorBtn", ...
    text: String,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Debug, Default)]
struct Dialog {
    id: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    style: u32,
    caption: Option<String>,
    menu: Option<String>,
    class: Option<String>,
    font_size: Option<u32>,
    font_name: Option<String>,
    controls: Vec<Control>,
}

fn parse_dialogs(rc: &str) -> Vec<Dialog> {
    let mut dialogs: Vec<Dialog> = Vec::new();
    let mut current: Option<Dialog> = None;
    let mut in_body = false;

    for (idx, raw_line) in rc.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let line = raw_line.trim();

        if let Some(rest) = line.strip_prefix("CONTROL ") {
            let dlg = current
                .as_mut()
                .unwrap_or_else(|| panic!("line {line_no}: CONTROL outside of a dialog"));
            let fields = split_top_level(rest);
            // CONTROL text, id, class, style, x, y, w, h [, extended style]
            if fields.len() < 8 {
                panic!("line {line_no}: CONTROL with {} fields: {line}", fields.len());
            }
            let text = parse_rc_string(&fields[0], line_no);
            let id: i32 = fields[1]
                .parse()
                .unwrap_or_else(|_| panic!("line {line_no}: bad control id '{}'", fields[1]));
            let class = fields[2].trim_matches('"').to_string();
            let style = eval_style(&fields[3], line_no);
            let coords: Vec<i32> = fields[4..8]
                .iter()
                .map(|f| {
                    f.parse::<i32>()
                        .unwrap_or_else(|_| panic!("line {line_no}: bad control coord '{f}'"))
                })
                .collect();
            if let Some(ext) = fields.get(8) {
                if !ext.trim().is_empty() && ext.trim() != "0" {
                    panic!("line {line_no}: unsupported extended style '{ext}'");
                }
            }
            dlg.controls.push(Control {
                id,
                class,
                text,
                style,
                x: coords[0],
                y: coords[1],
                w: coords[2],
                h: coords[3],
            });
            continue;
        }

        if in_body {
            if line == "}" {
                in_body = false;
                dialogs.push(current.take().expect("dialog body without header"));
            }
            continue;
        }

        // Dialog header: "<id> DIALOG x, y, w, h"
        if let Some((header, geometry)) = line.split_once(" DIALOG ") {
            let id: u32 = header
                .parse()
                .unwrap_or_else(|_| panic!("line {line_no}: bad dialog id '{header}'"));
            let fields: Vec<i32> = geometry
                .split(',')
                .map(|f| {
                    f.trim()
                        .parse::<i32>()
                        .unwrap_or_else(|_| panic!("line {line_no}: bad dialog coord '{f}'"))
                })
                .collect();
            assert!(
                current.is_none(),
                "line {line_no}: dialog {id} starts before previous one is closed"
            );
            current = Some(Dialog {
                id,
                x: fields[0],
                y: fields[1],
                w: fields[2],
                h: fields[3],
                ..Default::default()
            });
            continue;
        }

        if let Some(dlg) = current.as_mut() {
            if let Some(rest) = line.strip_prefix("STYLE ") {
                dlg.style = eval_style(rest, line_no);
            } else if let Some(rest) = line.strip_prefix("CAPTION ") {
                dlg.caption = Some(parse_rc_string(rest, line_no));
            } else if let Some(rest) = line.strip_prefix("MENU ") {
                dlg.menu = Some(parse_rc_string(rest, line_no));
            } else if let Some(rest) = line.strip_prefix("CLASS ") {
                dlg.class = Some(parse_rc_string(rest, line_no));
            } else if let Some(rest) = line.strip_prefix("FONT ") {
                let fields = split_top_level(rest);
                // FONT size, "name"
                dlg.font_size = Some(
                    fields[0]
                        .parse()
                        .unwrap_or_else(|_| panic!("line {line_no}: bad font size '{}'", fields[0])),
                );
                dlg.font_name = Some(parse_rc_string(&fields[1], line_no));
            } else if line == "{" {
                in_body = true;
            }
            // LANGUAGE and anything else: ignored
        }
    }

    assert!(current.is_none(), "dialogs.rc ends with an unclosed dialog");
    dialogs
}

// ---------------------------------------------------------------------------
// Code generation: dialogs.rs
// ---------------------------------------------------------------------------

/// Maps a raw .rc control class token to the generated enum variant.
fn control_kind(class: &str) -> &'static str {
    match class {
        "BUTTON" => "Button",
        "STATIC" => "Static",
        "EDIT" => "Edit",
        "COMBOBOX" => "Combobox",
        "BorShade" => "BorShade",
        "BorBtn" => "BorBtn",
        "BorStatic" => "BorStatic",
        "BorCheck" => "BorCheck",
        "BorRadio" => "BorRadio",
        "BMPView" => "BmpView",
        "BMPTipa" => "BmpTipa",
        "msctls_progress" => "MsctlsProgress",
        other => panic!("unknown control class '{other}'"),
    }
}

fn gen_dialogs(dialogs: &[Dialog], out: &mut String) {
    macro_rules! w {
        ($s:expr) => {
            writeln!(out, "{}", $s).unwrap()
        };
    }

    w!("// @generated by tabboz-win/build.rs from resources/dialogs/dialogs.rc -- do not edit.");
    w!("");
    w!("/// Control class, mapped 1:1 from the .rc CONTROL class token.");
    w!("#[derive(Clone, Copy, Debug, PartialEq, Eq)]");
    w!("pub enum ControlKind {");
    w!("    Button,");
    w!("    Static,");
    w!("    Edit,");
    w!("    Combobox,");
    w!("    BorShade,");
    w!("    BorBtn,");
    w!("    BorStatic,");
    w!("    BorCheck,");
    w!("    BorRadio,");
    w!("    BmpView,");
    w!("    BmpTipa,");
    w!("    MsctlsProgress,");
    w!("}");
    w!("");
    w!("/// A single CONTROL entry of a dialog template.");
    w!("#[derive(Clone, Copy, Debug)]");
    w!("pub struct Control {");
    w!("    /// Control id (dialog-local); -1 for static decorations.");
    w!("    pub id: i32,");
    w!("    pub kind: ControlKind,");
    w!("    /// Raw class token from the .rc file (\"BorBtn\", \"STATIC\", ...).");
    w!("    pub class: &'static str,");
    w!("    /// Decoded control text (may be empty).");
    w!("    pub text: &'static str,");
    w!("    /// OR-ed Win32 style bits.");
    w!("    pub style: u32,");
    w!("    /// Position and size in dialog units.");
    w!("    pub x: i32,");
    w!("    pub y: i32,");
    w!("    pub w: i32,");
    w!("    pub h: i32,");
    w!("}");
    w!("");
    w!("/// A DIALOG template from the original resource file.");
    w!("#[derive(Clone, Copy, Debug)]");
    w!("pub struct Dialog {");
    w!("    /// Dialog resource id (MAIN = 1, ABOUT = 2, ...).");
    w!("    pub id: u32,");
    w!("    /// OR-ed dialog/window style bits.");
    w!("    pub style: u32,");
    w!("    pub caption: Option<&'static str>,");
    w!("    pub menu: Option<&'static str>,");
    w!("    /// Registered window class (\"bordlg\", \"Bordlg\", \"BorDlg_Gray\", ...).");
    w!("    pub class: Option<&'static str>,");
    w!("    pub font_size: Option<u32>,");
    w!("    pub font_name: Option<&'static str>,");
    w!("    /// Dialog position and size in dialog units.");
    w!("    pub x: i32,");
    w!("    pub y: i32,");
    w!("    pub w: i32,");
    w!("    pub h: i32,");
    w!("    pub controls: &'static [Control],");
    w!("}");
    w!("");

    // Static slices of controls, one per dialog, then the dialog table.
    for (i, dlg) in dialogs.iter().enumerate() {
        w!(&format!("static CONTROLS_{}: &[Control] = &[", i));
        for c in &dlg.controls {
            w!(&format!(
                "    Control {{ id: {}, kind: ControlKind::{}, class: {}, text: {}, style: 0x{:08X}, x: {}, y: {}, w: {}, h: {} }},",
                c.id,
                control_kind(&c.class),
                rust_lit(&c.class),
                rust_lit(&c.text),
                c.style,
                c.x,
                c.y,
                c.w,
                c.h,
            ));
        }
        w!("];");
        w!("");
    }

    w!("/// All dialog templates, in .rc order.");
    w!("pub static DIALOGS: &[Dialog] = &[");
    for (i, dlg) in dialogs.iter().enumerate() {
        w!(&format!(
            "    Dialog {{ id: {}, style: 0x{:08X}, caption: {}, menu: {}, class: {}, font_size: {}, font_name: {}, x: {}, y: {}, w: {}, h: {}, controls: CONTROLS_{} }},",
            dlg.id,
            dlg.style,
            opt_lit(&dlg.caption),
            opt_lit(&dlg.menu),
            opt_lit(&dlg.class),
            dlg.font_size.map(|v| format!("Some({v})")).unwrap_or_else(|| "None".to_string()),
            opt_lit(&dlg.font_name),
            dlg.x,
            dlg.y,
            dlg.w,
            dlg.h,
            i,
        ));
    }
    w!("];");
    w!("");
    w!("/// Looks up a dialog template by resource id.");
    w!("pub fn find(id: u32) -> Option<&'static Dialog> {");
    w!("    DIALOGS.iter().find(|d| d.id == id)");
    w!("}");
    w!("");
    w!("/// Total number of control templates across all dialogs.");
    w!("pub fn control_count() -> usize {");
    w!("    DIALOGS.iter().map(|d| d.controls.len()).sum()");
    w!("}");
}

fn opt_lit(s: &Option<String>) -> String {
    match s {
        Some(v) => format!("Some({})", rust_lit(v)),
        None => "None".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Code generation: assets.rs
// ---------------------------------------------------------------------------

fn collect_assets(dir: &Path, exts: &[&str], skip_png_with_bmp_twin: bool) -> Vec<(String, PathBuf)> {
    let mut by_name: BTreeMap<String, PathBuf> = BTreeMap::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !exts.contains(&ext.as_str()) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if skip_png_with_bmp_twin && ext == "png" && dir.join(format!("{stem}.bmp")).exists() {
            continue; // prefer the original BMP, drop the PNG twin
        }
        by_name.insert(stem.to_string(), path);
    }
    let mut v: Vec<(String, PathBuf)> = by_name.into_iter().collect();
    v.sort_by(|a, b| natural_key(&a.0).cmp(&natural_key(&b.0)));
    v
}

/// Sorts "2" before "10" so asset arrays are in numeric resource order.
fn natural_key(s: &str) -> (u32, String) {
    (s.parse::<u32>().unwrap_or(u32::MAX), s.to_string())
}

fn gen_asset_array(out: &mut String, name: &str, entries: &[(String, PathBuf)], assets: &Path) -> usize {
    macro_rules! w {
        ($s:expr) => {
            writeln!(out, "{}", $s).unwrap()
        };
    }
    w!(&format!(
        "/// Embedded assets ({name}); keys are resource names from the original game."
    ));
    w!(&format!("pub static {}: &[Asset] = &[", name.to_uppercase()));
    for (stem, path) in entries {
        let rel = path
            .strip_prefix(assets)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        w!(&format!(
            "    Asset {{ name: {}, data: include_bytes!(concat!(env!(\"TABBOZ_ASSETS_DIR\"), \"/{}\")) }},",
            rust_lit(stem),
            rel
        ));
    }
    w!("];");
    w!("");
    entries.len()
}

fn gen_assets(resources: &Path, out: &mut String) -> usize {
    macro_rules! w {
        ($s:expr) => {
            writeln!(out, "{}", $s).unwrap()
        };
    }

    w!("// @generated by tabboz-win/build.rs from resources/ -- do not edit.");
    w!("");
    w!("#[derive(Clone, Copy)]");
    w!("pub struct Asset {");
    w!("    pub name: &'static str,");
    w!("    pub data: &'static [u8],");
    w!("}");
    w!("");

    let bitmaps = collect_assets(&resources.join("bitmaps"), &["bmp", "png"], true);
    let chrome = collect_assets(&resources.join("bitmaps/novantotto"), &["bmp", "png"], true);
    let icons = collect_assets(&resources.join("icons"), &["ico"], false);
    let sounds = collect_assets(&resources.join("wavs"), &["wav"], false);
    let strings = collect_assets(&resources.join("strings"), &["json"], false);
    let menus = collect_assets(&resources.join("menus"), &["json"], false);

    let count = gen_asset_array(out, "BITMAPS", &bitmaps, resources)
        + gen_asset_array(out, "CHROME_BITMAPS", &chrome, resources)
        + gen_asset_array(out, "ICONS", &icons, resources)
        + gen_asset_array(out, "SOUNDS", &sounds, resources)
        + gen_asset_array(out, "STRINGS", &strings, resources)
        + gen_asset_array(out, "MENUS", &menus, resources);
    w!("/// Total number of embedded assets.");
    w!("pub fn asset_count() -> usize {");
    w!("    BITMAPS.len() + CHROME_BITMAPS.len() + ICONS.len() + SOUNDS.len() + STRINGS.len() + MENUS.len()");
    w!("}");
    w!("");
    w!("/// Total embedded asset bytes.");
    w!("pub fn asset_bytes() -> usize {");
    w!("    BITMAPS.iter().chain(CHROME_BITMAPS.iter()).chain(ICONS.iter()).chain(SOUNDS.iter()).chain(STRINGS.iter()).chain(MENUS.iter())");
    w!("        .map(|a| a.data.len())");
    w!("        .sum()");
    w!("}");
    count
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // crates/tabboz-win -> repo root
    let repo = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("cannot locate repo root")
        .to_path_buf();
    let resources = repo.join("resources");

    let rc_path = resources.join("dialogs/dialogs.rc");
    println!("cargo:rerun-if-changed={}", rc_path.display());
    println!("cargo:rerun-if-changed={}", resources.display());
    // Absolute path usable by include_bytes! in the generated code.
    println!("cargo:rustc-env=TABBOZ_ASSETS_DIR={}", resources.display());

    let rc = fs::read_to_string(&rc_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", rc_path.display()));
    let dialogs = parse_dialogs(&rc);
    assert!(
        !dialogs.is_empty(),
        "no dialogs parsed from {}",
        rc_path.display()
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    let mut dialogs_code = String::new();
    gen_dialogs(&dialogs, &mut dialogs_code);
    fs::write(out_dir.join("dialogs.rs"), dialogs_code).expect("write dialogs.rs");

    let mut assets_code = String::new();
    let asset_count = gen_assets(&resources, &mut assets_code);
    fs::write(out_dir.join("assets.rs"), assets_code).expect("write assets.rs");

    println!(
        "cargo:warning=tabboz-win codegen: {} dialogs, {} controls, {} embedded assets",
        dialogs.len(),
        dialogs.iter().map(|d| d.controls.len()).sum::<usize>(),
        asset_count
    );
}
