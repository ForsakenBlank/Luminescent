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
would re-render every child on every repaint); the panel's glow is a second
layer on top, so switching pages is one blit. Performance mode drops the glow,
the background image and every animation (core.anim).

If a page breaks the overlay (a widget gone that something still used), repair()
throws everything away and builds it again, so the hub keeps working.
"""
import os
import logging


from PyQt6.QtWidgets import (
    QApplication, QWidget, QFrame, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QStackedWidget, QButtonGroup,
    QLineEdit, QTextEdit, QPlainTextEdit, QAbstractSpinBox, QComboBox, QGraphicsOpacityEffect
)
from PyQt6.QtCore import (Qt, QRect, QRectF, QPointF, QTimer, QEasingCurve, QEvent, QObject, QCoreApplication,
                          pyqtSignal)
from PyQt6.QtGui import QPainter, QColor, QPixmap, QPen, QRadialGradient, QBrush

from core import hypr, theme, widgets, events, anim, APP_NAME
from core.guard import alive
from core.settings import NAV_SECTIONS, ALL_PAGES, REQUIRED_PAGES
from ui.chrome import ChromeLayer

log = logging.getLogger("overlay")

_FLAGS = Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool
DOT_STEP = 20
PANEL_TOP = 96          # panel pages start below the docked switcher
FULL_PAGES = ("dashboard", "automation")    # pages that fill the screen instead of sitting in the panel
_KEYPRESS = QEvent.Type.KeyPress
_ACTIVATING = frozenset((QEvent.Type.WindowActivate, QEvent.Type.MouseButtonPress))


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
        self.reveal = 1.0                          # 0..1 while fading in or out
        self._base: QPixmap | None = None          # dim, picture, dots, top glow
        self._base_key = None
        self._cache: QPixmap | None = None         # the base plus the panel's glow
        self._cache_key = None
        self._pressed_outside = False

    def invalidate(self):
        self._base_key = None
        self._cache_key = None
        self.update()

    def set_reveal(self, v: float):
        self.reveal = max(0.0, min(1.0, v))
        self.update()

    def _panel_rect(self):
        if self.panel is not None and alive(self.panel) and self.panel.isVisible():
            g = self.panel.geometry()
            return g.x(), g.y(), g.width(), g.height()
        return None

    def _bkey(self):
        return (self.width(), self.height(), round(self.dim, 3), self.glow, self.primary, self.image_path,
                round(self.image_opacity, 2), theme.P.primary, theme.P.text_primary, theme.is_light())

    @staticmethod
    def _dot_tile() -> QPixmap:
        tile = QPixmap(DOT_STEP, DOT_STEP)
        tile.fill(QColor(0, 0, 0, 0))
        p = QPainter(tile)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        dot = QColor(theme.P.text_primary)
        dot.setAlphaF(0.085 if not theme.is_light() else 0.12)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(dot)
        off = DOT_STEP / 2
        p.drawEllipse(QPointF(off, off), 1.1, 1.1)
        p.end()
        return tile

    def _render_base(self) -> QPixmap:
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
        # the dot grid, edge to edge: one tiled brush instead of thousands of little circles
        p.fillRect(0, 0, w, h, QBrush(self._dot_tile()))
        if self.glow and self.primary:
            # a soft halo hanging from the top-centre edge, behind the page switcher
            c = QColor(theme.P.primary)
            p.setPen(Qt.PenStyle.NoPen)
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
        p.end()
        return out

    def _render_panel(self, base: QPixmap, rect) -> QPixmap:
        out = QPixmap(base)
        p = QPainter(out)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        pr = QRectF(*rect)
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

    def _pixmap(self) -> QPixmap:
        bkey = self._bkey()
        if self._base is None or bkey != self._base_key:
            self._base = self._render_base()
            self._base_key = bkey
            self._cache_key = None
        rect = self._panel_rect()
        if rect is None:
            return self._base
        key = (bkey, rect, theme.P.radius)
        if self._cache is None or key != self._cache_key:
            self._cache = self._render_panel(self._base, rect)
            self._cache_key = key
        return self._cache

    def paintEvent(self, e):
        px = self._pixmap()
        p = QPainter(self)
        p.setClipRegion(e.region())
        p.setCompositionMode(QPainter.CompositionMode.CompositionMode_Source)
        if self.reveal >= 0.999:
            p.drawPixmap(0, 0, px)
        else:
            p.fillRect(e.rect(), QColor(0, 0, 0, 0))
            p.setCompositionMode(QPainter.CompositionMode.CompositionMode_SourceOver)
            p.setOpacity(self.reveal)
            p.drawPixmap(0, 0, px)
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


class _Keys(QObject):
    """While the overlay is up: Ctrl+1…9 switch pages, and typing on the Dashboard or Themes page opens the
    command palette with what you typed."""

    def __init__(self, overlay: "MainOverlay"):
        super().__init__()
        self.o = overlay

    def eventFilter(self, obj, e):
        # every event of the whole app passes here: one type() call and out for nearly all of them
        t = e.type()
        if t in _ACTIVATING:
            self.o._activation.seen(obj)
            return False
        if t != _KEYPRESS or not self.o.is_visible():
            return False
        win = QApplication.activeWindow()
        if win is None or win not in self.o._backdrops:
            return False
        mods = e.modifiers()
        ctrl = bool(mods & Qt.KeyboardModifier.ControlModifier)
        k = e.key()
        chrome = self.o.chrome
        if k == Qt.Key.Key_Escape and chrome is not None and chrome.editing:
            chrome.set_editing(False)                 # Esc leaves edit mode before it closes anything
            return True
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
    """A click anywhere on the overlay (a nav button, the dashboard, a page) focuses it, and the compositor
    then draws it over the panels: lift them again. Watches the whole app, because a click on a child widget
    never reaches the backdrop window itself."""

    def __init__(self, overlay: "MainOverlay"):
        super().__init__()
        self.o = overlay
        self._soon = QTimer()
        self._soon.setSingleShot(True)
        self._soon.setInterval(80)
        self._soon.timeout.connect(self._lift)

    def _lift(self):
        self.o.raise_panels()
        QTimer.singleShot(250, self.o.raise_panels)      # once more for a compositor that is slow to restack

    def seen(self, obj):
        if isinstance(obj, QWidget) and self.o.is_visible() and obj.window() in self.o._backdrops:
            self._soon.start()


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
        self._nav_group: QButtonGroup | None = None
        self._pill: QFrame | None = None            # the sliding marker under the page switcher
        self._pill_on = False
        self._current = "dashboard"
        self._host_name = ""
        self._screen_sig = None
        self.chrome: ChromeLayer | None = None      # the logo, clock, chips... (ui/chrome.py)
        self._last_repair = 0.0
        self._opening = False
        self._tick = QTimer()
        self._tick.setInterval(1000)
        self._tick.timeout.connect(self._tick_chrome)
        self._idle = QTimer()                      # builds the other pages while the overlay is open
        self._idle.setSingleShot(True)
        self._idle.timeout.connect(self._idle_build)
        self._keys = _Keys(self)
        self._activation = _Activation(self)
        QApplication.instance().installEventFilter(self._keys)       # also tells _activation about clicks

    # ── building ───────────────────────────────────────────────────────────
    def _perf(self) -> bool:
        return bool(self.settings.get("ui.performance_mode", False))

    def _host_screen(self):
        if self.settings.get("ui.overlay_screen", "cursor") == "cursor":
            return widgets.cursor_screen() or QApplication.primaryScreen()
        return QApplication.primaryScreen()

    def _blur(self) -> bool:
        return bool(self.settings.get("ui.blur", True)) and not self._perf()

    def _screens_signature(self, host: str = ""):
        # host: keep the screen it is on now (while it is open it does not jump to the mouse)
        return (tuple((s.geometry().x(), s.geometry().y(), s.geometry().width(), s.geometry().height())
                      for s in QApplication.screens()), host or self._host_screen().name(), self._blur(),
                tuple(self.nav_order()))

    def _build(self, keep_pages: bool = False):
        kept = self._destroy(keep_pages)
        try:
            self._build_windows()
        except Exception:
            log.exception("could not build the overlay")
            self._destroy()
            for page in kept.values():
                page.deleteLater()
            raise
        known = dict(ALL_PAGES)
        for key, page in kept.items():
            if key in known and alive(page):
                self._pages[key] = page
                self.stack.addWidget(page)
            elif alive(page):
                page.deleteLater()

    def _build_windows(self):
        dim = float(self.settings.get("ui.overlay_dim", 0.64))
        prim = self._host_screen()
        blur = self._blur()
        for scr in QApplication.screens():
            b = _Backdrop(scr.geometry(), dim, blur, scr.name())
            b.clicked_outside.connect(self._outside_click)
            b.escape.connect(self.hide)
            self._backdrops.append(b)
            if scr is prim:
                self._primary = b
        if self._primary is None:
            self._primary = self._backdrops[0]
            prim = QApplication.screens()[0]
        self._primary.primary = True
        self._host_name = prim.name()
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
        self.chrome = ChromeLayer(self)
        self.chrome.build({b.screen_name: b for b in self._backdrops}, self._host_name, self._insets[0])
        self._screen_sig = self._screens_signature(self._host_name)

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
        self._pill_on = anim.on("nav")
        self._pill = QFrame(nav)
        self._pill.setObjectName("lumNavPill")
        self._pill.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        self._pill.hide()
        self._nav_group = QButtonGroup(nav)
        self._nav_group.setExclusive(True)
        self._nav_buttons = {}
        names = dict(ALL_PAGES)
        for i, key in enumerate(self.nav_order()):
            b = QPushButton(names[key])
            b.setProperty("nav", True)
            b.setProperty("pill", self._pill_on)
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
        nl.activate()
        dock.adjustSize()
        dock.move((W - dock.width()) // 2, top)
        self._pill.lower()

        # top-right: the hotkey hint and a round ✕
        right = QWidget(host)
        rl = QHBoxLayout(right)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.setSpacing(14)
        from core.hotkeys import pretty
        combo = self.settings.get("ui.overlay_hotkey", "")
        if combo:
            rl.addWidget(widgets.label(pretty(combo).upper(), 10, 500, mono=True, spacing=0.08, tone="muted"))
        if getattr(self.launcher, "safe_mode", False):
            rl.addWidget(widgets.label("SAFE MODE", 10, 600, mono=True, spacing=0.08, tone="danger"))
        edit = QPushButton("✎")
        edit.setObjectName("lumClose")
        edit.setFixedSize(36, 36)
        edit.setCursor(Qt.CursorShape.PointingHandCursor)
        edit.setToolTip("Edit the layout: move, add, copy, resize or freeze the logo, clock, date and status chips")
        edit.clicked.connect(lambda: self.chrome.toggle_editing() if self.chrome is not None else None)
        rl.addWidget(edit)
        close = QPushButton("✕")
        close.setObjectName("lumClose")
        close.setFixedSize(36, 36)
        close.setCursor(Qt.CursorShape.PointingHandCursor)
        close.setToolTip("Hide (Esc). Luminescent keeps running; quit from Settings → General.")
        close.clicked.connect(self.hide)
        rl.addWidget(close)
        right.adjustSize()
        right.move(W - 28 - right.width(), top + 26)
        self._chrome = [dock, right]
        for c in (dock, right):
            c.mousePressEvent = lambda e: e.accept()      # a click between the pills is not "outside"
        for c in self._chrome:
            c.raise_()

    def _place_pill(self, key: str, animate: bool):
        pill = self._pill
        if not alive(pill):
            return
        b = self._nav_buttons.get(key)
        if not self._pill_on or b is None:
            pill.hide()
            return
        nav = pill.parentWidget()
        if nav is not None and nav.layout() is not None:
            nav.layout().activate()                  # the buttons' real places, not Qt's defaults
        target = b.geometry()
        if not pill.isVisible() or not animate:
            anim.stop(pill)
            pill.setGeometry(target)
            pill.show()
            pill.lower()
        else:
            anim.geometry(pill, target, "nav", 230)

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

    def _destroy(self, keep_pages: bool = False) -> dict:
        """Throw the overlay away. keep_pages: the panel pages (Settings, Themes...) are taken out first and
        returned, so a move to another screen does not have to build them again."""
        kept = {}
        self._idle.stop()
        for key, page in list(self._pages.items()):
            if not alive(page):
                continue
            if keep_pages and key not in FULL_PAGES and alive(self.stack):
                try:
                    self.stack.removeWidget(page)
                    page.hide()
                    page.setParent(None)
                    kept[key] = page
                    continue
                except RuntimeError:
                    log.exception("could not keep the %s page", key)
            closer = getattr(page, "teardown", None)
            if closer:
                try:
                    closer()
                except Exception:
                    log.exception("page teardown failed")
        for b in self._backdrops:
            if not alive(b):
                continue
            try:
                anim.stop(b)
                b.close()
                b.deleteLater()
            except RuntimeError:
                log.exception("could not close an overlay window")
        self._backdrops = []
        self._primary = None
        self.panel = None
        self.stack = None
        self.surface = None
        self._extra_surfaces = {}
        self._chrome = []
        self._pages = {}
        self._nav_buttons = {}
        self._nav_group = None
        self._pill = None
        self.chrome = None
        self._screen_sig = None
        self._tick.stop()
        return kept

    def _make_page(self, key: str) -> QWidget:
        try:
            if key == "dashboard":
                from ui.dashboard import DashboardPage
                return DashboardPage(self.launcher, self.surface, self._host_name, dict(self._extra_surfaces))
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
            if key == "calendar":
                from ui.calendar_page import CalendarPage
                return CalendarPage(self.launcher)
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
            if alive(old):
                if alive(self.stack) and self.stack.indexOf(old) >= 0:
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
        return alive(self._primary) and self._primary.isVisible()

    def toggle(self):
        if self.is_visible():
            self.hide()
        else:
            self.open(self.start_page())

    def start_page(self) -> str:
        """Where the overlay opens: the Dashboard, or the page you were on (Settings → Overlay)."""
        last = str(self.settings.get("ui.last_page", "") or "")
        if self.settings.get("ui.remember_page", False) and last in self.nav_order():
            return last
        return "dashboard"

    def open(self, section: str = "dashboard", sub: str = "", fade: bool = True):
        try:
            self._open(section, sub, fade)
        except Exception:
            # something in the old overlay was broken: start over from nothing, once
            log.exception("opening the overlay failed: building it again")
            self._hard_reset()
            try:
                self._open(section, sub, fade)
            except Exception:
                log.exception("the overlay could not be opened")
                self._hard_reset()
                self.launcher.notify(APP_NAME, "The overlay could not be opened (see the log). Tray → Restart, or "
                                               "Restart in safe mode from the command palette, can help.", 8000)

    def _open(self, section: str, sub: str, fade: bool):
        visible = self.is_visible()
        if self._primary is None or self._screen_sig != self._screens_signature(self._host_name if visible else ""):
            self._build(keep_pages=True)
            visible = False
        self._apply_look()
        self._opening = True
        try:
            self.show_page(section or "dashboard", sub)
        finally:
            self._opening = False
        if not visible:
            start = 0.0 if fade and anim.on("overlay") else 1.0
            for b in self._backdrops:
                b.set_reveal(start)
                b.show()
                b.raise_()
            if start < 1.0:
                self._fade(1.0)
        widgets.bring_to_front(self._primary)
        # the buttons only have their real size once they are on screen and styled
        QTimer.singleShot(0, lambda: self._place_pill(self._current, False))
        self._tick_chrome()
        self._tick.start()
        self.focus_page()
        events.emit("overlay.opened", self._current)
        # panels that were open (Calculator, Notes…) stay above the overlay; the backdrop's map and focus
        # put it on top of them, so lift them again once it is up
        QTimer.singleShot(250, self.raise_panels)
        self._idle.start(900)

    def _hard_reset(self):
        try:
            self._destroy()
        except Exception:
            log.exception("could not tear the overlay down cleanly")
            self._backdrops, self._primary, self._pages = [], None, {}
            self.panel = self.stack = self.surface = None
            self._extra_surfaces, self._chrome, self._nav_buttons = {}, [], {}
            self._screen_sig = None

    def repair(self, key: str = ""):
        """Something on the overlay broke: build all of it again (at most every few seconds)."""
        import time
        now = time.monotonic()
        if now - self._last_repair < 3:
            return
        self._last_repair = now
        was_open, cur = self.is_visible(), key or self._current
        log.warning("repairing the overlay (was on %s)", cur)
        self._hard_reset()
        if was_open:
            self.open(cur, fade=False)
        else:
            QTimer.singleShot(1500, self.prewarm)

    def focus_page(self):
        page = self._pages.get(self._current)
        focus = getattr(page, "focus_target", None) if alive(page) else None

        def go():
            if callable(focus) and alive(page):
                focus()
            elif alive(self._primary):
                self._primary.setFocus()
        QTimer.singleShot(0, go)

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
        if self.chrome is not None:
            self.chrome.tick()

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
        try:
            self._show_page(key, sub)
        except RuntimeError:
            # a page kept a widget that is gone: rebuild everything instead of leaving a blank overlay
            log.exception("showing the %s page failed", key)
            QTimer.singleShot(0, lambda: self.repair(key))

    def _show_page(self, key: str, sub: str):
        if key not in self._nav_buttons and key not in dict(ALL_PAGES):
            key = "dashboard"
        was, was_full = self._current, self._current in FULL_PAGES and self.surface.isVisible()
        old = self._pages.get(self._current)
        if alive(old) and self._current != key:
            off = getattr(old, "on_hide", None)
            if callable(off):
                off()
        page = self._pages.get(key)
        if page is not None and not alive(page):
            page = None
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
            if alive(other) and k != key:
                other.hide()
        if full:
            page.setGeometry(self.surface.rect())
            page.show()
        else:
            self.stack.setCurrentWidget(page)
        for c in self._chrome:
            if alive(c):
                c.raise_()
        if self.chrome is not None:
            self.chrome.page_changed(key)            # hides the clock and chips over the node editor
        if key in self._nav_buttons:
            self._nav_buttons[key].setChecked(True)
        elif self._nav_group is not None:
            checked = self._nav_group.checkedButton()
            if checked is not None:
                self._nav_group.setExclusive(False)
                checked.setChecked(False)
                self._nav_group.setExclusive(True)
        switching = key != was and self.is_visible() and not getattr(self, "_opening", False)
        self._place_pill(key, switching)
        self._current = key
        if alive(self._primary):
            self._primary.invalidate()
        if switching:
            # the page fades in; coming from the dashboard the whole panel does
            target = page if full else (self.panel if was_full else self.stack)
            anim.fade(target, 0.0, 1.0, "pages", 170)
        if sub and hasattr(page, "open_section"):
            page.open_section(sub)
        on_show = getattr(page, "on_show", None)
        if callable(on_show):
            on_show()

    def _fade_targets(self) -> list[QWidget]:
        out = [self.panel, self.surface, *self._extra_surfaces.values(), *self._chrome,
               *(self.chrome.widgets() if self.chrome is not None else [])]
        return [w for w in out if alive(w) and w.isVisible() and (w.graphicsEffect() is None or
                                                                   isinstance(w.graphicsEffect(),
                                                                              QGraphicsOpacityEffect))]

    def _fade(self, to: float, then=None):
        """Fade the overlay in or out. Window opacity does nothing on Wayland, so the backdrops paint
        themselves fainter and the widgets on them get an opacity effect for the length of the fade."""
        if not alive(self._primary):
            if then:
                then()
            return
        start = self._primary.reveal
        effects = []
        for w in self._fade_targets():
            eff = w.graphicsEffect()
            if eff is None:
                eff = QGraphicsOpacityEffect(w)
                eff.setOpacity(start)
                w.setGraphicsEffect(eff)
            effects.append((w, eff))
        backdrops = list(self._backdrops)

        def frame(v):
            for b in backdrops:
                if alive(b):
                    b.set_reveal(v)
            for _w, eff in effects:
                if alive(eff):
                    eff.setOpacity(v)

        def done():
            for w, eff in effects:
                if alive(w) and w.graphicsEffect() is eff:
                    w.setGraphicsEffect(None)
            if then:
                then()
        anim.value(start, to, frame, "overlay", 150 if to > 0 else 120, done, self._primary,
                   QEasingCurve.Type.OutCubic if to > 0 else QEasingCurve.Type.InCubic, "reveal")

    def hide(self):
        if not self.is_visible():
            return
        page = self._pages.get(self._current)
        on_hide = getattr(page, "on_hide", None) if alive(page) else None
        if callable(on_hide):
            on_hide()
        self._idle.stop()
        if self.chrome is not None:
            self.chrome.set_editing(False)
        if anim.on("overlay"):
            self._fade(0.0, self._hide_now)
        else:
            self._hide_now()

    def _hide_now(self):
        events.emit("overlay.closed")
        if self.settings.get("ui.last_page", "") != self._current:
            self.settings.set("ui.last_page", self._current)
            self.settings.save_quiet()
        self._tick.stop()
        for b in self._backdrops:
            if alive(b):
                b.hide()
                b.set_reveal(1.0)
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

    def _outside_click(self):
        if self.settings.get("ui.click_outside_closes", True):
            self.hide()

    def outside_click(self):
        """A click on empty overlay space inside a page."""
        self._outside_click()

    def left_chrome_bottom(self) -> int:
        """The bottom of the logo, clock and status chips (wherever you put them in the top-left part of the
        main screen) in the full-screen layer's coordinates, with room for two more chips (a timer started later
        must not end up under a dashboard box)."""
        top = getattr(self, "_insets", (0, 0))[0]
        return max(0, (self.chrome.bottom_on_host() if self.chrome is not None else 0) - top)

    # ── changes ────────────────────────────────────────────────────────────
    def _snapshots(self) -> dict[str, QPixmap]:
        out = {}
        for b in self._backdrops:
            if alive(b) and b.isVisible():
                try:
                    out[b.screen_name] = b.grab()
                except RuntimeError:
                    pass
        return out

    def _crossfade(self, snaps: dict[str, QPixmap]):
        """Lay a picture of the old look over the new one and fade it away."""
        for b in self._backdrops:
            px = snaps.get(b.screen_name)
            if px is None:
                continue
            curtain = QLabel(b)
            curtain.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            curtain.setPixmap(px)
            curtain.setScaledContents(True)
            curtain.setGeometry(b.rect())
            curtain.show()
            curtain.raise_()
            anim.fade(curtain, 1.0, 0.0, "theme", 280, then=curtain.deleteLater,
                      curve=QEasingCurve.Type.InOutQuad)

    def rebuild(self, between=None):
        """Theme, text size or navigation changed: rebuild everything, keeping the current page if it is open.
        between() runs once the old overlay is gone (restyling widgets that are about to be deleted would only
        cost time)."""
        was_open, cur = self.is_visible(), self._current
        snaps = self._snapshots() if was_open and anim.on("theme") else {}
        self._hard_reset()
        QCoreApplication.sendPostedEvents(None, QEvent.Type.DeferredDelete)
        if between is not None:
            try:
                between()
            except Exception:
                log.exception("restyling failed")
        if was_open:
            self.open(cur, fade=False)
            if snaps:
                self._crossfade(snaps)
        else:
            QTimer.singleShot(1500, self.prewarm)

    def prewarm(self, keys: list | None = None):
        """Build the overlay and its pages ahead of time, one page per event-loop pass, so the first
        open (Settings takes most of a second to build) is instant. Nothing is shown."""
        if self.is_visible():
            return
        try:
            if self._primary is None or self._screen_sig != self._screens_signature():
                self._build(keep_pages=True)
        except Exception:
            log.exception("could not prepare the overlay")
            self._hard_reset()
            return
        keys = list(self._nav_buttons) if keys is None else keys
        while keys and keys[0] in self._pages:
            keys.pop(0)
        if not keys:
            return
        self._prepare(keys.pop(0))
        if keys:
            QTimer.singleShot(200, lambda: self.prewarm(keys))

    def _prepare(self, key: str):
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

    def _idle_build(self):
        """While the overlay is open: build one page that is not built yet, so switching to it is instant."""
        if not self.is_visible() or self.stack is None:
            return
        if QApplication.mouseButtons() != Qt.MouseButton.NoButton:
            self._idle.start(500)                  # not in the middle of a drag
            return
        todo = [k for k in self._nav_buttons if k not in self._pages]
        if not todo:
            return
        self._prepare(todo[0])
        if len(todo) > 1:
            self._idle.start(350)

    def settings_applied(self):
        """Settings were saved: pick up a new nav order / dim / performance mode without closing."""
        if self._primary is None:
            return
        if list(self._nav_buttons) != self.nav_order() or self._pill_on != anim.on("nav") \
                or self._screen_sig != self._screens_signature(self._host_name):
            QTimer.singleShot(0, self.rebuild)
            return
        self._apply_look()
        if self.chrome is not None:
            self.chrome.apply_settings()
        page = self._pages.get("dashboard")
        if alive(page) and hasattr(page, "refresh"):
            page.refresh(force=True)
        auto = self._pages.get("automation")
        if alive(auto) and hasattr(auto, "settings_applied"):
            auto.settings_applied()

    def page(self, key: str):
        page = self._pages.get(key)
        return page if alive(page) else None

    def shutdown(self):
        self._destroy()
