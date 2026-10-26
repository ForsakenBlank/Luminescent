"""
Metadata Stripper: show and remove EXIF/other metadata from images and videos.

Images use Pillow; videos use ffprobe/ffmpeg if they are on PATH.
"""
import os
import json
import tempfile
import subprocess
from pathlib import Path

from PyQt6.QtWidgets import (
    QLabel, QVBoxLayout, QHBoxLayout, QFrame, QFileDialog, QTextEdit, QApplication
)
from PyQt6.QtCore import Qt, pyqtSignal, QThread, QMimeData, QUrl
from PyQt6.QtGui import QFont, QPixmap

from core import theme, widgets, clip
from core.module import Module

SUPPORTED_IMAGE = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tiff", ".tif"}
SUPPORTED_VIDEO = {".mp4", ".mov", ".avi", ".mkv", ".webm", ".flv", ".wmv"}
_FMT = {".jpg": "JPEG", ".jpeg": "JPEG", ".png": "PNG", ".gif": "GIF", ".webp": "WEBP",
        ".bmp": "BMP", ".tiff": "TIFF", ".tif": "TIFF"}


def read_image_meta(path: Path) -> dict:
    from PIL import Image, ExifTags
    meta = {}
    try:
        with Image.open(path) as img:
            meta["Image:format"] = img.format or ""
            meta["Image:size"] = f"{img.width}x{img.height}"
            meta["Image:mode"] = img.mode
            for k, v in (img.info or {}).items():
                if k in ("exif", "icc_profile", "xmp", "photoshop"):
                    meta[f"Image:{k}"] = f"<{len(v)} bytes>" if isinstance(v, (bytes, bytearray)) else str(v)[:200]
                else:
                    meta[f"Image:{k}"] = str(v)[:300]
            exif = img.getexif()
            for tag_id, val in exif.items():
                tag = ExifTags.TAGS.get(tag_id, str(tag_id))
                meta[f"EXIF:{tag}"] = str(val)[:300]
            try:
                gps = exif.get_ifd(ExifTags.IFD.GPSInfo)
                for tag_id, val in gps.items():
                    meta[f"GPS:{ExifTags.GPSTAGS.get(tag_id, tag_id)}"] = str(val)[:200]
            except Exception:
                pass
    except Exception as e:
        meta["_error"] = str(e)
    return meta


def read_video_meta(path: Path) -> dict:
    meta = {}
    try:
        r = subprocess.run(["ffprobe", "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", str(path)],
                           capture_output=True, text=True, timeout=15)
        if r.returncode != 0:
            meta["_error"] = "ffprobe failed. Install ffmpeg (sudo pacman -S ffmpeg) for video metadata."
            return meta
        data = json.loads(r.stdout)
        fmt = data.get("format", {})
        for k in ("format_name", "duration", "size", "bit_rate"):
            meta[f"Format:{k}"] = str(fmt.get(k, ""))
        for k, v in fmt.get("tags", {}).items():
            meta[f"Format:tag:{k}"] = str(v)
        for i, st in enumerate(data.get("streams", [])):
            for k, v in st.items():
                if k != "tags":
                    meta[f"Stream{i}:{k}"] = str(v)[:200]
            for k, v in st.get("tags", {}).items():
                meta[f"Stream{i}:tag:{k}"] = str(v)
    except FileNotFoundError:
        meta["_error"] = "ffprobe not found. Install ffmpeg: sudo pacman -S ffmpeg"
    except Exception as e:
        meta["_error"] = str(e)
    return meta


def strip_image(src: Path, dst: Path, jpeg_quality: int = 95):
    from PIL import Image, ImageOps
    with Image.open(src) as img:
        fmt = _FMT.get(src.suffix.lower(), "PNG")
        frames_ok = getattr(img, "is_animated", False) and fmt in ("GIF", "WEBP")
        if frames_ok:
            # keep animation but drop everything else
            frames = []
            durations = []
            try:
                while True:
                    frames.append(img.copy().convert("RGBA" if fmt == "WEBP" else "P"))
                    durations.append(img.info.get("duration", 100))
                    img.seek(img.tell() + 1)
            except EOFError:
                pass
            frames[0].save(dst, fmt, save_all=True, append_images=frames[1:], duration=durations,
                           loop=img.info.get("loop", 0))
            return
        # Phone photos are usually stored sideways plus an EXIF "rotate me" tag. Bake the rotation into
        # the pixels first, or the stripped copy (no tag any more) shows up sideways.
        img = ImageOps.exif_transpose(img)
        # rebuild from raw pixels only: no EXIF, ICC, XMP, comments, etc.
        clean = Image.frombytes(img.mode, img.size, img.tobytes())
        if img.mode == "P" and img.getpalette():
            clean.putpalette(img.getpalette())
        kwargs = {}
        if fmt in ("PNG", "GIF") and "transparency" in img.info:
            kwargs["transparency"] = img.info["transparency"]      # pixel data, not metadata
        if fmt == "JPEG":
            if clean.mode not in ("RGB", "L"):
                clean = clean.convert("RGB")
            kwargs = {"quality": jpeg_quality, "subsampling": 0 if jpeg_quality >= 95 else -1}
        clean.save(dst, fmt, **kwargs)


