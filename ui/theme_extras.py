"""
Theme editor pages for the desktop around Luminescent: the Caelestia shell's look (see-through panels, fonts,
sizes, pictures, the lock screen...), glass (how see-through every app is) and the terminal (its colours, how
see-through it is and the picture fastfetch shows when it opens). Everything is per theme and optional: a
setting the theme leaves alone stays as it is, or goes back to what it was before another theme changed it.

    theme["caelestia"]["shell"]   {dotted shell.json key: value}
    theme["caelestia"]["extras"]  {"lyrics_image", "lock_image", "lock_blur"}
    theme["glass"]                {"active", "inactive", "blur_size", "blur_passes"}
    theme["terminal"]             {"colours": {name: hex}, "opacity", "logo", "logo_width"}
"""
import os

from PyQt6.QtWidgets import (
    QWidget, QFrame, QVBoxLayout, QHBoxLayout, QGridLayout, QCheckBox, QSlider, QSpinBox, QLineEdit,
    QFontComboBox, QFileDialog, QLabel
)
from PyQt6.QtCore import Qt, QRectF, pyqtSignal
from PyQt6.QtGui import QPainter, QColor, QFont, QPixmap, QPen

from core import theme, widgets, colourpick, terminal

PICTURES = "Pictures and GIFs (*.png *.jpg *.jpeg *.webp *.gif *.svg *.bmp);;All files (*)"


class SetRow(QWidget):
    """One optional setting: tick it to have the theme set it, then pick the value. Unticked it shows what is
    there now, greyed out. changed(value) with None when it is unticked."""
    changed = pyqtSignal(object)

    def __init__(self, label: str, kind: str, extra, value, current, hint: str = ""):
        super().__init__()
        self.kind, self.extra = kind, extra
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 2, 0, 2)
        lay.setSpacing(10)
        self.on = QCheckBox(label)
        self.on.setChecked(value is not None)
        if hint:
            self.on.setToolTip(hint)
        lay.addWidget(self.on, 1)
        start = value if value is not None else current
        if kind == "bool":
            self.ctl = QCheckBox("On")
            self.ctl.setChecked(bool(start))
            self.ctl.toggled.connect(lambda _on: self._emit())
            lay.addWidget(self.ctl)
        elif kind in ("real", "blur", "percent"):
            lo, hi, step = extra if kind == "real" else (0, 1, 0.01)
            self.scale = 1 / step
            self.ctl = QSlider(Qt.Orientation.Horizontal)
            self.ctl.setRange(int(round(lo * self.scale)), int(round(hi * self.scale)))
            self.ctl.setFixedWidth(170)
            try:
                self.ctl.setValue(int(round(float(start if start is not None else lo) * self.scale)))
            except (TypeError, ValueError):
                self.ctl.setValue(int(round(lo * self.scale)))
            self.val = widgets.label("", 11.5, mono=True)
            self.val.setFixedWidth(46)
            self._show_val()
            self.ctl.valueChanged.connect(lambda _v: (self._show_val(), self._emit()))
            lay.addWidget(self.ctl)
            lay.addWidget(self.val)
        elif kind == "int":
            self.ctl = QSpinBox()
            self.ctl.setRange(*extra)
            self.ctl.setValue(int(start or 0))
            self.ctl.valueChanged.connect(lambda _v: self._emit())
            lay.addWidget(self.ctl)
        elif kind == "font":
            self.ctl = QFontComboBox()
            self.ctl.setFixedWidth(220)
            if start:
                self.ctl.setCurrentFont(QFont(str(start)))
            self.ctl.currentFontChanged.connect(lambda _f: self._emit())
            lay.addWidget(self.ctl)
        else:                                       # image
            self.ctl = QLineEdit(str(start or ""))
            self.ctl.setPlaceholderText("A picture or GIF")
            self.ctl.setMinimumWidth(200)
            self.ctl.editingFinished.connect(self._emit)
            self.thumb = QLabel()
            self.thumb.setFixedSize(34, 34)
            self.thumb.setScaledContents(True)
            self._show_thumb()
            browse = widgets.button("Browse…")
            browse.clicked.connect(self._browse)
            self.browse = browse
            lay.addWidget(self.thumb)
            lay.addWidget(self.ctl, 2)
            lay.addWidget(browse)
        self.on.toggled.connect(lambda _on: (self._enable(), self._emit()))
        self._enable()

    def _enable(self):
        for w in (self.ctl, getattr(self, "browse", None)):
            if w is not None:
                w.setEnabled(self.on.isChecked())

    def _show_val(self):
        v = self.ctl.value() / self.scale
        self.val.setText(f"{int(v * 100)} %" if self.kind in ("blur", "percent") else
                         (f"{v:.0f}" if self.scale <= 1 else f"{v:.2f}".rstrip("0").rstrip(".")))

    def _show_thumb(self):
        p = os.path.expanduser(self.ctl.text().strip())
        px = QPixmap(p) if p and os.path.isfile(p) else QPixmap()
        self.thumb.setPixmap(px)
        self.thumb.setVisible(not px.isNull())

    def _browse(self):
        p, _ = QFileDialog.getOpenFileName(self, "Choose a picture", self.ctl.text(), PICTURES)
        if p:
            self.ctl.setText(p)
            self._emit()

    def value(self):
        if not self.on.isChecked():
            return None
        if self.kind == "bool":
            return self.ctl.isChecked()
        if self.kind in ("real", "blur", "percent"):
            return round(self.ctl.value() / self.scale, 3)
        if self.kind == "int":
            return self.ctl.value()
        if self.kind == "font":
            return self.ctl.currentFont().family()
        self._show_thumb()
        return self.ctl.text().strip() or None

    def _emit(self):
        self.changed.emit(self.value())


