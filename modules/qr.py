"""
QR code maker: the text (or link) on the clipboard as a QR code on screen.

The hotkey reads the clipboard and shows the code big enough to scan from a phone. The text box under
it can be changed and the code follows as you type. Copy image puts the code on the clipboard as a
picture; Save as… writes a PNG (or an SVG when the name ends in .svg). Uses the segno package.
"""
import os

from PyQt6.QtWidgets import QHBoxLayout, QVBoxLayout, QLabel, QPlainTextEdit, QFileDialog
from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtGui import QImage, QPixmap, QGuiApplication

from core import theme, widgets, clip
from core.module import Module

MAX_TEXT = 2900                 # about what a QR code holds as bytes at the lowest error correction


def make_qr(text: str, scale: int = 10, dark: str = "#000000", light: str = "#FFFFFF") -> QImage:
    """`text` as a QR code image (a quiet zone of 4 modules round it)."""
    import io
    import segno
    q = segno.make(text, error="m" if len(text.encode("utf-8")) < 2000 else "l", micro=False)
    buf = io.BytesIO()
    q.save(buf, kind="png", scale=scale, border=4, dark=dark, light=light)
    img = QImage()
    img.loadFromData(buf.getvalue(), "PNG")
    return img


class QrPanel(widgets.FloatingPanel):
    def __init__(self, m: "QrModule"):
        self.m = m
        super().__init__("QR Code", min_size=(340, 460), opacity=0.97, close_hides=True)
        self._img: QImage | None = None
        lay = QVBoxLayout()
        lay.setContentsMargins(16, 14, 16, 16)
        lay.setSpacing(10)
        self.code = QLabel()
        self.code.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.code.setMinimumSize(280, 280)
        self.code.setStyleSheet("background: #FFFFFF; border-radius: 8px;")
        lay.addWidget(self.code, 1)
        self.info = widgets.label("", 11, tone="muted")
        self.info.setWordWrap(True)
        lay.addWidget(self.info)
        self.text = QPlainTextEdit()
        self.text.setPlaceholderText("Text or a link to turn into a QR code")
        self.text.setFixedHeight(70)
        self.text.textChanged.connect(lambda: self._debounce.start())
        lay.addWidget(self.text)
        row = QHBoxLayout()
        row.setSpacing(8)
        paste = widgets.button("From clipboard", "ghost")
        paste.clicked.connect(self.from_clipboard)
        row.addWidget(paste)
        row.addStretch()
        copy = widgets.button("Copy image", "primary")
        copy.clicked.connect(self.copy_image)
        save = widgets.button("Save as…")
        save.clicked.connect(self.save_as)
        row.addWidget(copy)
        row.addWidget(save)
        lay.addLayout(row)
        self.body.addLayout(lay)
        self._debounce = QTimer(self)
        self._debounce.setSingleShot(True)
        self._debounce.setInterval(250)
        self._debounce.timeout.connect(self.render)
        self.resize(380, 540)
        self.center_on(widgets.cursor_screen_geometry())
        self.escape_pressed.connect(self.hide)

    def from_clipboard(self):
        text = QGuiApplication.clipboard().text().strip()
        if not text:
            md = clip.mime_data()
            text = md.text().strip() if md.hasText() else ""
        self.text.setPlainText(text)
        self.render()

    def render(self):
        text = self.text.toPlainText().strip()
        self._img = None
        if not text:
            self.code.setPixmap(QPixmap())
            self.info.setText("Copy some text or a link, then press From clipboard (or type it below).")
            return
        if len(text.encode("utf-8")) > MAX_TEXT:
            self.code.setPixmap(QPixmap())
            self.info.setText(f"Too long for one QR code ({len(text.encode('utf-8'))} bytes, {MAX_TEXT} at most).")
            return
        try:
            self._img = make_qr(text)
        except ImportError:
            self.info.setText("The segno package is missing: .venv/bin/pip install segno")
            return
        except Exception as e:
            self.info.setText(f"Could not make a QR code: {e}")
            return
        side = min(self.code.width(), self.code.height()) - 16
        self.code.setPixmap(QPixmap.fromImage(self._img).scaled(side, side, Qt.AspectRatioMode.KeepAspectRatio,
                                                                Qt.TransformationMode.FastTransformation))
        kind = "link" if text.startswith(("http://", "https://", "www.")) else "text"
        self.info.setText(f"{kind.capitalize()}, {len(text)} character{'s' if len(text) != 1 else ''}. "
                          f"Point a phone camera at it.")

    def resizeEvent(self, e):
        super().resizeEvent(e)
        if self._img is not None:
            QTimer.singleShot(0, self.render)

    def copy_image(self):
        if self._img is None:
            return
        clip.set_image(self._img)
        self.info.setText("QR code copied as a picture.")

    def save_as(self):
        if self._img is None:
            return
        start = os.path.join(os.path.expanduser("~/Pictures"), "qr-code.png")
        path, _ = QFileDialog.getSaveFileName(self, "Save the QR code", start, "PNG image (*.png);;SVG (*.svg)")
        if not path:
            return
        try:
            if path.lower().endswith(".svg"):
                import segno
                segno.make(self.text.toPlainText().strip(), micro=False).save(path, scale=10, border=4)
            else:
                if not path.lower().endswith(".png"):
                    path += ".png"
                if not self._img.save(path, "PNG"):
                    raise OSError("could not write the file")
        except Exception as e:
            widgets.error(self, "Save QR code", str(e))
            return
        self.info.setText(f"Saved {path}")


class QrModule(Module):
    key = "qr"
    title = "QR Code"
    icon = "▦"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: QrPanel | None = None

    def start(self):
        self.hotkey("hotkey", self.show_clipboard, "QR code of the clipboard")

    def stop(self):
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def retheme(self):
        if self.panel and not self.panel.isVisible():
            self.stop()

    def actions(self):
        return [("clipboard", "QR code of the clipboard", self.show_clipboard)]

    def show_clipboard(self):
        if self.panel is None:
            with self.themed():
                self.panel = QrPanel(self)
        if self.panel.isVisible():
            self.panel.hide()
            return
        self.panel.show_and_raise()
        self.panel.from_clipboard()
