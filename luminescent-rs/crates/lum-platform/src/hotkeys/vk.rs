//! Windows virtual-key codes for our key names, both ways. The table is plain data so it is tested on every
//! system; on Windows the punctuation keys come from the keyboard layout instead (a UK `#` is not a US one).

/// The extended-key bit tells the keypad's Enter from the main one.
pub const ENTER: u16 = 0x0D;

const NAMED: [(&str, u16); 25] = [
    ("space", 0x20),
    ("enter", ENTER),
    ("esc", 0x1B),
    ("tab", 0x09),
    ("backspace", 0x08),
    ("delete", 0x2E),
    ("insert", 0x2D),
    ("home", 0x24),
    ("end", 0x23),
    ("page up", 0x21),
    ("page down", 0x22),
    ("up", 0x26),
    ("down", 0x28),
    ("left", 0x25),
    ("right", 0x27),
    ("print screen", 0x2C),
    ("print", 0x2C),
    ("pause", 0x13),
    ("caps lock", 0x14),
    ("num lock", 0x90),
    ("scroll lock", 0x91),
    ("menu", 0x5D),
    ("apps", 0x5D),
    ("num *", 0x6A),
    ("num +", 0x6B),
];

const MORE: [(&str, u16); 3] = [("num -", 0x6D), ("num .", 0x6E), ("num /", 0x6F)];

/// The modifier keys, each side, and the name both sides share.
pub const MODIFIER_KEYS: [(&str, [u16; 2]); 4] =
    [("ctrl", [0xA2, 0xA3]), ("alt", [0xA4, 0xA5]), ("shift", [0xA0, 0xA1]), ("windows", [0x5B, 0x5C])];

/// The side-neutral codes Windows also reports for modifiers (`GetAsyncKeyState` takes them too).
const MODIFIER_ANY: [(&str, u16); 3] = [("ctrl", 0x11), ("alt", 0x12), ("shift", 0x10)];

const MEDIA: [(&str, u16); 16] = [
    ("xf86audioplay", 0xB3),
    ("xf86audiopause", 0xB3),
    ("xf86audiostop", 0xB2),
    ("xf86audionext", 0xB0),
    ("xf86audioprev", 0xB1),
    ("xf86audiomute", 0xAD),
    ("xf86audiolowervolume", 0xAE),
    ("xf86audioraisevolume", 0xAF),
    ("xf86back", 0xA6),
    ("xf86forward", 0xA7),
    ("xf86refresh", 0xA8),
    ("xf86search", 0xAA),
    ("xf86favorites", 0xAB),
    ("xf86homepage", 0xAC),
    ("xf86mail", 0xB4),
    ("xf86calculator", 0xB7),
];

/// The US layout's punctuation keys, for systems without a layout to ask.
const US_PUNCTUATION: [(char, u16); 11] = [
    (';', 0xBA),
    ('=', 0xBB),
    (',', 0xBC),
    ('-', 0xBD),
    ('.', 0xBE),
    ('/', 0xBF),
    ('`', 0xC0),
    ('[', 0xDB),
    ('\\', 0xDC),
    (']', 0xDD),
    ('\'', 0xDE),
];

