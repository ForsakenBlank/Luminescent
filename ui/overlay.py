"""
The Luminescent overlay: a full-screen hub, the way game bars do it.

One frameless, always-on-top, translucent window per screen. Every screen is
dimmed and dotted edge to edge. The primary one also carries the chrome: the
logo top-left, the page switcher hanging from the top-centre edge (Dashboard ·
Themes · Clipboard · Settings, order set in Settings) and the round ✕ top-right.
The Dashboard's tab boxes float straight on the screen; the other pages sit in
a centred panel below the switcher.  ✕ / Esc / the hotkey only hide it: the
process keeps running so it reopens instantly.  Pages are built the first time
they are shown.

Performance: nothing runs while it is hidden. The dim, dots, background image
and glow are painted once into a cached pixmap per screen (a QGraphicsEffect
would re-render every child on every repaint), and performance mode drops the
glow, the background image and the fade.
"""
import os
import logging

from datetime import datetime

from PyQt6.QtWidgets import (
    QApplication, QWidget, QFrame, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QStackedWidget, QButtonGroup,
    QLineEdit, QTextEdit, QPlainTextEdit, QAbstractSpinBox, QComboBox
)
from PyQt6.QtCore import Qt, QRect, QRectF, QPointF, QTimer, QPropertyAnimation, QEasingCurve, QEvent, QObject, pyqtSignal
from PyQt6.QtGui import QPainter, QColor, QPixmap, QPen, QRadialGradient

from core import hypr, theme, widgets, events, APP_NAME
from core.settings import NAV_SECTIONS, ALL_PAGES, REQUIRED_PAGES

log = logging.getLogger("overlay")

_FLAGS = Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool
DOT_STEP = 20
PANEL_TOP = 96          # panel pages start below the docked switcher
FULL_PAGES = ("dashboard", "automation")    # pages that fill the screen instead of sitting in the panel


