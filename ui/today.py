"""
Today page (the focus log): what you worked on today, from the activity log in core/usage.

Dashboard items you ran, Luminescent tools you used and the time timers and stopwatches ran, in order,
with a summary on top. ‹ and › step to earlier days.
"""
from collections import Counter
from datetime import date, datetime, timedelta

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QScrollArea, QFrame, QSizePolicy
from PyQt6.QtCore import Qt

from core import theme, widgets, usage
from core.timeparse import fmt_secs

KINDS = {"run": ("RUN", "Dashboard"), "tool": ("TOOL", "Tool"), "timer": ("TIMER", "Timer")}


class TodayPage(QWidget):
    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        self.day = date.today()
        lay = QVBoxLayout(self)
        lay.setContentsMargins(24, 12, 24, 24)
        lay.setSpacing(14)
        head = QHBoxLayout()
        self.title = widgets.label("Today", 15, 600)
        head.addWidget(self.title)
        self.date_lbl = widgets.label("", 10, 500, mono=True, spacing=0.06, tone="muted")
        head.addWidget(self.date_lbl)
        head.addStretch()
        prev = widgets.icon_button("‹", "The day before", 30, 14)
        prev.clicked.connect(lambda: self._step(-1))
        self.next_btn = widgets.icon_button("›", "The day after", 30, 14)
        self.next_btn.clicked.connect(lambda: self._step(1))
        head.addWidget(prev)
        head.addWidget(self.next_btn)
        lay.addLayout(head)
        self.summary = QHBoxLayout()
        self.summary.setSpacing(12)
        lay.addLayout(self.summary)
        body = QHBoxLayout()
        body.setSpacing(20)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        self.timeline = QVBoxLayout(holder)
        self.timeline.setContentsMargins(0, 0, 8, 0)
        self.timeline.setSpacing(2)
        scroll.setWidget(holder)
        body.addWidget(scroll, 3)
        side = QVBoxLayout()
        side.setSpacing(6)
        side.addWidget(widgets.label("MOST USED", 9.5, 600, mono=True, spacing=0.12, tone="muted"))
        self.top = QVBoxLayout()
        self.top.setSpacing(4)
        side.addLayout(self.top)
        side.addStretch()
        sw = QWidget()
        sw.setLayout(side)
        sw.setFixedWidth(300)
        body.addWidget(sw)
        lay.addLayout(body, 1)

    def state(self) -> dict:
        return {"days_back": (date.today() - self.day).days}

    def restore_state(self, st: dict):
        back = int(st.get("days_back") or 0)
        if back:
            self._step(-back)

    def _step(self, days: int):
        self.day = min(date.today(), self.day + timedelta(days=days))
        self.refresh()

    def _chip(self, value: str, label: str) -> QFrame:
        p = theme.P
        f = QFrame()
        f.setObjectName("todayChip")
        f.setStyleSheet(f"#todayChip {{ background: {p.bg_card}; border: 1px solid {p.border};"
                        f" border-radius: {p.radius}px; }}")
        v = QVBoxLayout(f)
        v.setContentsMargins(16, 10, 16, 10)
        v.setSpacing(2)
        v.addWidget(widgets.label(value, 20, 600, mono=True))
        v.addWidget(widgets.label(label.upper(), 9, 600, mono=True, spacing=0.12, tone="muted"))
        return f

    def refresh(self):
        today = self.day == date.today()
        self.title.setText("Today" if today else ("Yesterday" if self.day == date.today() - timedelta(days=1)
                                                  else self.day.strftime("%A")))
        self.date_lbl.setText(f"{self.day:%d %B %Y}".upper().lstrip("0"))
        self.next_btn.setEnabled(not today)
        entries = usage.day_activity(self.day)
        runs = [e for e in entries if e.get("kind") == "run"]
        tools = [e for e in entries if e.get("kind") == "tool"]
        timed = sum(float(e.get("secs") or 0) for e in entries if e.get("kind") == "timer")
        widgets.clear_layout(self.summary)
        first = entries[0]["t"][11:16] if entries else "-"
        last = entries[-1]["t"][11:16] if entries else "-"
        for value, label in ((str(len(runs)), "things run"), (fmt_secs(timed) if timed else "0m", "on timers"),
                             (str(len(tools)), "tools used"), (f"{first} to {last}" if entries else "-", "first to last")):
            self.summary.addWidget(self._chip(value, label))
        self.summary.addStretch()

        widgets.clear_layout(self.timeline)
        p = theme.P
        if not entries:
            msg = ("Nothing yet today. Run something from the Dashboard or start a timer and it shows up here."
                   if today else "Nothing was logged that day.")
            empty = widgets.label(msg, 13, tone="muted")
            empty.setWordWrap(True)
            self.timeline.addWidget(empty)
        last_hour = None
        for e in entries:
            try:
                t = datetime.fromisoformat(e["t"])
            except (KeyError, ValueError):
                continue
            if t.hour != last_hour:
                last_hour = t.hour
                h = widgets.label(f"{t.hour:02d}:00", 9.5, 600, mono=True, spacing=0.12, tone="muted")
                h.setContentsMargins(0, 10, 0, 2)
                self.timeline.addWidget(h)
            row = QFrame()
            row.setObjectName("todayRow")
            row.setStyleSheet(f"#todayRow {{ border: none; border-radius: 6px; }}"
                              f"#todayRow:hover {{ background: {p.bg_card_hover}; }}")
            rl = QHBoxLayout(row)
            rl.setContentsMargins(8, 5, 8, 5)
            rl.setSpacing(12)
            rl.addWidget(widgets.label(t.strftime("%H:%M"), 11, mono=True, tone="muted"))
            tag, _ = KINDS.get(e.get("kind"), ((e.get("kind") or "").upper(), ""))
            badge = widgets.label(tag, 9, 600, mono=True, spacing=0.08, tone="")
            badge.setFixedWidth(54)
            badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
            badge.setStyleSheet(f"background: {p.primary_muted}; color: {p.primary_text}; border-radius: 4px;"
                                f" padding: 2px 0;")
            rl.addWidget(badge)
            name = widgets.label(e.get("label", ""), 13)
            name.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
            rl.addWidget(name, 1)
            if e.get("secs"):
                rl.addWidget(widgets.label(fmt_secs(float(e["secs"])), 11, mono=True, tone="secondary"))
            self.timeline.addWidget(row)
        self.timeline.addStretch()

        widgets.clear_layout(self.top)
        counts = Counter((e.get("kind"), e.get("label", "")) for e in entries if e.get("kind") in ("run", "tool"))
        for (kind, label), n in counts.most_common(8):
            r = QHBoxLayout()
            nm = widgets.label(label, 12)
            nm.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
            r.addWidget(nm, 1)
            r.addWidget(widgets.label(f"×{n}", 11, mono=True, tone="muted"))
            self.top.addLayout(r)
        if not counts:
            self.top.addWidget(widgets.label("-", 12, tone="muted"))

    def on_show(self):
        usage.flush()
        self.refresh()
