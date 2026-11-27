//! Notes' data: `notes.json` with its notes, to-do items, sticky places and panel geometry, kept over raw JSON so
//! a field this version does not know survives a save. Nothing here draws; `lum-ui` shows it.
//!
//! The file is the Python app's, so a few things look odd on purpose: `open` is an index in Python and an id in
//! the sample file (both are read, the index is written), and a sticky's place is a list of four numbers in
//! Python and an object with a screen name in newer files (both are read, the shape found is the shape kept).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jiff::civil::{Date, DateTime};
use serde_json::{Map, Value, json};

use crate::pyjson::{self, Style};
use crate::store::{self, Writer};

/// How long Undo stays on the list after a note is deleted.
pub const UNDO_SECONDS: u64 = 12;

pub const COLOURS: [(&str, &str); 8] = [
    ("Red", "#E5484D"),
    ("Orange", "#F76B15"),
    ("Yellow", "#FFC53D"),
    ("Green", "#30A46C"),
    ("Teal", "#12A594"),
    ("Blue", "#3E63DD"),
    ("Purple", "#8E4EC6"),
    ("Pink", "#D6409F"),
];

pub fn new_id() -> String {
    let mut id = uuid::Uuid::new_v4().simple().to_string();
    id.truncate(10);
    id
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Note,
    Todo,
}

impl Kind {
    fn from_text(text: &str) -> Kind {
        if text == "todo" { Kind::Todo } else { Kind::Note }
    }

    pub fn as_text(self) -> &'static str {
        match self {
            Kind::Note => "note",
            Kind::Todo => "todo",
        }
    }
}

fn text_of(map: &Map<String, Value>, key: &str) -> String {
    map.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// A task in a to-do note.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub text: String,
    pub checked: bool,
    /// "2026-11-24T09:00", or empty.
    pub due: String,
    raw: Map<String, Value>,
}

impl Item {
    pub fn new(text: &str) -> Item {
        Item { id: new_id(), text: text.to_owned(), checked: false, due: String::new(), raw: Map::new() }
    }

    fn from_map(raw: &Map<String, Value>) -> Item {
        Item {
            id: raw.get("id").and_then(Value::as_str).map_or_else(new_id, str::to_owned),
            text: text_of(raw, "text"),
            checked: raw.get("checked").is_some_and(truthy),
            due: text_of(raw, "due"),
            raw: raw.clone(),
        }
    }

    /// Python writes `due` only while there is one; a file that already has the key keeps it.
    fn to_value(&self) -> Value {
        let mut map = self.raw.clone();
        map.insert("id".into(), json!(self.id));
        map.insert("text".into(), json!(self.text));
        map.insert("checked".into(), json!(self.checked));
        if !self.due.is_empty() || map.contains_key("due") {
            map.insert("due".into(), json!(self.due));
        }
        Value::Object(map)
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Null => false,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub content: String,
    pub items: Vec<Item>,
    pub pinned: bool,
    pub colour: String,
    pub tags: Vec<String>,
    raw: Map<String, Value>,
}

impl Note {
    pub fn new(name: &str, kind: Kind) -> Note {
        let mut note = Note {
            id: new_id(),
            name: name.to_owned(),
            kind,
            content: String::new(),
            items: Vec::new(),
            pinned: false,
            colour: String::new(),
            tags: Vec::new(),
            raw: Map::new(),
        };
        // a new note is written in Python's order, which differs from the one it fills in missing keys with
        for key in ["id", "name", "type", "content", "items", "pinned", "colour", "tags"] {
            note.raw.insert(key.into(), Value::Null);
        }
        note
    }

