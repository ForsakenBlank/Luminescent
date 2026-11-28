//! Actions: nodes that do something when a pulse arrives.

use crate::engine::{Cx, NodeRuntime};
use crate::text::{self, clip_chars};

/// Set variable: keeps a value for later (also after a restart). Done carries the new value.
pub struct VarSet;

impl NodeRuntime for VarSet {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let name = cx.raw_str("name").trim().to_owned();
        if name.is_empty() {
            cx.set_status("⚠ give it a name", None);
            return;
        }
        let value = cx.f("value", input);
        let old = cx.get_var(&name);
        let value = match cx.raw_str("mode").as_str() {
            "append" => old + &value,
            "add" => {
                let as_number = |s: &str| if s.is_empty() { Some(0.0) } else { text::py_float(s) };
                match (as_number(&old), as_number(&value)) {
                    (Some(a), Some(b)) => text::fmt_number(a + b),
                    _ => {
                        cx.set_status("⚠ not a number", None);
                        return;
                    }
                }
            }
            "clear" => String::new(),
            _ => value,
        };
        cx.set_var(&name, &value);
        cx.set_status(format!("{name} = {}", clip_chars(&value, 50)), None);
        cx.fire("done", &value);
    }
}

/// Show: shows what comes through (handy to see what a wire carries) and passes it on.
pub struct Show;

impl NodeRuntime for Show {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let line = input.replace('\n', " ⏎ ");
        let shown = clip_chars(&line, 120);
        cx.set_status(if shown.is_empty() { "(empty)" } else { shown }, None);
        tracing::info!("show [{}]: {}", cx.label(), clip_chars(input, 500));
        cx.fire("out", input);
    }
}
