//! Key names, the way `core/hotkeys.py` writes them. Combos are stored as `ctrl+shift+space`, `alt`,
//! `ctrl+alt+/`, shown as `Ctrl+Shift+Space` and bound in Hyprland as `CTRL + SHIFT + space`. Everything here
//! is pure, so both systems can check every case in `hotkey_cases.json`.

/// Which system's names to use (Win or Super, and which keys can be bound).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
}

impl Os {
    pub const fn current() -> Os {
        if cfg!(windows) { Os::Windows } else { Os::Linux }
    }
}

const ALIASES: [(&str, &str); 13] = [
    ("control", "ctrl"),
    ("win", "windows"),
    ("super", "windows"),
    ("cmd", "windows"),
    ("meta", "windows"),
    ("escape", "esc"),
    ("return", "enter"),
    ("del", "delete"),
    ("ins", "insert"),
    ("pgup", "page up"),
    ("pgdn", "page down"),
    ("pageup", "page up"),
    ("pagedown", "page down"),
];

/// Our modifier names and Hyprland's.
pub const MODIFIERS: [(&str, &str); 4] =
    [("ctrl", "CTRL"), ("alt", "ALT"), ("shift", "SHIFT"), ("windows", "SUPER")];

/// The xkb keysyms of each modifier key (what Hyprland calls the key itself).
const MOD_KEYSYMS: [(&str, [&str; 2]); 4] = [
    ("ctrl", ["Control_L", "Control_R"]),
    ("alt", ["Alt_L", "Alt_R"]),
    ("shift", ["Shift_L", "Shift_R"]),
    ("windows", ["Super_L", "Super_R"]),
];

/// Hyprland's bits for each modifier in a bind's `modmask`.
pub const MODMASK: [(&str, u32); 4] = [("SHIFT", 1), ("CTRL", 4), ("ALT", 8), ("SUPER", 64)];

/// Our key names and their xkb keysym names.
const NAMED: [(&str, &str); 55] = [
    ("space", "space"),
    ("esc", "Escape"),
    ("enter", "Return"),
    ("tab", "Tab"),
    ("backspace", "BackSpace"),
    ("delete", "Delete"),
    ("insert", "Insert"),
    ("home", "Home"),
    ("end", "End"),
    ("page up", "Prior"),
    ("page down", "Next"),
    ("up", "Up"),
    ("down", "Down"),
    ("left", "Left"),
    ("right", "Right"),
    ("print screen", "Print"),
    ("print", "Print"),
    ("pause", "Pause"),
    ("caps lock", "Caps_Lock"),
    ("num lock", "Num_Lock"),
    ("scroll lock", "Scroll_Lock"),
    ("menu", "Menu"),
    ("apps", "Menu"),
    ("=", "equal"),
    ("-", "minus"),
    ("+", "plus"),
    ("/", "slash"),
    ("\\", "backslash"),
    (",", "comma"),
    (".", "period"),
    (";", "semicolon"),
    ("'", "apostrophe"),
    ("#", "numbersign"),
    ("[", "bracketleft"),
    ("]", "bracketright"),
    ("`", "grave"),
    ("*", "asterisk"),
    ("!", "exclam"),
    ("?", "question"),
    (":", "colon"),
    ("<", "less"),
    (">", "greater"),
    ("~", "asciitilde"),
    ("@", "at"),
    ("$", "dollar"),
    ("%", "percent"),
    ("^", "asciicircum"),
    ("&", "ampersand"),
    ("(", "parenleft"),
    (")", "parenright"),
    ("_", "underscore"),
    ("|", "bar"),
    ("\"", "quotedbl"),
    ("£", "sterling"),
    ("¬", "notsign"),
];

/// Media and other special keys (some mice send these from their extra buttons).
const XF86: [&str; 27] = [
    "AudioPlay",
    "AudioPause",
    "AudioStop",
    "AudioNext",
    "AudioPrev",
    "AudioMute",
    "AudioRaiseVolume",
    "AudioLowerVolume",
    "AudioMicMute",
    "Back",
    "Forward",
    "Search",
    "HomePage",
    "Tools",
    "Launch5",
    "Launch6",
    "Launch7",
    "Launch8",
    "Launch9",
    "Calculator",
    "Mail",
    "Explorer",
    "Favorites",
    "Refresh",
    "Copy",
    "Cut",
    "Paste",
];

