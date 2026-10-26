"""
Notes: a floating always-on-top panel holding plain notes and to-do lists.

Hotkeys: show/hide, edit mode.  Drag the header to move the panel any time;
edit mode = resize/anchor the panel and add/rename/delete notes.  Right-click a note on the home list for the same
actions without entering edit mode, and to pin it: pinned notes sort first and
(setting) showing the panel opens the first pinned note.  The search box filters
by name, content and tags as you type (#work shows the notes tagged work).

Notes can have a colour and tags, be dragged into a new order, be exported to a .md / .txt file and be
stuck on the screen as their own little sticky panel (several at once).  Text notes are plain text with
Markdown formatting that the 👁 button shows rendered.  To-do items can be dragged by their handle and get
a due date (right-click), which adds a Luminescent reminder.
"""
import os
import uuid
from datetime import datetime

from PyQt6.QtWidgets import (
    QWidget, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QFrame, QTextEdit, QTextBrowser,
    QLineEdit, QScrollArea, QSizePolicy, QFileDialog
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QPixmap, QIcon, QColor

from core import DATA_DIR, theme, widgets, save_json, load_json
from core.module import Module

SAVE_PATH = os.path.join(DATA_DIR, "notes.json")
NOTE_COLOURS = [("Red", "#E5484D"), ("Orange", "#F76B15"), ("Yellow", "#FFC53D"), ("Green", "#30A46C"),
                ("Teal", "#12A594"), ("Blue", "#3E63DD"), ("Purple", "#8E4EC6"), ("Pink", "#D6409F")]


def _new_id() -> str:
    return uuid.uuid4().hex[:10]


def _default_data(w, h):
    return {"notes": [{"id": _new_id(), "name": "My First Note", "type": "note", "content": "", "items": []}],
            "x": 100, "y": 100, "w": w, "h": h, "open": None, "stickies": {}, "preview": False}


def _swatch(colour: str, size: int = 12) -> QIcon:
    px = QPixmap(size, size)
    px.fill(QColor(colour))
    return QIcon(px)


def _due_text(due: str) -> str:
    try:
        d = datetime.fromisoformat(due)
    except (TypeError, ValueError):
        return ""
    today = datetime.now().date()
    if d.date() == today:
        day = "today"
    elif (d.date() - today).days == 1:
        day = "tomorrow"
    elif 0 < (d.date() - today).days < 7:
        day = d.strftime("%a")
    else:
        day = d.strftime("%d %b")
    return f"{day} {d:%H:%M}"


def _overdue(due: str) -> bool:
    try:
        return datetime.fromisoformat(due) < datetime.now()
    except (TypeError, ValueError):
        return False


def note_markdown(note: dict) -> str:
    """The note as Markdown text (what Export writes)."""
    if note.get("type") == "todo":
        lines = [f"# {note.get('name', 'Note')}", ""]
        for it in note.get("items", []):
            due = f"  (due {it['due'].replace('T', ' ')[:16]})" if it.get("due") else ""
            lines.append(f"- [{'x' if it.get('checked') else ' '}] {it.get('text', '')}{due}")
        return "\n".join(lines) + "\n"
    return note.get("content", "")


def markdown_view(font_px: int) -> QTextBrowser:
    view = QTextBrowser()
    view.setOpenExternalLinks(True)
    p = theme.P
    view.setStyleSheet(f"""
        QTextBrowser {{ background: transparent; border: none; padding: 12px 14px;
                        font-size: {font_px}px; color: {p.text_primary}; }}
    """)
    view.document().setDefaultStyleSheet(f"a {{ color: {p.primary_text}; }} code {{ background: {p.bg_card}; }}")
    return view


class TodoItem(QFrame):
    deleted = pyqtSignal(object)
    changed = pyqtSignal()
    menu_requested = pyqtSignal(object, object)

    def __init__(self, data: dict, font_size: int):
        super().__init__()
        self.setObjectName("todoRow")
        p = theme.P
        self.setStyleSheet(f"#todoRow {{ background: transparent; border: none; border-radius: 6px; }}"
                           f"#todoRow:hover {{ background: {p.bg_card_hover}; }}")
        self._font_size = font_size
        self.item_id = data.get("id") or _new_id()
        self.due = data.get("due") or ""
        self.setMinimumHeight(34)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(2, 4, 4, 4)
        lay.setSpacing(8)

        self.handle = widgets.label("⋮⋮", 11, tone="muted")
        self.handle.setFixedWidth(12)
        self.handle.setToolTip("Drag to reorder")

        self.check = QPushButton()
        self.check.setFixedSize(16, 16)
        self.check.setCheckable(True)
        self.check.setChecked(bool(data.get("checked")))
        self.check.setCursor(Qt.CursorShape.PointingHandCursor)
        self.check.clicked.connect(self._toggle)

        self.label = QLabel(data.get("text", ""))
        self.label.setWordWrap(True)
        self.label.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        self.label.setContextMenuPolicy(Qt.ContextMenuPolicy.NoContextMenu)   # right-click = this row's menu
        self.label.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)

        self.edit = QLineEdit(data.get("text", ""))
        self.edit.hide()
        self.edit.returnPressed.connect(self._finish_edit)
        self.edit.editingFinished.connect(self._finish_edit)

        self.due_label = widgets.label("", 10, mono=True, tone="")
        self.due_label.setToolTip("Due date (right-click to change)")

        delete = widgets.icon_button("✕", "Remove", 22, 10, ghost=True)
        delete.clicked.connect(lambda: self.deleted.emit(self))

        lay.addWidget(self.handle)
        lay.addWidget(self.check)
        lay.addWidget(self.label, 1)
        lay.addWidget(self.edit, 1)
        lay.addWidget(self.due_label)
        lay.addWidget(delete)
        self._style()

    def _style(self):
        on = self.check.isChecked()
        p = theme.P
        self.check.setText("✓" if on else "")
        self.check.setStyleSheet(
            f"QPushButton {{ background: {p.primary if on else 'transparent'}; color: {p.on_primary};"
            f" border: 2px solid {p.primary if on else p.text_muted}; border-radius: 4px; padding: 0;"
            f" font-size: 10px; font-weight: 600; }}"
            f"QPushButton:hover {{ border-color: {p.primary}; }}"
        )
        f = theme.qfont(self._font_size + 1)
        f.setStrikeOut(on)
        self.label.setFont(f)
        self.label.setStyleSheet(f"color: {p.text_muted if on else p.text_primary}; border: none;")
        self.due_label.setText(f"⏰ {_due_text(self.due)}" if self.due else "")
        self.due_label.setVisible(bool(self.due))
        late = bool(self.due) and not on and _overdue(self.due)
        self.due_label.setStyleSheet(f"color: {'#E5484D' if late else p.text_muted}; border: none;")

    def set_due(self, due: str):
        self.due = due or ""
        self._style()
        self.changed.emit()

    def _toggle(self):
        self._style()
        self.changed.emit()

    def contextMenuEvent(self, e):
        self.menu_requested.emit(self, e.globalPos())

    def mouseDoubleClickEvent(self, e):
        self.start_edit()

    def start_edit(self):
        self.label.hide()
        self.edit.setText(self.label.text())
        self.edit.show()
        self.edit.setFocus()
        self.edit.selectAll()

    def _finish_edit(self):
        if not self.edit.isVisible():
            return
        text = self.edit.text().strip()
        self.edit.hide()
        self.label.show()
        if text and text != self.label.text():
            self.label.setText(text)
            self.changed.emit()

    def data(self) -> dict:
        out = {"id": self.item_id, "text": self.label.text(), "checked": self.check.isChecked()}
        if self.due:
            out["due"] = self.due
        return out


