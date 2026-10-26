"""
Reminders: pop-ups, sounds and desktop notifications at a set time.

Schedule types
  once        a specific date + time
  daily       every day at a time
  weekly      chosen weekdays at a time
  monthly     a day of the month at a time (the 31st means the last day in shorter months)
  every_days  every N days at a time, counted from a start date
  interval    every N minutes (optionally only between two times of day)
  startup     when the launcher starts

Quick reminders ("remind me in 20m to stretch" in the command palette) and to-do due
dates (Notes) are ordinary "once" reminders that Luminescent adds itself.

Reminder definitions live in settings.json under reminders.items (edited in
Settings → Reminders).  Runtime state (last fired, snoozed until) is kept in
data/reminders_state.json so that saving settings never clobbers it.
"""
import os
import re
import json
import uuid
import calendar
import logging
from datetime import datetime, date, time as dtime, timedelta

from PyQt6.QtWidgets import (
    QWidget, QLabel, QVBoxLayout, QHBoxLayout, QFrame, QListWidget, QListWidgetItem, QLineEdit,
    QTextEdit, QCheckBox, QComboBox, QSpinBox, QDateEdit, QTimeEdit, QPushButton, QFileDialog,
    QStackedWidget, QFormLayout, QScrollArea, QSizePolicy
)
from PyQt6.QtCore import Qt, QTimer, QDate, QTime, pyqtSignal

from core import DATA_DIR, theme, widgets, sounds, save_json, load_json
from core.timeparse import parse_duration, parse_clock
from core.module import Module
from core.settings import ANCHOR_CHOICES, SOUND_CHOICES, AUDIO_FILTER

log = logging.getLogger("reminders")
STATE_PATH = os.path.join(DATA_DIR, "reminders_state.json")
DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


# ── schedule maths ─────────────────────────────────────────────────────────────
def _parse_time(s: str, default=dtime(9, 0)) -> dtime:
    try:
        parts = s.split(":")
        return dtime(int(parts[0]), int(parts[1]), int(parts[2]) if len(parts) > 2 else 0)
    except Exception:
        return default


def _parse_date(s: str) -> date | None:
    try:
        return date.fromisoformat(s)
    except Exception:
        return None


def next_due(rem: dict, after: datetime) -> datetime | None:
    """First moment strictly after `after` when `rem` should fire, or None if never."""
    sch = rem.get("schedule", {})
    kind = sch.get("type", "daily")
    t = _parse_time(sch.get("time", "09:00"))

    if kind == "startup":
        return None                     # fired by RemindersModule.start(), not by the clock

    if kind == "once":
        d = _parse_date(sch.get("date", ""))
        if d is None:
            return None
        due = datetime.combine(d, t)
        return due if due > after else None

    if kind == "daily":
        due = datetime.combine(after.date(), t)
        return due if due > after else due + timedelta(days=1)

    if kind == "monthly":
        day = max(1, min(31, int(sch.get("monthday", 1))))
        y, mth = after.year, after.month
        for _ in range(14):
            d = date(y, mth, min(day, calendar.monthrange(y, mth)[1]))
            due = datetime.combine(d, t)
            if due > after:
                return due
            y, mth = (y + 1, 1) if mth == 12 else (y, mth + 1)
        return None

    if kind == "every_days":
        every = max(1, int(sch.get("every_days", 2)))
        start = _parse_date(sch.get("date", "")) or after.date()
        first = datetime.combine(start, t)
        if first > after:
            return first
        steps = (after - first).days // every
        due = first + timedelta(days=steps * every)
        while due <= after:
            due += timedelta(days=every)
        return due

    if kind == "weekly":
        days = sch.get("days") or []
        if not days:
            return None
        for i in range(8):
            d = after.date() + timedelta(days=i)
            if d.weekday() in days:
                due = datetime.combine(d, t)
                if due > after:
                    return due
        return None

    if kind == "interval":
        every = max(1, int(sch.get("every_minutes", 60)))
        due = after + timedelta(minutes=every)
        if sch.get("window"):
            start, end = _parse_time(sch.get("from", "09:00")), _parse_time(sch.get("to", "18:00"))
            if start <= end:
                if due.time() < start:
                    due = datetime.combine(due.date(), start)
                elif due.time() > end:
                    due = datetime.combine(due.date() + timedelta(days=1), start)
            else:  # window wraps midnight, e.g. 22:00-02:00
                if end < due.time() < start:
                    due = datetime.combine(due.date(), start)
        return due
    return None