    fn from_map(raw: &Map<String, Value>) -> Note {
        let items = raw
            .get("items")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_object).map(Item::from_map).collect())
            .unwrap_or_default();
        let tags = raw
            .get("tags")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default();
        Note {
            id: raw.get("id").and_then(Value::as_str).map_or_else(new_id, str::to_owned),
            name: raw.get("name").and_then(Value::as_str).unwrap_or("Note").to_owned(),
            kind: Kind::from_text(&text_of(raw, "type")),
            content: text_of(raw, "content"),
            items,
            pinned: raw.get("pinned").is_some_and(truthy),
            colour: text_of(raw, "colour"),
            tags,
            raw: raw.clone(),
        }
    }

    /// Keys this version knows are put back where they were; ones the file lacked follow Python's `setdefault`
    /// order (content, items, type, pinned, colour, tags).
    pub fn to_value(&self) -> Value {
        let mut map = self.raw.clone();
        map.insert("id".into(), json!(self.id));
        map.insert("name".into(), json!(self.name));
        map.insert("content".into(), json!(self.content));
        map.insert("items".into(), Value::Array(self.items.iter().map(Item::to_value).collect()));
        map.insert("type".into(), json!(self.kind.as_text()));
        map.insert("pinned".into(), json!(self.pinned));
        map.insert("colour".into(), json!(self.colour));
        map.insert("tags".into(), json!(self.tags));
        Value::Object(map)
    }

    pub fn is_todo(&self) -> bool {
        self.kind == Kind::Todo
    }

    /// (done, all) for a to-do note.
    pub fn progress(&self) -> (usize, usize) {
        (self.items.iter().filter(|i| i.checked).count(), self.items.len())
    }

    /// Whether the search box lets this note through: name, text, tags and tasks, or `#tag` for tags only.
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        if let Some(tag) = q.strip_prefix('#') {
            let mut tags = self.tags.iter().map(|t| t.to_lowercase());
            return if tag.is_empty() { tags.next().is_some() } else { tags.any(|t| t.starts_with(tag)) };
        }
        let has = |text: &str| text.to_lowercase().contains(&q);
        has(&self.name)
            || has(&self.content)
            || self.tags.iter().any(|t| has(t))
            || self.items.iter().any(|i| has(&i.text))
    }

    /// What the palette searches: the tags and the start of the text.
    pub fn palette_hint(&self) -> String {
        let body = if self.is_todo() {
            self.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>().join(" ")
        } else {
            self.content.clone()
        };
        let tags: Vec<String> = self.tags.iter().map(|t| format!("#{t}")).collect();
        let start: String = body.chars().take(60).collect::<String>().replace('\n', " ");
        let hint = format!("{} {start}", tags.join(" "));
        let hint = hint.trim();
        if hint.is_empty() { "note".to_owned() } else { hint.to_owned() }
    }

    /// The note as Markdown text, which is what Export writes.
    pub fn markdown(&self) -> String {
        if !self.is_todo() {
            return self.content.clone();
        }
        let mut lines = vec![format!("# {}", self.name), String::new()];
        for item in &self.items {
            let due = if item.due.is_empty() {
                String::new()
            } else {
                let shown: String = item.due.replace('T', " ").chars().take(16).collect();
                format!("  (due {shown})")
            };
            lines.push(format!("- [{}] {}{due}", if item.checked { 'x' } else { ' ' }, item.text));
        }
        lines.join("\n") + "\n"
    }

    /// A file name for Export: the name without what Windows refuses.
    pub fn export_name(&self) -> String {
        let safe: String = self.name.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect();
        let safe = safe.trim();
        if safe.is_empty() { "note".to_owned() } else { safe.to_owned() }
    }

    pub fn add_item(&mut self, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        self.items.push(Item::new(text));
        true
    }

    pub fn clear_done(&mut self) -> Vec<Item> {
        let (done, keep) = std::mem::take(&mut self.items).into_iter().partition(|i| i.checked);
        self.items = keep;
        done
    }

    /// Moves the item at `from` to `to` (the place it ends at).
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from < self.items.len() && to < self.items.len() && from != to {
            let item = self.items.remove(from);
            self.items.insert(to, item);
        }
    }
}

