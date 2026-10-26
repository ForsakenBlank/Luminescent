"""
Keeping the base running when one part fails.

    alive(w)                is the Qt object behind a Python wrapper still there?
    safely(fn, "what")      run fn, log an error instead of letting it escape
    Faults                  counts errors per part (a module, the overlay...) from the tracebacks the
                            excepthook sees, so a part that keeps failing can be switched off for the session
    StartGuard              notices a start that never got going (twice in a row = safe mode next time)
"""
import os
import time
import json
import logging
import traceback

log = logging.getLogger("guard")

try:
    from PyQt6 import sip
except ImportError:                     # very old PyQt6 wheels shipped sip on its own
    import sip                          # type: ignore


def alive(obj) -> bool:
    if obj is None:
        return False
    try:
        return not sip.isdeleted(obj)
    except TypeError:
        return True                     # not a Qt object at all


def safely(fn, what: str = "", *args, default=None, **kw):
    try:
        return fn(*args, **kw)
    except Exception:
        log.exception("%s failed", what or getattr(fn, "__name__", "a step"))
        return default


def part_of(tb) -> str:
    """Which part a traceback belongs to: "module:<key>", "overlay", "dashboard", "automation", "settings" or ""."""
    frames = traceback.extract_tb(tb)
    for fr in reversed(frames):
        path = fr.filename.replace("\\", "/")
        if "/modules/" in path:
            name = os.path.splitext(os.path.basename(path))[0]
            return "module:" + name.removesuffix("_win").removesuffix("_store")
        for key, marker in (("dashboard", "/ui/dashboard.py"), ("automation", "/ui/automation.py"),
                            ("settings", "/settings_ui.py"), ("overlay", "/ui/")):
            if path.endswith(marker) or (marker.endswith("/") and marker in path):
                return key
    return ""


class Faults:
    """Errors per part within a sliding window. `add` returns True once a part has failed too often."""

    def __init__(self, limit: int = 6, window: float = 60.0):
        self.limit = limit
        self.window = window
        self._seen: dict[str, list[float]] = {}
        self.tripped: set[str] = set()

    def add(self, part: str) -> bool:
        if not part or part in self.tripped:
            return False
        now = time.monotonic()
        hits = [t for t in self._seen.get(part, []) if now - t < self.window] + [now]
        self._seen[part] = hits
        if len(hits) >= self.limit:
            self.tripped.add(part)
            return True
        return False

    def forgive(self, part: str):
        self._seen.pop(part, None)
        self.tripped.discard(part)


class StartGuard:
    """A small file that says "a start is under way". It is removed once the app has run for a while; if it is
    still there at the next start, that start crashed or hung. Two such starts in a row and the next one runs in
    safe mode (no modules, no page boxes, automations paused) so you can still get in and fix things."""

    def __init__(self, path: str):
        self.path = path
        self.failed_starts = 0
        try:
            with open(path, "r", encoding="utf-8") as f:
                self.failed_starts = int(json.load(f).get("failed", 0)) + 1
        except (OSError, ValueError, AttributeError, TypeError):
            self.failed_starts = 0
        self._write(self.failed_starts)

    @property
    def safe_mode(self) -> bool:
        return self.failed_starts >= 2 or bool(os.environ.get("LUMINESCENT_SAFE_MODE"))

    def _write(self, failed: int):
        try:
            with open(self.path, "w", encoding="utf-8") as f:
                json.dump({"failed": failed, "at": time.time()}, f)
        except OSError:
            pass

    def ok(self):
        # the start went fine: a crash later on is not a start-up problem
        try:
            os.remove(self.path)
        except OSError:
            pass
