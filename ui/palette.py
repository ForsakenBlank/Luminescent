"""
Command palette / Quick Action bar: one search box for everything.

The index (module actions, dashboard items, notes, settings pages, themes,
calculator pins, recent colours...) is rebuilt each time the palette opens, so
it is never stale and costs nothing while closed.  Matching is a fuzzy
subsequence match with bonuses for word starts and runs; no library, well
under a millisecond per keystroke for a few hundred entries.  Typing a sum
shows its answer as the first result.
"""
import os
import logging
from datetime import date

from PyQt6.QtWidgets import (
    QWidget, QFrame, QLabel, QVBoxLayout, QHBoxLayout, QLineEdit, QListWidget, QListWidgetItem,
    QStyledItemDelegate, QStyle
)
from PyQt6.QtCore import Qt, QSize, QTimer, QRect, QRectF, QEvent
from PyQt6.QtGui import QColor, QPainter, QPen

from core import DATA_DIR, theme, widgets, save_json, load_json, clip, anim

log = logging.getLogger("palette")

RECENT_PATH = os.path.join(DATA_DIR, "palette.json")
MAX_SHOWN = 60


class Entry:
    __slots__ = ("id", "label", "hint", "kind", "fn", "low", "starts")

    def __init__(self, eid: str, label: str, fn, hint: str = "", kind: str = ""):
        self.id, self.label, self.hint, self.kind, self.fn = eid, label, hint, kind, fn
        self.low = f"{label} {hint}".lower()
        self.starts = {0} | {i + 1 for i, c in enumerate(self.low) if not c.isalnum()}


def score(q: str, e: Entry) -> float | None:
    """Higher is better; None = no match. `q` is lower-case without spaces."""
    text, pos, total, prev = e.low, 0, 0.0, -2
    for ch in q:
        i = text.find(ch, pos)
        if i < 0:
            return None
        if i in e.starts:
            total += 8
        if i == prev + 1:
            total += 5
        total -= min(i - pos, 6) * 0.5
        prev, pos = i, i + 1
    if text.startswith(q):
        total += 20
    elif q in text:
        total += 10
    return total - len(text) * 0.02


_GROUPS = {"page": "OVERLAY", "theme": "THEME", "settings": "SETTINGS", "luminescent": "LUMINESCENT", "calc": "CALC",
           "note": "NOTES", "colour": "COLOUR", "pin": "CALCULATOR", "reminder": "REMIND"}


def columns(e: Entry) -> tuple[str, str]:
    """(group shown on the left in mono caps, keys shown on the right) for a row."""
    parts = [x.strip() for x in (e.hint or "").split(" · ")]
    if e.kind in ("action", "item"):
        return parts[0].upper(), (parts[1] if len(parts) > 1 else "")
    if e.kind == "theme":
        return "THEME", ("ACTIVE" if e.hint == "active" else "")
    if e.kind == "calc":
        return "CALC", "ENTER COPIES"
    if e.kind == "reminder":
        return "REMIND", (parts[1] if len(parts) > 1 else "")
    return _GROUPS.get(e.kind, (parts[0] or e.kind).upper()), (parts[1] if len(parts) > 1 else "")


class _Delegate(QStyledItemDelegate):
    ROW_H = 40
    GROUP_W = 76

    def sizeHint(self, option, index):
        return QSize(100, self.ROW_H)

    def paint(self, p: QPainter, option, index):
        e: Entry = index.data(Qt.ItemDataRole.UserRole)
        if e is None:
            return
        pal = theme.P
        r = option.rect
        p.save()
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        if option.state & QStyle.StateFlag.State_Selected:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(pal.primary_muted))
            rad = max(0, pal.radius - 4)
            p.drawRoundedRect(QRectF(r), rad, rad)
            p.setBrush(QColor(pal.primary))
            p.drawRoundedRect(QRectF(r.left(), r.top() + 10, 2, r.height() - 20), 1, 1)
        group, keys = columns(e)
        x = r.left() + 14
        p.setFont(theme.qfont(9, 500, mono=True, spacing=0.08))
        p.setPen(QColor(pal.text_muted))
        p.drawText(QRect(x, r.top(), self.GROUP_W, r.height()), Qt.AlignmentFlag.AlignVCenter,
                   p.fontMetrics().elidedText(group, Qt.TextElideMode.ElideRight, self.GROUP_W - 6))
        right = 0
        if keys:
            p.setFont(theme.qfont(11, mono=True))
            right = min(170, p.fontMetrics().horizontalAdvance(keys) + 4)
            p.drawText(QRect(r.right() - 14 - right, r.top(), right, r.height()),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                       p.fontMetrics().elidedText(keys, Qt.TextElideMode.ElideLeft, right))
        lx = x + self.GROUP_W + 12
        p.setFont(theme.qfont(13.5))
        p.setPen(QColor(pal.text_primary))
        avail = r.right() - 14 - right - 16 - lx
        p.drawText(QRect(lx, r.top(), max(10, avail), r.height()), Qt.AlignmentFlag.AlignVCenter,
                   p.fontMetrics().elidedText(e.label, Qt.TextElideMode.ElideRight, max(10, avail)))
        p.restore()


