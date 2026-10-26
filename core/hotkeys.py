"""
Global hotkeys through Hyprland keybinds.

Wayland gives no program a global keyboard hook, so every hotkey is a Hyprland
bind added at runtime (see core.hypr).  When it fires, Hyprland writes the
bind's id into our FIFO and the callback runs on the Qt main thread.  Binds are
added and removed live, so changing a hotkey in Settings applies at once, and
they are put back after a Hyprland config reload.

Modules register hotkeys with an `owner` string so they can be torn down and
re-registered when settings change, and duplicate combos are refused so two
modules (or Luminescent and your own hyprland config) never fight over keys.

Hold keys (Doodle's "hold Alt") are a press bind plus a release bind, both
non-consuming so the key still reaches the focused app.  While a hold key is
down a 100 ms watchdog asks Hyprland whether it really still is, so a missed
release can not leave Doodle stuck on.

Combos are stored as before ('ctrl+shift+space', 'alt', 'ctrl+alt+/') and
translated to Hyprland's syntax ('CTRL + SHIFT + space') when bound.
"""
import os
import time
import logging

from PyQt6.QtCore import QObject, QTimer

from core import IS_WIN, hypr, usage

log = logging.getLogger("hotkeys")

_ALIASES = {
    "control": "ctrl", "win": "windows", "super": "windows", "cmd": "windows", "meta": "windows",
    "escape": "esc", "return": "enter", "del": "delete", "ins": "insert",
    "pgup": "page up", "pgdn": "page down", "pageup": "page up", "pagedown": "page down",
}

MODIFIERS = {"ctrl": "CTRL", "alt": "ALT", "shift": "SHIFT", "windows": "SUPER"}
# xkb keysyms of each modifier key (what Hyprland calls the key itself)
_MOD_KEYSYMS = {"ctrl": ["Control_L", "Control_R"], "alt": ["Alt_L", "Alt_R"],
                "shift": ["Shift_L", "Shift_R"], "windows": ["Super_L", "Super_R"]}
_MODMASK = {"SHIFT": 1, "CTRL": 4, "ALT": 8, "SUPER": 64}

# our key names -> xkb keysym names
_KEYSYMS = {
    "space": "space", "esc": "Escape", "enter": "Return", "tab": "Tab", "backspace": "BackSpace",
    "delete": "Delete", "insert": "Insert", "home": "Home", "end": "End",
    "page up": "Prior", "page down": "Next", "up": "Up", "down": "Down", "left": "Left", "right": "Right",
    "print screen": "Print", "print": "Print", "pause": "Pause", "caps lock": "Caps_Lock",
    "num lock": "Num_Lock", "scroll lock": "Scroll_Lock", "menu": "Menu", "apps": "Menu",
    "=": "equal", "-": "minus", "+": "plus", "/": "slash", "\\": "backslash", ",": "comma", ".": "period",
    ";": "semicolon", "'": "apostrophe", "#": "numbersign", "[": "bracketleft", "]": "bracketright",
    "`": "grave", "*": "asterisk", "!": "exclam", "?": "question", ":": "colon", "<": "less", ">": "greater",
    "~": "asciitilde", "@": "at", "$": "dollar", "%": "percent", "^": "asciicircum", "&": "ampersand",
    "(": "parenleft", ")": "parenright", "_": "underscore", "|": "bar", '"': "quotedbl",
    "£": "sterling", "¬": "notsign",
}
_KEYSYMS.update({f"f{i}": f"F{i}" for i in range(1, 25)})
_KEYSYMS.update({f"num {i}": f"KP_{i}" for i in range(10)})
_KEYSYMS.update({"num +": "KP_Add", "num -": "KP_Subtract", "num *": "KP_Multiply", "num /": "KP_Divide",
                 "num enter": "KP_Enter", "num .": "KP_Decimal"})


def normalize(combo: str) -> str:
    """'Ctrl + Shift+M' -> 'ctrl+shift+m'."""
    if not combo:
        return ""
    parts = [p.strip().lower() for p in combo.replace(" + ", "+").split("+") if p.strip()]
    if combo.strip().endswith("++"):          # "ctrl++" means Ctrl and the plus key
        parts.append("+")
    parts = [_ALIASES.get(p, p) for p in parts]
    for word in ("left ", "right "):            # Hyprland binds do not tell left from right modifiers
        parts = [p[len(word):] if p.startswith(word) and p[len(word):] in MODIFIERS else p for p in parts]
    return "+".join(parts)


