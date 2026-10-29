"""
Calculator: a floating, resizable pop-out with history and named pins.

  - Type a sum, Enter. "×", "÷", "^", "50%", "1,000" and "mod" all work; a sum that starts with an
    operator ("*2") continues from the last answer, and `ans` is the last answer.
  - ↑ / ↓ walk through the history; click a history row to insert its result, double-click to edit it.
  - Pin a result with a name (right-click → Pin…). Pins stay at the top and the name works as a variable:
    pin 950*12 as "rent", then type rent/4.
  - Functions: sqrt cbrt abs round floor ceil sin cos tan asin acos atan ln log log2 exp fact min max hypot;
    constants pi e tau. Trig uses degrees or radians (setting).
  - Ctrl+C with an empty box copies the last result; Ctrl+Enter calculates, copies the result and closes.
  - Conversions and dates (core/convert.py): "5 km to mi", "20 usd to eur", "today + 45 days",
    "days until 25/12/2026".
  - DEC / HEX / BIN / OCT in the header shows whole-number results in that base; 0x1F, 0b101, &, |, xor,
    << and >> work in sums.
  - Pins can go in folders and get a colour (right-click a pin). The history is grouped by day.

Evaluation walks a whitelisted AST (no eval), with limits so 9**9**9 can not hang the launcher.
History and pins are kept in data/calculator.json.
"""
import os
import re
import ast
import math
import time
import operator

from PyQt6.QtWidgets import (
    QFrame, QLineEdit, QListWidget, QListWidgetItem, QVBoxLayout, QHBoxLayout, QGridLayout, QPushButton, QWidget,
    QStyledItemDelegate, QStyle
)
from PyQt6.QtCore import Qt, QTimer, QEvent, QSize, QRect
from PyQt6.QtGui import QColor, QPixmap, QIcon
from datetime import date, timedelta

from core import DATA_DIR, theme, widgets, save_json, load_json, clip, convert
from core.module import Module

SAVE_PATH = os.path.join(DATA_DIR, "calculator.json")
MAX_POW = 10_000
MAX_FACT = 1_000
_angle = {"deg": True}


class CalcError(ValueError):
    pass


def _trig(fn):
    return lambda x: fn(math.radians(x) if _angle["deg"] else x)


def _atrig(fn):
    return lambda x: math.degrees(fn(x)) if _angle["deg"] else fn(x)


def _fact(x):
    if x != int(x) or x < 0:
        raise CalcError("fact() needs a whole number ≥ 0")
    if x > MAX_FACT:
        raise CalcError(f"fact() is limited to {MAX_FACT}")
    return math.factorial(int(x))


def _round(x, n=0):
    return round(x, int(n))


FUNCS = {
    "sqrt": math.sqrt, "cbrt": lambda x: math.copysign(abs(x) ** (1 / 3), x), "abs": abs, "round": _round,
    "floor": math.floor, "ceil": math.ceil, "sin": _trig(math.sin), "cos": _trig(math.cos), "tan": _trig(math.tan),
    "asin": _atrig(math.asin), "acos": _atrig(math.acos), "atan": _atrig(math.atan), "ln": math.log,
    "log": math.log10, "log2": math.log2, "exp": math.exp, "fact": _fact, "factorial": _fact, "min": min,
    "max": max, "hypot": math.hypot,
}
CONSTS = {"pi": math.pi, "e": math.e, "tau": math.tau}
RESERVED = set(FUNCS) | set(CONSTS) | {"ans", "mod"}

_BIN = {ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul, ast.Div: operator.truediv,
        ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod, ast.Pow: operator.pow}
_BITS = {ast.BitAnd: operator.and_, ast.BitOr: operator.or_, ast.BitXor: operator.xor,
         ast.LShift: operator.lshift, ast.RShift: operator.rshift}
BASES = [("dec", "DEC"), ("hex", "HEX"), ("bin", "BIN"), ("oct", "OCT")]
_PLAIN = re.compile(r"-?(?:0[xbo][0-9A-Fa-f ]+|[\d,]*\.?\d+(?:e[+-]?\d+)?)")
PIN_COLOURS = [("Red", "#E5484D"), ("Orange", "#F76B15"), ("Yellow", "#FFC53D"), ("Green", "#30A46C"),
               ("Blue", "#3E63DD"), ("Purple", "#8E4EC6")]
_UN = {ast.UAdd: operator.pos, ast.USub: operator.neg}