class TodoList(QWidget):
    """The rows of a to-do note, the "Add a task" box and "Clear done". Used by the panel and by stickies.

    Every change is written straight into `note["items"]`, and to-do reminders follow the due dates.
    """
    changed = pyqtSignal()

    def __init__(self, module: "NotesModule", note: dict, compact: bool = False):
        super().__init__()
        self.m = module
        self.note = note
        self.items: list[TodoItem] = []
        self._known = {}
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        self._layout = QVBoxLayout(holder)
        self._layout.setContentsMargins(6, 0, 6, 4)
        self._layout.setSpacing(0)
        self._reorder = widgets.DragReorder(self._layout, self)
        self._reorder.moved.connect(lambda *_: self._changed())
        self._layout.addStretch()
        for it in note.get("items", []):
            self._add_widget(it)
        self._snapshot()
        scroll.setWidget(holder)
        outer.addWidget(scroll, 1)

        row = QFrame()
        widgets.style(row, "background: transparent; border: none;")
        rl = QHBoxLayout(row)
        rl.setContentsMargins(10 if compact else 14, 8, 10 if compact else 14, 10 if compact else 14)
        rl.setSpacing(8)
        self.input = QLineEdit()
        self.input.setFixedHeight(30 if compact else 32)
        self.input.setPlaceholderText("Add a task…")
        self.input.returnPressed.connect(self._add_from_input)
        rl.addWidget(self.input, 1)
        if not compact:
            clear_done = widgets.button("Clear done", "ghost", "Remove completed items")
            clear_done.setStyleSheet("QPushButton { font-size: 12px; padding: 4px 6px; }")
            clear_done.clicked.connect(self.clear_done)
            rl.addWidget(clear_done)
        outer.addWidget(row)

    def _snapshot(self):
        self._known = {i.item_id: i.data() for i in self.items}

    def _add_widget(self, data: dict) -> TodoItem:
        item = TodoItem(data, int(self.m.s("todo_font_size", 12)))
        item.deleted.connect(self._remove)
        item.changed.connect(self._changed)
        item.menu_requested.connect(self._item_menu)
        self._layout.insertWidget(max(0, self._layout.count() - 1), item)
        self._reorder.add(item, item.handle)
        self.items.append(item)
        return item

    def _ordered(self) -> list[TodoItem]:
        return sorted(self.items, key=self._layout.indexOf)

    def values(self) -> list[dict]:
        return [i.data() for i in self._ordered()]

    def _add_from_input(self):
        text = self.input.text().strip()
        if text:
            self._add_widget({"id": _new_id(), "text": text, "checked": False})
            self.input.clear()
            self._changed()

    def _remove(self, item: TodoItem):
        if item in self.items:
            self.items.remove(item)
        item.setParent(None)
        item.deleteLater()
        self._changed()

    def clear_done(self):
        for it in [i for i in self.items if i.check.isChecked()]:
            self.items.remove(it)
            it.setParent(None)
            it.deleteLater()
        self._changed()

    def _item_menu(self, item: TodoItem, pos):
        m = widgets.menu(self)
        m.addAction("Edit", item.start_edit)
        m.addAction("⏰  Set due date…" if not item.due else "⏰  Change due date…", lambda: self._ask_due(item))
        if item.due:
            m.addAction("Clear due date", lambda: item.set_due(""))
        m.addSeparator()
        m.addAction("Remove", lambda: self._remove(item))
        m.exec(pos)

    def _ask_due(self, item: TodoItem):
        cur = None
        try:
            cur = datetime.fromisoformat(item.due) if item.due else None
        except ValueError:
            pass
        when = widgets.ask_datetime(self, "Due date", f"When is “{item.label.text()}” due?\n"
                                                      "Luminescent reminds you at that time.", cur)
        if when is not None:
            item.set_due(when.isoformat(timespec="minutes"))

    def _changed(self):
        values = self.values()
        self.note["items"] = values
        now = {v["id"]: v for v in values}
        for iid, v in now.items():
            old = self._known.get(iid)
            if old is None or (old.get("due"), old.get("checked"), old.get("text")) != \
                    (v.get("due"), v.get("checked"), v.get("text")):
                if v.get("due") or (old or {}).get("due"):
                    self.m.sync_todo(self.note, v)
        for iid in set(self._known) - set(now):
            if self._known[iid].get("due"):
                self.m.drop_todo(iid)
        self._known = now
        self.changed.emit()
        self.m.note_edited(self.note, self.window())

    def done_count(self) -> tuple[int, int]:
        vals = [i.check.isChecked() for i in self.items]
        return sum(vals), len(vals)


