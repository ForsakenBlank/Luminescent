"""
Snippets: a grid of files, text and images that paste with one click.

Press the hotkey -> the grid appears over a dimmed screen -> click a card:
  .txt/.md/.py  -> contents copied as text
  anything else -> the file itself is copied (like Ctrl+C in Explorer)
then the window closes and Ctrl+V is sent to whatever you were using.
Ctrl+click also presses Enter and re-opens the grid in the same folder.
Folders open in place; the breadcrumb (or Backspace) goes back up.

The search box looks through every folder (names and text). Drag a card onto another card to put it in a new
place, onto a folder (or a folder in the breadcrumb) to move it there. Right-click → Favourite puts a clip in the
row at the top of the root folder. Resting the mouse on a card shows a bigger preview.
The order, favourites and folder colours live in <snippets folder>/.luminescent.json.

Thumbnails are generated on a background thread so big folders open instantly.
"""
import os
import html
import shutil
import logging
import subprocess
from datetime import datetime

from PyQt6.QtWidgets import (
    QApplication, QWidget, QLabel, QScrollArea, QGridLayout, QVBoxLayout, QHBoxLayout, QFrame, QColorDialog,
    QLineEdit, QSizePolicy
)
from PyQt6.QtCore import Qt, QThread, pyqtSignal, QTimer, QFile, QSize, QRect, QPoint, QMimeData, QUrl
from PyQt6.QtGui import (QPixmap, QImage, QImageReader, QColor, QPainter, QPainterPath, QPen, QShortcut, QKeySequence,
                         QDrag)

from core import theme, widgets, runner, save_json, load_json, clip, desktop
from core.module import Module

log = logging.getLogger("snippets")

TEXT_EXT = {".txt", ".md", ".py"}
IMAGE_EXT = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp"}
VIDEO_EXT = {".mp4", ".mov", ".avi", ".webm", ".mkv"}


def rounded(pixmap: QPixmap, radius: int) -> QPixmap:
    out = QPixmap(pixmap.size())
    out.fill(QColor(0, 0, 0, 0))
    p = QPainter(out)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    path = QPainterPath()
    path.addRoundedRect(0, 0, pixmap.width(), pixmap.height(), radius, radius)
    p.setClipPath(path)
    p.drawPixmap(0, 0, pixmap)
    p.end()
    return out


class ThumbWorker(QThread):
    """Loads thumbnails off the UI thread. Emits (path, QImage-or-None)."""
    ready = pyqtSignal(str, object)

    def __init__(self, jobs: list[tuple[str, str]], size: QSize):
        super().__init__()
        self.jobs = jobs            # [(path, kind)], kind in image/video/link
        self.size = size
        self._stop = False

    def stop(self):
        self._stop = True

    def run(self):
        for path, kind in self.jobs:
            if self._stop:
                return
            img = None
            try:
                if kind == "image":
                    img = self._image(path)
                elif kind == "video":
                    img = self._video(path)
                elif kind == "link":
                    img = self._link(path)
            except Exception:
                log.debug("thumbnail failed for %s", path, exc_info=True)
            if not self._stop:
                self.ready.emit(path, img)

    def _image(self, path: str):
        r = QImageReader(path)
        r.setAutoTransform(True)
        s = r.size()
        if s.isValid():
            r.setScaledSize(s.scaled(self.size * 2, Qt.AspectRatioMode.KeepAspectRatio))
        img = r.read()
        return img if not img.isNull() else None

    def _video(self, path: str):
        try:
            import cv2
        except ImportError:
            return None
        cap = cv2.VideoCapture(path)
        ok, frame = cap.read()
        cap.release()
        if not ok:
            return None
        frame = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        h, w, _ = frame.shape
        return QImage(frame.data, w, h, 3 * w, QImage.Format.Format_RGB888).copy()

    def _link(self, path: str):
        try:
            import requests
        except ImportError:
            return None
        with open(path, "r", encoding="utf-8", errors="ignore") as f:
            url = f.read().strip().splitlines()[0]
        resp = requests.get(url, timeout=5, headers={"User-Agent": "Mozilla/5.0"})
        img = QImage.fromData(resp.content)
        return img if not img.isNull() else None


CODE_EXT = {".py"}
PREVIEW_H = 128
FOOT_H = 34
THUMB = QSize(360, 260)
DRAG_MIME = "application/x-luminescent-snippet"
SEARCH_LIMIT = 200


class _Draggable(QFrame):
    """A card that can be clicked (on release) or dragged onto another card, a folder or the breadcrumb."""
    clicked = pyqtSignal(object)
    menu_requested = pyqtSignal(object)
    dropped = pyqtSignal(str, object)            # dragged path, this card
    hovered = pyqtSignal(object, bool)

    path = ""
    view: "SnippetsView | None" = None

    def _init_drag(self):
        self._press: QPoint | None = None
        self.setAcceptDrops(True)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._press = e.position().toPoint()

    def mouseMoveEvent(self, e):
        if self._press is not None and \
                (e.position().toPoint() - self._press).manhattanLength() >= QApplication.startDragDistance():
            self._press = None
            self.hovered.emit(self, False)
            self._start_drag()

    def mouseReleaseEvent(self, e):
        if self._press is not None and e.button() == Qt.MouseButton.LeftButton:
            self._press = None
            self.clicked.emit(e)

    def _start_drag(self):
        md = QMimeData()
        md.setData(DRAG_MIME, self.path.encode("utf-8"))
        md.setUrls([QUrl.fromLocalFile(self.path)])     # other apps get the file (a copy, never a move)
        drag = QDrag(self)
        drag.setMimeData(md)
        shot = self.grab()
        drag.setPixmap(shot.scaledToWidth(min(180, shot.width()), Qt.TransformationMode.SmoothTransformation))
        drag.setHotSpot(QPoint(20, 20))
        drag.exec(Qt.DropAction.CopyAction)

    def _mark(self, on: bool):
        self.setProperty("dropTarget", on)
        self.setStyleSheet(f"#clipCard {{ border: 1px solid {theme.P.primary}; }}" if on else "")

    def dragEnterEvent(self, e):
        src = bytes(e.mimeData().data(DRAG_MIME)).decode("utf-8") if e.mimeData().hasFormat(DRAG_MIME) else ""
        if src and src != self.path:
            e.acceptProposedAction()
            self._mark(True)
        else:
            e.ignore()

    def dragLeaveEvent(self, e):
        self._mark(False)

    def dropEvent(self, e):
        self._mark(False)
        src = bytes(e.mimeData().data(DRAG_MIME)).decode("utf-8")
        e.acceptProposedAction()
        QTimer.singleShot(0, lambda: self.dropped.emit(src, self))   # after the drag has finished

    def enterEvent(self, e):
        self.hovered.emit(self, True)
        super().enterEvent(e)

    def leaveEvent(self, e):
        self.hovered.emit(self, False)
        super().leaveEvent(e)

    def contextMenuEvent(self, e):
        self.menu_requested.emit(e.globalPos())


