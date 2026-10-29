"""
App-wide behaviour fixes for Qt's standard controls, installed once at start-up.

* Sliders jump to where you click (Qt's default only steps a page towards the click).
* The mouse wheel scrolls the page you are on. A spin box, combo box or slider under the pointer only takes
  the wheel once you have clicked it (until it loses the focus); before that the wheel goes to the scroll area
  around it, so scrolling down a page no longer changes a value on the way past.
* The text in a spin box (numbers, times) gets a little room from the left border. Style sheets can not pad it:
  Qt puts the spin box's text field flush against the border whatever the padding says.
"""
from PyQt6.QtWidgets import (QApplication, QProxyStyle, QStyle, QAbstractSpinBox, QComboBox, QAbstractSlider,
                             QScrollBar, QAbstractScrollArea, QWidget, QDateTimeEdit)
from PyQt6.QtCore import QObject, QEvent, Qt

_installed = []


class LumStyle(QProxyStyle):
    def styleHint(self, hint, option=None, widget=None, data=None):
        if hint == QStyle.StyleHint.SH_Slider_AbsoluteSetButtons:
            return Qt.MouseButton.LeftButton.value
        if hint == QStyle.StyleHint.SH_Slider_PageSetButtons:
            return Qt.MouseButton.MiddleButton.value
        return super().styleHint(hint, option, widget, data)


def _guarded(obj) -> bool:
    return isinstance(obj, (QAbstractSpinBox, QComboBox, QAbstractSlider)) and not isinstance(obj, QScrollBar)


def scroll_parent(w: QWidget):
    """The nearest scroll area around `w` that can actually scroll, or None."""
    p = w.parentWidget()
    while p is not None:
        if isinstance(p, QAbstractScrollArea) and not _guarded(p):
            if p.verticalScrollBar().maximum() > 0 or p.horizontalScrollBar().maximum() > 0:
                return p
        p = p.parentWidget()
    return None


def _control(obj):
    """`obj` or the spin box / combo box it is part of (their text field gets the clicks)."""
    for _ in range(3):
        if obj is None or not isinstance(obj, QWidget):
            return None
        if _guarded(obj):
            return obj
        obj = obj.parentWidget()
    return None


class WheelGuard(QObject):
    def eventFilter(self, obj, e):
        t = e.type()
        if t == QEvent.Type.Polish and _guarded(obj):
            # a wheel turn must not give these the focus by itself (that would let the next turn change them)
            if obj.focusPolicy() == Qt.FocusPolicy.WheelFocus:
                obj.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
            popup = isinstance(obj, QDateTimeEdit) and obj.calendarPopup()       # that one is padded already
            if isinstance(obj, QAbstractSpinBox) and not popup and obj.lineEdit() is not None:
                obj.lineEdit().setTextMargins(7, 0, 0, 0)
        elif t == QEvent.Type.MouseButtonPress:
            c = _control(obj)
            if c is not None:
                c.setProperty("lum_wheel", True)
        elif t == QEvent.Type.FocusOut and _guarded(obj):
            obj.setProperty("lum_wheel", False)
        elif t == QEvent.Type.Wheel and _guarded(obj) and not (obj.hasFocus() and obj.property("lum_wheel")):
            area = scroll_parent(obj)
            if area is not None:
                QApplication.sendEvent(area.viewport(), e)
                return True
        return False


def install(app: QApplication):
    if _installed:
        return
    app.setStyle(LumStyle())
    guard = WheelGuard(app)
    app.installEventFilter(guard)
    _installed.append(guard)