def describe(rem: dict) -> str:
    sch = rem.get("schedule", {})
    kind = sch.get("type", "daily")
    t = sch.get("time", "09:00")
    if kind == "startup":
        return f"When the launcher starts (after {int(sch.get('startup_delay', 10))} s)"
    if kind == "once":
        return f"Once on {sch.get('date', '?')} at {t}"
    if kind == "daily":
        return f"Every day at {t}"
    if kind == "weekly":
        days = ", ".join(DAY_NAMES[d] for d in sorted(sch.get("days") or []))
        return f"{days or 'No days'} at {t}"
    if kind == "monthly":
        d = int(sch.get("monthday", 1))
        suffix = "th" if 11 <= d % 100 <= 13 else {1: "st", 2: "nd", 3: "rd"}.get(d % 10, "th")
        return f"Every month on the {d}{suffix} at {t}"
    if kind == "every_days":
        n = int(sch.get("every_days", 2))
        return f"Every {n} days at {t} (from {sch.get('date', '?')})"
    if kind == "interval":
        s = f"Every {sch.get('every_minutes', 60)} min"
        if sch.get("window"):
            s += f" ({sch.get('from', '09:00')}–{sch.get('to', '18:00')})"
        return s
    return kind


def new_reminder() -> dict:
    return {
        "id": uuid.uuid4().hex,
        "title": "New reminder",
        "message": "",
        "enabled": True,
        "schedule": {"type": "daily", "time": "09:00", "date": date.today().isoformat(),
                     "days": [0, 1, 2, 3, 4], "every_minutes": 60, "window": False, "from": "09:00", "to": "18:00",
                     "startup_delay": 10, "monthday": date.today().day, "every_days": 2},
        "actions": {"popup": True, "position": "default", "sound": "default", "sound_file": "",
                    "sound_repeat": 1, "toast": True},
    }


_UNIT = r"(?:seconds?|secs?|s|minutes?|mins?|m|hours?|hrs?|h|days?|d|weeks?|w)(?![a-z])"
_DUR = rf"[\d.,:]+\s*(?:{_UNIT})?(?:\s*\d+\s*(?:{_UNIT})?)?"
_QUICK = [
    # remind me in 20m to stretch / in 1h30 call mum
    re.compile(r"^(?:remind(?:\s+me)?\s+)?in\s+(?P<dur>" + _DUR + r")\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+)$",
               re.I),
    # remind me to stretch in 20m
    re.compile(r"^remind(?:\s+me)?\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+?)\s+in\s+(?P<dur>" + _DUR + r")$",
               re.I),
    # remind me at 17:30 to call mum / at 9am standup / tomorrow at 9 dentist
    re.compile(r"^(?:remind(?:\s+me)?\s+)?(?P<tmrw>tomorrow\s+)?at\s+(?P<clock>\d{1,2}(?:[:.]\d{2})?\s*(?:am|pm)?)\s+"
               r"(?:to\s+|that\s+|about\s+)?(?P<what>.+)$", re.I),
    # remind me to call mum at 17:30
    re.compile(r"^remind(?:\s+me)?\s+(?:to\s+|that\s+|about\s+)?(?P<what>.+?)\s+(?P<tmrw>tomorrow\s+)?at\s+"
               r"(?P<clock>\d{1,2}(?:[:.]\d{2})?\s*(?:am|pm)?)$", re.I),
]


def parse_quick(text: str, now: datetime | None = None) -> tuple[datetime, str] | None:
    """(when, what) for "remind me in 20m to stretch", "at 17:30 call mum"... None if it is not one."""
    now = now or datetime.now()
    t = (text or "").strip()
    if len(t) < 6:
        return None
    for rx in _QUICK:
        m = rx.match(t)
        if not m:
            continue
        what = m.group("what").strip().rstrip(".")
        if not what or re.fullmatch(_UNIT, what.lower()):
            continue
        if "dur" in m.groupdict() and m.group("dur"):
            secs = parse_duration(m.group("dur"))
            if not secs or secs < 1:
                continue
            return now + timedelta(seconds=secs), what[:1].upper() + what[1:]
        when = parse_clock(m.group("clock"), now)
        if when is None:
            continue
        if m.groupdict().get("tmrw"):
            when = datetime.combine(now.date() + timedelta(days=1), when.time())
        return when, what[:1].upper() + what[1:]
    return None