def _prepare(text: str) -> str:
    t = text.strip().replace("×", "*").replace("÷", "/").replace("−", "-").replace("^", "**")
    t = re.sub(r"(?<=\d),(?=\d{3}(?!\d))", "", t)                 # 1,000,000
    m = re.fullmatch(r"(.+?)\s*([+-])\s*(\d+(?:\.\d+)?)\s*%", t)   # 54 + 10% -> 54 plus 10 % of it, like a phone
    if m:
        t = f"({m.group(1)})*(1{m.group(2)}{m.group(3)}/100)"
    t = re.sub(r"(\d+(?:\.\d+)?|\))\s*%", r"\1/100", t)          # 50% -> 50/100
    t = re.sub(r"\bmod\b", "%", t)
    t = re.sub(r"\bxor\b", "^", t)          # after ^ became **, so xor is the only ^ left
    t = re.sub(r"\band\b", "&", t)
    t = re.sub(r"\bor\b", "|", t)
    return t


def evaluate(text: str, variables: dict | None = None):
    """Evaluate a sum safely. Raises CalcError (or ZeroDivisionError/ValueError/OverflowError) on bad input."""
    src = _prepare(text)
    if not src:
        raise CalcError("empty")
    try:
        tree = ast.parse(src, mode="eval")
    except SyntaxError:
        raise CalcError("not a sum")
    names = {**CONSTS, **(variables or {})}

    def ev(n):
        if isinstance(n, ast.Expression):
            return ev(n.body)
        if isinstance(n, ast.Constant) and isinstance(n.value, (int, float)) and not isinstance(n.value, bool):
            return n.value
        if isinstance(n, ast.BinOp) and type(n.op) in _BIN:
            a, b = ev(n.left), ev(n.right)
            if isinstance(n.op, ast.Pow) and (abs(b) > MAX_POW or (abs(a) > 1 and abs(b) * math.log10(abs(a)) > 4000)):
                raise CalcError("that power is too big")
            return _BIN[type(n.op)](a, b)
        if isinstance(n, ast.BinOp) and type(n.op) in _BITS:
            a, b = ev(n.left), ev(n.right)
            if a != int(a) or b != int(b):
                raise CalcError("&, |, xor, << and >> need whole numbers")
            if isinstance(n.op, ast.LShift) and b > 4096:
                raise CalcError("that shift is too big")
            return _BITS[type(n.op)](int(a), int(b))
        if isinstance(n, ast.UnaryOp) and type(n.op) in _UN:
            return _UN[type(n.op)](ev(n.operand))
        if isinstance(n, ast.UnaryOp) and isinstance(n.op, ast.Invert):
            v = ev(n.operand)
            if v != int(v):
                raise CalcError("~ needs a whole number")
            return ~int(v)
        if isinstance(n, ast.Name):
            if n.id in names:
                return names[n.id]
            raise CalcError(f"unknown name '{n.id}'")
        if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id in FUNCS and not n.keywords:
            return FUNCS[n.func.id](*[ev(a) for a in n.args])
        raise CalcError("not allowed in a sum")

    value = ev(tree)
    if isinstance(value, float) and (math.isnan(value) or math.isinf(value)):
        raise CalcError("no finite answer")
    return value


def fmt(value, decimals: int = 10, thousands: bool = True, base: str = "dec") -> str:
    if base != "dec" and isinstance(value, (int, float)) and float(value).is_integer() and abs(value) < 2 ** 64:
        v = int(value)
        digits = {"hex": f"{abs(v):X}", "bin": f"{abs(v):b}", "oct": f"{abs(v):o}"}[base]
        if base == "bin" and thousands:
            digits = " ".join(digits[max(0, i - 4):i] for i in range(len(digits), 0, -4)[::-1])
        return ("-" if v < 0 else "") + {"hex": "0x", "bin": "0b", "oct": "0o"}[base] + digits
    if isinstance(value, int) or (isinstance(value, float) and value.is_integer() and abs(value) < 1e15):
        v = int(value)
        return f"{v:,}" if thousands else str(v)
    if abs(value) >= 1e15 or (value != 0 and abs(value) < 10 ** -decimals):
        return f"{value:.{min(decimals, 8)}e}"
    text = f"{value:,.{decimals}f}" if thousands else f"{value:.{decimals}f}"
    return text.rstrip("0").rstrip(".")


def _day_name(day: str) -> str:
    today = date.today()
    if day == today.isoformat():
        return "Today"
    if day == (today - timedelta(days=1)).isoformat():
        return "Yesterday"
    try:
        d = date.fromisoformat(day)
    except ValueError:
        return "Earlier"
    return f"{d:%a} {d.day} {d:%b}" + (f" {d.year}" if d.year != today.year else "")


def pin_name(name: str) -> str:
    n = re.sub(r"\W+", "_", name.strip().lower()).strip("_")
    if not n or n[0].isdigit():
        n = "v_" + n
    return n


