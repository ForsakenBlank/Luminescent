"""
Hyprland integration: the compositor does what Win32 did on Windows.

Wayland does not let a program place its own windows, read the cursor outside
its windows, grab the screen or see keys pressed in other programs.  Hyprland
can do all of that for us through its control socket, so this module:

  * talks to the socket directly (a request is ~1 ms; no hyprctl process),
  * keeps every Luminescent window where Qt thinks it is.  Qt on Wayland still
    remembers the geometry the code asks for (move(), setGeometry()), so the
    rest of the app keeps working in global coordinates; WindowSync moves the
    real window there whenever Qt's idea of it changes,
  * runs a little Lua inside Hyprland (the LUMI table) that owns our keybinds
    and window rules.  A bind reports back by writing one line into a FIFO
    (events.fifo in $XDG_RUNTIME_DIR/luminescent),
  * listens to the event socket, so binds and rules come back after a config
    reload (which throws away everything added at runtime).

Window titles carry invisible markers so the window rules can tell windows
apart: NO_ACTIVATE (zero-width space) = do not focus it when it opens,
NO_FOCUS (zero-width non-joiner) = never give it keyboard focus (careful:
Hyprland then sends it no mouse input either), NO_PIN (word joiner) = not pinned, so other apps'
windows can be drawn above it (the overlay's backdrops; set the widget property
"lum_no_pin"), BLUR (function application) = blur what is behind it ("lum_blur").
"""
import os
import json
import stat
import socket
import logging

from PyQt6.QtCore import QObject, QTimer, QSocketNotifier, QEvent, QPoint, QRect, Qt, pyqtSignal
from PyQt6.QtGui import QCursor
from PyQt6.QtNetwork import QLocalSocket
from PyQt6.QtWidgets import QApplication, QWidget, QDialog

from core.guard import alive

log = logging.getLogger("hypr")

APP_ID = "luminescent"              # the Wayland app id (= Hyprland window class)
NO_ACTIVATE = "\u200b"
NO_FOCUS = "\u200c"
NO_PIN = "\u2060"
BLUR = "\u2061"                      # function application: Hyprland blurs what is behind the window
GLASS = "\u2062"                     # invisible times: a glass panel, blurred behind with rounded corners
RULES_VERSION = 3                    # bump when the rules below change: the old ones are switched off
BIND_DESC = "Luminescent"

_sig = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE", "")
_runtime = os.environ.get("XDG_RUNTIME_DIR") or (f"/run/user/{os.getuid()}" if hasattr(os, "getuid") else "")
SOCKET = os.path.join(_runtime, "hypr", _sig, ".socket.sock") if _sig else ""
EVENT_SOCKET = os.path.join(_runtime, "hypr", _sig, ".socket2.sock") if _sig else ""
FIFO_DIR = os.path.join(_runtime, "luminescent")
FIFO = os.path.join(FIFO_DIR, "events.fifo")

_warned: set[str] = set()


def available() -> bool:
    return bool(SOCKET) and os.path.exists(SOCKET)


def wayland() -> bool:
    return QApplication.platformName().startswith("wayland")


# ── the control socket ────────────────────────────────────────────────────────
def request(cmd: str, timeout: float = 1.0) -> str:
    """Send one command to Hyprland ('j/clients', 'eval <lua>', 'repl <lua>'...) and return the reply."""
    if not SOCKET:
        return ""
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(timeout)
            s.connect(SOCKET)
            s.sendall(cmd.encode("utf-8"))
            chunks = []
            while True:
                b = s.recv(65536)
                if not b:
                    break
                chunks.append(b)
        return b"".join(chunks).decode("utf-8", "replace")
    except OSError as e:
        if "socket" not in _warned:
            _warned.add("socket")
            log.warning("Hyprland socket not reachable: %s", e)
        return ""


