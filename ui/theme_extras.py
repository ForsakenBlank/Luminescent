"""
Theme editor pages for the desktop around Luminescent: the Caelestia shell's look (see-through panels, fonts,
sizes, pictures, the lock screen...), glass (how see-through every app is) and the terminal (its colours, how
see-through it is and the picture fastfetch shows when it opens). Everything is per theme and optional: a
setting the theme leaves alone stays as it is, or goes back to what it was before another theme changed it.

    theme["caelestia"]["shell"]   {dotted shell.json key: value}
    theme["caelestia"]["extras"]  {"lyrics_image", "lock_image", "lock_blur"}
    theme["glass"]                {"active", "inactive", "blur_size", "blur_passes"}
    theme["terminal"]             {"colours": {name: hex}, "opacity", "logo", "logo_width", "logo_height",
                                   "logo_padding", "logo_padding_top", "logo_position",
                                   "narrow": {"below", "then"},
                                   "greeting": {"on", "text", "colour", "info", "style", "lines", "args"}}
"""
import os

from PyQt6.QtWidgets import (
    QWidget, QFrame, QVBoxLayout, QHBoxLayout, QGridLayout, QCheckBox, QSlider, QSpinBox, QLineEdit,
    QFontComboBox, QFileDialog, QLabel, QComboBox, QPlainTextEdit
)
from PyQt6.QtCore import Qt, QRectF, QTimer, pyqtSignal
from PyQt6.QtGui import QPainter, QColor, QFont, QPixmap, QPen

from core import theme, widgets, colourpick, terminal, tones, IS_WIN, DATA_DIR

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
            for k in ("logo", "logo_width", "logo_height", "logo_padding", "logo_padding_top", "logo_position"):
                term.pop(k, None)
        if isinstance(term.get("greeting"), dict) and not term["greeting"].get("on"):
            term.pop("greeting", None)
        if not term.get("greeting") and not term.get("logo"):
            term.pop("narrow", None)
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
               "selectionBackground": "Selection", "colour16": "Accent (16)", "colour17": "Second accent (17)",
               "colour18": "Third accent (18)"}
TERM_GROUPS = [
    ("basics", "Basics", ["background", "foreground", "cursorColor", "selectionBackground"]),
    ("extra", "Accents 16 to 18", ["colour16", "colour17", "colour18"]),
    ("normal", "Normal", ["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"]),
    ("bright", "Bright", ["brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightPurple",
                          "brightCyan", "brightWhite"]),
]
SLOT_NAMES = [("colour16", "Accent (16)"), ("colour17", "Second accent (17)"), ("colour18", "Third accent (18)"),
              ("foreground", "Text")] + [(n, n.replace("bright", "bright ").lower().capitalize())
                                         for n in terminal.NAMES[4:20]]
INFO_NAMES = {"os": "Distro", "kernel": "Kernel", "uptime": "Uptime", "shell": "Shell", "packages": "Packages",
              "memory": "Memory", "cpu": "Processor", "gpu": "Graphics card", "wm": "Window manager",
              "terminal": "Terminal", "disk": "Disk", "battery": "Battery", "host": "Computer", "title": "User",
              "localip": "Local IP", "datetime": "Date and time", "colors": "Colour dots"}


def term_label(name: str) -> str:
    return TERM_LABELS.get(name) or name.replace("bright", "bright ").capitalize().replace("Bright ", "Bright ")


def term_values(dlg) -> dict:
    """The terminal colours the theme gives: its own on top of the ones worked out from its colours."""
    term = dlg.t.get("terminal") or {}
    base = _term_base(dlg)
    return {k: (term.get("colours") or {}).get(k, base.get(k)) for k in terminal.NAMES}


def _term_base(dlg) -> dict:
    """What the terminal colours would be from this theme's own colours (it may not be the active theme)."""
    old = theme.P
    try:
        theme.P = dlg.base_palette()
        return terminal._palette_scheme()
    finally:
        theme.P = old


