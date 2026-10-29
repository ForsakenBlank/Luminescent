"""
Calendar page: Atlas's calendar on the PC. A month with a dot on every day that has something, and the
chosen day's events and tasks from Atlas (as of the last sync) plus Luminescent's own reminders.

Events and tasks are made and changed in Atlas; Sync with Atlas (top right) brings the newest over.
"""
import calendar as cal
from datetime import date, datetime, timedelta

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QGridLayout, QPushButton, QScrollArea, QFrame
from PyQt6.QtCore import Qt

from core import theme, widgets, atlas_sync


class CalendarPage(QWidget):
    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        self.month = date.today().replace(day=1)
        self.day = date.today()
        lay = QVBoxLayout(self)
        lay.setContentsMargins(24, 12, 24, 24)
        lay.setSpacing(14)
        head = QHBoxLayout()
        self.title = widgets.label("", 15, 600)
        head.addWidget(self.title)
        head.addStretch()
        prev = widgets.icon_button("‹", "Month before", 30, 14)
        prev.clicked.connect(lambda: self._step(-1))
        today = widgets.button("Today", "ghost")
        today.clicked.connect(self._today)
        nxt = widgets.icon_button("›", "Month after", 30, 14)
        nxt.clicked.connect(lambda: self._step(1))
        sync = widgets.button("Sync with Atlas", "primary")
        sync.clicked.connect(self._sync)
        for w in (prev, today, nxt, sync):
            head.addWidget(w)
        lay.addLayout(head)
        body = QHBoxLayout()
        body.setSpacing(24)
        left = QVBoxLayout()
        self.grid = QGridLayout()
        self.grid.setSpacing(4)
        left.addLayout(self.grid)
        left.addStretch()
        self.synced = widgets.label("", 11, tone="muted")
        self.synced.setWordWrap(True)
        left.addWidget(self.synced)
        lw = QWidget()
        lw.setLayout(left)
        lw.setFixedWidth(420)
        body.addWidget(lw)
        right = QVBoxLayout()
        right.setSpacing(8)
        self.day_title = widgets.label("", 13, 600)
        right.addWidget(self.day_title)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        self.agenda = QVBoxLayout(holder)
        self.agenda.setContentsMargins(0, 0, 8, 0)
        self.agenda.setSpacing(6)
        scroll.setWidget(holder)
        right.addWidget(scroll, 1)
        body.addLayout(right, 1)
        lay.addLayout(body, 1)
        m = self._sync_module()
        if m is not None:
            m.calendar_changed.connect(self.refresh)

    # ── data ───────────────────────────────────────────────────────────────
    def _sync_module(self):
        m = self.launcher.modules.get("atlas_sync")
        return m if m is not None and m.running else None

    def _items_on(self, d: date, events: list, tasks: list) -> list[tuple]:
        """[(sort key, time text, title, kind, colour)] for one day."""
        start = int(datetime(d.year, d.month, d.day).timestamp() * 1000)
        end = start + 86_400_000 - 1
        out = []
        for e in events:
            span = max(0, int(e.get("endsAt") or 0) - int(e.get("startsAt") or 0))
            # an event that started on an earlier day and is still going counts too
            for t in atlas_sync.occurrences(e, start - span, end):
                if t + span < start:
                    continue
                when = "All day" if e.get("allDay") else datetime.fromtimestamp(max(t, start) / 1000).strftime("%H:%M")
                out.append((0 if e.get("allDay") else t, when, e.get("title") or "Event", "event",
                            atlas_sync.int_to_colour(e.get("color")) or theme.P.primary))
        for t in tasks:
            if t.get("due") is None:
                continue
            due = datetime.fromtimestamp(int(t["due"]) / 1000).date()
            if due == d:
                out.append((1, "Done" if t.get("done") else "Task", t.get("title") or "Task",
                            "done" if t.get("done") else "task", theme.P.text_secondary))
        rm = self.launcher.modules.get("reminders")
        if rm is not None and rm.running:
            from modules.reminders import next_due
            for r in rm.items():
                if not r.get("enabled", True):
                    continue
                try:
                    due = next_due(r, datetime(d.year, d.month, d.day) - timedelta(seconds=1))
                except Exception:
                    continue
                if due is not None and due.date() == d:
                    out.append((int(due.timestamp() * 1000), due.strftime("%H:%M"), r.get("title") or "Reminder",
                                "reminder", theme.P.primary_text))
        out.sort(key=lambda x: x[0])
        return out

    # ── drawing ────────────────────────────────────────────────────────────
    def refresh(self):
        m = self._sync_module()
        events, tasks = m.calendar() if m is not None else ([], [])
        self.title.setText(self.month.strftime("%B %Y"))
        widgets.clear_layout(self.grid)
        for i, name in enumerate(("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")):
            h = widgets.label(name.upper(), 9.5, 600, mono=True, spacing=0.1, tone="muted")
            h.setAlignment(Qt.AlignmentFlag.AlignCenter)
            self.grid.addWidget(h, 0, i)
        p = theme.P
        weeks = cal.Calendar(0).monthdatescalendar(self.month.year, self.month.month)
        for r, week in enumerate(weeks, 1):
            for c, d in enumerate(week):
                n = len(self._items_on(d, events, tasks))
                b = QPushButton(f"{d.day}" + ("\n•" if n else "\n "))
                b.setFixedSize(56, 50)
                b.setCursor(Qt.CursorShape.PointingHandCursor)
                sel, today = d == self.day, d == date.today()
                other = d.month != self.month.month
                bg = p.primary_muted if sel else "transparent"
                fg = p.primary_text if sel or today else (p.text_muted if other else p.text_primary)
                border = p.primary if today else "transparent"
                b.setStyleSheet(f"QPushButton {{ background: {bg}; color: {fg}; border: 1px solid {border};"
                                f" border-radius: 10px; font-weight: {600 if today else 400}; }}"
                                f"QPushButton:hover {{ background: {p.bg_card_hover}; }}")
                b.clicked.connect(lambda _c=False, d=d: self._pick(d))
                self.grid.addWidget(b, r, c)
        self._fill_day(events, tasks)
        if m is None:
            self.synced.setText("Turn on Atlas Sync (Settings → Atlas Sync) to see Atlas's calendar here.")
        else:
            peers = m.store.state.get("peers", {}) if m.store else {}
            last = max((p.get("last", 0) for p in peers.values()), default=0)
            self.synced.setText(f"{len(events)} events and {len(tasks)} tasks from Atlas · last synced "
                                f"{datetime.fromtimestamp(last / 1000):%d %b %H:%M}" if last else
                                "Not synced with Atlas yet. Press Sync with Atlas, then connect from Atlas → Sync.")

    def _fill_day(self, events: list, tasks: list):
        d = self.day
        rel = {date.today(): "Today", date.today() + timedelta(1): "Tomorrow",
               date.today() - timedelta(1): "Yesterday"}.get(d)
        self.day_title.setText((rel + " · " if rel else "") + d.strftime("%A %d %B").replace(" 0", " "))
        widgets.clear_layout(self.agenda)
        items = self._items_on(d, events, tasks)
        if not items:
            self.agenda.addWidget(widgets.label("Nothing on this day.", 13, tone="muted"))
        p = theme.P
        for _k, when, title, kind, colour in items:
            row = QFrame()
            row.setObjectName("lumCard")
            h = QHBoxLayout(row)
            h.setContentsMargins(12, 8, 12, 8)
            h.setSpacing(12)
            bar = QFrame()
            bar.setFixedSize(4, 22)
            bar.setStyleSheet(f"background: {colour}; border-radius: 2px;")
            h.addWidget(bar)
            t = widgets.label(when, 11.5, mono=True, tone="secondary")
            t.setFixedWidth(64)
            h.addWidget(t)
            name = widgets.label(title, 13, tone="muted" if kind == "done" else "")
            if kind == "done":
                name.setStyleSheet("text-decoration: line-through;")
            h.addWidget(name, 1)
            h.addWidget(widgets.label({"event": "ATLAS", "task": "TASK", "done": "TASK",
                                       "reminder": "REMINDER"}[kind], 9, 600, mono=True, spacing=0.1, tone="muted"))
            self.agenda.addWidget(row)
        self.agenda.addStretch()

    # ── actions ────────────────────────────────────────────────────────────
    def _pick(self, d: date):
        self.day = d
        if d.month != self.month.month:
            self.month = d.replace(day=1)
        self.refresh()

    def _step(self, months: int):
        m = self.month.month - 1 + months
        self.month = date(self.month.year + m // 12, m % 12 + 1, 1)
        self.refresh()

    def _today(self):
        self.day = date.today()
        self.month = self.day.replace(day=1)
        self.refresh()

    def _sync(self):
        m = self._sync_module()
        if m is None:
            self.launcher.open_settings("atlas_sync")
            return
        m.start_hosting()

    def state(self) -> dict:
        return {}

    def restore_state(self, st: dict):
        pass

    def on_show(self):
        self.refresh()