def lstr(s: str) -> str:
    """A Lua string literal."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def lua(code: str) -> str:
    """Run Lua statements inside Hyprland. Returns '' on success or the error text."""
    out = request("eval " + code).strip()
    if out == "ok":
        return ""
    if out:
        log.warning("hyprland lua: %s", out.splitlines()[0])
    return out or "no reply from Hyprland"


def value(expr: str) -> str:
    """Evaluate a Lua expression inside Hyprland and return it as text."""
    return request("repl return " + expr).strip()


def query(name: str):
    """A JSON query ('clients', 'monitors', 'binds', 'cursorpos', 'activewindow')."""
    out = request("j/" + name)
    try:
        return json.loads(out) if out else None
    except ValueError:
        return None


def dispatch(expr: str) -> str:
    """Run a dispatcher, e.g. dispatch('hl.dsp.focus({ window = "address:0x1" })')."""
    return lua(f"hl.dispatch({expr})")


def move_window(addr: str, x: int, y: int, pinned: bool = True) -> str:
    """Move a window to (x, y) in layout coordinates. Hyprland answers "ok" to a move of a pinned window but
    leaves it where it is, which kept every Luminescent panel and pop-up wherever Hyprland first put it: a
    pinned window is unpinned for the move and pinned again, in one go so nothing shows in between."""
    t = lstr("address:" + addr)
    move = f"hl.dispatch(hl.dsp.window.move({{ x = {int(x)}, y = {int(y)}, window = {t} }}))"
    if not pinned:
        return lua(move)
    return lua(f"hl.dispatch(hl.dsp.window.pin({{ action = \"disable\", window = {t} }})) {move} "
               f"hl.dispatch(hl.dsp.window.pin({{ action = \"enable\", window = {t} }}))")


# ── cursor, screens, keyboard ─────────────────────────────────────────────────
def cursor_pos() -> QPoint:
    """The pointer in global (layout) coordinates, wherever it is."""
    d = query("cursorpos") if available() else None
    if isinstance(d, dict) and "x" in d:
        return QPoint(int(d["x"]), int(d["y"]))
    return QCursor.pos()


def move_cursor(p: QPoint):
    if available():
        dispatch(f"hl.dsp.cursor.move({{ x = {int(p.x())}, y = {int(p.y())} }})")
    else:
        QCursor.setPos(p)


def screen_at_cursor():
    return QApplication.screenAt(cursor_pos()) or QApplication.primaryScreen()


def dialog_area(parent) -> tuple[QRect, QRect]:
    """Where a pop-up (ask a name, confirm, pick a colour...) goes: (the area to centre it on, the screen to keep
    it inside). The middle of the window that opened it when the mouse is on that window, else the middle of
    the screen with the mouse, never the window's spot on some other monitor."""
    cur = cursor_pos()
    scr = available_geometry(QApplication.screenAt(cur) or QApplication.primaryScreen())
    win = parent.window() if isinstance(parent, QWidget) else None
    if win is not None and win.isVisible():
        g = win.frameGeometry()
        if g.contains(cur):
            inside = g.intersected(scr)
            return (inside if not inside.isEmpty() else scr), scr
    return scr, scr


def available_geometry(screen) -> QRect:
    """The screen minus what bars and docks reserve (Qt on Wayland does not know about those)."""
    g = screen.geometry()
    mons = query("monitors") if available() else None
    for m in mons or []:
        if m.get("name") == screen.name():
            res = m.get("reserved") or [0, 0, 0, 0]
            if len(res) == 4:
                left, top, right, bottom = (int(v) for v in res)
                return g.adjusted(left, top, -right, -bottom)
    return screen.availableGeometry()


def key_down(keysyms: list[str]) -> bool | None:
    """True if any of the xkb keysyms ('Alt_L', 'a'...) is held. None if Hyprland can't say."""
    if not keysyms or not available():
        return None
    out = value(" or ".join(f"hl.is_key_down({lstr(k)})" for k in keysyms))
    if out in ("true", "false"):
        return out == "true"
    return None


def send_shortcut(mods: str, key: str) -> str:
    """Send a key combination to the focused window ('CTRL', 'V')."""
    return dispatch(f"hl.dsp.send_shortcut({{ mods = {lstr(mods)}, key = {lstr(key)} }})")