class HoverPreview(QFrame):
    """The bigger look at a card: the picture, the whole text or the colour."""

    def __init__(self, parent: QWidget):
        super().__init__(parent, Qt.WindowType.ToolTip | Qt.WindowType.FramelessWindowHint)
        self.setObjectName("clipPreviewTip")
        p = theme.P
        self.setStyleSheet(f"#clipPreviewTip {{ background: {p.bg_base}; border: 1px solid {p.border};"
                           f" border-radius: {p.radius}px; }}")
        lay = QVBoxLayout(self)
        lay.setContentsMargins(10, 10, 10, 10)
        self.label = QLabel()
        self.label.setWordWrap(True)
        self.label.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)
        lay.addWidget(self.label)

    def show_for(self, card: "Card", thumb: QImage | None):
        lbl = self.label
        lbl.setPixmap(QPixmap())
        lbl.setText("")
        lbl.setStyleSheet("")
        lbl.setMinimumSize(0, 0)
        lbl.setMaximumSize(16777215, 16777215)
        kind = card.kind
        if kind == "image":
            r = QImageReader(card.path)
            r.setAutoTransform(True)
            if r.size().isValid():
                r.setScaledSize(r.size().scaled(QSize(560, 420), Qt.AspectRatioMode.KeepAspectRatio)
                                if r.size().width() > 560 or r.size().height() > 420 else r.size())
            img = r.read()
            if img.isNull():
                return
            lbl.setPixmap(QPixmap.fromImage(img))
        elif kind == "video":
            if thumb is None:
                return
            lbl.setPixmap(QPixmap.fromImage(thumb).scaled(QSize(560, 420), Qt.AspectRatioMode.KeepAspectRatio,
                                                         Qt.TransformationMode.SmoothTransformation))
        elif kind in ("text", "code", "link"):
            try:
                with open(card.path, "r", encoding="utf-8", errors="ignore") as f:
                    text = f.read(4000)
            except OSError:
                return
            lbl.setFont(theme.qfont(12, mono=kind == "code"))
            lbl.setTextFormat(Qt.TextFormat.PlainText)
            lbl.setText(text.rstrip() + ("…" if len(text) >= 4000 else ""))
            lbl.setMaximumWidth(520)
            lbl.setMaximumHeight(480)
        elif kind == "colour":
            lbl.setFixedSize(220, 140)
            lbl.setStyleSheet(f"background: {card.colour}; border-radius: 6px;")
        else:
            return
        self.adjustSize()
        g = card.mapToGlobal(QPoint(card.width() + 10, 0))
        scr = QApplication.screenAt(card.mapToGlobal(card.rect().center()))
        area = scr.availableGeometry() if scr else QRect(0, 0, 99999, 99999)
        if g.x() + self.width() > area.right():
            g = card.mapToGlobal(QPoint(-self.width() - 10, 0))
        g.setY(max(area.top(), min(g.y(), area.bottom() - self.height())))
        self.move(g)
        self.show()


def _age(path: str) -> str:
    """How long ago a file changed, the short way: 2m, 3h, yesterday, 4d, 12 Oct."""
    try:
        t = datetime.fromtimestamp(os.path.getmtime(path))
    except OSError:
        return ""
    s = max(0, int((datetime.now() - t).total_seconds()))   # the PC clock can jump ahead: never negative
    if s < 60:
        return "now"
    if s < 3600:
        return f"{s // 60}m"
    if s < 86400:
        return f"{s // 3600}h"
    if s < 2 * 86400:
        return "yesterday"
    if s < 7 * 86400:
        return f"{s // 86400}d"
    return t.strftime("%d %b").lstrip("0")


def _size(path: str) -> str:
    try:
        n = os.path.getsize(path)
    except OSError:
        return ""
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024 or unit == "GB":
            return f"{n:.0f} {unit}" if unit in ("B", "KB") else f"{n:.1f} {unit}"
        n /= 1024
    return ""


