// Build script for tabboz-res: parses the exported string tables and menu
// template into typed Rust code. Zero external dependencies.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("cannot locate repo root")
        .to_path_buf();
    let resources = repo.join("resources");

    let menus_path = resources.join("menus/ZARROSIM.json");
    let strings_dir = resources.join("strings");
    println!("cargo:rerun-if-changed={}", menus_path.display());
    println!("cargo:rerun-if-changed={}", strings_dir.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    let mut menus_code = String::new();
    let menu_items = gen_menus(&menus_path, &mut menus_code);
    fs::write(out_dir.join("menus.rs"), menus_code).expect("write menus.rs");

    let mut strings_code = String::new();
    let string_count = gen_strings(&strings_dir, &mut strings_code);
    fs::write(out_dir.join("strings.rs"), strings_code).expect("write strings.rs");

    println!(
        "cargo:warning=tabboz-res codegen: {} menu items, {} strings",
        menu_items, string_count
    );
}

// ---------------------------------------------------------------------------
// Minimal JSON parser (only what the exported resources need)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)] // the grammar is wider than what the resources use
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn as_str(&self) -> &str {
        match self {
            Json::Str(s) => s,
            other => panic!("expected JSON string, got {other:?}"),
        }
    }
    fn as_f64(&self) -> f64 {
        match self {
            Json::Num(n) => *n,
            other => panic!("expected JSON number, got {other:?}"),
        }
    }
    fn as_arr(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            other => panic!("expected JSON array, got {other:?}"),
        }
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> u8 {
        self.bytes[self.pos]
    }

    fn expect(&mut self, c: u8) {
        self.skip_ws();
        assert_eq!(self.peek(), c, "JSON: expected '{}' at byte {}", c as char, self.pos);
        self.pos += 1;
    }

    fn parse_value(&mut self) -> Json {
        self.skip_ws();
        match self.peek() {
            b'{' => {
                self.pos += 1;
                let mut pairs = Vec::new();
                self.skip_ws();
                if self.peek() == b'}' {
                    self.pos += 1;
                    return Json::Obj(pairs);
                }
                loop {
                    self.skip_ws();
                    let key = match self.parse_value() {
                        Json::Str(s) => s,
                        other => panic!("JSON: object key must be a string, got {other:?}"),
                    };
                    self.expect(b':');
                    let val = self.parse_value();
                    pairs.push((key, val));
                    self.skip_ws();
                    match self.peek() {
                        b',' => self.pos += 1,
                        b'}' => {
                            self.pos += 1;
                            return Json::Obj(pairs);
                        }
                        c => panic!("JSON: expected ',' or '}}', got '{}' at byte {}", c as char, self.pos),
                    }
                }
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.peek() == b']' {
                    self.pos += 1;
                    return Json::Arr(items);
                }
                loop {
                    items.push(self.parse_value());
                    self.skip_ws();
                    match self.peek() {
                        b',' => self.pos += 1,
                        b']' => {
                            self.pos += 1;
                            return Json::Arr(items);
                        }
                        c => panic!("JSON: expected ',' or ']', got '{}' at byte {}", c as char, self.pos),
                    }
                }
            }
            b'"' => Json::Str(self.parse_string()),
            b't' => {
                self.expect_word("true");
                Json::Bool(true)
            }
            b'f' => {
                self.expect_word("false");
                Json::Bool(false)
            }
            b'n' => {
                self.expect_word("null");
                Json::Null
            }
            _ => {
                let start = self.pos;
                while self.pos < self.bytes.len()
                    && matches!(self.bytes[self.pos], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    self.pos += 1;
                }
                let raw = std::str::from_utf8(&self.bytes[start..self.pos]).expect("utf8 in number");
                Json::Num(raw.parse().unwrap_or_else(|_| panic!("JSON: bad number '{raw}'")))
            }
        }
    }

    fn expect_word(&mut self, word: &str) {
        for c in word.bytes() {
            assert_eq!(self.bytes[self.pos], c, "JSON: expected '{word}'");
            self.pos += 1;
        }
    }

    fn parse_string(&mut self) -> String {
        assert_eq!(self.peek(), b'"');
        self.pos += 1;
        let mut out = String::new();
        loop {
            let c = self.bytes[self.pos];
            self.pos += 1;
            match c {
                b'"' => return out,
                b'\\' => {
                    let e = self.bytes[self.pos];
                    self.pos += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'u' => {
                            let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4]).expect("utf8 hex");
                            self.pos += 4;
                            let cp = u32::from_str_radix(hex, 16).expect("bad \\u escape");
                            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                        }
                        other => panic!("JSON: unsupported escape '\\{}'", other as char),
                    }
                }
                _ => {
                    // Collect a full UTF-8 sequence.
                    let start = self.pos - 1;
                    let len = utf8_len(c);
                    self.pos = start + len;
                    out.push_str(std::str::from_utf8(&self.bytes[start..self.pos]).expect("utf8"));
                }
            }
        }
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