/// The tags typed in the Tags box: commas, spaces and `#` all separate, a repeat in other letters is dropped.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.replace(['#', ','], " ").split_whitespace() {
        if !tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
            tags.push(tag.to_owned());
        }
    }
    tags
}

pub fn parse_due(due: &str) -> Option<DateTime> {
    if due.is_empty() {
        return None;
    }
    due.trim().parse::<DateTime>().ok()
}

/// "today 09:00", "tomorrow 09:00", "Fri 09:00" within the week, else "24 Nov 09:00".
pub fn due_text(due: &str, today: Date) -> String {
    let Some(when) = parse_due(due) else { return String::new() };
    let days = (when.date() - today).get_days();
    let day = match days {
        0 => "today".to_owned(),
        1 => "tomorrow".to_owned(),
        2..=6 => when.strftime("%a").to_string(),
        _ => when.strftime("%d %b").to_string(),
    };
    format!("{day} {}", when.strftime("%H:%M"))
}

pub fn overdue(due: &str, now: DateTime) -> bool {
    parse_due(due).is_some_and(|when| when < now)
}

/// What the Reminders module should do about a to-do item's due date.
#[derive(Debug, Clone, PartialEq)]
pub enum TodoSync {
    Set { id: String, title: String, when: DateTime, message: String },
    Drop { id: String },
}

/// The reminder id a to-do item uses.
pub fn reminder_id(item_id: &str) -> String {
    format!("todo-{item_id}")
}

/// The reminder that goes with `item`: set while it has a due date in the future and is not done, else dropped.
pub fn sync_for(note: &Note, item: &Item, now: DateTime) -> TodoSync {
    let id = reminder_id(&item.id);
    match parse_due(&item.due) {
        Some(when) if !item.checked && when > now => TodoSync::Set {
            id,
            title: format!("To-do: {}", item.text),
            when,
            message: format!("From the note \u{201c}{}\u{201d}", note.name),
        },
        _ => TodoSync::Drop { id },
    }
}

/// What changed between two looks at a note's items, as far as reminders care: an item whose due date, tick or
/// text changed (and that has or had a due date), and items that went away with one.
pub fn todo_changes(note: &Note, before: &[Item], now: DateTime) -> Vec<TodoSync> {
    let mut out = Vec::new();
    for item in &note.items {
        let old = before.iter().find(|b| b.id == item.id);
        let changed =
            old.is_none_or(|o| (&o.due, o.checked, &o.text) != (&item.due, item.checked, &item.text));
        if changed && (!item.due.is_empty() || old.is_some_and(|o| !o.due.is_empty())) {
            out.push(sync_for(note, item, now));
        }
    }
    for old in before {
        if !old.due.is_empty() && !note.items.iter().any(|i| i.id == old.id) {
            out.push(TodoSync::Drop { id: reminder_id(&old.id) });
        }
    }
    out
}

#[derive(Debug)]
struct Trash {
    note: Note,
    index: usize,
    at: Instant,
}

#[derive(Debug)]
pub struct Notes {
    path: PathBuf,
    /// Everything in the file except the notes themselves, in the file's order.
    root: Map<String, Value>,
    pub notes: Vec<Note>,
    trash: Option<Trash>,
    writer: Writer,
    dirty: bool,
}