const KEYPAD: [(&str, &str); 6] = [
    ("num +", "KP_Add"),
    ("num -", "KP_Subtract"),
    ("num *", "KP_Multiply"),
    ("num /", "KP_Divide"),
    ("num enter", "KP_Enter"),
    ("num .", "KP_Decimal"),
];

/// Every button a mouse can report (Linux `BTN_MIDDLE` to `BTN_TASK` and the eight after it): gaming mice like
/// Logitech's put their extra thumb and DPI buttons up there.
pub fn mouse_buttons() -> Vec<(String, String)> {
    let mut list = vec![
        ("mouse:274".to_owned(), "Middle button".to_owned()),
        ("mouse:275".to_owned(), "Side button (back)".to_owned()),
        ("mouse:276".to_owned(), "Side button (forward)".to_owned()),
    ];
    list.extend((277..289).map(|n| (format!("mouse:{n}"), format!("Extra button {}", n - 276))));
    list
}

pub fn hypr_modifier(name: &str) -> Option<&'static str> {
    MODIFIERS.iter().find(|(m, _)| *m == name).map(|(_, h)| *h)
}

fn is_mod(name: &str) -> bool {
    hypr_modifier(name).is_some()
}

/// `Ctrl + Shift+M` → `ctrl+shift+m`.
pub fn normalize(combo: &str) -> String {
    if combo.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = combo
        .replace(" + ", "+")
        .split('+')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    // "ctrl++" means Ctrl and the plus key
    if combo.trim().ends_with("++") {
        parts.push("+".to_owned());
    }
    for p in &mut parts {
        if let Some((_, to)) = ALIASES.iter().find(|(from, _)| from == p) {
            *p = (*to).to_owned();
        }
        // Hyprland binds do not tell left from right modifiers
        for word in ["left ", "right "] {
            if let Some(rest) = p.strip_prefix(word)
                && is_mod(rest)
            {
                *p = rest.to_owned();
            }
        }
    }
    parts.join("+")
}

/// Python's `str.title()`: a letter after a non-letter goes up, the others down.
fn title(s: &str) -> String {
    let mut out = String::new();
    let mut after_letter = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if after_letter {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            after_letter = true;
        } else {
            out.push(c);
            after_letter = false;
        }
    }
    out
}

fn pretty_part(p: &str, os: Os) -> String {
    let fixed = match p {
        "ctrl" => Some("Ctrl"),
        "alt" => Some("Alt"),
        "shift" => Some("Shift"),
        "windows" => Some(if os == Os::Windows { "Win" } else { "Super" }),
        "esc" => Some("Esc"),
        "enter" => Some("Enter"),
        "space" => Some("Space"),
        "tab" => Some("Tab"),
        "backspace" => Some("Backspace"),
        "delete" => Some("Delete"),
        "insert" => Some("Insert"),
        "home" => Some("Home"),
        "end" => Some("End"),
        "page up" => Some("PgUp"),
        "page down" => Some("PgDn"),
        "up" => Some("↑"),
        "down" => Some("↓"),
        "left" => Some("←"),
        "right" => Some("→"),
        _ => None,
    };
    if let Some(name) = fixed {
        return name.to_owned();
    }
    if let Some((_, label)) = mouse_buttons().into_iter().find(|(b, _)| b == p) {
        return label
            .replace("Side button (", "Mouse ")
            .replace(')', "")
            .replace("Extra button", "Mouse extra");
    }
    if is_code(p) {
        return format!("Key {}", &p[5..]);
    }
    if let Some(rest) = p.strip_prefix("xf86") {
        return title(rest);
    }
    if p.chars().count() == 1 { p.to_uppercase() } else { title(p) }
}

/// The parts of a combo as menus and tooltips show them (`Ctrl`, `Shift`, `M`).
pub fn pretty_parts(combo: &str, os: Os) -> Vec<String> {
    let n = normalize(combo);
    if n.is_empty() {
        return Vec::new();
    }
    n.split('+').map(|p| pretty_part(p, os)).collect()
}

