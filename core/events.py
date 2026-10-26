"""
A tiny event bus: Luminescent tells it what happened ("timer.finished", "overlay.opened"...)
and the Automation page's event triggers listen. Emitting costs nothing when no one listens.

    from core import events
    events.emit("timer.finished", "Focus")      # name, a short text that goes along
"""
import logging

from PyQt6.QtCore import QObject, pyqtSignal

log = logging.getLogger("events")

# name -> what the Automation page calls it
NAMES = [
    ("luminescent.started", "Luminescent started"),
    ("overlay.opened", "Overlay opened"),
    ("overlay.closed", "Overlay closed"),
    ("theme.changed", "Theme changed"),
    ("dashboard.switched", "Dashboard switched"),
    ("dashboard.item_run", "A dashboard item ran"),
    ("timer.finished", "A timer finished"),
    ("reminder.fired", "A reminder went off"),
    ("hearing.over_limit", "Hearing Guard: over the limit"),
    ("clipboard.changed", "Clipboard changed"),
]


class _Bus(QObject):
    fired = pyqtSignal(str, str)


_bus: _Bus | None = None


def bus() -> _Bus:
    global _bus
    if _bus is None:
        _bus = _Bus()
    return _bus


def emit(name: str, text: str = ""):
    try:
        bus().fired.emit(name, str(text or ""))
    except Exception:
        log.exception("event %s failed", name)