impl Notes {
    /// An unreadable file is kept as `notes.json.bad` by the loader, so the default note that replaces it is
    /// never saved over the real file.
    pub fn load(path: &Path, width: i64, height: i64) -> Notes {
        let mut root = match store::load_json(path) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        let mut notes: Vec<Note> = match root.get("notes") {
            Some(Value::Array(list)) => {
                list.iter().filter_map(Value::as_object).map(Note::from_map).collect()
            }
            _ => Vec::new(),
        };
        let fresh = !root.contains_key("notes");
        if fresh {
            notes.push(Note::new("My First Note", Kind::Note));
        }
        let defaults = [
            ("notes", Value::Array(Vec::new())),
            ("x", json!(100)),
            ("y", json!(100)),
            ("w", json!(width)),
            ("h", json!(height)),
            ("open", Value::Null),
            ("stickies", json!({})),
            ("preview", json!(false)),
        ];
        for (key, value) in defaults {
            root.entry(key).or_insert(value);
        }
        if !root.get("stickies").is_some_and(Value::is_object) {
            root.insert("stickies".into(), json!({}));
        }
        Notes {
            path: path.to_owned(),
            root,
            notes,
            trash: None,
            writer: Writer::new("notes-save"),
            dirty: false,
        }
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.notes.iter().position(|n| n.id == id)
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn note_mut(&mut self, id: &str) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    pub fn first_pinned(&self) -> Option<usize> {
        self.notes.iter().position(|n| n.pinned)
    }

    /// The order the list shows: pinned notes first, each group in the order of the file.
    pub fn order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.notes.len()).collect();
        order.sort_by_key(|&i| (!self.notes[i].pinned, i));
        order
    }

    pub fn add(&mut self, name: &str, kind: Kind) -> usize {
        self.notes.push(Note::new(name, kind));
        self.dirty = true;
        self.notes.len() - 1
    }

    pub fn move_note(&mut self, index: usize, delta: isize) {
        let Some(to) = index.checked_add_signed(delta).filter(|&to| to < self.notes.len()) else { return };
        self.notes.swap(index, to);
        self.dirty = true;
    }

    /// A note was dragged in the list: `from` and `to` are positions in [`Notes::order`].
    pub fn reorder(&mut self, from: usize, to: usize) {
        let mut order = self.order();
        if from >= order.len() || to >= order.len() || from == to {
            return;
        }
        let moved = order.remove(from);
        order.insert(to, moved);
        let mut old: Vec<Option<Note>> = std::mem::take(&mut self.notes).into_iter().map(Some).collect();
        self.notes = order.into_iter().filter_map(|i| old[i].take()).collect();
        self.dirty = true;
    }

    /// Takes the note out and keeps it for Undo. Its sticky goes with it.
    pub fn delete(&mut self, index: usize, now: Instant) -> Option<Note> {
        if index >= self.notes.len() {
            return None;
        }
        let note = self.notes.remove(index);
        self.remove_sticky(&note.id);
        self.trash = Some(Trash { note: note.clone(), index, at: now });
        self.dirty = true;
        Some(note)
    }

    /// Puts the last deleted note back where it was. `None` when there is nothing to bring back.
    pub fn restore(&mut self) -> Option<Note> {
        let Trash { note, index, .. } = self.trash.take()?;
        if self.notes.iter().any(|n| n.id == note.id) {
            return None;
        }
        self.notes.insert(index.min(self.notes.len()), note.clone());
        self.dirty = true;
        Some(note)
    }

    /// The name of the note Undo would bring back and how long Undo has left. Gone after [`UNDO_SECONDS`].
    pub fn undo(&mut self, now: Instant) -> Option<(&str, Duration)> {
        let limit = Duration::from_secs(UNDO_SECONDS);
        if self.trash.as_ref().is_some_and(|t| now.saturating_duration_since(t.at) > limit) {
            self.trash = None;
        }
        let trash = self.trash.as_ref()?;
        Some((trash.note.name.as_str(), limit.saturating_sub(now.saturating_duration_since(trash.at))))
    }

    fn number(&self, key: &str) -> Option<f32> {
        self.root.get(key).and_then(Value::as_f64).map(|v| v as f32)
    }

    /// The panel's x, y, width and height.
    pub fn geometry(&self) -> [f32; 4] {
        ["x", "y", "w", "h"].map(|k| self.number(k).unwrap_or(100.0))
    }

    pub fn set_geometry(&mut self, g: [f32; 4]) {
        for (key, value) in ["x", "y", "w", "h"].into_iter().zip(g) {
            let value = json!(value.round() as i64);
            if self.root.get(key) != Some(&value) {
                self.root.insert(key.into(), value);
                self.dirty = true;
            }
        }
    }

    /// The note the panel had open, by index (Python) or by id.
    pub fn open_note(&self) -> Option<usize> {
        match self.root.get("open")? {
            Value::Number(n) => n.as_u64().map(|i| i as usize).filter(|&i| i < self.notes.len()),
            Value::String(id) => self.index_of(id),
            _ => None,
        }
    }

    pub fn set_open(&mut self, open: Option<usize>) {
        let value = open.map_or(Value::Null, |i| json!(i));
        if self.open_note() != open || (open.is_none() && !self.root.get("open").is_some_and(Value::is_null))
        {
            self.root.insert("open".into(), value);
            self.dirty = true;
        }
    }

    pub fn preview(&self) -> bool {
        self.root.get("preview").is_some_and(truthy)
    }

    pub fn set_preview(&mut self, on: bool) {
        if self.preview() != on {
            self.root.insert("preview".into(), json!(on));
            self.dirty = true;
        }
    }

    fn stickies(&self) -> Option<&Map<String, Value>> {
        self.root.get("stickies").and_then(Value::as_object)
    }

    /// The notes stuck on the screen, with where each one sits (x, y, width, height).
    pub fn sticky_places(&self) -> Vec<(String, [f32; 4])> {
        let Some(stickies) = self.stickies() else { return Vec::new() };
        stickies.iter().filter_map(|(id, place)| Some((id.clone(), place_of(place)?))).collect()
    }

    pub fn is_stuck(&self, id: &str) -> bool {
        self.stickies().is_some_and(|s| s.contains_key(id))
    }

    pub fn set_sticky(&mut self, id: &str, place: [f32; 4]) {
        let Some(stickies) = self.root.get_mut("stickies").and_then(Value::as_object_mut) else { return };
        let rounded = place.map(|v| v.round() as i64);
        match stickies.get_mut(id) {
            Some(Value::Object(existing)) => {
                for (key, value) in ["x", "y", "w", "h"].into_iter().zip(rounded) {
                    existing.insert(key.into(), json!(value));
                }
            }
            _ => {
                stickies.insert(id.to_owned(), json!(rounded));
            }
        }
        self.dirty = true;
    }

    pub fn remove_sticky(&mut self, id: &str) {
        if let Some(stickies) = self.root.get_mut("stickies").and_then(Value::as_object_mut)
            && stickies.remove(id).is_some()
        {
            self.dirty = true;
        }
    }

    /// Sticky places of notes that no longer exist are dropped when the file is loaded.
    pub fn forget_lost_stickies(&mut self) {
        let lost: Vec<String> = self
            .sticky_places()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| self.index_of(id).is_none())
            .collect();
        for id in lost {
            self.remove_sticky(&id);
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn to_text(&self) -> String {
        let mut root = self.root.clone();
        root.insert("notes".into(), Value::Array(self.notes.iter().map(Note::to_value).collect()));
        pyjson::to_string(&Value::Object(root), Style::indented(2))
    }

    /// Hands the file to the writer thread; the UI never waits for the disk.
    pub fn save(&mut self) {
        self.dirty = false;
        self.writer.save(&self.path, self.to_text());
    }

    pub fn flush(&mut self) {
        if self.dirty {
            self.save();
        }
        self.writer.flush();
    }
}