def pretty(combo: str) -> str:
    """'ctrl+shift+m' -> 'Ctrl+Shift+M' for menus and tooltips."""
    if not combo:
        return "(none)"
    names = {"ctrl": "Ctrl", "alt": "Alt", "shift": "Shift", "windows": "Win" if IS_WIN else "Super", "esc": "Esc",
             "enter": "Enter", "space": "Space", "tab": "Tab", "backspace": "Backspace",
             "delete": "Delete", "insert": "Insert", "home": "Home", "end": "End",
             "page up": "PgUp", "page down": "PgDn", "up": "↑", "down": "↓", "left": "←", "right": "→"}
    return "+".join(names.get(p, p.upper() if len(p) == 1 else p.title()) for p in normalize(combo).split("+"))


def is_modifier(key: str) -> bool:
    return normalize(key) in MODIFIERS


def keysym(key: str) -> str:
    """'page up' -> 'Prior', 'q' -> 'q', '=' -> 'equal'; '' if unknown."""
    key = normalize(key)
    if key in _KEYSYMS:
        return _KEYSYMS[key]
    if len(key) == 1 and key.isalnum():
        return key
    return ""


def keysyms_of(key: str) -> list[str]:
    """Every keysym that means `key` (both Alt keys for 'alt')."""
    key = normalize(key)
    if key in _MOD_KEYSYMS:
        return list(_MOD_KEYSYMS[key])
    ks = keysym(key)
    return [ks] if ks else []


def _split(combo: str) -> tuple[list[str], str]:
    parts = normalize(combo).split("+")
    mods = [p for p in parts if p in MODIFIERS]
    keys = [p for p in parts if p not in MODIFIERS]
    if not keys and mods:               # a bare modifier ('alt'): the last modifier is the key
        keys = [mods.pop()]
    return mods, keys[0] if len(keys) == 1 else ""


def to_hyprland(combo: str) -> list[str]:
    """Hyprland bind strings for `combo`. A bare modifier key needs one per side ('ALT + Alt_L')."""
    mods, key = _split(combo)
    hmods = [MODIFIERS[m] for m in mods]
    if key in MODIFIERS:
        hmods = hmods + [MODIFIERS[key]]
        return [" + ".join(hmods + [ks]) for ks in _MOD_KEYSYMS[key]]
    return [" + ".join(hmods + [keysym(key)])]


def validate(combo: str) -> str:
    """Return '' if the combo can be bound, else an error message."""
    combo = normalize(combo)
    if not combo:
        return ""
    parts = combo.split("+")
    keys = [p for p in parts if p not in MODIFIERS]
    if len(keys) > 1:
        return "Use one key plus modifiers (e.g. ctrl+shift+m)"
    if keys and not keysym(keys[0]):
        return f"Unknown key: {keys[0]}"
    return ""


# Hyprland's is_key_down() answers "up" for keys it does not track (Alt, at least, on 0.56). Only once it
# has said "down" for something is an "up" believed; until then the answer is "don't know".
_keydown_trusted = False


def key_state(combo: str) -> bool | None:
    """True if every key in `combo` is held right now, False if one is not, None if Hyprland can't tell."""
    global _keydown_trusted
    result: bool | None = True
    for part in normalize(combo).split("+"):
        down = hypr.key_down(keysyms_of(part))
        if down:
            if not _keydown_trusted:
                log.info("Hyprland reports held keys: key-state checks are on")
            _keydown_trusted = True
        elif down is False and _keydown_trusted:
            return False
        else:
            result = None
    return result


def key_is_physically_down(combo: str) -> bool:
    """True if every key in `combo` is held (or Hyprland can't tell), so nothing is stopped wrongly."""
    return key_state(combo) is not False


def _mask(hmods: list[str]) -> int:
    m = 0
    for x in hmods:
        m |= _MODMASK.get(x, 0)
    return m