/// `ctrl+shift+m` → `Ctrl+Shift+M`.
pub fn pretty(combo: &str, os: Os) -> String {
    if combo.is_empty() {
        return "(none)".to_owned();
    }
    let n = normalize(combo);
    n.split('+').map(|p| pretty_part(p, os)).collect::<Vec<_>>().join("+")
}

pub fn is_modifier(key: &str) -> bool {
    is_mod(&normalize(key))
}

pub fn is_mouse(key: &str) -> bool {
    key.strip_prefix("mouse:").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// A raw key code (`code:191`): keys with no name, like the extra keys some mice and keyboards send.
pub fn is_code(key: &str) -> bool {
    key.strip_prefix("code:").is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn table_keysym(key: &str) -> Option<String> {
    if let Some((_, ks)) = NAMED.iter().chain(KEYPAD.iter()).find(|(k, _)| *k == key) {
        return Some((*ks).to_owned());
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&n)
        && key == format!("f{n}")
    {
        return Some(format!("F{n}"));
    }
    if let Some(n) = key.strip_prefix("num ").and_then(|n| n.parse::<u32>().ok())
        && n < 10
        && key == format!("num {n}")
    {
        return Some(format!("KP_{n}"));
    }
    if let Some(rest) = key.strip_prefix("xf86") {
        return XF86.iter().find(|k| k.to_lowercase() == rest).map(|k| format!("XF86{k}"));
    }
    None
}

/// `page up` → `Prior`, `q` → `q`, `=` → `equal`; empty if unknown.
pub fn keysym(key: &str, os: Os) -> String {
    let key = normalize(key);
    if let Some(ks) = table_keysym(&key) {
        return ks;
    }
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_alphanumeric()
    {
        return key;
    }
    // Hyprland binds mouse buttons by number (mouse:275 is the side back button)
    if os == Os::Linux && (is_mouse(&key) || is_code(&key)) {
        return key;
    }
    String::new()
}

/// Every keysym that means `key` (both Alt keys for `alt`).
pub fn keysyms_of(key: &str, os: Os) -> Vec<String> {
    let key = normalize(key);
    if let Some((_, syms)) = MOD_KEYSYMS.iter().find(|(m, _)| *m == key) {
        return syms.iter().map(|s| (*s).to_owned()).collect();
    }
    let ks = keysym(&key, os);
    if ks.is_empty() { Vec::new() } else { vec![ks] }
}

/// The modifiers and the one key of a combo. A bare modifier (`alt`) is its own key. The key is empty when
/// there is more than one.
pub fn split(combo: &str) -> (Vec<String>, String) {
    let n = normalize(combo);
    let parts: Vec<&str> = n.split('+').collect();
    let mut mods: Vec<String> = parts.iter().filter(|p| is_mod(p)).map(|p| (*p).to_owned()).collect();
    let mut keys: Vec<String> = parts.iter().filter(|p| !is_mod(p)).map(|p| (*p).to_owned()).collect();
    if keys.is_empty()
        && let Some(last) = mods.pop()
    {
        keys.push(last);
    }
    let key = if keys.len() == 1 { keys.remove(0) } else { String::new() };
    (mods, key)
}

/// Hyprland bind strings for `combo`. A bare modifier key needs one per side (`ALT + Alt_L`).
pub fn to_hyprland(combo: &str) -> Vec<String> {
    let (mods, key) = split(combo);
    let mut hmods: Vec<&str> = mods.iter().filter_map(|m| hypr_modifier(m)).collect();
    if let Some((_, syms)) = MOD_KEYSYMS.iter().find(|(m, _)| *m == key) {
        hmods.extend(hypr_modifier(&key));
        return syms
            .iter()
            .map(|ks| hmods.iter().copied().chain([*ks]).collect::<Vec<_>>().join(" + "))
            .collect();
    }
    let ks = keysym(&key, Os::Linux);
    vec![hmods.iter().copied().chain([ks.as_str()]).collect::<Vec<_>>().join(" + ")]
}

/// Hyprland's modmask for these Hyprland modifier names.
pub fn mask(hmods: &[&str]) -> u32 {
    hmods.iter().filter_map(|m| MODMASK.iter().find(|(n, _)| n == m)).fold(0, |acc, (_, bit)| acc | bit)
}

/// Empty if the combo can be bound, else what is wrong with it.
pub fn validate(combo: &str, os: Os) -> String {
    let n = normalize(combo);
    if n.is_empty() {
        return String::new();
    }
    let keys: Vec<&str> = n.split('+').filter(|p| !is_mod(p)).collect();
    if keys.len() > 1 {
        return "Use one key plus modifiers (e.g. ctrl+shift+m)".to_owned();
    }
    match (keys.first(), os) {
        (Some(key), Os::Linux) if keysym(key, os).is_empty() => format!("Unknown key: {key}"),
        (Some(key), Os::Windows) if super::vk::of(key).is_none() => format!("Unknown key: {key}"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lum_fixtures::read_json;
    use serde_json::Value;

    fn strings(v: &Value) -> Vec<String> {
        v.as_array().expect("a list").iter().map(|s| s.as_str().expect("text").to_owned()).collect()
    }

    #[test]
    fn every_fixture_combo_matches() {
        let cases = read_json("hotkey_cases.json");
        for row in cases["combos"].as_array().expect("combos") {
            let combo = row["combo"].as_str().expect("combo");
            let text = |k: &str| row[k].as_str().expect("text").to_owned();
            assert_eq!(normalize(combo), text("normalize"), "normalize {combo:?}");
            assert_eq!(pretty(combo, Os::Linux), text("pretty_linux"), "pretty {combo:?}");
            assert_eq!(pretty(combo, Os::Windows), text("pretty_windows"), "pretty on Windows {combo:?}");
            assert_eq!(
                is_modifier(combo),
                row["is_modifier"].as_bool().expect("bool"),
                "is_modifier {combo:?}"
            );
            assert_eq!(keysym(combo, Os::Linux), text("keysym"), "keysym {combo:?}");
            assert_eq!(keysym(combo, Os::Windows), text("keysym_windows"), "keysym on Windows {combo:?}");
            assert_eq!(keysyms_of(combo, Os::Linux), strings(&row["keysyms_of"]), "keysyms_of {combo:?}");
            assert_eq!(validate(combo, Os::Linux), text("validate"), "validate {combo:?}");
            if !combo.is_empty() {
                assert_eq!(to_hyprland(combo), strings(&row["to_hyprland"]), "to_hyprland {combo:?}");
            }
        }
    }

    #[test]
    fn the_tables_match_keys_json() {
        let keys = read_json("keys.json");
        let aliases = keys["aliases"].as_object().expect("aliases");
        assert_eq!(aliases.len(), ALIASES.len());
        for (from, to) in ALIASES {
            assert_eq!(aliases[from], to);
        }
        let syms = keys["keysyms"].as_object().expect("keysyms");
        for (key, sym) in syms {
            assert_eq!(table_keysym(key).as_deref(), sym.as_str(), "{key}");
        }
        let buttons = keys["mouse_buttons"].as_array().expect("buttons");
        let ours = mouse_buttons();
        assert_eq!(buttons.len(), ours.len());
        for (theirs, (b, label)) in buttons.iter().zip(ours) {
            assert_eq!(theirs[0], b.as_str());
            assert_eq!(theirs[1], label.as_str());
        }
        for (m, bit) in MODMASK {
            assert_eq!(keys["modmask"][m], bit);
        }
    }

    #[test]
    fn windows_knows_its_own_keys() {
        assert_eq!(validate("ctrl+shift+space", Os::Windows), "");
        assert_eq!(validate("alt", Os::Windows), "");
        assert_eq!(validate("ctrl+f24", Os::Windows), "");
        assert_eq!(validate("ctrl+num enter", Os::Windows), "");
        assert_eq!(validate("ctrl+/", Os::Windows), "");
        assert_eq!(validate("mouse:275", Os::Windows), "Unknown key: mouse:275");
        assert_eq!(validate("ctrl+unknownkey", Os::Windows), "Unknown key: unknownkey");
    }

    #[test]
    fn parts_for_key_caps() {
        assert_eq!(pretty_parts("ctrl+shift+space", Os::Linux), ["Ctrl", "Shift", "Space"]);
        assert_eq!(pretty_parts("super + m", Os::Windows), ["Win", "M"]);
        assert!(pretty_parts("", Os::Linux).is_empty());
    }
}