def _card(title: str, hint: str = "") -> tuple[QFrame, QVBoxLayout]:
    card = QFrame()
    card.setObjectName("lumCard")
    c = QVBoxLayout(card)
    c.setContentsMargins(16, 12, 16, 14)
    c.setSpacing(6)
    c.addWidget(widgets.label(title, 10.5, 600, mono=True, spacing=0.14, tone="accent"))
    if hint:
        h = widgets.label(hint, 11, tone="muted")
        h.setWordWrap(True)
        c.addWidget(h)
    return card, c


def _sub(t: dict, *path) -> dict:
    d = t
    for p in path:
        if not isinstance(d.get(p), dict):
            d[p] = {}
        d = d[p]
    return d


def tidy(t: dict):
    """Empty groups out of a theme before it is saved, so a theme that sets nothing carries nothing."""
    c = t.get("caelestia")
    if isinstance(c, dict):
        for k in ("shell", "extras", "colours"):
            if not c.get(k):
                c.pop(k, None)
        if not c:
            t.pop("caelestia", None)
    term = t.get("terminal")
    if isinstance(term, dict):
        if not term.get("colours"):
            term.pop("colours", None)
        if not term.get("logo"):
            term.pop("logo", None)
            term.pop("logo_width", None)
        if not term:
            t.pop("terminal", None)
    if isinstance(t.get("glass"), dict) and not t["glass"]:
        t.pop("glass", None)


