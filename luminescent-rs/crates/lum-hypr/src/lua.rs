//! The Lua we run inside Hyprland, and the dispatcher text. Everything here is the Python app's text from
//! `core/hypr.py`, character for character: it is proven on real machines, and `hypr_cases.json` checks it
//! stays that way. The only change is the table name (see [`Names`]), swapped in after the text is built.
//!
//! Binds are never removed, only disabled. Hyprland 0.56 crashes (use after free in `CLuaKeybind`) when
//! `:remove()` is called on a bind that shares its key with another one, because removing the first already
//! deleted both: exactly the case of a hold key's press and release binds. So every distinct (keys, release,
//! non consuming) gets one "slot" that lives until the next config reload, and (un)binding just points the
//! slot at a bind id and enables or disables it.

use crate::marker;
use std::path::PathBuf;

/// Bump when the rules in [`bootstrap`] change: the old ones are switched off.
pub const RULES_VERSION: u32 = 3;
/// Every bind of ours carries this description, which is how [`crate::Hypr::foreign_binds`] tells them apart.
pub const BIND_DESC: &str = "Luminescent";
/// The Python app's table.
pub const PYTHON_TABLE: &str = "LUMI";

/// What our Lua is called inside Hyprland and where it reports back.
///
/// The Python app keeps its slots, rules and FIFO path in the global `LUMI`. Both apps may run at once until
/// phase 15, and sharing the table would let each one's bootstrap switch off the other's binds and point the
/// other's binds at its own FIFO, so the Rust app uses its own table and FIFO (D27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    /// The Lua global holding everything (`LUMI` for the Python app).
    pub table: String,
    /// The window class our rules match.
    pub app_id: String,
    /// Where our binds write their ids.
    pub fifo: PathBuf,
}

impl Names {
    /// The Rust app's names, under `runtime` (`$XDG_RUNTIME_DIR`).
    pub fn app(runtime: impl Into<PathBuf>) -> Names {
        Names {
            table: "LUMRS".to_owned(),
            app_id: "luminescent".to_owned(),
            fifo: runtime.into().join("luminescent").join("events-rs.fifo"),
        }
    }

    /// The Python app's names, to check the text against its fixtures.
    pub fn python(fifo: impl Into<PathBuf>) -> Names {
        Names { table: PYTHON_TABLE.to_owned(), app_id: "luminescent".to_owned(), fifo: fifo.into() }
    }

    fn own(&self, code: String) -> String {
        if self.table == PYTHON_TABLE { code } else { code.replace(PYTHON_TABLE, &self.table) }
    }

    fn class(&self) -> String {
        lstr(&format!("^{}$", self.app_id))
    }
}

