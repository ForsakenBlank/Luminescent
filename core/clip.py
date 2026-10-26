"""
Clipboard that works from a hotkey.

On Wayland a program may only set or read the clipboard while one of its
windows has keyboard focus, which is rarely true for a background tool.
wl-copy / wl-paste (wl-clipboard) have no such limit, so they are used when
installed; Qt's clipboard is the fallback.
"""
import shutil
import logging
import subprocess

from PyQt6.QtCore import QBuffer, QByteArray, QIODevice, QMimeData, QUrl
from PyQt6.QtGui import QImage, QPixmap
from PyQt6.QtWidgets import QApplication

from core import hypr

log = logging.getLogger("clipboard")


def _wl() -> bool:
    return hypr.wayland() and bool(shutil.which("wl-copy")) and bool(shutil.which("wl-paste"))


def _copy(data: bytes, mime: str) -> bool:
    try:
        # wl-copy forks and keeps serving the data. Its output must not be a pipe: the forked copy keeps
        # the pipe open, and waiting for it froze Luminescent for the whole timeout.
        r = subprocess.run(["wl-copy", "--type", mime], input=data, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=5)
        if r.returncode == 0:
            return True
        log.warning("wl-copy failed (exit code %s)", r.returncode)
    except (OSError, subprocess.SubprocessError) as e:
        log.warning("wl-copy failed: %s", e)
    return False


def _paste(mime: str) -> bytes:
    try:
        r = subprocess.run(["wl-paste", "--no-newline", "--type", mime], capture_output=True, timeout=5)
        return r.stdout if r.returncode == 0 else b""
    except (OSError, subprocess.SubprocessError):
        return b""


def set_text(text: str):
    if not (_wl() and _copy(text.encode("utf-8"), "text/plain;charset=utf-8")):
        QApplication.clipboard().setText(text)


def set_image(img):
    """A QImage or QPixmap, as PNG."""
    if isinstance(img, QPixmap):
        img = img.toImage()
    if _wl():
        ba = QByteArray()
        buf = QBuffer(ba)
        buf.open(QIODevice.OpenModeFlag.WriteOnly)
        img.save(buf, "PNG")
        buf.close()
        if _copy(bytes(ba), "image/png"):
            return
    QApplication.clipboard().setImage(img)


def set_files(paths: list[str]):
    """Copied files, as file managers and chat apps expect them."""
    if _wl():
        uris = "".join(QUrl.fromLocalFile(p).toString() + "\r\n" for p in paths)
        if _copy(uris.encode("utf-8"), "text/uri-list"):
            return
    md = QMimeData()
    md.setUrls([QUrl.fromLocalFile(p) for p in paths])
    QApplication.clipboard().setMimeData(md)


def set_mime(md: QMimeData):
    if md.hasUrls() and all(u.isLocalFile() for u in md.urls()):
        set_files([u.toLocalFile() for u in md.urls()])
    elif md.hasImage():
        set_image(QImage(md.imageData()))
    elif md.hasText():
        set_text(md.text())
    else:
        QApplication.clipboard().setMimeData(md)


def mime_data() -> QMimeData:
    """What is on the clipboard right now (files, an image and/or text)."""
    if not _wl():
        src = QApplication.clipboard().mimeData()
        md = QMimeData()
        if src is not None:
            if src.hasUrls():
                md.setUrls(src.urls())
            if src.hasImage():
                md.setImageData(QApplication.clipboard().image())
            if src.hasText():
                md.setText(src.text())
        return md
    md = QMimeData()
    try:
        types = subprocess.run(["wl-paste", "--list-types"], capture_output=True, text=True, timeout=5).stdout.split()
    except (OSError, subprocess.SubprocessError):
        types = []
    if "text/uri-list" in types:
        urls = [QUrl(line.strip()) for line in _paste("text/uri-list").decode("utf-8", "replace").splitlines()
                if line.strip() and not line.startswith("#")]
        if urls:
            md.setUrls(urls)
    img_type = next((t for t in ("image/png", "image/jpeg", "image/bmp", "image/webp") if t in types), "")
    if img_type:
        img = QImage.fromData(_paste(img_type))
        if not img.isNull():
            md.setImageData(img)
    text_type = next((t for t in ("text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "TEXT", "STRING")
                      if t in types), "")
    if text_type:
        md.setText(_paste(text_type).decode("utf-8", "replace"))
    return md