class _Backdrop(QWidget):
    """A full-screen dim with the dot grid (+ background image, glow and chrome on the primary screen)."""
    clicked_outside = pyqtSignal()
    escape = pyqtSignal()

    def __init__(self, geom: QRect, dim: float, blur: bool = False, screen_name: str = ""):
        super().__init__()
        self.screen_name = screen_name
        self.setWindowFlags(_FLAGS)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setProperty("lum_no_pin", True)      # so apps opened from the overlay can be drawn above it
        self.setProperty("lum_blur", blur)        # Hyprland blurs what is behind it (window rule on the title)
        self.setWindowTitle("Luminescent")
        self.setGeometry(geom)
        self.dim = dim
        self.panel: QWidget | None = None          # set while a panel page is shown
        self.glow = False
        self.primary = False
        self.image_path, self.image_opacity = "", 0.35
        self._cache: QPixmap | None = None
        self._cache_key = None
        self._pressed_outside = False

    def invalidate(self):
        self._cache_key = None
        self.update()

    def _key(self):
        pr = self.panel.geometry() if self.panel is not None and self.panel.isVisible() else None
        pr = (pr.x(), pr.y(), pr.width(), pr.height()) if pr is not None else None
        return (self.width(), self.height(), round(self.dim, 3), self.glow, self.primary, self.image_path,
                round(self.image_opacity, 2), pr, theme.P.primary, theme.P.text_primary, theme.P.radius)

    def _render(self) -> QPixmap:
        w, h = self.width(), self.height()
        out = QPixmap(w, h)
        out.fill(QColor(0, 0, 0, 0))
        p = QPainter(out)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        r, g, b = theme.scrim_rgb()
        p.fillRect(0, 0, w, h, QColor(r, g, b, int(max(0.02, min(1.0, self.dim)) * 255)))
        if self.image_path and os.path.isfile(self.image_path):
            px = QPixmap(self.image_path)
            if not px.isNull():
                px = px.scaled(self.size(), Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                               Qt.TransformationMode.SmoothTransformation)
                p.setOpacity(max(0.0, min(1.0, self.image_opacity)))
                p.drawPixmap((w - px.width()) // 2, (h - px.height()) // 2, px)
                p.setOpacity(1.0)
        # the dot grid, edge to edge
        dot = QColor(theme.P.text_primary)
        dot.setAlphaF(0.085 if not theme.is_light() else 0.12)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(dot)
        off = DOT_STEP // 2
        for y in range(off, h, DOT_STEP):
            for x in range(off, w, DOT_STEP):
                p.drawEllipse(QPointF(x, y), 1.1, 1.1)
        if self.glow and self.primary:
            # a soft halo hanging from the top-centre edge, behind the page switcher
            c = QColor(theme.P.primary)
            p.save()
            p.translate(w / 2, 0)
            p.scale(1.0, 0.4)
            grad = QRadialGradient(QPointF(0, 0), 450)
            c.setAlphaF(theme.glow_alpha())
            grad.setColorAt(0.0, c)
            c.setAlphaF(theme.glow_alpha() * 0.45)
            grad.setColorAt(0.5, c)
            c.setAlphaF(0.0)
            grad.setColorAt(1.0, c)
            p.setBrush(grad)
            p.drawEllipse(QPointF(0, 0), 450, 450)
            p.restore()
        if self.panel is not None and self.panel.isVisible():
            pr = QRectF(self.panel.geometry())
            rad = theme.P.radius
            if self.glow:
                c = QColor(theme.P.primary)
                steps = 26
                p.setBrush(Qt.BrushStyle.NoBrush)
                for i in range(1, steps + 1):
                    c.setAlphaF(theme.glow_alpha() * 0.16 * (1 - i / (steps + 1)) ** 2)
                    p.setPen(QPen(c, 6))
                    d = i * 6 - 2
                    p.drawRoundedRect(pr.adjusted(-d, -d, d, d), rad + d, rad + d)
            ring = QColor(theme.P.primary)
            ring.setAlphaF(0.16)
            p.setPen(QPen(ring, 1))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(pr.adjusted(-1.5, -1.5, 1.5, 1.5), rad + 1.5, rad + 1.5)
        p.end()
        return out

    def paintEvent(self, e):
        key = self._key()
        if self._cache is None or key != self._cache_key:
            self._cache = self._render()
            self._cache_key = key
        p = QPainter(self)
        p.setCompositionMode(QPainter.CompositionMode.CompositionMode_Source)
        p.setClipRegion(e.region())
        p.drawPixmap(0, 0, self._cache)
        p.end()

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self.invalidate()

    def _outside(self, pos) -> bool:
        return self.panel is None or not self.panel.isVisible() or not self.panel.geometry().contains(pos)

    def mousePressEvent(self, e):
        self._pressed_outside = e.button() == Qt.MouseButton.LeftButton and self._outside(e.position().toPoint())

    def mouseReleaseEvent(self, e):
        if self._pressed_outside and self._outside(e.position().toPoint()):
            self.clicked_outside.emit()
        self._pressed_outside = False

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Escape:
            self.escape.emit()
            return
        super().keyPressEvent(e)


class _Logo(QWidget):
    """The sparkle + LUMINESCENT lockup (top-left of the overlay)."""

    def __init__(self, parent=None):
        super().__init__(parent)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(14)
        mark = QLabel()
        mark.setPixmap(theme.sparkle_pixmap(30))
        mark.setFixedSize(30, 30)
        lay.addWidget(mark)
        lay.addWidget(widgets.label(APP_NAME.upper(), 15, 600, mono=True, spacing=0.3))
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.adjustSize()


class _Clock(QWidget):
    """Time and date under the logo."""

    def __init__(self, parent=None):
        super().__init__(parent)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(2)
        self.time = widgets.label("", 26, 500, mono=True, spacing=-0.02)
        self.date = widgets.label("", 10.5, 500, mono=True, spacing=0.12, tone="muted")
        lay.addWidget(self.time)
        lay.addWidget(self.date)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self.tick()

    def tick(self):
        now = datetime.now()
        self.time.setText(now.strftime("%H:%M"))
        self.date.setText(now.strftime("%A %d %B").upper().replace(" 0", " "))
        self.adjustSize()


class _Status(QWidget):
    """Little chips: running timers, the Hearing Guard level, the next reminder. Hidden when there is nothing."""

    def __init__(self, launcher, parent=None):
        super().__init__(parent)
        self.launcher = launcher
        self.lay = QVBoxLayout(self)
        self.lay.setContentsMargins(0, 0, 0, 0)
        self.lay.setSpacing(6)
        self._last = None

    def _lines(self) -> list[tuple[str, str]]:
        out = []
        mods = self.launcher.modules
        t = mods.get("timer")
        if t is not None and t.running:
            out += [("⏱", line) for line in t.tooltip_lines()[:4]]
        h = mods.get("hearing")
        if h is not None and h.running and h.display_level is not None and h.display_level >= 40:
            out.append(("🎧", f"{h.display_level:.0f} dB · {h.dose * 100:.0f} % of today's dose"))
        r = mods.get("reminders")
        nxt = r.next_reminder() if r is not None and r.running and hasattr(r, "next_reminder") else None
        if nxt:
            when, title = nxt
            day = "" if when.date() == datetime.now().date() else when.strftime("%a ")
            out.append(("⏰", f"{day}{when:%H:%M}  {title}"))
        return out

    def tick(self):
        lines = self._lines()
        if lines == self._last:
            return
        self._last = lines
        widgets.clear_layout(self.lay)
        for icon, text in lines:
            chip = QFrame()
            chip.setObjectName("lumChip")
            cl = QHBoxLayout(chip)
            cl.setContentsMargins(10, 5, 12, 5)
            cl.setSpacing(8)
            ic = widgets.label(icon, 11)
            cl.addWidget(ic)
            cl.addWidget(widgets.label(text, 11.5, 500, mono=True, tone="secondary"))
            self.lay.addWidget(chip, 0, Qt.AlignmentFlag.AlignLeft)
        self.adjustSize()
        self.setVisible(bool(lines))


class _Keys(QObject):
    """While the overlay is up: Ctrl+1…9 switch pages, and typing on the Dashboard or Themes page opens the
    command palette with what you typed."""

    def __init__(self, overlay: "MainOverlay"):
        super().__init__()
        self.o = overlay

    def eventFilter(self, obj, e):
        if e.type() != QEvent.Type.KeyPress or not self.o.is_visible():
            return False
        win = QApplication.activeWindow()
        if win is None or win not in self.o._backdrops:
            return False
        mods = e.modifiers()
        ctrl = bool(mods & Qt.KeyboardModifier.ControlModifier)
        k = e.key()
        fw = QApplication.focusWidget()
        typing = isinstance(fw, (QLineEdit, QTextEdit, QPlainTextEdit, QAbstractSpinBox, QComboBox))
        hook = getattr(self.o._pages.get(self.o._current), "overlay_key", None)
        if callable(hook) and not typing and hook(e):
            return True
        if ctrl and Qt.Key.Key_1 <= k <= Qt.Key.Key_9 and not mods & Qt.KeyboardModifier.AltModifier:
            order = self.o.nav_order()
            i = k - Qt.Key.Key_1
            if i < len(order):
                self.o.show_page(order[i])
                self.o.focus_page()
            return True
        if ctrl or mods & (Qt.KeyboardModifier.AltModifier | Qt.KeyboardModifier.MetaModifier):
            return False
        text = e.text()
        if not text or not text.isprintable() or text.isspace():
            return False
        if not self.o.settings.get("ui.type_to_search", True) or self.o._current not in ("dashboard", "themes"):
            return False
        if typing:
            return False
        self.o.launcher.open_palette(text)
        return True


class _Activation(QObject):
    """A click on the overlay focuses it, and Hyprland then draws it over the panels: lift them again."""

    def __init__(self, overlay: "MainOverlay"):
        super().__init__()
        self.o = overlay

    def eventFilter(self, obj, e):
        if e.type() in (QEvent.Type.WindowActivate, QEvent.Type.MouseButtonPress):
            QTimer.singleShot(80, self.o.raise_panels)
        return False


class MainOverlay:
    def __init__(self, launcher):
        self.launcher = launcher
        self.settings = launcher.settings
        self._backdrops: list[_Backdrop] = []
        self._floated: list[str] = []              # other apps' windows put on top of the overlay
        sync = hypr.sync()
        if sync is not None and sync.enabled:
            sync.foreign_opened.connect(self._foreign_opened)
        self._primary: _Backdrop | None = None
        self.panel: QFrame | None = None
        self.stack: QStackedWidget | None = None
        self.surface: QWidget | None = None         # full-screen layer for the Dashboard and Automation
        self._extra_surfaces: dict[str, QWidget] = {}   # the Dashboard's layer on the other screens
        self._chrome: list[QWidget] = []
        self._pages: dict[str, QWidget] = {}
        self._nav_buttons: dict[str, QPushButton] = {}
        self._current = "dashboard"
        self._anims = []
        self._screen_sig = None
        self._clock: _Clock | None = None
        self._status: _Status | None = None
        self._tick = QTimer()
        self._tick.setInterval(1000)
        self._tick.timeout.connect(self._tick_chrome)
        self._keys = _Keys(self)
        self._activation = _Activation(self)
        QApplication.instance().installEventFilter(self._keys)

    # ── building ───────────────────────────────────────────────────────────
    def _perf(self) -> bool:
        return bool(self.settings.get("ui.performance_mode", False))

    def _host_screen(self):
        if self.settings.get("ui.overlay_screen", "primary") == "cursor":
            return widgets.cursor_screen() or QApplication.primaryScreen()
        return QApplication.primaryScreen()

    def _blur(self) -> bool:
        return bool(self.settings.get("ui.blur", True)) and not self._perf()

    def _screens_signature(self):
        return (tuple((s.geometry().x(), s.geometry().y(), s.geometry().width(), s.geometry().height())
                      for s in QApplication.screens()), self._host_screen().name(), self._blur(),
                tuple(self.nav_order()))

    def _build(self):
        self._destroy()
        dim = float(self.settings.get("ui.overlay_dim", 0.64))
        prim = self._host_screen()
        blur = self._blur()
        for scr in QApplication.screens():
            b = _Backdrop(scr.geometry(), dim, blur, scr.name())
            b.clicked_outside.connect(self._outside_click)
            b.escape.connect(self.hide)
            b.installEventFilter(self._activation)
            self._backdrops.append(b)
            if scr is prim:
                self._primary = b
        if self._primary is None:
            self._primary = self._backdrops[0]
        self._primary.primary = True
        # bars and docks (Noctalia...) stay on top of the overlay: keep the chrome in the space they leave free
        g = prim.geometry()
        a = hypr.available_geometry(prim)
        self._insets = (max(0, a.top() - g.top()), max(0, g.bottom() - a.bottom()))
        self._build_chrome(self._primary)
        # the other screens get a layer of their own, so dashboard boxes can live there too
        self._extra_surfaces = {}
        for b, scr in zip(self._backdrops, QApplication.screens()):
            if b is self._primary:
                continue
            g, a = scr.geometry(), hypr.available_geometry(scr)
            top, bottom = max(0, a.top() - g.top()), max(0, g.bottom() - a.bottom())
            surf = QWidget(b)
            surf.setGeometry(0, top, b.width(), b.height() - top - bottom)
            surf.hide()
            self._extra_surfaces[scr.name()] = surf
        self._screen_sig = self._screens_signature()

    def _build_chrome(self, host: _Backdrop):
        W, H = host.width(), host.height()
        top, bottom = getattr(self, "_insets", (0, 0))
        # the Dashboard's layer covers the free part of the screen, under the chrome
        self.surface = QWidget(host)
        self.surface.setGeometry(0, top, W, H - top - bottom)
        self.surface.hide()

        # panel for the other pages
        w = min(1180, int(W * 0.84))
        h = min(780, H - top - bottom - PANEL_TOP - 56)
        panel = QFrame(host)
        panel.setObjectName("lumPanel")
        panel.setGeometry((W - w) // 2, top + max(PANEL_TOP, (H - top - bottom - h) // 2 + 20), w, h)
        lay = QVBoxLayout(panel)
        lay.setContentsMargins(1, 1, 1, 1)
        lay.setSpacing(0)
        self.stack = QStackedWidget()
        lay.addWidget(self.stack, 1)
        panel.hide()
        host.panel = panel
        self.panel = panel
        self._pages = {}

        logo = _Logo(host)
        logo.move(36, top + 30)
        self._clock = _Clock(host)
        self._clock.move(38, top + 30 + logo.height() + 22)
        show_clock = bool(self.settings.get("ui.show_clock", True))
        self._clock.setVisible(show_clock)
        self._status = _Status(self.launcher, host)
        # isVisible() is still False here (the overlay is not on screen yet), so go by the setting
        self._status.move(36, top + 30 + logo.height() + 22 + (self._clock.sizeHint().height() + 18
                                                               if show_clock else 0))
        self._status.hide()

        # page switcher hanging from the top-centre edge: a 2 px accent line over the pills
        dock = QWidget(host)
        dl = QVBoxLayout(dock)
        dl.setContentsMargins(0, 0, 0, 0)
        dl.setSpacing(0)
        line = QFrame()
        line.setObjectName("lumNavLine")
        line.setFixedHeight(2)
        dl.addWidget(line)
        nav = QFrame()
        nav.setObjectName("lumNav")
        nl = QHBoxLayout(nav)
        nl.setContentsMargins(6, 6, 6, 7)
        nl.setSpacing(2)
        self._nav_group = QButtonGroup(nav)
        self._nav_group.setExclusive(True)
        self._nav_buttons = {}
        names = dict(ALL_PAGES)
        for i, key in enumerate(self.nav_order()):
            b = QPushButton(names[key])
            b.setProperty("nav", True)
            b.setCheckable(True)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            hk = self.settings.get(self._nav_hotkey_key(key), "") if key in dict(NAV_SECTIONS) else ""
            tip = f"Ctrl+{i + 1}" if i < 9 else ""
            if hk:
                from core.hotkeys import pretty
                tip = f"{pretty(hk)}   ·   {tip}" if tip else pretty(hk)
            b.setToolTip(tip)
            b.clicked.connect(lambda _, k=key: self.show_page(k))
            self._nav_group.addButton(b)
            nl.addWidget(b)
            self._nav_buttons[key] = b
        dl.addWidget(nav)
        dock.adjustSize()
        dock.move((W - dock.width()) // 2, top)

        # top-right: the hotkey hint and a round ✕
        right = QWidget(host)
        rl = QHBoxLayout(right)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(14)
        from core.hotkeys import pretty
        combo = self.settings.get("ui.overlay_hotkey", "")
        if combo:
            rl.addWidget(widgets.label(pretty(combo).upper(), 10, 500, mono=True, spacing=0.08, tone="muted"))
        close = QPushButton("✕")
        close.setObjectName("lumClose")
        close.setFixedSize(36, 36)
        close.setCursor(Qt.CursorShape.PointingHandCursor)
        close.setToolTip("Hide (Esc). Luminescent keeps running; quit from Settings → General.")
        close.clicked.connect(self.hide)
        rl.addWidget(close)
        right.adjustSize()
        right.move(W - 28 - right.width(), top + 26)
        self._chrome = [logo, dock, right, self._clock, self._status]
        for c in (dock, right):
            c.mousePressEvent = lambda e: e.accept()      # a click between the pills is not "outside"
        for c in self._chrome:
            c.raise_()

    @staticmethod
    def _nav_hotkey_key(key: str) -> str:
        return "ui.hotkey_settings_page" if key == "settings" else f"ui.hotkey_{key}"

    def nav_order(self) -> list[str]:
        """The pages in the navigation bar, in order (Dashboard and Settings are always there)."""
        known = [k for k, _ in ALL_PAGES]
        order = [k for k in (self.settings.get("ui.nav_order") or []) if k in known]
        order = list(dict.fromkeys(order))
        for k in REQUIRED_PAGES:
            if k not in order:
                order.insert(0 if k == "dashboard" else len(order), k)
        return order

    def _destroy(self):
        for page in self._pages.values():
            closer = getattr(page, "teardown", None)
            if closer:
                try:
                    closer()
                except Exception:
                    log.exception("page teardown failed")
        for b in self._backdrops:
            b.close()
            b.deleteLater()
        self._backdrops = []
        self._primary = None
        self.panel = None
        self.stack = None
        self.surface = None
        self._extra_surfaces = {}
        self._chrome = []
        self._pages = {}
        self._clock = self._status = None
        self._tick.stop()

    def _make_page(self, key: str) -> QWidget:
        try:
            if key == "dashboard":
                from ui.dashboard import DashboardPage
                return DashboardPage(self.launcher, self.surface, self._host_screen().name(),
                                     dict(self._extra_surfaces))
            if key == "automation":
                from ui.automation import AutomationPage
                return AutomationPage(self.launcher, self.surface)
            if key == "themes":
                from ui.themes_page import ThemesPage
                return ThemesPage(self.launcher)
            if key == "clipboard":
                return self._clipboard_page()
            if key == "settings":
                from settings_ui import SettingsPage
                return SettingsPage(self.settings, self.launcher)
            if key in ("notes", "timers", "calculator"):
                m = self.launcher.modules.get({"timers": "timer"}.get(key, key))
                if m is None or not m.running:
                    return self._message_page(f"The {key.capitalize()} module is turned off "
                                              f"(Settings → {m.title if m else key}).")
                return m.make_page()
            if key == "today":
                from ui.today import TodayPage
                return TodayPage(self.launcher)
            if key == "stats":
                from ui.stats import StatsPage
                return StatsPage(self.launcher)
        except Exception as e:
            log.exception("could not build the %s page", key)
            return self._message_page(f"⚠ The {key} page could not be built:\n{type(e).__name__}: {e}\n\n"
                                      "See Settings → General → log file.")
        return self._message_page(key)

    def _clipboard_page(self) -> QWidget:
        m = self.launcher.modules.get("snippets")
        if m is None or not m.running:
            page = self._message_page("The Clipboard (Snippets) module is turned off.")
            btn = widgets.button("Turn it on", "primary")
            btn.clicked.connect(lambda: (self.launcher.toggle_module("snippets", True),
                                         QTimer.singleShot(50, self.refresh_page_later("clipboard"))))
            page.layout().addWidget(btn, 0, Qt.AlignmentFlag.AlignHCenter)
            page.layout().addStretch()
            return page
        return m.make_view(embedded=True, on_used=self.hide)

    def refresh_page_later(self, key: str):
        def go():
            old = self._pages.pop(key, None)
            if old is not None:
                if self.stack is not None and self.stack.indexOf(old) >= 0:
                    self.stack.removeWidget(old)
                old.hide()
                old.deleteLater()
            if self.is_visible() and self._current == key:
                self.show_page(key)
        return go

    @staticmethod
    def _message_page(text: str) -> QWidget:
        w = QWidget()
        lay = QVBoxLayout(w)
        lay.addStretch()
        lbl = QLabel(text)
        lbl.setAlignment(Qt.AlignmentFlag.AlignCenter)
        lbl.setWordWrap(True)
        lbl.setObjectName("lumHint")
        lay.addWidget(lbl)
        return w

    # ── showing ────────────────────────────────────────────────────────────
    def is_visible(self) -> bool:
        return self._primary is not None and self._primary.isVisible()

    def toggle(self):
        if self.is_visible():
            self.hide()
        else:
            self.open("dashboard")

    def open(self, section: str = "dashboard", sub: str = ""):
        if self._primary is None or self._screen_sig != self._screens_signature():
            self._build()
        self._apply_look()
        self.show_page(section or "dashboard", sub)
        if not self.is_visible():
            perf = self._perf()
            for b in self._backdrops:
                b.setWindowOpacity(1.0 if perf else 0.0)
                b.show()
                b.raise_()
            if not perf:
                self._fade(1.0)
        widgets.bring_to_front(self._primary)
        self._tick_chrome()
        self._tick.start()
        self.focus_page()
        events.emit("overlay.opened", self._current)
        # panels that were open (Calculator, Notes…) stay above the overlay; the backdrop's map and focus
        # put it on top of them, so lift them again once it is up
        QTimer.singleShot(250, self.raise_panels)

    def focus_page(self):
        page = self._pages.get(self._current)
        focus = getattr(page, "focus_target", None)
        QTimer.singleShot(0, lambda: (focus() if callable(focus) else self._primary.setFocus()
                                      if self._primary is not None else None))

    def raise_panels(self):
        """Put Luminescent's own floating panels (Calculator, Notes, Timers…) back above the overlay."""
        if not self.is_visible():
            return
        for w in QApplication.topLevelWidgets():
            if isinstance(w, widgets.FloatingPanel) and w.isVisible():
                widgets.raise_window(w)

    def _tick_chrome(self):
        if not self.is_visible():
            self._tick.stop()
            return
        if self._clock is not None and self._clock.isVisible():
            self._clock.tick()
        if self._status is not None:
            if self.settings.get("ui.show_status", True) and self._current != "automation":
                self._status.tick()
            else:
                self._status.hide()

    def _apply_look(self):
        perf = self._perf()
        dim = float(self.settings.get("ui.overlay_dim", 0.64))
        for b in self._backdrops:
            b.dim = dim
            b.glow = bool(theme.P.glow) and not perf and b is self._primary
            b.image_path = "" if perf else theme.P.background_image
            b.image_opacity = theme.P.image_opacity
            b.invalidate()

    def show_page(self, key: str, sub: str = ""):
        if self.stack is None:
            return
        if key not in self._nav_buttons and key not in dict(ALL_PAGES):
            key = "dashboard"
        old = self._pages.get(self._current)
        if old is not None and self._current != key:
            off = getattr(old, "on_hide", None)
            if callable(off):
                off()
        page = self._pages.get(key)
        if page is None:
            page = self._make_page(key)
            self._pages[key] = page
            if key not in FULL_PAGES:
                self.stack.addWidget(page)
        full = key in FULL_PAGES
        self.surface.setVisible(full)
        for surf in self._extra_surfaces.values():
            surf.setVisible(key == "dashboard")
        self.panel.setVisible(not full)
        for k in FULL_PAGES:
            other = self._pages.get(k)
            if other is not None and k != key:
                other.hide()
        if full:
            page.setGeometry(self.surface.rect())
            page.show()
        else:
            self.stack.setCurrentWidget(page)
        # the node editor uses the whole screen: the clock and chips would sit on top of the nodes
        if self._clock is not None:
            self._clock.setVisible(key != "automation" and bool(self.settings.get("ui.show_clock", True)))
        if self._status is not None and key == "automation":
            self._status.hide()
        for c in self._chrome:
            c.raise_()
        if key in self._nav_buttons:
            self._nav_buttons[key].setChecked(True)
        else:
            checked = self._nav_group.checkedButton()
            if checked is not None:
                self._nav_group.setExclusive(False)
                checked.setChecked(False)
                self._nav_group.setExclusive(True)
        self._current = key
        self._primary.invalidate()
        if sub and hasattr(page, "open_section"):
            page.open_section(sub)
        on_show = getattr(page, "on_show", None)
        if callable(on_show):
            on_show()

    def hide(self):
        if not self.is_visible():
            return
        page = self._pages.get(self._current)
        on_hide = getattr(page, "on_hide", None)
        if callable(on_hide):
            on_hide()
        if self._perf():
            self._hide_now()
        else:
            self._fade(0.0, self._hide_now)

    def _hide_now(self):
        events.emit("overlay.closed")
        self._tick.stop()
        for b in self._backdrops:
            b.hide()
            b.setWindowOpacity(1.0)
        sync = hypr.sync()
        for addr in self._floated:                  # back into the tiling layout
            if sync is not None:
                sync.unfloat(addr)
        self._floated = []

    def _foreign_opened(self, addr: str, cls: str):
        """An app started while the overlay is up (Super+E, a dashboard item...): show it on top, floating."""
        if not self.is_visible() or not self.settings.get("ui.apps_over_overlay", True):
            return
        sync = hypr.sync()
        if sync is None:
            return
        sync.float_on_top(addr, widgets.cursor_screen_geometry())
        self._floated.append(addr)

    def _fade(self, to: float, then=None):
        for a in self._anims:
            a.stop()
        self._anims = []
        for b in self._backdrops:
            a = QPropertyAnimation(b, b"windowOpacity")
            a.setDuration(120)
            a.setStartValue(b.windowOpacity())
            a.setEndValue(to)
            a.setEasingCurve(QEasingCurve.Type.OutCubic)
            self._anims.append(a)
        if then and self._anims:
            self._anims[0].finished.connect(then)
        for a in self._anims:
            a.start()

    def _outside_click(self):
        if self.settings.get("ui.click_outside_closes", True):
            self.hide()

    def outside_click(self):
        """A click on empty overlay space inside a page."""
        self._outside_click()

    def left_chrome_bottom(self) -> int:
        """The bottom of the logo, clock and status chips in the full-screen layer's coordinates, with room for
        two more chips (a timer started later must not end up under a dashboard box)."""
        top = getattr(self, "_insets", (0, 0))[0]
        y = 0
        for w in (self._clock, self._status):
            if w is not None and (w.isVisible() or w is self._status):
                y = max(y, w.y() + (w.height() if w.isVisible() else 0))
        if self._status is not None and self.settings.get("ui.show_status", True):
            y += 2 * 34
        return max(0, y - top)

    # ── changes ────────────────────────────────────────────────────────────
    def rebuild(self):
        """Theme or navigation changed: rebuild everything, keeping the current page if it is open."""
        was_open, cur = self.is_visible(), self._current
        self._destroy()
        if was_open:
            self.open(cur)
        else:
            QTimer.singleShot(1500, self.prewarm)

    def prewarm(self, keys: list | None = None):
        """Build the overlay and its pages ahead of time, one page per event-loop pass, so the first
        open (Settings takes most of a second to build) is instant. Nothing is shown."""
        if self.is_visible():
            return
        if self._primary is None or self._screen_sig != self._screens_signature():
            self._build()
        keys = list(self._nav_buttons) if keys is None else keys
        while keys and keys[0] in self._pages:
            keys.pop(0)
        if not keys:
            return
        key = keys.pop(0)
        try:
            page = self._make_page(key)
        except Exception:
            log.exception("could not prepare the %s page", key)
            return
        self._pages[key] = page
        if key not in FULL_PAGES:
            self.stack.addWidget(page)
        else:
            page.hide()
        if keys:
            QTimer.singleShot(200, lambda: self.prewarm(keys))

    def settings_applied(self):
        """Settings were saved: pick up a new nav order / dim / performance mode without closing."""
        if self._primary is None:
            return
        if list(self._nav_buttons) != self.nav_order() or self._screen_sig != self._screens_signature():
            QTimer.singleShot(0, self.rebuild)
            return
        self._apply_look()
        if self._clock is not None:
            self._clock.setVisible(bool(self.settings.get("ui.show_clock", True)))
        page = self._pages.get("dashboard")
        if page is not None and hasattr(page, "refresh"):
            page.refresh(force=True)
        auto = self._pages.get("automation")
        if auto is not None and hasattr(auto, "settings_applied"):
            auto.settings_applied()

    def page(self, key: str):
        return self._pages.get(key)

    def shutdown(self):
        self._destroy()