class CommandPalette(QWidget):
    GLOW = 36           # transparent margin around the box for the painted glow

    def __init__(self, launcher):
        super().__init__()
        self.launcher = launcher
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        g = self.GLOW
        self.setFixedWidth(600 + 2 * g)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(g, g, g, g)
        self.box = QFrame()
        self.box.setObjectName("paletteBox")
        outer.addWidget(self.box)
        lay = QVBoxLayout(self.box)
        lay.setContentsMargins(1, 1, 1, 1)
        lay.setSpacing(0)

        head = QFrame()
        head.setObjectName("paletteHead")
        head.setFixedHeight(56)
        hl = QHBoxLayout(head)
        hl.setContentsMargins(18, 0, 18, 0)
        hl.setSpacing(12)
        mark = QLabel()
        mark.setPixmap(theme.sparkle_pixmap(13))
        hl.addWidget(mark)
        self.input = QLineEdit()
        self.input.setObjectName("paletteInput")
        self.input.setPlaceholderText("Search everything…  type a sum, 5 km to mi, or remind me in 20m to …")
        self.input.textChanged.connect(self._filter)
        self.input.installEventFilter(self)
        hl.addWidget(self.input, 1)
        self.count = widgets.label("", 10, mono=True, spacing=0.06, tone="muted")
        hl.addWidget(self.count)
        tick = QFrame(head)
        tick.setStyleSheet(f"background: {theme.P.primary};")
        tick.setGeometry(0, 55, 56, 1)
        lay.addWidget(head)

        self.list = QListWidget()
        self.list.setObjectName("paletteList")
        self.list.setItemDelegate(_Delegate(self.list))
        self.list.setUniformItemSizes(True)
        self.list.setSpacing(0)
        self.list.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.list.itemActivated.connect(lambda it: self._run(it.data(Qt.ItemDataRole.UserRole)))
        self.list.itemClicked.connect(lambda it: self._run(it.data(Qt.ItemDataRole.UserRole)))
        self.list.setFixedHeight(_Delegate.ROW_H * 9 + 12)
        self.list.setViewportMargins(0, 0, 0, 0)
        holder = QWidget()
        ll = QVBoxLayout(holder)
        ll.setContentsMargins(6, 6, 6, 6)
        ll.addWidget(self.list)
        lay.addWidget(holder)

        foot = QFrame()
        foot.setObjectName("paletteFoot")
        foot.setFixedHeight(34)
        fl = QHBoxLayout(foot)
        fl.setContentsMargins(18, 0, 18, 0)
        fl.setSpacing(18)
        for text in ("↑↓ MOVE", "ENTER RUN", "TAB FILL"):
            fl.addWidget(widgets.label(text, 10, mono=True, spacing=0.04, tone="muted"))
        fl.addStretch()
        fl.addWidget(widgets.label("ESC CLOSE", 10, mono=True, spacing=0.04, tone="muted"))
        lay.addWidget(foot)
        self.entries: list[Entry] = []
        self.recent: list[str] = []
        self._shown = False

    def paintEvent(self, e):
        if self.launcher.settings.get("ui.performance_mode", False) or not theme.P.glow:
            return
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r = QRectF(self.box.geometry())
        rad = theme.P.radius
        c = QColor(theme.P.primary)
        p.setBrush(Qt.BrushStyle.NoBrush)
        for i in range(1, 12):
            c.setAlphaF(theme.glow_alpha() * 0.22 * (1 - i / 12) ** 2)
            p.setPen(QPen(c, 3))
            d = i * 3
            p.drawRoundedRect(r.adjusted(-d, -d, d, d), rad + d, rad + d)
        c.setAlphaF(0.16)
        p.setPen(QPen(c, 1))
        p.drawRoundedRect(r.adjusted(-1.5, -1.5, 1.5, 1.5), rad + 1.5, rad + 1.5)
        p.end()

    # ── opening ────────────────────────────────────────────────────────────
    def toggle(self, text: str = ""):
        if self.isVisible() and not text:
            self.hide()
        else:
            self.open(text)

    def open(self, text: str = ""):
        try:
            self.entries = self.launcher.palette_entries()
        except Exception:
            log.exception("could not build the palette index")
            self.entries = []
        data = load_json(RECENT_PATH, {})
        self.recent = [r for r in (data.get("recent") if isinstance(data, dict) else []) or [] if isinstance(r, str)]
        self.input.clear()
        self._filter("")
        if text:
            self.input.setText(text)          # typed on the overlay: carry on searching from there
        self.adjustSize()
        g = widgets.cursor_screen_geometry()
        self.move(g.center().x() - self.width() // 2, g.top() + int(g.height() * 0.18) - self.GLOW)
        self.show()
        self.raise_()
        anim.fade(self.box, 0.0, 1.0, "panels", 140)
        widgets.bring_to_front(self)
        self.input.setFocus()
        self.input.end(False)
        self._shown = True

    def changeEvent(self, e):
        if e.type() == QEvent.Type.ActivationChange and self._shown and not self.isActiveWindow():
            QTimer.singleShot(0, lambda: self.hide() if not self.isActiveWindow() else None)
        super().changeEvent(e)

    def hideEvent(self, e):
        self._shown = False
        super().hideEvent(e)

    # ── searching ──────────────────────────────────────────────────────────
    def _filter(self, text: str):
        q = "".join(text.lower().split())
        results: list[Entry] = []
        calc = self._calc_entry(text)
        if calc:
            results.append(calc)
        rem = self._reminder_entry(text)
        if rem:
            results.append(rem)
        if not q:
            by_id = {e.id: e for e in self.entries}
            results += [by_id[r] for r in self.recent if r in by_id]
            seen = {e.id for e in results}
            results += [e for e in self.entries if e.id not in seen]
        else:
            rank = {r: len(self.recent) - i for i, r in enumerate(self.recent)}
            scored = []
            for e in self.entries:
                s = score(q, e)
                if s is not None:
                    scored.append((s + rank.get(e.id, 0) * 0.8, e))
            scored.sort(key=lambda x: -x[0])
            results += [e for _, e in scored[:MAX_SHOWN]]
        self.list.clear()
        for e in results[:MAX_SHOWN]:
            it = QListWidgetItem()
            it.setData(Qt.ItemDataRole.UserRole, e)
            self.list.addItem(it)
        if self.list.count():
            self.list.setCurrentRow(0)
        n = len(results)
        self.count.setText(f"{min(n, MAX_SHOWN)}{'+' if n > MAX_SHOWN else ''} RESULT{'' if n == 1 else 'S'}")

    def _calc_entry(self, text: str) -> Entry | None:
        text = text.strip()
        if not text or not any(c.isdigit() for c in text) or not any(c in "+-*/^%()" for c in text[1:]):
            return None
        calc = self.launcher.modules.get("calculator")
        try:
            from modules.calculator import evaluate, fmt
            value = evaluate(text, calc.variables() if calc else {})
        except Exception:
            return None
        shown = fmt(value)

        def copy():
            clip.set_text(shown.replace(",", ""))
            if calc and calc.running:
                calc.add_history(text, value)
            self.launcher.notify("Calculator", f"{text} = {shown}  (copied)", 2500)
        return Entry("calc:result", f"= {shown}", copy, f"{text} · Enter copies", "calc")

    def _reminder_entry(self, text: str) -> Entry | None:
        """"remind me in 20m to stretch", "remind me at 17:30 to call mum", "in 2h tea"."""
        m = self.launcher.modules.get("reminders")
        if m is None or not m.running:
            return None
        from modules.reminders import parse_quick
        parsed = parse_quick(text)
        if parsed is None:
            return None
        when, title = parsed

        def add():
            m.add_quick(title, when)
            self.launcher.notify("Reminders", f"{title} — {when:%H:%M}" +
                                 ("" if when.date() == date.today() else f" on {when:%a %d %b}"),
                                 3000)
        day = "" if when.date() == date.today() else f"{when:%a %d %b} "
        return Entry("remind:quick", f"Remind me: {title}", add, f"REMINDER · {day}{when:%H:%M}", "reminder")

    def eventFilter(self, obj, e):
        if obj is self.input and e.type() == QEvent.Type.KeyPress:
            k = e.key()
            if k == Qt.Key.Key_Escape:
                self.hide()
                return True
            if k in (Qt.Key.Key_Down, Qt.Key.Key_Up, Qt.Key.Key_PageDown, Qt.Key.Key_PageUp):
                step = {Qt.Key.Key_Down: 1, Qt.Key.Key_Up: -1, Qt.Key.Key_PageDown: 8, Qt.Key.Key_PageUp: -8}[k]
                n = self.list.count()
                if n:
                    self.list.setCurrentRow(max(0, min(n - 1, self.list.currentRow() + step)))
                return True
            if k == Qt.Key.Key_Tab:
                it = self.list.currentItem()
                e_ = it.data(Qt.ItemDataRole.UserRole) if it else None
                if e_ is not None and not e_.id.startswith("calc:"):
                    self.input.setText(e_.label)
                return True
            if k in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
                it = self.list.currentItem()
                if it:
                    self._run(it.data(Qt.ItemDataRole.UserRole))
                return True
        return super().eventFilter(obj, e)

    def _run(self, e: Entry | None):
        if e is None:
            return
        self.hide()
        if not e.id.startswith(("calc:", "remind:")):
            from core import usage
            usage.record("tools", "palette:" + e.id.split(":")[0], "Command palette")
            self.recent = [e.id] + [r for r in self.recent if r != e.id][:19]
            try:
                save_json(RECENT_PATH, {"recent": self.recent})
            except OSError:
                pass
        QTimer.singleShot(60, e.fn)