# ── Lua side: binds, window rules, the FIFO ───────────────────────────────────
#
# Binds are never removed, only disabled.  Hyprland 0.56 crashes (use after free in
# CLuaKeybind) when :remove() is called on a bind that shares its key with another
# one, because removing the first already deleted both: exactly the case of a hold
# key's press + release binds.  So every distinct (keys, release, non_consuming)
# gets one "slot" that lives until the next config reload, and (un)binding just
# points the slot at a bind id and enables or disables it.
def _bootstrap_code() -> str:
    cls = f"^{APP_ID}$"
    rules = [
        f"{{ match = {{ class = {lstr(cls)} }}, float = true, pin = true, decorate = false, no_anim = true, "
        f"no_shadow = true, no_blur = true, no_dim = true, border_size = 0, rounding = 0, "
        f"opacity = \"1 override 1 override\" }}",
        f"{{ match = {{ class = {lstr(cls)}, title = {lstr('^.*' + NO_ACTIVATE + '.*$')} }}, no_initial_focus = true }}",
        f"{{ match = {{ class = {lstr(cls)}, title = {lstr('^.*' + NO_FOCUS + '.*$')} }}, "
        f"no_initial_focus = true, no_focus = true }}",
        f"{{ match = {{ class = {lstr(cls)}, title = {lstr('^.*' + NO_PIN + '.*$')} }}, pin = false }}",
        f"{{ match = {{ class = {lstr(cls)}, title = {lstr('^.*' + BLUR + '.*$')} }}, no_blur = false }}",
    ]
    fifo = FIFO.replace("'", "")
    return (
        "LUMI = LUMI or {} "
        "LUMI.slots = LUMI.slots or {} "
        "for _, s in pairs(LUMI.slots) do s.id = nil pcall(function() s.bind:set_enabled(false) end) end "
        f"LUMI.fifo = {lstr(fifo)} "
        "function LUMI.send(msg) "
        # (a command starting with "[" would be read as exec rules, hence `test`)
        "hl.exec_cmd(\"test -p '\" .. LUMI.fifo .. \"' && printf '%s\\\\n' '\" .. msg .. \"' 1<>'\" .. LUMI.fifo .. \"'\") "
        "end "
        "function LUMI.bind(slot, id, keys, opts) "
        "local s = LUMI.slots[slot] "
        "if s then s.id = id s.bind:set_enabled(true) return end "
        "s = { id = id } "
        "s.bind = hl.bind(keys, function() if s.id then LUMI.send(s.id) end end, opts) "
        "LUMI.slots[slot] = s "
        "end "
        "function LUMI.unbind(id) "
        "for _, s in pairs(LUMI.slots) do if s.id == id then s.id = nil s.bind:set_enabled(false) end end "
        "end "
        "function LUMI.unbind_all() "
        "for _, s in pairs(LUMI.slots) do if s.id then s.id = nil s.bind:set_enabled(false) end end "
        "end "
        f"if LUMI.rules_version ~= {RULES_VERSION} then "
        "for _, r in pairs(LUMI.rules or {}) do pcall(function() r:set_enabled(false) end) end "
        f"LUMI.rules = {{}} LUMI.rules_version = {RULES_VERSION} "
        + " ".join(f"LUMI.rules[#LUMI.rules + 1] = hl.window_rule({r})" for r in rules)
        + " end"
    )


def bootstrap() -> str:
    """(Re)install the LUMI table and our window rules. Must run before the first window is shown."""
    if not available():
        return "Hyprland is not running"
    return lua(_bootstrap_code())


def shutdown():
    if available():
        lua("if LUMI and LUMI.unbind_all then LUMI.unbind_all() end")


def bind(bind_id: str, keys: str, release: bool = False, non_consuming: bool = False) -> str:
    """Bind `keys` ('CTRL + SHIFT + space') so that pressing it writes `bind_id` into the FIFO."""
    opts = [f"description = {lstr(BIND_DESC)}"]
    if release:
        opts.append("release = true")
    if non_consuming:
        opts.append("non_consuming = true")
    slot = f"{keys}|{int(release)}|{int(non_consuming)}"
    return lua(f"LUMI.bind({lstr(slot)}, {lstr(bind_id)}, {lstr(keys)}, {{ {', '.join(opts)} }})")


def unbind(bind_id: str):
    lua(f"LUMI.unbind({lstr(bind_id)})")


def unbind_all():
    lua("LUMI.unbind_all()")


