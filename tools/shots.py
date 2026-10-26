"""Render the overlay pages and panels to PNGs without showing anything on screen (WA_DontShowOnScreen + grab).

usage (from the project folder, with a COPY of settings.json in a scratch folder):
    set LUMINESCENT_HOME=<scratch folder>  &  set LUMINESCENT_NO_HOOK=1
    python tools/shots.py . <out dir> [dark|light|<theme id>]
"""
import os
import sys

proj, out = sys.argv[1], sys.argv[2]
theme_id = sys.argv[3] if len(sys.argv) > 3 else "dark"
sys.path.insert(0, proj)
os.makedirs(out, exist_ok=True)

from PyQt6.QtWidgets import QApplication, QWidget
from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtGui import QPixmap, QPainter, QColor, QPen

app = QApplication(sys.argv)
import Launcher as L

lau = L.Launcher(app)
lau.tray.hide()
if theme_id != lau.themes.data["active"]:
    lau.themes.apply(theme_id)
    app.processEvents()

ov = lau.overlay


def offscreen(w: QWidget):
    w.setAttribute(Qt.WidgetAttribute.WA_DontShowOnScreen, True)
    w.show()
    app.processEvents()


def save(w: QWidget, name: str, bg: str = "#2b3440"):
    """Grab `w` over a stand-in desktop (diagonal stripes, like the mock-ups' wallpaper)."""
    app.processEvents()
    px = w.grab()
    canvas = QPixmap(px.size())
    canvas.fill(QColor(bg))
    p = QPainter(canvas)
    p.setPen(QPen(QColor(255, 255, 255, 10), 10))
    for x in range(-px.height(), px.width(), 28):
        p.drawLine(x, px.height(), x + px.height(), 0)
    p.drawPixmap(0, 0, px)
    p.end()
    canvas.save(os.path.join(out, f"{theme_id}-{name}.png"))
    print("saved", name, px.size().width(), px.size().height())


ov._build()
ov._apply_look()
for b in ov._backdrops:
    b.setAttribute(Qt.WidgetAttribute.WA_DontShowOnScreen, True)
prim = ov._primary
prim.show()
app.processEvents()

steps = []
for key in ("dashboard", "themes", "clipboard", "settings"):
    def go(k=key):
        ov.show_page(k)
        app.processEvents()
        save(prim, f"page-{k}")
    steps.append(go)


def settings_overlay():
    page = ov.page("settings")
    page.open_section("ui")
    save(prim, "page-settings-ui")
    page.open_section("calculator")
    save(prim, "page-settings-calc")


steps.append(settings_overlay)


def whole_backdrop():
    ov.show_page("dashboard")
    save(prim, "backdrop", bg="#5a6b7c")


steps.append(whole_backdrop)


def palette():
    from ui.palette import CommandPalette
    pal = CommandPalette(lau)
    pal.entries = lau.palette_entries()
    pal._filter("")
    offscreen(pal)
    pal.adjustSize()
    save(pal, "palette-empty")
    pal.input.setText("not")
    pal._filter("not")
    save(pal, "palette-not")
    pal.input.setText("12*4+10%")
    pal._filter("12*4+10%")
    save(pal, "palette-calc")


steps.append(palette)


def panels():
    calc = lau.modules["calculator"]
    calc.add_history("950*12", 11400)
    calc.add_history("ans/4", 2850)
    calc.data["pins"].append({"name": "rent", "var": "rent", "expr": "950*12", "result": "11,400", "value": 11400})
    from modules.calculator import CalcPanel
    cp = CalcPanel(calc)
    cp.keypad.setVisible(True)
    cp.refresh()
    cp.input.setText("rent/4")
    offscreen(cp)
    save(cp, "calculator")

    tm = lau.modules["timer"]
    from modules.timer import TimerPanel
    tp = TimerPanel(tm, {"mode": "stopwatch", "label": "Run", "laps": [61000, 122500, 190000], "elapsed_ms": 200000})
    tp.resize(280, 260)
    offscreen(tp)
    save(tp, "stopwatch-laps")

    tp2 = TimerPanel(tm, {"mode": "timer", "label": "Focus", "duration_ms": 1500000, "elapsed_ms": 400000})
    tp2.resize(280, 230)
    offscreen(tp2)
    save(tp2, "timer")

    from modules.reminders import ReminderPopup, new_reminder
    rem = {**new_reminder(), "title": "Stand up and stretch", "message": "You have been at the desk for 1h 50m."}
    rp = ReminderPopup(rem, 10)
    offscreen(rp)
    save(rp, "reminder")

    hm = lau.modules.get("hearing")
    if hm is not None:
        from modules.hearing import HearingPanel
        hp = HearingPanel(hm)
        offscreen(hp)
        save(hp, "hearing")

    nm = lau.modules["notes"]
    if nm.panel:
        offscreen(nm.panel)
        nm.panel.show_home()
        nm.panel._search.setText("")
        save(nm.panel, "notes")

    from ui.themes_page import ThemeDialog
    td = ThemeDialog(lau, lau.themes.active())
    offscreen(td)
    save(td, "theme-dialog")

    from ui.cheatsheet import _Sheet
    from PyQt6.QtCore import QRect
    sh = _Sheet(QRect(0, 0, 1600, 900), lau.cheatsheet.groups())
    offscreen(sh)
    save(sh, "cheatsheet", bg="#3a4a5a")

    pl = lau.modules["promptlog"]
    from modules.promptlog import PromptLogPanel
    pp = PromptLogPanel(pl)
    pp.refresh_sources()
    offscreen(pp)
    save(pp, "promptlog")


steps.append(panels)


def run(i=0):
    if i >= len(steps):
        lau.shutdown()
        app.quit()
        return
    try:
        steps[i]()
    except Exception:
        import traceback
        traceback.print_exc()
    QTimer.singleShot(150, lambda: run(i + 1))


QTimer.singleShot(300, run)
app.exec()