class _RowDelegate(QStyledItemDelegate):
    """One line per sum: a ⌖ for pins, the sum in muted mono, the result on the right."""
    ROW_H = 19

    def sizeHint(self, option, index):
        return QSize(100, self.ROW_H)

    def paint(self, p, option, index):
        d = index.data(Qt.ItemDataRole.UserRole)
        if not d:
            return
        pal = theme.P
        r = option.rect
        p.save()
        if "header" in d:
            # a day in the history, a folder of pins: small caps in the muted colour, a hairline after it
            p.setFont(theme.qfont(9.5, 600, mono=True, spacing=0.1))
            p.setPen(QColor(d.get("colour") or pal.text_muted))
            text = d["header"].upper()
            p.drawText(QRect(r.left(), r.top(), r.width(), r.height()), Qt.AlignmentFlag.AlignVCenter, text)
            x = r.left() + p.fontMetrics().horizontalAdvance(text) + 8
            p.setPen(QColor(pal.border))
            p.drawLine(x, r.center().y(), r.right(), r.center().y())
            p.restore()
            return
        if option.state & QStyle.StateFlag.State_MouseOver:
            p.fillRect(r, QColor(pal.bg_card_hover))
        p.setFont(theme.qfont(12, mono=True))
        fm = p.fontMetrics()
        pinned = "var" in d
        if pinned:
            p.setPen(QColor(d.get("colour") or pal.primary_text))
            p.drawText(QRect(r.left(), r.top(), 16, r.height()), Qt.AlignmentFlag.AlignVCenter, "⌖")
        result = d["result"]
        rw = fm.horizontalAdvance(result) + 2
        p.setPen(QColor(pal.text_primary))
        p.drawText(QRect(r.right() - rw, r.top(), rw, r.height()),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight, result)
        p.setPen(QColor(pal.text_muted))
        left = d["var"] if pinned else d["expr"]
        avail = r.width() - 16 - rw - 10
        p.drawText(QRect(r.left() + 16, r.top(), max(10, avail), r.height()), Qt.AlignmentFlag.AlignVCenter,
                   fm.elidedText(left, Qt.TextElideMode.ElideRight, max(10, avail)))
        p.restore()


def _row_list() -> QListWidget:
    lst = QListWidget()
    lst.setItemDelegate(_RowDelegate(lst))
    lst.setUniformItemSizes(True)
    lst.setSpacing(0)
    lst.setMouseTracking(True)
    lst.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
    lst.setStyleSheet("QListWidget { background: transparent; border: none; padding: 0; }"
                      "QListWidget::item { padding: 0; margin: 0 0 7px 0; }"
                      "QListWidget::item:selected, QListWidget::item:hover { background: transparent; }")
    return lst