class _Picture(QWidget):
    """A card's picture area: the thumbnail cropped to fill it, striped with a mono caption until it loads,
    or a solid colour swatch."""

    def __init__(self, caption: str = "", swatch: str = ""):
        super().__init__()
        self.caption = caption
        self.swatch = swatch
        self._image: QImage | None = None
        self._scaled: QPixmap | None = None
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)

    def set_image(self, img: QImage | None):
        self._image = img
        self._scaled = None
        self.update()

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._scaled = None

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        r = theme.P.radius - 1
        clip = QPainterPath()
        clip.addRoundedRect(0, 0, w, h + r + 2, r, r)          # rounded top corners only
        p.setClipPath(clip)
        pal = theme.P
        if self.swatch:
            p.fillRect(0, 0, w, h, QColor(self.swatch))
            light = theme.luminance(QColor(self.swatch).name()) > 165
            p.setPen(QColor("#18180F" if light else "#FFFFFF"))
            p.setFont(theme.qfont(11, 500, mono=True))
            p.drawText(12, h - 12, self.swatch.upper())
        elif self._image is not None:
            if self._scaled is None or self._scaled.size() != self.size():
                px = QPixmap.fromImage(self._image).scaled(
                    self.size(), Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                    Qt.TransformationMode.SmoothTransformation)
                self._scaled = px.copy((px.width() - w) // 2, (px.height() - h) // 2, w, h)
            p.drawPixmap(0, 0, self._scaled)
        else:
            p.fillRect(0, 0, w, h, QColor(pal.bg_frame))
            p.setPen(QPen(QColor(pal.bg_header), 8))
            for x in range(-h, w + h, 16):
                p.drawLine(x, h, x + h, 0)
            p.setPen(QColor(pal.text_muted))
            p.setFont(theme.qfont(10, mono=True))
            p.drawText(QRect(8, 0, w - 16, h), Qt.AlignmentFlag.AlignCenter,
                       p.fontMetrics().elidedText(self.caption, Qt.TextElideMode.ElideMiddle, w - 16))
        p.end()


class FolderCard(_Draggable):
    def __init__(self, path: str, count: int, colour: str):
        super().__init__()
        self._init_drag()
        self.path = path
        name = os.path.basename(path)
        self.kind = "folder"
        self.search_text = name.lower()
        self.setObjectName("clipCard")
        self.setFixedHeight(52)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(14, 0, 14, 0)
        lay.setSpacing(12)
        icon = QFrame()
        icon.setFixedSize(22, 16)
        icon.setStyleSheet(f"border: 2px solid {colour}; border-radius: 3px; background: transparent;")
        name_lbl = widgets.label(name, 13, 500)
        name_lbl.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        n = widgets.label(f"{count:02d}", 10, mono=True, tone="muted")
        for w in (icon, name_lbl, n):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        lay.addWidget(icon)
        lay.addWidget(name_lbl, 1)
        lay.addWidget(n)


class Card(_Draggable):
    """A clip: a 128 px preview (text, code, link, picture, file or colour) over a footer with its kind and age."""

    def __init__(self, path: str, kind: str, body: str = "", title: str = "", where: str = "", fav: bool = False):
        super().__init__()
        self._init_drag()
        self.kind = kind
        self.path = path
        self.fav = fav                      # in the favourites row
        self.colour = body if kind == "colour" else ""
        self.picture: _Picture | None = None
        self.search_text = f"{os.path.basename(path)} {body}".lower()
        self.setObjectName("clipCard")
        self.setFixedHeight(PREVIEW_H + FOOT_H + 2)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)

        prev = QFrame()
        prev.setObjectName("clipPreview")
        prev.setFixedHeight(PREVIEW_H)
        pl = QVBoxLayout(prev)
        pl.setContentsMargins(0, 0, 0, 1)
        pl.setSpacing(0)
        if kind in ("image", "video"):
            self.picture = _Picture(os.path.basename(path))
            pl.addWidget(self.picture)
        elif kind == "colour":
            self.picture = _Picture(swatch=body)
            pl.addWidget(self.picture)
        elif kind == "file":
            pl.addStretch()
            ext = (os.path.splitext(path)[1].lstrip(".") or "file").upper()[:4]
            doc = widgets.label(ext, 8, 600, mono=True, tone="muted")
            doc.setFixedSize(34, 42)
            doc.setAlignment(Qt.AlignmentFlag.AlignHCenter | Qt.AlignmentFlag.AlignBottom)
            doc.setStyleSheet(f"border: 2px solid {theme.P.text_muted}; border-radius: 4px; padding-bottom: 4px;")
            pl.addWidget(doc, 0, Qt.AlignmentFlag.AlignHCenter)
            pl.addSpacing(10)
            size = _size(path)
            cap = widgets.label(f"{body} · {size}" if size else body, 12)
            cap.setAlignment(Qt.AlignmentFlag.AlignCenter)
            cap.setContentsMargins(8, 0, 8, 0)
            pl.addWidget(cap)
            pl.addStretch()
        elif kind == "link":
            pl.setContentsMargins(14, 14, 14, 8)
            pl.setSpacing(8)
            t = widgets.label(title, 13, 600)
            t.setWordWrap(True)
            u = widgets.label(body, 11, mono=True, tone="accent")
            u.setWordWrap(True)
            pl.addWidget(t)
            pl.addWidget(u)
            pl.addStretch()
        else:   # text / code
            pl.setContentsMargins(14, 12, 14, 8)
            code = kind == "code"
            txt = widgets.label("", 11.5 if code else 13, mono=code, tone="secondary")
            txt.setTextFormat(Qt.TextFormat.RichText)
            txt.setText(f'<div style="line-height:{155 if code else 145}%; white-space:pre-wrap">'
                        f'{html.escape(body)}</div>')
            txt.setWordWrap(True)
            txt.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)
            txt.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Ignored)
            pl.addWidget(txt, 1)
        lay.addWidget(prev)

        foot = QHBoxLayout()
        foot.setContentsMargins(12, 0, 12, 0)
        foot.setSpacing(8)
        foot.addWidget(widgets.label(kind.upper(), 9, 500, mono=True, spacing=0.08, tone="muted"))
        if where:
            w = widgets.label(where, 9.5, mono=True, tone="accent")
            w.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
            foot.addWidget(w, 1)
        foot.addStretch()
        foot.addWidget(widgets.label(_age(path), 10, mono=True, tone="muted"))
        holder = QWidget()
        holder.setFixedHeight(FOOT_H)
        holder.setLayout(foot)
        lay.addWidget(holder)
        for w in self.findChildren(QWidget):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        if kind in ("file",):
            self.setToolTip(os.path.basename(path))     # the others get the bigger hover preview instead

    def set_image(self, image: QImage | None):
        if image is not None and self.picture is not None:
            self.picture.set_image(image)