fn place_of(value: &Value) -> Option<[f32; 4]> {
    let number = |v: &Value| v.as_f64().map(|n| n as f32);
    match value {
        Value::Array(list) if list.len() == 4 => {
            Some([number(&list[0])?, number(&list[1])?, number(&list[2])?, number(&list[3])?])
        }
        Value::Object(map) => Some([
            number(map.get("x")?)?,
            number(map.get("y")?)?,
            number(map.get("w")?)?,
            number(map.get("h")?)?,
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../fixtures/sample_home/data/notes.json");

    fn sample() -> (tempfile::TempDir, Notes) {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("notes.json");
        std::fs::write(&path, SAMPLE).expect("the sample is written");
        let notes = Notes::load(&path, 360, 460);
        (dir, notes)
    }

    fn at(text: &str) -> DateTime {
        text.parse().expect("a date and time")
    }

    #[test]
    fn the_sample_notes_file_is_saved_back_unchanged() {
        let (_dir, notes) = sample();
        assert_eq!(notes.to_text(), SAMPLE.trim_end());
    }

    #[test]
    fn a_missing_file_starts_with_the_first_note() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let notes = Notes::load(&dir.path().join("notes.json"), 360, 460);
        assert_eq!(notes.notes.len(), 1);
        assert_eq!(notes.notes[0].name, "My First Note");
        let text = notes.to_text();
        let keys: Vec<&str> = text.lines().filter(|l| l.starts_with("  \"")).map(|l| l.trim()).collect();
        let tops =
            ["\"notes\"", "\"x\"", "\"y\"", "\"w\"", "\"h\"", "\"open\"", "\"stickies\"", "\"preview\""];
        for (line, key) in keys.iter().zip(tops) {
            assert!(line.starts_with(key), "{line} should start with {key}");
        }
    }

    #[test]
    fn a_broken_file_is_kept_and_not_replaced() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("notes.json");
        std::fs::write(&path, "{not json").expect("written");
        let notes = Notes::load(&path, 360, 460);
        assert_eq!(notes.notes.len(), 1);
        assert!(dir.path().join("notes.json.bad").exists());
        assert!(!path.exists());
    }

    #[test]
    fn missing_fields_get_python_s_defaults_and_unknown_ones_stay() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let path = dir.path().join("notes.json");
        std::fs::write(
            &path,
            r##"{"notes": [{"name": "Old", "colour": "#fff", "mine": 5}, 7], "extra": true}"##,
        )
        .expect("written");
        let notes = Notes::load(&path, 300, 200);
        assert_eq!(notes.notes.len(), 1);
        let text = notes.to_text();
        let value: Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(value["extra"], json!(true));
        assert_eq!(value["w"], json!(300));
        let note = &value["notes"][0];
        assert_eq!(note["type"], "note");
        assert_eq!(note["mine"], 5);
        assert_eq!(note["tags"], json!([]));
        assert_eq!(note["id"].as_str().map(str::len), Some(10));
    }

    #[test]
    fn the_search_box_reads_names_text_tags_and_tasks() {
        let (_dir, notes) = sample();
        let first = &notes.notes[0];
        let shopping = &notes.notes[1];
        assert!(first.matches(""));
        assert!(first.matches("RUST"));
        assert!(first.matches("#work") && first.matches("#wo") && first.matches("#"));
        assert!(!first.matches("#home"));
        assert!(!shopping.matches("#"), "a note without tags has none to show");
        assert!(shopping.matches("bread"));
        assert!(!shopping.matches("#bread"));
    }

    #[test]
    fn tags_are_split_on_commas_spaces_and_hashes_without_repeats() {
        assert_eq!(parse_tags("work, #Home  home,WORK ideas"), vec!["work", "Home", "ideas"]);
        assert!(parse_tags(" , # ").is_empty());
    }

    #[test]
    fn due_dates_read_as_today_tomorrow_a_weekday_or_a_date() {
        let today: Date = "2026-11-24".parse().expect("a date");
        assert_eq!(due_text("2026-11-24T09:00", today), "today 09:00");
        assert_eq!(due_text("2026-11-25T17:30", today), "tomorrow 17:30");
        assert_eq!(due_text("2026-11-27T08:05", today), "Fri 08:05");
        assert_eq!(due_text("2026-11-30T08:05", today), "Mon 08:05", "six days ahead is still a weekday");
        assert_eq!(due_text("2026-12-01T08:05", today), "01 Dec 08:05");
        assert_eq!(due_text("2026-11-20T08:05", today), "20 Nov 08:05");
        assert_eq!(due_text("soon", today), "");
        assert!(overdue("2026-11-24T09:00", at("2026-11-24T09:01")));
        assert!(!overdue("2026-11-24T09:00", at("2026-11-24T08:59")));
        assert!(!overdue("", at("2026-11-24T08:59")));
    }

    #[test]
    fn export_writes_markdown_for_both_kinds() {
        let (_dir, notes) = sample();
        assert_eq!(notes.notes[0].markdown(), "# Ideas\n\nTry the **Rust** build.");
        assert_eq!(
            notes.notes[1].markdown(),
            "# Shopping\n\n- [x] milk\n- [ ] bread  (due 2026-11-24 09:00)\n"
        );
        let mut odd = Note::new("a/b: c?", Kind::Note);
        assert_eq!(odd.export_name(), "ab c");
        odd.name = "???".into();
        assert_eq!(odd.export_name(), "note");
    }

    #[test]
    fn items_keep_their_extra_keys_and_empty_due_when_the_file_had_one() {
        let (_dir, mut notes) = sample();
        let shopping = &mut notes.notes[1];
        shopping.items[0].checked = false;
        shopping.add_item("  eggs ");
        assert!(!shopping.add_item("   "));
        let text = notes.to_text();
        assert!(text.contains("\"text\": \"eggs\""));
        let eggs = text.split("\"text\": \"eggs\"").nth(1).expect("eggs is written");
        assert!(
            !eggs.trim_start_matches(['\n', ' ']).starts_with(",\n        \"due\""),
            "no due key for a new task"
        );
        assert!(text.contains("\"checked\": false,\n          \"due\": \"\""));
    }

    #[test]
    fn tasks_move_and_done_ones_clear() {
        let mut note = Note::new("List", Kind::Todo);
        for text in ["a", "b", "c"] {
            note.add_item(text);
        }
        note.move_item(0, 2);
        let names: Vec<&str> = note.items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(names, ["b", "c", "a"]);
        note.items[1].checked = true;
        assert_eq!(note.progress(), (1, 3));
        let gone = note.clear_done();
        assert_eq!(gone.len(), 1);
        assert_eq!(note.items.len(), 2);
    }

    #[test]
    fn deleting_keeps_the_note_for_twelve_seconds_and_undo_puts_it_back() {
        let (_dir, mut notes) = sample();
        let start = Instant::now();
        let gone = notes.delete(0, start).expect("the first note");
        assert_eq!(notes.notes.len(), 1);
        let (name, left) = notes.undo(start + Duration::from_secs(2)).expect("undo is on offer");
        assert_eq!(name, "My First Note");
        assert_eq!(left, Duration::from_secs(10));
        let back = notes.restore().expect("it comes back");
        assert_eq!(back.id, gone.id);
        assert_eq!(notes.notes[0].id, gone.id, "at the place it left");
        notes.delete(1, start);
        assert!(notes.undo(start + Duration::from_secs(13)).is_none(), "too late");
        assert!(notes.restore().is_none());
    }

    #[test]
    fn deleting_a_stuck_note_unsticks_it() {
        let (_dir, mut notes) = sample();
        assert!(notes.is_stuck("todo1"));
        notes.delete(1, Instant::now());
        assert!(!notes.is_stuck("todo1"));
    }

    #[test]
    fn pinned_notes_come_first_and_dragging_follows_that_order() {
        let mut notes = Notes::load(Path::new("/nonexistent/notes.json"), 360, 460);
        notes.notes.clear();
        for name in ["a", "b", "c"] {
            notes.add(name, Kind::Note);
        }
        notes.notes[2].pinned = true;
        assert_eq!(notes.order(), vec![2, 0, 1]);
        notes.reorder(2, 1);
        let names: Vec<&str> = notes.notes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["c", "b", "a"], "the list is saved in the order it shows");
        notes.move_note(0, 1);
        assert_eq!(notes.notes[0].name, "b");
        notes.move_note(0, -1);
        assert_eq!(notes.notes[0].name, "b", "the first note can not go further up");
    }

    #[test]
    fn open_and_sticky_places_are_read_in_both_shapes_and_written_in_the_shape_found() {
        let (_dir, mut notes) = sample();
        assert_eq!(notes.open_note(), Some(1));
        assert_eq!(notes.sticky_places(), vec![("todo1".to_owned(), [40.0, 60.0, 280.0, 300.0])]);
        notes.set_sticky("todo1", [50.0, 60.0, 280.0, 300.0]);
        notes.set_sticky("b921c60766", [1.0, 2.0, 3.0, 4.0]);
        notes.set_open(Some(0));
        let value: Value = serde_json::from_str(&notes.to_text()).expect("valid json");
        assert_eq!(
            value["stickies"]["todo1"],
            json!({"x": 50, "y": 60, "w": 280, "h": 300, "screen": "DP-1"})
        );
        assert_eq!(value["stickies"]["b921c60766"], json!([1, 2, 3, 4]));
        assert_eq!(value["open"], json!(0));
        assert!(notes.sticky_places().iter().any(|(id, p)| id == "b921c60766" && *p == [1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn a_due_date_in_the_future_sets_a_reminder_and_anything_else_drops_it() {
        let note = Note::new("Shopping", Kind::Todo);
        let mut item = Item::new("bread");
        item.due = "2026-11-24T09:00".into();
        let now = at("2026-11-23T12:00");
        let TodoSync::Set { id, title, when, message } = sync_for(&note, &item, now) else {
            panic!("a future due date sets a reminder");
        };
        assert_eq!(id, format!("todo-{}", item.id));
        assert_eq!(title, "To-do: bread");
        assert_eq!(when, at("2026-11-24T09:00"));
        assert_eq!(message, "From the note \u{201c}Shopping\u{201d}");
        item.checked = true;
        assert!(matches!(sync_for(&note, &item, now), TodoSync::Drop { .. }));
        item.checked = false;
        assert!(
            matches!(sync_for(&note, &item, at("2026-11-25T00:00")), TodoSync::Drop { .. }),
            "already past"
        );
    }

    #[test]
    fn only_changes_that_involve_a_due_date_touch_reminders() {
        let now = at("2026-11-23T12:00");
        let mut note = Note::new("List", Kind::Todo);
        note.add_item("plain");
        note.add_item("dated");
        note.items[1].due = "2026-11-24T09:00".into();
        let before = note.items.clone();
        assert!(todo_changes(&note, &before, now).is_empty());

        note.items[0].text = "plain, edited".into();
        assert!(todo_changes(&note, &before, now).is_empty(), "no due date, nothing to remind");

        note.items[1].checked = true;
        let changes = todo_changes(&note, &before, now);
        assert_eq!(changes, vec![TodoSync::Drop { id: reminder_id(&note.items[1].id) }]);

        let removed = note.items.remove(1);
        let changes = todo_changes(&note, &before, now);
        assert_eq!(changes, vec![TodoSync::Drop { id: reminder_id(&removed.id) }]);
    }

    #[test]
    fn the_palette_hint_shows_tags_and_the_start_of_the_text() {
        let (_dir, notes) = sample();
        assert_eq!(notes.notes[0].palette_hint(), "#work # Ideas  Try the **Rust** build.");
        assert_eq!(notes.notes[1].palette_hint(), "milk bread");
        assert_eq!(Note::new("Empty", Kind::Note).palette_hint(), "note");
    }

    #[test]
    fn saving_goes_through_the_writer_and_loads_back() {
        let (dir, mut notes) = sample();
        notes.add("Another", Kind::Todo);
        notes.set_geometry([10.4, 20.6, 300.0, 400.0]);
        notes.flush();
        let again = Notes::load(&dir.path().join("notes.json"), 360, 460);
        assert_eq!(again.notes.len(), 3);
        assert_eq!(again.geometry(), [10.0, 21.0, 300.0, 400.0]);
        assert!(again.notes[2].is_todo());
    }
}