def strip_video(src: Path, dst: Path):
    r = subprocess.run(["ffmpeg", "-y", "-i", str(src), "-map_metadata", "-1", "-map_chapters", "-1",
                        "-c", "copy", str(dst)],
                       capture_output=True, timeout=300)
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode(errors="ignore")[-400:] or "ffmpeg failed")


class Worker(QThread):
    done = pyqtSignal(bool, str)

    def __init__(self, fn):
        super().__init__()
        self.fn = fn

    def run(self):
        try:
            self.fn()
            self.done.emit(True, "")
        except Exception as e:
            self.done.emit(False, str(e))


class DropArea(QFrame):
    file_dropped = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self.setAcceptDrops(True)
        self.setMinimumHeight(130)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._style(False)
        lay = QVBoxLayout(self)
        lay.setAlignment(Qt.AlignmentFlag.AlignCenter)
        icon = QLabel("⬆")
        icon.setFont(QFont(theme.P.font_family, 30))
        icon.setAlignment(Qt.AlignmentFlag.AlignCenter)
        icon.setStyleSheet(f"color: {theme.P.text_muted}; border: none;")
        hint = QLabel("Drop an image or video here, or click to browse")
        hint.setAlignment(Qt.AlignmentFlag.AlignCenter)
        hint.setStyleSheet(f"color: {theme.P.text_muted}; border: none; font-size: 11px;")
        for w in (icon, hint):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            lay.addWidget(w)

    def _style(self, hover: bool):
        p = theme.P
        self.setStyleSheet(f"QFrame {{ background: {p.primary_muted if hover else p.bg_card};"
                           f" border: 2px dashed {p.primary if hover else p.border}; border-radius: {p.radius}px; }}")

    def mousePressEvent(self, e):
        exts = " ".join(f"*{x}" for x in sorted(SUPPORTED_IMAGE | SUPPORTED_VIDEO))
        path, _ = QFileDialog.getOpenFileName(self, "Select file", "", f"Media files ({exts});;All files (*)")
        if path:
            self.file_dropped.emit(path)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()
            self._style(True)

    def dragLeaveEvent(self, e):
        self._style(False)

    def dropEvent(self, e):
        urls = e.mimeData().urls()
        self._style(False)
        if urls:
            self.file_dropped.emit(urls[0].toLocalFile())


