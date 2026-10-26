"""
Windows hotkeys: one global keyboard hook (the `keyboard` library) for the whole launcher.

core/hotkeys.py uses this HotkeyManager on Windows instead of Hyprland binds; the interface is the same.

The `keyboard` library runs its callbacks on its own listener thread.  Anything
that touches Qt widgets has to happen on the main thread, so every callback is
bounced through a queued Qt signal before it runs.

Stuck keys
----------
The library decides whether "alt+r" was pressed by comparing the exact set of
keys it *thinks* are down.  It only learns that a key went up from the hook, and
Windows drops key-up events in several common cases (an admin window has focus,
Win+L / Ctrl+Alt+Del / UAC, the hook being briefly too slow).  A lost Alt key-up
meant every later "r" looked like Alt+R, and a lost key-up of any other key meant
no combo matched exactly any more (so Esc / Alt+X stopped working while drawing).

So before the library looks at a key press, `_sanitize` throws out every key
Windows itself says is no longer held, each event carries the pressed-key set as
it was at that moment (the library re-reads the live set later on another
thread), and a 100 ms watchdog does the same clean-up and lets go of hold keys
whose release was never seen.
"""
import os
import time
import logging
import ctypes

import keyboard
from PyQt6.QtCore import QObject, Qt, QTimer, pyqtSignal

from core import usage

log = logging.getLogger("hotkeys")


def normalize(combo: str) -> str:
    from core.hotkeys import normalize as _normalize      # here, not at the top: core.hotkeys imports this file
    return _normalize(combo)


def pretty(combo: str) -> str:
    from core.hotkeys import pretty as _pretty
    return _pretty(combo)

_u32 = ctypes.windll.user32


def validate(combo: str) -> str:
    """Return '' if the combo is usable by the keyboard library, else an error message."""
    combo = normalize(combo)
    if not combo:
        return ""
    try:
        keyboard.parse_hotkey(combo)
    except Exception as e:
        return str(e) or "Unknown key"
    return ""


# Virtual-key codes for polling the physical state of modifier keys.
_VK = {"alt": [0x12], "ctrl": [0x11], "shift": [0x10], "windows": [0x5B, 0x5C]}
_MAPVK_VSC_TO_VK, _MAPVK_VSC_TO_VK_EX = 1, 3


def _down(vk: int) -> bool:
    return bool(_u32.GetAsyncKeyState(vk) & 0x8000)


def _modifier_vks(name: str) -> list[int] | None:
    """VKs for a key name like 'alt', 'right shift', 'left windows', 'alt gr'."""
    name = (name or "").lower()
    for word in ("left ", "right "):
        if name.startswith(word):
            name = name[len(word):]
    if name == "alt gr":
        name = "alt"
    return _VK.get(name)


_sc_cache: dict[int, list[int]] = {}


def _scan_code_vks(scan_code: int) -> list[int]:
    """Every VK a scan code can mean (plain and E0-extended, e.g. numpad 8 / up arrow)."""
    vks = _sc_cache.get(scan_code)
    if vks is None:
        vks = []
        for code, mode in ((scan_code, _MAPVK_VSC_TO_VK), (0xE000 | scan_code, _MAPVK_VSC_TO_VK_EX)):
            try:
                vk = _u32.MapVirtualKeyW(code, mode)
            except Exception:
                vk = 0
            if vk and vk not in vks:
                vks.append(vk)
        _sc_cache[scan_code] = vks
    return vks


def _event_physically_down(ev) -> bool | None:
    """True/False from Windows for a keyboard-library event, None if the key cannot be mapped."""
    vks = _modifier_vks(getattr(ev, "name", "")) or _scan_code_vks(ev.scan_code)
    if not vks:
        return None
    return any(_down(vk) for vk in vks)


def key_is_physically_down(combo: str) -> bool:
    """True if every key in `combo` is currently held according to Windows itself."""
    for part in normalize(combo).split("+"):
        vks = _VK.get(part)
        if vks is None:
            try:
                vks = _scan_code_vks(keyboard.key_to_scan_codes(part)[0])
            except Exception:
                vks = []
            if not vks:
                return True  # unknown key: assume held so we never deactivate wrongly
        if not any(_down(vk) for vk in vks):
            return False
    return True


def key_state(combo: str) -> bool | None:
    """True if every key in `combo` is held right now, False if not (Windows always knows)."""
    return key_is_physically_down(combo)