# ── Caelestia look ─────────────────────────────────────────────────────────
def caelestia_look_page(dlg) -> QWidget:
    from modules import caelestia_look as look
    page, v = dlg._page("Caelestia look",
                        "How the Caelestia shell looks with this theme, besides its colours. Tick a setting to have "
                        "the theme set it; whatever is left unticked stays as it is, or goes back to what it was "
                        "before another theme changed it.")
    if not look.available():
        v.addWidget(widgets.label("Caelestia's shell.json was not found (is Caelestia installed?).", 12,
                                  tone="secondary"))
        v.addStretch()
        return page
    shell = _sub(dlg.t, "caelestia", "shell")
    now = look.read_shell()

    def setter(key):
        def go(val):
            if val is None:
                shell.pop(key, None)
            else:
                shell[key] = val
            dlg.changed()
        return go

    for title, rows in look.OPTIONS:
        card, c = _card(title)
        for key, label, kind, extra in rows:
            cur = look.get(now, key, None)
            row = SetRow(label, kind, extra, shell.get(key), cur)
            row.changed.connect(setter(key))
            c.addWidget(row)
        v.addWidget(card)

    extras = _sub(dlg.t, "caelestia", "extras")
    mod = dlg.launcher.modules.get("caelestia")
    on = bool(mod.s("extras", False)) if mod is not None else False
    card, c = _card("EXTRAS", "Caelestia has no setting for these, so Luminescent adds them with a patched copy of "
                              "two of its files (in ~/.config/quickshell/caelestia, made of links to everything "
                              "else; Caelestia's own files are never changed). Turning them on or off restarts "
                              "Caelestia once.")
    state = widgets.label("", 11.5, tone="secondary")
    state.setWordWrap(True)
    c.addWidget(state)
    toggle = widgets.button("", "")
    c.addWidget(toggle, 0, Qt.AlignmentFlag.AlignLeft)

    def show_state():
        st = look.mirror_state()
        is_on = bool(mod.s("extras", False)) if mod is not None else False
        if st == "foreign":
            state.setText(f"{look.MIRROR} is your own copy of the shell, so Luminescent leaves it alone and the "
                          "extras are off.")
            toggle.hide()
            return
        state.setText("The extras are on." if is_on and st == "on" else
                      "The extras are off: these pictures are saved with the theme and show once they are on.")
        toggle.setText("Turn the extras off" if is_on else "Turn the extras on")
        toggle.show()

    def flip():
        if mod is None:
            return
        want = not bool(mod.s("extras", False))
        if not widgets.confirm(dlg, "Caelestia extras",
                               ("Luminescent makes ~/.config/quickshell/caelestia (links to Caelestia's files plus "
                                "two patched copies) and restarts Caelestia once." if want else
                                "Luminescent removes its ~/.config/quickshell/caelestia and restarts Caelestia "
                                "once, so it runs exactly as installed.") + " Go ahead?", yes="Do it"):
            return
        err = mod.set_extras(want)
        if err:
            widgets.error(dlg, "Caelestia extras", err)
        show_state()

    toggle.clicked.connect(flip)
    show_state()

    def extra_setter(key):
        def go(val):
            if val is None:
                extras.pop(key, None)
            else:
                extras[key] = val
            dlg.changed()
        return go

    for key, label, kind in look.EXTRAS:
        row = SetRow(label, kind, None, extras.get(key), look.EXTRA_DEFAULTS.get(key))
        row.changed.connect(extra_setter(key))
        c.addWidget(row)
    v.addWidget(card)

    card, c = _card("TRY IT")
    row = QHBoxLayout()
    lock = widgets.button("Lock the screen now", "", "Caelestia's lock screen, the one after a while away. Your "
                                                     "password unlocks it.")
    lock.clicked.connect(lambda: _lock(dlg, mod))
    row.addWidget(lock)
    row.addStretch()
    c.addLayout(row)
    hint = widgets.label("Save first: the lock screen shows the theme that is applied.", 11, tone="muted")
    c.addWidget(hint)
    v.addWidget(card)
    v.addStretch()
    return page


def _lock(dlg, mod):
    if mod is None:
        return
    if widgets.confirm(dlg, "Lock the screen", "Lock the screen now? Type your password to come back.",
                       yes="Lock"):
        mod.lock_screen()


