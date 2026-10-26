"""
Notes: a floating always-on-top panel holding plain notes and to-do lists.

Hotkeys: show/hide, edit mode.  Drag the header to move the panel any time;
edit mode = resize/anchor the panel and add/rename/delete notes.  Right-click a note on the home list for the same
actions without entering edit mode, and to pin it: pinned notes sort first and
(setting) showing the panel opens the first pinned note.  The search box filters
by name and content as you type.
"""
import os

from PyQt6.QtWidgets import (
    QWidget, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QFrame, QTextEdit,
    QLineEdit, QScrollArea, QSizePolicy
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal

from core import DATA_DIR, theme, widgets, save_json, load_json
from core.module import Module

SAVE_PATH = os.path.join(DATA_DIR, "notes.json")


def _default_data(w, h):
    return {"notes": [{"name": "My First Note", "type": "note", "content": "", "items": []}],
            "x": 100, "y": 100, "w": w, "h": h, "open": None}


class TodoItem(QFrame):
    deleted = pyqtSignal(object)
    changed = pyqtSignal()

    def __init__(self, text: str, checked: bool, font_size: int):
        super().__init__()
        self.setObjectName("todoRow")
        p = theme.P
        self.setStyleSheet(f"#todoRow {{ background: transparent; border: none; border-radius: 6px; }}"
                           f"#todoRow:hover {{ background: {p.bg_card_hover}; }}")
        self._font_size = font_size
        self.setMinimumHeight(34)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(8, 4, 4, 4)
        lay.setSpacing(10)

        self.check = QPushButton()
        self.check.setFixedSize(16, 16)
        self.check.setCheckable(True)
        self.check.setChecked(checked)
        self.check.setCursor(Qt.CursorShape.PointingHandCursor)
        self.check.clicked.connect(self._toggle)

        self.label = QLabel(text)
        self.label.setWordWrap(True)
        self.label.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        self.label.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)

        self.edit = QLineEdit(text)
        self.edit.hide()
        self.edit.returnPressed.connect(self._finish_edit)
        self.edit.editingFinished.connect(self._finish_edit)

        delete = widgets.icon_button("✕", "Remove", 22, 10, ghost=True)
        delete.clicked.connect(lambda: self.deleted.emit(self))

        lay.addWidget(self.check)
        lay.addWidget(self.label, 1)
        lay.addWidget(self.edit, 1)
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

    def _toggle(self):
        self._style()
        self.changed.emit()

    def mouseDoubleClickEvent(self, e):
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
        return {"text": self.label.text(), "checked": self.check.isChecked()}


