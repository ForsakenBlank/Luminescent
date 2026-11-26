//! The theme being edited: the fields of the General and "When it is applied" pages, and reading and writing
//! its colours and the looks of single parts.

use lum_theme::theme::RoleValue;
use lum_theme::{Gradient, Rgb, Role, Section, Theme};
use serde_json::{Map, Value, json};

use super::ThemeEditor;
use crate::picker::Picked;

/// What the text boxes, sliders and ticks of the editor hold; they go back into the theme in `result`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fields {
    pub name: String,
    pub font: String,
    pub radius: f64,
    pub glow: bool,
    pub background_image: String,
    /// 0 to 100.
    pub image_opacity: f64,
    pub icon: String,
    pub hotkey: String,
    pub wallpaper: String,
    pub profile: String,
}

impl Fields {
    pub fn of(t: &Theme) -> Fields {
        Fields {
            name: t.name().to_owned(),
            font: t.font_family().to_owned(),
            radius: t.radius() as f64,
            glow: t.glow(),
            background_image: t.background_image().to_owned(),
            image_opacity: (t.image_opacity() * 100.0).round(),
            icon: t.icon().to_owned(),
            hotkey: t.hotkey().to_owned(),
            wallpaper: t.wallpaper().to_owned(),
            profile: t.profile().to_owned(),
        }
    }

    fn put(&self, t: &mut Theme, hotkey_ok: bool) {
        let name = self.name.trim();
        if !name.is_empty() {
            t.set("name", json!(name));
        }
        t.set("font_family", json!(self.font));
        t.set("radius", json!(self.radius as i64));
        t.set("glow", json!(self.glow));
        t.set("background_image", json!(self.background_image.trim()));
        t.set("image_opacity", json!(self.image_opacity / 100.0));
        t.set("icon", json!(self.icon.trim()));
        if hotkey_ok && !self.hotkey.is_empty() {
            t.set("hotkey", json!(self.hotkey));
        } else {
            t.0.remove("hotkey");
        }
        t.set("wallpaper", json!(self.wallpaper.trim()));
        t.set("profile", json!(self.profile));
    }
}

pub fn picked_of(value: RoleValue) -> Picked {
    match value {
        RoleValue::Colour(c) => Picked::Colour(c),
        RoleValue::Gradient(g) => Picked::Gradient(g),
    }
}

/// What a stored colour is: `"#RRGGBB"` or a gradient object.
pub fn picked_from_value(v: &Value) -> Option<Picked> {
    match v {
        Value::String(s) => Rgb::parse_strict(s).map(Picked::Colour),
        Value::Object(_) => Gradient::clean(v).map(Picked::Gradient),
        _ => None,
    }
}

pub fn value_of(p: &Picked) -> Value {
    match p {
        Picked::Colour(c) => json!(c.hex()),
        Picked::Gradient(g) => g.to_value(),
    }
}

fn modules_mut(t: &mut Theme) -> &mut Map<String, Value> {
    let slot = t.0.entry("modules").or_insert_with(|| json!({}));
    if !slot.is_object() {
        *slot = json!({});
    }
    slot.as_object_mut().unwrap_or_else(|| unreachable!("it was just made an object"))
}

impl ThemeEditor {
    /// The value a colour role has in the theme: its gradient, its colour or Dark's.
    pub(super) fn global(&self, role: Role) -> Picked {
        picked_of(self.t.role_value(role))
    }

    /// Sets a role to a colour or a gradient; a gradient's plain colour is its middle.
    pub(super) fn set_global(&mut self, role: Role, value: &Picked) {
        let mut gradients = self.t.gradients();
        gradients.retain(|(k, _)| k != role.key());
        match value {
            Picked::Colour(c) => {
                self.t.colours_mut().insert(role.key().to_owned(), json!(c.hex()));
            }
            Picked::Gradient(g) => {
                self.t.colours_mut().insert(role.key().to_owned(), json!(g.mid().hex()));
                gradients.push((role.key().to_owned(), g.clone()));
            }
        }
        self.t.set_gradients(&gradients);
        self.change();
    }

    /// `set_global`, noted in "Recently changed".
    pub(super) fn set_global_tracked(&mut self, role: Role, value: &Picked) {
        let before = self.global(role);
        if &before == value {
            return;
        }
        self.set_global(role, value);
        let one = |v: Value| Map::from_iter([(role.key().to_owned(), v)]);
        lum_theme::history::note_change(
            &mut self.recents,
            "lum",
            one(value_of(&before)),
            one(value_of(value)),
            role.label(),
            Map::new(),
        );
    }

    pub(super) fn section(&self, key: &str) -> Section {
        self.t.section(key).unwrap_or_default()
    }

    /// What one part of the theme gives a role of its own, if anything.
    pub(super) fn section_value(&self, key: &str, role: Role) -> Option<Picked> {
        let s = self.section(key);
        match s.gradients.iter().find(|(r, _)| *r == role) {
            Some((_, g)) => Some(Picked::Gradient(g.clone())),
            None => s.colour(role).map(Picked::Colour),
        }
    }

    pub(super) fn edit_section(&mut self, key: &str, f: impl FnOnce(&mut Section)) {
        let mut s = self.section(key);
        f(&mut s);
        let value = s.to_value();
        let modules = modules_mut(&mut self.t);
        if Section::clean(&value).is_some() {
            modules.insert(key.to_owned(), value);
        } else {
            modules.shift_remove(key);
        }
        self.change();
    }

    pub(super) fn set_section_role(&mut self, key: &str, role: Role, value: Option<&Picked>) {
        self.edit_section(key, |s| {
            s.colours.retain(|(r, _)| *r != role);
            s.gradients.retain(|(r, _)| *r != role);
            match value {
                Some(Picked::Colour(c)) => s.colours.push((role, *c)),
                Some(Picked::Gradient(g)) => s.gradients.push((role, g.clone())),
                None => {}
            }
        });
    }

    pub(super) fn set_section_accent(&mut self, key: &str, accent: Option<Rgb>) {
        self.edit_section(key, |s| s.accent = accent);
    }

    pub(super) fn set_section_radius(&mut self, key: &str, radius: Option<i64>) {
        self.edit_section(key, |s| s.radius = radius);
    }

    pub(super) fn clear_section(&mut self, key: &str) {
        modules_mut(&mut self.t).shift_remove(key);
        self.change();
    }

    /// Copies the look of another part, or clears this one (`None`).
    pub(super) fn copy_section(&mut self, key: &str, from: Option<&str>) {
        match from.and_then(|f| modules_mut(&mut self.t).get(f).cloned()) {
            Some(value) => {
                modules_mut(&mut self.t).insert(key.to_owned(), value);
            }
            None => {
                modules_mut(&mut self.t).shift_remove(key);
            }
        }
        self.change();
    }

    /// The theme as it would be saved now: the fields put back in and every part's look cleaned.
    pub(super) fn result(&self) -> Theme {
        let mut t = self.t.clone();
        self.fields.put(&mut t, self.key_warning.is_empty());
        let sections: Map<String, Value> = t.sections().into_iter().map(|(k, s)| (k, s.to_value())).collect();
        t.0.insert("modules".into(), Value::Object(sections));
        let gradients = t.gradients();
        t.set_gradients(&gradients);
        t
    }
}