class StickyNote(widgets.FloatingPanel):
    """One note stuck on the screen as its own small panel (Notes → right-click → Stick on screen)."""

    def __init__(self, module: "NotesModule", note: dict, geo=None):
        self.m = module
        self.note = note
        super().__init__("Sticky", min_size=(180, 120), opacity=float(module.s("bg_opacity", 0.85)),
                         drag_needs_edit_mode=False, close_hides=False, header_height=32)
        self.set_title(note.get("name", "Note"))
        if note.get("colour"):
            self._tick.setStyleSheet(f"background: {note['colour']}; border: none;")
            self._tick.setFixedWidth(64)
        self._view = None
        self._preview = False
        if note.get("type") != "todo" and module.s("markdown", True):
            self._eye = widgets.icon_button("👁", "Show formatted / edit", 24, 11, ghost=True)
            self._eye.setCheckable(True)
            self._eye.toggled.connect(self._set_preview)
            self.header_right.addWidget(self._eye)
        more = widgets.icon_button("⋯", "Sticky options", 24, 12, ghost=True)
        more.clicked.connect(lambda: self._menu(more.mapToGlobal(more.rect().bottomLeft())))
        self.header_right.addWidget(more)
        if geo and len(geo) == 4:
            self.setGeometry(*[int(v) for v in geo])
            self.clamp_to_screens()
        else:
            self.resize(260, 240)
            self.center_on()
        self.geometry_changed.connect(lambda: module.sticky_moved(self))
        self.refresh()

    def note_id(self) -> str:
        return self.note.get("id", "")

    def refresh(self):
        widgets.clear_layout(self.body)
        font = int(self.m.s("font_size", 16))
        if self.note.get("type") == "todo":
            self._view = TodoList(self.m, self.note, compact=True)
        elif self._preview:
            self._view = markdown_view(font)
            self._view.setMarkdown(self.note.get("content", ""))
        else:
            ed = QTextEdit()
            ed.setPlaceholderText("Start typing…")
            ed.setStyleSheet(f"QTextEdit {{ background: transparent; border: none; padding: 10px 12px;"
                             f" font-size: {font}px; color: {theme.P.text_primary}; }}")
            ed.setPlainText(self.note.get("content", ""))
            ed.textChanged.connect(lambda: self._typed(ed))
            self._view = ed
        self.body.addWidget(self._view)

    def _typed(self, ed: QTextEdit):
        self.note["content"] = ed.toPlainText()
        self.m.note_edited(self.note, self)

    def _set_preview(self, on: bool):
        self._preview = on
        self.refresh()

    def _menu(self, pos):
        m = widgets.menu(self)
        m.addAction("Open in the Notes panel", lambda: self.m.show_note_id(self.note_id()))
        m.addAction("Unstick (close)", lambda: self.m.unstick(self.note_id()))
        m.exec(pos)

    def close_requested(self):
        self.m.unstick(self.note_id())


