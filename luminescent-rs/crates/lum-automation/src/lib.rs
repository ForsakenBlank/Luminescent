//! Automation: nodes wired together on the Automation page, running in the background.
//!
//! The document (`automation.json`) is [`doc`], what each node type looks like is [`spec`] (straight from the
//! Python app's list), the shared text rules are [`text`], [`Engine`] runs it all and [`Automation`] gives it a thread of its own.

pub mod doc;
pub mod engine;
mod nodes;
mod runner;
pub mod spec;
pub mod text;

pub use engine::{
    Clock, Config, Cx, Engine, FakeClock, Host, NodeRuntime, Out, Pulse, Status, SystemClock, Updates,
};
pub use runner::Automation;
pub use text::MAX_TEXT;
