//! Geist for the UI and Geist Mono for hotkeys, counts and labels, bundled in the binary (the same files as
//! `assets/fonts`). egui has no font weights, so each weight is a family of its own.
//!
//! Qt finds symbols Geist lacks (✕, ✓, ✦, ⚙) in the system's fonts; egui only has what it is given. So every
//! family falls back to Luminescent Symbols (a subset of DejaVu Sans, `luminescent-rs/assets/fonts`), then to
//! egui's own fonts with their black and white emoji, which cover the emoji the Python app uses as icons.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const GEIST: [(&str, &[u8]); 3] = [
    ("Geist", include_bytes!("../../../../assets/fonts/Geist-Regular.ttf")),
    ("Geist Medium", include_bytes!("../../../../assets/fonts/Geist-Medium.ttf")),
    ("Geist SemiBold", include_bytes!("../../../../assets/fonts/Geist-SemiBold.ttf")),
];

const GEIST_MONO: [(&str, &[u8]); 3] = [
    ("Geist Mono", include_bytes!("../../../../assets/fonts/GeistMono-Regular.ttf")),
    ("Geist Mono Medium", include_bytes!("../../../../assets/fonts/GeistMono-Medium.ttf")),
    ("Geist Mono SemiBold", include_bytes!("../../../../assets/fonts/GeistMono-SemiBold.ttf")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Weight {
    #[default]
    Regular,
    Medium,
    SemiBold,
}

impl Weight {
    /// From a CSS-style number, as the Python code writes them (`qfont(px, 600)`).
    pub fn from_css(weight: u16) -> Weight {
        match weight {
            0..=449 => Weight::Regular,
            450..=549 => Weight::Medium,
            _ => Weight::SemiBold,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The family for a weight of Geist or Geist Mono.
pub fn family(weight: Weight, mono: bool) -> FontFamily {
    match (weight, mono) {
        (Weight::Regular, false) => FontFamily::Proportional,
        (Weight::Regular, true) => FontFamily::Monospace,
        (w, false) => FontFamily::Name(GEIST[w.index()].0.into()),
        (w, true) => FontFamily::Name(GEIST_MONO[w.index()].0.into()),
    }
}

/// A font at `px` logical pixels (already scaled by the caller if it should follow the text size).
pub fn font(px: f32, weight: Weight, mono: bool) -> FontId {
    FontId::new(px, family(weight, mono))
}

const SYMBOLS: (&str, &[u8]) =
    ("Luminescent Symbols", include_bytes!("../../../assets/fonts/LuminescentSymbols.ttf"));

/// The fonts, each falling back to the symbols and then to egui's own fonts.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(SYMBOLS.0.to_owned(), Arc::new(FontData::from_static(SYMBOLS.1)));
    let mut fallbacks = vec![SYMBOLS.0.to_owned()];
    fallbacks.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    for (list, mono) in [(&GEIST, false), (&GEIST_MONO, true)] {
        for (i, (name, bytes)) in list.iter().enumerate() {
            fonts.font_data.insert((*name).to_owned(), Arc::new(FontData::from_static(bytes)));
            // a heavier weight falls back to the regular one, then to egui's fonts
            let mut chain = vec![(*name).to_owned()];
            if i > 0 {
                chain.push(list[0].0.to_owned());
            }
            chain.extend(fallbacks.iter().cloned());
            let weight = [Weight::Regular, Weight::Medium, Weight::SemiBold][i];
            fonts.families.insert(family(weight, mono), chain);
        }
    }
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_weight_has_a_family_that_starts_with_its_own_file() {
        let defs = definitions();
        for mono in [false, true] {
            for weight in [Weight::Regular, Weight::Medium, Weight::SemiBold] {
                let chain = &defs.families[&family(weight, mono)];
                assert!(chain[0].starts_with(if mono { "Geist Mono" } else { "Geist" }), "{chain:?}");
                assert!(chain.contains(&SYMBOLS.0.to_owned()), "the symbols fill in: {chain:?}");
                assert!(chain.len() > 3, "egui's fonts come last: {chain:?}");
            }
        }
        assert_eq!(Weight::from_css(400), Weight::Regular);
        assert_eq!(Weight::from_css(500), Weight::Medium);
        assert_eq!(Weight::from_css(700), Weight::SemiBold);
    }
}
