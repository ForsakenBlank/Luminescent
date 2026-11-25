//! Every colour maths, generation and gradient case from the Python app's fixtures.
#![allow(clippy::unwrap_used)]

use lum_fixtures::read_json;
use lum_theme::palette::{self, Palette, Role, Section};
use lum_theme::tones::{self, Adjust, Colours, Main};
use lum_theme::{Gradient, Rgb};
use serde_json::{Map, Value};

fn rgb(text: &str) -> Rgb {
    Rgb::parse(text).unwrap_or_else(|| panic!("{text} is not a colour"))
}

fn colours_of(value: &Value) -> Colours {
    value.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned())).collect()
}

fn main_of(value: &Value) -> Main {
    let get = |k: &str| value.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    Main {
        accent: get("accent"),
        accent2: get("accent2"),
        accent3: get("accent3"),
        background: get("background"),
        text: get("text"),
        success: get("success"),
        error: get("error"),
    }
}

fn near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 1e-9, "{what}: {a} against {b}");
}

/// The preset the fixtures call "dark preset" or "light preset".
fn preset(name: &str) -> Colours {
    palette::preset_colours(name.starts_with("light"))
}

#[test]
fn single_colour_maths() {
    let cases = read_json("tones_cases.json");
    for c in cases["colours"].as_array().unwrap() {
        let text = c["colour"].as_str().unwrap();
        let col = rgb(text);
        let (l, ch, h) = tones::oklch(col);
        let o = c["oklch"].as_array().unwrap();
        near(l, o[0].as_f64().unwrap(), text);
        near(ch, o[1].as_f64().unwrap(), text);
        near(h, o[2].as_f64().unwrap(), text);
        near(tones::tone(col), c["tone"].as_f64().unwrap(), text);
        assert_eq!(tones::is_light(col), c["is_light"].as_bool().unwrap(), "{text}");
        assert_eq!(tones::family(col), c["family"].as_str().unwrap(), "{text}");
        assert_eq!(tones::readable_on(col, None).hex(), c["readable_on"].as_str().unwrap(), "{text}");
        assert_eq!(
            tones::readable_on(col, Some(rgb("#7B6CF6"))).hex(),
            c["readable_on_hint"].as_str().unwrap(),
            "{text}"
        );
        assert_eq!(tones::with_tone(col, 20.0).hex(), c["with_tone_20"].as_str().unwrap(), "{text}");
        assert_eq!(tones::with_tone(col, 80.0).hex(), c["with_tone_80"].as_str().unwrap(), "{text}");
        assert_eq!(tones::shift_tone(col, 10.0).hex(), c["shift_tone_10"].as_str().unwrap(), "{text}");
        assert_eq!(tones::shift_tone(col, -25.0).hex(), c["shift_tone_-25"].as_str().unwrap(), "{text}");
        assert_eq!(tones::from_oklch(l, ch, h).hex(), c["from_oklch_back"].as_str().unwrap(), "{text}");
    }
}

#[test]
fn contrast_and_mixing() {
    for c in read_json("tones_cases.json")["pairs"].as_array().unwrap() {
        let (a, b) = (rgb(c["a"].as_str().unwrap()), rgb(c["b"].as_str().unwrap()));
        near(tones::contrast(a, b), c["contrast"].as_f64().unwrap(), "contrast");
        assert_eq!(tones::mix(a, b, 0.25).hex(), c["mix_0.25"].as_str().unwrap());
        assert_eq!(tones::mix(a, b, 0.5).hex(), c["mix_0.5"].as_str().unwrap());
    }
}

#[test]
fn tones_at_every_hue_and_chroma() {
    for c in read_json("tones_cases.json")["at_tone"].as_array().unwrap() {
        let got = tones::at_tone(
            c["hue"].as_f64().unwrap(),
            c["chroma"].as_f64().unwrap(),
            c["tone"].as_f64().unwrap(),
        );
        assert_eq!(got.hex(), c["colour"].as_str().unwrap(), "{c}");
    }
}

#[test]
fn what_counts_as_a_colour() {
    for c in read_json("tones_cases.json")["is_hex"].as_array().unwrap() {
        let got = c["value"].as_str().is_some_and(Rgb::is_hex);
        assert_eq!(got, c["is_hex"].as_bool().unwrap(), "{c}");
    }
}