# ── pop-up window ──────────────────────────────────────────────────────────────
class ReminderPopup(QWidget):
    dismissed = pyqtSignal(object)
    snoozed = pyqtSignal(object)

    def __init__(self, rem: dict, snooze_minutes: int, timeout_s: int = 0):
        super().__init__()
        self.rem = rem
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.setFixedWidth(320)
        p = theme.P

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        box = QFrame()
        box.setObjectName("reminderBox")
        box.setStyleSheet(f"#reminderBox {{ background: {p.bg_base}; border: 1px solid {p.border};"
                          f" border-radius: {p.radius}px; }}")
        outer.addWidget(box)
        lay = QVBoxLayout(box)
        lay.setContentsMargins(1, 1, 1, 1)
        lay.setSpacing(0)
        line = QFrame()
        line.setFixedHeight(2)
        line.setStyleSheet(f"background: {p.primary}; border-top-left-radius: {p.radius - 1}px;"
                           f" border-top-right-radius: {p.radius - 1}px;")
        lay.addWidget(line)

        text = QVBoxLayout()
        text.setContentsMargins(15, 13, 15, 12)
        text.setSpacing(6)
        head = QHBoxLayout()
        head.setSpacing(8)
        head.addWidget(widgets.label("REMINDER", 9.5, 600, mono=True, spacing=0.14, tone="accent"))
        head.addStretch()
        when = widgets.label(datetime.now().strftime("%H:%M"), 10, mono=True, tone="muted")
        when.setToolTip(describe(rem))
        head.addWidget(when)
        text.addLayout(head)
        title = widgets.label(rem.get("title") or "Reminder", 14, 500)
        title.setWordWrap(True)
        text.addWidget(title)
        msg = (rem.get("message") or "").strip()
        if msg:
            body = widgets.label(msg, 12.5, tone="muted")
            body.setWordWrap(True)
            body.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
            text.addWidget(body)
        lay.addLayout(text)

        btns = QHBoxLayout()
        btns.setContentsMargins(15, 0, 15, 14)
        btns.setSpacing(8)
        snooze = widgets.button(f"Snooze {snooze_minutes} min", "outline")
        snooze.clicked.connect(self._snooze)
        ok = widgets.button("Dismiss", "primary")
        ok.clicked.connect(self._dismiss)
        for b_ in (snooze, ok):
            b_.setFixedHeight(30)
            btns.addWidget(b_, 1)
        lay.addLayout(btns)

        self.adjustSize()
        self._done = False
        if timeout_s > 0:
            t = QTimer(self)                # owned by the popup, so it dies with it
            t.setSingleShot(True)
            t.timeout.connect(self._dismiss)
            t.start(timeout_s * 1000)

    def _dismiss(self):
        if self._done:
            return
        self._done = True
        self.dismissed.emit(self)
        self.close()

    def _snooze(self):
        if self._done:
            return
        self._done = True
        self.snoozed.emit(self)
        self.close()

    def keyPressEvent(self, e):
        if e.key() in (Qt.Key.Key_Escape, Qt.Key.Key_Return, Qt.Key.Key_Enter):
            self._dismiss()