def foreign_binds() -> list[dict]:
    """Hyprland binds that are not ours (the user's own config)."""
    return [b for b in (query("binds") or []) if b.get("description") != BIND_DESC]


class Events(QObject):
    """Lines written into the FIFO by our binds."""
    message = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self._fd = self._keep = -1
        self._buf = b""
        self._notifier = None
        try:
            os.makedirs(FIFO_DIR, mode=0o700, exist_ok=True)
            if os.path.exists(FIFO) and not stat.S_ISFIFO(os.stat(FIFO).st_mode):
                os.remove(FIFO)
            if not os.path.exists(FIFO):
                os.mkfifo(FIFO, 0o600)
            self._fd = os.open(FIFO, os.O_RDONLY | os.O_NONBLOCK)
            self._keep = os.open(FIFO, os.O_WRONLY | os.O_NONBLOCK)   # so the reader never sees EOF
            self._notifier = QSocketNotifier(self._fd, QSocketNotifier.Type.Read, self)
            self._notifier.activated.connect(self._read)
        except OSError:
            log.exception("could not create %s", FIFO)

    def _read(self, *_):
        try:
            data = os.read(self._fd, 65536)
        except BlockingIOError:
            return
        except OSError:
            log.exception("reading the event FIFO failed")
            return
        self._buf += data
        *lines, self._buf = self._buf.split(b"\n")
        for line in lines:
            text = line.decode("utf-8", "replace").strip()
            if text:
                self.message.emit(text)

    def close(self):
        if self._notifier:
            self._notifier.setEnabled(False)
        for fd in (self._fd, self._keep):
            if fd >= 0:
                try:
                    os.close(fd)
                except OSError:
                    pass
        self._fd = self._keep = -1


# ── keeping windows where Qt thinks they are ──────────────────────────────────
def _is_tracked(w) -> bool:
    if not isinstance(w, QWidget) or not w.isWindow():
        return False
    t = w.windowType()
    return t not in (Qt.WindowType.Popup, Qt.WindowType.ToolTip, Qt.WindowType.Desktop, Qt.WindowType.SplashScreen)


def _marker(w: QWidget) -> str:
    m = (NO_PIN if w.property("lum_no_pin") else "") + (BLUR if w.property("lum_blur") else "") + \
        (GLASS if w.property("lum_glass") else "")
    if w.windowFlags() & Qt.WindowType.WindowDoesNotAcceptFocus:
        return m + NO_FOCUS
    if w.testAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating):
        return m + NO_ACTIVATE
    return m


def _strip(title: str) -> str:
    return title.rstrip(NO_ACTIVATE + NO_FOCUS + NO_PIN + BLUR + GLASS)


def set_glass_rule(rounding: int) -> str:
    """The rule for Luminescent's own glass windows (panels and notifications with glass on): what is behind
    them is blurred, inside corners as round as the panel's so the blur does not show past them."""
    if not available():
        return ""
    title = lstr("^.*" + GLASS + ".*$")
    return lua("LUMI = LUMI or {} "
               "if LUMI.glass_rule then pcall(function() LUMI.glass_rule:set_enabled(false) end) end "
               f"LUMI.glass_rule = hl.window_rule({{ match = {{ class = {lstr('^' + APP_ID + '$')}, title = {title} }}, "
               f"no_blur = false, rounding = {max(0, int(rounding))} }})")


