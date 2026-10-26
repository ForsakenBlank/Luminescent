"""
Usage page: what you use most. Dashboard items, hotkeys and Luminescent tools, each with how often and
when last, plus a bar per day for the last 30 days. The counts come from data/usage.json (core/usage);
Reset clears them.
"""
from datetime import date, datetime, timedelta

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QFrame, QScrollArea, QSizePolicy
from PyQt6.QtCore import Qt, QRectF, QPointF
from PyQt6.QtGui import QPainter, QColor, QPen

from core import theme, widgets, usage
from core.hotkeys import pretty


def _ago(iso: str) -> str:
    try:
        t = datetime.fromisoformat(iso)
    except (TypeError, ValueError):
        return ""
    s = max(0, int((datetime.now() - t).total_seconds()))
    if s < 3600:
        return f"{max(1, s // 60)}m ago"
    if s < 86400:
        return f"{s // 3600}h ago"
    if s < 2 * 86400:
        return "yesterday"
    return f"{s // 86400}d ago"


class DaysChart(QWidget):
    """Uses per day for the last 30 days."""

    def __init__(self):
        super().__init__()
        self.setFixedHeight(90)
        self.days: list[tuple[date, int]] = []

    def set_data(self, counts: dict):
        today = date.today()
        self.days = [(today - timedelta(days=b), int(counts.get((today - timedelta(days=b)).isoformat(), 0)))
                     for b in range(29, -1, -1)]
        self.update()

    def paintEvent(self, e):
        p = theme.P
        qp = QPainter(self)
        qp.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.rect()).adjusted(0, 4, 0, -16)
        qp.setPen(QPen(QColor(p.border), 1))
        qp.drawLine(QPointF(r.left(), r.bottom() + 0.5), QPointF(r.right(), r.bottom() + 0.5))
        top = max([n for _d, n in self.days] + [1])
        n = max(1, len(self.days))
        gap = 4.0
        w = (r.width() - gap * (n - 1)) / n
        qp.setFont(theme.qfont(9, mono=True))
        for i, (d, count) in enumerate(self.days):
            x = r.left() + i * (w + gap)
            if count:
                h = max(2.0, count / top * r.height())
                qp.setPen(Qt.PenStyle.NoPen)
                qp.setBrush(QColor(p.primary if d != date.today() else p.primary_text))
                qp.drawRoundedRect(QRectF(x, r.bottom() - h, w, h), 2, 2)
            if i % 7 == 2 or d == date.today():
                qp.setPen(QColor(p.text_muted))
                qp.drawText(QRectF(x - 20, r.bottom() + 3, w + 40, 12), Qt.AlignmentFlag.AlignCenter,
                            "TODAY" if d == date.today() else f"{d.day} {d:%b}".upper())
        qp.end()


class StatsPage(QWidget):
    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        lay = QVBoxLayout(self)
        lay.setContentsMargins(24, 12, 24, 24)
        lay.setSpacing(14)
        head = QHBoxLayout()
        head.addWidget(widgets.label("Usage", 15, 600))
        self.total = widgets.label("", 10, 500, mono=True, spacing=0.06, tone="muted")
        head.addWidget(self.total)
        head.addStretch()
        reset = widgets.button("Reset…", "ghost", "Start counting from zero")
        reset.clicked.connect(self._reset)
        head.addWidget(reset)
        lay.addLayout(head)
        lay.addWidget(widgets.label("LAST 30 DAYS", 9.5, 600, mono=True, spacing=0.12, tone="muted"))
        self.chart = DaysChart()
        lay.addWidget(self.chart)
        cols = QHBoxLayout()
        cols.setSpacing(20)
        self.columns = {}
        for key, title in (("items", "DASHBOARD ITEMS"), ("hotkeys", "HOTKEYS"), ("tools", "TOOLS")):
            box = QVBoxLayout()
            box.setSpacing(6)
            box.addWidget(widgets.label(title, 9.5, 600, mono=True, spacing=0.12, tone="muted"))
            scroll = QScrollArea()
            scroll.setWidgetResizable(True)
            scroll.setFrameShape(QFrame.Shape.NoFrame)
            holder = QWidget()
            widgets.style(holder, "background: transparent;")
            rows = QVBoxLayout(holder)
            rows.setContentsMargins(0, 0, 6, 0)
            rows.setSpacing(4)
            scroll.setWidget(holder)
            box.addWidget(scroll, 1)
            cols.addLayout(box, 1)
            self.columns[key] = rows
        lay.addLayout(cols, 1)

    def _reset(self):
        if widgets.confirm(self, "Reset usage", "Forget every count (the Today log stays)?", yes="Reset", danger=True):
            usage.reset()
            self.refresh()

    def refresh(self):
        d = usage.stats()
        self.chart.set_data(d.get("days", {}))
        total = sum(int(v) for v in d.get("days", {}).values())
        self.total.setText(f"{total} USES COUNTED" if total else "")
        p = theme.P
        for key, rows in self.columns.items():
            widgets.clear_layout(rows)
            entries = sorted((d.get(key) or {}).items(), key=lambda kv: -int(kv[1].get("n", 0)))[:25]
            top = max([int(v.get("n", 0)) for _k, v in entries] + [1])
            for k, v in entries:
                label = v.get("label") or k
                if key == "hotkeys":
                    label = f"{pretty(k)}  ·  {label.split(': ', 1)[-1]}"
                row = QWidget()
                rl = QVBoxLayout(row)
                rl.setContentsMargins(0, 2, 0, 4)
                rl.setSpacing(3)
                line = QHBoxLayout()
                nm = widgets.label(label, 12)
                nm.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
                nm.setToolTip(label)
                line.addWidget(nm, 1)
                line.addWidget(widgets.label(f"{v.get('n', 0)}×  {_ago(v.get('last', ''))}", 10, mono=True,
                                             tone="muted"))
                rl.addLayout(line)
                bar = QFrame()
                bar.setFixedHeight(3)
                bar.setStyleSheet(f"background: {p.bg_frame}; border-radius: 1px;")
                fill = QFrame(bar)
                fill.setStyleSheet(f"background: {p.primary}; border-radius: 1px;")
                fill.setFixedHeight(3)
                fill.setProperty("frac", int(v.get("n", 0)) / top)
                rl.addWidget(bar)
                bar.resizeEvent = lambda e, f=fill: f.setFixedWidth(int(e.size().width() * f.property("frac")))
                rows.addWidget(row)
            if not entries:
                rows.addWidget(widgets.label("Nothing counted yet.", 12, tone="muted"))
            rows.addStretch()

    def on_show(self):
        usage.flush()
        self.refresh()