def term_from_caelestia(cols: dict) -> dict:
    """Terminal colours from a Caelestia scheme, the way Caelestia gives them to terminals."""
    out = {}
    pairs = {"foreground": "onSurface", "background": "surface", "cursorColor": "secondary",
             "selectionBackground": "secondaryContainer", "colour16": "primary", "colour17": "secondary",
             "colour18": "tertiary"}
    for k, ck in pairs.items():
        if tones.is_hex(cols.get(ck)):
            out[k] = cols[ck].upper()
    for i, name in enumerate(terminal.NAMES[4:20]):
        # NAMES go black, brightBlack, red, brightRed...: normal colours are term0-7, bright term8-15
        n = i // 2 + (8 if i % 2 else 0)
        if tones.is_hex(cols.get(f"term{n}")):
            out[name] = cols[f"term{n}"].upper()
    return out


def terminal_page(dlg) -> QWidget:
    page, v = dlg._page("Terminal", "Your terminal with this theme: its colours, how see-through it is, and what "
                                    "fish shows when a terminal opens (the text, a picture and the info box).")
    term = _sub(dlg.t, "terminal")
    cols = _sub(term, "colours")
    settings = dlg.launcher.settings

    # where the colours come from, and what stops them
    if not IS_WIN:
        card, c = _card("WHERE YOUR TERMINAL GETS ITS COLOURS")
        status = QVBoxLayout()
        status.setSpacing(6)
        c.addLayout(status)

        def fill_status():
            widgets.clear_layout(status)
            own = bool(settings.get("ui.own_terminal", False))
            kitty = terminal.kitty_installed()
            state = terminal.kitty_include_state() if kitty else "missing"
            fish = terminal.fish_loads_caelestia()

            def line(mark, text, button=None):
                row = QHBoxLayout()
                m = widgets.label(mark, 13, 700, tone="accent" if mark == "✓" else "secondary")
                m.setFixedWidth(18)
                m.setAlignment(Qt.AlignmentFlag.AlignTop)
                row.addWidget(m)
                t = widgets.label(text, 11.5)
                t.setWordWrap(True)
                row.addWidget(t, 1)
                if button is not None:
                    row.addWidget(button, 0, Qt.AlignmentFlag.AlignTop)
                status.addLayout(row)
            if own:
                line("✓", "Your own terminals follow the theme (Settings → Overlay & Interface).")
            else:
                go = widgets.button("Make them follow", "primary")
                go.clicked.connect(lambda: (settings.set("ui.own_terminal", True), settings.save(),
                                            QTimer.singleShot(300, fill_status)))
                line("✕", "Only the terminals Luminescent opens get these colours. Your own kitty windows keep "
                          "theirs until \"your own terminal follows the theme\" is on.", go)
            if kitty and state == "broken":
                fix = widgets.button("Put it right", "")
                fix.clicked.connect(lambda: (_fix_kitty(dlg), fill_status()))
                line("⚠", "kitty.conf has an include line with a typo (luminescent-theme.con instead of .conf), so "
                          "kitty never loads the colours. Luminescent can put it right; a copy of kitty.conf is kept "
                          "first.", fix if own else None)
            elif kitty and own and state == "ok":
                line("✓", "kitty.conf includes luminescent-theme.conf, so new kitty windows load the colours.")
            if fish:
                line("ℹ", "Every new fish shell loads Caelestia's terminal colours on top (your config.fish prints "
                          "~/.local/state/caelestia/sequences.txt). " +
                     ("Luminescent puts this theme's colours in that file too, and sends them to the terminals "
                      "that are already open." if own else
                      "So right now Caelestia's colours win; turn the option on and Luminescent's go there too."))
            line("ℹ", "Open terminals change as soon as you save. How see-through kitty is changes in new windows.")
        fill_status()
        v.addWidget(card)

    # the colours
    card, c = _card("COLOURS", "Left on Theme, a colour is worked out from the theme's own colours.")
    c.addWidget(colour_tools_bar(dlg))
    base = _term_base(dlg)
    swatches: dict[str, colourpick.ColourSwatch] = {}
    for gid, gtitle, names in TERM_GROUPS:
        c.addWidget(widgets.label(gtitle.upper(), 9.5, 600, mono=True, spacing=0.14, tone="muted"))
        if gid == "extra":
            hint = widgets.label("Fish greetings and fastfetch boxes often use these (Caelestia's does).", 10.5,
                                 tone="muted")
            c.addWidget(hint)
        grid = QGridLayout()
        grid.setHorizontalSpacing(10)
        grid.setVerticalSpacing(4)
        for i, name in enumerate(names):
            label = term_label(name)
            sw = colourpick.ColourSwatch(cols.get(name), f"Terminal: {label}", unset_text="Theme", width=110,
                                         live=True)
            sw.setToolTip(f"Theme: {base.get(name, '')}")
            sw.fallback = base.get(name)

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
            lbl = widgets.label(label, 12)
            lbl.setMinimumWidth(96)
            grid.addWidget(lbl, r, col)
            grid.addWidget(sw, r, col + 1)
            grid.addWidget(clear, r, col + 2)
            swatches[name] = sw
        grid.setColumnStretch(2, 1)
        c.addLayout(grid)
    row = QHBoxLayout()
    from_theme = widgets.button("All from the theme", "ghost", "Forget every terminal colour set here")
    from_theme.clicked.connect(lambda: dlg.apply_bulk("term", {k: None for k in list(cols)},
                                                      "Terminal colours from the theme"))
    row.addWidget(from_theme)
    if dlg.cael_values():
        from_cael = widgets.button("Same as Caelestia's", "ghost", "Take the terminal colours this theme gives "
                                                                 "Caelestia")
        from_cael.clicked.connect(lambda: dlg.apply_bulk("term", term_from_caelestia(dlg.cael_values()),
                                                         "Terminal colours from Caelestia"))
        row.addWidget(from_cael)
    row.addStretch()
    c.addLayout(row)
    v.addWidget(card)

    def refresh_swatches():
        for n, sw in swatches.items():
            sw.set_value(cols.get(n))
    dlg._term_refresh = refresh_swatches

    card, c = _card("WINDOW")
    row = SetRow("How solid the background is (kitty)", "percent", None, term.get("opacity"), 1.0)
    row.changed.connect(lambda val: (term.pop("opacity", None) if val is None else
                                     term.__setitem__("opacity", max(0.1, val)), dlg.changed()))
    c.addWidget(row)
    v.addWidget(card)

    # the picture
    card, c = _card("START-UP PICTURE", "fastfetch shows it when a terminal opens. Only its logo setting is "
                                         "changed in your fastfetch config (a copy goes to data/terminal-backups "
                                         "first).")
    pic = SetRow("Picture", "image", None, term.get("logo"), "")

    def set_pic(val):
        if val is None:
            for k in ("logo", "logo_width", "logo_height", "logo_padding", "logo_padding_top", "logo_position"):
                term.pop(k, None)
        else:
            term["logo"] = val
            term.setdefault("logo_width", 30)
        for w in pic_rows:
            w.setEnabled(val is not None)
        dlg.changed()
    pic.changed.connect(set_pic)
    c.addWidget(pic)
    pic_rows = []
    for key, label, rng, default, hint in (
            ("logo_width", "Width (columns)", (4, 120), 30, ""),
            ("logo_height", "Height (rows, 0 = keep its shape)", (0, 60), 0, ""),
            ("logo_padding", "Space either side", (0, 12), 2, ""),
            ("logo_padding_top", "Space above", (0, 12), 0, "")):
        row = QHBoxLayout()
        row.setContentsMargins(0, 0, 0, 0)
        row.addWidget(widgets.label(label, 12), 1)
        spin = QSpinBox()
        spin.setRange(*rng)
        spin.setValue(int(term.get(key, default)))
        spin.valueChanged.connect(lambda val, k=key: (term.__setitem__(k, val), dlg.changed()))
        row.addWidget(spin)
        box = QWidget()
        box.setLayout(row)
        c.addWidget(box)
        pic_rows.append(box)
    row = QHBoxLayout()
    row.setContentsMargins(0, 0, 0, 0)
    row.addWidget(widgets.label("Where it goes", 12), 1)
    pos = QComboBox()
    for key, label in (("left", "Left of the info"), ("top", "Above the info"), ("right", "Right of the info"),
                       ("auto", "Left, or above when narrow (fastfetch decides)")):
        pos.addItem(label, key)
    pos.setCurrentIndex(max(0, pos.findData(term.get("logo_position", "left"))))
    pos.currentIndexChanged.connect(lambda _i: (term.__setitem__("logo_position", pos.currentData()),
                                                dlg.changed()))
    row.addWidget(pos)
    box = QWidget()
    box.setLayout(row)
    c.addWidget(box)
    pic_rows.append(box)
    for w in pic_rows:
        w.setEnabled(bool(term.get("logo")))
    v.addWidget(card)

    # the start-up text and the info box
    g = term.get("greeting") if isinstance(term.get("greeting"), dict) else {}
    card, c = _card("WHEN A TERMINAL OPENS (FISH)", "Luminescent runs the start-up for fish: your text, then "
                                                    "fastfetch, made smaller when the terminal is narrow so nothing "
                                                    "wraps. Your own fish_greeting file is not changed; Luminescent "
                                                    "adds conf.d/luminescent-greeting.fish, which takes its place "
                                                    "while this is on.")
    on = QCheckBox("Luminescent runs the start-up with this theme")
    on.setChecked(bool(g.get("on")))
    c.addWidget(on)
    body = QWidget()
    bl = QVBoxLayout(body)
    bl.setContentsMargins(0, 4, 0, 0)
    bl.setSpacing(6)
    c.addWidget(body)

    def greet() -> dict:
        d = term.get("greeting")
        if not isinstance(d, dict):
            d = term["greeting"] = {}
        return d

    def set_on(val):
        greet()["on"] = bool(val)
        if val and "text" not in greet():
            text, args = terminal.own_greeting()
            greet().update({"text": text, "args": args})
            text_edit.setPlainText(text)
            args_edit.setText(args)
        body.setEnabled(bool(val))
        dlg.changed()
    bl.addWidget(widgets.label("Text at the top (leave empty for none)", 12))
    text_edit = QPlainTextEdit(g.get("text", ""))
    text_edit.setFont(QFont(theme.MONO_FAMILY, 10))
    text_edit.setFixedHeight(120)
    text_edit.setLineWrapMode(QPlainTextEdit.LineWrapMode.NoWrap)
    text_edit.textChanged.connect(lambda: (greet().__setitem__("text", text_edit.toPlainText()), dlg.changed()))
    bl.addWidget(text_edit)
    row = QHBoxLayout()
    take = widgets.button("Take my current greeting's text", "ghost",
                          "The echo lines of ~/.config/fish/functions/fish_greeting.fish")
    take.clicked.connect(lambda: text_edit.setPlainText(terminal.own_greeting()[0]))
    row.addWidget(take)
    row.addStretch()
    row.addWidget(widgets.label("Its colour", 12))
    colour = QComboBox()
    for key, label in SLOT_NAMES:
        colour.addItem(label, key)
    colour.setCurrentIndex(max(0, colour.findData(g.get("colour", "colour16"))))
    colour.currentIndexChanged.connect(lambda _i: (greet().__setitem__("colour", colour.currentData()),
                                                   dlg.changed()))
    row.addWidget(colour)
    bl.addLayout(row)

    row = QHBoxLayout()
    row.addWidget(widgets.label("Info box", 12), 1)
    info = QComboBox()
    for key, label in (("own", "Your own fastfetch setup"), ("luminescent", "Luminescent's (pick the lines below)"),
                       ("none", "None")):
        info.addItem(label, key)
    info.setCurrentIndex(max(0, info.findData(g.get("info", "own"))))
    row.addWidget(info)
    bl.addLayout(row)
    row = QHBoxLayout()
    row.addWidget(widgets.label("Extra fastfetch options", 12), 1)
    args_edit = QLineEdit(g.get("args", ""))
    args_edit.setPlaceholderText("e.g. --key-padding-left 5")
    args_edit.textChanged.connect(lambda txt: (greet().__setitem__("args", txt.strip()), dlg.changed()))
    row.addWidget(args_edit, 1)
    bl.addLayout(row)

    # narrow terminals
    bl.addWidget(widgets.label("NARROW TERMINALS", 9.5, 600, mono=True, spacing=0.14, tone="muted"))
    nar = term.get("narrow") if isinstance(term.get("narrow"), dict) else {}
    row = QHBoxLayout()
    row.addWidget(widgets.label("When it is narrower than", 12))
    below = QSpinBox()
    below.setRange(20, 300)
    below.setSuffix(" columns")
    below.setValue(int(nar.get("below", 90)))
    row.addWidget(below)
    then = QComboBox()
    for key, label in (("hide", "leave the picture out"), ("top", "put a smaller picture above"),
                       ("text", "show only the text")):
        then.addItem(label, key)
    then.setCurrentIndex(max(0, then.findData(nar.get("then", "hide"))))
    row.addWidget(then)
    row.addStretch()
    bl.addLayout(row)
    hint = widgets.label("Four terminals side by side on a 1920 px screen are about 45 to 60 columns each.", 10.5,
                         tone="muted")
    bl.addWidget(hint)

    def set_narrow(*_a):
        term["narrow"] = {"below": below.value(), "then": then.currentData()}
        dlg.changed()
    below.valueChanged.connect(set_narrow)
    then.currentIndexChanged.connect(set_narrow)

    # Luminescent's info lines
    lines_box = QWidget()
    ll = QVBoxLayout(lines_box)
    ll.setContentsMargins(0, 4, 0, 0)
    ll.setSpacing(4)
    ll.addWidget(widgets.label("INFO LINES", 9.5, 600, mono=True, spacing=0.14, tone="muted"))
    row = QHBoxLayout()
    row.addWidget(widgets.label("Style", 12), 1)
    style = QComboBox()
    style.addItem("A box down the side", "box")
    style.addItem("Plain lines", "plain")
    style.setCurrentIndex(max(0, style.findData(g.get("style", "box"))))
    style.currentIndexChanged.connect(lambda _i: (greet().__setitem__("style", style.currentData()), dlg.changed()))
    row.addWidget(style)
    ll.addLayout(row)
    grid = QGridLayout()
    grid.setHorizontalSpacing(8)
    grid.setVerticalSpacing(3)
    have = {ln.get("type"): ln for ln in (g.get("lines") or []) if isinstance(ln, dict)}
    if not have:
        have = {t: {"type": t, "label": dict(terminal.INFO_MODULES)[t], "colour": "colour16"}
                for t in terminal.DEFAULT_INFO}
    order = [t for t in have] + [t for t, _l in terminal.INFO_MODULES if t not in have]
    ctl: dict[str, tuple] = {}

    def save_lines():
        greet()["lines"] = [{"type": t, "label": ctl[t][1].text(), "colour": ctl[t][2].currentData()}
                            for t in order if ctl[t][0].isChecked()]
        dlg.changed()
    for i, t in enumerate(order):
        ln = have.get(t, {"label": dict(terminal.INFO_MODULES)[t], "colour": "colour16"})
        tick = QCheckBox(INFO_NAMES.get(t, t))
        tick.setChecked(t in have)
        label = QLineEdit(ln.get("label", ""))
        label.setPlaceholderText("label")
        label.setFixedWidth(110)
        col = QComboBox()
        for key, name in SLOT_NAMES:
            col.addItem(name, key)
        col.setCurrentIndex(max(0, col.findData(ln.get("colour", "colour16"))))
        tick.toggled.connect(lambda _on: save_lines())
        label.textChanged.connect(lambda _t: save_lines())
        col.currentIndexChanged.connect(lambda _i: save_lines())
        grid.addWidget(tick, i, 0)
        grid.addWidget(label, i, 1)
        grid.addWidget(col, i, 2)
        ctl[t] = (tick, label, col)
    grid.setColumnStretch(0, 1)
    ll.addLayout(grid)
    bl.addWidget(lines_box)

    def set_info(_i=0):
        greet()["info"] = info.currentData()
        lines_box.setVisible(info.currentData() == "luminescent")
        args_edit.setEnabled(info.currentData() != "none")
        if info.currentData() == "luminescent" and not greet().get("lines"):
            save_lines()
        dlg.changed()
    info.currentIndexChanged.connect(set_info)
    lines_box.setVisible(g.get("info", "own") == "luminescent")
    on.toggled.connect(set_on)
    body.setEnabled(bool(g.get("on")))
    v.addWidget(card)
    v.addStretch()
    return page