def _prune_stale(skip_scan_code=None, min_age: float = 0.0) -> list[str]:
    """Drop keys the library thinks are held but Windows says are up. Returns their names."""
    dropped = []
    now = time.time()
    lst = keyboard._listener
    with keyboard._pressed_events_lock:
        for sc, ev in list(keyboard._pressed_events.items()):
            if sc == skip_scan_code:
                continue
            age = now - (ev.time or now)
            if age < min_age:
                continue
            state = _event_physically_down(ev)
            # unmappable keys auto-repeat while held, so an old entry means a lost key-up
            if state is False or (state is None and age > 2.0):
                del keyboard._pressed_events[sc]
                keyboard._logically_pressed_keys.pop(sc, None)
                getattr(lst, "active_modifiers", set()).discard(sc)
                dropped.append(ev.name or str(sc))
    return dropped


class HotkeyManager(QObject):
    _invoke = pyqtSignal(object)

    def __init__(self):
        super().__init__()
        self._invoke.connect(self._run, Qt.ConnectionType.QueuedConnection)
        self._owned: dict[str, list] = {}       # owner -> [(kind, handle)]
        self._in_use: dict[str, str] = {}       # combo -> owner
        self._holds: dict[str, dict] = {}       # key -> {owner, on_press, on_release, down, since}
        self._labels: dict[str, str] = {}       # combo -> what it does (for the Keybinds panel)
        self._suspended = False
        self.pass_keys = True                   # the hook never swallows keys: hotkeys also reach the app
        self._current_event = None              # event being processed on the listener thread
        # tests and the exe smoke test run next to the real launcher: no global hook there
        self.dry = bool(os.environ.get("LUMINESCENT_NO_HOOK"))
        if not self.dry:
            self._install_guards()

        self._watch = QTimer(self)
        self._watch.setInterval(100)
        self._watch.timeout.connect(self._watchdog)
        self._watch.start()

    def set_suspended(self, on: bool):
        """Temporarily ignore every hotkey (used while recording a new hotkey in settings)."""
        self._suspended = bool(on)
        if not on:
            self.reset_keys(log_it=False)

    # ── stuck-key protection ───────────────────────────────────────────────
    def _install_guards(self):
        try:
            keyboard.hook(self._sanitize, suppress=True)     # runs first, inside the OS hook
            lst = keyboard._listener
            lst.pre_process_event = self._pre_process_event   # replaces the bound method on the instance
        except Exception:
            log.exception("could not install the stuck-key guard (keyboard library changed?)")

    def _sanitize(self, event):
        """OS hook thread, before the library updates its pressed-key table. Must always return True."""
        try:
            sc = event.scan_code
            pressed = keyboard._pressed_events
            if event.event_type == keyboard.KEY_DOWN:
                _prune_stale(skip_scan_code=sc)
                # Windows has not updated the async state for *this* key yet: still "down" means
                # auto-repeat, "up" means a fresh press after a key-up we never saw
                event.is_repeat = sc in pressed and _event_physically_down(event) is not False
                event.combo = tuple(sorted(set(pressed) | {sc}))
                if not event.is_repeat and sc:          # sc 0 = injected dummy keys (doodle's menu mask)
                    for h in list(self._holds.values()):
                        if h["on_interrupt"] and sc not in h["scan_codes"] and h["scan_codes"] & set(pressed):
                            name = event.name or ""
                            self._invoke.emit(lambda f=h["on_interrupt"], n=name: f(n))
            else:
                event.is_repeat = False
                event.combo = tuple(sorted(pressed))
        except Exception:
            pass
        return True

    def _pre_process_event(self, event):
        """Listener thread: the library's matcher, using the key set captured when the event happened."""
        lst = keyboard._listener
        self._current_event = event
        try:
            for key_hook in list(lst.nonblocking_keys.get(event.scan_code, ())):
                key_hook(event)
            combo = getattr(event, "combo", None)
            if combo is None:
                with keyboard._pressed_events_lock:
                    combo = tuple(sorted(keyboard._pressed_events))
            for callback in list(lst.nonblocking_hotkeys.get(combo, ())):
                callback(event)
        finally:
            self._current_event = None
        return event.scan_code or (event.name and event.name != "unknown")

    def _watchdog(self):
        """Main thread, every 100 ms: forget lost key-ups and release hold keys Windows says are up."""
        if self.dry:
            return
        try:
            dropped = _prune_stale(min_age=0.3)
            if dropped:
                log.info("cleared stuck key(s) the hook never saw released: %s", ", ".join(dropped))
        except Exception:
            log.exception("stuck-key watchdog failed")
        now = time.monotonic()
        for key, h in self._holds.items():
            if h["down"] and now - h["since"] > 0.15 and not key_is_physically_down(key):
                log.info("%s: release of %s missed by the hook; releasing", h["owner"], pretty(key))
                h["down"] = False
                self._run(h["on_release"])

    def reset_keys(self, log_it: bool = True):
        """Forget every key the library thinks is held (tray → Reset stuck keys)."""
        if self.dry:
            return
        with keyboard._pressed_events_lock:
            keyboard._pressed_events.clear()
            keyboard._logically_pressed_keys.clear()
            getattr(keyboard._listener, "active_modifiers", set()).clear()
        for h in self._holds.values():
            if h["down"]:
                h["down"] = False
                self._invoke.emit(h["on_release"])
        if log_it:
            log.info("pressed-key state reset")

    # ── main-thread trampoline ─────────────────────────────────────────────
    def _run(self, fn):
        try:
            fn()
        except Exception:
            log.exception("hotkey callback failed")

    # ── registration ───────────────────────────────────────────────────────
    def register(self, owner: str, combo: str, callback, label: str = "", non_consuming: bool = False) -> str:
        """Register a press hotkey. Returns '' on success or an error message. (Keys always reach the focused
        app as well, so `non_consuming` changes nothing here.)"""
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

        def counted():
            if combo != "esc" and not combo.endswith("+esc"):
                usage.record("hotkeys", combo, f"{owner}: {label}" if label else owner)
            self._run(callback)

        def fire():
            if self._suspended:
                return
            ev = self._current_event
            if ev is not None and getattr(ev, "is_repeat", False):
                return                                  # holding the keys down: fire once
            self._invoke.emit(counted)

        try:
            handle = keyboard.add_hotkey(combo, fire, suppress=False, trigger_on_release=False)
        except Exception as e:
            log.warning("%s: could not hook %r: %s", owner, combo, e)
            return str(e)
        self._owned.setdefault(owner, []).append(("hotkey", handle))
        self._in_use[combo] = owner
        log.info("%s: %s -> %s", owner, pretty(combo), label or callback.__name__)
        return ""

    def register_hold(self, owner: str, key: str, on_press, on_release, on_interrupt=None) -> str:
        """Call on_press when `key` goes down and on_release when it comes back up.

        on_interrupt(key_name) is called when another key is pressed while `key` is held
        (Alt+N, Alt+Tab...), so the owner can tell "holding Alt" from "using an Alt shortcut".
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
        try:
            scan_codes = set(keyboard.key_to_scan_codes(key))
        except Exception:
            scan_codes = set()
        state = {"owner": owner, "on_press": on_press, "on_release": on_release, "down": False, "since": 0.0,
                 "on_interrupt": on_interrupt, "scan_codes": scan_codes}

        def hook(event):
            if self._suspended:
                return
            if event.event_type == keyboard.KEY_DOWN:
                if getattr(event, "is_repeat", False) and state["down"]:
                    return                              # auto-repeat
                state["down"], state["since"] = True, time.monotonic()
                self._invoke.emit(on_press)
            else:
                state["down"] = False
                self._invoke.emit(on_release)

        try:
            handle = keyboard.hook_key(key, hook, suppress=False)
        except Exception as e:
            return str(e)
        self._holds[key] = state
        self._owned.setdefault(owner, []).append(("hook", handle))
        log.info("%s: hold %s", owner, pretty(key))
        return ""

    def unregister_all(self, owner: str):
        for kind, handle in self._owned.pop(owner, []):
            try:
                if kind == "hotkey":
                    keyboard.remove_hotkey(handle)
                else:
                    keyboard.unhook(handle)
            except Exception:
                pass
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
        if self.dry:
            return
        try:
            keyboard.unhook_all()
        except Exception:
            pass

    # ── sending keys (used by snippets for paste) ──────────────────────────
    @staticmethod
    def send(combo: str):
        keyboard.send(combo)

    @staticmethod
    def release_modifiers():
        """Let go of any modifier the user is still physically holding (e.g. ctrl after a ctrl+click)."""
        for m in ("ctrl", "alt", "shift", "windows"):
            try:
                if key_is_physically_down(m):
                    keyboard.release(m)
            except Exception:
                pass