# ── module ─────────────────────────────────────────────────────────────────────
class RemindersModule(Module):
    key = "reminders"
    title = "Reminders"
    icon = "⏰"

    def __init__(self, ctx):
        super().__init__(ctx)
        self._state: dict = {}
        self._popups: list[ReminderPopup] = []
        self._timer = QTimer(self)
        self._timer.setInterval(10_000)
        self._timer.timeout.connect(self.tick)
        self._startup_fired = False      # once per process, not once per enable/disable

    # ── lifecycle ──────────────────────────────────────────────────────────
    def start(self):
        self._load_state()
        self._prime_new_reminders()
        self._timer.start()
        QTimer.singleShot(1500, self.tick)
        self._fire_startup_reminders()
        self.log.info("%d reminder(s) loaded", len(self.items()))

    def _fire_startup_reminders(self):
        if self._startup_fired:
            return
        self._startup_fired = True
        for r in self.items():
            if r.get("enabled", True) and r.get("schedule", {}).get("type") == "startup":
                delay = max(1, int(r["schedule"].get("startup_delay", 10)))
                QTimer.singleShot(delay * 1000, lambda r=r: self.fire(r) if self.running else None)

    def stop(self):
        self._timer.stop()
        for p in list(self._popups):
            p.close()
        self._popups = []
        self._save_state()

    def apply_settings(self):
        self._prime_new_reminders()
        self._save_state()

    def tray_actions(self):
        return [("Reminders…", lambda: self.ctx.open_settings("reminders"))]

    # ── reminders other modules add ────────────────────────────────────────
    def add_quick(self, title: str, when: datetime, rid: str = "", message: str = "") -> str:
        """Add (or replace, by id) a one-off reminder at `when`. Returns its id."""
        rem = new_reminder()
        rem["id"] = rid or rem["id"]
        rem["title"] = title
        rem["message"] = message
        rem["schedule"].update({"type": "once", "date": when.date().isoformat(), "time": when.strftime("%H:%M:%S")})
        rem["quick"] = True
        items = [r for r in (self.s("items", []) or []) if isinstance(r, dict) and r.get("id") != rem["id"]]
        # tidy up: finished quick reminders older than a day
        cutoff = (date.today() - timedelta(days=1)).isoformat()
        items = [r for r in items if not (r.get("quick") and not r.get("enabled", True)
                                          and r.get("schedule", {}).get("date", "") < cutoff)]
        items.append(rem)
        self.settings.set("reminders.items", items)
        self.settings.save_quiet()
        st = self._state.setdefault(rem["id"], {})
        st["last"] = datetime.now().isoformat(timespec="seconds")
        st["sched"] = json.dumps(rem["schedule"], sort_keys=True)
        st.pop("snoozed_until", None)
        self._save_state()
        self.log.info("quick reminder %r at %s", title, when)
        return rem["id"]

    def remove(self, rid: str):
        items = [r for r in (self.s("items", []) or []) if isinstance(r, dict)]
        keep = [r for r in items if r.get("id") != rid]
        if len(keep) != len(items):
            self.settings.set("reminders.items", keep)
            self.settings.save_quiet()
            self._state.pop(rid, None)
            self._save_state()

    def next_reminder(self) -> tuple[datetime, str] | None:
        """The next reminder that will fire (for the overlay's status strip)."""
        now = datetime.now()
        best = None
        for r in self.items():
            if not r.get("enabled", True):
                continue
            st = self._state.get(r["id"], {})
            due = None
            if st.get("snoozed_until"):
                try:
                    due = datetime.fromisoformat(st["snoozed_until"])
                except ValueError:
                    due = None
            if due is None:
                due = next_due(r, now)
            if due is not None and (best is None or due < best[0]):
                best = (due, r.get("title") or "Reminder")
        return best

    # ── data ───────────────────────────────────────────────────────────────
    def items(self) -> list[dict]:
        return [r for r in (self.s("items", []) or []) if isinstance(r, dict) and r.get("id")]

    def _load_state(self):
        state = load_json(STATE_PATH, {})
        self._state = state if isinstance(state, dict) else {}

    def _save_state(self):
        try:
            save_json(STATE_PATH, self._state, indent=2)
        except Exception:
            self.log.exception("could not save reminders_state.json")

    def _prime_new_reminders(self):
        """A reminder we have never seen starts its clock now (so 'every 30 min' fires in 30 min, not instantly).

        The same goes for one whose schedule was just edited: moving 21:30 to 08:00 at 10:00 used to
        fire it straight away as a missed 08:00 "catch-up".
        """
        now = datetime.now().isoformat(timespec="seconds")
        for r in self.items():
            st = self._state.setdefault(r["id"], {})
            sig = json.dumps(r.get("schedule", {}), sort_keys=True)
            if "last" not in st or ("sched" in st and st["sched"] != sig):
                st["last"] = now
            st["sched"] = sig

    # ── scheduling ─────────────────────────────────────────────────────────
    def tick(self):
        now = datetime.now()
        grace = timedelta(minutes=max(2, int(self.s("catch_up_minutes", 120))))
        changed = False
        for r in self.items():
            if not r.get("enabled", True):
                continue
            st = self._state.setdefault(r["id"], {})
            snooze = st.get("snoozed_until")
            if snooze:
                try:
                    if datetime.fromisoformat(snooze) > now + timedelta(minutes=int(self.s("snooze_minutes", 10)) + 5):
                        snooze = now.isoformat(timespec="seconds")   # saved while the clock was ahead
                    if datetime.fromisoformat(snooze) <= now:
                        st.pop("snoozed_until", None)
                        # the snoozed firing replaces the one it postponed; without this an "every N min"
                        # reminder fired twice in a row when the snooze was at least N minutes long
                        st["last"] = now.isoformat(timespec="seconds")
                        self.fire(r)
                        changed = True
                except ValueError:
                    st.pop("snoozed_until", None)
                continue
            last_s = st.get("last")
            try:
                last = datetime.fromisoformat(last_s) if last_s else now
            except ValueError:
                last = now
            if last > now + timedelta(minutes=5):
                # the clock was ahead when this was saved (a "last" in 2026-11 silenced the daily
                # reminders until that date); count from now instead
                self.log.info("%r: last fired %s is in the future; resetting", r.get("title"), last_s)
                last = now
                st["last"] = now.isoformat(timespec="seconds")
                changed = True
            after = max(last, now - grace)
            due = next_due(r, after)
            if due is None or due > now:
                continue
            st["last"] = due.isoformat(timespec="seconds")
            changed = True
            self.fire(r)
            if r.get("schedule", {}).get("type") == "once":
                self._disable_once(r["id"])
        if changed:
            self._save_state()

    def _disable_once(self, rid: str):
        items = self.s("items", []) or []
        for r in items:
            if r.get("id") == rid:
                r["enabled"] = False
        self.settings.set("reminders.items", items)
        self.settings.save(emit=False)

    # ── firing ─────────────────────────────────────────────────────────────
    def fire(self, rem: dict, preview: bool = False):
        acts = rem.get("actions", {})
        self.log.info("firing %r", rem.get("title"))
        # theme switch (time-based themes)
        tid = acts.get("theme") or ""
        launcher = self.ctx.launcher
        if tid and launcher is not None and launcher.themes.get(tid) is not None:
            QTimer.singleShot(0, lambda: launcher.apply_theme(tid))
        # sound
        kind = acts.get("sound", "default")
        file = acts.get("sound_file", "")
        if kind == "default":
            kind, file = self.s("default_sound", "chime"), self.s("default_sound_file", "")
        repeat = max(1, int(acts.get("sound_repeat", 1)))
        for i in range(repeat):
            QTimer.singleShot(i * 3000, lambda k=kind, f=file: sounds.play(k, f))
        # toast
        if acts.get("toast", True) and not preview:
            self.ctx.toast(rem.get("title") or "Reminder", rem.get("message") or describe(rem))
        # popup
        if acts.get("popup", True):
            self.show_popup(rem)

    def show_popup(self, rem: dict):
        pos_key = rem.get("actions", {}).get("position", "default")
        if pos_key == "default":
            pos_key = self.s("popup_position", "br")
        screen = widgets.cursor_screen_geometry() if self.s("popup_screen") == "cursor" else widgets.primary_geometry()
        popup = ReminderPopup(rem, int(self.s("snooze_minutes", 10)), int(self.s("popup_timeout", 0)))
        popup.dismissed.connect(self._popup_closed)
        popup.snoozed.connect(self._popup_snoozed)
        # stack below/above other popups at the same anchor
        offset = sum(p.height() + 8 for p in self._popups if p.property("anchor") == pos_key)
        pos = widgets.anchor_position(popup.width(), popup.height(), pos_key, 16, screen)
        if pos_key[0] == "b":
            pos.setY(pos.y() - offset)
        else:
            pos.setY(pos.y() + offset)
        popup.setProperty("anchor", pos_key)
        popup.move(pos)
        popup.show()
        popup.raise_()
        self._popups.append(popup)

    def _popup_closed(self, popup):
        if popup in self._popups:
            self._popups.remove(popup)

    def _popup_snoozed(self, popup):
        self._popup_closed(popup)
        rid = popup.rem.get("id")
        if rid:
            until = datetime.now() + timedelta(minutes=int(self.s("snooze_minutes", 10)))
            self._state.setdefault(rid, {})["snoozed_until"] = until.isoformat(timespec="seconds")
            self._save_state()


