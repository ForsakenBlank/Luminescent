"""
Hotkey cheat sheet: every hotkey, grouped by module, over a dimmed screen.

Hold mode (default): it stays up while the hotkey's keys are held (polled every
50 ms, only while visible) and disappears when you let go.  Toggle mode: press
again or Esc.  Modules that are switched off are greyed out.
"""
from PyQt6.QtWidgets import QApplication, QWidget, QLabel, QVBoxLayout, QHBoxLayout, QGridLayout, QFrame
from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtGui import QPainter, QColor

from core import theme, widgets
from core import hotkeys as hk
from core.settings import SCHEMA

_FLAGS = Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool


class _Sheet(QWidget):
    def __init__(self, geom, groups: list[tuple[str, bool, list[tuple[str, str]]]] | None, toggle: bool = False):
        super().__init__()
        self.setWindowFlags(_FLAGS)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setGeometry(geom)
        if groups is None:
            return
        outer = QVBoxLayout(self)
        side = 140 if geom.width() >= 1400 else 60
        outer.setContentsMargins(side, 80, side, 60)
        outer.setSpacing(36)
        head = QHBoxLayout()
        head.setSpacing(16)
        mark = QLabel()
        mark.setPixmap(theme.sparkle_pixmap(16))
        head.addWidget(mark)
        head.addWidget(widgets.label("HOTKEYS", 13, 600, mono=True, spacing=0.28))
        head.addWidget(widgets.rule(), 1, Qt.AlignmentFlag.AlignVCenter)
        head.addWidget(widgets.label("PRESS AGAIN OR ESC TO CLOSE" if toggle else "RELEASE TO CLOSE",
                                     11, mono=True, spacing=0.06, tone="muted"))
        outer.addLayout(head)
        grid = QGridLayout()
        grid.setSpacing(14)
        cols = 4 if geom.width() >= 1400 else 3
        for c in range(cols):
            grid.setColumnStretch(c, 1)
        for i, (name, on, rows) in enumerate(groups):
            box = QFrame()
            box.setObjectName("cheatCard")
            bl = QVBoxLayout(box)
            bl.setContentsMargins(18, 16, 18, 16)
            bl.setSpacing(12)
            h = QHBoxLayout()
            h.setSpacing(8)
            tick = QFrame()
            tick.setFixedSize(3, 11)
            tick.setStyleSheet(f"background: {theme.P.primary if on else theme.P.text_muted}; border-radius: 1px;")
            h.addWidget(tick)
            h.addWidget(widgets.label(name.upper() + ("" if on else "  (OFF)"), 10.5, 600, mono=True, spacing=0.14,
                                      tone="primary" if on else "muted"))
            h.addStretch()
            bl.addLayout(h)
            keys = QVBoxLayout()
            keys.setSpacing(8)
            for label, combo in rows:
                r = QHBoxLayout()
                r.setSpacing(10)
                l1 = widgets.label(label, 13, tone="secondary" if on else "muted")
                l1.setWordWrap(True)
                r.addWidget(l1, 1)
                r.addWidget(widgets.label(hk.pretty(combo), 11, mono=True, tone="primary" if on else "muted"))
                keys.addLayout(r)
            bl.addLayout(keys)
            bl.addStretch()
            grid.addWidget(box, i // cols, i % cols)
        outer.addLayout(grid)
        outer.addStretch()

    def paintEvent(self, e):
        p = QPainter(self)
        r, g, b = theme.scrim_rgb()
        p.fillRect(self.rect(), QColor(r, g, b, int(0.64 * 255)))
        base = QColor(theme.P.bg_base)
        base.setAlphaF(0.55)
        p.fillRect(self.rect(), base)
        p.end()


class CheatSheet:
    def __init__(self, launcher):
        self.launcher = launcher
        self._windows: list[_Sheet] = []
        self._combo = ""
        self._poll = QTimer()
        self._poll.setInterval(50)
        self._poll.timeout.connect(self._check)

    def groups(self):
        L = self.launcher
        out = []
        for s in SCHEMA:
            sec = L.settings.section(s.key)
            rows = [(f.label, sec.get(f.key)) for f in s.fields if f.type == "hotkey" and sec.get(f.key)]
            if s.key == "doodle" and sec.get("mode", "hold") == "hold":
                rows = [("Hold to draw" if lbl == "Draw hotkey" else lbl, c) for lbl, c in rows]
            if rows:
                on = sec.get("enabled", True) and (s.key not in L.modules or L.modules[s.key].running)
                out.append((s.title, bool(on), rows))
        dash = [(it["name"], it["hotkey"]) for t in L.dashboard.tabs() for it in t["items"] if it.get("hotkey")]
        if dash:
            out.append(("Dashboard items", True, dash))
        return out

    def trigger(self, combo: str):
        """The hotkey was pressed."""
        if self._windows:
            if getattr(self, "_toggle_mode", False):
                self.hide()
            return
        self._combo = combo
        # hold mode needs to see the keys go up; if Hyprland can't report that, press again / Esc closes it
        toggle_mode = bool(self.launcher.settings.get("ui.cheatsheet_toggle", False)) or hk.key_state(combo) is None
        self._toggle_mode = toggle_mode
        groups = self.groups()
        prim = QApplication.primaryScreen()
        for scr in QApplication.screens():
            w = _Sheet(scr.geometry(), groups if scr is prim else None, toggle_mode)
            w.show()
            w.raise_()
            self._windows.append(w)
        if toggle_mode:
            for w in self._windows:
                w.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating, False)
                w.mousePressEvent = lambda e: self.hide()
            widgets.bring_to_front(self._windows[0])
            self._windows[0].keyPressEvent = lambda e: self.hide() if e.key() == Qt.Key.Key_Escape else None
        else:
            self._poll.start()

    def _check(self):
        if not hk.key_is_physically_down(self._combo):
            self.hide()

    def hide(self):
        self._poll.stop()
        for w in self._windows:
            w.close()
            w.deleteLater()
        self._windows = []