#[test]
fn harmonies() {
    for c in read_json("tones_cases.json")["harmonies"].as_array().unwrap() {
        let get = |k: &str| c.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(rgb);
        let got = tones::harmony(
            rgb(c["accent"].as_str().unwrap()),
            c["kind"].as_str().unwrap(),
            c["light"].as_bool().unwrap(),
            get("background"),
            get("text"),
        );
        assert_eq!(got, main_of(&c["main"]), "{c}");
    }
}

#[test]
fn surprises_are_harmonies() {
    let mut rng = tones::Rng::new(7);
    for light in [false, true] {
        let m = tones::surprise(light, &mut rng);
        for key in ["accent", "accent2", "accent3", "background", "text"] {
            assert!(Rgb::parse_strict(m.get(key)).is_some(), "{key} in {m:?}");
        }
        assert_eq!(tones::is_light(rgb(&m.background)), light);
    }
}

#[test]
fn whole_set_changes() {
    let cases = read_json("tones_cases.json");
    for c in cases["adjust"].as_array().unwrap() {
        let a = &c["args"];
        let n = |k: &str| a.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let by = Adjust {
            hue: n("hue"),
            brightness: n("brightness"),
            contrast: n("contrast_"),
            saturation: n("saturation"),
            warmth: n("warmth"),
            pivot: a.get("pivot").and_then(Value::as_f64),
        };
        let got = tones::adjust(&preset(c["colours"].as_str().unwrap()), by);
        assert_eq!(got, colours_of(&c["result"]), "{a}");
    }
    for c in cases["flip"].as_array().unwrap() {
        let got = tones::flip(&preset(c["colours"].as_str().unwrap()));
        assert_eq!(got, colours_of(&c["result"]));
    }
    for c in cases["follow_hue"].as_array().unwrap() {
        let families: Vec<&str> =
            c["families"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let got = tones::follow_hue(&preset("dark"), &families, rgb(c["target"].as_str().unwrap()), &[]);
        assert_eq!(got, colours_of(&c["result"]), "{c}");
    }
    for c in cases["spread"].as_array().unwrap() {
        let values: Vec<Rgb> =
            c["values"].as_array().unwrap().iter().map(|v| rgb(v.as_str().unwrap())).collect();
        let got: Vec<String> =
            tones::spread(&values, c["light"].as_bool().unwrap(), 4.0).iter().map(|c| c.hex()).collect();
        let want: Vec<&str> = c["result"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(got, want, "{c}");
    }
}

#[test]
fn schemes_from_main_colours() {
    let cases = read_json("tones_cases.json");
    for c in cases["main_from_scheme"].as_array().unwrap() {
        assert_eq!(tones::main_from_scheme(&colours_of(&c["scheme"])), main_of(&c["main"]), "{c}");
    }
    for c in cases["caelestia_scheme"].as_array().unwrap() {
        let got = tones::caelestia_scheme(&main_of(&c["main"]), None);
        let want = colours_of(&c["scheme"]);
        assert_eq!(got.len(), want.len());
        for (k, v) in &want {
            assert_eq!(got.get(k), Some(v), "caelestia {k} for {}", c["main"]);
        }
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>(),
            "the order of the scheme"
        );
    }
    for c in cases["lum_roles"].as_array().unwrap() {
        let got = tones::lum_roles(&main_of(&c["main"]));
        assert_eq!(got, colours_of(&c["roles"]), "{c}");
    }
    for c in cases["main_from_roles"].as_array().unwrap() {
        assert_eq!(tones::main_from_roles(&colours_of(&c["roles"])), main_of(&c["main"]), "{c}");
    }
}

#[test]
fn gradients_are_cleaned_and_sampled() {
    for c in read_json("gradient_cases.json")["gradients"].as_array().unwrap() {
        let got = Gradient::clean(&c["input"]);
        if c["clean"].is_null() {
            assert!(got.is_none(), "{c}");
            continue;
        }
        let g = got.unwrap_or_else(|| panic!("{c} should be a gradient"));
        assert_eq!(g.to_value(), c["clean"], "{c}");
        if let Some(at) = c.get("at").and_then(Value::as_object) {
            for (t, want) in at {
                assert_eq!(g.at(t.parse().unwrap()).hex(), want.as_str().unwrap(), "{c} at {t}");
            }
        }
        if let Some(mid) = c.get("mid").and_then(Value::as_str) {
            assert_eq!(g.mid().hex(), mid, "{c}");
        }
    }
}

#[test]
fn linear_gradients_run_corner_to_corner() {
    for c in read_json("gradient_cases.json")["linear_line"].as_array().unwrap() {
        let g = Gradient::new(
            lum_theme::GradientKind::Linear,
            c["angle"].as_f64().unwrap(),
            vec![(0.0, Rgb::BLACK), (1.0, Rgb::WHITE)],
        )
        .unwrap();
        let want: Vec<f64> = c["line"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        for (got, want) in g.line().iter().zip(&want) {
            near(*got, *want, &format!("angle {}", c["angle"]));
        }
    }
}

#[test]
fn generated_themes_and_accent_roles() {
    let cases = read_json("gradient_cases.json");
    for c in cases["generate"].as_array().unwrap() {
        let got = palette::generate(rgb(c["accent"].as_str().unwrap()), c["light"].as_bool().unwrap());
        assert_eq!(got, colours_of(&c["colours"]), "{c}");
    }
    for c in cases["accent_roles"].as_array().unwrap() {
        let got: Colours =
            palette::accent_roles(rgb(c["accent"].as_str().unwrap()), rgb(c["bg"].as_str().unwrap()))
                .iter()
                .map(|(r, c)| (r.key().to_owned(), c.hex()))
                .collect();
        assert_eq!(got, colours_of(&c["roles"]), "{c}");
    }
    for c in cases["derive_primary_text"].as_array().unwrap() {
        let cols = colours_of(&c["colours"]);
        let get = |k: &str, default: &str| rgb(cols.get(k).map_or(default, String::as_str));
        let got = palette::derive_primary_text(get("primary", "#7B6CF6"), get("bg_base", "#131317"));
        assert_eq!(got.hex(), c["result"].as_str().unwrap());
    }
    for (hex, want) in cases["luminance"].as_object().unwrap() {
        near(rgb(hex).luminance(), want.as_f64().unwrap(), hex);
    }
    for c in cases["mix"].as_array().unwrap() {
        let got = rgb(c["a"].as_str().unwrap()).mix(rgb(c["b"].as_str().unwrap()), c["t"].as_f64().unwrap());
        assert_eq!(got.hex(), c["result"].as_str().unwrap(), "{c}");
    }
}

#[test]
fn colours_from_a_handful_of_roles() {
    for c in read_json("gradient_cases.json")["colours_from_roles"].as_array().unwrap() {
        let roles: Map<String, Value> = c["roles"].as_object().unwrap().clone();
        assert_eq!(palette::colours_from_roles(&roles), colours_of(&c["colours"]), "{c}");
    }
}

#[test]
fn looks_for_single_modules() {
    for c in read_json("gradient_cases.json")["module_sections"].as_array().unwrap() {
        let section = Section::clean(&c["input"]);
        if c["clean"].is_null() {
            assert!(section.is_none(), "{c}");
            continue;
        }
        let section = section.unwrap_or_else(|| panic!("{c} should be a look"));
        let want = &c["clean"];
        assert_eq!(
            section.accent.map(Rgb::hex).as_deref(),
            want.get("accent").and_then(Value::as_str),
            "{c}"
        );
        assert_eq!(section.radius, want.get("radius").and_then(Value::as_i64), "{c}");
        let cols: Colours = section.colours.iter().map(|(r, c)| (r.key().to_owned(), c.hex())).collect();
        assert_eq!(cols, colours_of(&want["colours"]), "{c}");
        assert_eq!(section.gradients.len(), want["gradients"].as_object().unwrap().len(), "{c}");

        let Some(pal) = c.get("palette") else { continue };
        let got = Palette::dark().with_section(&section);
        for role in Role::ALL {
            assert_eq!(got.get(role).hex(), pal[role.key()].as_str().unwrap(), "{} of {c}", role.key());
        }
        assert_eq!(got.radius, pal["radius"].as_i64().unwrap(), "{c}");
        assert_eq!(got.gradients.len(), pal["gradients"].as_object().unwrap().len(), "{c}");
    }
}
