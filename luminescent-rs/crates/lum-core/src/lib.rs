//! Shared infrastructure every other crate builds on: where files live, how JSON is read and written, the
//! settings store with its profiles, logging, and the small services the Python app keeps in `core/`.

pub mod actions;
pub mod backup;
pub mod clock;
pub mod dashboard;
pub mod events;
pub mod guard;
pub mod logging;
pub mod module;
pub mod paths;
pub mod pyjson;
pub mod settings;
pub mod store;
pub mod usage;

pub use paths::{APP_NAME, Paths, VERSION};
