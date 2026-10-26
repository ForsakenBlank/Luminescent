"""
Animations, all switchable: Settings → Animations has a master switch, a speed and one switch per kind.
Performance mode turns every one of them off.

Windows on Wayland can not fade (Qt has no window opacity there), so fades happen inside a window: a
QGraphicsOpacityEffect on a child for the length of the animation, removed again afterwards (an effect left in
place would re-render that child offscreen on every repaint).

    anim.on("boxes")                           is this kind of animation wanted right now?
    anim.ms(180)                               a duration scaled by the speed setting
    anim.fade(w, 0.0, 1.0, "pages")            fade a child widget in (or out, then=w.hide)
    anim.move(w, QPoint(...), "boxes")         glide a child widget to a new spot
    anim.value(0.0, 1.0, fn, "overlay", ...)   call fn(v) on every frame
"""
from PyQt6.QtCore import QObject, QPropertyAnimation, QVariantAnimation, QEasingCurve, QPoint, QRect
from PyQt6.QtWidgets import QWidget, QGraphicsOpacityEffect

from core.guard import alive

# key, what Settings → Animations calls it, the help under it
KINDS = [
    ("overlay", "Overlay fades in and out", "The dimmed hub and everything on it."),
    ("pages", "Switching pages", "Pages and Settings sections fade in when you switch to them."),
    ("nav", "Sliding marker under the page switcher", ""),
    ("boxes", "Dashboard boxes glide", "Tidy up, folding, new boxes and boxes moved to another screen."),
    ("theme", "Crossfade when the theme changes", "The old look fades into the new one instead of jumping."),
    ("toasts", "Notifications fade in and out", "Luminescent's own notification cards."),
    ("panels", "Panels fade in and out", "Notes, Calculator, Timers and the other floating panels, and the "
                                         "command palette."),
    ("zoom", "Smooth zoom on the Automation page", ""),
    ("pulses", "Pulses travelling along Automation wires", "Only drawn while the Automation page is open."),
]
SPEEDS = [("fast", "Fast"), ("normal", "Normal"), ("relaxed", "Relaxed")]
_FACTOR = {"fast": 0.6, "normal": 1.0, "relaxed": 1.6}

_settings = None
_running: dict[tuple[int, str], QObject] = {}


def init(settings):
    global _settings
    _settings = settings


def on(kind: str = "") -> bool:
    s = _settings
    if s is None or s.get("ui.performance_mode", False) or not s.get("animations.enabled", True):
        return False
    return not kind or bool(s.get(f"animations.{kind}", True))


def ms(base: int) -> int:
    speed = (_settings.get("animations.speed", "normal") if _settings is not None else "normal") or "normal"
    return max(1, int(base * _FACTOR.get(speed, 1.0)))


def _take(obj, prop: str, a: QObject | None):
    # one running animation per (object, property): a new one replaces the old one
    if len(_running) > 200:
        for k in [k for k, v in _running.items() if not alive(v)]:
            _running.pop(k, None)
    key = (id(obj), prop)
    old = _running.pop(key, None)
    if old is not None and alive(old):
        old.stop()
    if a is not None:
        _running[key] = a
        a.finished.connect(lambda k=key, x=a: _running.pop(k, None) if _running.get(k) is x else None)


def stop(obj, prop: str = ""):
    for key in [k for k in _running if k[0] == id(obj) and (not prop or k[1] == prop)]:
        old = _running.pop(key)
        if alive(old):
            old.stop()


def value(start: float, end: float, fn, kind: str, base_ms: int = 180, then=None, owner: QObject | None = None,
          curve=QEasingCurve.Type.OutCubic, tag: str = "value"):
    """fn(v) on every frame from start to end. Off: fn(end) and then() straight away."""
    if not on(kind):
        _take(owner, tag, None) if owner is not None else None
        fn(end)
        if then:
            then()
        return None
    a = QVariantAnimation(owner)
    a.setStartValue(float(start))
    a.setEndValue(float(end))
    a.setDuration(ms(base_ms))
    a.setEasingCurve(curve)

    def frame(v):
        if owner is None or alive(owner):
            fn(float(v))
    a.valueChanged.connect(frame)
    if then:
        a.finished.connect(lambda: then() if owner is None or alive(owner) else None)
    _take(owner if owner is not None else a, tag, a)
    a.start(QVariantAnimation.DeletionPolicy.DeleteWhenStopped)
    return a


def fade(w: QWidget, start: float, end: float, kind: str, base_ms: int = 170, then=None,
         curve=QEasingCurve.Type.OutCubic):
    """Fade a child widget. A widget that already has another graphics effect is left alone."""
    if not alive(w):
        return None
    eff = w.graphicsEffect()
    if eff is not None and not isinstance(eff, QGraphicsOpacityEffect):
        if then:
            then()
        return None
    if not on(kind):
        _take(w, "fade", None)
        if isinstance(eff, QGraphicsOpacityEffect):
            w.setGraphicsEffect(None)
        if then:
            then()
        return None
    if not isinstance(eff, QGraphicsOpacityEffect):
        eff = QGraphicsOpacityEffect(w)
        eff.setOpacity(start)
        w.setGraphicsEffect(eff)

    def done():
        if alive(w) and end >= 0.999 and isinstance(w.graphicsEffect(), QGraphicsOpacityEffect):
            w.setGraphicsEffect(None)            # fully shown: no offscreen rendering from now on
        if then:
            then()
    return value(start, end, lambda v: eff.setOpacity(v) if alive(eff) else None, kind, base_ms, done, w, curve,
                 "fade")


def move(w: QWidget, to: QPoint, kind: str, base_ms: int = 220, then=None, curve=QEasingCurve.Type.OutCubic):
    if not alive(w):
        return None
    if not on(kind) or not w.isVisible() or w.pos() == to:
        _take(w, "pos", None)
        w.move(to)
        if then:
            then()
        return None
    a = QPropertyAnimation(w, b"pos", w)
    a.setStartValue(w.pos())
    a.setEndValue(to)
    a.setDuration(ms(base_ms))
    a.setEasingCurve(curve)
    if then:
        a.finished.connect(lambda: then() if alive(w) else None)
    _take(w, "pos", a)
    a.start(QPropertyAnimation.DeletionPolicy.DeleteWhenStopped)
    return a


def geometry(w: QWidget, to: QRect, kind: str, base_ms: int = 200, then=None, curve=QEasingCurve.Type.OutCubic):
    if not alive(w):
        return None
    if not on(kind) or not w.isVisible():
        _take(w, "geometry", None)
        w.setGeometry(to)
        if then:
            then()
        return None
    a = QPropertyAnimation(w, b"geometry", w)
    a.setStartValue(w.geometry())
    a.setEndValue(to)
    a.setDuration(ms(base_ms))
    a.setEasingCurve(curve)
    if then:
        a.finished.connect(lambda: then() if alive(w) else None)
    _take(w, "geometry", a)
    a.start(QPropertyAnimation.DeletionPolicy.DeleteWhenStopped)
    return a


def busy(w) -> bool:
    return any(k[0] == id(w) for k in _running)
