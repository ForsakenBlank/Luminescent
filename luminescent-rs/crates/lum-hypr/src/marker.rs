//! Invisible characters at the end of a window title, so the window rules can tell our windows apart. The
//! same characters as the Python app, which keeps both apps' rules interchangeable.

/// Do not focus it when it opens (zero-width space).
pub const NO_ACTIVATE: char = '\u{200b}';
/// Never give it the keyboard (zero-width non-joiner). Careful: Hyprland then sends it no mouse input either.
pub const NO_FOCUS: char = '\u{200c}';
/// Not pinned, so other programs' windows can be drawn above it (the overlay's backdrops; word joiner).
pub const NO_PIN: char = '\u{2060}';
/// Blur what is behind it (function application).
pub const BLUR: char = '\u{2061}';
/// A glass panel: blurred behind, with rounded corners (invisible times).
pub const GLASS: char = '\u{2062}';

const ALL: [char; 5] = [NO_ACTIVATE, NO_FOCUS, NO_PIN, BLUR, GLASS];

/// The title without the markers at its end.
pub fn strip(title: &str) -> &str {
    title.trim_end_matches(ALL)
}

pub fn has(title: &str, marker: char) -> bool {
    title.contains(marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_come_off_the_end_only() {
        let t = format!("Luminescent Notes{NO_PIN}{NO_ACTIVATE}");
        assert_eq!(strip(&t), "Luminescent Notes");
        assert!(has(&t, NO_ACTIVATE) && has(&t, NO_PIN) && !has(&t, NO_FOCUS));
        assert_eq!(strip("plain"), "plain");
    }
}