def _fix_kitty(dlg):
    err = terminal.link_kitty(os.path.join(DATA_DIR, "terminal-backups"))
    if err:
        widgets.error(dlg, "kitty.conf", err)
    else:
        dlg.launcher.notify("Terminal", "kitty.conf now includes luminescent-theme.conf (a copy of the old one is in "
                                        "data/terminal-backups).")


def colour_tools_bar(dlg) -> QWidget:
    """The quick tools for the terminal's colours."""
    from ui import colour_tools
    box = QWidget()
    row = QHBoxLayout(box)
    row.setContentsMargins(0, 0, 0, 4)
    row.setSpacing(6)
    for key in ("main", "harmony", "adjust", "follow", "flip"):
        text, tip = next((t, d) for k, t, d in colour_tools.TOOLS if k == key)
        b = widgets.button(text, "ghost", tip)
        b.clicked.connect(lambda _c=False, k=key: colour_tools.open_tool(dlg, "term", k))
        row.addWidget(b)
    row.addStretch()
    return box


# ── previews ───────────────────────────────────────────────────────────────
def paint_terminal(qp: QPainter, w: float, y: float, dlg):
    term = dlg.t.get("terminal") or {}
    s = term_values(dlg)
    h = draw_terminal(qp, QRectF(0, y, w, 300), s, term)
    g = term.get("greeting") or {}
    nar = term.get("narrow") or {}
    if g.get("on"):
        y += h + 14
        nw = min(w, max(180.0, w * 0.6))
        f = QFont(theme.MONO_FAMILY)
        f.setPixelSize(10)
        qp.setFont(f)
        qp.setPen(QColor(dlg.base_palette().text_muted))
        qp.drawText(QRectF(0, y, w, 14), Qt.AlignmentFlag.AlignLeft,
                    f"NARROWER THAN {nar.get('below', 90)} COLUMNS")
        draw_terminal(qp, QRectF(0, y + 18, nw, 260), s, term, narrow=nar.get("then", "hide"))


