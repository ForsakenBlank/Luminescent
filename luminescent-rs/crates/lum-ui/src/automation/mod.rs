//! The Automation page: an endless canvas with nodes you wire together (`ui/automation.py`).
//!
//! Drag from a port (the dots on a node's edges) to another to wire them: round ports carry pulses, diamonds
//! carry values (text a field reads instead of its box). Right-click or double-click the open space to add a
//! node, middle- or right-drag (or Space + drag) to move around, the wheel zooms.
//!
//! The page only edits the workspace; the engine runs it, overlay open or not. It keeps its own copy of the
//! workspace it shows and hands every change to the engine straight away, so the two never drift apart.

mod canvas;
mod node;
mod picker;
mod pulses;
mod vars;

use std::collections::{BTreeSet, HashMap};

use egui::{Key, Modifiers, PointerButton, Pos2, Rect, Sense, Ui, pos2, vec2};
use lum_automation::{Handle, Status, doc};
use serde_json::{Value, json};

use crate::dialog::{AskDialog, Outcome};
use crate::overlay::PageFrame;
use crate::theme::Tokens;
use crate::widgets::menu;

pub use canvas::View;
pub use node::{compatible, is_source};
use picker::{Picked, Picker};

/// What the page needs from the app.
pub struct Env<'a> {
    /// A choice field's list that only the app knows (`_themes`, `_items`...).
    pub choices: &'a dyn Fn(&str) -> Vec<(String, String)>,
    /// Settings → Dashboard → snap: nodes move in steps of this many units.
    pub snap: f32,
}

/// A file or folder for a field, picked with the system's dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browse {
    pub workspace: String,
    pub node: String,
    pub key: String,
    pub folder: bool,
    pub title: String,
    pub start: String,
}

/// What the page asks the app for after a frame.
#[derive(Debug, Default)]
pub struct Out {
    /// A hotkey field is recording (true) or stopped: the app's own hotkeys must not fire meanwhile.
    pub suspend_keys: Option<bool>,
    pub browse: Option<Browse>,
    /// The switch said "All paused": resume every automation.
    pub resume: bool,
    /// Export to a file…: the name to suggest, and the workspace as a `.lumflow` holds it.
    pub export: Option<(String, Value)>,
    /// Import from a file…
    pub import: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Drag {
    None,
    /// Moving the canvas with `button`, from where the pointer was last.
    Pan {
        last: Pos2,
        start: Pos2,
        button: PointerButton,
        moved: bool,
    },
    /// Moving the selected nodes: where the pointer started on the canvas, and where each node was.
    Move {
        start: Pos2,
        origin: Vec<(String, f32, f32)>,
    },
    /// A selection box from this canvas point.
    Band {
        start: Pos2,
    },
    /// A wire being pulled out of a port.
    Wire {
        node: String,
        port: String,
    },
}

/// What a right-click opened a menu on.
#[derive(Debug, Clone)]
enum MenuOn {
    Node(String),
    Wire(String),
    Canvas,
    /// The ▾ on the dock: switch, new, rename, export...
    Workspaces,
}

/// What a menu entry does, once the menu is gone.
type MenuAct = Box<dyn FnOnce(&mut AutomationUi)>;

struct Menu {
    at: Pos2,
    scene: Pos2,
    on: MenuOn,
}

enum Asking {
    Rename(String),
    /// A new workspace, a copy of this one when given.
    NewWorkspace(Option<String>),
    RenameWorkspace(String),
    DeleteWorkspace(String),
}

struct Ask {
    what: Asking,
    dialog: AskDialog,
}

/// How a change reaches the engine.
enum Push {
    /// Nodes or wires came or went: the engine indexes the wires again and starts new nodes.
    Structure,
    /// One node's field: only a trigger restarts.
    Field(String),
    /// Positions and names: saved, nothing restarts.
    Quiet,
}

pub struct AutomationUi {
    handle: Option<Handle>,
    wid: String,
    ws: Value,
    tabs: Vec<(String, String, bool)>,
    paused: bool,
    running: bool,
    stale: bool,
    watching: bool,
    view: View,
    selected: BTreeSet<String>,
    wires_selected: BTreeSet<String>,
    order: Vec<String>,
    drag: Drag,
    space: bool,
    picker: Option<Picker>,
    menu: Option<Menu>,
    ask: Option<Ask>,
    recording: Option<(String, String)>,
    /// Where the field editors were last frame, on the screen: a right-drag or the wheel there is theirs.
    field_rects: Vec<Rect>,
    /// Nodes and wires that glow for a moment after a pulse: until when.
    flash: HashMap<String, f64>,
    heat: HashMap<String, f64>,
    statuses: HashMap<String, Status>,
    live: bool,
    /// A node type picked from the right-click menu, added once the menu is gone.
    pending_add: Option<(String, Pos2)>,
    pulses: pulses::Pulses,
    /// Earlier versions of each workspace's nodes and wires, newest last.
    undo: HashMap<String, Vec<Value>>,
    redo: HashMap<String, Vec<Value>>,
    /// The field typed in last and when: typing in one field is one step to undo.
    last_step: (String, f64),
    /// The workspace as it was when a node was pressed, kept if the press moves it.
    before_move: Option<Value>,
    /// Copied nodes and the wires between them, for Paste in any workspace.
    clipboard: Option<Value>,
    vars: Option<vars::VarsPanel>,
    out: Out,
}

impl Default for AutomationUi {
    fn default() -> AutomationUi {
        AutomationUi::new()
    }
}

/// How long a node glows and a wire stays hot after a pulse.
const FLASH: f64 = 0.45;
const HEAT: f64 = 0.5;

impl AutomationUi {
    pub fn new() -> AutomationUi {
        AutomationUi {
            handle: None,
            wid: String::new(),
            ws: Value::Null,
            tabs: Vec::new(),
            paused: false,
            running: false,
            stale: true,
            watching: false,
            view: View::default(),
            selected: BTreeSet::new(),
            wires_selected: BTreeSet::new(),
            order: Vec::new(),
            drag: Drag::None,
            space: false,
            picker: None,
            menu: None,
            ask: None,
            recording: None,
            field_rects: Vec::new(),
            flash: HashMap::new(),
            heat: HashMap::new(),
            statuses: HashMap::new(),
            live: false,
            pending_add: None,
            pulses: pulses::Pulses::default(),
            undo: HashMap::new(),
            redo: HashMap::new(),
            last_step: (String::new(), f64::NEG_INFINITY),
            before_move: None,
            clipboard: None,
            vars: None,
            out: Out::default(),
        }
    }

    /// The engine this page edits.
    pub fn set_engine(&mut self, handle: Handle) {
        self.handle = Some(handle);
        self.stale = true;
    }

    pub fn take_out(&mut self) -> Out {
        std::mem::take(&mut self.out)
    }