class NotePanel(widgets.FloatingPanel):
    def __init__(self, module: "NotesModule"):
        self.m = module
        super().__init__("Notes",
                         min_size=(int(module.s("min_width", 250)), int(module.s("min_height", 180))),
                         opacity=float(module.s("bg_opacity", 0.85)),
                         drag_needs_edit_mode=False, close_hides=True)
        self.data = self._load()
        self.current: int | None = None
        self._todo_items: list[TodoItem] = []
        self._dim: widgets.DimOverlay | None = None
        self._anchor_bar: widgets.AnchorBar | None = None
        self._editor: QTextEdit | None = None
        self._query = ""
        self._search: QLineEdit | None = None
        self._list_layout: QVBoxLayout | None = None
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.setInterval(400)
        self._save_timer.timeout.connect(self._save_now)

        self.setGeometry(self.data["x"], self.data["y"], self.data["w"], self.data["h"])
        self.clamp_to_screens()

        # header: back button, side panel
        self.back_btn = widgets.icon_button("←", "Back to all notes", 24, 13, ghost=True)
        self.back_btn.clicked.connect(self.show_home)
        self.back_btn.hide()
        self.header_left.addWidget(self.back_btn)
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
        d["notes"] = [n for n in d["notes"] if isinstance(n, dict)]
        for n in d["notes"]:
            n.setdefault("name", "Note")
            n.setdefault("content", "")
            n.setdefault("items", [])
            n.setdefault("type", "note")
            n.setdefault("pinned", False)
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

    # ── views ──────────────────────────────────────────────────────────────
    def _clear_views(self):
        self._search = self._list_layout = None
        self._todo_badge = None
        widgets.clear_layout(self.content)
        widgets.clear_layout(self.side_layout)
        self._editor = None
        self._todo_items = []

    def show_home(self):
        self._capture_current()
        self.current = None
        self.back_btn.hide()
        self.set_title("Notes")
        self._clear_views()

        top = QFrame()
        widgets.style(top, "background: transparent; border: none;")
        tl = QHBoxLayout(top)
        tl.setContentsMargins(14, 12, 14, 0)
        self._search = QLineEdit(self._query)
        self._search.setFixedHeight(32)
        self._search.setPlaceholderText("Search notes")
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
        hay = [note.get("name", ""), note.get("content", "")] + [it.get("text", "") for it in note.get("items", [])]
        return any(q in h.lower() for h in hay)

    def _fill_list(self):
        v = self._list_layout
        v.setContentsMargins(14, 10, 14, 14)
        v.setSpacing(6)
        p = theme.P
        order = sorted(range(len(self.data["notes"])), key=lambda i: (not self.data["notes"][i].get("pinned"), i))
        shown = 0
        for i in order:
            note = self.data["notes"][i]
            if not self._matches(note):
                continue
            shown += 1
            card = QFrame()
            card.setObjectName("noteCard")
            card.setCursor(Qt.CursorShape.PointingHandCursor)
            card.setFixedHeight(48)
            card.setStyleSheet(f"""
                #noteCard {{ background: {p.bg_card}; border: 1px solid {p.border}; border-radius: {p.radius}px; }}
                #noteCard:hover {{ background: {p.bg_card_hover}; }}
            """)
            cl = QHBoxLayout(card)
            cl.setContentsMargins(14, 0, 12, 0)
            cl.setSpacing(10)
            tick = QFrame()
            tick.setFixedSize(3, 12)
            tick.setStyleSheet(f"background: {p.primary if note.get('pinned') else p.border}; border-radius: 1px;")
            name = widgets.label(note["name"], 13, 500)
            name.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
            if note["type"] == "todo":
                done = sum(1 for it in note["items"] if it.get("checked"))
                badge = self._badge(f"{done}/{len(note['items'])}")
            else:
                badge = widgets.label("NOTE", 9, 500, mono=True, spacing=0.08, tone="muted")
            if note.get("pinned"):
                cl.addWidget(tick)
            cl.addWidget(name, 1)
            if note.get("pinned"):
                cl.addWidget(widgets.label("PINNED", 9, 500, mono=True, spacing=0.08, tone="accent"))
            cl.addWidget(badge)
            card.mousePressEvent = lambda e, idx=i: self._card_click(e, idx)
            v.addWidget(card)
        if not shown:
            text = ("No notes yet.\nPress the edit hotkey and click + to add one." if not self.data["notes"]
                    else f'Nothing matches "{self._query.strip()}".')
            empty = widgets.label(text, 12, tone="muted")
            empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
            v.addWidget(empty)
        v.addStretch()

    def _card_click(self, e, idx: int):
        if e.button() == Qt.MouseButton.RightButton:
            self._note_menu(idx, e.globalPosition().toPoint())
        elif e.button() == Qt.MouseButton.LeftButton:
            self.open_note(idx)

    def _note_menu(self, idx: int, pos):
        m = widgets.menu(self)
        m.addAction("Open", lambda: self.open_note(idx))
        pinned = self.data["notes"][idx].get("pinned")
        m.addAction("Unpin" if pinned else "📌  Pin (shown first, opened first)", lambda: self._pin(idx, not pinned))
        m.addAction("Rename…", lambda: self.rename_note(idx))
        m.addSeparator()
        if idx > 0:
            m.addAction("Move up", lambda: self._move_note(idx, -1))
        if idx < len(self.data["notes"]) - 1:
            m.addAction("Move down", lambda: self._move_note(idx, 1))
        m.addSeparator()
        m.addAction("Delete…", lambda: self.delete_note(idx))
        m.exec(pos)

    def _pin(self, idx: int, on: bool):
        self.data["notes"][idx]["pinned"] = on
        self.show_home()

    def first_pinned(self) -> int | None:
        return next((i for i, n in enumerate(self.data["notes"]) if n.get("pinned")), None)

    def open_note(self, idx: int):
        self._capture_current()
        self._search = self._list_layout = None
        self.current = idx
        note = self.data["notes"][idx]
        self.back_btn.show()
        self.set_title("Notes", note["name"])
        self._clear_views()
        if note["type"] == "todo":
            self._build_todo(note)
        else:
            self._build_editor(note)
        self._rebuild_side()
        self.schedule_save()

    def _build_editor(self, note: dict):
        p = theme.P
        ed = QTextEdit()
        ed.setPlaceholderText("Start typing…")
        ed.setStyleSheet(f"""
            QTextEdit {{ background: transparent; border: none; padding: 12px 14px;
                         font-size: {int(self.m.s('font_size', 16))}px; color: {p.text_primary}; }}
        """)
        ed.setPlainText(note.get("content", ""))
        ed.textChanged.connect(self._editor_changed)
        self.content.addWidget(ed)
        self._editor = ed

    def _editor_changed(self):
        if self.current is not None and self._editor is not None:
            self.data["notes"][self.current]["content"] = self._editor.toPlainText()
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
        self._todo_fill.setStyleSheet(f"background: {p.primary}; border-radius: 1px;")
        self._todo_track = track
        wrap = QHBoxLayout()
        wrap.setContentsMargins(14, 0, 14, 10)
        wrap.addWidget(track)
        self.content.addLayout(wrap)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        self._todo_layout = QVBoxLayout(holder)
        self._todo_layout.setContentsMargins(6, 0, 6, 4)
        self._todo_layout.setSpacing(0)
        for it in note.get("items", []):
            self._add_todo_widget(it.get("text", ""), it.get("checked", False))
        self._todo_layout.addStretch()
        scroll.setWidget(holder)
        self.content.addWidget(scroll, 1)

        row = QFrame()
        widgets.style(row, "background: transparent; border: none;")
        rl = QHBoxLayout(row)
        rl.setContentsMargins(14, 10, 14, 14)
        rl.setSpacing(8)
        self._todo_input = QLineEdit()
        self._todo_input.setFixedHeight(32)
        self._todo_input.setPlaceholderText("Add a task…")
        self._todo_input.returnPressed.connect(self._add_todo_from_input)
        rl.addWidget(self._todo_input, 1)
        clear_done = widgets.button("Clear done", "ghost", "Remove completed items")
        clear_done.setStyleSheet("QPushButton { font-size: 12px; padding: 4px 6px; }")
        clear_done.clicked.connect(self._clear_done)
        rl.addWidget(clear_done)
        self.content.addWidget(row)
        self._update_progress()

    def _update_progress(self):
        badge = getattr(self, "_todo_badge", None)
        if badge is None or self.current is None:
            return
        try:
            items = [i.data() for i in self._todo_items]
            done = sum(1 for i in items if i["checked"])
            badge.setText(f"{done}/{len(items)}")
            frac = done / len(items) if items else 0.0
            self._todo_fill.setGeometry(0, 0, int(self._todo_track.width() * frac), 2)
        except RuntimeError:
            self._todo_badge = None         # the to-do view was closed

    def resizeEvent(self, e):
        super().resizeEvent(e)
        QTimer.singleShot(0, self._update_progress)

    def _add_todo_widget(self, text: str, checked: bool):
        item = TodoItem(text, checked, int(self.m.s("todo_font_size", 12)))
        item.deleted.connect(self._remove_todo)
        item.changed.connect(self._todos_changed)
        self._todo_layout.insertWidget(max(0, self._todo_layout.count() - 1), item)
        self._todo_items.append(item)

    def _add_todo_from_input(self):
        text = self._todo_input.text().strip()
        if text:
            self._add_todo_widget(text, False)
            self._todo_input.clear()
            self._todos_changed()

    def _remove_todo(self, item: TodoItem):
        if item in self._todo_items:
            self._todo_items.remove(item)
        item.setParent(None)
        item.deleteLater()
        self._todos_changed()

    def _clear_done(self):
        for it in [i for i in self._todo_items if i.check.isChecked()]:
            self._todo_items.remove(it)
            it.setParent(None)
            it.deleteLater()
        self._todos_changed()

    def _todos_changed(self):
        self._update_progress()
        if self.current is not None:
            self.data["notes"][self.current]["items"] = [i.data() for i in self._todo_items]
            self.schedule_save()

    def _capture_current(self):
        if self.current is None or self.current >= len(self.data["notes"]):
            return
        note = self.data["notes"][self.current]
        if note["type"] == "todo":
            note["items"] = [i.data() for i in self._todo_items]
        elif self._editor is not None:
            note["content"] = self._editor.toPlainText()

    def _header_menu(self, pos):
        m = widgets.menu(self)
        if self.current is None:
            m.addAction("New note…", self.add_note)
        else:
            idx = self.current
            m.addAction("Rename…", lambda: self.rename_note(idx))
            pinned = self.data["notes"][idx].get("pinned")
            m.addAction("Unpin" if pinned else "Pin (shown first, opened first)", lambda: self._pin(idx, not pinned))
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
        self.data["notes"].append({"name": name.strip(), "type": kind, "content": "", "items": []})
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
        if self._dim:
            self._dim.close()
        if self._anchor_bar:
            self._anchor_bar.close()
        self.close()


