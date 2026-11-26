//! The overlay's pages and the order the page switcher shows them in (`NAV_SECTIONS`, `EXTRA_PAGES` and
//! `nav_order` in the Python app).

/// The pages the switcher has by default, in that order.
pub const NAV_SECTIONS: &[(&str, &str)] = &[
    ("dashboard", "Dashboard"),
    ("automation", "Automation"),
    ("themes", "Themes"),
    ("clipboard", "Clipboard"),
    ("settings", "Settings"),
];

/// Pages you can add to the switcher (Settings → Overlay → Navigation bar).
pub const EXTRA_PAGES: &[(&str, &str)] = &[
    ("notes", "Notes"),
    ("timers", "Timers"),
    ("calculator", "Calculator"),
    ("today", "Today"),
    ("stats", "Usage"),
    ("calendar", "Calendar"),
    ("hearing", "Hearing Guard"),
    ("sysmon", "System Monitor"),
    ("media", "Media"),
    ("syscontrol", "System Settings"),
    ("performance", "Performance"),
];

/// These are always in the switcher, whatever the settings say.
pub const REQUIRED_PAGES: &[&str] = &["dashboard", "settings"];

/// Pages that fill the screen instead of sitting in the panel below the switcher.
pub const FULL_PAGES: &[&str] = &["dashboard", "automation"];

pub fn all_pages() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    NAV_SECTIONS.iter().chain(EXTRA_PAGES)
}

/// The page's name in the switcher, or `None` for a key that is not a page.
pub fn title(key: &str) -> Option<&'static str> {
    all_pages().find(|(k, _)| *k == key).map(|(_, name)| *name)
}

pub fn is_full(key: &str) -> bool {
    FULL_PAGES.contains(&key)
}

/// The pages in the switcher, in order, from `ui.nav_order`: unknown keys and repeats dropped, Dashboard put
/// first and Settings last when they are missing.
pub fn nav_order(wanted: &[String]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    for key in wanted {
        if title(key).is_some() && !order.contains(key) {
            order.push(key.clone());
        }
    }
    for key in REQUIRED_PAGES {
        if !order.iter().any(|k| k == key) {
            let at = if *key == "dashboard" { 0 } else { order.len() };
            order.insert(at, (*key).to_owned());
        }
    }
    order
}

/// The settings key of a page's "Jump to" hotkey.
pub fn hotkey_setting(key: &str) -> String {
    if key == "settings" { "ui.hotkey_settings_page".to_owned() } else { format!("ui.hotkey_{key}") }
}

/// A page that is not built yet: a line in the middle of its space, so the switcher still goes everywhere.
pub fn placeholder(ui: &mut egui::Ui, key: &str) {
    let name = title(key).unwrap_or(key);
    let rect = ui.max_rect();
    let tokens = crate::Tokens::of(ui.ctx());
    let text = format!("{name} is not in this build yet.");
    let galley = ui.painter().layout(
        text,
        tokens.font(12.0, crate::fonts::Weight::Regular, false),
        tokens.p.text_muted,
        (rect.width() - 48.0).max(40.0),
    );
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, tokens.p.text_muted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_owned()).collect()
    }

    #[test]
    fn the_order_keeps_what_it_knows_and_adds_what_it_must() {
        assert_eq!(
            nav_order(&keys(&["themes", "notes", "nope", "themes"])),
            keys(&["dashboard", "themes", "notes", "settings"])
        );
        assert_eq!(nav_order(&[]), keys(&["dashboard", "settings"]));
        assert_eq!(nav_order(&keys(&["settings", "dashboard"])), keys(&["settings", "dashboard"]));
    }

    #[test]
    fn page_names_and_hotkeys() {
        assert_eq!(title("stats"), Some("Usage"));
        assert_eq!(title("gallery"), None);
        assert!(is_full("automation") && !is_full("themes"));
        assert_eq!(hotkey_setting("settings"), "ui.hotkey_settings_page");
        assert_eq!(hotkey_setting("themes"), "ui.hotkey_themes");
    }
}