def draw_terminal(qp: QPainter, r: QRectF, s: dict, term: dict, narrow: str = "") -> float:
    """A terminal in these colours with the start-up as the theme sets it. Returns the height drawn."""
    g = term.get("greeting") or {}
    bg = QColor(s["background"])
    bg.setAlphaF(float(term.get("opacity", 1.0)))
    f = QFont(theme.MONO_FAMILY)
    f.setPixelSize(10 if narrow else 11)
    qp.setFont(f)
    fm = qp.fontMetrics()
    lh = fm.height() + 1
    x0, y = r.left() + 12, r.top() + 12
    lines = []                                   # (x, y, text, colour)
    dots = []                                    # (rect, colour): the colour-dots line
    if g.get("on") and g.get("text") and narrow != "x":
        tc = s.get(g.get("colour", "colour16"), s["foreground"])
        for ln in g["text"].split("\n")[:8]:
            lines.append((x0, y, ln, tc))
            y += lh
        y += 4
    info = g.get("info", "own") if g.get("on") else "own"
    show_info = not (g.get("on") and (info == "none" or narrow == "text"))
    logo = os.path.expanduser(term.get("logo") or "")
    px = QPixmap(logo) if logo and os.path.isfile(logo) and narrow != "hide" and narrow != "text" else QPixmap()
    pos = term.get("logo_position", "left")
    if narrow == "top":
        pos = "top"
    cw = fm.horizontalAdvance("M")
    pic_rect = None
    if show_info and not px.isNull():
        lw = int(term.get("logo_width", 30)) * cw
        if narrow == "top":
            lw = min(lw, r.width() - 24)
        px = px.scaled(int(lw), int(lw), Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation)
        pad = int(term.get("logo_padding", 2)) * cw
        top = y + int(term.get("logo_padding_top", 0)) * lh
        if pos == "top":
            pic_rect = QRectF(x0 + pad, top, px.width(), px.height())
            y = pic_rect.bottom() + 6
            ix = x0
        elif pos == "right":
            pic_rect = QRectF(r.right() - 12 - pad - px.width(), top, px.width(), px.height())
            ix = x0
        else:
            pic_rect = QRectF(x0 + pad, top, px.width(), px.height())
            ix = pic_rect.right() + pad
    else:
        ix = x0
    if show_info:
        if g.get("on") and info == "luminescent":
            items = [(ln.get("label", ""), ln.get("colour", "colour16"), ln.get("type")) for ln in g.get("lines") or []]
        else:
            items = [("kernel", "brightBlue", "kernel"), ("shell", "green", "shell"), ("mem", "yellow", "memory"),
                     ("uptime", "colour16", "uptime")]
        sample = {"os": "CachyOS x86_64", "kernel": "Linux 6.18", "uptime": "4 hours", "shell": "fish 4.0",
                  "packages": "1394 (pacman)", "memory": "6.2 / 31 GiB", "cpu": "Ryzen 7", "gpu": "Radeon",
                  "wm": "Hyprland", "terminal": "kitty", "disk": "120 / 930 GiB", "battery": "80 %",
                  "host": "Desktop", "title": "blank@cachyos", "localip": "192.168.1.4", "datetime": "12:45"}
        box = g.get("on") and info == "luminescent" and g.get("style", "box") == "box"
        iy = y if pos != "left" or pic_rect is None else pic_rect.top()
        if box:
            lines.append((ix, iy, "╭" + "─" * 18, s.get(g.get("colour", "colour16"), s["foreground"])))
            iy += lh
        for label, col, typ in items:
            if typ == "colors":
                for i, n in enumerate(terminal.NAMES[4:20:2]):
                    dots.append((QRectF(ix + (2 if box else 0) * cw + i * (cw + 4), iy + 2, cw, cw), s[n]))
                iy += lh
                continue
            key = ("│ " if box else "") + label.ljust(7)
            lines.append((ix, iy, key, s.get(col, s["foreground"])))
            lines.append((ix + fm.horizontalAdvance(key + "  "), iy, sample.get(typ, ""), s["foreground"]))
            iy += lh
        if box:
            lines.append((ix, iy, "╰" + "─" * 18, s.get(g.get("colour", "colour16"), s["foreground"])))
            iy += lh
        y = max(iy, pic_rect.bottom() if pic_rect is not None else 0) + 6
    # the 16 colours and a prompt
    rows_y = y
    bw = (r.width() - 24) / 8
    y += 2 * 20 + 6
    py = y
    y += lh + 10
    h = y - r.top()
    box_r = QRectF(r.left(), r.top(), r.width(), h)
    qp.setPen(QPen(QColor(s["brightBlack"]), 1))
    qp.setBrush(bg)
    qp.drawRoundedRect(box_r.adjusted(0.5, 0.5, -0.5, -0.5), 12, 12)
    qp.save()
    qp.setClipRect(box_r.adjusted(4, 4, -4, -4))
    if pic_rect is not None:
        qp.drawPixmap(pic_rect.topLeft(), px)
    for lx, ly, text, col in lines:
        qp.setPen(QColor(col))
        qp.drawText(QRectF(lx, ly, r.right() - lx, lh), Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                    text)
    qp.setPen(Qt.PenStyle.NoPen)
    for rect, col in dots:
        qp.setBrush(QColor(col))
        qp.drawEllipse(rect)
    for row, pre in enumerate(("", "bright")):
        for i, n in enumerate(["black", "red", "green", "yellow", "blue", "purple", "cyan", "white"]):
            key = n if not pre else pre + n[0].upper() + n[1:]
            qp.fillRect(QRectF(r.left() + 12 + i * bw, rows_y + row * 20, bw - 4, 16), QColor(s[key]))
    qp.setPen(QColor(s["green"]))
    qp.drawText(QRectF(r.left() + 12, py, 30, lh), Qt.AlignmentFlag.AlignVCenter, "❯")
    qp.setPen(QColor(s["foreground"]))
    qp.drawText(QRectF(r.left() + 28, py, 200, lh), Qt.AlignmentFlag.AlignVCenter, "ls ~/Pictures")
    qp.fillRect(QRectF(r.left() + 28 + fm.horizontalAdvance("ls ~/Pictures") + 3, py + 2, cw * 0.8, lh - 4),
                QColor(s["cursorColor"]))
    qp.restore()
    return h


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
