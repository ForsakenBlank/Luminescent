//! A colour role can be a gradient as well, from `core/theme.py`:
//! `"gradients": {"bg_base": {"type": "linear", "angle": 135, "stops": [[0, "#1E1B3A"], [1, "#0E0E11"]]}}`.
//! The role keeps a plain colour too (the middle of the gradient), for everything that needs one colour:
//! text contrast, mixing, lines painted by hand.

use serde_json::{Map, Value, json};

use crate::colour::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    /// "conical" in themes.json, "Sweep" in the theme editor.
    Sweep,
}

impl GradientKind {
    pub const ALL: [GradientKind; 3] = [GradientKind::Linear, GradientKind::Radial, GradientKind::Sweep];

    pub fn key(self) -> &'static str {
        match self {
            GradientKind::Linear => "linear",
            GradientKind::Radial => "radial",
            GradientKind::Sweep => "conical",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GradientKind::Linear => "Linear",
            GradientKind::Radial => "Radial",
            GradientKind::Sweep => "Sweep",
        }
    }

    fn from_key(key: &str) -> Option<GradientKind> {
        GradientKind::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// `angle` works like CSS: the direction it runs in, 0 = upwards, 90 = to the right, 180 = down.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub angle: f64,
    /// Sorted by position, at least two.
    pub stops: Vec<(f64, Rgb)>,
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

impl Gradient {
    /// The gradient with its stops sorted and checked, or `None` when it is not one (fewer than two stops).
    pub fn new(kind: GradientKind, angle: f64, stops: Vec<(f64, Rgb)>) -> Option<Gradient> {
        if stops.len() < 2 {
            return None;
        }
        let mut stops: Vec<(f64, Rgb)> = stops.into_iter().map(|(p, c)| (p.clamp(0.0, 1.0), c)).collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some(Gradient { kind, angle: angle.rem_euclid(360.0), stops })
    }

    /// `theme.clean_gradient`: bad stops are dropped, an unknown type becomes linear, a bad angle 90.
    pub fn clean(value: &Value) -> Option<Gradient> {
        let spec = value.as_object()?;
        let stops: Vec<(f64, Rgb)> = spec
            .get("stops")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|stop| {
                let pair = stop.as_array()?;
                let pos = number(pair.first()?)?;
                let colour = Rgb::parse_strict(pair.get(1)?.as_str()?)?;
                Some((pos, colour))
            })
            .collect();
        let kind = spec.get("type").and_then(Value::as_str).and_then(GradientKind::from_key);
        let angle = spec.get("angle").map_or(Some(90.0), number).unwrap_or(90.0);
        Gradient::new(
            kind.unwrap_or(GradientKind::Linear),
            if angle.is_finite() { angle } else { 90.0 },
            stops,
        )
    }

    pub fn to_value(&self) -> Value {
        let stops: Vec<Value> = self.stops.iter().map(|(p, c)| json!([p, c.hex()])).collect();
        json!({"type": self.kind.key(), "angle": self.angle, "stops": stops})
    }

    /// The colour at position `t` (0 to 1).
    pub fn at(&self, t: f64) -> Rgb {
        let first = self.stops[0];
        if t <= first.0 {
            return first.1;
        }
        for pair in self.stops.windows(2) {
            let ((p0, c0), (p1, c1)) = (pair[0], pair[1]);
            if t <= p1 {
                return c0.mix(c1, if p1 <= p0 { 0.0 } else { (t - p0) / (p1 - p0) });
            }
        }
        self.stops[self.stops.len() - 1].1
    }

    /// One colour that stands for the gradient: its average along the way.
    pub fn mid(&self) -> Rgb {
        let samples: Vec<Rgb> = (0..9).map(|i| self.at(f64::from(i) / 8.0)).collect();
        let n = samples.len() as f64;
        let avg = |f: fn(&Rgb) -> u8| samples.iter().map(|c| f64::from(f(c))).sum::<f64>() / n;
        let ch = |v: f64| v.round_ties_even() as u8;
        Rgb::new(ch(avg(|c| c.r)), ch(avg(|c| c.g)), ch(avg(|c| c.b)))
    }

    /// Start and end of a linear gradient across a unit box, long enough that the first and last colours reach
    /// the corners (`theme._line`).
    pub fn line(&self) -> [f64; 4] {
        let a = self.angle.to_radians();
        let (dx, dy) = (a.sin(), -a.cos());
        let span = (dx.abs() + dy.abs()) / 2.0;
        [0.5 - dx * span, 0.5 - dy * span, 0.5 + dx * span, 0.5 + dy * span]
    }

    /// A second colour that goes with `first`, for the ◐ button that turns a colour into a gradient.
    pub fn from_colour(first: Rgb) -> Gradient {
        Gradient {
            kind: GradientKind::Linear,
            angle: 135.0,
            stops: vec![(0.0, first), (1.0, crate::partner(first))],
        }
    }
}

/// `theme.clean_gradients`: the usable gradients of a `"gradients"` object, in their order.
pub fn clean_all(value: Option<&Value>) -> Vec<(String, Gradient)> {
    let Some(map) = value.and_then(Value::as_object) else { return Vec::new() };
    map.iter().filter_map(|(role, g)| Gradient::clean(g).map(|g| (role.clone(), g))).collect()
}

pub fn gradients_to_value(gradients: &[(String, Gradient)]) -> Value {
    Value::Object(gradients.iter().map(|(role, g)| (role.clone(), g.to_value())).collect::<Map<_, _>>())
}