class SnippetsView(QWidget):
    """The folder grid. Used by the hotkey pop-up window and by the overlay's Clipboard page."""
    title_changed = pyqtSignal(str, bool)       # title, at the root folder
    used = pyqtSignal()                         # a snippet was picked: the host should close

    def __init__(self, m: "SnippetsModule", path: str, embedded: bool = False):
        super().__init__()
        self.m = m
        self.embedded = embedded
        self.cols = 5 if embedded else max(1, int(m.s("card_cols", 4)))
        self.root = os.path.normpath(m.s("base_path", ""))
        self.path = os.path.normpath(path)
        self._worker: ThumbWorker | None = None
        self._cards: dict[str, Card] = {}
        self._all: list[QWidget] = []
        self._preview: HoverPreview | None = None
        self._hover_card = None
        self._hover_timer = QTimer(self)
        self._hover_timer.setSingleShot(True)
        self._hover_timer.setInterval(650)
        self._hover_timer.timeout.connect(self._show_preview)
        self._search_timer = QTimer(self)
        self._search_timer.setSingleShot(True)
        self._search_timer.setInterval(220)
        self._search_timer.timeout.connect(self.load)
        pad = 24 if embedded else 16
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)

        head = QWidget()
        head.setFixedHeight(52)
        bar = QHBoxLayout(head)
        bar.setContentsMargins(pad, 0, pad, 0)
        bar.setSpacing(8)
        self.crumbs = QHBoxLayout()
        self.crumbs.setSpacing(8)
        bar.addLayout(self.crumbs)
        bar.addStretch()
        self.search = QLineEdit()
        self.search.setPlaceholderText("Search every folder")
        self.search.setFixedSize(220, 30)
        self.search.setStyleSheet("QLineEdit { padding-right: 52px; }")
        self.search.textChanged.connect(lambda _t: self._search_timer.start())
        hint = widgets.label("CTRL+F", 10, mono=True, tone="muted")
        hint.setParent(self.search)
        hint.adjustSize()
        hint.move(220 - hint.width() - 10, (30 - hint.height()) // 2)
        hint.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
        bar.addWidget(self.search)
        QShortcut(QKeySequence("Ctrl+F"), self, activated=lambda: (self.search.setFocus(), self.search.selectAll()),
                  context=Qt.ShortcutContext.WidgetWithChildrenShortcut)
        folder = widgets.button("New folder", "ghost")
        folder.clicked.connect(self.new_folder)
        opn = widgets.button("Open folder", "ghost")
        opn.clicked.connect(lambda: runner.open_path(self.path) if os.path.isdir(self.path) else None)
        save = widgets.button("Paste here", "primary", tooltip="Saves the text, image or files on the clipboard "
                                                                 "into this folder")
        save.clicked.connect(self.save_clipboard)
        for b in (folder, opn, save):
            b.setFixedHeight(30)
            bar.addWidget(b)
        lay.addWidget(head)

        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setFrameShape(QFrame.Shape.NoFrame)
        self.scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.grid_widget = QWidget()
        widgets.style(self.grid_widget, "background: transparent;")
        body = QVBoxLayout(self.grid_widget)
        body.setContentsMargins(pad, 4, pad, pad)
        body.setSpacing(20)
        self.fav_head = widgets.label("FAVOURITES", 10, 600, mono=True, spacing=0.12, tone="muted")
        body.addWidget(self.fav_head)
        self.fav_grid = QGridLayout()
        self.folder_grid = QGridLayout()
        self.grid = QGridLayout()
        for g in (self.fav_grid, self.folder_grid, self.grid):
            g.setSpacing(12)
            g.setAlignment(Qt.AlignmentFlag.AlignTop)
            for i in range(self.cols):
                g.setColumnStretch(i, 1)
            body.addLayout(g)
        self.empty = widgets.label("", 13, tone="muted")
        self.empty.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.empty.setWordWrap(True)
        body.addWidget(self.empty)
        body.addStretch()
        self.scroll.setWidget(self.grid_widget)
        lay.addWidget(self.scroll, 1)
        if embedded:
            self.load()

    def focus_target(self):
        self.setFocus()

    # ── navigation ─────────────────────────────────────────────────────────
    def at_root(self) -> bool:
        return os.path.normcase(self.path) == os.path.normcase(self.root)

    def state(self) -> dict:
        """The folder it is in, relative to the snippets folder (a dashboard box remembers it)."""
        rel = os.path.relpath(self.path, self.root)
        return {"folder": "" if rel == "." else rel}

    def restore_state(self, st: dict):
        path = os.path.normpath(os.path.join(self.root, st.get("folder") or ""))
        if os.path.isdir(path) and os.path.normcase(path).startswith(os.path.normcase(self.root)):
            self._open_folder(path)

    def go_up(self):
        if not self.at_root():
            self.path = os.path.dirname(self.path)
            self.load()

    def _open_folder(self, path: str):
        self.path = path
        self.search.clear()
        self.load()

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Backspace and not self.search.hasFocus():
            self.go_up()
            return
        super().keyPressEvent(e)

    def _build_crumbs(self):
        widgets.clear_layout(self.crumbs)
        rel = "" if self.at_root() else os.path.relpath(self.path, self.root)
        parts = [("Clipboard", self.root)]
        acc = self.root
        for part in [p for p in rel.split(os.sep) if p]:
            acc = os.path.join(acc, part)
            parts.append((part, acc))
        for i, (name, target) in enumerate(parts):
            last = i == len(parts) - 1
            if i:
                self.crumbs.addWidget(widgets.label("/", 11, tone="muted"))
            lbl = widgets.label(name, 13, 600 if last else 500, tone="primary" if last else "muted")
            if not last:
                lbl.setCursor(Qt.CursorShape.PointingHandCursor)
                lbl.mousePressEvent = lambda e, t=target: self._open_folder(t)
                self._crumb_drop(lbl, target)
            self.crumbs.addWidget(lbl)

    def _crumb_drop(self, lbl: QLabel, target: str):
        """Drop a card on a folder in the breadcrumb to move it up there."""
        lbl.setAcceptDrops(True)

        def enter(e):
            if e.mimeData().hasFormat(DRAG_MIME):
                e.acceptProposedAction()

        def drop(e):
            src = bytes(e.mimeData().data(DRAG_MIME)).decode("utf-8")
            e.acceptProposedAction()
            QTimer.singleShot(0, lambda: self._move_into(src, target))
        lbl.dragEnterEvent = enter
        lbl.dropEvent = drop

    def _kind(self, path: str) -> tuple[str, str, str]:
        """(kind, body, title) for a file: text, code, link, colour, image, video or file."""
        limit = int(self.m.s("name_limit", 20))
        name = os.path.basename(path)
        short = name if len(name) <= limit else name[:limit] + "…"
        ext = os.path.splitext(name)[1].lower()
        if ext in IMAGE_EXT:
            return "image", "", ""
        if ext in VIDEO_EXT:
            return "video", "", ""
        if ext in TEXT_EXT:
            try:
                with open(path, "r", encoding="utf-8", errors="ignore") as f:
                    content = f.read(600).strip()
            except Exception:
                return "file", short, ""
            one_line = "\n" not in content
            if content.startswith(("http://", "https://")) and one_line:
                return "link", content.split("://", 1)[1], os.path.splitext(name)[0]
            if one_line and len(content) in (4, 7) and content.startswith("#") and QColor(content).isValid():
                return "colour", content, ""
            if ext in CODE_EXT:
                return "code", content[:400], ""
            return "text", content[:300] or short, ""
        return "file", short, ""

    def _connect(self, card, full: str):
        card.view = self
        card.clicked.connect(lambda e, p=full: self._card_clicked(e, p))
        card.menu_requested.connect(lambda pos, p=full: self._card_menu(p, pos))
        card.dropped.connect(self._dropped)
        card.hovered.connect(self._hovered)

    def _file_card(self, full: str, jobs: list, where: str = "", fav: bool = False) -> Card:
        kind, body, title = self._kind(full)
        card = Card(full, kind, body, title, where, fav)
        self._connect(card, full)
        self._all.append(card)
        if kind in ("image", "video"):
            self._cards.setdefault(full, card)
            cached = self.m.thumb_cache.get(full)
            try:
                mtime = os.path.getmtime(full)
            except OSError:
                mtime = None
            if cached and cached[0] == mtime:
                card.set_image(cached[1])
            else:
                jobs.append((full, kind))
        return card

    def _where(self, full: str) -> str:
        rel = os.path.relpath(os.path.dirname(full), self.root)
        return "" if rel == "." else rel

    def load(self):
        self.stop()
        self._hide_preview()
        for g in (self.fav_grid, self.folder_grid, self.grid):
            widgets.clear_layout(g)
        self._cards = {}
        self._all = []
        at_root = self.at_root()
        query = self.search.text().strip().lower()
        self._build_crumbs()
        self.title_changed.emit("Clipboard" if at_root else os.path.basename(self.path), at_root)
        self.empty.setText("")
        self.fav_head.hide()
        jobs = []
        if query:
            self._load_search(query, jobs)
        else:
            if not self._load_folder(at_root, jobs):
                return
        if jobs:
            self._worker = ThumbWorker(jobs, THUMB)
            self._worker.ready.connect(self._thumb_ready)
            self._worker.start()

    def _load_folder(self, at_root: bool, jobs: list) -> bool:
        try:
            names = os.listdir(self.path)
        except Exception as e:
            self.empty.setText(f"Cannot open folder:\n{self.path}\n\n{e}")
            return False
        names = [n for n in names if not n.startswith(".")]
        order = {n: i for i, n in enumerate(self.m.order_of(self.path))}
        dirs = sorted((n for n in names if os.path.isdir(os.path.join(self.path, n))), key=str.lower)
        files = sorted((n for n in names if not os.path.isdir(os.path.join(self.path, n))),
                       key=lambda n: (order.get(n, len(order)), n.lower()))
        if at_root:
            favs = [f for f in self.m.favourites() if os.path.isfile(f)]
            for i, full in enumerate(favs):
                self.fav_grid.addWidget(self._file_card(full, jobs, self._where(full), fav=True),
                                        i // self.cols, i % self.cols)
            self.fav_head.setVisible(bool(favs))
        colours = self.m.folder_colours()
        accents = [theme.P.primary, "#3DBE96", "#E0894E", "#5B9BE6", theme.P.text_muted]
        for nf, n in enumerate(dirs):
            full = os.path.join(self.path, n)
            try:
                count = sum(1 for x in os.listdir(full) if not x.startswith("."))
            except OSError:
                count = 0
            card = FolderCard(full, count, colours.get(os.path.normcase(full)) or accents[nf % len(accents)])
            self._connect(card, full)
            self.folder_grid.addWidget(card, nf // self.cols, nf % self.cols)
            self._all.append(card)
        for nc, n in enumerate(files):
            full = os.path.join(self.path, n)
            self.grid.addWidget(self._file_card(full, jobs), nc // self.cols, nc % self.cols)
        if not names:
            self.empty.setText("This folder is empty. Copy something and press Paste here.")
        return True

    def _load_search(self, query: str, jobs: list):
        """Every folder under the root: folder names, file names and what text clips say."""
        found_dirs, found = [], []
        for dirpath, dirnames, filenames in os.walk(self.root):
            dirnames[:] = sorted(d for d in dirnames if not d.startswith("."))
            for d in dirnames:
                if query in d.lower():
                    found_dirs.append(os.path.join(dirpath, d))
            for n in sorted(filenames, key=str.lower):
                if n.startswith("."):
                    continue
                full = os.path.join(dirpath, n)
                hit = query in n.lower()
                if not hit and os.path.splitext(n)[1].lower() in TEXT_EXT:
                    try:
                        with open(full, "r", encoding="utf-8", errors="ignore") as f:
                            hit = query in f.read(20000).lower()
                    except OSError:
                        pass
                if hit:
                    found.append(full)
            if len(found) >= SEARCH_LIMIT:
                break
        colours = self.m.folder_colours()
        for nf, full in enumerate(found_dirs[:SEARCH_LIMIT]):
            try:
                count = sum(1 for x in os.listdir(full) if not x.startswith("."))
            except OSError:
                count = 0
            card = FolderCard(full, count, colours.get(os.path.normcase(full)) or theme.P.primary)
            self._connect(card, full)
            self.folder_grid.addWidget(card, nf // self.cols, nf % self.cols)
            self._all.append(card)
        for nc, full in enumerate(found[:SEARCH_LIMIT]):
            self.grid.addWidget(self._file_card(full, jobs, self._where(full)), nc // self.cols, nc % self.cols)
        if not found and not found_dirs:
            self.empty.setText(f'Nothing in any folder matches "{query}".')
        elif len(found) >= SEARCH_LIMIT:
            self.empty.setText(f"Showing the first {SEARCH_LIMIT} matches.")

    def _apply_filter(self, text: str):
        self.load()

    # ── drag and drop ──────────────────────────────────────────────────────
    def _dropped(self, src: str, target):
        if not os.path.exists(src):
            return
        if isinstance(target, FolderCard):
            self._move_into(src, target.path)
        elif getattr(target, "fav", False):
            if src in self.m.favourites():
                self.m.reorder_favourite(src, target.path)
            else:
                self.m.set_favourite(src, True, before=target.path)
            self.load()
        elif os.path.isfile(target.path):
            folder = os.path.dirname(target.path)
            if os.path.normcase(os.path.dirname(src)) != os.path.normcase(folder):
                src = self._move_into(src, folder, reload=False)
                if not src:
                    return
            if os.path.isfile(src):
                self.m.place_before(src, target.path)
            self.load()

    def _move_into(self, src: str, folder: str, reload: bool = True) -> str:
        """Move a clip (or folder) into `folder`. Returns its new path, '' if it could not move."""
        if os.path.normcase(os.path.dirname(src)) == os.path.normcase(folder):
            return src
        if os.path.isdir(src) and os.path.normcase(folder + os.sep).startswith(os.path.normcase(src + os.sep)):
            return ""                       # not into itself
        dst = _unique(os.path.join(folder, os.path.basename(src)))
        try:
            shutil.move(src, dst)
        except OSError as e:
            widgets.error(self, "Move", str(e))
            return ""
        self.m.path_moved(src, dst)
        if reload:
            self.load()
        return dst

    # ── hover preview ──────────────────────────────────────────────────────
    def _hovered(self, card, inside: bool):
        if inside and isinstance(card, Card) and card.kind not in ("file",):
            self._hover_card = card
            self._hover_timer.start()
        elif self._hover_card is card or not inside:
            self._hover_card = None
            self._hover_timer.stop()
            self._hide_preview()

    def _show_preview(self):
        card = self._hover_card
        if card is None or not card.isVisible() or not card.underMouse():
            return
        if self._preview is None:
            self._preview = HoverPreview(self.window())
        cached = self.m.thumb_cache.get(card.path)
        try:
            self._preview.show_for(card, cached[1] if cached else None)
        except RuntimeError:
            pass

    def _hide_preview(self):
        if self._preview is not None:
            self._preview.hide()

    def _thumb_ready(self, path: str, image):
        card = self._cards.get(path)
        if card is None or image is None:
            return
        try:
            self.m.thumb_cache[path] = (os.path.getmtime(path), image)
        except OSError:
            pass
        card.set_image(image)

    # ── actions ────────────────────────────────────────────────────────────
    def _card_clicked(self, e, path: str):
        if e.button() != Qt.MouseButton.LeftButton:
            return
        if os.path.isdir(path):
            self._open_folder(path)
            return
        ctrl = bool(e.modifiers() & Qt.KeyboardModifier.ControlModifier)
        self.used.emit()
        self.m.use_snippet(path, send_enter=ctrl, reopen_at=self.path if ctrl and not self.embedded else None)

    def _card_menu(self, path: str, pos):
        m = widgets.menu(self)
        if os.path.isdir(path):
            m.addAction("Open", lambda: self._open_folder(path))
            m.addAction("Colour…", lambda: self._folder_colour(path))
        else:
            m.addAction("Copy (don't paste)", lambda: self.m.copy_snippet(path))
            m.addAction("Open file", lambda: runner.open_path(path))
            fav = path in self.m.favourites()
            m.addAction("☆  Remove from favourites" if fav else "★  Favourite (row at the top)",
                        lambda: (self.m.set_favourite(path, not fav), self.load()))
        m.addAction("Rename…", lambda: self._rename(path))
        m.addAction("Show in the file manager", lambda: desktop.reveal(path))
        m.addSeparator()
        m.addAction("Delete… (to the Trash)", lambda: self._delete(path))
        m.exec(pos)

    def _folder_colour(self, path: str):
        cur = self.m.folder_colours().get(os.path.normcase(path), theme.P.folder_color)
        c = QColorDialog.getColor(QColor(cur), self, "Folder colour")
        if c.isValid():
            self.m.set_folder_colour(path, c.name())
            self.load()

    def _rename(self, path: str):
        name, ok = widgets.ask_text(self, "Rename", "New name:", os.path.basename(path))
        if not ok or not name.strip() or name.strip() == os.path.basename(path):
            return
        dst = os.path.join(os.path.dirname(path), name.strip())
        try:
            os.rename(path, dst)
            self.m.path_moved(path, dst)
        except OSError as e:
            widgets.error(self, "Rename", str(e))
        self.load()

    def _delete(self, path: str):
        if not widgets.confirm(self, "Delete", f'Move "{os.path.basename(path)}" to the Trash?',
                               yes="Delete", danger=True):
            return
        err = recycle(path)
        if err:
            widgets.error(self, "Delete", err)
        self.load()

    def new_folder(self):
        name, ok = widgets.ask_text(self, "New folder", "Folder name:")
        if ok and name.strip():
            try:
                os.makedirs(os.path.join(self.path, name.strip()), exist_ok=False)
            except OSError as e:
                widgets.error(self, "New folder", str(e))
            self.load()

    def save_clipboard(self):
        err, _saved = self.m.save_clipboard(self.path)
        if err:
            widgets.error(self, "Save clipboard", err)
        self.load()

    def stop(self):
        if self._worker:
            self._worker.stop()
            self._worker.wait(500)
            self._worker = None

    def teardown(self):
        self.stop()
        self._hide_preview()

    def hideEvent(self, e):
        self._hide_preview()
        super().hideEvent(e)

    def on_show(self):
        root = os.path.normpath(self.m.s("base_path", ""))
        if os.path.normcase(root) != os.path.normcase(self.root):
            self.root = self.path = root
        self.load()


class SnippetsWindow(widgets.FloatingPanel):
    def __init__(self, m: "SnippetsModule", path: str):
        self.m = m
        super().__init__("Clipboard", min_size=(400, 300), opacity=1.0, resizable=False,
                         close_hides=False)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.view = SnippetsView(m, path)
        self.view.title_changed.connect(lambda t, root: self.set_title("Clipboard", "" if root else t))
        self.view.used.connect(self.close)
        self.body.addWidget(self.view)
        self.resize(int(m.s("window_width", 960)), int(m.s("window_height", 560)))
        self.center_on(widgets.cursor_screen_geometry())
        self._dim = widgets.DimOverlay(float(m.s("overlay_dim", 0.35)), click_through=False, on_click=self.close)
        self.escape_pressed.connect(self.close)
        self.view.load()

    def show_and_raise(self, activate: bool = True):
        self._dim.show()
        super().show_and_raise(activate)

    def closeEvent(self, e):
        self.view.stop()
        self._dim.close()
        self.m.window_closed(self)
        super().closeEvent(e)


def recycle(path: str) -> str:
    """Move a file or folder to the Trash. Returns '' or an error."""
    if QFile.moveToTrash(os.path.abspath(path))[0]:
        return ""
    try:
        r = subprocess.run(["gio", "trash", os.path.abspath(path)], capture_output=True, text=True, timeout=30)
        if r.returncode == 0:
            return ""
        return r.stderr.strip() or "Could not move it to the Trash."
    except (OSError, subprocess.SubprocessError) as e:
        return f"Could not move it to the Trash ({e})."


def _unique(path: str) -> str:
    base, ext = os.path.splitext(path)
    n = 2
    while os.path.exists(path):
        path = f"{base} ({n}){ext}"
        n += 1
    return path


class SnippetsModule(Module):
    key = "snippets"
    title = "Snippets"
    icon = "📋"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.window: SnippetsWindow | None = None
        self.thumb_cache: dict[str, tuple[float, QImage]] = {}

    def start(self):
        self.hotkey("hotkey", self.open, "open snippets")

    def stop(self):
        if self.window:
            self.window.close()
        self.thumb_cache.clear()

    def apply_settings(self):
        self.thumb_cache.clear()
        super().apply_settings()

    def retheme(self):
        if self.window:
            self.window.close()

    def tray_actions(self):
        return [("Open Snippets", self.open)]

    def actions(self):
        return [("open", "Open Snippets", self.open),
                ("save_clipboard", "Save the clipboard to Snippets", self._save_and_tell)]

    def make_view(self, embedded: bool = True, on_used=None) -> QWidget:
        base = self.s("base_path", "")
        if not os.path.isdir(base):
            w = QLabel(f"The snippets folder does not exist:\n{base}\n\nChange it in Settings → Clipboard (Snippets).")
            w.setAlignment(Qt.AlignmentFlag.AlignCenter)
            w.setObjectName("lumHint")
            return w
        v = SnippetsView(self, base, embedded=embedded)
        if on_used:
            v.used.connect(on_used)
        return v

    def open(self, path: str | None = None):
        if self.window is not None:
            try:
                if self.window.isVisible():
                    self.window.close()
                    return
            except RuntimeError:
                self.window = None
        base = self.s("base_path", "")
        if not os.path.isdir(base):
            self.ctx.notify(self.title, f"Snippets folder does not exist:\n{base}\nChange it in Settings → Snippets.")
            return
        with self.themed():
            self.window = SnippetsWindow(self, path or base)
        self.window.show_and_raise()

    def window_closed(self, win):
        if self.window is win:
            self.window = None

    # ── folder colours (stored in <snippets folder>/.luminescent.json) ─────
    def _meta_path(self) -> str:
        return os.path.join(self.s("base_path", ""), ".luminescent.json")

    def _meta(self) -> dict:
        d = load_json(self._meta_path(), {})
        return d if isinstance(d, dict) else {}

    def _save_meta(self, d: dict):
        try:
            save_json(self._meta_path(), d, indent=2)
        except OSError as e:
            self.ctx.notify(self.title, f"Could not save the clipboard folder's settings: {e}")

    def _rel(self, path: str) -> str:
        return os.path.relpath(path, self.s("base_path", ""))

    def _abs(self, rel: str) -> str:
        return os.path.normpath(os.path.join(self.s("base_path", ""), rel))

    def folder_colours(self) -> dict:
        cols = self._meta().get("folder_colours", {})
        return {os.path.normcase(self._abs(k)): v for k, v in cols.items()} if isinstance(cols, dict) else {}

    def set_folder_colour(self, path: str, colour: str):
        d = self._meta()
        d.setdefault("folder_colours", {})[self._rel(path)] = colour
        self._save_meta(d)

    def favourites(self) -> list[str]:
        favs = self._meta().get("favourites", [])
        return [self._abs(f) for f in favs if isinstance(f, str)] if isinstance(favs, list) else []

    def set_favourite(self, path: str, on: bool, before: str = ""):
        d = self._meta()
        favs = [f for f in d.get("favourites", []) if isinstance(f, str) and f != self._rel(path)]
        if on:
            rel_before = self._rel(before) if before else ""
            favs.insert(favs.index(rel_before) if rel_before in favs else len(favs), self._rel(path))
        d["favourites"] = favs
        self._save_meta(d)

    def reorder_favourite(self, path: str, before: str):
        self.set_favourite(path, True, before)

    def order_of(self, folder: str) -> list[str]:
        order = self._meta().get("order", {})
        names = order.get(self._rel(folder), []) if isinstance(order, dict) else []
        return [n for n in names if isinstance(n, str)]

    def place_before(self, path: str, before: str):
        """Put clip `path` just before `before` in their folder's order."""
        folder = os.path.dirname(before)
        try:
            names = [n for n in os.listdir(folder) if not n.startswith(".") and not os.path.isdir(os.path.join(folder, n))]
        except OSError:
            return
        known = {n: i for i, n in enumerate(self.order_of(folder))}
        names.sort(key=lambda n: (known.get(n, len(known)), n.lower()))
        me, target = os.path.basename(path), os.path.basename(before)
        if me in names:
            names.remove(me)
        names.insert(names.index(target) if target in names else len(names), me)
        d = self._meta()
        d.setdefault("order", {})[self._rel(folder)] = names
        self._save_meta(d)

    def path_moved(self, old: str, new: str):
        """A clip or folder was moved or renamed: keep its favourite, colour and place."""
        d = self._meta()
        ro, rn = self._rel(old), self._rel(new)
        favs = d.get("favourites", [])
        if isinstance(favs, list):
            d["favourites"] = [rn + f[len(ro):] if f == ro or f.startswith(ro + os.sep) else f for f in favs]
        cols = d.get("folder_colours", {})
        if isinstance(cols, dict):
            d["folder_colours"] = {(rn + k[len(ro):] if k == ro or k.startswith(ro + os.sep) else k): v
                                   for k, v in cols.items()}
        order = d.get("order", {})
        if isinstance(order, dict):
            folder = self._rel(os.path.dirname(old))
            if folder in order and os.path.basename(old) in order[folder]:
                names = order[folder]
                if os.path.dirname(old) == os.path.dirname(new):
                    names[names.index(os.path.basename(old))] = os.path.basename(new)
                else:
                    names.remove(os.path.basename(old))
        self._save_meta(d)

    # ── clipboard → file ───────────────────────────────────────────────────
    def save_clipboard(self, folder: str) -> tuple[str, list[str]]:
        """Save what is on the clipboard into `folder` as files. Returns (error, saved paths)."""
        stamp = datetime.now().strftime("%Y-%m-%d %H-%M-%S")
        md = clip.mime_data()
        saved = []
        try:
            if md.hasUrls() and any(u.isLocalFile() for u in md.urls()):
                for u in md.urls():
                    src = u.toLocalFile()
                    if not src or not os.path.exists(src):
                        continue
                    dst = _unique(os.path.join(folder, os.path.basename(src.rstrip("/\\"))))
                    (shutil.copytree if os.path.isdir(src) else shutil.copy2)(src, dst)
                    saved.append(dst)
            elif md.hasImage():
                img = QImage(md.imageData())
                dst = _unique(os.path.join(folder, f"Clip {stamp}.png"))
                if img.isNull() or not img.save(dst, "PNG"):
                    return "The image on the clipboard could not be saved.", []
                saved.append(dst)
            elif md.hasText() and md.text().strip():
                text = md.text()
                first = text.strip().splitlines()[0][:40]
                safe = "".join(c for c in first if c.isalnum() or c in " -_").strip() or f"Clip {stamp}"
                dst = _unique(os.path.join(folder, f"{safe}.txt"))
                with open(dst, "w", encoding="utf-8") as f:
                    f.write(text)
                saved.append(dst)
            else:
                return "The clipboard is empty (or holds something that is not text, an image or files).", []
        except OSError as e:
            return str(e), saved
        return "", saved

    def _save_and_tell(self):
        base = self.s("base_path", "")
        if not os.path.isdir(base):
            self.ctx.notify(self.title, f"Snippets folder does not exist:\n{base}")
            return
        err, saved = self.save_clipboard(base)
        self.ctx.notify(self.title, err or f"Saved {', '.join(os.path.basename(s) for s in saved)}")

    # ── using a snippet ────────────────────────────────────────────────────
    def copy_snippet(self, path: str) -> bool:
        ext = os.path.splitext(path)[1].lower()
        if ext in TEXT_EXT:
            try:
                with open(path, "r", encoding="utf-8", errors="ignore") as f:
                    clip.set_text(f.read().strip())
            except Exception as e:
                self.ctx.notify(self.title, f"Could not read {os.path.basename(path)}: {e}")
                return False
        else:
            clip.set_files([os.path.abspath(path)])
        return True

    def use_snippet(self, path: str, send_enter: bool = False, reopen_at: str | None = None):
        if not self.copy_snippet(path):
            return
        if not self.s("auto_paste", True):
            return

        def paste():
            self.hotkeys.release_modifiers()
            self.hotkeys.send("ctrl+v")
            if send_enter:
                QTimer.singleShot(120, lambda: self.hotkeys.send("enter"))
            if reopen_at:
                QTimer.singleShot(450, lambda: self.open(reopen_at))

        # give focus a moment to return to the previous window (and the overlay time to fade) before pasting
        QTimer.singleShot(220, paste)