class NotePanel(widgets.FloatingPanel):
    def __init__(self, module: "NotesModule"):
        self.m = module
        super().__init__("Notes",
                         min_size=(int(module.s("min_width", 250)), int(module.s("min_height", 180))),
                         opacity=float(module.s("bg_opacity", 0.85)),
                         drag_needs_edit_mode=False, close_hides=True)
        self.data = self._load()
        self.current: int | None = None
        self._todo: TodoList | None = None
        self._dim: widgets.DimOverlay | None = None
        self._anchor_bar: widgets.AnchorBar | None = None
        self._editor: QTextEdit | None = None
        self._query = ""
        self._search: QLineEdit | None = None
        self._list_layout: QVBoxLayout | None = None
        self._reorder: widgets.DragReorder | None = None
        self._order: list[int] = []
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.setInterval(400)
        self._save_timer.timeout.connect(self._save_now)

        self.setGeometry(self.data["x"], self.data["y"], self.data["w"], self.data["h"])
        self.clamp_to_screens()

        # header: back button, preview toggle, options
        self.back_btn = widgets.icon_button("←", "Back to all notes", 24, 13, ghost=True)
        self.back_btn.clicked.connect(self.show_home)
        self.back_btn.hide()
        self.header_left.addWidget(self.back_btn)
        self.eye_btn = widgets.icon_button("👁", "Show formatted (Markdown) / edit", 26, 12, ghost=True)
        self.eye_btn.setCheckable(True)
        self.eye_btn.setChecked(bool(self.data.get("preview")))
        self.eye_btn.toggled.connect(self._set_preview)
        self.eye_btn.hide()
        self.header_right.addWidget(self.eye_btn)
        more = widgets.icon_button("⋯", "Note options", 26, 12, ghost=True)
        more.clicked.connect(lambda: self._header_menu(more.mapToGlobal(more.rect().bottomLeft())))
        self.header_right.addWidget(more)

        body = QHBoxLayout()
        body.setContentsMargins(0, 0, 0, 0)
        body.setSpacing(0)
        self.content = QVBoxLayout()
        self.content.setContentsMargins(0, 0, 0, 0)
        self.content.setSpacing(0)
        body.addLayout(self.content, 1)

        self.side = QFrame()
        self.side.setFixedWidth(36)
        widgets.style(self.side, "background: transparent; border: none;")
        self.side_layout = QVBoxLayout(self.side)
        self.side_layout.setContentsMargins(4, 8, 4, 8)
        self.side_layout.setSpacing(6)
        self.side_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.side.hide()
        body.addWidget(self.side)
        self.body.addLayout(body)

        self.geometry_changed.connect(self.schedule_save)
        self.escape_pressed.connect(lambda: self.set_edit(False) if self.edit_mode else None)
        # hiding the panel (hotkey or ✕) while editing used to leave the screen dimmed and the anchor bar up
        self.hidden.connect(lambda: self.set_edit(False) if self.edit_mode else self.flush())
        module.note_changed.connect(self._note_changed_elsewhere)

        opened = self.data.get("open")
        if isinstance(opened, int) and 0 <= opened < len(self.data["notes"]):
            self.open_note(opened)
        else:
            self.show_home()

    # ── persistence ────────────────────────────────────────────────────────
    def _load(self) -> dict:
        d = _default_data(int(self.m.s("default_width", 360)), int(self.m.s("default_height", 460)))
        # an unreadable file is kept as notes.json.bad by load_json (it used to be overwritten
        # by the default "My First Note" on the next save, losing every note)
        saved = load_json(SAVE_PATH, {})
        if isinstance(saved, dict):
            d.update({k: v for k, v in saved.items() if k in d})
        if not isinstance(d["notes"], list):
            d["notes"] = []
        if not isinstance(d.get("stickies"), dict):
            d["stickies"] = {}
        d["notes"] = [n for n in d["notes"] if isinstance(n, dict)]
        for n in d["notes"]:
            n.setdefault("id", _new_id())
            n.setdefault("name", "Note")
            n.setdefault("content", "")
            n.setdefault("items", [])
            n.setdefault("type", "note")
            n.setdefault("pinned", False)
            n.setdefault("colour", "")
            n.setdefault("tags", [])
            n["items"] = [it for it in n["items"] if isinstance(it, dict)]
            for it in n["items"]:
                it.setdefault("id", _new_id())
        return d

    def schedule_save(self):
        self._save_timer.start()

    def _save_now(self):
        g = self.geometry()
        self.data.update({"x": g.x(), "y": g.y(), "w": g.width(), "h": g.height(), "open": self.current})
        try:
            save_json(SAVE_PATH, self.data, indent=2)
        except Exception:
            self.m.log.exception("could not save notes.json")

    def flush(self):
        self._capture_current()
        self._save_timer.stop()
        self._save_now()

    def index_of(self, nid: str) -> int | None:
        return next((i for i, n in enumerate(self.data["notes"]) if n.get("id") == nid), None)

    # ── views ──────────────────────────────────────────────────────────────
    def _clear_views(self):
        self._search = self._list_layout = None
        self._todo_badge = None
        self._todo = None
        self._reorder = None
        widgets.clear_layout(self.content)
        widgets.clear_layout(self.side_layout)
        self._editor = None

    def show_home(self):
        self._capture_current()
        self.current = None
        self.back_btn.hide()
        self.eye_btn.hide()
        self.set_title("Notes")
        self._clear_views()

        top = QFrame()
        widgets.style(top, "background: transparent; border: none;")
        tl = QHBoxLayout(top)
        tl.setContentsMargins(14, 12, 14, 0)
        self._search = QLineEdit(self._query)
        self._search.setFixedHeight(32)
        self._search.setPlaceholderText("Search notes (#tag for a tag)")
        self._search.setClearButtonEnabled(True)
        self._search.textChanged.connect(self._search_changed)
        tl.addWidget(self._search)
        self.content.addWidget(top)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        self._list_layout = QVBoxLayout(holder)
        self._fill_list()
        scroll.setWidget(holder)
        self.content.addWidget(scroll)
        self._rebuild_side()
        self.schedule_save()

    def _search_changed(self, text: str):
        self._query = text
        if self._list_layout is not None:
            widgets.clear_layout(self._list_layout)
            self._fill_list()

    def _matches(self, note: dict) -> bool:
        q = self._query.strip().lower()
        if not q:
            return True
        tags = [t.lower() for t in note.get("tags", [])]
        if q.startswith("#"):
            return any(t.startswith(q[1:]) for t in tags) if q[1:] else bool(tags)
        hay = [note.get("name", ""), note.get("content", "")] + tags + \
              [it.get("text", "") for it in note.get("items", [])]
        return any(q in h.lower() for h in hay)

    def _fill_list(self):
        v = self._list_layout
        v.setContentsMargins(14, 10, 14, 14)
        v.setSpacing(6)
        p = theme.P
        self._reorder = widgets.DragReorder(v, self)
        self._reorder.enabled = not self._query.strip()        # reordering a filtered list would be confusing
        self._reorder.moved.connect(self._dropped)
        self._order = sorted(range(len(self.data["notes"])), key=lambda i: (not self.data["notes"][i].get("pinned"), i))
        shown = 0
        for i in self._order:
            note = self.data["notes"][i]
            if not self._matches(note):
                continue
            shown += 1
            card = QFrame()
            card.setObjectName("noteCard")
            card.setCursor(Qt.CursorShape.PointingHandCursor)
            card.setFixedHeight(48)
            colour = note.get("colour") or ""
            card.setStyleSheet(f"""
                #noteCard {{ background: {theme.mix(p.bg_card, colour, 0.12) if colour else p.bg_card};
                             border: 1px solid {p.border}; border-radius: {p.radius}px; }}
                #noteCard:hover {{ background: {p.bg_card_hover}; }}
            """)
            cl = QHBoxLayout(card)
            cl.setContentsMargins(14, 0, 12, 0)
            cl.setSpacing(10)
            if note.get("pinned") or colour:
                tick = QFrame()
                tick.setFixedSize(3, 18 if colour else 12)
                tick.setStyleSheet(f"background: {colour or p.primary}; border-radius: 1px;")
                cl.addWidget(tick)
            name = widgets.label(note["name"], 13, 500)
            name.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
            cl.addWidget(name, 1)
            for tag in note.get("tags", [])[:3]:
                chip = widgets.label(f"#{tag}", 9.5, 500, mono=True, tone="")
                chip.setStyleSheet(f"color: {colour or p.primary_text}; background: transparent;")
                cl.addWidget(chip)
            if note.get("pinned"):
                cl.addWidget(widgets.label("PINNED", 9, 500, mono=True, spacing=0.08, tone="accent"))
            if note["type"] == "todo":
                done = sum(1 for it in note["items"] if it.get("checked"))
                cl.addWidget(self._badge(f"{done}/{len(note['items'])}"))
            else:
                cl.addWidget(widgets.label("NOTE", 9, 500, mono=True, spacing=0.08, tone="muted"))
            card.setProperty("note_index", i)
            card.mousePressEvent = lambda e, idx=i: self._card_press(e, idx)
            card.mouseReleaseEvent = lambda e, idx=i: self._card_release(e, idx)
            self._reorder.add(card)
            v.addWidget(card)
        if not shown:
            text = ("No notes yet.\nPress the edit hotkey and click + to add one." if not self.data["notes"]
                    else f'Nothing matches "{self._query.strip()}".')
            empty = widgets.label(text, 12, tone="muted")
            empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
            v.addWidget(empty)
        v.addStretch()

    def _card_press(self, e, idx: int):
        if e.button() == Qt.MouseButton.RightButton:
            self._note_menu(idx, e.globalPosition().toPoint())

    def _card_release(self, e, idx: int):
        if e.button() == Qt.MouseButton.LeftButton:
            self.open_note(idx)

    def _dropped(self, old: int, new: int):
        """A card was dragged from position `old` to `new` on the home list."""
        order = list(self._order)
        order.insert(new, order.pop(old))
        self.data["notes"] = [self.data["notes"][i] for i in order]
        QTimer.singleShot(0, self.show_home)          # rebuild after the drag's mouse release is handled

    def _note_menu(self, idx: int, pos):
        note = self.data["notes"][idx]
        m = widgets.menu(self)
        m.addAction("Open", lambda: self.open_note(idx))
        pinned = note.get("pinned")
        m.addAction("Unpin" if pinned else "📌  Pin (shown first, opened first)", lambda: self._pin(idx, not pinned))
        self._decorate_menu(m, idx)
        m.addSeparator()
        if idx > 0:
            m.addAction("Move up", lambda: self._move_note(idx, -1))
        if idx < len(self.data["notes"]) - 1:
            m.addAction("Move down", lambda: self._move_note(idx, 1))
        m.addSeparator()
        m.addAction("Delete…", lambda: self.delete_note(idx))
        m.exec(pos)

    def _decorate_menu(self, m, idx: int):
        """Rename, colour, tags, sticky and export: shared by the list's right-click and the ⋯ menu."""
        note = self.data["notes"][idx]
        m.addAction("Rename…", lambda: self.rename_note(idx))
        colours = m.addMenu("Colour")
        for name, hexc in NOTE_COLOURS:
            a = colours.addAction(_swatch(hexc), name, lambda c=hexc: self._set_colour(idx, c))
            a.setCheckable(True)
            a.setChecked(note.get("colour") == hexc)
        colours.addSeparator()
        colours.addAction("No colour", lambda: self._set_colour(idx, ""))
        m.addAction("Tags…", lambda: self._edit_tags(idx))
        m.addSeparator()
        stuck = note.get("id") in self.data.get("stickies", {})
        m.addAction("Unstick from the screen" if stuck else "📌  Stick on the screen (own small panel)",
                    lambda: self.m.unstick(note["id"]) if stuck else self.m.stick(note["id"]))
        m.addAction("Export to a file…", lambda: self.m.export_note(note, self))

    def _set_colour(self, idx: int, colour: str):
        self.data["notes"][idx]["colour"] = colour
        self.m.note_edited(self.data["notes"][idx], self, rebuild=True)
        if self.current is None:
            self.show_home()
        self.schedule_save()

    def _edit_tags(self, idx: int):
        note = self.data["notes"][idx]
        text, ok = widgets.ask_text(self, "Tags", "Tags, separated by commas (search them with #tag):",
                                    ", ".join(note.get("tags", [])))
        if not ok:
            return
        tags = []
        for t in text.replace("#", " ").replace(",", " ").split():
            if t.lower() not in [x.lower() for x in tags]:
                tags.append(t)
        note["tags"] = tags
        if self.current is None:
            self.show_home()
        self.schedule_save()

    def _pin(self, idx: int, on: bool):
        self.data["notes"][idx]["pinned"] = on
        self.show_home()

    def first_pinned(self) -> int | None:
        return next((i for i, n in enumerate(self.data["notes"]) if n.get("pinned")), None)

    def open_note(self, idx: int):
        self._capture_current()
        self._show_note(idx)

    def _show_note(self, idx: int):
        self._search = self._list_layout = None
        self.current = idx
        note = self.data["notes"][idx]
        self.back_btn.show()
        self.eye_btn.setVisible(note["type"] != "todo" and bool(self.m.s("markdown", True)))
        self.set_title("Notes", note["name"])
        self._clear_views()
        if note["type"] == "todo":
            self._build_todo(note)
        else:
            self._build_editor(note)
        self._rebuild_side()
        self.schedule_save()

    def _note_changed_elsewhere(self, nid: str, source):
        """A sticky changed the note this panel shows: show the new text."""
        if source is self or self.current is None:
            return
        if self.data["notes"][self.current].get("id") == nid:
            self._show_note(self.current)

    def _set_preview(self, on: bool):
        self.data["preview"] = on
        if self.current is not None and self.data["notes"][self.current]["type"] != "todo":
            self.open_note(self.current)

    def _build_editor(self, note: dict):
        p = theme.P
        font = int(self.m.s("font_size", 16))
        if self.data.get("preview") and self.m.s("markdown", True):
            view = markdown_view(font)
            view.setMarkdown(note.get("content", "") or "*Empty note. Click 👁 to write.*")
            self.content.addWidget(view)
            return
        ed = QTextEdit()
        ed.setAcceptRichText(False)
        ed.setPlaceholderText("Start typing…" + ("  (**bold**, # heading, - list; 👁 shows it formatted)"
                                                 if self.m.s("markdown", True) else ""))
        ed.setStyleSheet(f"""
            QTextEdit {{ background: transparent; border: none; padding: 12px 14px;
                         font-size: {font}px; color: {p.text_primary}; }}
        """)
        ed.setPlainText(note.get("content", ""))
        ed.textChanged.connect(self._editor_changed)
        self.content.addWidget(ed)
        self._editor = ed

    def _editor_changed(self):
        if self.current is not None and self._editor is not None:
            note = self.data["notes"][self.current]
            note["content"] = self._editor.toPlainText()
            self.m.note_edited(note, self)
            self.schedule_save()

    @staticmethod
    def _badge(text: str) -> QLabel:
        b = widgets.label(text, 10, 500, mono=True, tone="")
        p = theme.P
        b.setStyleSheet(f"background: {p.primary_muted}; color: {p.primary_text}; border-radius: 4px; padding: 3px 7px;")
        return b

    def _build_todo(self, note: dict):
        p = theme.P
        head = QHBoxLayout()
        head.setContentsMargins(14, 14, 14, 6)
        head.setSpacing(10)
        title = widgets.label(note["name"], 15, 600)
        title.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        head.addWidget(title, 1)
        self._todo_badge = self._badge("")
        head.addWidget(self._todo_badge)
        self.content.addLayout(head)
        track = QFrame()
        track.setFixedHeight(2)
        track.setStyleSheet(f"background: {p.bg_frame}; border-radius: 1px;")
        self._todo_fill = QFrame(track)
        self._todo_fill.setStyleSheet(f"background: {note.get('colour') or p.primary}; border-radius: 1px;")
        self._todo_track = track
        wrap = QHBoxLayout()
        wrap.setContentsMargins(14, 0, 14, 10)
        wrap.addWidget(track)
        self.content.addLayout(wrap)
        self._todo = TodoList(self.m, note)
        self._todo.changed.connect(self._todos_changed)
        self.content.addWidget(self._todo, 1)
        self._update_progress()

    def _update_progress(self):
        badge = getattr(self, "_todo_badge", None)
        if badge is None or self.current is None or self._todo is None:
            return
        try:
            done, total = self._todo.done_count()
            badge.setText(f"{done}/{total}")
            frac = done / total if total else 0.0
            self._todo_fill.setGeometry(0, 0, int(self._todo_track.width() * frac), 2)
        except RuntimeError:
            self._todo_badge = None         # the to-do view was closed

    def resizeEvent(self, e):
        super().resizeEvent(e)
        QTimer.singleShot(0, self._update_progress)

    def _todos_changed(self):
        self._update_progress()
        self.schedule_save()

    def _capture_current(self):
        if self.current is None or self.current >= len(self.data["notes"]):
            return
        note = self.data["notes"][self.current]
        if note["type"] == "todo":
            if self._todo is not None:
                note["items"] = self._todo.values()
        elif self._editor is not None:
            note["content"] = self._editor.toPlainText()

    def _header_menu(self, pos):
        m = widgets.menu(self)
        if self.current is None:
            m.addAction("New note…", self.add_note)
        else:
            idx = self.current
            pinned = self.data["notes"][idx].get("pinned")
            m.addAction("Unpin" if pinned else "Pin (shown first, opened first)", lambda: self._pin(idx, not pinned))
            self._decorate_menu(m, idx)
            m.addSeparator()
            m.addAction("Delete…", lambda: self.delete_note(idx))
        m.addSeparator()
        m.addAction("Exit edit mode" if self.edit_mode else "Edit mode (move, resize, anchor)",
                    lambda: self.set_edit(not self.edit_mode))
        m.exec(pos)

    # ── side buttons ───────────────────────────────────────────────────────
    def _rebuild_side(self):
        widgets.clear_layout(self.side_layout)
        if self.current is None:
            add = widgets.icon_button("＋", "Add a new note", 26, 14)
            add.clicked.connect(self.add_note)
            self.side_layout.addWidget(add)
        else:
            ren = widgets.icon_button("✎", "Rename this note", 26, 13)
            ren.clicked.connect(lambda: self.rename_note(self.current))
            dele = widgets.icon_button("🗑", "Delete this note", 26, 12)
            dele.clicked.connect(lambda: self.delete_note(self.current))
            self.side_layout.addWidget(ren)
            self.side_layout.addWidget(dele)
        self.side.setVisible(self.edit_mode)

    # ── note management ────────────────────────────────────────────────────
    def add_note(self):
        name, ok = widgets.ask_text(self, "New Note", "Name:")
        if not ok or not name.strip():
            return
        kind, ok = widgets.ask_choice(self, "Type", "Choose type:", ["note", "todo"])
        if not ok:
            return
        self.data["notes"].append({"id": _new_id(), "name": name.strip(), "type": kind, "content": "", "items": [],
                                   "pinned": False, "colour": "", "tags": []})
        self.open_note(len(self.data["notes"]) - 1)

    def rename_note(self, idx: int):
        if idx is None:
            return
        cur = self.data["notes"][idx]["name"]
        name, ok = widgets.ask_text(self, "Rename", "New name:", cur)
        if ok and name.strip():
            self.data["notes"][idx]["name"] = name.strip()
            if self.current == idx:
                self.set_title("Notes", name.strip())
            else:
                self.show_home()
            self.m.note_edited(self.data["notes"][idx], self, rebuild=True)
            self.schedule_save()

    def delete_note(self, idx: int):
        if idx is None:
            return
        note = self.data["notes"][idx]
        if not widgets.confirm(self, "Delete note", f'Delete "{note["name"]}"? This cannot be undone.',
                               yes="Delete", danger=True):
            return
        self.current = None
        self.data["notes"].pop(idx)
        self.m.note_deleted(note)
        self.show_home()

    def _move_note(self, idx: int, delta: int):
        notes = self.data["notes"]
        j = idx + delta
        if 0 <= j < len(notes):
            notes[idx], notes[j] = notes[j], notes[idx]
            self.show_home()

    # ── edit mode ──────────────────────────────────────────────────────────
    def set_edit(self, on: bool):
        self.set_edit_mode(on)
        if on:
            if self._dim is None:
                self._dim = widgets.DimOverlay(float(self.m.s("overlay_dim", 0.35)), click_through=True)
            self._dim.show()
            if self._anchor_bar is None:
                self._anchor_bar = widgets.AnchorBar("Snap notes to:")
                self._anchor_bar.anchor_chosen.connect(
                    lambda k: self.snap_to(k, int(self.m.s("anchor_padding", 10))))
            self._anchor_bar.show()
            self._anchor_bar.raise_()
            self.show_and_raise()
            # the dim appears a moment later on Wayland: put the panel and the bar back on top of it
            QTimer.singleShot(200, lambda: (widgets.raise_window(self), widgets.raise_window(self._anchor_bar))
                              if self.edit_mode and self._anchor_bar is not None else None)
        else:
            if self._dim:
                self._dim.hide()
            if self._anchor_bar:
                self._anchor_bar.hide()
            self.flush()
        self.side.setVisible(on)

    def toggle_edit(self):
        self.set_edit(not self.edit_mode)

    def teardown(self):
        self.flush()
        try:
            self.m.note_changed.disconnect(self._note_changed_elsewhere)
        except TypeError:
            pass
        if self._dim:
            self._dim.close()
        if self._anchor_bar:
            self._anchor_bar.close()
        self.close()