# ── glass ──────────────────────────────────────────────────────────────────
def glass_page(dlg) -> QWidget:
    page, v = dlg._page("Glass", "How see-through every app is with this theme (apps with a row of their own in "
                                 "Caelestia & Glass keep it), and how strong the blur behind them is. Unticked "
                                 "settings use the ones in Caelestia & Glass.")
    g = _sub(dlg.t, "glass")
    mod = dlg.launcher.modules.get("caelestia")
    card, c = _card("EVERY APP")
    for key, label, cur in (("active", "Focused (the mouse is on it)", (mod.s("all_active", 0) if mod else 0) or 1.0),
                            ("inactive", "Not focused", (mod.s("all_inactive", 0) if mod else 0) or 1.0)):
        row = SetRow(label, "percent", None, g.get(key), cur)
        row.changed.connect(lambda val, k=key: (g.pop(k, None) if val is None else g.__setitem__(k, max(0.2, val)),
                                                dlg.changed()))
        c.addWidget(row)
    v.addWidget(card)
    card, c = _card("LUMINESCENT", "Notes, the other panels and notifications.")
    row = SetRow("Glass: blur what is behind them", "bool", None, g.get("luminescent"),
                 bool(dlg.launcher.settings.get("ui.panel_glass", False)))
    row.changed.connect(lambda val: (g.pop("luminescent", None) if val is None else g.__setitem__("luminescent", val),
                                     dlg.changed()))
    c.addWidget(row)
    v.addWidget(card)
    card, c = _card("BLUR")
    for key, label, rng, cur in (("blur_size", "Size", (1, 30), mod.s("blur_size", 0) if mod else 0),
                                 ("blur_passes", "Passes", (1, 6), mod.s("blur_passes", 0) if mod else 0)):
        row = SetRow(label, "int", rng, g.get(key), cur or rng[0])
        row.changed.connect(lambda val, k=key: (g.pop(k, None) if val is None else g.__setitem__(k, val),
                                                dlg.changed()))
        c.addWidget(row)
    v.addWidget(card)
    v.addStretch()
    return page


# ── terminal ───────────────────────────────────────────────────────────────
TERM_LABELS = {"background": "Background", "foreground": "Text", "cursorColor": "Cursor",
               "selectionBackground": "Selection"}


def terminal_page(dlg) -> QWidget:
    page, v = dlg._page("Terminal", "The terminal with this theme: kitty windows (once Settings → Overlay & "
                                    "Interface → Your own terminal follows the theme is on) and the ones Luminescent "
                                    "opens. Colours left on Theme are worked out from the theme's colours.")
    term = _sub(dlg.t, "terminal")
    cols = _sub(term, "colours")
    base = terminal._palette_scheme()
    card, c = _card("COLOURS")
    grid = QGridLayout()
    grid.setHorizontalSpacing(10)
    grid.setVerticalSpacing(6)
    for i, name in enumerate(terminal.NAMES):
        label = TERM_LABELS.get(name) or name.replace("bright", "bright ").capitalize().replace("Bright ", "Bright ")
        sw = colourpick.ColourSwatch(cols.get(name), f"Terminal: {label}", unset_text="Theme", width=110)
        sw.setToolTip(f"Theme: {base.get(name, '')}")

        def set_col(val, n=name):
            if isinstance(val, str) and val:
                cols[n] = val.upper()
            else:
                cols.pop(n, None)
            dlg.changed()
        sw.changed.connect(set_col)
        clear = widgets.icon_button("✕", "Back to the theme's", 22, 10, ghost=True)
        clear.clicked.connect(lambda _c=False, s=sw, n=name: (s.set_value(None), cols.pop(n, None), dlg.changed()))
        r, col = i // 2, (i % 2) * 3
        grid.addWidget(widgets.label(label, 12), r, col)
        grid.addWidget(sw, r, col + 1)
        grid.addWidget(clear, r, col + 2)
    c.addLayout(grid)
    v.addWidget(card)

    card, c = _card("WINDOW")
    row = SetRow("How solid the background is (kitty)", "percent", None, term.get("opacity"), 1.0)
    row.changed.connect(lambda val: (term.pop("opacity", None) if val is None else
                                     term.__setitem__("opacity", max(0.1, val)), dlg.changed()))
    c.addWidget(row)
    v.addWidget(card)

    card, c = _card("START-UP PICTURE", "fastfetch shows it when a terminal opens (only its logo setting is "
                                         "changed; a copy of its config goes to data/terminal-backups first).")
    pic = SetRow("Picture", "image", None, term.get("logo"), "")
    width = SetRow("Its width (columns)", "int", (8, 80), term.get("logo_width") if term.get("logo") else None, 30)

    def set_pic(val):
        if val is None:
            term.pop("logo", None)
            term.pop("logo_width", None)
        else:
            term["logo"] = val
            term.setdefault("logo_width", 30)
        dlg.changed()
    pic.changed.connect(set_pic)
    width.changed.connect(lambda val: (term.__setitem__("logo_width", val or 30) if term.get("logo") else None,
                                       dlg.changed()))
    c.addWidget(pic)
    c.addWidget(width)
    v.addWidget(card)
    v.addStretch()
    return page