# ── settings editor (embedded in the settings window) ─────────────────────────
class RemindersEditor(QWidget):
    """Edits the reminders.items list. get_value()/set_value() like every other settings field."""
    changed = pyqtSignal()

    def __init__(self, preview_fn=None, themes=None):
        super().__init__()
        self._items: list[dict] = []
        self._current: int | None = None
        self._loading = False
        self._preview = preview_fn
        self._themes = themes or (lambda: [])
        p = theme.P

        root = QHBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(12)

        # left: list
        left = QVBoxLayout()
        left.setSpacing(6)
        self.list = QListWidget()
        self.list.setMinimumWidth(220)
        self.list.currentRowChanged.connect(self._select)
        left.addWidget(self.list, 1)
        lb = QHBoxLayout()
        add = widgets.button("＋ Add", "primary")
        add.clicked.connect(self._add)
        dup = widgets.button("Duplicate")
        dup.clicked.connect(self._duplicate)
        rem = widgets.button("Delete")
        rem.setProperty("variant", "danger")
        rem.clicked.connect(self._delete)
        for b in (add, dup, rem):
            lb.addWidget(b)
        left.addLayout(lb)
        root.addLayout(left, 0)

        # right: editor
        self.editor = QScrollArea()
        self.editor.setWidgetResizable(True)
        self.editor.setFrameShape(QFrame.Shape.NoFrame)
        form_host = QWidget()
        widgets.style(form_host, "background: transparent;")
        form = QFormLayout(form_host)
        form.setContentsMargins(4, 0, 8, 0)
        form.setSpacing(8)
        form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.AllNonFixedFieldsGrow)
        self.editor.setWidget(form_host)
        root.addWidget(self.editor, 1)

        self.f_enabled = QCheckBox("Enabled")
        self.f_title = QLineEdit()
        self.f_message = QTextEdit()
        self.f_message.setFixedHeight(70)
        self.f_message.setPlaceholderText("Optional longer text shown in the pop-up")
        form.addRow("", self.f_enabled)
        form.addRow("Title", self.f_title)
        form.addRow("Message", self.f_message)

        sep = QLabel("Schedule")
        sep.setStyleSheet(f"color: {p.text_secondary}; font-weight: bold; padding-top: 6px;")
        form.addRow(sep)
        self.f_type = QComboBox()
        for v, l in (("once", "Once (a specific day)"), ("daily", "Every day"), ("weekly", "Certain weekdays"),
                     ("monthly", "Every month"), ("every_days", "Every N days"),
                     ("interval", "Every N minutes"), ("startup", "When the launcher starts (PC startup)")):
            self.f_type.addItem(l, v)
        self.f_type.currentIndexChanged.connect(self._type_changed)
        form.addRow("Repeat", self.f_type)

        self.f_date = QDateEdit()
        self.f_date.setCalendarPopup(True)
        self.f_date.setDisplayFormat("yyyy-MM-dd")
        self.f_date.setDate(QDate.currentDate())
        self.row_date = form.rowCount()
        form.addRow("Date", self.f_date)

        self.f_time = QTimeEdit()
        self.f_time.setDisplayFormat("HH:mm")
        self.row_time = form.rowCount()
        form.addRow("Time", self.f_time)

        days_w = QWidget()
        widgets.style(days_w, "background: transparent;")
        dl = QHBoxLayout(days_w)
        dl.setContentsMargins(0, 0, 0, 0)
        self.f_days = []
        for i, name in enumerate(DAY_NAMES):
            cb = QCheckBox(name)
            cb.stateChanged.connect(self._field_changed)
            dl.addWidget(cb)
            self.f_days.append(cb)
        dl.addStretch()
        self.row_days = form.rowCount()
        form.addRow("Days", days_w)

        self.f_every = QSpinBox()
        self.f_every.setRange(1, 1440)
        self.f_every.setSuffix(" minutes")
        self.row_every = form.rowCount()
        form.addRow("Every", self.f_every)

        self.f_monthday = QSpinBox()
        self.f_monthday.setRange(1, 31)
        self.f_monthday.setPrefix("day ")
        self.f_monthday.setToolTip("29, 30 and 31 fire on the last day of shorter months")
        self.row_monthday = form.rowCount()
        form.addRow("On", self.f_monthday)

        self.f_every_days = QSpinBox()
        self.f_every_days.setRange(1, 365)
        self.f_every_days.setSuffix(" days")
        self.row_every_days = form.rowCount()
        form.addRow("Every", self.f_every_days)

        win_w = QWidget()
        widgets.style(win_w, "background: transparent;")
        wl = QHBoxLayout(win_w)
        wl.setContentsMargins(0, 0, 0, 0)
        self.f_window = QCheckBox("Only between")
        self.f_from = QTimeEdit(); self.f_from.setDisplayFormat("HH:mm")
        self.f_to = QTimeEdit(); self.f_to.setDisplayFormat("HH:mm")
        wl.addWidget(self.f_window)
        wl.addWidget(self.f_from)
        wl.addWidget(QLabel("and"))
        wl.addWidget(self.f_to)
        wl.addStretch()
        self.row_window = form.rowCount()
        form.addRow("", win_w)

        self.f_delay = QSpinBox()
        self.f_delay.setRange(1, 600)
        self.f_delay.setSuffix(" seconds after start")
        self.row_delay = form.rowCount()
        form.addRow("Show", self.f_delay)

        sep2 = QLabel("What happens")
        sep2.setStyleSheet(f"color: {p.text_secondary}; font-weight: bold; padding-top: 6px;")
        form.addRow(sep2)
        self.f_popup = QCheckBox("Show a pop-up window")
        form.addRow("", self.f_popup)
        self.f_position = QComboBox()
        self.f_position.addItem("Default (from settings below)", "default")
        for v, l in ANCHOR_CHOICES:
            self.f_position.addItem(l, v)
        form.addRow("Pop-up position", self.f_position)
        self.f_toast = QCheckBox("Desktop notification")
        form.addRow("", self.f_toast)
        self.f_sound = QComboBox()
        self.f_sound.addItem("Default (from settings below)", "default")
        for v, l in SOUND_CHOICES:
            self.f_sound.addItem(l, v)
        self.f_sound.currentIndexChanged.connect(self._sound_changed)
        form.addRow("Sound", self.f_sound)
        sf = QWidget()
        widgets.style(sf, "background: transparent;")
        sfl = QHBoxLayout(sf)
        sfl.setContentsMargins(0, 0, 0, 0)
        self.f_sound_file = QLineEdit()
        self.f_sound_file.setPlaceholderText("Path to .wav / .mp3")
        browse = widgets.button("Browse…")
        browse.clicked.connect(self._browse_sound)
        sfl.addWidget(self.f_sound_file, 1)
        sfl.addWidget(browse)
        self.row_sound_file = form.rowCount()
        form.addRow("Sound file", sf)
        self.f_repeat = QSpinBox()
        self.f_repeat.setRange(1, 10)
        self.f_repeat.setSuffix(" time(s)")
        form.addRow("Play sound", self.f_repeat)
        self.f_theme = QComboBox()
        self.f_theme.addItem("(don't change the theme)", "")
        for tid, name in self._themes():
            self.f_theme.addItem(f"Switch to: {name}", tid)
        form.addRow("Theme", self.f_theme)
        tip = QLabel("e.g. a daily 08:00 reminder that switches to Light and a 20:00 one that switches to Dark. "
                     "Untick the pop-up and notification for a silent switch.")
        tip.setWordWrap(True)
        tip.setStyleSheet(f"color: {p.text_muted}; font-size: 11px;")
        form.addRow("", tip)

        test = widgets.button("▶ Test this reminder now")
        test.clicked.connect(self._test)
        form.addRow("", test)
        self.form = form

        for w in (self.f_enabled, self.f_popup, self.f_toast, self.f_window):
            w.stateChanged.connect(self._field_changed)
        for w in (self.f_title, self.f_sound_file):
            w.textChanged.connect(self._field_changed)
        self.f_message.textChanged.connect(self._field_changed)
        for w in (self.f_position, self.f_sound, self.f_theme):
            w.currentIndexChanged.connect(self._field_changed)
        for w in (self.f_every, self.f_repeat, self.f_delay, self.f_monthday, self.f_every_days):
            w.valueChanged.connect(self._field_changed)
        self.f_date.dateChanged.connect(self._field_changed)
        for w in (self.f_time, self.f_from, self.f_to):
            w.timeChanged.connect(self._field_changed)

        self.editor.setEnabled(False)
        self._type_changed()

    # ── value API ──────────────────────────────────────────────────────────
    def set_value(self, items: list):
        self._items = [dict(i) for i in (items or []) if isinstance(i, dict)]
        self._refresh_list()
        if self._items:
            self.list.setCurrentRow(0)

    def get_value(self) -> list:
        self._store_current()
        return [dict(i) for i in self._items]

    # ── list ───────────────────────────────────────────────────────────────
    def _refresh_list(self):
        self._loading = True
        cur = self.list.currentRow()
        self.list.clear()
        for r in self._items:
            it = QListWidgetItem(f"{'●' if r.get('enabled', True) else '○'}  {r.get('title') or '(untitled)'}\n      {describe(r)}")
            self.list.addItem(it)
        self._loading = False
        if 0 <= cur < len(self._items):
            self.list.setCurrentRow(cur)

    def _select(self, row: int):
        if self._loading:
            return
        self._store_current()
        self._current = row if 0 <= row < len(self._items) else None
        self.editor.setEnabled(self._current is not None)
        if self._current is None:
            return
        r = self._items[self._current]
        sch, acts = r.get("schedule", {}), r.get("actions", {})
        self._loading = True
        self.f_enabled.setChecked(r.get("enabled", True))
        self.f_title.setText(r.get("title", ""))
        self.f_message.setPlainText(r.get("message", ""))
        idx = self.f_type.findData(sch.get("type", "daily"))
        self.f_type.setCurrentIndex(max(0, idx))
        d = _parse_date(sch.get("date", "")) or date.today()
        self.f_date.setDate(QDate(d.year, d.month, d.day))
        t = _parse_time(sch.get("time", "09:00"))
        self.f_time.setTime(QTime(t.hour, t.minute))
        for i, cb in enumerate(self.f_days):
            cb.setChecked(i in (sch.get("days") or []))
        self.f_every.setValue(int(sch.get("every_minutes", 60)))
        self.f_monthday.setValue(int(sch.get("monthday", d.day)))
        self.f_every_days.setValue(int(sch.get("every_days", 2)))
        self.f_window.setChecked(bool(sch.get("window", False)))
        self.f_delay.setValue(int(sch.get("startup_delay", 10)))
        f, to = _parse_time(sch.get("from", "09:00")), _parse_time(sch.get("to", "18:00"), dtime(18, 0))
        self.f_from.setTime(QTime(f.hour, f.minute))
        self.f_to.setTime(QTime(to.hour, to.minute))
        self.f_popup.setChecked(acts.get("popup", True))
        self.f_position.setCurrentIndex(max(0, self.f_position.findData(acts.get("position", "default"))))
        self.f_toast.setChecked(acts.get("toast", True))
        self.f_sound.setCurrentIndex(max(0, self.f_sound.findData(acts.get("sound", "default"))))
        self.f_sound_file.setText(acts.get("sound_file", ""))
        self.f_repeat.setValue(int(acts.get("sound_repeat", 1)))
        ti = self.f_theme.findData(acts.get("theme", ""))
        if ti < 0 and acts.get("theme"):
            self.f_theme.addItem("Switch to: (deleted theme)", acts["theme"])
            ti = self.f_theme.count() - 1
        self.f_theme.setCurrentIndex(max(0, ti))
        self._loading = False
        self._type_changed()
        self._sound_changed()

    def _store_current(self):
        if self._current is None or self._current >= len(self._items):
            return
        r = self._items[self._current]
        r["enabled"] = self.f_enabled.isChecked()
        r["title"] = self.f_title.text().strip() or "Reminder"
        r["message"] = self.f_message.toPlainText().strip()
        qd, qt = self.f_date.date(), self.f_time.time()
        old_time = (r.get("schedule") or {}).get("time", "")
        hhmm = f"{qt.hour():02d}:{qt.minute():02d}"
        r["schedule"] = {
            "type": self.f_type.currentData(),
            "date": f"{qd.year():04d}-{qd.month():02d}-{qd.day():02d}",
            # quick reminders keep their seconds unless the time was changed here
            "time": old_time if old_time[:5] == hhmm else hhmm,
            "monthday": self.f_monthday.value(),
            "every_days": self.f_every_days.value(),
            "days": [i for i, cb in enumerate(self.f_days) if cb.isChecked()],
            "every_minutes": self.f_every.value(),
            "window": self.f_window.isChecked(),
            "from": f"{self.f_from.time().hour():02d}:{self.f_from.time().minute():02d}",
            "to": f"{self.f_to.time().hour():02d}:{self.f_to.time().minute():02d}",
            "startup_delay": self.f_delay.value(),
        }
        r["actions"] = {
            "popup": self.f_popup.isChecked(),
            "position": self.f_position.currentData(),
            "toast": self.f_toast.isChecked(),
            "sound": self.f_sound.currentData(),
            "sound_file": self.f_sound_file.text().strip(),
            "sound_repeat": self.f_repeat.value(),
            "theme": self.f_theme.currentData() or "",
        }

    def _field_changed(self, *_):
        if self._loading or self._current is None:
            return
        self._store_current()
        item = self.list.item(self._current)
        if item:
            r = self._items[self._current]
            item.setText(f"{'●' if r.get('enabled', True) else '○'}  {r.get('title') or '(untitled)'}\n      {describe(r)}")
        self.changed.emit()

    def _type_changed(self, *_):
        kind = self.f_type.currentData()
        self.form.setRowVisible(self.row_date, kind in ("once", "every_days"))
        self.form.setRowVisible(self.row_time, kind in ("once", "daily", "weekly", "monthly", "every_days"))
        self.form.setRowVisible(self.row_monthday, kind == "monthly")
        self.form.setRowVisible(self.row_every_days, kind == "every_days")
        self.form.setRowVisible(self.row_days, kind == "weekly")
        self.form.setRowVisible(self.row_every, kind == "interval")
        self.form.setRowVisible(self.row_window, kind == "interval")
        self.form.setRowVisible(self.row_delay, kind == "startup")
        self._field_changed()

    def _sound_changed(self, *_):
        self.form.setRowVisible(self.row_sound_file, self.f_sound.currentData() == "custom")

    def _browse_sound(self):
        path, _ = QFileDialog.getOpenFileName(self, "Choose a sound", "", AUDIO_FILTER)
        if path:
            self.f_sound_file.setText(path)

    def _add(self):
        self._store_current()
        self._items.append(new_reminder())
        self._refresh_list()
        self.list.setCurrentRow(len(self._items) - 1)
        self.f_title.setFocus()
        self.f_title.selectAll()
        self.changed.emit()

    def _duplicate(self):
        if self._current is None:
            return
        self._store_current()
        copy_ = json.loads(json.dumps(self._items[self._current]))
        copy_["id"] = uuid.uuid4().hex
        copy_["title"] += " (copy)"
        self._items.insert(self._current + 1, copy_)
        self._refresh_list()
        self.list.setCurrentRow(self._current + 1)
        self.changed.emit()

    def _delete(self):
        if self._current is None:
            return
        idx = self._current
        self._current = None
        self._items.pop(idx)
        self._refresh_list()
        self.list.setCurrentRow(min(idx, len(self._items) - 1))
        if not self._items:
            self.editor.setEnabled(False)
        self.changed.emit()

    def _test(self):
        if self._current is None:
            return
        self._store_current()
        if self._preview:
            self._preview(self._items[self._current])