    /// The shown workspace's id.
    pub fn workspace_id(&self) -> &str {
        &self.wid
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Whether the node picker is open.
    pub fn picking(&self) -> bool {
        self.picker.is_some()
    }

    /// The selected nodes' ids.
    pub fn selection(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    /// The overlay went away or shows another page: no pulses for the animation, the view is remembered.
    pub fn hidden(&mut self) {
        if let Some(h) = &self.handle
            && self.watching
        {
            h.with(|e| e.watch_pulses(false));
        }
        self.watching = false;
        self.save_view();
        self.picker = None;
        self.menu = None;
        self.drag = Drag::None;
        if self.recording.take().is_some() {
            self.out.suspend_keys = Some(false);
        }
    }

    /// A page that failed while drawing starts again from what the engine has.
    pub fn reset(&mut self) {
        let handle = self.handle.take();
        let clipboard = self.clipboard.take();
        *self = AutomationUi::new();
        self.handle = handle;
        self.clipboard = clipboard;
    }

    /// Shows that workspace (an import, a click on its tab).
    pub fn show_workspace(&mut self, wid: &str) {
        self.save_view();
        if let Some(h) = &self.handle {
            h.with(|e| e.set_active(wid));
        }
        self.wid = String::new();
        self.stale = true;
        self.pulses.clear();
    }

    /// Pulses on their way, for tests and the frame budget.
    pub fn pulses_shown(&self) -> usize {
        self.pulses.len()
    }

    /// The file dialog a field asked for was answered.
    pub fn file_chosen(&mut self, browse: &Browse, path: Option<std::path::PathBuf>) {
        let Some(path) = path else { return };
        if browse.workspace != self.wid {
            return;
        }
        self.set_field(
            &browse.node,
            &browse.key,
            Value::String(path.to_string_lossy().into_owned()),
            f64::NAN,
        );
    }

    fn nodes(&self) -> &[Value] {
        self.ws["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[])
    }

    fn wires(&self) -> &[Value] {
        self.ws["wires"].as_array().map(Vec::as_slice).unwrap_or(&[])
    }

    fn node(&self, nid: &str) -> Option<&Value> {
        doc::node_in(&self.ws, nid)
    }

    fn node_mut(&mut self, nid: &str) -> Option<&mut Value> {
        doc::node_in_mut(&mut self.ws, nid)
    }

    // Keeping in step with the engine

    fn sync(&mut self, handle: &Handle, now: f64, animate: bool) {
        let updates = handle.take_updates();
        if updates.workspaces {
            self.stale = true;
        }
        if updates.vars
            && let Some(panel) = &mut self.vars
        {
            panel.engine_changed();
        }
        for pulse in updates.pulses.iter().filter(|p| p.workspace == self.wid) {
            if animate {
                self.pulses.add(&pulse.wire, &pulse.text, now);
            }
            self.heat.insert(pulse.wire.clone(), now + HEAT);
            let ends: Vec<String> = self
                .wires()
                .iter()
                .filter(|w| w["id"] == pulse.wire)
                .flat_map(|w| ["src", "dst"].map(|end| w[end][0].as_str().unwrap_or_default().to_owned()))
                .collect();
            for nid in ends {
                self.flash.insert(nid, now + FLASH);
            }
        }
        let stale = std::mem::take(&mut self.stale);
        let (wid, fresh) = handle.read(|e| {
            let wid = if e.workspace(&self.wid).is_some() { self.wid.clone() } else { e.active() };
            let fresh = (stale || wid != self.wid).then(|| e.workspace(&wid).cloned().unwrap_or(Value::Null));
            self.tabs = doc::workspaces(e.doc())
                .iter()
                .map(|w| {
                    (doc::str_of(w, "id").to_owned(), doc::str_of(w, "name").to_owned(), w["enabled"] == true)
                })
                .collect();
            self.paused = e.paused();
            self.running = e.is_running(&wid);
            self.statuses.clear();
            self.live = false;
            let ws = fresh.as_ref().unwrap_or(&self.ws);
            for n in ws["nodes"].as_array().into_iter().flatten() {
                let nid = doc::str_of(n, "id");
                if let Some(status) = e.info(&wid, nid) {
                    self.live |= e.is_live(&wid, nid);
                    self.statuses.insert(nid.to_owned(), status);
                }
            }
            (wid, fresh)
        });
        if let Some(ws) = fresh {
            let switched = wid != self.wid;
            self.wid = wid;
            self.ws = ws;
            if switched {
                self.view = View::of(&self.ws["view"]);
                self.selected.clear();
                self.wires_selected.clear();
                self.drag = Drag::None;
                self.pulses.clear();
            }
        }
        let ids: Vec<String> = self.nodes().iter().map(|n| doc::str_of(n, "id").to_owned()).collect();
        self.order.retain(|id| ids.contains(id));
        for id in ids {
            if !self.order.contains(&id) {
                self.order.push(id);
            }
        }
        self.selected.retain(|id| self.order.contains(id));
        self.flash.retain(|_, until| *until > now);
        self.heat.retain(|_, until| *until > now);
    }

    /// Hands the workspace's nodes and wires to the engine.
    fn push(&mut self, how: Push) {
        let Some(handle) = &self.handle else { return };
        let (wid, nodes, wires) = (self.wid.clone(), self.ws["nodes"].clone(), self.ws["wires"].clone());
        handle.with(|e| {
            if let Some(ws) = doc::workspace_mut(e.doc_mut(), &wid) {
                ws["nodes"] = nodes;
                ws["wires"] = wires;
            }
            match how {
                Push::Structure => e.edited(&wid, true, ""),
                Push::Field(nid) => e.edited(&wid, false, &nid),
                Push::Quiet => {}
            }
        });
    }

    fn snapshot(&self) -> Value {
        json!({"nodes": self.ws["nodes"], "wires": self.ws["wires"]})
    }

    /// Remembers the workspace before a change. Typing in one field (`key`) counts as one change.
    fn step(&mut self, key: &str, now: f64) {
        if !key.is_empty() && self.last_step.0 == key && now - self.last_step.1 < 1.5 {
            self.last_step.1 = now;
            return;
        }
        self.last_step = (key.to_owned(), now);
        let snap = self.snapshot();
        self.remember(snap);
    }

    fn remember(&mut self, snap: Value) {
        let stack = self.undo.entry(self.wid.clone()).or_default();
        stack.push(snap);
        if stack.len() > 60 {
            stack.remove(0);
        }
        self.redo.insert(self.wid.clone(), Vec::new());
    }

    fn can_undo(&self, redo: bool) -> bool {
        let stacks = if redo { &self.redo } else { &self.undo };
        stacks.get(&self.wid).is_some_and(|s| !s.is_empty())
    }

    /// Ctrl+Z (or Ctrl+Y): the workspace as it was, every node started again.
    pub fn undo(&mut self, redo: bool) {
        let from = if redo { &mut self.redo } else { &mut self.undo };
        let Some(snap) = from.get_mut(&self.wid).and_then(Vec::pop) else { return };
        let current = self.snapshot();
        let to = if redo { &mut self.undo } else { &mut self.redo };
        to.entry(self.wid.clone()).or_default().push(current);
        self.ws["nodes"] = snap["nodes"].clone();
        self.ws["wires"] = snap["wires"].clone();
        self.last_step = (String::new(), f64::NEG_INFINITY);
        let still: BTreeSet<String> = self.nodes().iter().map(|n| doc::str_of(n, "id").to_owned()).collect();
        self.selected.retain(|id| still.contains(id));
        self.wires_selected.clear();
        let Some(handle) = &self.handle else { return };
        let (wid, nodes, wires) = (self.wid.clone(), snap["nodes"].clone(), snap["wires"].clone());
        handle.with(|e| {
            if let Some(ws) = doc::workspace_mut(e.doc_mut(), &wid) {
                ws["nodes"] = nodes;
                ws["wires"] = wires;
            }
            e.replaced(&wid);
        });
    }

    /// Ctrl+C: the selected nodes and the wires between them.
    pub fn copy(&mut self) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        let nodes: Vec<Value> =
            self.nodes().iter().filter(|n| self.selected.contains(doc::str_of(n, "id"))).cloned().collect();
        let inside = |w: &Value, end: &str| w[end][0].as_str().is_some_and(|id| self.selected.contains(id));
        let wires: Vec<Value> =
            self.wires().iter().filter(|w| inside(w, "src") && inside(w, "dst")).cloned().collect();
        self.clipboard = Some(json!({"nodes": nodes, "wires": wires}));
        true
    }

    /// Ctrl+V: the copied nodes with new ids, their top-left corner at `at` (40 down and right without one).
    pub fn paste(&mut self, at: Option<Pos2>, now: f64) {
        let Some(mut clip) = self.clipboard.clone() else { return };
        let Some(list) = clip["nodes"].as_array().filter(|l| !l.is_empty()) else { return };
        let x0 = list.iter().filter_map(|n| n["x"].as_f64()).fold(f64::INFINITY, f64::min);
        let y0 = list.iter().filter_map(|n| n["y"].as_f64()).fold(f64::INFINITY, f64::min);
        let at = at.map_or((x0 + 40.0, y0 + 40.0), |p| (f64::from(p.x), f64::from(p.y)));
        self.step("", now);
        let mut ids = HashMap::new();
        let mut added = BTreeSet::new();
        for n in clip["nodes"].as_array_mut().into_iter().flatten() {
            let id = doc::new_id();
            ids.insert(doc::str_of(n, "id").to_owned(), id.clone());
            n["id"] = json!(id);
            n["x"] = json!(n["x"].as_f64().unwrap_or(0.0) - x0 + at.0);
            n["y"] = json!(n["y"].as_f64().unwrap_or(0.0) - y0 + at.1);
            n["state"] = json!({});
            added.insert(id);
        }
        for w in clip["wires"].as_array_mut().into_iter().flatten() {
            w["id"] = json!(doc::new_id());
            for end in ["src", "dst"] {
                let old = w[end][0].as_str().unwrap_or_default().to_owned();
                w[end][0] = json!(ids.get(&old).cloned().unwrap_or(old));
            }
        }
        for key in ["nodes", "wires"] {
            let new = clip[key].as_array().cloned().unwrap_or_default();
            if let Some(list) = self.ws[key].as_array_mut() {
                list.extend(new);
            }
        }
        self.selected = added;
        self.wires_selected.clear();
        self.push(Push::Structure);
    }

    /// Ctrl+D: a copy of the selected nodes, 40 down and right.
    pub fn duplicate(&mut self, now: f64) {
        let corner = self
            .nodes()
            .iter()
            .filter(|n| self.selected.contains(doc::str_of(n, "id")))
            .map(|n| (n["x"].as_f64().unwrap_or(0.0), n["y"].as_f64().unwrap_or(0.0)))
            .fold((f64::INFINITY, f64::INFINITY), |a, b| (a.0.min(b.0), a.1.min(b.1)));
        if self.copy() {
            self.paste(Some(pos2(corner.0 as f32 + 40.0, corner.1 as f32 + 40.0)), now);
        }
    }

    fn save_view(&mut self) {
        let Some(handle) = &self.handle else { return };
        if self.wid.is_empty() {
            return;
        }
        let view = self.view.to_json();
        if self.ws["view"] == view {
            return;
        }
        self.ws["view"] = view.clone();
        let wid = self.wid.clone();
        handle.with(|e| {
            if let Some(ws) = doc::workspace_mut(e.doc_mut(), &wid) {
                ws["view"] = view;
            }
        });
    }

    // Editing

    /// A new node of type `tid` at `at` on the canvas, wired to `link` when a wire was dragged out for it.
    pub fn add_node(
        &mut self,
        ui: &Ui,
        tid: &str,
        at: Pos2,
        link: Option<(String, String)>,
        snap: f32,
    ) -> Option<String> {
        let spec = lum_automation::spec::spec(tid)?;
        let mut n = doc::node(tid, 0.0, 0.0);
        let (x, y) = (canvas::snap(at.x, snap), canvas::snap(at.y, snap));
        n["x"] = canvas::number(x, snap);
        n["y"] = canvas::number(y, snap);
        for f in spec.fields.iter().filter(|f| f.kind == "date") {
            // Python's date box starts on tomorrow when it has no date yet
            if n["fields"][&f.key].as_str().is_none_or(str::is_empty) {
                let tomorrow =
                    jiff::ZonedDateTime::now().date().tomorrow().map(|d| d.to_string()).unwrap_or_default();
                n["fields"][&f.key] = json!(tomorrow);
            }
        }
        let nid = doc::str_of(&n, "id").to_owned();
        let now = ui.input(|i| i.time);
        self.step("", now);
        if let Some(list) = self.ws["nodes"].as_array_mut() {
            list.push(n);
        }
        self.order.push(nid.clone());
        if let Some((from, port)) = link {
            let ports = node::ports_of(spec);
            if let Some(other) = ports.iter().find(|p| compatible(&port, p)) {
                // the new node sits so its port is under the pointer
                let tokens = Tokens::of(ui.ctx());
                if let Some(at_port) = canvas::port_offset(ui.painter(), &tokens, spec, other) {
                    let nx = if is_source(&port) { at.x } else { at.x - at_port.x };
                    let (nx, ny) = (canvas::snap(nx, snap), canvas::snap(at.y - at_port.y, snap));
                    if let Some(n) = self.node_mut(&nid) {
                        n["x"] = json!(f64::from(nx));
                        n["y"] = json!(f64::from(ny));
                    }
                }
                self.connect(&from, &port, &nid, other);
            }
        }
        self.push(Push::Structure);
        self.selected = BTreeSet::from([nid.clone()]);
        self.wires_selected.clear();
        Some(nid)
    }

    /// Wires two ports (in either order). A value field takes one wire: a new one replaces the old.
    fn connect(&mut self, a: &str, pa: &str, b: &str, pb: &str) -> bool {
        if !compatible(pa, pb) {
            return false;
        }
        let (a, pa, b, pb) = if is_source(pa) { (a, pa, b, pb) } else { (b, pb, a, pa) };
        let exists = self.wires().iter().any(|w| w["src"] == json!([a, pa]) && w["dst"] == json!([b, pb]));
        if exists {
            return false;
        }
        let Some(list) = self.ws["wires"].as_array_mut() else { return false };
        if pb.starts_with("field:") {
            list.retain(|w| w["dst"] != json!([b, pb]));
        }
        list.push(json!({"id": doc::new_id(), "src": [a, pa], "dst": [b, pb]}));
        true
    }

    pub fn delete_nodes(&mut self, ids: &BTreeSet<String>) {
        if ids.is_empty() {
            return;
        }
        self.step("", f64::NAN);
        if let Some(list) = self.ws["nodes"].as_array_mut() {
            list.retain(|n| !ids.contains(doc::str_of(n, "id")));
        }
        if let Some(list) = self.ws["wires"].as_array_mut() {
            list.retain(|w| {
                !ids.contains(w["src"][0].as_str().unwrap_or_default())
                    && !ids.contains(w["dst"][0].as_str().unwrap_or_default())
            });
        }
        self.selected.retain(|id| !ids.contains(id));
        self.push(Push::Structure);
    }

    fn delete_wires(&mut self, ids: &BTreeSet<String>) {
        if ids.is_empty() {
            return;
        }
        self.step("", f64::NAN);
        if let Some(list) = self.ws["wires"].as_array_mut() {
            list.retain(|w| !ids.contains(doc::str_of(w, "id")));
        }
        self.wires_selected.retain(|id| !ids.contains(id));
        self.push(Push::Structure);
    }

    fn set_field(&mut self, nid: &str, key: &str, value: Value, now: f64) {
        if self.node(nid).is_none_or(|n| n["fields"][key] == value) {
            return;
        }
        self.step(&format!("{nid}:{key}"), now);
        let Some(n) = self.node_mut(nid) else { return };
        n["fields"][key] = value;
        self.push(Push::Field(nid.to_owned()));
    }

    fn rename(&mut self, nid: &str, title: &str) {
        self.step("", f64::NAN);
        let Some(n) = self.node_mut(nid) else { return };
        n["title"] = json!(title.trim());
        self.push(Push::Quiet);
    }

    fn toggle_off(&mut self, ids: &BTreeSet<String>) {
        self.step("", f64::NAN);
        let off = !ids.iter().all(|id| self.node(id).is_some_and(|n| n["off"] == true));
        for id in ids {
            if let Some(n) = self.node_mut(id) {
                n["off"] = json!(off);
            }
        }
        self.push(Push::Structure);
    }

    fn run_node(&mut self, nid: &str, now: f64) {
        self.flash.insert(nid.to_owned(), now + FLASH);
        if let Some(h) = &self.handle {
            let wid = self.wid.clone();
            h.with(|e| e.run_node(&wid, nid, ""));
        }
    }

    fn select_all(&mut self) {
        self.selected = self.order.iter().cloned().collect();
    }

    /// Shows every node (F): the view moves and zooms to fit them, never closer than 120 %.
    fn fit(&mut self, rect: Rect, layouts: &canvas::Layouts) {
        self.view = canvas::fit(rect, layouts);
        self.save_view();
    }

    fn set_workspace_on(&mut self, on: bool) {
        let Some(h) = &self.handle else { return };
        let wid = self.wid.clone();
        h.with(|e| e.set_enabled(&wid, on));
        self.stale = true;
    }

    // Drawing

    /// Draws the page into `rect`.
    pub fn page(&mut self, ui: &mut Ui, rect: Rect, env: &Env<'_>) -> PageFrame {
        let tokens = Tokens::of(ui.ctx());
        let Some(handle) = self.handle.clone() else {
            ui.put(
                rect,
                egui::Label::new(
                    egui::RichText::new("Automation is not running.").color(tokens.p.text_muted),
                ),
            );
            return PageFrame::default();
        };
        if !self.watching {
            handle.with(|e| e.watch_pulses(true));
            self.watching = true;
            self.stale = true;
        }
        let now = ui.input(|i| i.time);
        let animate = crate::anim::Motion::of(ui.ctx()).on(crate::anim::Kind::Pulses);
        self.sync(&handle, now, animate);
        let mut escape_used = false;
        let layouts = canvas::layouts(ui.painter(), &tokens, self.nodes());

        let bg = ui.interact(rect, ui.scope_id().with("automation-canvas-bg"), Sense::click_and_drag());
        canvas::background(ui.painter(), &tokens, rect, &self.view);
        self.view.glide(ui.ctx(), rect);
        let modal = self.picker.is_some() || self.menu.is_some() || self.ask.is_some() || self.vars.is_some();
        if !modal {
            self.pointer(ui, rect, &bg, &layouts, env, now);
            escape_used = self.keys(ui, rect, &layouts);
        }

        let field_events = self.draw_canvas(ui, rect, &layouts, &tokens, env, now);
        for (nid, key, did) in field_events {
            match did {
                node::FieldDid::Changed(v) => {
                    self.set_field(&nid, &key, v, now);
                    if self.recording.as_ref().is_some_and(|(n, k)| *n == nid && *k == key) {
                        self.recording = None;
                        self.out.suspend_keys = Some(false);
                    }
                }
                node::FieldDid::Record(on) => {
                    self.recording = on.then(|| (nid.clone(), key.clone()));
                    self.out.suspend_keys = Some(on);
                }
                node::FieldDid::Browse => self.browse(&nid, &key),
            }
        }

        self.dock(ui, rect, &tokens, &layouts);
        self.show_menu(ui, rect, &layouts, now);
        self.show_picker(ui, rect, env);
        self.show_ask(ui);
        self.show_vars(ui);

        let moving = self.pulses.step(now);
        let ctx = ui.ctx();
        if moving || !self.flash.is_empty() || !self.heat.is_empty() {
            ctx.request_repaint();
        } else if self.live {
            // countdowns move by themselves
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let typing = ctx.egui_wants_keyboard_input();
        PageFrame {
            busy: escape_used
                || typing
                || self.recording.is_some()
                || self.picker.is_some()
                || self.menu.is_some()
                || self.ask.is_some()
                || self.vars.is_some(),
        }
    }

    fn browse(&mut self, nid: &str, key: &str) {
        let Some(n) = self.node(nid) else { return };
        let Some(spec) = lum_automation::spec::spec(doc::str_of(n, "type")) else { return };
        let Some(f) = spec.field(key) else { return };
        let start = n["fields"][key].as_str().filter(|s| !s.is_empty()).unwrap_or("~").to_owned();
        let title = if f.label.is_empty() {
            if f.kind == "folder" { "Folder" } else { "File" }.to_owned()
        } else {
            f.label.clone()
        };
        self.out.browse = Some(Browse {
            workspace: self.wid.clone(),
            node: nid.to_owned(),
            key: key.to_owned(),
            folder: f.kind == "folder",
            title,
            start,
        });
    }

    /// Pans, the wheel, the selection box, and wires being dragged.
    fn pointer(
        &mut self,
        ui: &Ui,
        rect: Rect,
        bg: &egui::Response,
        layouts: &canvas::Layouts,
        env: &Env<'_>,
        now: f64,
    ) {
        let (hover, pressed, wheel, released, any_down) = ui.input(|i| {
            let pressed = [PointerButton::Primary, PointerButton::Secondary, PointerButton::Middle]
                .into_iter()
                .find(|b| i.pointer.button_pressed(*b));
            let wheel: f32 = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                        egui::MouseWheelUnit::Line => delta.y,
                        egui::MouseWheelUnit::Point => delta.y / 40.0,
                        egui::MouseWheelUnit::Page => delta.y * 3.0,
                    }),
                    _ => None,
                })
                .sum();
            (i.pointer.hover_pos(), pressed, wheel, i.pointer.any_released(), i.pointer.any_down())
        });
        let over_field = |p: Pos2| self.field_rects.iter().any(|r| r.contains(p));
        if let Some(at) = hover.filter(|p| rect.contains(*p) && !over_field(*p))
            && wheel != 0.0
        {
            self.view.zoom_by(ui.ctx(), rect, at, 1.15f32.powf(wheel));
        }
        if let (Some(button), Some(at)) = (pressed, hover)
            && rect.contains(at)
            && !over_field(at)
            && (button != PointerButton::Primary || self.space)
        {
            self.drag = Drag::Pan { last: at, start: at, button, moved: false };
        }
        match self.drag.clone() {
            Drag::Pan { last, start, button, moved } => {
                if let Some(at) = hover {
                    let moved = moved || (at - start).length() > 4.0;
                    if moved {
                        self.view.pan(at - last);
                    }
                    self.drag = Drag::Pan { last: at, start, button, moved };
                }
                if released || !any_down {
                    self.drag = Drag::None;
                    if button == PointerButton::Secondary
                        && !moved
                        && let Some(at) = hover
                    {
                        self.open_menu(at, rect, layouts);
                    }
                    self.save_view();
                }
            }
            Drag::Band { start } => {
                if released || !any_down {
                    if let Some(at) = hover {
                        let band = Rect::from_two_pos(start, self.view.to_scene(rect, at));
                        let add = ui.input(|i| i.modifiers.ctrl);
                        if !add {
                            self.selected.clear();
                        }
                        self.selected.extend(
                            layouts
                                .iter()
                                .filter(|(_, (o, l))| band.intersects(Rect::from_min_size(*o, l.layout.size)))
                                .map(|(id, _)| id.clone()),
                        );
                    }
                    self.drag = Drag::None;
                }
            }
            Drag::Wire { node, port } => {
                if released || !any_down {
                    self.drag = Drag::None;
                    if let Some(at) = hover {
                        self.end_wire(&node, &port, self.view.to_scene(rect, at), at, layouts, env);
                    }
                }
            }
            Drag::Move { .. } | Drag::None => {}
        }
        if bg.drag_started_by(PointerButton::Primary)
            && !self.space
            && self.drag == Drag::None
            && let Some(at) = ui.input(|i| i.pointer.press_origin())
        {
            self.drag = Drag::Band { start: self.view.to_scene(rect, at) };
        }
        if bg.clicked() {
            let at = bg.interact_pointer_pos().map(|p| self.view.to_scene(rect, p));
            let wire =
                at.and_then(|p| canvas::wire_at(p, self.wires(), layouts, 6.0 / self.view.zoom.max(0.2)));
            let alt = ui.input(|i| i.modifiers.alt);
            match wire {
                Some(id) if alt => self.delete_wires(&BTreeSet::from([id])),
                Some(id) => {
                    self.selected.clear();
                    self.wires_selected = BTreeSet::from([id]);
                }
                None => {
                    self.selected.clear();
                    self.wires_selected.clear();
                }
            }
        }
        if bg.double_clicked()
            && let Some(at) = bg.interact_pointer_pos()
        {
            self.picker = Some(Picker::new(at, self.view.to_scene(rect, at), None));
        }
        let _ = now;
    }