class CalcPanel(widgets.FloatingPanel):
    def __init__(self, m: "CalculatorModule", embedded: bool = False):
        self.m = m
        self.embedded = embedded            # its contents live in the overlay's Calculator page instead
        super().__init__("Calculator", min_size=(240, 260), opacity=float(m.s("bg_opacity", 0.92)),
                         drag_needs_edit_mode=False, close_hides=True)
        self.base_btn = widgets.button("DEC", "ghost", "Show whole-number results in decimal, hex, binary or octal")
        self.base_btn.setFixedHeight(24)
        self.base_btn.setStyleSheet(f"QPushButton {{ padding: 0 6px; font-size: {theme.fs(10)}px; }}")
        self.base_btn.setFont(theme.qfont(10, 600, mono=True))
        self.base_btn.clicked.connect(self._next_base)
        self.header_right.addWidget(self.base_btn)
        self.keypad_btn = widgets.icon_button("⌗", "Show / hide the keypad", 26, 12, ghost=True)
        self.keypad_btn.clicked.connect(self.toggle_keypad)
        self.header_right.addWidget(self.keypad_btn)
        more = widgets.icon_button("⋯", "More", 26, 12, ghost=True)
        more.clicked.connect(lambda: self._more_menu(more.mapToGlobal(more.rect().bottomLeft())))
        self.header_right.addWidget(more)

        # pinned sums (⌖) over the history, one mono line each
        tape = QFrame()
        widgets.style(tape, f"background: transparent; border: none; border-bottom: 1px solid {theme.P.border};")
        tl = QVBoxLayout(tape)
        tl.setContentsMargins(14, 10, 14, 3)
        tl.setSpacing(0)
        self.pins = _row_list()
        self.pins.itemClicked.connect(lambda it: self._insert(it.data(Qt.ItemDataRole.UserRole)["var"])
                                      if "var" in it.data(Qt.ItemDataRole.UserRole) else None)
        self.pins.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.pins.customContextMenuRequested.connect(self._pin_menu)
        tl.addWidget(self.pins)
        self.history = _row_list()
        self.history.itemClicked.connect(self._history_clicked)
        self.history.itemDoubleClicked.connect(lambda it: self._load(it.data(Qt.ItemDataRole.UserRole)["expr"])
                                               if "expr" in it.data(Qt.ItemDataRole.UserRole) else None)
        self.history.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.history.customContextMenuRequested.connect(self._history_menu)
        tl.addWidget(self.history, 1)
        self.body.addWidget(tape, 1)

        # the live result and the sum being typed, right-aligned
        disp = QVBoxLayout()
        disp.setContentsMargins(14, 14, 14, 12)
        disp.setSpacing(8)
        prow = QHBoxLayout()
        prow.setSpacing(6)
        copy = widgets.icon_button("⧉", "Copy the result (Ctrl+Enter: calculate, copy and close)", 22, 11, ghost=True)
        copy.clicked.connect(self.copy_result)
        prow.addWidget(copy)
        self.preview = widgets.label("", 12, mono=True, tone="muted")
        self.preview.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        prow.addWidget(self.preview, 1)
        disp.addLayout(prow)
        self.input = QLineEdit()
        self.input.setProperty("mono", True)
        self.input.setPlaceholderText("12*4.5 + 10%")
        self.input.setAlignment(Qt.AlignmentFlag.AlignRight)
        self.input.setFont(theme.qfont(26, 500, mono=True))
        self.input.setStyleSheet(f"QLineEdit {{ background: transparent; border: none; padding: 0; font-size: {theme.fs(26)}px; }}")
        self.input.textChanged.connect(self._live)
        self.input.installEventFilter(self)
        disp.addWidget(self.input)
        self.body.addLayout(disp)
        self.keypad = self._build_keypad()
        self.body.addWidget(self.keypad)

        st = m.data.get("panel", {})
        self.resize(int(st.get("w", 296)), int(st.get("h", 440)))
        if "x" in st and "y" in st:
            self.move(int(st["x"]), int(st["y"]))
            self.clamp_to_screens()
        else:
            self.center_on(widgets.cursor_screen_geometry())
        self.keypad.setVisible(bool(st.get("keypad", False)) or embedded)
        self.base_btn.setText(dict(BASES).get(m.base(), "DEC"))
        if embedded:
            self.close_btn.hide()
            self.input.setFont(theme.qfont(32, 500, mono=True))
            self.input.setStyleSheet(f"QLineEdit {{ background: transparent; border: none; padding: 0; font-size: {theme.fs(32)}px; }}")
        else:
            self.geometry_changed.connect(m.schedule_save)
        self.escape_pressed.connect(self._escape)
        self.refresh()

    # ── building ───────────────────────────────────────────────────────────
    def _build_keypad(self) -> QWidget:
        p = theme.P
        w = QWidget()
        g = QGridLayout(w)
        g.setContentsMargins(10, 0, 10, 10)
        g.setSpacing(4)
        keys = [["C", "(", ")", "%", "÷"], ["7", "8", "9", "√", "×"], ["4", "5", "6", "^", "−"],
                ["1", "2", "3", "π", "+"], ["0", ".", "ans", "⌫", "="]]
        ops = {"÷", "×", "−", "+", "%", "^", "√", "π", "(", ")"}
        for r, row in enumerate(keys):
            for c, k in enumerate(row):
                b = QPushButton(k)
                b.setFixedHeight(36)
                b.setCursor(Qt.CursorShape.PointingHandCursor)
                b.setFocusPolicy(Qt.FocusPolicy.NoFocus)
                b.setProperty("mono", True)
                if k == "=":
                    bg, fg, hover = p.primary, p.on_primary, p.primary_hover
                elif k in ops or k in ("C", "ans", "⌫"):
                    bg, fg, hover = p.bg_frame, (p.primary_text if k in ops else p.text_primary), p.bg_card_hover
                else:
                    bg, fg, hover = p.bg_card, p.text_primary, p.bg_card_hover
                fam = ' font-family: "Segoe UI Symbol";' if k == "⌫" else ""   # Geist Mono has no ⌫
                b.setStyleSheet(f"QPushButton {{ background: {bg}; color: {fg}; border: none; border-radius: 6px;"
                                f" padding: 0; font-size: {theme.fs(13)}px; font-weight: 500;{fam} }}"
                                f"QPushButton:hover {{ background: {hover}; }}")
                b.clicked.connect(lambda _, key=k: self._key(key))
                g.addWidget(b, r, c)
        return w

    def _key(self, k: str):
        if k == "=":
            self.calculate()
        elif k == "C":
            self.input.clear()
        elif k == "⌫":
            self.input.backspace()
        else:
            self._insert({"√": "sqrt(", "π": "pi", "−": "-"}.get(k, k))
        self.input.setFocus()

    def toggle_keypad(self):
        self.keypad.setVisible(not self.keypad.isVisible())
        if self.embedded:
            return
        self.m.data.setdefault("panel", {})["keypad"] = self.keypad.isVisible()
        self.m.schedule_save()

    # ── content ────────────────────────────────────────────────────────────
    @staticmethod
    def _plain(result: str) -> str:
        return result.replace(",", "")

    def refresh(self):
        self.pins.clear()
        pins = self.m.data["pins"]
        folders = sorted({p.get("folder", "") for p in pins}, key=lambda f: (f != "", f.lower()))
        rows = 0
        for folder in folders:
            if folder:
                head = QListWidgetItem()
                head.setData(Qt.ItemDataRole.UserRole, {"header": folder})
                head.setFlags(Qt.ItemFlag.NoItemFlags)
                self.pins.addItem(head)
                rows += 1
            for pin in [p for p in pins if p.get("folder", "") == folder]:
                it = QListWidgetItem()
                it.setToolTip(f"{pin['name']}\n{pin['expr']}\nClick to use it in a sum")
                it.setData(Qt.ItemDataRole.UserRole, pin)
                self.pins.addItem(it)
                rows += 1
        has = bool(pins)
        self.pins.setVisible(has)
        self.pins.setFixedHeight(min(6, rows) * (_RowDelegate.ROW_H + 7) if has else 0)
        self.history.clear()
        last_day = None
        for h in self.m.data["history"]:
            day = (h.get("t") or "")[:10]
            if day != last_day:
                last_day = day
                head = QListWidgetItem()
                head.setData(Qt.ItemDataRole.UserRole, {"header": _day_name(day)})
                head.setFlags(Qt.ItemFlag.NoItemFlags)
                self.history.addItem(head)
            it = QListWidgetItem()
            it.setData(Qt.ItemDataRole.UserRole, h)
            it.setToolTip("Click: insert the result · double-click: edit the sum · right-click: pin, copy…")
            self.history.addItem(it)
        self.history.scrollToBottom()

    def _history_clicked(self, it):
        h = it.data(Qt.ItemDataRole.UserRole)
        if "expr" in h:
            self._insert(self.m.plain(h) if h.get("value") is not None else h["result"])

    def _next_base(self):
        keys = [k for k, _ in BASES]
        self.m.set_base(keys[(keys.index(self.m.base()) + 1) % len(keys)])
        self.base_btn.setText(dict(BASES)[self.m.base()])
        self._live(self.input.text())

    def copy_result(self, close: bool = False):
        """Copy what the box works out to (or the last result when it is empty)."""
        t = self.input.text().strip()
        if t:
            try:
                value, result, _expr = self.m.compute(t)
            except Exception:
                self.preview.setText("⚠ nothing to copy")
                return False
            text = self.m.plain({"value": value, "result": result})
        elif self.m.data["history"]:
            text = self.m.plain(self.m.data["history"][-1])
        else:
            return False
        clip.set_text(text)
        self.preview.setText("copied ✓")
        return True

    def _insert(self, text: str):
        self.input.insert(text)
        self.input.setFocus()

    def _load(self, expr: str):
        self.input.setText(expr)
        self.input.setFocus()

    def _live(self, text: str):
        t = text.strip()
        if not t:
            self.preview.setText("")
            return
        try:
            self.preview.setText("= " + self.m.compute(t)[1])
        except Exception:
            self.preview.setText("")

    def calculate(self):
        t = self.input.text().strip()
        if not t:
            return
        try:
            value, result, expr = self.m.compute(t)
        except ZeroDivisionError:
            self.preview.setText("can't divide by 0")
            return False
        except Exception as e:
            self.preview.setText(f"⚠ {e}" if isinstance(e, (CalcError, convert.ConvertError)) else "⚠ not a sum")
            return False
        self.m.add_history(expr, value, result)
        self.input.clear()
        self.preview.setText("")
        self._recall = None
        return True

    # ── keys ───────────────────────────────────────────────────────────────
    _recall = None

    def eventFilter(self, obj, e):
        if obj is self.input and e.type() == QEvent.Type.KeyPress:
            k = e.key()
            if k in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
                if e.modifiers() & Qt.KeyboardModifier.ControlModifier:
                    # calculate, copy and close (or just copy and close the last result)
                    if (not self.input.text().strip() or self.calculate()) and self.copy_result():
                        QTimer.singleShot(250, self._close_after_copy)
                    return True
                self.calculate()
                return True
            if k in (Qt.Key.Key_Up, Qt.Key.Key_Down):
                hist = self.m.data["history"]
                if hist:
                    i = len(hist) if self._recall is None else self._recall
                    i = max(0, min(len(hist), i + (-1 if k == Qt.Key.Key_Up else 1)))
                    self._recall = i
                    self.input.setText(hist[i]["expr"] if i < len(hist) else "")
                return True
            if (k == Qt.Key.Key_C and e.modifiers() & Qt.KeyboardModifier.ControlModifier
                    and not self.input.hasSelectedText() and not self.input.text()):
                if self.m.data["history"]:
                    clip.set_text(self.m.plain(self.m.data["history"][-1]))
                    self.preview.setText("copied ✓")
                return True
        return super().eventFilter(obj, e)

    def _close_after_copy(self):
        launcher = self.m.ctx.launcher
        if self.embedded and launcher is not None:
            launcher.overlay.hide()
        else:
            self.hide()

    def _escape(self):
        if self.input.text():
            self.input.clear()
        else:
            self.hide()

    def showEvent(self, e):
        super().showEvent(e)
        QTimer.singleShot(0, self.input.setFocus)

    # ── menus ──────────────────────────────────────────────────────────────
    def _history_menu(self, pos):
        it = self.history.itemAt(pos)
        if it is None:
            return
        h = it.data(Qt.ItemDataRole.UserRole)
        if "expr" not in h:
            return
        menu = widgets.menu(self)
        menu.addAction("Copy result", lambda: clip.set_text(self.m.plain(h)))
        menu.addAction("Copy sum = result", lambda: clip.set_text(f"{h['expr']} = {h['result']}"))
        menu.addAction("Edit the sum", lambda: self._load(h["expr"]))
        if h.get("value") is not None:
            menu.addAction("📌  Pin with a name…", lambda: self.m.pin(h, self))
        menu.addSeparator()
        menu.addAction("Delete", lambda: self.m.delete_history(h))
        menu.exec(self.history.mapToGlobal(pos))

    def _pin_menu(self, pos):
        it = self.pins.itemAt(pos)
        if it is None:
            return
        pin = it.data(Qt.ItemDataRole.UserRole)
        if "var" not in pin:
            return
        menu = widgets.menu(self)
        menu.addAction("Copy result", lambda: clip.set_text(self.m.plain(pin)))
        menu.addAction(f"Insert '{pin['var']}'", lambda: self._insert(pin["var"]))
        menu.addAction("Edit the sum", lambda: self._load(pin["expr"]))
        menu.addAction("Recalculate", lambda: self.m.recalc_pin(pin))
        menu.addAction("Rename…", lambda: self.m.rename_pin(pin, self))
        folders = menu.addMenu("Folder")
        for f in sorted({p.get("folder", "") for p in self.m.data["pins"]} - {""}, key=str.lower):
            a = folders.addAction(f, lambda f=f: self.m.set_pin(pin, folder=f))
            a.setCheckable(True)
            a.setChecked(pin.get("folder") == f)
        folders.addSeparator()
        folders.addAction("New folder…", lambda: self.m.new_pin_folder(pin, self))
        if pin.get("folder"):
            folders.addAction("Out of the folder", lambda: self.m.set_pin(pin, folder=""))
        colours = menu.addMenu("Colour")
        for name, hexc in PIN_COLOURS:
            px = QPixmap(12, 12)
            px.fill(QColor(hexc))
            a = colours.addAction(QIcon(px), name, lambda c=hexc: self.m.set_pin(pin, colour=c))
            a.setCheckable(True)
            a.setChecked(pin.get("colour") == hexc)
        colours.addSeparator()
        colours.addAction("Accent colour", lambda: self.m.set_pin(pin, colour=""))
        menu.addSeparator()
        menu.addAction("Unpin", lambda: self.m.unpin(pin))
        menu.exec(self.pins.mapToGlobal(pos))

    def _more_menu(self, pos):
        menu = widgets.menu(self)
        menu.addAction("Clear history…", lambda: self.m.clear_history(self))
        menu.addAction("Keypad", self.toggle_keypad)
        base = menu.addMenu("Show results in")
        for k, label in BASES:
            a = base.addAction(label, lambda k=k: (self.m.set_base(k), self.base_btn.setText(dict(BASES)[k])))
            a.setCheckable(True)
            a.setChecked(self.m.base() == k)
        if self.m.s("currency", True):
            day = convert.rates_date()
            menu.addAction(f"Update currency rates{f' (from {day})' if day else ''}",
                           lambda: convert.refresh_rates(force=True))
        menu.addAction("Calculator settings…", lambda: self.m.ctx.open_settings("calculator"))
        menu.exec(pos)