/// A Lua string literal.
pub fn lstr(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

fn title_has(m: char) -> String {
    lstr(&format!("^.*{m}.*$"))
}

/// Installs the table, the bind slots and our window rules. Run it before the first window opens and again
/// after every config reload (which throws away everything added at runtime).
pub fn bootstrap(names: &Names) -> String {
    let cls = names.class();
    let rules = [
        format!(
            "{{ match = {{ class = {cls} }}, float = true, pin = true, decorate = false, no_anim = true, \
             no_shadow = true, no_blur = true, no_dim = true, border_size = 0, rounding = 0, \
             opacity = \"1 override 1 override\" }}"
        ),
        format!(
            "{{ match = {{ class = {cls}, title = {} }}, no_initial_focus = true }}",
            title_has(marker::NO_ACTIVATE)
        ),
        format!(
            "{{ match = {{ class = {cls}, title = {} }}, no_initial_focus = true, no_focus = true }}",
            title_has(marker::NO_FOCUS)
        ),
        format!("{{ match = {{ class = {cls}, title = {} }}, pin = false }}", title_has(marker::NO_PIN)),
        format!("{{ match = {{ class = {cls}, title = {} }}, no_blur = false }}", title_has(marker::BLUR)),
    ];
    let fifo = names.fifo.to_string_lossy().replace('\'', "");
    let rules = rules.iter().map(|r| format!("LUMI.rules[#LUMI.rules + 1] = hl.window_rule({r})"));
    let code = [
        "LUMI = LUMI or {} ".to_owned(),
        "LUMI.slots = LUMI.slots or {} ".to_owned(),
        "for _, s in pairs(LUMI.slots) do s.id = nil pcall(function() s.bind:set_enabled(false) end) end ".to_owned(),
        format!("LUMI.fifo = {} ", lstr(&fifo)),
        "function LUMI.send(msg) ".to_owned(),
        // (a command starting with "[" would be read as exec rules, hence `test`)
        r#"hl.exec_cmd("test -p '" .. LUMI.fifo .. "' && printf '%s\\n' '" .. msg .. "' 1<>'" .. LUMI.fifo .. "'") "#
            .to_owned(),
        "end ".to_owned(),
        "function LUMI.bind(slot, id, keys, opts) ".to_owned(),
        "local s = LUMI.slots[slot] ".to_owned(),
        "if s then s.id = id s.bind:set_enabled(true) return end ".to_owned(),
        "s = { id = id } ".to_owned(),
        "s.bind = hl.bind(keys, function() if s.id then LUMI.send(s.id) end end, opts) ".to_owned(),
        "LUMI.slots[slot] = s ".to_owned(),
        "end ".to_owned(),
        "function LUMI.unbind(id) ".to_owned(),
        "for _, s in pairs(LUMI.slots) do if s.id == id then s.id = nil s.bind:set_enabled(false) end end ".to_owned(),
        "end ".to_owned(),
        "function LUMI.unbind_all() ".to_owned(),
        "for _, s in pairs(LUMI.slots) do if s.id then s.id = nil s.bind:set_enabled(false) end end ".to_owned(),
        "end ".to_owned(),
        format!("if LUMI.rules_version ~= {RULES_VERSION} then "),
        "for _, r in pairs(LUMI.rules or {}) do pcall(function() r:set_enabled(false) end) end ".to_owned(),
        format!("LUMI.rules = {{}} LUMI.rules_version = {RULES_VERSION} "),
        rules.collect::<Vec<_>>().join(" "),
        " end".to_owned(),
    ]
    .concat();
    names.own(code)
}

/// How a bind is set up. `ignore_mods`: it also fires while Ctrl, Shift... are held (sprinting in a game and
/// clicking a side button).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BindOptions {
    pub release: bool,
    pub non_consuming: bool,
    pub ignore_mods: bool,
}

/// Binds `keys` (`CTRL + SHIFT + space`) so that pressing it writes `id` into the FIFO.
pub fn bind(names: &Names, id: &str, keys: &str, opts: BindOptions) -> String {
    let mut fields = vec![format!("description = {}", lstr(BIND_DESC))];
    if opts.release {
        fields.push("release = true".to_owned());
    }
    if opts.non_consuming {
        fields.push("non_consuming = true".to_owned());
    }
    if opts.ignore_mods {
        fields.push("ignore_mods = true".to_owned());
    }
    let mut slot = format!("{keys}|{}|{}", u8::from(opts.release), u8::from(opts.non_consuming));
    if opts.ignore_mods {
        slot.push_str("|i");
    }
    names.own(format!(
        "LUMI.bind({}, {}, {}, {{ {} }})",
        lstr(&slot),
        lstr(id),
        lstr(keys),
        fields.join(", ")
    ))
}

pub fn unbind(names: &Names, id: &str) -> String {
    names.own(format!("LUMI.unbind({})", lstr(id)))
}

pub fn unbind_all(names: &Names) -> String {
    names.own("LUMI.unbind_all()".to_owned())
}

/// What runs when the app quits: our binds go quiet, the slots stay for the next start.
pub fn shutdown(names: &Names) -> String {
    names.own("if LUMI and LUMI.unbind_all then LUMI.unbind_all() end".to_owned())
}

/// `hl.dispatch(...)` around a dispatcher.
pub fn dispatch(expr: &str) -> String {
    format!("hl.dispatch({expr})")
}

fn window(addr: &str) -> String {
    lstr(&format!("address:{addr}"))
}

/// Moves a window to (x, y) in layout coordinates. Hyprland answers "ok" to a move of a pinned window but
/// leaves it where it is, which kept every panel and pop-up wherever Hyprland first put it: a pinned window
/// is unpinned for the move and pinned again, in one go so nothing shows in between.
pub fn move_window(addr: &str, x: i32, y: i32, pinned: bool) -> String {
    let t = window(addr);
    let mv = format!("hl.dispatch(hl.dsp.window.move({{ x = {x}, y = {y}, window = {t} }}))");
    if !pinned {
        return mv;
    }
    format!(
        "hl.dispatch(hl.dsp.window.pin({{ action = \"disable\", window = {t} }})) {mv} \
         hl.dispatch(hl.dsp.window.pin({{ action = \"enable\", window = {t} }}))"
    )
}

