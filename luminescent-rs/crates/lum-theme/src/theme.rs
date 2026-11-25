//! One theme as `themes.json` holds it. The data stays the JSON object it was read as, so keys this version
//! does not know (Caelestia's colours, terminal, glass, borders, a key another version added) are kept and
//! written back in their place.

use serde_json::{Map, Value, json};

use crate::colour::Rgb;
use crate::gradient::{Gradient, clean_all, gradients_to_value};
use crate::palette::{DEFAULT_FONT, Palette, Role, Section, preset_colours};

/// The look keys a palette is made from, next to the colours (`themes.LOOK_KEYS`).
pub const LOOK_KEYS: [&str; 5] = ["font_family", "radius", "glow", "background_image", "image_opacity"];

#[derive(Debug, Clone, PartialEq)]
pub struct Theme(pub Map<String, Value>);

impl Theme {
    /// `themes._builtin`: the keys every theme has, with the dark colours.
    pub fn builtin(id: &str, name: &str) -> Theme {
        let mut m = Map::new();
        m.insert("id".into(), json!(id));
        m.insert("name".into(), json!(name));
        m.insert("folder".into(), json!(""));
        m.insert("builtin".into(), json!(true));
        let colours = preset_colours(id == "light");
        m.insert("colours".into(), Value::Object(colours.into_iter().map(|(k, v)| (k, json!(v))).collect()));
        m.insert("font_family".into(), json!(DEFAULT_FONT));
        m.insert("radius".into(), json!(10));
        m.insert("glow".into(), json!(true));
        m.insert("icon".into(), json!(""));
        m.insert("background_image".into(), json!(""));
        m.insert("image_opacity".into(), json!(0.35));
        m.insert("wallpaper".into(), json!(""));
        m.insert("profile".into(), json!(""));
        Theme(m)
    }

    /// `themes.new_theme`: a copy of `base` (or of Dark) with a new id and name. A copy does not take the key of
    /// the theme it came from.
    pub fn new(id: String, name: &str, base: Option<&Theme>) -> Theme {
        let mut t = base.cloned().unwrap_or_else(|| Theme::builtin("dark", name));
        t.0.insert("id".into(), json!(id));
        t.0.insert("name".into(), json!(name));
        t.0.insert("builtin".into(), json!(false));
        t.0.shift_remove("hotkey");
        t
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn str(&self, key: &str) -> &str {
        self.0.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.0.insert(key.to_owned(), value);
    }

    pub fn id(&self) -> &str {
        self.str("id")
    }

    pub fn name(&self) -> &str {
        self.0.get("name").and_then(Value::as_str).unwrap_or("Theme")
    }

    pub fn builtin_flag(&self) -> bool {
        truthy(self.0.get("builtin"))
    }

    pub fn folder(&self) -> &str {
        self.str("folder")
    }

    /// The key that switches straight to this theme, empty for none.
    pub fn hotkey(&self) -> &str {
        self.str("hotkey")
    }

    pub fn profile(&self) -> &str {
        self.str("profile")
    }

    pub fn wallpaper(&self) -> &str {
        self.str("wallpaper")
    }

    pub fn icon(&self) -> &str {
        self.str("icon")
    }

    pub fn background_image(&self) -> &str {
        self.str("background_image")
    }

    pub fn font_family(&self) -> &str {
        match self.str("font_family") {
            "" => DEFAULT_FONT,
            f => f,
        }
    }

    pub fn radius(&self) -> i64 {
        self.0.get("radius").and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))).unwrap_or(10)
    }

    pub fn glow(&self) -> bool {
        self.0.get("glow").is_none_or(|v| truthy(Some(v)))
    }

    pub fn image_opacity(&self) -> f64 {
        self.0.get("image_opacity").and_then(Value::as_f64).unwrap_or(0.35)
    }

    pub fn colours(&self) -> Map<String, Value> {
        self.0.get("colours").and_then(Value::as_object).cloned().unwrap_or_default()
    }

    pub fn colours_mut(&mut self) -> &mut Map<String, Value> {
        let slot = self.0.entry("colours").or_insert_with(|| json!({}));
        if !slot.is_object() {
            *slot = json!({});
        }
        slot.as_object_mut().unwrap_or_else(|| unreachable!("it was just made an object"))
    }

    pub fn gradients(&self) -> Vec<(String, Gradient)> {
        clean_all(self.0.get("gradients"))
    }

    pub fn set_gradients(&mut self, gradients: &[(String, Gradient)]) {
        self.0.insert("gradients".into(), gradients_to_value(gradients));
    }

    /// The palette this theme gives, without switching to it (`themes.palette_values` and `palette_from`).
    pub fn palette(&self) -> Palette {
        let mut values = self.colours();
        for key in LOOK_KEYS {
            if let Some(v) = self.0.get(key) {
                values.insert(key.to_owned(), v.clone());
            }
        }
        let gradients =
            self.0.get("gradients").filter(|v| truthy(Some(v))).cloned().unwrap_or_else(|| json!({}));
        values.insert("gradients".into(), gradients);
        Palette::from_values(&values)
    }

    /// The looks of single modules, notifications and warnings, cleaned, in the order they are stored.
    pub fn sections(&self) -> Vec<(String, Section)> {
        let Some(map) = self.0.get("modules").and_then(Value::as_object) else { return Vec::new() };
        map.iter().filter_map(|(k, v)| Section::clean(v).map(|s| (k.clone(), s))).collect()
    }

    pub fn section(&self, key: &str) -> Option<Section> {
        self.0.get("modules").and_then(|m| m.get(key)).and_then(Section::clean)
    }

    /// The value one role has in this theme: its gradient, else its colour, else Dark's.
    pub fn role_value(&self, role: Role) -> RoleValue {
        if let Some((_, g)) = self.gradients().into_iter().find(|(k, _)| k == role.key()) {
            return RoleValue::Gradient(g);
        }
        let colour = self.colours().get(role.key()).and_then(Value::as_str).and_then(Rgb::parse);
        RoleValue::Colour(colour.unwrap_or_else(|| Palette::dark().get(role)))
    }
}

/// What one colour role holds: a plain colour or a gradient.
#[derive(Debug, Clone, PartialEq)]
pub enum RoleValue {
    Colour(Rgb),
    Gradient(Gradient),
}

impl RoleValue {
    /// A plain colour for it: a gradient's middle.
    pub fn solid(&self) -> Rgb {
        match self {
            RoleValue::Colour(c) => *c,
            RoleValue::Gradient(g) => g.mid(),
        }
    }
}

/// Python truthiness of a JSON value (missing, null, false, 0, "" and empty containers are false).
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}
