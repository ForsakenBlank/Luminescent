//! Shared infrastructure every other crate builds on: where files live, how JSON is read and written, the
//! settings store with its profiles, logging, and the small services the Python app keeps in `core/`.

pub mod clock;
pub mod logging;
pub mod paths;
pub mod pyjson;
pub mod settings;
pub mod store;

pub use paths::{APP_NAME, Paths, VERSION};