pub fn focus(addr: &str) -> String {
    dispatch(&format!("hl.dsp.focus({{ window = {} }})", window(addr)))
}

pub fn raise(addr: &str) -> String {
    dispatch(&format!("hl.dsp.window.alter_zorder({{ mode = \"top\", window = {} }})", window(addr)))
}

pub fn float(addr: &str, on: bool) -> String {
    let action = if on { "enable" } else { "disable" };
    dispatch(&format!("hl.dsp.window.float({{ action = \"{action}\", window = {} }})", window(addr)))
}

pub fn resize(addr: &str, w: i32, h: i32) -> String {
    dispatch(&format!("hl.dsp.window.resize({{ x = {w}, y = {h}, window = {} }})", window(addr)))
}

/// A move without the pin dance, for other programs' windows.
pub fn move_other(addr: &str, x: i32, y: i32) -> String {
    dispatch(&format!("hl.dsp.window.move({{ x = {x}, y = {y}, window = {} }})", window(addr)))
}

pub fn move_cursor(x: i32, y: i32) -> String {
    dispatch(&format!("hl.dsp.cursor.move({{ x = {x}, y = {y} }})"))
}

/// Sends a key combination to the focused window (`CTRL`, `V`).
pub fn send_shortcut(mods: &str, key: &str) -> String {
    dispatch(&format!("hl.dsp.send_shortcut({{ mods = {}, key = {} }})", lstr(mods), lstr(key)))
}

/// A Lua expression that is true while any of the xkb keysyms (`Alt_L`, `a`) is held.
pub fn key_down(keysyms: &[&str]) -> String {
    keysyms.iter().map(|k| format!("hl.is_key_down({})", lstr(k))).collect::<Vec<_>>().join(" or ")
}

/// The rule for our glass windows (panels and notifications with glass on): what is behind them is blurred,
/// inside corners as round as the panel's so the blur does not show past them.
pub fn glass_rule(names: &Names, rounding: i32) -> String {
    names.own(format!(
        "LUMI = LUMI or {{}} if LUMI.glass_rule then pcall(function() LUMI.glass_rule:set_enabled(false) end) end \
         LUMI.glass_rule = hl.window_rule({{ match = {{ class = {}, title = {} }}, no_blur = false, rounding = {} }})",
        names.class(),
        title_has(marker::GLASS),
        rounding.max(0)
    ))
}

/// A window to open full size at the top-left of one monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    /// The start of the title (markers may follow it).
    pub title: String,
    pub monitor: String,
    pub width: u32,
    pub height: u32,
}

/// Rules that open each window titled `cover.title` full size at the top-left of its monitor. Asking for a
/// spot on another monitor is not enough on Hyprland: the window opens on the focused monitor first and a move
/// made while it is still being placed is lost, so the colour picker's frozen picture of one screen used to
/// sit a little way down the other one. [`clear_cover_rules`] takes them out again.
pub fn cover_rules(names: &Names, covers: &[Cover]) -> String {
    let mut code = "LUMI = LUMI or {} LUMI.cover_rules = LUMI.cover_rules or {} ".to_owned();
    for c in covers {
        // move is measured from the monitor's own corner
        code += &format!(
            "LUMI.cover_rules[#LUMI.cover_rules + 1] = hl.window_rule({{ match = {{ class = {}, title = {} }}, \
             monitor = {}, move = {}, size = {} }}) ",
            names.class(),
            lstr(&format!("^{}.*$", c.title)),
            lstr(&c.monitor),
            lstr("0 0"),
            lstr(&format!("{} {}", c.width, c.height))
        );
    }
    names.own(code)
}

pub fn clear_cover_rules(names: &Names) -> String {
    names.own(
        "for _, r in pairs((LUMI or {}).cover_rules or {}) do pcall(function() r:set_enabled(false) end) end \
         if LUMI then LUMI.cover_rules = {} end"
            .to_owned(),
    )
}