class CalculatorModule(Module):
    key = "calculator"
    title = "Calculator"
    icon = "🧮"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: CalcPanel | None = None
        self._page_calcs: list[CalcPanel] = []        # the ones inside Calculator pages (overlay, dashboard boxes)
        self.data = {"history": [], "pins": [], "panel": {}}
        self._save_timer = QTimer(self)
        self._save_timer.setSingleShot(True)
        self._save_timer.setInterval(500)
        self._save_timer.timeout.connect(self.save_now)

    # ── lifecycle ──────────────────────────────────────────────────────────
    def start(self):
        d = load_json(SAVE_PATH, {})
        d = d if isinstance(d, dict) else {}
        self.data = {"history": [h for h in d.get("history", []) if isinstance(h, dict) and "expr" in h],
                     "pins": [p for p in d.get("pins", []) if isinstance(p, dict) and "var" in p],
                     "panel": d.get("panel", {}) if isinstance(d.get("panel"), dict) else {}}
        _angle["deg"] = self.s("angle", "deg") == "deg"
        self.hotkey("hotkey", self.toggle, "calculator")
        if self.s("currency", True):
            QTimer.singleShot(8000, convert.refresh_rates)
        if self.data["panel"].get("visible"):
            self.toggle()

    def stop(self):
        if self.panel:
            was_open = self.panel.isVisible()
            self._remember_panel()
            self.panel.close()              # its hide sets "visible" False...
            self.panel.deleteLater()
            self.panel = None
            self.data["panel"]["visible"] = was_open    # ...but a restart should bring it back
        self.save_now()

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        _angle["deg"] = self.s("angle", "deg") == "deg"
        self.hotkey("hotkey", self.toggle, "calculator")
        if self.panel:
            self.panel.set_opacity(float(self.s("bg_opacity", 0.92)))
            self._reformat()

    def retheme(self):
        if self.panel:
            visible = self.panel.isVisible()
            self._remember_panel()
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None
            if visible:
                self.toggle()

    def tray_actions(self):
        return [("Calculator", self.toggle)]

    def actions(self):
        return [("toggle", "Calculator", self.toggle)]

    def palette_entries(self, Entry) -> list:
        return [Entry(f"calc:pin:{p['var']}", f"📌 {p['var']} = {p['result']}",
                      lambda r=p["result"]: clip.set_text(r.replace(",", "")),
                      "calculator pin · Enter copies", "calc") for p in self.data["pins"]]

    # ── panel ──────────────────────────────────────────────────────────────
    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = CalcPanel(self)
            self.panel.hidden.connect(self._panel_hidden)
        if self.panel.isVisible() and not self.panel.property("fading_out"):
            self.panel.fade_hide()
        else:
            self.panel.show_and_raise()
            self.data["panel"]["visible"] = True
            self.schedule_save()

    def _panel_hidden(self):
        self._remember_panel()
        self.data["panel"]["visible"] = False
        self.schedule_save()

    def _remember_panel(self):
        if self.panel:
            g = self.panel.geometry()
            self.data["panel"].update({"x": g.x(), "y": g.y(), "w": g.width(), "h": g.height(),
                                       "keypad": self.panel.keypad.isVisible()})

    def refresh_views(self):
        """The pop-out and the overlay's Calculator page show the same history and pins."""
        for v in [self.panel] + self._page_calcs:
            if v is not None:
                try:
                    v.refresh()
                except RuntimeError:
                    pass

    def make_page(self) -> QWidget:
        """The overlay's Calculator page: the calculator itself, larger, with the keypad out."""
        page = QWidget()
        outer = QHBoxLayout(page)
        outer.setContentsMargins(24, 16, 24, 24)
        outer.addStretch()
        with self.themed():
            calc = CalcPanel(self, embedded=True)
        self._page_calcs.append(calc)
        holder = calc.container
        holder.setParent(page)
        holder.setMinimumWidth(460)
        holder.setMaximumWidth(620)
        outer.addWidget(holder, 3)
        outer.addStretch()
        page.on_show = lambda: QTimer.singleShot(0, calc.input.setFocus)
        page.teardown = lambda: (calc.close(), calc.deleteLater(),
                                 self._page_calcs.remove(calc) if calc in self._page_calcs else None)
        return page

    # ── maths ──────────────────────────────────────────────────────────────
    def base(self) -> str:
        b = self.data["panel"].get("base", "dec")
        return b if b in dict(BASES) else "dec"

    def set_base(self, base: str):
        self.data["panel"]["base"] = base
        self._reformat()
        self.schedule_save()

    def fmt(self, value) -> str:
        return fmt(value, int(self.s("decimals", 10)), bool(self.s("thousands", True)), self.base())

    def plain(self, h: dict) -> str:
        """A result as text to paste: no thousands separators, no unit."""
        v = h.get("value")
        if v is None or not isinstance(v, (int, float)):
            return h.get("result", "")
        return fmt(v, int(self.s("decimals", 10)), False, self.base()).replace(" ", "")

    def compute(self, text: str):
        """(value, result text, the sum as stored) for what was typed; raises on a bad sum.
        Dates give no value (None); conversions keep their unit in the text."""
        expr = self.continued(text)
        got = convert.date_math(expr)
        if got is not None:
            value, result = got
            return value, (self.fmt(value) + " days" if value is not None and self.base() != "dec" else result), expr
        variables = self.variables()
        got = convert.convert(expr, lambda e: evaluate(e, variables),
                              convert.rates() if self.s("currency", True) else {})
        if got is not None:
            value, unit = got
            return value, f"{fmt(value, int(self.s('decimals', 10)), bool(self.s('thousands', True)))} {unit}", expr
        value = evaluate(expr, variables)
        return value, self.fmt(value), expr

    def variables(self) -> dict:
        v = {}
        nums = [h["value"] for h in self.data["history"] if isinstance(h.get("value"), (int, float))]
        if nums:
            v["ans"] = nums[-1]
        for p in self.data["pins"]:
            v[p["var"]] = p.get("value", 0)
        return v

    def continued(self, text: str) -> str:
        """'*2' carries on from the last answer."""
        t = text.strip()
        if t and t[0] in "*/×÷^+" and self.data["history"]:
            return "ans" + t
        return t

    def add_history(self, expr: str, value, result: str = ""):
        self.data["history"].append({"expr": expr, "result": result or self.fmt(value), "value": value,
                                     "t": time.strftime("%Y-%m-%d %H:%M")})
        limit = int(self.s("history_limit", 200))
        del self.data["history"][:-limit]
        self.schedule_save()
        self.refresh_views()

    def _reformat(self):
        for h in self.data["history"] + self.data["pins"]:
            if not isinstance(h.get("value"), (int, float)):
                continue
            # plain numbers only: "3.1 mi", "60 days" keep their text
            if "var" in h or _PLAIN.fullmatch(h.get("result", "")):
                h["result"] = self.fmt(h["value"])
        self.refresh_views()

    def delete_history(self, h: dict):
        self.data["history"] = [x for x in self.data["history"] if x is not h]
        self.schedule_save()
        self.refresh_views()

    def clear_history(self, parent):
        if widgets.confirm(parent, "Clear history", "Delete the whole calculator history? Pins are kept.",
                           yes="Clear", danger=True):
            self.data["history"] = []
            self.schedule_save()
            self.refresh_views()

    # ── pins ───────────────────────────────────────────────────────────────
    def pin(self, h: dict, parent):
        name, ok = widgets.ask_text(parent, "Pin", "Name (you can then use it in sums, e.g. rent/4):")
        if not ok or not name.strip():
            return
        var = pin_name(name)
        if var in RESERVED:
            widgets.error(parent, "Pin", f"'{var}' is a built-in name. Pick another.")
            return
        self.data["pins"] = [p for p in self.data["pins"] if p["var"] != var]
        self.data["pins"].append({"name": name.strip(), "var": var, "expr": h["expr"], "result": h["result"],
                                  "value": h.get("value", 0)})
        self.schedule_save()
        self.refresh_views()

    def rename_pin(self, pin: dict, parent):
        name, ok = widgets.ask_text(parent, "Rename pin", "New name:", pin["name"])
        if ok and name.strip():
            var = pin_name(name)
            if var in RESERVED or any(p["var"] == var and p is not pin for p in self.data["pins"]):
                widgets.error(parent, "Pin", f"'{var}' is already used. Pick another.")
                return
            pin.update({"name": name.strip(), "var": var})
            self.schedule_save()
            self.refresh_views()

    def recalc_pin(self, pin: dict):
        try:
            value = evaluate(pin["expr"], {k: v for k, v in self.variables().items() if k != pin["var"]})
        except Exception as e:
            self.ctx.notify(self.title, f"Could not recalculate {pin['var']}: {e}")
            return
        pin.update({"value": value, "result": self.fmt(value)})
        self.schedule_save()
        self.refresh_views()

    def set_pin(self, pin: dict, **changes):
        pin.update(changes)
        self.schedule_save()
        self.refresh_views()

    def new_pin_folder(self, pin: dict, parent):
        name, ok = widgets.ask_text(parent, "Pin folder", "Folder name (e.g. Bills, Work):")
        if ok and name.strip():
            self.set_pin(pin, folder=name.strip())

    def unpin(self, pin: dict):
        self.data["pins"] = [p for p in self.data["pins"] if p is not pin]
        self.schedule_save()
        self.refresh_views()

    # ── saving ─────────────────────────────────────────────────────────────
    def schedule_save(self):
        self._save_timer.start()

    def save_now(self):
        self._save_timer.stop()
        self._remember_panel()
        try:
            save_json(SAVE_PATH, self.data, indent=1)
        except (OSError, TypeError, ValueError):
            self.log.exception("could not save calculator.json")