class NotesModule(Module):
    key = "notes"
    title = "Notes"
    icon = "📝"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: NotePanel | None = None

    def start(self):
        self.panel = NotePanel(self)
        if self.ctx.shows_on_start("notes"):
            self.panel.show()
        self.hotkey("hotkey", self.toggle, "show/hide notes")
        self.hotkey("edit_hotkey", self.toggle_edit, "notes edit mode")

    def stop(self):
        if self.panel:
            self.panel.teardown()
            self.panel.deleteLater()
            self.panel = None

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

    def retheme(self):
        if not self.panel:
            return
        visible, edit = self.panel.isVisible(), self.panel.edit_mode
        self.panel.teardown()
        self.panel.deleteLater()
        self.panel = NotePanel(self)
        if visible:
            self.panel.show()
            if edit:
                self.panel.set_edit(True)

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
            out.append(Entry(f"note:{i}:{n.get('name')}", f"{'📌 ' if n.get('pinned') else ''}{n.get('name')}",
                             lambda idx=i: self.show_note(idx), body[:60].replace("\n", " ") or "note", "note"))
        return out

    def toggle_edit(self):
        if self.panel:
            if not self.panel.isVisible():
                self.panel.show_and_raise()
            self.panel.toggle_edit()

    def tray_actions(self):
        return [("Show / hide Notes", self.toggle)]