fn parse_json(s: &str) -> Json {
    let mut p = JsonParser::new(s);
    let v = p.parse_value();
    p.skip_ws();
    assert_eq!(p.pos, p.bytes.len(), "JSON: trailing garbage");
    v
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

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

fn gen_menus(menus_path: &Path, out: &mut String) -> usize {
    let json = fs::read_to_string(menus_path).expect("read menus json");
    let root = parse_json(&json);
    let popups = root.as_arr();
    macro_rules! w {
        ($s:expr) => {
            writeln!(out, "{}", $s).unwrap()
        };
    }
    w!("// @generated by tabboz-res/build.rs from resources/menus/ -- do not edit.");
    w!("");
    w!("/// One entry of a popup menu.");
    w!("#[derive(Clone, Copy, Debug)]");
    w!("pub struct MenuItem {");
    w!("    /// '&' marks the keyboard hotkey.");
    w!("    pub label: &'static str,");
    w!("    /// Command id sent to the dialog handler (0 for separators).");
    w!("    pub menu_id: u16,");
    w!("    pub separator: bool,");
    w!("}");
    w!("");
    w!("/// One top-level popup of the menu bar.");
    w!("#[derive(Clone, Copy, Debug)]");
    w!("pub struct MenuPopup {");
    w!("    pub label: &'static str,");
    w!("    pub items: &'static [MenuItem],");
    w!("}");
    w!("");
    let mut total = 0;
    for (i, popup) in popups.iter().enumerate() {
        let label = popup.get("label").expect("popup label").as_str();
        let items = popup.get("items").expect("popup items").as_arr();
        w!(format!("static MENU_ITEMS_{}: &[MenuItem] = &[", i));
        for item in items {
            let kind = item.get("kind").expect("item kind").as_str();
            match kind {
                "menuitem" => {
                    let label = item.get("label").expect("item label").as_str();
                    let id = item.get("menu_id").expect("item menu_id").as_f64() as u16;
                    w!(format!(
                        "    MenuItem {{ label: {}, menu_id: {}, separator: false }},",
                        rust_lit(label),
                        id
                    ));
                }
                "separator" => {
                    w!("    MenuItem { label: \"\", menu_id: 0, separator: true },");
                }
                other => panic!("unknown menu item kind '{other}'"),
            }
            total += 1;
        }
        w!("];");
        w!(format!("static MENU_POPUP_{}: MenuPopup = MenuPopup {{ label: {}, items: MENU_ITEMS_{} }};", i, rust_lit(label), i));
        w!("");
    }
    w!("/// Menu bar of the main window (resource \"zarrosim\").");
    w!("pub static MENUBAR: &[MenuPopup] = &[");
    for i in 0..popups.len() {
        w!(format!("    MENU_POPUP_{i},"));
    }
    w!("];");
    total
}

fn gen_strings(strings_dir: &Path, out: &mut String) -> usize {
    let mut entries: Vec<(u16, String)> = Vec::new();
    let mut files: Vec<String> = fs::read_dir(strings_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", strings_dir.display()))
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect();
    files.sort();
    for file in files {
        let json = fs::read_to_string(strings_dir.join(&file)).expect("read string table");
        let root = parse_json(&json);
        let Json::Obj(pairs) = root else { panic!("{file}: expected a JSON object") };
        for (key, value) in pairs {
            let id: u16 = key.parse().unwrap_or_else(|_| panic!("{file}: bad string id '{key}'"));
            entries.push((id, value.as_str().to_string()));
        }
    }
    entries.sort_by_key(|(id, _)| *id);

    macro_rules! w {
        ($s:expr) => {
            writeln!(out, "{}", $s).unwrap()
        };
    }
    w!("// @generated by tabboz-res/build.rs from resources/strings/ -- do not edit.");
    w!("");
    w!("/// One entry of the original STRINGTABLE resources.");
    w!("#[derive(Clone, Copy, Debug)]");
    w!("pub struct StringEntry {");
    w!("    pub id: u16,");
    w!("    pub text: &'static str,");
    w!("}");
    w!("");
    w!("/// All string table entries, sorted by id.");
    w!("pub static STRINGS: &[StringEntry] = &[");
    for (id, text) in &entries {
        w!(format!("    StringEntry {{ id: {}, text: {} }},", id, rust_lit(text)));
    }
    w!("];");
    w!("");
    w!("/// Looks up a string table entry by id (binary search, STRINGS is sorted).");
    w!("pub fn find(id: u16) -> Option<&'static str> {");
    w!("    STRINGS");
    w!("        .binary_search_by_key(&id, |s| s.id)");
    w!("        .ok()");
    w!("        .map(|i| STRINGS[i].text)");
    w!("}");
    entries.len()
}