    fn end_wire(
        &mut self,
        from: &str,
        port: &str,
        at: Pos2,
        screen: Pos2,
        layouts: &canvas::Layouts,
        _env: &Env<'_>,
    ) {
        let hit = canvas::port_at(at, layouts, 18.0 / self.view.zoom.max(0.2));
        match hit {
            Some((nid, other)) if compatible(port, &other) => {
                let before = self.snapshot();
                if self.connect(from, port, &nid, &other) {
                    self.remember(before);
                    self.push(Push::Structure);
                }
            }
            None => {
                let start = layouts.get(from).and_then(|(o, l)| l.port(port).map(|p| *o + p.to_vec2()));
                let far = start.is_some_and(|s| (at - s).abs().x + (at - s).abs().y > 30.0);
                if far {
                    self.picker = Some(Picker::new(screen, at, Some((from.to_owned(), port.to_owned()))));
                }
            }
            Some(_) => {}
        }
    }

    /// Keys the page answers while no text box has the keyboard. Gives back whether it used Esc.
    fn keys(&mut self, ui: &Ui, rect: Rect, layouts: &canvas::Layouts) -> bool {
        let ctx = ui.ctx();
        let typing = ctx.egui_wants_keyboard_input();
        let (space_down, space_up) = ui.input(|i| {
            let mut down = None;
            let mut up = None;
            for e in &i.events {
                if let egui::Event::Key { key: Key::Space, pressed, repeat: false, .. } = e {
                    if *pressed { down = Some(()) } else { up = Some(()) }
                }
            }
            (down.is_some(), up.is_some())
        });
        if space_down && !typing {
            self.space = true;
        }
        if space_up {
            self.space = false;
        }
        if typing {
            return false;
        }
        let mut used_escape = false;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                if matches!(self.drag, Drag::Wire { .. }) {
                    self.drag = Drag::None;
                    used_escape = true;
                } else if !self.selected.is_empty() || !self.wires_selected.is_empty() {
                    self.selected.clear();
                    self.wires_selected.clear();
                    used_escape = true;
                } else {
                    // nothing to drop here: the overlay's Esc closes it
                    i.events.push(egui::Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
        });
        let (delete, select_all, fit) = ctx.input_mut(|i| {
            let delete =
                i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace);
            (delete, i.consume_key(Modifiers::COMMAND, Key::A), i.consume_key(Modifiers::NONE, Key::F))
        });
        let shift_command = Modifiers::COMMAND | Modifiers::SHIFT;
        let (undo, redo, copy, paste, duplicate) = ctx.input_mut(|i| {
            let redo = i.consume_key(shift_command, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y);
            (
                i.consume_key(Modifiers::COMMAND, Key::Z),
                redo,
                i.consume_key(Modifiers::COMMAND, Key::C)
                    || i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.consume_key(Modifiers::COMMAND, Key::V)
                    || i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
                i.consume_key(Modifiers::COMMAND, Key::D),
            )
        });
        let now = ui.input(|i| i.time);
        if undo || redo {
            self.undo(redo);
        }
        if copy {
            self.copy();
        }
        if paste {
            let at = ui
                .input(|i| i.pointer.hover_pos())
                .filter(|p| rect.contains(*p))
                .map(|p| self.view.to_scene(rect, p));
            self.paste(at, now);
        }
        if duplicate {
            self.duplicate(now);
        }
        if delete {
            if !self.selected.is_empty() {
                let ids = self.selected.clone();
                self.delete_nodes(&ids);
            } else if !self.wires_selected.is_empty() {
                let ids = self.wires_selected.clone();
                self.delete_wires(&ids);
            }
        }
        if select_all {
            self.select_all();
        }
        if fit {
            self.fit(rect, layouts);
        }
        used_escape
    }

    /// Wires, nodes and their fields on the canvas layer. Gives back what the field editors did.
    fn draw_canvas(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        layouts: &canvas::Layouts,
        tokens: &Tokens,
        env: &Env<'_>,
        now: f64,
    ) -> Vec<(String, String, node::FieldDid)> {
        let to_global = self.view.transform(rect);
        let layer = egui::LayerId::new(ui.layer_id().order, ui.scope_id().with("automation-canvas"));
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        ui.ctx().set_transform_layer(layer, to_global);
        let scene_rect = to_global.inverse() * rect;
        let mut canvas_ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(scene_rect));
        canvas_ui.set_clip_rect(scene_rect);
        let painter = canvas_ui.painter().clone();

        // wires under the nodes
        for w in self.wires() {
            let id = doc::str_of(w, "id");
            let heat = self.heat.get(id).map_or(0.0, |until| ((until - now) / HEAT).clamp(0.0, 1.0) as f32);
            canvas::paint_wire(&painter, tokens, w, layouts, self.wires_selected.contains(id), heat);
        }

        let pointer = canvas_ui.input(|i| i.pointer.hover_pos());
        let mut events = Vec::new();
        let mut field_rects = Vec::new();
        let wired: BTreeSet<(String, String)> = self
            .wires()
            .iter()
            .flat_map(|w| {
                ["src", "dst"].map(|end| {
                    (
                        w[end][0].as_str().unwrap_or_default().to_owned(),
                        w[end][1].as_str().unwrap_or_default().to_owned(),
                    )
                })
            })
            .collect();
        let order = self.order.clone();
        for nid in &order {
            let Some((origin, layout)) = layouts.get(nid) else { continue };
            let Some(n) = self.node(nid).cloned() else { continue };
            let spec = layout.spec;
            let node_rect = Rect::from_min_size(*origin, layout.layout.size);
            let id = egui::Id::unique(("automation-node", nid));
            let body = canvas_ui.interact(node_rect, id, Sense::click_and_drag());
            let off = n["off"] == true;
            let status = match self.statuses.get(nid) {
                _ if off => Status { text: "turned off".into(), progress: None },
                Some(s) => s.clone(),
                None if !self.running && spec.is_trigger() => {
                    Status { text: "workspace off".into(), progress: None }
                }
                None => Status::default(),
            };
            let title = match doc::str_of(&n, "title") {
                "" => spec.title.as_str(),
                t => t,
            };
            let is_wired = |port: &str| wired.contains(&(nid.clone(), port.to_owned()));
            let flash =
                self.flash.get(nid).map_or(0.0, |until| ((until - now) / FLASH).clamp(0.0, 1.0) as f32);
            let mut node_painter = painter.clone();
            if off {
                node_painter.multiply_opacity(0.45);
            }
            let look = node::Look {
                spec,
                title,
                selected: self.selected.contains(nid),
                flash,
                status: status.clone(),
                wired: &is_wired,
            };
            node::paint(&node_painter, tokens, *origin, &layout.layout, &look);
            body.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{title}: {}", status.text))
            });

            if let Some(run) = layout.layout.run {
                let run = canvas_ui.interact(run.translate(origin.to_vec2()), id.with("run"), Sense::click());
                run.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Run {title}"))
                });
                if run.clicked() {
                    self.run_node(nid, now);
                }
            }
            for (port, at) in &layout.layout.ports {
                let at = *origin + at.to_vec2();
                let r = canvas_ui.interact(
                    Rect::from_center_size(at, vec2(16.0, 16.0)),
                    id.with(port),
                    Sense::drag(),
                );
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{title} {port}"))
                });
                if r.hovered() {
                    canvas_ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                if r.drag_started_by(PointerButton::Primary) && !self.space {
                    self.begin_wire(nid, port);
                }
            }
            let mut fields_ui =
                canvas_ui.new_child(egui::UiBuilder::new().max_rect(node_rect).id_salt(id.with("fields")));
            if off {
                fields_ui.multiply_opacity(0.45);
            }
            let recording = self.recording.as_ref().filter(|(n, _)| n == nid).map(|(_, k)| k.as_str());
            let fenv = node::FieldEnv { choices: &|name| self.choices(name, env), recording };
            for (key, did) in
                node::fields(&mut fields_ui, *origin, nid, &layout.layout, &n["fields"], &is_wired, &fenv)
            {
                events.push((nid.clone(), key, did));
            }
            for row in &layout.layout.fields {
                field_rects.push(to_global * row.editor.translate(origin.to_vec2()));
            }

            if body.double_clicked() && pointer.is_some_and(|p| p.y <= origin.y + node::HEAD) {
                self.ask = Some(Ask {
                    what: Asking::Rename(nid.clone()),
                    dialog: AskDialog::ask_text(
                        egui::Id::unique("automation-rename"),
                        "Rename node",
                        "Name (empty = its type):",
                        doc::str_of(&n, "title"),
                    ),
                });
            } else if body.clicked() {
                let ctrl = canvas_ui.input(|i| i.modifiers.ctrl);
                self.wires_selected.clear();
                if ctrl {
                    if !self.selected.remove(nid) {
                        self.selected.insert(nid.clone());
                    }
                } else {
                    self.selected = BTreeSet::from([nid.clone()]);
                }
            }
            if body.drag_started_by(PointerButton::Primary) && !self.space {
                if !self.selected.contains(nid) {
                    let ctrl = canvas_ui.input(|i| i.modifiers.ctrl);
                    if !ctrl {
                        self.selected.clear();
                    }
                    self.selected.insert(nid.clone());
                }
                self.wires_selected.clear();
                let start =
                    canvas_ui.input(|i| i.pointer.press_origin()).map(|p| self.view.to_scene(rect, p));
                self.before_move = Some(self.snapshot());
                if let Some(start) = start {
                    let origin = self
                        .selected
                        .iter()
                        .filter_map(|id| layouts.get(id).map(|(o, _)| (id.clone(), o.x, o.y)))
                        .collect();
                    self.drag = Drag::Move { start, origin };
                }
                self.bring_to_front(nid);
            }
        }
        self.field_rects = field_rects;

        if let Drag::Move { start, origin } = self.drag.clone() {
            let (down, at) = canvas_ui.input(|i| (i.pointer.primary_down(), i.pointer.hover_pos()));
            if let Some(at) = at {
                let d = self.view.to_scene(rect, at) - start;
                for (id, x, y) in &origin {
                    let (nx, ny) = (canvas::snap(x + d.x, env.snap), canvas::snap(y + d.y, env.snap));
                    if let Some(n) = self.node_mut(id) {
                        n["x"] = json!(f64::from(nx));
                        n["y"] = json!(f64::from(ny));
                    }
                }
            }
            if !down {
                self.drag = Drag::None;
                let moved = origin.iter().any(|(id, x, y)| {
                    self.node(id).is_some_and(|n| {
                        n["x"].as_f64() != Some(f64::from(*x)) || n["y"].as_f64() != Some(f64::from(*y))
                    })
                });
                if moved {
                    if let Some(before) = self.before_move.take() {
                        self.remember(before);
                    }
                    self.push(Push::Quiet);
                }
                self.before_move = None;
            }
        }

        if let Drag::Wire { node, port } = &self.drag
            && let Some((origin, layout)) = layouts.get(node)
            && let Some(at) = layout.layout.port(port)
            && let Some(p) = pointer
        {
            let from = *origin + at.to_vec2();
            let hit = canvas::port_at(p, layouts, 18.0 / self.view.zoom.max(0.2));
            let to = match hit {
                Some((n, other)) if compatible(port, &other) => layouts
                    .get(&n)
                    .and_then(|(o, l)| l.layout.port(&other).map(|q| *o + q.to_vec2()))
                    .unwrap_or(p),
                _ => p,
            };
            let (a, b) = if is_source(port) { (from, to) } else { (to, from) };
            let colour = if node::is_value(port) { node::VALUE_COLOUR } else { tokens.p.primary };
            canvas::dashed_wire(&painter, a, b, egui::Stroke::new(2.0, colour));
        }
        let pulse_layer = egui::LayerId::new(ui.layer_id().order, ui.scope_id().with("automation-pulses"));
        ui.ctx().set_sublayer(layer, pulse_layer);
        ui.ctx().set_transform_layer(pulse_layer, to_global);
        let pulse_painter = ui.ctx().layer_painter(pulse_layer).with_clip_rect(scene_rect);
        self.pulses.paint(&pulse_painter, tokens, self.wires(), layouts, now);
        if let Drag::Band { start } = &self.drag
            && let Some(p) = pointer
        {
            let band = Rect::from_two_pos(*start, p);
            painter.rect_filled(band, 4.0, tokens.p.primary.gamma_multiply(0.12));
            painter.rect_stroke(
                band,
                4.0,
                egui::Stroke::new(1.0 / self.view.zoom, tokens.p.primary),
                egui::StrokeKind::Inside,
            );
        }
        events
    }

    fn bring_to_front(&mut self, nid: &str) {
        self.order.retain(|id| id != nid);
        self.order.push(nid.to_owned());
    }

    fn begin_wire(&mut self, nid: &str, port: &str) {
        let (mut nid, mut port) = (nid.to_owned(), port.to_owned());
        if !is_source(&port) {
            // dragging out of an input picks its wire up again (the last one plugged in)
            let plugged = self.wires().iter().rev().find(|w| w["dst"] == json!([nid, port])).cloned();
            if let Some(w) = plugged {
                let id = doc::str_of(&w, "id").to_owned();
                nid = w["src"][0].as_str().unwrap_or_default().to_owned();
                port = w["src"][1].as_str().unwrap_or_default().to_owned();
                self.delete_wires(&BTreeSet::from([id]));
            }
        }
        self.drag = Drag::Wire { node: nid, port };
    }

    fn choices(&self, name: &str, env: &Env<'_>) -> Vec<(String, String)> {
        if name == "_workspaces" {
            let mut out = vec![(String::new(), "This workspace".to_owned())];
            out.extend(self.tabs.iter().map(|(id, n, _)| (id.clone(), n.clone())));
            return out;
        }
        (env.choices)(name)
    }

    fn open_menu(&mut self, at: Pos2, rect: Rect, layouts: &canvas::Layouts) {
        let scene = self.view.to_scene(rect, at);
        let on_node = self.order.iter().rev().find(|id| {
            layouts.get(*id).is_some_and(|(o, l)| Rect::from_min_size(*o, l.layout.size).contains(scene))
        });
        let on = if let Some(nid) = on_node {
            if !self.selected.contains(nid) {
                self.selected = BTreeSet::from([nid.clone()]);
            }
            MenuOn::Node(nid.clone())
        } else if let Some(wire) =
            canvas::wire_at(scene, self.wires(), layouts, 6.0 / self.view.zoom.max(0.2))
        {
            MenuOn::Wire(wire)
        } else {
            MenuOn::Canvas
        };
        self.menu = Some(Menu { at, scene, on });
    }

    fn show_menu(&mut self, ui: &Ui, rect: Rect, layouts: &canvas::Layouts, now: f64) {
        let Some(m) = self.menu.take() else { return };
        let mut keep = true;
        let mut act: Option<MenuAct> = None;
        let area = egui::Area::new(egui::Id::unique("automation-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(m.at)
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    match &m.on {
                        MenuOn::Node(nid) => {
                            let Some(n) = self.node(nid) else { return };
                            let Some(spec) = lum_automation::spec::spec(doc::str_of(n, "type")) else {
                                return;
                            };
                            let sel = self.selected.clone();
                            let help =
                                if spec.help.is_empty() { spec.title.clone() } else { spec.help.clone() };
                            ui.add_enabled_ui(false, |ui| ui.add(egui::Label::new(help).wrap()));
                            menu::separator(ui);
                            let nid = nid.clone();
                            if (spec.is_trigger() || !spec.ins.is_empty())
                                && menu::item(ui, "▶  Run it now").clicked()
                            {
                                let id = nid.clone();
                                act = Some(Box::new(move |s| s.run_node(&id, now)));
                            }
                            if menu::item(ui, "Rename…").clicked() {
                                let title = doc::str_of(n, "title").to_owned();
                                let id = nid.clone();
                                act = Some(Box::new(move |s| {
                                    s.ask = Some(Ask {
                                        what: Asking::Rename(id),
                                        dialog: AskDialog::ask_text(
                                            egui::Id::unique("automation-rename"),
                                            "Rename node",
                                            "Name (empty = its type):",
                                            &title,
                                        ),
                                    })
                                }));
                            }
                            if menu::item(ui, "Duplicate   (Ctrl+D)").clicked() {
                                act = Some(Box::new(move |s| s.duplicate(now)));
                            }
                            if menu::item(ui, "Copy   (Ctrl+C)").clicked() {
                                act = Some(Box::new(|s| {
                                    s.copy();
                                }));
                            }
                            let all_off =
                                sel.iter().all(|id| self.node(id).is_some_and(|n| n["off"] == true));
                            let label =
                                if all_off { "Turn on" } else { "Turn off (it stays, but does nothing)" };
                            if menu::item(ui, label).clicked() {
                                let ids = sel.clone();
                                act = Some(Box::new(move |s| s.toggle_off(&ids)));
                            }
                            menu::separator(ui);
                            let delete = if sel.len() > 1 {
                                format!("Delete {} nodes", sel.len())
                            } else {
                                "Delete   (Del)".to_owned()
                            };
                            if menu::item(ui, &delete).clicked() {
                                act = Some(Box::new(move |s| s.delete_nodes(&sel)));
                            }
                        }
                        MenuOn::Workspaces => self.workspace_menu(ui, &mut act),
                        MenuOn::Wire(wid) => {
                            if menu::item(ui, "Delete wire").clicked() {
                                let ids = BTreeSet::from([wid.clone()]);
                                act = Some(Box::new(move |s| s.delete_wires(&ids)));
                            }
                        }
                        MenuOn::Canvas => {
                            let specs = lum_automation::spec::specs();
                            for cat in &specs.categories {
                                ui.menu_button(&cat.label, |ui| {
                                    for t in specs.nodes.iter().filter(|t| t.category == cat.id) {
                                        if menu::item(ui, &t.title).on_hover_text(&t.help).clicked() {
                                            let (tid, at) = (t.id.clone(), m.scene);
                                            act = Some(Box::new(move |s| {
                                                s.pending_add = Some((tid, at));
                                            }));
                                        }
                                    }
                                });
                            }
                            if menu::item(ui, "Search for a node…").clicked() {
                                let (at, scene) = (m.at, m.scene);
                                act = Some(Box::new(move |s| s.picker = Some(Picker::new(at, scene, None))));
                            }
                            menu::separator(ui);
                            let can_paste = self.clipboard.is_some();
                            if ui
                                .add_enabled_ui(can_paste, |ui| menu::item(ui, "Paste   (Ctrl+V)"))
                                .inner
                                .clicked()
                            {
                                let at = m.scene;
                                act = Some(Box::new(move |s| s.paste(Some(at), now)));
                            }
                            if menu::item(ui, "Select all   (Ctrl+A)").clicked() {
                                act = Some(Box::new(AutomationUi::select_all));
                            }
                            if menu::item(ui, "Fit everything in view   (F)").clicked() {
                                let fitted = canvas::fit(rect, layouts);
                                act = Some(Box::new(move |s| {
                                    s.view = fitted;
                                    s.save_view();
                                }));
                            }
                            menu::separator(ui);
                            let (can_undo, can_redo) = (self.can_undo(false), self.can_undo(true));
                            if ui
                                .add_enabled_ui(can_undo, |ui| menu::item(ui, "Undo   (Ctrl+Z)"))
                                .inner
                                .clicked()
                            {
                                act = Some(Box::new(|s| s.undo(false)));
                            }
                            if ui
                                .add_enabled_ui(can_redo, |ui| menu::item(ui, "Redo   (Ctrl+Y)"))
                                .inner
                                .clicked()
                            {
                                act = Some(Box::new(|s| s.undo(true)));
                            }
                            if menu::item(ui, "Variables…").clicked() {
                                act = Some(Box::new(AutomationUi::open_vars));
                            }
                        }
                    }
                });
            });
        let clicked_outside = ui.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p))
        });
        let escape = ui.ctx().input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if act.is_some() || clicked_outside || escape {
            keep = false;
        }
        if keep {
            self.menu = Some(m);
        }
        if let Some(act) = act {
            act(self);
        }
    }

    fn show_picker(&mut self, ui: &Ui, rect: Rect, env: &Env<'_>) {
        if let Some((tid, at)) = self.pending_add.take() {
            self.add_node(ui, &tid, at, None, env.snap);
        }
        let Some(picker) = &mut self.picker else { return };
        match picker.show(ui.ctx(), rect) {
            Picked::Open => {}
            Picked::Closed => self.picker = None,
            Picked::Type(tid) => {
                let (at, link) = (picker.scene, picker.link.clone());
                self.picker = None;
                self.add_node(ui, &tid, at, link, env.snap);
            }
        }
    }

    fn show_ask(&mut self, ui: &Ui) {
        let Some(ask) = &mut self.ask else { return };
        match ask.dialog.show(ui.ctx()) {
            Outcome::Open => {}
            Outcome::Cancelled => self.ask = None,
            Outcome::Accepted(_) => {
                let text = ask.dialog.text().to_owned();
                let Some(ask) = self.ask.take() else { return };
                let Some(handle) = self.handle.clone() else { return };
                match ask.what {
                    Asking::Rename(nid) => self.rename(&nid, &text),
                    Asking::NewWorkspace(copy) => {
                        let name = text.trim().to_owned();
                        if name.is_empty() {
                            return;
                        }
                        self.save_view();
                        let wid = handle.with(|e| {
                            let from = copy.and_then(|c| e.workspace(&c).cloned());
                            e.add_workspace(&name, from.as_ref())
                        });
                        self.show_workspace(&wid);
                    }
                    Asking::RenameWorkspace(wid) => {
                        handle.with(|e| e.rename_workspace(&wid, &text));
                        self.stale = true;
                    }
                    Asking::DeleteWorkspace(wid) => {
                        handle.with(|e| e.delete_workspace(&wid));
                        self.undo.remove(&wid);
                        self.redo.remove(&wid);
                        self.stale = true;
                    }
                }
            }
        }
    }

    /// The bar at the bottom: workspace tabs and their menu, the switch, Add node, Fit, the zoom and Variables.
    fn dock(&mut self, ui: &mut Ui, rect: Rect, tokens: &Tokens, layouts: &canvas::Layouts) {
        let current = self.tabs.iter().find(|(id, _, _)| *id == self.wid).cloned();
        let (name, on) = current.map(|(_, n, e)| (n, e)).unwrap_or_default();
        let several = self.tabs.len() > 1;
        let tabs_width: f32 = self.tabs.iter().map(|(_, n, _)| n.chars().count() as f32 * 8.0 + 30.0).sum();
        let compact = several && tabs_width + 900.0 > rect.width() - 80.0;
        let mut switch_to = None;
        let mut menu_at = None;
        let area = egui::Area::new(egui::Id::unique(("automation-dock", ui.scope_id())))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(rect.center().x, rect.max.y - 28.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .constrain_to(rect)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(tokens.p.bg_card.gamma_multiply(0.96))
                    .stroke(egui::Stroke::new(1.0, tokens.p.border))
                    .corner_radius(14.0)
                    .inner_margin(5.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if several && !compact {
                                for (id, tab, enabled) in &self.tabs {
                                    let dot = if *enabled && !self.paused { "● " } else { "" };
                                    let mut button = crate::widgets::Button::ghost(format!("{dot}{tab}"));
                                    if *id == self.wid {
                                        button = crate::widgets::Button::new(format!("{dot}{tab}"));
                                    }
                                    let r = ui.add(button);
                                    if r.clicked() {
                                        switch_to = Some(id.clone());
                                    }
                                    if r.secondary_clicked() {
                                        switch_to = Some(id.clone());
                                        menu_at = Some(r.rect.left_top());
                                    }
                                }
                            }
                            let more = if several && !compact { "▾".to_owned() } else { format!("{name}  ▾") };
                            let more = ui.add(crate::widgets::Button::ghost(more)).on_hover_text("Workspaces: switch, new, rename, export…");
                            if more.clicked() {
                                menu_at = Some(more.rect.left_top());
                            }
                            let (text, tip) = if self.paused {
                                ("⏸ All paused", "Every automation is paused (Settings → Automation, the tray, or its hotkey). Click to resume.")
                            } else if on {
                                ("● Running", "This workspace runs in the background. Click to switch it off.")
                            } else {
                                ("○ Off", "Switched off: nothing in this workspace runs. Click to switch it on.")
                            };
                            if ui.add(crate::widgets::Button::new(text)).on_hover_text(tip).clicked() {
                                if self.paused {
                                    self.out.resume = true;
                                } else {
                                    self.set_workspace_on(!on);
                                }
                            }
                            ui.separator();
                            if ui.add(crate::widgets::Button::new("+ Add node")).clicked() {
                                let at = pos2(rect.center().x - 220.0, rect.max.y - 640.0);
                                self.picker = Some(Picker::new(at, self.view.center, None));
                            }
                            if ui.add(crate::widgets::Button::ghost("Fit")).on_hover_text("Show every node (F)").clicked() {
                                self.fit(rect, layouts);
                            }
                            let zoom = format!("{:.0} %", self.view.zoom * 100.0);
                            if ui.add(crate::widgets::Button::ghost(zoom)).on_hover_text("Back to 100 %").clicked() {
                                self.view.set_zoom(1.0);
                                self.save_view();
                            }
                            if ui.add(crate::widgets::Button::ghost("Variables")).clicked() {
                                self.open_vars();
                            }
                            if rect.width() > 1100.0 {
                                ui.separator();
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            "Right-click or double-click to add · drag from a dot to wire · right-drag to move around · wheel zooms",
                                        )
                                        .size(tokens.fs(12.0))
                                        .color(tokens.p.text_muted),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            }
                        });
                    });
            });
        let _ = area;
        if let Some(wid) = switch_to
            && wid != self.wid
        {
            self.show_workspace(&wid);
        }
        if let Some(at) = menu_at {
            // the menu opens above the dock
            let scene = self.view.center;
            self.menu = Some(Menu { at: at - vec2(0.0, 300.0), scene, on: MenuOn::Workspaces });
        }
    }

    fn open_vars(&mut self) {
        if let Some(h) = &self.handle {
            let vars = h.read(|e| e.vars().clone());
            self.vars = Some(vars::VarsPanel::new(&vars));
        }
    }

    fn show_vars(&mut self, ui: &Ui) {
        let Some(handle) = self.handle.clone() else { return };
        let Some(panel) = &mut self.vars else { return };
        if panel.needs_reload() {
            let vars = handle.read(|e| e.vars().clone());
            panel.reload(&vars);
        }
        match panel.show(ui.ctx()) {
            vars::VarsDone::Open => {}
            vars::VarsDone::Cancelled => self.vars = None,
            vars::VarsDone::Saved(map) => {
                handle.with(|e| e.replace_vars(map));
                self.vars = None;
            }
        }
    }

    fn workspace_menu(&mut self, ui: &mut Ui, act: &mut Option<MenuAct>) {
        let current = self.tabs.iter().find(|(id, _, _)| *id == self.wid).cloned().unwrap_or_default();
        let (wid, name) = (current.0.clone(), current.1.clone());
        if self.tabs.len() > 1 {
            for (id, n, enabled) in &self.tabs {
                let mark = if *id == self.wid { "●  " } else { "     " };
                let off = if *enabled { "" } else { "   (off)" };
                if menu::item(ui, &format!("{mark}{n}{off}")).clicked() {
                    let id = id.clone();
                    *act = Some(Box::new(move |s| s.show_workspace(&id)));
                }
            }
            menu::separator(ui);
        }
        let ask = |what: Asking, dialog: AskDialog| -> MenuAct {
            Box::new(move |s| s.ask = Some(Ask { what, dialog }))
        };
        if menu::item(ui, "New workspace…").clicked() {
            let d = AskDialog::ask_text(
                egui::Id::unique("automation-ws-new"),
                "Workspace",
                "Name (e.g. Morning, Backups):",
                "",
            );
            *act = Some(ask(Asking::NewWorkspace(None), d));
        }
        if menu::item(ui, &format!("Duplicate “{name}”…")).clicked() {
            let d = AskDialog::ask_text(
                egui::Id::unique("automation-ws-new"),
                "Workspace",
                "Name (e.g. Morning, Backups):",
                &format!("{name} copy"),
            );
            *act = Some(ask(Asking::NewWorkspace(Some(wid.clone())), d));
        }
        if menu::item(ui, &format!("Rename “{name}”…")).clicked() {
            let d = AskDialog::ask_text(
                egui::Id::unique("automation-ws-rename"),
                "Rename workspace",
                "New name:",
                &name,
            );
            *act = Some(ask(Asking::RenameWorkspace(wid.clone()), d));
        }
        menu::separator(ui);
        if menu::item(ui, "Export to a file…").clicked() {
            let file = json!({"name": name, "nodes": self.ws["nodes"], "wires": self.ws["wires"]});
            let name = name.clone();
            *act = Some(Box::new(move |s| s.out.export = Some((name, file))));
        }
        if menu::item(ui, "Import from a file…").clicked() {
            *act = Some(Box::new(|s| s.out.import = true));
        }
        menu::separator(ui);
        let count = self.nodes().len();
        let enabled = self.tabs.len() > 1;
        if ui.add_enabled_ui(enabled, |ui| menu::item(ui, &format!("Delete “{name}”…"))).inner.clicked()
        {
            let text = format!("Delete “{name}” with its {count} node(s)?");
            let d = AskDialog::confirm(
                egui::Id::unique("automation-ws-delete"),
                "Delete workspace",
                &text,
                "Delete",
                "Cancel",
                true,
            );
            *act = Some(ask(Asking::DeleteWorkspace(wid.clone()), d));
        }
    }
}

/// A point on the screen as the canvas sees it, for tests that click around.
pub fn scene_to_screen(view: &View, rect: Rect, p: Pos2) -> Pos2 {
    view.to_screen(rect, p)
}