class MetaPanel(widgets.FloatingPanel):
    def __init__(self, m: "MetadataModule"):
        self.m = m
        super().__init__("Metadata Stripper", min_size=(520, 560), opacity=0.96)
        self._file: Path | None = None
        self._meta: dict = {}
        self._is_video = False
        self._worker: Worker | None = None
        self.resize(640, 680)
        self.center_on()
        self.escape_pressed.connect(self.hide)

        body = QVBoxLayout()
        body.setContentsMargins(14, 12, 14, 14)
        body.setSpacing(10)

        self.drop = DropArea()
        self.drop.file_dropped.connect(self.load_file)
        body.addWidget(self.drop)

        self.preview = QLabel()
        self.preview.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.preview.setFixedHeight(130)
        self.preview.setStyleSheet(f"background: {theme.P.bg_card}; border: 1px solid {theme.P.border};"
                                   f" border-radius: {theme.P.radius}px;")
        self.preview.hide()
        body.addWidget(self.preview)

        file_row = QHBoxLayout()
        self.file_label = QLabel("No file loaded")
        self.file_label.setStyleSheet(f"color: {theme.P.text_secondary};")
        file_row.addWidget(self.file_label, 1)
        self.new_btn = widgets.button("← Load another file")
        self.new_btn.clicked.connect(self.reset)
        self.new_btn.hide()
        file_row.addWidget(self.new_btn)
        body.addLayout(file_row)

        meta_row = QHBoxLayout()
        title = QLabel("Metadata")
        title.setFont(QFont(theme.P.font_family, 11, QFont.Weight.Bold))
        meta_row.addWidget(title, 1)
        self.copy_btn = widgets.button("📋 Copy as text")
        self.copy_btn.clicked.connect(self.copy_meta)
        self.copy_btn.hide()
        meta_row.addWidget(self.copy_btn)
        body.addLayout(meta_row)

        self.view = QTextEdit()
        self.view.setReadOnly(True)
        self.view.setPlaceholderText("Metadata will appear here after loading a file…")
        self.view.setStyleSheet(f"QTextEdit {{ font-family: {theme.MONO_CSS}; font-size: 11px; }}")
        body.addWidget(self.view, 1)

        self.status = QLabel("")
        self.status.setStyleSheet(f"color: {theme.P.text_muted};")
        self.status.setWordWrap(True)
        body.addWidget(self.status)

        btns = QHBoxLayout()
        btns.setSpacing(8)
        self.strip_copy_btn = widgets.button("🧹 Strip & copy to clipboard", "primary")
        self.strip_copy_btn.clicked.connect(self.strip_and_copy)
        self.strip_save_btn = widgets.button("💾 Strip & save file")
        self.strip_save_btn.clicked.connect(self.strip_and_save)
        for b in (self.strip_copy_btn, self.strip_save_btn):
            b.hide()
            btns.addWidget(b)
        body.addLayout(btns)
        self.body.addLayout(body)

    # ── state ──────────────────────────────────────────────────────────────
    def _set_status(self, text: str, err: bool = False):
        self.status.setStyleSheet(f"color: {'#FF6B6B' if err else theme.P.text_muted};")
        self.status.setText(text)

    def reset(self):
        self._file, self._meta, self._is_video = None, {}, False
        self.file_label.setText("No file loaded")
        self.view.setPlainText("")
        self.preview.hide()
        self.preview.clear()
        self.drop.show()
        for b in (self.new_btn, self.copy_btn, self.strip_copy_btn, self.strip_save_btn):
            b.hide()
        self._set_status("")

    def load_file(self, path: str):
        p = Path(path)
        ext = p.suffix.lower()
        if ext not in SUPPORTED_IMAGE | SUPPORTED_VIDEO:
            self._set_status(f"Unsupported file type: {ext or '(none)'}", err=True)
            return
        self._file = p
        self._is_video = ext in SUPPORTED_VIDEO
        size = p.stat().st_size
        for unit in ("B", "KB", "MB", "GB"):
            if size < 1024:
                break
            size /= 1024
        self.file_label.setText(f"{'🎬' if self._is_video else '🖼️'}  {p.name}  ({size:.1f} {unit})")
        self.drop.hide()
        if not self._is_video:
            px = QPixmap(str(p))
            if not px.isNull():
                self.preview.setPixmap(px.scaled(self.width() - 40, 130, Qt.AspectRatioMode.KeepAspectRatio,
                                                 Qt.TransformationMode.SmoothTransformation))
                self.preview.show()
        self._set_status("Reading metadata…")
        QApplication.processEvents()
        self._meta = read_video_meta(p) if self._is_video else read_image_meta(p)
        if self._meta:
            self.view.setPlainText("\n".join(f"{k}: {v}" for k, v in sorted(self._meta.items())))
        else:
            self.view.setPlainText("No metadata found.")
        for b in (self.new_btn, self.copy_btn, self.strip_copy_btn, self.strip_save_btn):
            b.show()
        n = len([k for k in self._meta if not k.startswith("_")])
        self._set_status(self._meta.get("_error") or f"{n} metadata fields found.", err="_error" in self._meta)

    def copy_meta(self):
        clip.set_text("\n".join(f"{k}: {v}" for k, v in sorted(self._meta.items())))
        self._set_status("Metadata copied to clipboard as text.")

    def _run(self, fn, on_ok, label: str):
        if self._worker and self._worker.isRunning():
            return
        self._set_status(label)
        self.strip_copy_btn.setEnabled(False)
        self.strip_save_btn.setEnabled(False)

        def done(ok, err):
            self.strip_copy_btn.setEnabled(True)
            self.strip_save_btn.setEnabled(True)
            if ok:
                on_ok()
            else:
                self._set_status(f"Error: {err}", err=True)

        self._worker = Worker(fn)
        self._worker.done.connect(done)
        self._worker.start()

    def _strip_to(self, dst: Path):
        if self._is_video:
            strip_video(self._file, dst)
        else:
            strip_image(self._file, dst, int(self.m.s("jpeg_quality", 95)))

    def strip_and_copy(self):
        if not self._file:
            return
        tmp_dir = Path(tempfile.gettempdir()) / "UsefulScripts-meta"
        tmp_dir.mkdir(exist_ok=True)
        dst = tmp_dir / self._file.name

        def ok():
            mime = QMimeData()
            mime.setUrls([QUrl.fromLocalFile(str(dst))])
            clip.set_mime(mime)
            self._set_status("Clean file copied to clipboard — paste it anywhere.")

        self._run(lambda: self._strip_to(dst), ok, "Stripping metadata…")

    def strip_and_save(self):
        if not self._file:
            return
        out_dir = Path(self.m.s("save_dir", ""))
        out_dir.mkdir(parents=True, exist_ok=True)
        dst = out_dir / self._file.name
        i = 1
        while dst.exists():
            dst = out_dir / f"{self._file.stem}_{i}{self._file.suffix}"
            i += 1
        self._run(lambda: self._strip_to(dst), lambda: self._set_status(f"Saved clean file to: {dst}"),
                  "Stripping and saving…")


class MetadataModule(Module):
    key = "metadata"
    title = "Metadata Stripper"
    icon = "🧹"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: MetaPanel | None = None

    def start(self):
        self.hotkey("hotkey", self.toggle, "metadata stripper")

    def stop(self):
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.start()

    def toggle(self):
        if self.panel is None:
            self.panel = MetaPanel(self)
        self.panel.toggle()

    def tray_actions(self):
        return [("Metadata Stripper", self.toggle)]

    def retheme(self):
        if self.panel and not self.panel.isVisible():
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None          # rebuilt in the new colours next time it opens
