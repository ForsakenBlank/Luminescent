//! One list of things Luminescent can do, shared by the command palette, the tray, the dashboard's Tools tab
//! and the automation "Luminescent tool" node, as in the Python app.
//!
//! Ids are `"<owner>:<action>"` (`notes:new_note`), the same ids the Python app stores in dashboard items and
//! automations, so those keep working.

use std::fmt;
use std::sync::Arc;

pub type Callback = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct Action {
    /// The id inside its owner (`new_note`); the registry adds the owner.
    pub id: String,
    pub label: String,
    /// A short line under the label in the palette (can be empty).
    pub hint: String,
    run: Callback,
}

impl Action {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        run: impl Fn() + Send + Sync + 'static,
    ) -> Action {
        Action { id: id.into(), label: label.into(), hint: String::new(), run: Arc::new(run) }
    }

    /// An action whose id comes from its label, like the Python `Module.actions()` default.
    pub fn from_label(label: &str, run: impl Fn() + Send + Sync + 'static) -> Action {
        Action::new(id_from_label(label), label, run)
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Action {
        self.hint = hint.into();
        self
    }

    pub fn run(&self) {
        (self.run)();
    }
}

impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Action").field("id", &self.id).field("label", &self.label).finish_non_exhaustive()
    }
}

/// "Show / hide notes" becomes "show___hide_notes": letters and digits kept, everything else an underscore,
/// underscores trimmed from both ends.
pub fn id_from_label(label: &str) -> String {
    let id: String =
        label.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    id.trim_matches('_').to_owned()
}

#[derive(Debug, Clone)]
pub struct Registered {
    pub owner: String,
    pub action: Action,
}

impl Registered {
    pub fn full_id(&self) -> String {
        format!("{}:{}", self.owner, self.action.id)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ActionError {
    #[error("{0} is not available (not loaded)")]
    UnknownOwner(String),
    #[error("{owner} has no action '{id}' any more")]
    UnknownAction { owner: String, id: String },
}

#[derive(Debug, Default)]
pub struct ActionRegistry {
    entries: Vec<Registered>,
}

impl ActionRegistry {
    pub fn new() -> ActionRegistry {
        ActionRegistry::default()
    }

    /// Replaces whatever `owner` registered before.
    pub fn set_owner(&mut self, owner: &str, actions: Vec<Action>) {
        self.remove_owner(owner);
        let owned = actions.into_iter().map(|action| Registered { owner: owner.to_owned(), action });
        self.entries.extend(owned);
    }

    pub fn remove_owner(&mut self, owner: &str) {
        self.entries.retain(|r| r.owner != owner);
    }

    pub fn all(&self) -> &[Registered] {
        &self.entries
    }

    pub fn find(&self, full_id: &str) -> Result<&Registered, ActionError> {
        let (owner, id) = full_id.split_once(':').unwrap_or((full_id, ""));
        if !self.entries.iter().any(|r| r.owner == owner) {
            return Err(ActionError::UnknownOwner(owner.to_owned()));
        }
        let found = self.entries.iter().find(|r| r.owner == owner && r.action.id == id);
        found.ok_or_else(|| ActionError::UnknownAction { owner: owner.to_owned(), id: id.to_owned() })
    }

    /// Runs `"<owner>:<action>"` and gives back what ran (for usage counts and the activity log).
    pub fn run(&self, full_id: &str) -> Result<&Registered, ActionError> {
        let found = self.find(full_id)?;
        found.action.run();
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn ids_from_labels_match_python() {
        assert_eq!(id_from_label("New note"), "new_note");
        assert_eq!(id_from_label("Show / hide notes"), "show___hide_notes");
        assert_eq!(id_from_label("  Start…"), "start");
        assert_eq!(id_from_label("Café 2"), "café_2");
    }

    #[test]
    fn runs_by_full_id_and_forgets_owners() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let mut reg = ActionRegistry::new();
        reg.set_owner(
            "notes",
            vec![Action::from_label("New note", move || {
                c.fetch_add(1, Ordering::SeqCst);
            })],
        );
        assert_eq!(reg.run("notes:new_note").unwrap().full_id(), "notes:new_note");
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            reg.run("notes:gone").unwrap_err(),
            ActionError::UnknownAction { owner: "notes".into(), id: "gone".into() }
        );
        reg.remove_owner("notes");
        assert_eq!(reg.run("notes:new_note").unwrap_err(), ActionError::UnknownOwner("notes".into()));
    }
}