# ── previews ───────────────────────────────────────────────────────────────
def paint_terminal(qp: QPainter, w: float, y: float, dlg):
    term = dlg.t.get("terminal") or {}
    s = {**terminal._palette_scheme(), **(term.get("colours") or {})}
    r = QRectF(0, y, w, 260)
    bg = QColor(s["background"])
    bg.setAlphaF(float(term.get("opacity", 1.0)))
    qp.setPen(QPen(QColor(s["brightBlack"]), 1))
    qp.setBrush(bg)
    qp.drawRoundedRect(r.adjusted(0.5, 0.5, -0.5, -0.5), 12, 12)
    logo = os.path.expanduser(term.get("logo") or "")
    x0 = 16
    if logo and os.path.isfile(logo):
        px = QPixmap(logo)
        if not px.isNull():
            px = px.scaled(84, 84, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation)
            qp.drawPixmap(int(16), int(y + 18), px)
            x0 = 16 + px.width() + 14
    f = QFont("monospace")
    f.setPixelSize(12)
    qp.setFont(f)
    lines = [("brightBlue", "kernel  "), ("foreground", "6.18"), ("green", "shell   "), ("foreground", "fish"),
             ("yellow", "mem     "), ("foreground", "6.2 / 31 GiB")]
    yy = y + 32
    for i in range(0, len(lines), 2):
        qp.setPen(QColor(s[lines[i][0]]))
        qp.drawText(QRectF(x0, yy - 12, 70, 16), lines[i][1])
        qp.setPen(QColor(s[lines[i + 1][0]]))
        qp.drawText(QRectF(x0 + 66, yy - 12, w - x0 - 70, 16), lines[i + 1][1])
        yy += 20
    yy = max(yy, y + 120)
    order = ["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"]
    bw = (w - 32) / 8
    for row, pre in enumerate(("", "bright")):
        for i, n in enumerate(order):
            key = n if not pre else pre + n[0].upper() + n[1:]
            qp.fillRect(QRectF(16 + i * bw, yy + row * 22, bw - 4, 18), QColor(s[key]))
    yy += 54
    qp.setPen(QColor(s["green"]))
    qp.drawText(QRectF(16, yy, 30, 16), "❯")
    qp.setPen(QColor(s["foreground"]))
    qp.drawText(QRectF(32, yy, 200, 16), "ls ~/Pictures")
    qp.fillRect(QRectF(132, yy + 1, 8, 15), QColor(s["cursorColor"]))


def paint_glass(qp: QPainter, w: float, y: float, dlg):
    base = dlg.base_palette()
    g = dlg.t.get("glass") or {}
    mod = dlg.launcher.modules.get("caelestia")
    act = float(g.get("active", (mod.s("all_active", 0) if mod else 0) or 1.0))
    ina = float(g.get("inactive", (mod.s("all_inactive", 0) if mod else 0) or 1.0))
    # a striped "wallpaper" so you can see through the windows
    wall = QRectF(0, y, w, 240)
    qp.fillRect(wall, QColor(base.primary_muted))
    qp.setPen(Qt.PenStyle.NoPen)
    for i in range(0, int(w) + 240, 28):
        qp.setBrush(QColor(base.primary))
        qp.drawRect(QRectF(i - 120, y, 10, 240))
    for (rx, ry, op, label) in ((18, 28, ina, "not focused"), (w * 0.38, 70, act, "focused")):
        win = QRectF(rx, y + ry, w * 0.55, 140)
        c = QColor(base.bg_card)
        c.setAlphaF(op)
        qp.setBrush(c)
        qp.setPen(QPen(QColor(base.border), 1))
        qp.drawRoundedRect(win, 10, 10)
        qp.setPen(QColor(base.text_primary))
        f = QFont(base.font_family if hasattr(base, "font_family") else "sans")
        f.setPixelSize(12)
        qp.setFont(f)
        qp.drawText(win.adjusted(12, 10, -12, -10), Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop,
                    f"{label} · {int(op * 100)} %")