class NotesPage(QWidget):
    """The overlay's Notes page: every note on the left, the open one on the right. The same notes as the
    panel and the stickies (all three show each other's changes)."""

    def __init__(self, m: "NotesModule"):
        super().__init__()
        self.m = m
        self.current_id = ""
        self._view = None
        self._preview = False
        root = QHBoxLayout(self)
        root.setContentsMargins(24, 12, 24, 24)
        root.setSpacing(20)

        left = QVBoxLayout()
        left.setSpacing(8)
        head = QHBoxLayout()
        head.addWidget(widgets.label("Notes", 15, 600))
        head.addStretch()
        new = widgets.button("＋ New note", "primary")
        new.setFixedHeight(30)
        new.clicked.connect(self._new)
        head.addWidget(new)
        left.addLayout(head)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Search notes (#tag for a tag)")
        self.search.setClearButtonEnabled(True)
        self.search.setFixedHeight(32)
        self.search.textChanged.connect(lambda _t: self._fill())
        left.addWidget(self.search)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        self.list = QVBoxLayout(holder)
        self.list.setContentsMargins(0, 4, 0, 4)
        self.list.setSpacing(6)
        scroll.setWidget(holder)
        left.addWidget(scroll, 1)
        lw = QWidget()
        lw.setLayout(left)
        lw.setFixedWidth(320)
        root.addWidget(lw)

        self.right = QFrame()
        self.right.setObjectName("notePageBody")
        p = theme.P
        self.right.setStyleSheet(f"#notePageBody {{ background: {p.bg_card}; border: 1px solid {p.border};"
                                 f" border-radius: {p.radius}px; }}")
        rl = QVBoxLayout(self.right)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(0)
        bar = QHBoxLayout()
        bar.setContentsMargins(18, 12, 12, 8)
        self.title = widgets.label("", 16, 600)
        bar.addWidget(self.title, 1)
        self.eye = widgets.icon_button("👁", "Show formatted (Markdown) / edit", 28, 12, ghost=True)
        self.eye.setCheckable(True)
        self.eye.toggled.connect(self._set_preview)
        bar.addWidget(self.eye)
        more = widgets.icon_button("⋯", "Note options", 28, 12, ghost=True)
        more.clicked.connect(lambda: self._menu(more.mapToGlobal(more.rect().bottomLeft())))
        bar.addWidget(more)
        rl.addLayout(bar)
        self.body = QVBoxLayout()
        rl.addLayout(self.body, 1)
        root.addWidget(self.right, 1)
        m.note_changed.connect(self._changed_elsewhere)
        self._fill()

    # ── list ───────────────────────────────────────────────────────────────
    def notes(self) -> list[dict]:
        return self.m.panel.data["notes"] if self.m.panel else []

    def _fill(self):
        widgets.clear_layout(self.list)
        panel = self.m.panel
        if panel is None:
            return
        panel._query = self.search.text()
        p = theme.P
        order = sorted(range(len(self.notes())), key=lambda i: (not self.notes()[i].get("pinned"), i))
        shown = 0
        for i in order:
            note = self.notes()[i]
            if not panel._matches(note):
                continue
            shown += 1
            card = QFrame()
            card.setObjectName("notePageCard")
            card.setCursor(Qt.CursorShape.PointingHandCursor)
            card.setFixedHeight(46)
            on = note.get("id") == self.current_id
            colour = note.get("colour") or ""
            card.setStyleSheet(f"#notePageCard {{ background: {p.primary_muted if on else p.bg_card};"
                               f" border: 1px solid {p.primary if on else p.border}; border-radius: {p.radius}px; }}"
                               f"#notePageCard:hover {{ border-color: {p.primary}; }}")
            cl = QHBoxLayout(card)
            cl.setContentsMargins(12, 0, 12, 0)
            cl.setSpacing(8)
            if colour or note.get("pinned"):
                tick = QFrame()
                tick.setFixedSize(3, 16)
                tick.setStyleSheet(f"background: {colour or p.primary}; border-radius: 1px;")
                cl.addWidget(tick)
            name = widgets.label(("📌 " if note.get("pinned") else "") + note["name"], 13, 500)
            name.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
            cl.addWidget(name, 1)
            if note["type"] == "todo":
                done = sum(1 for it in note["items"] if it.get("checked"))
                cl.addWidget(widgets.label(f"{done}/{len(note['items'])}", 10, mono=True, tone="muted"))
            for w in card.findChildren(QWidget):
                w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            card.mousePressEvent = lambda e, nid=note["id"], idx=i: self._card_click(e, nid, idx)
            self.list.addWidget(card)
        if not shown:
            self.list.addWidget(widgets.label("No notes match." if self.notes() else "No notes yet.", 12,
                                              tone="muted"))
        self.list.addStretch()

    def _card_click(self, e, nid: str, idx: int):
        if e.button() == Qt.MouseButton.RightButton:
            self.m.panel._note_menu(idx, e.globalPosition().toPoint())
            self._after_menu()
        else:
            self.open(nid)

    def _after_menu(self):
        if self.m.panel and self.m.panel.index_of(self.current_id) is None:
            self.current_id = ""
        self._fill()
        self._show()

    def _menu(self, pos):
        idx = self.m.panel.index_of(self.current_id) if self.m.panel and self.current_id else None
        if idx is None:
            return
        mm = widgets.menu(self)
        pinned = self.notes()[idx].get("pinned")
        mm.addAction("Unpin" if pinned else "📌  Pin (shown first)", lambda: self.m.panel._pin(idx, not pinned))
        self.m.panel._decorate_menu(mm, idx)
        mm.addSeparator()
        mm.addAction("Delete…", lambda: self.m.panel.delete_note(idx))
        mm.exec(pos)
        self._after_menu()

    def _new(self):
        if self.m.panel is None:
            return
        before = len(self.notes())
        self.m.panel.add_note()
        if len(self.notes()) > before:
            self.open(self.notes()[-1]["id"])

    def state(self) -> dict:
        return {"note": self.current_id}

    def restore_state(self, st: dict):
        if any(n.get("id") == st.get("note") for n in self.notes()):
            self.open(st["note"])

    # ── the open note ──────────────────────────────────────────────────────
    def open(self, nid: str):
        self.current_id = nid
        self._fill()
        self._show()

    def _note(self) -> dict | None:
        return next((n for n in self.notes() if n.get("id") == self.current_id), None)

    def _show(self):
        widgets.clear_layout(self.body)
        self._view = None
        note = self._note()
        if note is None:
            self.title.setText("")
            self.eye.hide()
            hint = widgets.label("Pick a note on the left, or make a new one.", 13, tone="muted")
            hint.setAlignment(Qt.AlignmentFlag.AlignCenter)
            self.body.addWidget(hint, 1)
            return
        self.title.setText(note["name"])
        text_note = note["type"] != "todo"
        self.eye.setVisible(text_note and bool(self.m.s("markdown", True)))
        font = int(self.m.s("font_size", 16))
        if not text_note:
            self._view = TodoList(self.m, note)
            self._view.changed.connect(self._fill)
        elif self._preview and self.m.s("markdown", True):
            self._view = markdown_view(font)
            self._view.setMarkdown(note.get("content", "") or "*Empty note. Click 👁 to write.*")
        else:
            ed = QTextEdit()
            ed.setAcceptRichText(False)
            ed.setPlaceholderText("Start typing…")
            ed.setStyleSheet(f"QTextEdit {{ background: transparent; border: none; padding: 8px 18px;"
                             f" font-size: {font}px; color: {theme.P.text_primary}; }}")
            ed.setPlainText(note.get("content", ""))
            ed.textChanged.connect(lambda: self._typed(ed))
            self._view = ed
        self.body.addWidget(self._view, 1)

    def _typed(self, ed: QTextEdit):
        note = self._note()
        if note is not None:
            note["content"] = ed.toPlainText()
            self.m.note_edited(note, self)

    def _set_preview(self, on: bool):
        self._preview = on
        self._show()

    def _changed_elsewhere(self, nid: str, source):
        if source is self or (self._view is not None and source is self._view.window() and source is not self.m.panel):
            return
        if nid == self.current_id and not (self._view is not None and self._view.hasFocus()):
            self._show()

    def on_show(self):
        if not self.current_id and self.m.panel:
            pin = self.m.panel.first_pinned()
            notes = self.notes()
            if notes:
                self.current_id = notes[pin if pin is not None else 0]["id"]
        self._fill()
        self._show()

    def teardown(self):
        try:
            self.m.note_changed.disconnect(self._changed_elsewhere)
        except TypeError:
            pass


class NotesModule(Module):
    key = "notes"
    title = "Notes"
    icon = "📝"
    note_changed = pyqtSignal(str, object)       # note id, the view that changed it (None = rebuild all)

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: NotePanel | None = None
        self.stickies: dict[str, StickyNote] = {}

    def start(self):
        with self.themed():
            self.panel = NotePanel(self)
        if self.ctx.shows_on_start("notes"):
            self.panel.show()
        self._open_stickies()
        self.hotkey("hotkey", self.toggle, "show/hide notes")
        self.hotkey("edit_hotkey", self.toggle_edit, "notes edit mode")

    def stop(self):
        self._close_stickies()
        if self.panel:
            self.panel.teardown()
            self.panel.deleteLater()
            self.panel = None

    # ── stickies ───────────────────────────────────────────────────────────
    def _open_stickies(self):
        for nid, geo in list(self.panel.data.get("stickies", {}).items()):
            idx = self.panel.index_of(nid)
            if idx is None:
                self.panel.data["stickies"].pop(nid, None)
                continue
            self._make_sticky(self.panel.data["notes"][idx], geo).show()

    def _close_stickies(self):
        for st in list(self.stickies.values()):
            st.close()
            st.deleteLater()
        self.stickies = {}

    def _make_sticky(self, note: dict, geo=None) -> StickyNote:
        with self.themed():
            st = StickyNote(self, note, geo)
        self.stickies[note["id"]] = st
        return st

    def stick(self, nid: str):
        if not self.panel:
            return
        idx = self.panel.index_of(nid)
        if idx is None:
            return
        st = self.stickies.get(nid) or self._make_sticky(self.panel.data["notes"][idx])
        self.sticky_moved(st)
        st.show_and_raise()

    def unstick(self, nid: str):
        st = self.stickies.pop(nid, None)
        if st is not None:
            st.close()
            st.deleteLater()
        if self.panel:
            self.panel.data.get("stickies", {}).pop(nid, None)
            self.panel.schedule_save()

    def sticky_moved(self, st: StickyNote):
        if self.panel:
            g = st.geometry()
            self.panel.data.setdefault("stickies", {})[st.note_id()] = [g.x(), g.y(), g.width(), g.height()]
            self.panel.schedule_save()

    # ── shared by the panel and the stickies ───────────────────────────────
    def note_edited(self, note: dict, source, rebuild: bool = False):
        """A view changed `note`: save it and show the change everywhere else."""
        if self.panel:
            self.panel.schedule_save()
        nid = note.get("id", "")
        for st in list(self.stickies.values()):
            if st.note_id() == nid and (st is not source or rebuild):
                if rebuild:
                    geo = st.geometry()
                    self.unstick(nid)
                    self._make_sticky(note, [geo.x(), geo.y(), geo.width(), geo.height()]).show()
                    self.sticky_moved(self.stickies[nid])
                elif not st.isActiveWindow():
                    st.refresh()
        if not rebuild:
            self.note_changed.emit(nid, source)

    def note_deleted(self, note: dict):
        self.unstick(note.get("id", ""))
        for it in note.get("items", []):
            if it.get("due"):
                self.drop_todo(it.get("id", ""))

    def _reminders(self):
        launcher = self.ctx.launcher
        m = launcher.modules.get("reminders") if launcher is not None else None
        return m if m is not None and m.running else None

    def sync_todo(self, note: dict, item: dict):
        """Keep the reminder of a to-do item with a due date in step with the item."""
        rem = self._reminders()
        rid = f"todo-{item.get('id')}"
        if rem is None:
            if item.get("due") and not item.get("checked"):
                self.ctx.notify(self.title, "Reminders is turned off, so this due date will not remind you.")
            return
        when = None
        try:
            when = datetime.fromisoformat(item["due"]) if item.get("due") else None
        except ValueError:
            pass
        if when is not None and not item.get("checked") and when > datetime.now():
            rem.add_quick(f"To-do: {item.get('text', '')}", when, rid=rid,
                          message=f"From the note “{note.get('name', '')}”")
        else:
            rem.remove(rid)

    def drop_todo(self, item_id: str):
        rem = self._reminders()
        if rem is not None and item_id:
            rem.remove(f"todo-{item_id}")

    def export_note(self, note: dict, parent=None):
        safe = "".join(c for c in note.get("name", "note") if c not in '\\/:*?"<>|').strip() or "note"
        start = os.path.join(os.path.expanduser("~/Documents"), safe + ".md")
        path, flt = QFileDialog.getSaveFileName(parent, "Export note", start,
                                                "Markdown (*.md);;Text (*.txt);;All files (*)")
        if not path:
            return
        if not os.path.splitext(path)[1]:
            path += ".txt" if "txt" in flt else ".md"
        try:
            with open(path, "w", encoding="utf-8") as f:
                f.write(note_markdown(note))
        except OSError as e:
            widgets.error(parent, "Export note", f"Could not write the file:\n{e}")
            return
        self.ctx.notify(self.title, f"Exported to {path}")

    # ── panel ──────────────────────────────────────────────────────────────
    def toggle(self):
        if not self.panel:
            return
        if not self.panel.isVisible() and self.s("open_pinned", True):
            pin = self.panel.first_pinned()
            if pin is not None and self.panel.current != pin:
                self.panel.open_note(pin)
        self.panel.toggle()

    def show_note(self, idx: int):
        if not self.panel:
            return
        if 0 <= idx < len(self.panel.data["notes"]):
            self.panel.open_note(idx)
        self.panel.show_and_raise()

    def show_note_id(self, nid: str):
        if self.panel:
            idx = self.panel.index_of(nid)
            if idx is not None:
                self.show_note(idx)

    def retheme(self):
        if not self.panel:
            return
        visible, edit = self.panel.isVisible(), self.panel.edit_mode
        self._close_stickies()
        self.panel.teardown()
        self.panel.deleteLater()
        with self.themed():
            self.panel = NotePanel(self)
        self._open_stickies()
        if visible:
            self.panel.show()
            if edit:
                self.panel.set_edit(True)

    def make_page(self) -> QWidget:
        """The overlay's Notes page."""
        return NotesPage(self)

    def actions(self):
        return [("toggle", "Show / hide Notes", self.toggle), ("edit", "Notes edit mode", self.toggle_edit),
                ("new", "New note…", self._new_note)]

    def _new_note(self):
        if self.panel:
            self.panel.show_and_raise()
            self.panel.show_home()
            self.panel.add_note()

    def palette_entries(self, Entry) -> list:
        if not self.panel:
            return []
        out = []
        for i, n in enumerate(self.panel.data["notes"]):
            body = n.get("content", "") if n.get("type") != "todo" else " ".join(it.get("text", "") for it in n.get("items", []))
            tags = " ".join(f"#{t}" for t in n.get("tags", []))
            out.append(Entry(f"note:{i}:{n.get('name')}", f"{'📌 ' if n.get('pinned') else ''}{n.get('name')}",
                             lambda idx=i: self.show_note(idx),
                             (tags + " " + body[:60].replace("\n", " ")).strip() or "note", "note"))
        return out

    def toggle_edit(self):
        if self.panel:
            if not self.panel.isVisible():
                self.panel.show_and_raise()
            self.panel.toggle_edit()

    def tray_actions(self):
        return [("Show / hide Notes", self.toggle)]
