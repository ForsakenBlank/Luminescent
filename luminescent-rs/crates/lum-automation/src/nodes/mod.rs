//! The node types, grouped as `core/automation.py` groups them.

mod actions;
mod logic;
mod triggers;
mod values;

use crate::engine::{Cx, NodeRuntime};
use crate::spec::NodeSpec;

/// A fresh running node of that type.
pub fn make(spec: &NodeSpec) -> Box<dyn NodeRuntime> {
    match spec.id.as_str() {
        "interval" => Box::new(triggers::Interval::default()),
        "manual" => Box::new(Plain),

        "var_set" => Box::new(actions::VarSet),
        "show" => Box::new(actions::Show),

        "delay" => Box::new(logic::Delay::default()),
        "if" => Box::new(logic::If),
        "gate" => Box::new(logic::Gate::default()),
        "counter" => Box::new(logic::Counter::default()),
        "once" => Box::new(logic::Once::default()),
        "cooldown" => Box::new(logic::Cooldown::default()),
        "sequence" => Box::new(logic::Sequence),
        "turns" => Box::new(logic::Turns::default()),
        "random" => Box::new(logic::Chance),
        "repeat" => Box::new(logic::Repeat::default()),

        "text" => Box::new(values::Text),
        "number" => Box::new(values::Number),
        "var_get" => Box::new(values::VarGet),
        "join" => Box::new(values::Join),
        "math" => Box::new(values::Maths),
        "now" => Box::new(values::Now),
        "clip_get" => Box::new(values::ClipGet),
        "read_file" => Box::new(values::ReadFile),
        "replace" => Box::new(values::Replace),
        "change" => Box::new(values::Change),
        "random_num" => Box::new(values::RandomNum),

        "comment" => Box::new(Plain),
        _ => Box::new(Unavailable),
    }
}

/// A node with nothing of its own to do: the Button (▶ fires it) and the Note.
struct Plain;

impl NodeRuntime for Plain {}

/// A node type whose part of Luminescent is not in this version yet. It says so and lets pulses stop there.
struct Unavailable;

impl NodeRuntime for Unavailable {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.set_status("not available yet", None);
    }
}
