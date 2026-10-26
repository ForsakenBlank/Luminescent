"""
Screen text grab (OCR): drag a box around text anywhere on screen; it is copied.

Uses Tesseract (sudo pacman -S tesseract tesseract-data-eng, plus any other
tesseract-data-<language> you read).  The screens are grabbed once when the
hotkey is pressed (grim), dimmed, and you drag a rectangle (Esc or right-click
cancels).  Recognition runs on a worker thread.  If Tesseract is missing only
this module is unavailable.

With "Show the text in an editable box" on, the result opens in a small panel
first: fix what OCR got wrong, then Copy (Ctrl+Enter). Esc throws it away.
"""
import os
import shutil
import tempfile
import threading
import subprocess

from PyQt6.QtWidgets import QApplication, QWidget, QPlainTextEdit, QHBoxLayout
from PyQt6.QtCore import Qt, QRect, QPoint, QTimer, pyqtSignal, QObject
from PyQt6.QtGui import QShortcut, QKeySequence
from PyQt6.QtGui import QPainter, QColor, QPen, QPixmap, QImage

from core import theme, widgets, clip
from core.module import Module


def _languages(exe: str) -> str:
    """Every installed Tesseract language except the 'osd' helper, joined for -l (eng+deu+...)."""
    try:
        out = subprocess.run([exe, "--list-langs"], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return "eng"
    langs = [ln.strip() for ln in out.splitlines()[1:] if ln.strip() and ln.strip() not in ("osd", "snum")]
    if not langs:
        raise RuntimeError("Tesseract has no language data. Run:\nsudo pacman -S tesseract-data-eng")
    return "+".join(["eng"] + [x for x in langs if x != "eng"] if "eng" in langs else langs)


def _ocr_file(path: str, keep_lines: bool) -> str:
    """Blocking: run Tesseract on an image file. Raises RuntimeError with a readable message."""
    exe = shutil.which("tesseract")
    if not exe:
        raise RuntimeError("Tesseract is not installed. Run:\nsudo pacman -S tesseract tesseract-data-eng")
    # --psm 6: one block of text, which is what a dragged box usually is
    r = subprocess.run([exe, path, "-", "-l", _languages(exe), "--psm", "6"],
                       capture_output=True, text=True, timeout=60)
    if r.returncode != 0:
        raise RuntimeError(f"Tesseract failed: {(r.stderr or '').strip()[:200]}")
    lines = [ln.strip() for ln in r.stdout.splitlines() if ln.strip()]
    return "\n".join(lines) if keep_lines else " ".join(lines)


class _Selector(QWidget):
    def __init__(self, m: "TextGrabModule", screen, shot: QPixmap):
        super().__init__()
        self.m = m
        self.shot = shot
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setGeometry(screen.geometry())
        self.setCursor(Qt.CursorShape.CrossCursor)
        self._a: QPoint | None = None
        self._b: QPoint | None = None

    def sel(self) -> QRect:
        return QRect(self._a, self._b).normalized() if self._a and self._b else QRect()

    def paintEvent(self, e):
        p = QPainter(self)
        p.drawPixmap(self.rect(), self.shot)
        p.fillRect(self.rect(), QColor(0, 0, 0, 110))
        r = self.sel()
        if not r.isEmpty():
            p.drawPixmap(r, self.shot, QRect(r.topLeft() * self.shot.devicePixelRatio(),
                                             r.size() * self.shot.devicePixelRatio()))
            p.setPen(QPen(QColor(theme.P.primary), 2))
            p.drawRect(r.adjusted(0, 0, -1, -1))
        else:
            p.setPen(QColor("#FFFFFF"))
            p.drawText(self.rect().adjusted(0, 40, 0, 0), Qt.AlignmentFlag.AlignHCenter,
                       "Drag a box around the text  ·  Esc to cancel")
        p.end()

    def mousePressEvent(self, e):
        if e.button() != Qt.MouseButton.LeftButton:
            self.m.cancel()
            return
        self._a = self._b = e.position().toPoint()
        self.update()

    def mouseMoveEvent(self, e):
        if self._a is not None:
            self._b = e.position().toPoint()
            self.update()

    def mouseReleaseEvent(self, e):
        if self._a is None:
            return
        r = self.sel()
        self._a = self._b = None
        if r.width() < 6 or r.height() < 6:
            self.update()
            return
        dpr = self.shot.devicePixelRatio()
        crop = self.shot.copy(QRect(r.topLeft() * dpr, r.size() * dpr))
        self.m.recognise(crop.toImage())

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Escape:
            self.m.cancel()


class _Bridge(QObject):
    done = pyqtSignal(str, str)          # text, error


class ReviewPanel(widgets.FloatingPanel):
    """The grabbed text in an editable box: Copy (Ctrl+Enter) puts it on the clipboard."""

    def __init__(self, m: "TextGrabModule", text: str):
        self.m = m
        super().__init__("Text Grab", min_size=(320, 180), opacity=0.96, close_hides=False)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.set_title("Text Grab", "check the text")
        self.edit = QPlainTextEdit(text)
        self.edit.setFont(theme.qfont(13))
        self.edit.setStyleSheet("QPlainTextEdit { background: transparent; border: none; padding: 10px 12px; }")
        self.body.addWidget(self.edit, 1)
        row = QHBoxLayout()
        row.setContentsMargins(12, 6, 12, 12)
        row.addWidget(widgets.label("Ctrl+Enter copies · Esc closes", 11, tone="muted"))
        row.addStretch()
        cancel = widgets.button("Cancel", "ghost")
        cancel.clicked.connect(self.close)
        copy = widgets.button("Copy", "primary")
        copy.clicked.connect(self.copy)
        row.addWidget(cancel)
        row.addWidget(copy)
        self.body.addLayout(row)
        for keys in ("Ctrl+Return", "Ctrl+Enter"):
            QShortcut(QKeySequence(keys), self, activated=self.copy)
        self.escape_pressed.connect(self.close)
        lines = text.count("\n") + 1
        self.resize(min(720, max(380, max((len(x) for x in text.splitlines()), default=20) * 8 + 60)),
                    min(560, 130 + lines * 22))
        self.center_on(widgets.cursor_screen_geometry())

    def showEvent(self, e):
        super().showEvent(e)
        QTimer.singleShot(0, lambda: (widgets.bring_to_front(self), self.edit.setFocus()))

    def copy(self):
        text = self.edit.toPlainText()
        if text.strip():
            clip.set_text(text)
            self.m.ctx.notify(self.m.title, "Copied.", 1500)
        self.close()


class TextGrabModule(Module):
    key = "textgrab"
    title = "Text Grab"
    icon = "🔤"

    def __init__(self, ctx):
        super().__init__(ctx)
        self._screens: list[_Selector] = []
        self._busy = False
        self._bridge = _Bridge()
        self._bridge.done.connect(self._finished)

    def start(self):
        self.hotkey("hotkey", self.grab, "grab text")

    def stop(self):
        self.cancel()

    def actions(self):
        return [("grab", "Grab text from the screen (OCR)", self.grab)]

    def grab(self):
        if self._screens or self._busy:
            return
        for scr in QApplication.screens():
            self._screens.append(_Selector(self, scr, widgets.grab_screen(scr)))
        for w in self._screens:
            w.show()
            w.raise_()
        here = widgets.cursor_screen()
        front = next((w for w in self._screens if here and w.geometry() == here.geometry()), self._screens[0])
        widgets.bring_to_front(front)
        front.setFocus()

    def cancel(self):
        for w in self._screens:
            w.close()
            w.deleteLater()
        self._screens = []

    def recognise(self, img: QImage):
        self.cancel()
        if self.s("upscale", True) and img.height() < 200:
            factor = 3 if img.height() < 60 else 2
            img = img.scaled(img.width() * factor, img.height() * factor, Qt.AspectRatioMode.KeepAspectRatio,
                             Qt.TransformationMode.SmoothTransformation)
        if max(img.width(), img.height()) > 9000:
            img = img.scaled(9000, 9000, Qt.AspectRatioMode.KeepAspectRatio, Qt.TransformationMode.SmoothTransformation)
        fd, path = tempfile.mkstemp(suffix=".png", prefix="lum-ocr-")
        os.close(fd)
        if not img.save(path, "PNG"):
            self.ctx.notify(self.title, "Could not prepare the picture for OCR.")
            return
        self._busy = True
        keep = bool(self.s("keep_lines", True))

        def work():
            text, err = "", ""
            try:
                text = _ocr_file(path, keep)
            except Exception as e:
                err = str(e) or type(e).__name__
            finally:
                try:
                    os.remove(path)
                except OSError:
                    pass
            self._bridge.done.emit(text, err)

        threading.Thread(target=work, name="ocr", daemon=True).start()

    def _finished(self, text: str, err: str):
        self._busy = False
        if err:
            self.ctx.toast(self.title, err)
            return
        if not text.strip():
            self.ctx.notify(self.title, "No text found in that area.", 2500)
            return
        if self.s("review", False):
            with self.themed():
                panel = ReviewPanel(self, text.strip())
            panel.show_and_raise()
            return
        clip.set_text(text)
        preview = text.replace("\n", " ")
        self.ctx.notify(self.title, f"Copied: {preview[:60]}{'…' if len(preview) > 60 else ''}", 3000)