/// The virtual key for one of our key names (already normalised), or None if Windows has no such key.
pub fn of(name: &str) -> Option<u16> {
    if let Some((_, [left, _])) = MODIFIER_KEYS.iter().find(|(n, _)| *n == name) {
        return Some(*left);
    }
    if name == "num enter" {
        return Some(ENTER);
    }
    if let Some((_, vk)) = NAMED.iter().chain(MORE.iter()).chain(MEDIA.iter()).find(|(n, _)| *n == name) {
        return Some(*vk);
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
        && name == format!("f{n}")
    {
        return Some(0x6F + n);
    }
    if let Some(n) = name.strip_prefix("num ").and_then(|n| n.parse::<u16>().ok())
        && n < 10
        && name == format!("num {n}")
    {
        return Some(0x60 + n);
    }
    let mut chars = name.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    match c {
        'a'..='z' => Some(c.to_ascii_uppercase() as u16),
        '0'..='9' => Some(c as u16),
        _ => punctuation(c),
    }
}

/// Our name for a key Windows reported. `extended` tells the keypad's Enter from the main one.
pub fn name(vk: u16, extended: bool) -> Option<String> {
    if let Some((n, _)) = MODIFIER_KEYS.iter().find(|(_, sides)| sides.contains(&vk)) {
        return Some((*n).to_owned());
    }
    if let Some((n, _)) = MODIFIER_ANY.iter().find(|(_, code)| *code == vk) {
        return Some((*n).to_owned());
    }
    if vk == ENTER {
        return Some(if extended { "num enter" } else { "enter" }.to_owned());
    }
    if let Some((n, _)) = NAMED.iter().chain(MORE.iter()).find(|(_, code)| *code == vk) {
        return Some((*n).to_owned());
    }
    match vk {
        0x41..=0x5A => return Some(((vk as u8) as char).to_ascii_lowercase().to_string()),
        0x30..=0x39 => return Some(((vk as u8) as char).to_string()),
        0x70..=0x87 => return Some(format!("f{}", vk - 0x6F)),
        0x60..=0x69 => return Some(format!("num {}", vk - 0x60)),
        _ => {}
    }
    if let Some((n, _)) = MEDIA.iter().find(|(_, code)| *code == vk) {
        return Some((*n).to_owned());
    }
    character(vk).map(|c| c.to_string())
}

/// Every code that means this name (both sides of a modifier), for asking Windows whether it is down.
pub fn codes(name: &str) -> Vec<u16> {
    match MODIFIER_KEYS.iter().find(|(n, _)| *n == name) {
        Some((_, sides)) => sides.to_vec(),
        None => of(name).into_iter().collect(),
    }
}

#[cfg(not(windows))]
fn punctuation(c: char) -> Option<u16> {
    US_PUNCTUATION.iter().find(|(p, _)| *p == c).map(|(_, vk)| *vk)
}

#[cfg(not(windows))]
fn character(vk: u16) -> Option<char> {
    US_PUNCTUATION.iter().find(|(_, code)| *code == vk).map(|(c, _)| *c)
}

#[cfg(windows)]
fn punctuation(c: char) -> Option<u16> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW;
    let mut units = [0u16; 2];
    let [unit] = c.encode_utf16(&mut units) else { return None };
    // SAFETY: no preconditions; it reads the current keyboard layout.
    let scan = unsafe { VkKeyScanW(*unit) };
    if scan == -1 {
        return US_PUNCTUATION.iter().find(|(p, _)| *p == c).map(|(_, vk)| *vk);
    }
    // the low byte is the key; a shifted character (`!` on `1`) still names that key
    Some((scan as u16) & 0xFF)
}

#[cfg(windows)]
fn character(vk: u16) -> Option<char> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW};
    // SAFETY: no preconditions; it reads the current keyboard layout.
    let code = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_CHAR) } & 0xFFFF;
    char::from_u32(code).filter(|c| !c.is_control()).map(|c| c.to_lowercase().next().unwrap_or(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_codes_go_both_ways() {
        for key in [
            "a",
            "z",
            "0",
            "9",
            "f1",
            "f24",
            "num 0",
            "num 9",
            "space",
            "enter",
            "esc",
            "page up",
            "num +",
            "num /",
            "/",
            ";",
            "ctrl",
            "alt",
            "shift",
            "windows",
            "xf86audioplay",
        ] {
            let vk = of(key).unwrap_or_else(|| panic!("{key} has a code"));
            assert_eq!(name(vk, false).as_deref(), Some(key), "{key}");
        }
        assert_eq!(name(ENTER, true).as_deref(), Some("num enter"));
        assert_eq!(name(0xA3, false).as_deref(), Some("ctrl"), "right ctrl is ctrl");
        assert_eq!(name(0x11, false).as_deref(), Some("ctrl"));
        assert_eq!(of("f25"), None);
        assert_eq!(of("mouse:275"), None);
        assert_eq!(codes("alt"), [0xA4, 0xA5]);
        assert_eq!(codes("k"), [0x4B]);
    }
}