class WindowSync(QObject):
    """
    Watches every Luminescent top-level window. When Hyprland maps one (openwindow event) it is moved to
    the position Qt has for it; later move()s are forwarded too. A window nobody placed is left where
    Hyprland put it and Qt is told that position, so global coordinates stay right either way.
    """
    reloaded = pyqtSignal()
    foreign_opened = pyqtSignal(str, str)      # address, class of another program's new window

    def __init__(self, app: QApplication):
        super().__init__()
        self.app = app
        self._addr: dict[QWidget, str] = {}
        self._by_addr: dict[str, QWidget] = {}
        self._pending: list[QWidget] = []
        self._want_focus: set[QWidget] = set()
        self._dirty: set[QWidget] = set()
        self._quiet = False
        self._flush_timer = QTimer(self)
        self._flush_timer.setSingleShot(True)
        self._flush_timer.timeout.connect(self._flush)
        self._track: dict[QWidget, list] = {}
        self._track_timer = QTimer(self)
        self._track_timer.setInterval(60)
        self._track_timer.timeout.connect(self._track_tick)
        self._events = QLocalSocket(self)
        self._events.readyRead.connect(self._on_events)
        self._events.disconnected.connect(lambda: QTimer.singleShot(2000, self._connect_events))
        self._ebuf = b""
        self.enabled = available() and wayland()
        if self.enabled:
            app.installEventFilter(self)
            self._connect_events()

    def _connect_events(self):
        if self._events.state() == QLocalSocket.LocalSocketState.UnconnectedState and EVENT_SOCKET:
            self._events.connectToServer(EVENT_SOCKET)

    # ── Qt side ────────────────────────────────────────────────────────────
    def eventFilter(self, obj, e):
        t = e.type()
        if t in (QEvent.Type.Show, QEvent.Type.Hide, QEvent.Type.Move, QEvent.Type.WindowTitleChange) \
                and _is_tracked(obj):
            if t == QEvent.Type.Show:
                self._mark(obj)
                if obj not in self._pending and obj not in self._addr:
                    self._pending.append(obj)
            elif t == QEvent.Type.Hide:
                self._forget(obj)
            elif t == QEvent.Type.Move:
                if obj in self._addr and not self._quiet:
                    self._dirty.add(obj)
                    if not self._flush_timer.isActive():
                        self._flush_timer.start(0)
            elif t == QEvent.Type.WindowTitleChange:
                self._mark(obj)
        return False

    def _mark(self, w: QWidget):
        want = _marker(w)
        title = w.windowTitle()
        if want and not title.endswith(want):
            w.setWindowTitle(_strip(title) + want)

    def _prune(self):
        # a window deleted while it was still listed here (shown, then destroyed before Hyprland mapped it)
        # used to make every later lookup raise "wrapped C/C++ object ... has been deleted", so no new window
        # was ever placed again: drop the dead ones first
        self._pending = [w for w in self._pending if alive(w)]
        for w in [w for w in self._addr if not alive(w)]:
            self._by_addr.pop(self._addr.pop(w), None)
        for w in [w for w in self._by_addr.values() if not alive(w)]:
            self._by_addr = {a: x for a, x in self._by_addr.items() if x is not w}
        self._want_focus = {w for w in self._want_focus if alive(w)}
        self._dirty = {w for w in self._dirty if alive(w)}
        for w in [w for w in self._track if not alive(w)]:
            self._track.pop(w, None)

    def _forget(self, w: QWidget):
        if w in self._pending:
            self._pending.remove(w)
        addr = self._addr.pop(w, None)
        if addr:
            self._by_addr.pop(addr, None)
        self._want_focus.discard(w)
        self._dirty.discard(w)
        self._track.pop(w, None)

    def _flush(self):
        self._prune()
        for w in list(self._dirty):
            self._dirty.discard(w)
            addr = self._addr.get(w)
            if addr and w.isVisible():
                self._move(addr, w)

    @staticmethod
    def _target(w: QWidget) -> QPoint:
        frameless = bool(w.windowFlags() & Qt.WindowType.FramelessWindowHint)
        return w.geometry().topLeft() if frameless else w.frameGeometry().topLeft()

    def _move(self, addr: str, w: QWidget):
        p = self._target(w)
        move_window(addr, p.x(), p.y(), pinned=not w.property("lum_no_pin"))

    def _set_qt_pos(self, w: QWidget, x: int, y: int):
        """Tell Qt where the window really is, without sending it back to Hyprland."""
        self._quiet = True
        try:
            w.move(x, y)            # move() places the frame, like Hyprland's "at"
        finally:
            self._quiet = False

    # ── Hyprland side ──────────────────────────────────────────────────────
    def _on_events(self):
        self._ebuf += bytes(self._events.readAll())
        *lines, self._ebuf = self._ebuf.split(b"\n")
        for raw in lines:
            try:
                self._event(raw.decode("utf-8", "replace"))
            except Exception:
                log.exception("handling the Hyprland event %r failed", raw[:120])

    def _event(self, line: str):
        name, _, data = line.partition(">>")
        if name == "openwindow":
            parts = data.split(",", 3)
            if len(parts) == 4 and parts[2] == APP_ID:
                self._opened("0x" + parts[0], parts[3])
            elif len(parts) == 4:
                self.foreign_opened.emit("0x" + parts[0], parts[2])
        elif name == "closewindow":
            w = self._by_addr.pop("0x" + data.strip(), None)
            if w is not None:
                self._addr.pop(w, None)
        elif name == "configreloaded":
            log.info("Hyprland config reloaded: reinstalling binds and window rules")
            bootstrap()
            self.reloaded.emit()

    def _opened(self, addr: str, title: str):
        self._prune()
        w = next((p for p in self._pending if p.windowTitle() == title), None)
        if w is None:
            w = next((p for p in self._pending if _strip(p.windowTitle()) == _strip(title)), None)
        if w is None and self._pending:
            w = self._pending[0]
        if w is None:
            return
        self._pending.remove(w)
        self._addr[w] = addr
        self._by_addr[addr] = w
        if isinstance(w, QDialog):
            self._centre_dialog(w, addr)
            self._dialog_on_top(w)
            # a panel raised a moment later (Notes re-raises itself after opening) must not cover it
            QTimer.singleShot(250, lambda d=w: self._dialog_on_top(d)
                              if alive(d) and d in self._addr and d.isVisible() else None)
        elif w.testAttribute(Qt.WidgetAttribute.WA_Moved):
            p = self._target(w)
            self._settle(addr, w, p.x(), p.y())
        else:
            self._read_back(w, addr)
        if w in self._want_focus:
            self._want_focus.discard(w)
            self.focus(w)

    def _centre_dialog(self, w: QWidget, addr: str):
        """Dialogs (ask a name, confirm, pick a file or colour...) go where dialog_area() says. Qt works their
        spot out from window positions it can not know on Wayland, which put them in the top-left corner."""
        parent = w.parentWidget().window() if w.parentWidget() is not None else None
        area, scr = dialog_area(parent if parent is not w else None)
        c = self._client(addr)
        if c and isinstance(c.get("size"), list):
            dw, dh = int(c["size"][0]), int(c["size"][1])
        else:
            dw, dh = w.width(), w.height()
        x = max(scr.left(), min(area.center().x() - dw // 2, scr.right() - dw))
        y = max(scr.top(), min(area.center().y() - dh // 2, scr.bottom() - dh))
        self._set_qt_pos(w, x, y)
        self._settle(addr, w, x, y)

    def _settle(self, addr: str, w: QWidget, x: int, y: int, checks=(15, 60, 180, 420)):
        """Move a window that has just opened to (x, y), and keep at it for a moment: Hyprland announces a new
        window before it has finished placing it, and that first placement used to win over our move about half
        the time (every panel then sat where Hyprland put it, whatever Panels like Notes open said)."""
        pinned = not w.property("lum_no_pin")
        move_window(addr, x, y, pinned)

        def check(rest):
            if not alive(w) or self._addr.get(w) != addr or not w.isVisible() or w in self._track:
                return
            c = self._client(addr)
            if c and isinstance(c.get("at"), list) and c["at"] != [x, y]:
                move_window(addr, x, y, pinned)
            if rest:
                QTimer.singleShot(rest[0], lambda: check(rest[1:]))
        QTimer.singleShot(checks[0], lambda: check(list(checks[1:])))

    def _client(self, addr: str) -> dict | None:
        return next((c for c in query("clients") or [] if c.get("address") == addr), None)

    def _read_back(self, w: QWidget, addr: str) -> tuple[int, int] | None:
        c = self._client(addr)
        if c and isinstance(c.get("at"), list):
            x, y = int(c["at"][0]), int(c["at"][1])
            self._set_qt_pos(w, x, y)
            return x, y
        return None

    # ── API ────────────────────────────────────────────────────────────────
    def focus(self, w: QWidget):
        if not alive(w):
            return
        addr = self._addr.get(w)
        if not addr:
            self._want_focus.add(w)
            return
        dispatch(f"hl.dsp.focus({{ window = {lstr('address:' + addr)} }})")

    def raise_window(self, w: QWidget):
        self._prune()
        if not alive(w):
            return
        addr = self._addr.get(w)
        if addr:
            dispatch(f"hl.dsp.window.alter_zorder({{ mode = \"top\", window = {lstr('address:' + addr)} }})")
        # an open dialog always stays above Luminescent's other windows: it blocks clicks on them, so one
        # hidden behind a panel left you stuck
        for d in self._open_dialogs():
            if d is not w:
                self._dialog_on_top(d, focus=False)

    def _open_dialogs(self) -> list[QWidget]:
        modal = QApplication.activeModalWidget()
        out = [d for d in self._addr if alive(d) and isinstance(d, QDialog) and d.isVisible()]
        if modal is not None and modal in out:
            out.remove(modal)
            out.append(modal)                    # the modal one last, so it ends up on top
        return out

    def _dialog_on_top(self, d: QWidget, focus: bool = True):
        addr = self._addr.get(d)
        if not addr:
            return
        target = lstr("address:" + addr)
        dispatch(f"hl.dsp.window.alter_zorder({{ mode = \"top\", window = {target} }})")
        if focus:
            dispatch(f"hl.dsp.focus({{ window = {target} }})")

    def float_on_top(self, addr: str, area: QRect):
        """Make another program's window float, centred on `area`, above Luminescent's windows."""
        target = lstr("address:" + addr)
        dispatch(f"hl.dsp.window.float({{ action = \"enable\", window = {target} }})")
        c = self._client(addr)
        if c and isinstance(c.get("size"), list):
            cw, ch = int(c["size"][0]), int(c["size"][1])
            w, h = min(cw, area.width() - 80), min(ch, area.height() - 80)   # clear of bars and docks
            if w < 640 or h < 400:                         # a tiny tiled leftover: give it a usable size
                w, h = min(1200, area.width() - 160), min(800, area.height() - 160)
            if (w, h) != (cw, ch):
                dispatch(f"hl.dsp.window.resize({{ x = {w}, y = {h}, window = {target} }})")
            x, y = area.center().x() - w // 2, area.center().y() - h // 2
            dispatch(f"hl.dsp.window.move({{ x = {x}, y = {y}, window = {target} }})")
        dispatch(f"hl.dsp.window.alter_zorder({{ mode = \"top\", window = {target} }})")
        dispatch(f"hl.dsp.focus({{ window = {target} }})")

    def unfloat(self, addr: str):
        if self._client(addr) is not None:
            dispatch(f"hl.dsp.window.float({{ action = \"disable\", window = {lstr('address:' + addr)} }})")

    def follow(self, w: QWidget, done=None):
        """After a compositor-driven move/resize: read the real geometry back until it stops changing."""
        if w not in self._addr:
            return
        self._track[w] = [None, 0, 0, done]      # last 'at', polls unchanged, polls total, callback
        if not self._track_timer.isActive():
            self._track_timer.start()

    def _track_tick(self):
        self._prune()
        for w, st in list(self._track.items()):
            addr = self._addr.get(w)
            c = self._client(addr) if addr else None
            if c is None or not alive(w):
                self._track.pop(w, None)
                continue
            at = tuple(c.get("at") or (0, 0))
            st[2] += 1
            st[1] = st[1] + 1 if at == st[0] else 0
            st[0] = at
            # still for ~0.5 s, after it has moved (or 1.5 s without moving at all): the drag is over
            if (st[1] >= 8 and st[2] > 8) or st[2] > 600:
                self._track.pop(w, None)
                self._set_qt_pos(w, int(at[0]), int(at[1]))
                if st[3]:
                    st[3]()
        if not self._track:
            self._track_timer.stop()


_sync: WindowSync | None = None
_events: Events | None = None


def init(app: QApplication) -> WindowSync:
    """Call once, after QApplication and before any window is shown."""
    global _sync
    # a test copy (LUMINESCENT_NO_HOOK) must not wipe the binds of the real instance
    if available() and not os.environ.get("LUMINESCENT_NO_HOOK"):
        err = bootstrap()
        if err:
            log.warning("could not install the Hyprland helpers: %s", err)
    _sync = WindowSync(app)
    return _sync


def sync() -> WindowSync | None:
    return _sync


def events() -> Events:
    global _events
    if _events is None:
        _events = Events()
    return _events