/// The rule for the Rust app's unseen main window (D18), which the Python app does not have. It has to exist
/// and be "shown" for the UI to run, but nobody should see it. It draws nothing (fully transparent), so it
/// needs no opacity rule, and a window Hyprland sees as shown keeps getting frame callbacks, which every other
/// window of ours depends on (they are drawn in its pass, R20).
pub fn root_rule(names: &Names, title: &str) -> String {
    names.own(format!(
        "LUMI = LUMI or {{}} if LUMI.root_rule then pcall(function() LUMI.root_rule:set_enabled(false) end) end \
         LUMI.root_rule = hl.window_rule({{ match = {{ class = {}, title = {} }}, no_initial_focus = true, \
         no_focus = true, pin = false, size = \"1 1\", move = \"0 0\" }})",
        names.class(),
        lstr(&format!("^{title}.*$"))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lum_fixtures::read_json;
    use serde_json::Value;

    fn cases() -> Value {
        read_json("hypr_cases.json")
    }

    fn python() -> Names {
        Names::python(cases()["fifo"].as_str().expect("the fixture names the FIFO"))
    }

    fn eval(sent: &Value) -> String {
        let all = sent.as_array().expect("a list of commands");
        assert_eq!(all.len(), 1, "one command each");
        all[0].as_str().expect("text").strip_prefix("eval ").expect("an eval").to_owned()
    }

    #[test]
    fn the_bootstrap_is_the_python_text() {
        let c = cases();
        assert_eq!(c["rules_version"], RULES_VERSION);
        assert_eq!(c["bind_desc"], BIND_DESC);
        assert_eq!(bootstrap(&python()), c["bootstrap"].as_str().expect("text"));
    }

    #[test]
    fn markers_match() {
        let m = &cases()["markers"];
        for (key, ch) in [
            ("no_activate", marker::NO_ACTIVATE),
            ("no_focus", marker::NO_FOCUS),
            ("no_pin", marker::NO_PIN),
            ("blur", marker::BLUR),
            ("glass", marker::GLASS),
        ] {
            assert_eq!(m[key].as_str(), Some(ch.to_string().as_str()), "{key}");
        }
    }

    #[test]
    fn binds_and_moves_match() {
        let c = cases();
        let names = python();
        for b in c["binds"].as_array().expect("binds") {
            let flag = |k: &str| b[k].as_bool().unwrap_or(false);
            let opts = BindOptions {
                release: flag("release"),
                non_consuming: flag("non_consuming"),
                ignore_mods: flag("ignore_mods"),
            };
            let (id, keys) = (b["id"].as_str().expect("id"), b["keys"].as_str().expect("keys"));
            assert_eq!(bind(&names, id, keys, opts), eval(&b["sent"]), "{keys}");
        }
        assert_eq!(unbind(&names, "hk3"), eval(&c["unbind"]));
        assert_eq!(unbind_all(&names), eval(&c["unbind_all"]));
        assert_eq!(shutdown(&names), eval(&c["shutdown_unbind"]));
        for m in c["move_window"].as_array().expect("moves") {
            let n = |k: &str| m[k].as_i64().expect("number") as i32;
            let addr = m["addr"].as_str().expect("addr");
            let pinned = m["pinned"].as_bool().expect("pinned");
            assert_eq!(move_window(addr, n("x"), n("y"), pinned), eval(&m["sent"]));
        }
        for g in c["glass_rule"].as_array().expect("glass") {
            let r = g["rounding"].as_i64().expect("rounding") as i32;
            assert_eq!(glass_rule(&names, r), eval(&g["sent"]));
        }
        assert_eq!(clear_cover_rules(&names), eval(&c["clear_cover_rules"]));
        assert_eq!(send_shortcut("CTRL", "V"), eval(&c["send_shortcut"]));
        assert_eq!(move_cursor(5, 7), eval(&c["move_cursor"]));
        for l in c["lstr"].as_array().expect("lstr") {
            assert_eq!(lstr(l["in"].as_str().expect("in")), l["out"].as_str().expect("out"));
        }
    }

    #[test]
    fn our_own_table_changes_nothing_but_the_names() {
        let ours = Names::app("/run/user/1000");
        let code = bootstrap(&ours);
        assert!(!code.contains("LUMI.") && !code.contains("LUMI "));
        assert!(code.contains("LUMRS.fifo = \"/run/user/1000/luminescent/events-rs.fifo\""));
        let theirs = bootstrap(&Names::python("/run/user/1000/luminescent/events-rs.fifo"));
        assert_eq!(code, theirs.replace("LUMI", "LUMRS"));
    }
}
