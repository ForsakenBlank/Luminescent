//! The node types, grouped as `core/automation.py` groups them.

mod actions;
mod logic;
mod triggers;
mod values;

use crate::engine::{Cx, NodeRuntime, Out};
use crate::spec::NodeSpec;

/// A fresh running node of that type.
pub fn make(spec: &NodeSpec) -> Box<dyn NodeRuntime> {
    match spec.id.as_str() {
        "interval" => Box::new(triggers::Interval::default()),
        "at_time" => Box::new(triggers::AtTime::default()),
        "on_date" => Box::new(triggers::OnDate),
        "hotkey" | "manual" => Box::new(Plain),
        "event" => Box::new(triggers::EventTrigger),
        "process" => Box::new(triggers::Process::default()),
        "file_watch" => Box::new(triggers::FileWatch::default()),
        "clipboard" => Box::new(triggers::ClipTrigger),
        "idle" => Box::new(triggers::Idle::default()),
        "threshold" => Box::new(triggers::Threshold::default()),
        "window" => Box::new(triggers::Window::default()),
        "network" => Box::new(triggers::Network::default()),

        "command" => Box::new(actions::Command::default()),
        "open" => Box::new(actions::Open),
        "close_program" => Box::new(actions::CloseProgram),
        "lum_action" => Box::new(actions::LumAction),
        "overlay" => Box::new(actions::Overlay),
        "dash_item" => Box::new(actions::AskApp(Out::RunItem, "item")),
        "switch_board" => Box::new(actions::AskApp(Out::SwitchBoard, "board")),
        "theme" => Box::new(actions::AskApp(Out::Theme, "theme")),
        "timer" => Box::new(actions::Timer::default()),
        "workspace" => Box::new(actions::Workspace),
        "notify" => Box::new(actions::Notify),
        "message" => Box::new(actions::Message::default()),
        "ask" => Box::new(actions::Ask::default()),
        "sound" => Box::new(actions::Sound),
        "speak" => Box::new(actions::Speak::default()),
        "type_text" => Box::new(actions::TypeText::default()),
        "keys" => Box::new(actions::Keys),
        "set_clipboard" => Box::new(actions::SetClipboard),
        "http" => Box::new(actions::Http::default()),
        "download" => Box::new(actions::Download),
        "file_op" => Box::new(actions::FileOp),
        "write_file" => Box::new(actions::WriteFile),
        "alert" => Box::new(actions::Alert::default()),
        "volume" => Box::new(actions::Volume::default()),
        "power" => Box::new(actions::Power::default()),
        "keep_awake" => Box::new(actions::KeepAwake::default()),
        "open_note" => Box::new(actions::OpenNote::default()),
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

/// A node with nothing of its own to do: the Hotkey (the app tells it), the Button (▶ fires it) and the Note.
struct Plain;

impl NodeRuntime for Plain {}

/// A node type whose part of Luminescent is not in this version yet (Set wallpaper waits for the wallpaper
/// backends). It says so and lets pulses stop there.
struct Unavailable;

impl NodeRuntime for Unavailable {
    fn start(&mut self, cx: &mut Cx<'_>) {
        cx.set_status("not available yet", None);
    }
}