class HotkeyManager(QObject):
    def __init__(self):
        super().__init__()
        self._owned: dict[str, list[str]] = {}       # owner -> [bind ids]
        self._in_use: dict[str, str] = {}            # combo -> owner
        self._labels: dict[str, str] = {}            # combo -> what it does (for the Keybinds panel)
        self._binds: dict[str, dict] = {}            # bind id -> {keys, release, non_consuming, fire}
        self._holds: dict[str, dict] = {}            # key -> {owner, on_press, on_release, down, since, keysyms}
        self._seq = 0
        self._suspended = False
        self.pass_keys = True          # hotkeys also reach the focused app, as the Windows hook did
        # tests and the smoke test run next to the real launcher: no binds there
        self.dry = bool(os.environ.get("LUMINESCENT_NO_HOOK")) or not hypr.available()
        if not self.dry:
            hypr.events().message.connect(self._on_message)
            sync = hypr.sync()
            if sync is not None:
                sync.reloaded.connect(self._reinstall)
        elif not os.environ.get("LUMINESCENT_NO_HOOK"):
            log.warning("Hyprland is not running: global hotkeys are off")

        self._watch = QTimer(self)
        self._watch.setInterval(100)
        self._watch.timeout.connect(self._watchdog)

    # ── Hyprland side ──────────────────────────────────────────────────────
    def _add(self, keys: str, fire, release: bool = False, non_consuming: bool = False) -> str:
        self._seq += 1
        bind_id = f"k{self._seq}"
        self._binds[bind_id] = {"keys": keys, "release": release, "non_consuming": non_consuming, "fire": fire}
        if not self._suspended:
            err = hypr.bind(bind_id, keys, release, non_consuming)
            if err:
                self._binds.pop(bind_id)
                raise RuntimeError(err)
        return bind_id

    def _reinstall(self):
        """After a config reload (Hyprland forgot every runtime bind) or when un-suspending."""
        for bind_id, b in self._binds.items():
            hypr.bind(bind_id, b["keys"], b["release"], b["non_consuming"])

    def _on_message(self, msg: str):
        b = self._binds.get(msg)
        if b is None or self._suspended:
            return
        try:
            b["fire"]()
        except Exception:
            log.exception("hotkey callback failed")

    def _conflict(self, keys: str) -> str:
        """Describe the user's own Hyprland bind on `keys`, if there is one."""
        parts = [p.strip() for p in keys.split("+")]
        key, hmods = parts[-1].lower(), parts[:-1]
        mask = _mask(hmods)
        for b in self._foreign:
            if str(b.get("key", "")).lower() == key and int(b.get("modmask", -1)) == mask and not b.get("release"):
                what = b.get("dispatcher", "")
                return f"{keys} is already bound in your Hyprland config" + (f" ({what})" if what else "")
        return ""

    @property
    def _foreign(self) -> list[dict]:
        now = time.monotonic()
        if now - getattr(self, "_foreign_at", -10) > 5:
            self._foreign_cache = hypr.foreign_binds()
            self._foreign_at = now
        return self._foreign_cache

    # ── state ──────────────────────────────────────────────────────────────
    def set_suspended(self, on: bool):
        """Ignore every hotkey (used while recording a new hotkey in settings). The binds are lifted so the
        keys reach the recorder instead of being swallowed by Hyprland."""
        on = bool(on)
        if on == self._suspended:
            return
        self._suspended = on
        if self.dry:
            return
        if on:
            hypr.unbind_all()
        else:
            self._reinstall()
            self.reset_keys(log_it=False)

    def _watchdog(self):
        """Every 100 ms while a hold key is down: release it if Hyprland says it is up (a missed release bind)."""
        now = time.monotonic()
        any_down = False
        for key, h in self._holds.items():
            if not h["down"]:
                continue
            any_down = True
            if now - h["since"] <= 0.15:
                continue
            if key_state(key) is False:            # never cuts a hold short on an untrusted "up"
                log.info("%s: release of %s missed; releasing", h["owner"], pretty(key))
                h["down"] = False
                self._run(h["on_release"])
        if not any_down:
            self._watch.stop()

    def reset_keys(self, log_it: bool = True):
        """Let go of every hold key (tray → Reset stuck keys)."""
        for h in self._holds.values():
            if h["down"]:
                h["down"] = False
                self._run(h["on_release"])
        if log_it:
            log.info("hold keys reset")

    @staticmethod
    def _run(fn):
        try:
            fn()
        except Exception:
            log.exception("hotkey callback failed")

    # ── registration ───────────────────────────────────────────────────────
    def register(self, owner: str, combo: str, callback, label: str = "", non_consuming: bool = False) -> str:
        """Register a press hotkey. Returns '' on success or an error message.

        non_consuming: the key press also reaches the focused app (like Esc for Doodle)."""
        combo = normalize(combo)
        if not combo:
            return ""
        err = validate(combo)
        if err:
            log.warning("%s: invalid hotkey %r (%s)", owner, combo, err)
            return err
        if combo in self._in_use and self._in_use[combo] != owner:
            msg = f"{pretty(combo)} is already used by {self._in_use[combo]}"
            log.warning("%s: %s", owner, msg)
            return msg
        self._labels[combo] = label or getattr(callback, "__name__", "")
        if self.dry:
            self._owned.setdefault(owner, [])
            self._in_use[combo] = owner
            return ""
        keys_list = to_hyprland(combo)
        for keys in keys_list if not non_consuming else []:
            clash = self._conflict(keys)
            if clash:
                log.warning("%s: %s", owner, clash)
                return clash

        def fire():
            for h in self._holds.values():
                if h["down"] and h["owner"] != owner and h["on_interrupt"]:
                    self._run(lambda f=h["on_interrupt"]: f(combo.split("+")[-1]))
            if combo != "esc" and not combo.endswith("+esc"):
                usage.record("hotkeys", combo, f"{owner}: {label}" if label else owner)
            self._run(callback)

        ids = []
        try:
            # a bare modifier (cheat sheet on "alt"...) fires on release, or it would eat every Alt shortcut
            bare_mod = combo in MODIFIERS
            for keys in keys_list:
                ids.append(self._add(keys, fire, release=bare_mod,
                                     non_consuming=bare_mod or non_consuming or self.pass_keys))
        except RuntimeError as e:
            for bind_id in ids:
                self._drop(bind_id)
            log.warning("%s: could not bind %r: %s", owner, combo, e)
            return str(e)
        self._owned.setdefault(owner, []).extend(ids)
        self._in_use[combo] = owner
        log.info("%s: %s -> %s", owner, pretty(combo), label or getattr(callback, "__name__", "callback"))
        return ""

    def register_hold(self, owner: str, key: str, on_press, on_release, on_interrupt=None) -> str:
        """Call on_press when `key` goes down and on_release when it comes back up.

        on_interrupt(key_name) is called when another of Luminescent's hotkeys fires while `key` is held
        (Alt+N...), so the owner can tell "holding Alt" from "using an Alt shortcut".
        """
        key = normalize(key)
        if not key:
            return ""
        if "+" in key:
            return "Hold keys must be a single key (e.g. alt, or f8)"
        err = validate(key)
        if err:
            return err
        if key in self._holds and self._holds[key]["owner"] != owner:
            return f"{pretty(key)} is already a hold key for {self._holds[key]['owner']}"
        if self.dry:
            return ""
        keysyms = keysyms_of(key)
        state = {"owner": owner, "on_press": on_press, "on_release": on_release, "down": False, "since": 0.0,
                 "on_interrupt": on_interrupt, "keysyms": keysyms}

        def pressed():
            if state["down"]:
                return                     # the second of the two press binds below
            log.info("%s: %s down", owner, pretty(key))
            state["down"], state["since"] = True, time.monotonic()
            self._watch.start()
            self._run(on_press)

        def released():
            if not state["down"]:
                return
            log.info("%s: %s up after %.2f s", owner, pretty(key), time.monotonic() - state["since"])
            state["down"] = False
            self._run(on_release)

        ids = []
        try:
            for ks in keysyms:
                if key in MODIFIERS:
                    # depending on timing Hyprland sees the modifier itself as held or not: bind both
                    mod = MODIFIERS[key]
                    ids.append(self._add(ks, pressed, non_consuming=True))
                    ids.append(self._add(f"{mod} + {ks}", pressed, non_consuming=True))
                    ids.append(self._add(f"{mod} + {ks}", released, release=True, non_consuming=True))
                    ids.append(self._add(ks, released, release=True, non_consuming=True))
                else:
                    ids.append(self._add(ks, pressed, non_consuming=True))
                    ids.append(self._add(ks, released, release=True, non_consuming=True))
        except RuntimeError as e:
            for bind_id in ids:
                self._drop(bind_id)
            return str(e)
        self._holds[key] = state
        self._owned.setdefault(owner, []).extend(ids)
        log.info("%s: hold %s", owner, pretty(key))
        return ""

    def _drop(self, bind_id: str):
        if self._binds.pop(bind_id, None) is not None and not self.dry and not self._suspended:
            hypr.unbind(bind_id)

    def unregister_all(self, owner: str):
        for bind_id in self._owned.pop(owner, []):
            self._drop(bind_id)
        for combo, o in list(self._in_use.items()):
            if o == owner:
                del self._in_use[combo]
                self._labels.pop(combo, None)
        for key, h in list(self._holds.items()):
            if h["owner"] == owner:
                del self._holds[key]

    def owner_of(self, combo: str) -> str:
        return self._in_use.get(normalize(combo), "")

    def listing(self) -> list[tuple[str, str, str]]:
        """[(combo, owner, label)] for every hotkey and hold key, for the Keybinds panel."""
        out = [(c, o, self._labels.get(c, "")) for c, o in self._in_use.items()]
        out += [(k, h["owner"], "hold to use") for k, h in self._holds.items()]
        return out

    def combos(self) -> dict[str, str]:
        """{combo: owner} for every registered press hotkey."""
        return dict(self._in_use)

    def shutdown(self):
        self._watch.stop()
        for owner in list(self._owned):
            self.unregister_all(owner)
        if not self.dry:
            hypr.shutdown()

    # ── sending keys (used by snippets for paste) ──────────────────────────
    @staticmethod
    def send(combo: str):
        """Send a key combination ('ctrl+v', 'enter') to the focused window."""
        mods, key = _split(combo)
        hypr.send_shortcut(" ".join(MODIFIERS[m] for m in mods), keysym(key) or key)

    @staticmethod
    def release_modifiers():
        """Nothing to do on Wayland: send() delivers exactly the modifiers it names."""


if IS_WIN:
    # Windows: a global keyboard hook instead of Hyprland binds (same interface)
    from core.hotkeys_win import HotkeyManager, validate, key_state, key_is_physically_down  # noqa: E402,F811
