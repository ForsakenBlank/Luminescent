"""
Ghost Vault UI. Storage/crypto lives in vault_store.py.

The vault runs as your normal user. If the configured folder is not writable
you get a clear message pointing at Settings → Ghost Vault.

The search box finds names anywhere in the vault (names only: the contents stay encrypted).
Pictures get thumbnails, decrypted in memory on a worker thread and never written to disk.
"""
from pathlib import Path

from PyQt6.QtWidgets import (
    QApplication, QWidget, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QFrame, QLineEdit,
    QFileDialog, QProgressBar, QScrollArea, QGridLayout, QDialog
)
from PyQt6.QtCore import Qt, pyqtSignal, QThread, QTimer, QPoint, QMimeData, QBuffer, QByteArray, QSize
from PyQt6.QtGui import QFont, QDrag, QImage, QImageReader, QPixmap

from core import theme, widgets, runner, clip
from core.module import Module
from modules.vault_store import VaultStore, VaultError

OPENABLE = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".mp4", ".mov", ".avi", ".mkv", ".mp3", ".wav",
            ".flac", ".ogg", ".pdf", ".txt", ".md", ".py", ".js", ".html", ".csv", ".docx", ".xlsx", ".pptx", ".json"}
ICONS = {".png": "🖼️", ".jpg": "🖼️", ".jpeg": "🖼️", ".gif": "🖼️", ".webp": "🖼️", ".mp4": "🎬", ".mov": "🎬",
         ".avi": "🎬", ".mkv": "🎬", ".mp3": "🎵", ".wav": "🎵", ".flac": "🎵", ".pdf": "📕", ".txt": "📝",
         ".py": "🐍", ".zip": "🗜️", ".rar": "🗜️", ".7z": "🗜️"}


THUMB_EXT = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"}
THUMB_PX = 40


class ThumbLoader(QThread):
    """Decrypts pictures in memory and shrinks them to thumbnails. Emits (name, QImage)."""
    ready = pyqtSignal(str, QImage)

    def __init__(self, store: VaultStore, path: list[str], names: list[str]):
        super().__init__()
        self.store, self.path, self.names = store, list(path), names
        self._stop = False

    def stop(self):
        self._stop = True

    def run(self):
        for name in self.names:
            if self._stop:
                return
            try:
                data = self.store.read_bytes(self.path, name)
                buf = QBuffer()
                buf.setData(QByteArray(data))
                buf.open(QBuffer.OpenModeFlag.ReadOnly)
                r = QImageReader(buf)
                r.setAutoTransform(True)
                if r.size().isValid():
                    r.setScaledSize(r.size().scaled(QSize(THUMB_PX * 2, THUMB_PX * 2),
                                                    Qt.AspectRatioMode.KeepAspectRatioByExpanding))
                img = r.read()
                del data
                if not img.isNull() and not self._stop:
                    self.ready.emit(name, img)
            except Exception:
                continue


# ── dialogs ───────────────────────────────────────────────────────────────────
class PasswordDialog(QDialog):
    def __init__(self, title: str, prompt: str = "Password:", confirm: bool = False,
                 allow_empty: bool = False, forgot: bool = False, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setWindowFlags(Qt.WindowType.Dialog | Qt.WindowType.WindowStaysOnTopHint)
        self.setMinimumWidth(380)
        self.forgot_clicked = False
        self._confirm = confirm
        self._allow_empty = allow_empty

        lay = QVBoxLayout(self)
        lay.setSpacing(10)
        lay.setContentsMargins(20, 20, 20, 20)
        lay.addWidget(QLabel(prompt))
        self.pw = QLineEdit()
        self.pw.setEchoMode(QLineEdit.EchoMode.Password)
        self.pw.returnPressed.connect(self._ok)
        lay.addWidget(self.pw)
        if confirm:
            lay.addWidget(QLabel("Confirm:"))
            self.pw2 = QLineEdit()
            self.pw2.setEchoMode(QLineEdit.EchoMode.Password)
            self.pw2.returnPressed.connect(self._ok)
            lay.addWidget(self.pw2)
        show = QPushButton("Show password")
        show.setProperty("variant", "ghost")
        show.setCheckable(True)
        show.toggled.connect(lambda on: [w.setEchoMode(QLineEdit.EchoMode.Normal if on else QLineEdit.EchoMode.Password)
                                         for w in ([self.pw] + ([self.pw2] if confirm else []))])
        lay.addWidget(show, 0, Qt.AlignmentFlag.AlignLeft)
        self.err = QLabel("")
        self.err.setStyleSheet(f"color: #FF6B6B; font-size: {theme.fs(11)}px;")
        lay.addWidget(self.err)
        row = QHBoxLayout()
        if forgot:
            fb = QPushButton("Forgot password? Use recovery key")
            fb.setProperty("variant", "ghost")
            fb.clicked.connect(self._forgot)
            row.addWidget(fb)
        row.addStretch()
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("OK", "primary")
        ok.clicked.connect(self._ok)
        row.addWidget(cancel)
        row.addWidget(ok)
        lay.addLayout(row)
        self.pw.setFocus()

    def _forgot(self):
        self.forgot_clicked = True
        self.reject()

    def _ok(self):
        pw = self.pw.text()
        if not pw and not self._allow_empty:
            self.err.setText("Password cannot be empty.")
            return
        if self._confirm and pw != self.pw2.text():
            self.err.setText("Passwords do not match.")
            return
        self.accept()

    def password(self) -> str:
        return self.pw.text()


class RecoveryKeyDialog(QDialog):
    def __init__(self, key_text: str, saved_to: str = "", parent=None):
        super().__init__(parent)
        self.setWindowTitle("Vault recovery key")
        self.setWindowFlags(Qt.WindowType.Dialog | Qt.WindowType.WindowStaysOnTopHint)
        self.setMinimumWidth(460)
        lay = QVBoxLayout(self)
        lay.setSpacing(10)
        lay.setContentsMargins(20, 20, 20, 20)
        info = QLabel("This key unlocks the vault if you forget the password. Store it somewhere safe "
                      "(password manager, printed, another drive). Anyone with it can open the vault.")
        info.setWordWrap(True)
        lay.addWidget(info)
        box = QLineEdit(key_text)
        box.setReadOnly(True)
        box.setAlignment(Qt.AlignmentFlag.AlignCenter)
        box.setStyleSheet(f"font-family: {theme.MONO_CSS}; font-size: {theme.fs(15)}px; letter-spacing: 1px;")
        lay.addWidget(box)
        if saved_to:
            note = QLabel(f"A copy was written to:\n{saved_to}\nMove it somewhere safe.")
            note.setWordWrap(True)
            note.setStyleSheet(f"color: {theme.P.text_secondary}; font-size: {theme.fs(11)}px;")
            lay.addWidget(note)
        row = QHBoxLayout()
        copy = widgets.button("Copy")
        copy.clicked.connect(lambda: clip.set_text(key_text))
        save = widgets.button("Save to file…")
        save.clicked.connect(lambda: self._save(key_text))
        row.addWidget(copy)
        row.addWidget(save)
        row.addStretch()
        ok = widgets.button("I have saved it", "primary")
        ok.clicked.connect(self.accept)
        row.addWidget(ok)
        lay.addLayout(row)

    def _save(self, text):
        path, _ = QFileDialog.getSaveFileName(self, "Save recovery key", "vault_recovery.key", "Key file (*.key);;All files (*)")
        if path:
            Path(path).write_text(f"GHOST VAULT RECOVERY KEY\n{text}\n", encoding="utf-8")


# ── worker ────────────────────────────────────────────────────────────────────
class Worker(QThread):
    progress = pyqtSignal(int)
    done = pyqtSignal(bool, str)

    def __init__(self, fn):
        super().__init__()
        self.fn = fn

    def run(self):
        try:
            self.fn(self.progress.emit)
            self.done.emit(True, "")
        except Exception as e:
            self.done.emit(False, str(e))


# ── cards ─────────────────────────────────────────────────────────────────────
class DropZone(QFrame):
    files_dropped = pyqtSignal(list)

    def __init__(self):
        super().__init__()
        self.setAcceptDrops(True)
        self.setMinimumHeight(90)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._style(False)
        lay = QVBoxLayout(self)
        lay.setAlignment(Qt.AlignmentFlag.AlignCenter)
        a = QLabel("⬆")
        a.setFont(QFont(theme.P.font_family, 22))
        a.setAlignment(Qt.AlignmentFlag.AlignCenter)
        a.setStyleSheet(f"color: {theme.P.text_muted}; border: none;")
        h = QLabel("Drop files or folders here, or click to browse")
        h.setAlignment(Qt.AlignmentFlag.AlignCenter)
        h.setStyleSheet(f"color: {theme.P.text_muted}; border: none; font-size: {theme.fs(11)}px;")
        for w in (a, h):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)
            lay.addWidget(w)

    def _style(self, hover):
        p = theme.P
        self.setStyleSheet(f"QFrame {{ background: {p.primary_muted if hover else p.bg_card};"
                           f" border: 2px dashed {p.primary if hover else p.border}; border-radius: {p.radius}px; }}")

    def mousePressEvent(self, e):
        paths, _ = QFileDialog.getOpenFileNames(self, "Add files to the vault")
        if paths:
            self.files_dropped.emit(paths)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()
            self._style(True)

    def dragLeaveEvent(self, e):
        self._style(False)

    def dropEvent(self, e):
        self._style(False)
        paths = [u.toLocalFile() for u in e.mimeData().urls() if u.isLocalFile()]
        if paths:
            self.files_dropped.emit(paths)


class FileCard(QFrame):
    left_clicked = pyqtSignal(str)
    right_clicked = pyqtSignal(str, QPoint)
    double_clicked = pyqtSignal(str)
    dropped_onto = pyqtSignal(str, str)   # (dragged name, this folder name)

    def __init__(self, name: str, is_dir: bool, unlocked_copy: bool = False, is_up: bool = False, where: str = ""):
        super().__init__()
        self.name = name
        self.is_dir = is_dir
        self.is_up = is_up
        self._selected = False
        self._press = None
        self.setFixedHeight(52)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setAcceptDrops(is_dir)
        self._style(False)
        lay = QHBoxLayout(self)
        lay.setContentsMargins(10, 0, 8, 0)
        lay.setSpacing(8)
        self.icon = QLabel("⬆" if is_up else ("📁" if is_dir else ICONS.get(Path(name).suffix.lower(), "📄")))
        self.icon.setStyleSheet("border: none; font-size: 18px;")
        self.icon.setFixedSize(THUMB_PX, THUMB_PX)
        self.icon.setAlignment(Qt.AlignmentFlag.AlignCenter)
        label = QLabel(".. (up)" if is_up else name)
        label.setFont(QFont(theme.P.font_family, 10))
        label.setStyleSheet("border: none;")
        label.setToolTip(f"{where}/{name}" if where else name)
        lay.addWidget(self.icon)
        if where:
            col = QVBoxLayout()
            col.setSpacing(0)
            col.addWidget(label)
            w = QLabel(f"in {where}")
            w.setStyleSheet(f"border: none; color: {theme.P.text_muted}; font-size: {theme.fs(10)}px;")
            col.addWidget(w)
            lay.addLayout(col, 1)
        else:
            lay.addWidget(label, 1)
        if not is_dir:
            lock = QLabel("🔓" if unlocked_copy else "🔒")
            lock.setToolTip("A decrypted copy exists on disk" if unlocked_copy else "Encrypted")
            lock.setStyleSheet("border: none; font-size: 12px;")
            lay.addWidget(lock)
        for w in self.findChildren(QLabel):
            w.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)

    def set_thumb(self, img: QImage):
        px = QPixmap.fromImage(img).scaled(THUMB_PX, THUMB_PX, Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                                           Qt.TransformationMode.SmoothTransformation)
        x, y = (px.width() - THUMB_PX) // 2, (px.height() - THUMB_PX) // 2
        self.icon.setPixmap(px.copy(x, y, THUMB_PX, THUMB_PX))

    def _style(self, sel: bool, hover_drop: bool = False):
        p = theme.P
        bg = p.primary_muted if (sel or hover_drop) else p.bg_card
        border = p.primary if (sel or hover_drop) else p.border
        self.setStyleSheet(f"""
            FileCard {{ background: {bg}; border: 1px solid {border}; border-radius: {p.radius}px; }}
            FileCard:hover {{ background: {p.bg_card_hover}; border-color: {p.primary}; }}
        """)

    def set_selected(self, on: bool):
        self._selected = on
        self._style(on)

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self._press = e.position().toPoint()
            self.left_clicked.emit(self.name)
        elif e.button() == Qt.MouseButton.RightButton:
            self.right_clicked.emit(self.name, e.globalPosition().toPoint())

    def mouseMoveEvent(self, e):
        if self.is_up or self._press is None or not (e.buttons() & Qt.MouseButton.LeftButton):
            return
        if (e.position().toPoint() - self._press).manhattanLength() < 12:
            return
        self._press = None
        drag = QDrag(self)
        mime = QMimeData()
        mime.setText(f"vault_item:{self.name}")
        drag.setMimeData(mime)
        drag.exec(Qt.DropAction.MoveAction)

    def mouseDoubleClickEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            self.double_clicked.emit(self.name)

    def dragEnterEvent(self, e):
        t = e.mimeData().text() if e.mimeData().hasText() else ""
        if self.is_dir and t.startswith("vault_item:") and t[11:] != self.name:
            e.acceptProposedAction()
            self._style(self._selected, hover_drop=True)

    def dragLeaveEvent(self, e):
        self._style(self._selected)

    def dropEvent(self, e):
        self._style(self._selected)
        t = e.mimeData().text()
        if t.startswith("vault_item:"):
            self.dropped_onto.emit(t[11:], self.name)
            e.acceptProposedAction()


# ── window ────────────────────────────────────────────────────────────────────
class VaultWindow(widgets.FloatingPanel):
    def __init__(self, m: "VaultModule", store: VaultStore):
        self.m = m
        self.store = store
        title = "🔒 Ghost Vault" + ("  (decoy)" if store.mode == "decoy" else "")
        super().__init__(title, min_size=(700, 480), opacity=1.0,
                         close_hides=False, drag_needs_edit_mode=False)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.path: list[str] = []
        self.selected: set[str] = set()
        self.cards: dict[str, FileCard] = {}
        self._worker: Worker | None = None
        self._thumbs: ThumbLoader | None = None
        self._thumb_cache: dict[str, QImage] = {}        # blob -> thumbnail, memory only, gone when the vault locks
        self._closing = False
        self._close_pending = False

        self.resize(920, 620)
        self.center_on(widgets.cursor_screen_geometry())
        self._dim = widgets.DimOverlay(0.35, click_through=False, on_click=self.lock_and_close)
        self.escape_pressed.connect(self.lock_and_close)

        # header extras
        self.lock_label = QLabel("")
        self.lock_label.setStyleSheet(f"color: {theme.P.text_muted}; font-size: {theme.fs(10)}px; border: none;")
        self.header_right.addWidget(self.lock_label)
        gear = widgets.icon_button("⚙", "Vault options", 24, 13, ghost=True)
        gear.clicked.connect(self._options_menu)
        self.header_right.addWidget(gear)

        # breadcrumb
        self.bc = QFrame()
        self.bc.setFixedHeight(34)
        widgets.style(self.bc, f"background: {theme.P.bg_frame}; border: none; border-bottom: 1px solid {theme.P.border};")
        bc_row = QHBoxLayout(self.bc)
        bc_row.setContentsMargins(12, 0, 8, 0)
        self.bc_layout = QHBoxLayout()
        self.bc_layout.setSpacing(4)
        bc_row.addLayout(self.bc_layout, 1)
        self.search = QLineEdit()
        self.search.setPlaceholderText("Search the vault")
        self.search.setClearButtonEnabled(True)
        self.search.setFixedSize(220, 26)
        self.search.textChanged.connect(lambda _t: (self.touch(), self.reload()))
        bc_row.addWidget(self.search)
        self.body.addWidget(self.bc)

        # grid + drop zone
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        host = QWidget()
        widgets.style(host, "background: transparent;")
        hl = QVBoxLayout(host)
        hl.setContentsMargins(0, 0, 0, 0)
        hl.setSpacing(8)
        self.grid_widget = QWidget()
        widgets.style(self.grid_widget, "background: transparent;")
        self.grid = QGridLayout(self.grid_widget)
        self.grid.setContentsMargins(12, 12, 12, 4)
        self.grid.setSpacing(8)
        self.grid.setAlignment(Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        for i in range(4):
            self.grid.setColumnStretch(i, 1)
        hl.addWidget(self.grid_widget)
        self.drop = DropZone()
        self.drop.files_dropped.connect(self._add_files)
        dz = QHBoxLayout()
        dz.setContentsMargins(12, 0, 12, 8)
        dz.addWidget(self.drop)
        hl.addLayout(dz)
        hl.addStretch()
        scroll.setWidget(host)
        self.body.addWidget(scroll, 1)

        # action bar
        bar = QFrame()
        bar.setFixedHeight(52)
        widgets.style(bar, f"background: {theme.P.bg_header}; border: none; border-top: 1px solid {theme.P.border};"
                           f" border-bottom-left-radius: 14px; border-bottom-right-radius: 14px;")
        bl = QHBoxLayout(bar)
        bl.setContentsMargins(14, 0, 14, 0)
        bl.setSpacing(8)
        nf = widgets.button("📁 New folder")
        nf.clicked.connect(self._new_folder)
        bl.addWidget(nf)
        self.sel_label = QLabel("")
        self.sel_label.setStyleSheet(f"color: {theme.P.text_muted}; border: none;")
        bl.addWidget(self.sel_label)
        bl.addStretch()
        self.open_btn = widgets.button("▶ Open")
        self.open_btn.clicked.connect(self._open_selected)
        self.temp_btn = widgets.button("⏱ Decrypt (temp)", tooltip="Decrypted copy is wiped when the vault locks")
        self.temp_btn.clicked.connect(lambda: self._decrypt_selected(True))
        self.keep_btn = widgets.button("💾 Decrypt (keep)", "primary", tooltip="Decrypted copy stays on disk until you re-encrypt it")
        self.keep_btn.clicked.connect(lambda: self._decrypt_selected(False))
        self.re_btn = widgets.button("🔒 Re-encrypt", tooltip="Pull the kept copy back in and wipe it from disk")
        self.re_btn.clicked.connect(self._reencrypt_selected)
        self.del_btn = widgets.button("🗑 Delete")
        self.del_btn.setProperty("variant", "danger")
        self.del_btn.clicked.connect(self._delete_selected)
        for b in (self.open_btn, self.temp_btn, self.keep_btn, self.re_btn, self.del_btn):
            bl.addWidget(b)
        self.body.addWidget(bar)

        # progress
        self.prog = QFrame()
        widgets.style(self.prog, "background: transparent; border: none;")
        pl = QHBoxLayout(self.prog)
        pl.setContentsMargins(14, 4, 14, 6)
        self.prog_label = QLabel("")
        self.prog_label.setStyleSheet(f"color: {theme.P.text_secondary}; font-size: {theme.fs(10)}px; border: none;")
        self.prog_bar = QProgressBar()
        self.prog_bar.setFixedHeight(5)
        pl.addWidget(self.prog_label)
        pl.addWidget(self.prog_bar, 1)
        self.prog.hide()
        self.body.addWidget(self.prog)

        # auto-lock
        self._lock_timer = QTimer(self)
        self._lock_timer.setSingleShot(True)
        self._lock_timer.timeout.connect(self.lock_and_close)
        self._countdown = QTimer(self)
        self._countdown.setInterval(15_000)
        self._countdown.timeout.connect(self._update_lock_label)
        self._countdown.start()
        self.touch()
        self.reload()

    # ── helpers ────────────────────────────────────────────────────────────
    def touch(self):
        self._lock_timer.start(int(self.m.s("lock_minutes", 5)) * 60_000)
        self._update_lock_label()

    def _update_lock_label(self):
        mins = max(0, self._lock_timer.remainingTime() // 60_000 + 1)
        self.lock_label.setText(f"🔐 locks in {mins} min")

    def mousePressEvent(self, e):
        self.touch()
        super().mousePressEvent(e)

    def reload(self):
        self._stop_thumbs()
        widgets.clear_layout(self.grid)
        self.cards = {}
        self.selected = set()
        self._breadcrumb()
        node = self.store.node(self.path)
        row = col = 0

        def place(card):
            nonlocal row, col
            self.grid.addWidget(card, row, col)
            col += 1
            if col >= 4:
                col, row = 0, row + 1

        query = self.search.text().strip()
        if query:
            found = self.store.find(query)
            for path, name, is_dir in found:
                card = FileCard(name, is_dir, where="/".join(path) or "the top")
                card.left_clicked.connect(lambda _n, p=path, n=name: self._reveal(p, n))
                card.double_clicked.connect(lambda _n, p=path, n=name, d=is_dir: self._reveal(p, n, open_it=True))
                place(card)
            if not found:
                e = QLabel(f'Nothing in the vault is called anything like "{query}".')
                e.setStyleSheet(f"color: {theme.P.text_muted}; border: none;")
                self.grid.addWidget(e, 0, 0, 1, 4)
            self.drop.hide()
            self._update_buttons()
            return
        self.drop.show()

        if self.path:
            up = FileCard("..", True, is_up=True)
            up.left_clicked.connect(lambda _: self._go_up())
            up.dropped_onto.connect(lambda src, _: self._move(src, None))
            place(up)
        for name, val in sorted(node.items(), key=lambda kv: (not isinstance(kv[1], dict), kv[0].lower())):
            is_dir = isinstance(val, dict)
            card = FileCard(name, is_dir, unlocked_copy=self.store.perm_copy(self.path, name) is not None)
            card.left_clicked.connect(self._left)
            card.right_clicked.connect(self._menu)
            card.double_clicked.connect(self._double)
            card.dropped_onto.connect(self._move)
            self.cards[name] = card
            place(card)
        if not node:
            e = QLabel("Empty. Drop files below to encrypt them.")
            e.setStyleSheet(f"color: {theme.P.text_muted}; border: none;")
            self.grid.addWidget(e, 0, 0, 1, 4)
        self._update_buttons()
        self._load_thumbs(node)

    # ── search and thumbnails ──────────────────────────────────────────────
    def _reveal(self, path: list[str], name: str, open_it: bool = False):
        """A search result was clicked: go to its folder with it selected (double-click: open it)."""
        self.search.blockSignals(True)
        self.search.clear()
        self.search.blockSignals(False)
        self._jump(path)
        if name in self.cards:
            if open_it:
                self._double(name)
            else:
                self.selected = {name}
                self.cards[name].set_selected(True)
                self._update_buttons()

    def _load_thumbs(self, node: dict):
        if not self.m.s("thumbnails", True):
            return
        todo = []
        for name, val in node.items():
            if isinstance(val, str) and Path(name).suffix.lower() in THUMB_EXT and name in self.cards:
                if val in self._thumb_cache:
                    self.cards[name].set_thumb(self._thumb_cache[val])
                else:
                    todo.append(name)
        if todo:
            self._thumbs = ThumbLoader(self.store, self.path, todo)
            self._thumbs.ready.connect(self._thumb_ready)
            self._thumbs.start()

    def _thumb_ready(self, name: str, img: QImage):
        try:
            blob = self.store.node(self.path).get(name)
        except Exception:
            return
        if isinstance(blob, str):
            self._thumb_cache[blob] = img
        card = self.cards.get(name)
        if card is not None:
            card.set_thumb(img)

    def _stop_thumbs(self):
        if self._thumbs is not None:
            self._thumbs.stop()
            self._thumbs.wait(2000)
            self._thumbs = None

    def _breadcrumb(self):
        widgets.clear_layout(self.bc_layout)

        def crumb(text, path, last=False):
            b = QPushButton(text)
            b.setProperty("variant", "ghost")
            b.setStyleSheet(f"QPushButton {{ color: {theme.P.text_primary if last else theme.P.primary}; font-size: {theme.fs(11)}px; }}")
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.clicked.connect(lambda: self._jump(path))
            return b

        self.bc_layout.addWidget(crumb("Vault", [], last=not self.path))
        for i, part in enumerate(self.path):
            sep = QLabel("›")
            sep.setStyleSheet(f"color: {theme.P.text_muted}; border: none;")
            self.bc_layout.addWidget(sep)
            self.bc_layout.addWidget(crumb(part, self.path[:i + 1], last=i == len(self.path) - 1))
        self.bc_layout.addStretch()

    def _update_buttons(self):
        node = self.store.node(self.path)
        files = [n for n in self.selected if not isinstance(node.get(n), dict)]
        has_sel = bool(self.selected)
        has_kept = any(self.store.perm_copy(self.path, n) for n in files)
        self.open_btn.setVisible(any(Path(n).suffix.lower() in OPENABLE for n in files))
        self.temp_btn.setVisible(has_sel)
        self.keep_btn.setVisible(has_sel)
        self.re_btn.setVisible(has_kept)
        self.del_btn.setVisible(has_sel)
        self.sel_label.setText(f"{len(self.selected)} selected" if has_sel else "")

    # ── navigation / selection ─────────────────────────────────────────────
    def _jump(self, path):
        self.touch()
        self.path = list(path)
        self.reload()

    def _go_up(self):
        if self.path:
            self._jump(self.path[:-1])

    def _left(self, name):
        self.touch()
        node = self.store.node(self.path)
        if isinstance(node.get(name), dict) and name not in self.selected:
            mods = QApplication.keyboardModifiers()
            if not (mods & Qt.KeyboardModifier.ControlModifier):
                self._jump(self.path + [name])
                return
        if name in self.selected:
            self.selected.discard(name)
            self.cards[name].set_selected(False)
        else:
            self.selected.add(name)
            self.cards[name].set_selected(True)
        self._update_buttons()

    def _double(self, name):
        self.touch()
        node = self.store.node(self.path)
        if isinstance(node.get(name), dict):
            self._jump(self.path + [name])
        elif Path(name).suffix.lower() in OPENABLE:
            self._open_file(name)

    def _menu(self, name, pos):
        self.touch()
        node = self.store.node(self.path)
        is_dir = isinstance(node.get(name), dict)
        m = widgets.menu(self)
        m.addAction("☑ Select / deselect", lambda: self._toggle_select(name))
        m.addAction("✎ Rename…", lambda: self._rename(name))
        m.addSeparator()
        if is_dir:
            m.addAction("⏱ Decrypt folder (temp)", lambda: self._decrypt_folder(name, True))
            m.addAction("💾 Decrypt folder (keep)", lambda: self._decrypt_folder(name, False))
            m.addSeparator()
            m.addAction("📤 Unpack files here, remove folder", lambda: self._unpack(name))
            m.addAction("🗑 Delete folder and contents…", lambda: self._delete([name]))
        else:
            if Path(name).suffix.lower() in OPENABLE:
                m.addAction("▶ Open", lambda: self._open_file(name))
            m.addAction("⏱ Decrypt (temp)", lambda: self._decrypt([name], True))
            m.addAction("💾 Decrypt (keep)", lambda: self._decrypt([name], False))
            if self.store.perm_copy(self.path, name):
                m.addAction("🔒 Re-encrypt kept copy", lambda: self._reencrypt([name]))
            m.addSeparator()
            m.addAction("🗑 Delete…", lambda: self._delete([name]))
        m.exec(pos)

    def _toggle_select(self, name):
        if name in self.selected:
            self.selected.discard(name)
            self.cards[name].set_selected(False)
        else:
            self.selected.add(name)
            self.cards[name].set_selected(True)
        self._update_buttons()

    # ── operations ─────────────────────────────────────────────────────────
    def _busy(self, tell: bool = False) -> bool:
        busy = self._worker is not None and self._worker.isRunning()
        if busy and tell:
            widgets.message(self, "Busy", "Please wait for the current operation to finish.")
        return busy

    def _run(self, fn, label: str, then=None):
        if self._busy(tell=True):
            return
        self.prog.show()
        self.prog_label.setText(label)
        self.prog_bar.setValue(0)

        def done(ok, err):
            worker.wait()                  # `done` is emitted from inside run(): let the thread fully end first
            self.prog.hide()
            if not ok:
                self.m.log.error("vault operation failed: %s", err)
                widgets.error(self, "Vault", f"Operation failed:\n{err}")
            elif then and not self._close_pending:
                then()
            if self._close_pending:
                self.lock_and_close()
                return
            self.reload()

        worker = self._worker = Worker(fn)
        worker.progress.connect(self.prog_bar.setValue)
        worker.done.connect(done)
        worker.start()

    def _add_files(self, paths: list):
        self.touch()
        choice = widgets.choose(self, "Add to vault",
                                f"Add {len(paths)} item(s) to the vault.\n\nMove = originals are securely deleted.\n"
                                f"Copy = originals are left alone.", ["Move", "Copy"])
        if not choice:
            return
        move = choice == "Move"
        snapshot = list(self.path)
        self._run(lambda prog: self.store.add_files(paths, snapshot, move, prog), "Encrypting…")

    def _decrypt_selected(self, temp: bool):
        node = self.store.node(self.path)
        files = [n for n in self.selected if not isinstance(node.get(n), dict)]
        folders = [n for n in self.selected if isinstance(node.get(n), dict)]
        if files:
            self._decrypt(files, temp)
        for f in folders:
            self._decrypt_folder(f, temp)

    def _decrypt(self, names: list, temp: bool):
        self.touch()
        snapshot = list(self.path)

        def task(prog):
            for i, n in enumerate(names):
                self.store.decrypt_file(snapshot, n, temp)
                prog(int((i + 1) / len(names) * 100))

        where = self.store.out
        self._run(task, "Decrypting…", then=lambda: self._after_decrypt(where, temp))

    def _decrypt_folder(self, name: str, temp: bool):
        self.touch()
        snapshot = list(self.path)
        self._run(lambda prog: self.store.decrypt_folder(snapshot, name, temp, prog), f"Decrypting {name}…",
                  then=lambda: self._after_decrypt(self.store.out / name, temp))

    def _after_decrypt(self, where: Path, temp: bool):
        try:
            runner.open_path(str(where))
        except Exception:
            pass
        if temp:
            self.prog_label.setText("Temporary copies are wiped when the vault locks.")

    def _open_file(self, name: str):
        self.touch()
        try:
            p = self.store.decrypt_file(self.path, name, temp=True)
            err = runner.open_path(str(p))
            if err:
                raise OSError(err)
        except Exception as e:
            widgets.error(self, "Vault", f"Could not open {name}:\n{e}")
        self.reload()

    def _open_selected(self):
        node = self.store.node(self.path)
        for n in list(self.selected):
            if not isinstance(node.get(n), dict) and Path(n).suffix.lower() in OPENABLE:
                self._open_file(n)

    def _reencrypt_selected(self):
        self._reencrypt(list(self.selected))

    def _reencrypt(self, names: list):
        self.touch()
        snapshot = list(self.path)

        def task(prog):
            for i, n in enumerate(names):
                self.store.reencrypt(snapshot, n)
                prog(int((i + 1) / len(names) * 100))

        self._run(task, "Re-encrypting…")

    def _delete_selected(self):
        self._delete(list(self.selected))

    def _delete(self, names: list):
        self.touch()
        if not widgets.confirm(self, "Delete", f"Securely delete {len(names)} item(s) from the vault?\n"
                                               f"Kept decrypted copies are wiped too. This cannot be undone.",
                               yes="Delete", danger=True):
            return
        snapshot = list(self.path)
        self._run(lambda prog: self.store.delete(snapshot, names, prog), "Deleting…")

    def _unpack(self, name: str):
        self.touch()
        if self._busy(tell=True):
            return
        self.store.unpack_folder(self.path, name)
        self.reload()

    def _new_folder(self):
        self.touch()
        if self._busy(tell=True):
            return
        name, ok = widgets.ask_text(self, "New folder", "Folder name:")
        if ok and name.strip():
            self.store.new_folder(self.path, name.strip())
            self.reload()

    def _rename(self, name: str):
        if self._busy(tell=True):
            return
        new, ok = widgets.ask_text(self, "Rename", "New name:", name)
        if ok and new.strip():
            self.store.rename(self.path, name, new.strip())
            self.reload()

    def _move(self, src: str, dst_folder: str | None):
        """dst_folder None = up one level."""
        self.touch()
        if self._busy(tell=True):
            return
        try:
            if dst_folder is None:
                self.store.move(self.path, src, self.path[:-1])
            else:
                self.store.move(self.path, src, self.path + [dst_folder])
        except VaultError as e:
            widgets.error(self, "Vault", str(e))
        self.reload()

    # ── options ────────────────────────────────────────────────────────────
    def _options_menu(self):
        self.touch()
        m = widgets.menu(self)
        if self.store.mode == "real":
            m.addAction("Change password…", self._change_password)
            m.addAction("Set / change decoy password…", self._change_decoy)
            if self.store.has_decoy():
                m.addAction("Remove decoy password", self._remove_decoy)
            m.addAction("Generate a new recovery key…", self._new_recovery)
            m.addSeparator()
        m.addAction("Open decrypted-files folder", lambda: runner.open_path(str(self.store.out)))
        m.addAction("Vault settings…", lambda: self.m.ctx.open_settings("vault"))
        m.addSeparator()
        m.addAction("🔒 Lock now", self.lock_and_close)
        m.exec(self.mapToGlobal(QPoint(self.width() - 200, 50)))

    def _change_password(self):
        dlg = PasswordDialog("Change vault password", "New password:", confirm=True, parent=self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            self.store.set_password(dlg.password())
            widgets.message(self, "Vault", "Password changed.")

    def _change_decoy(self):
        dlg = PasswordDialog("Decoy password", "Decoy password (opens an empty-looking vault):", confirm=True, parent=self)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            if self.store.has_decoy() and not widgets.confirm(
                    self, "Decoy", "Replacing the decoy password discards the decoy vault's current contents. Continue?"):
                return
            self.store.set_decoy_password(dlg.password())
            widgets.message(self, "Vault", "Decoy password set.")

    def _remove_decoy(self):
        if widgets.confirm(self, "Decoy", "Remove the decoy password and its contents?", yes="Remove", danger=True):
            self.store.set_decoy_password(None)

    def _new_recovery(self):
        if not widgets.confirm(self, "Recovery key", "The old recovery key will stop working. Continue?"):
            return
        key = self.store.regenerate_recovery_key()
        RecoveryKeyDialog(key, parent=self).exec()

    # ── closing ────────────────────────────────────────────────────────────
    def lock_and_close(self):
        if self._busy():
            # Locking wipes the key; a worker still encrypting would then fail half-way, after it may already
            # have securely deleted originals. Lock as soon as it finishes instead.
            self._close_pending = True
            self.prog_label.setText("Locking as soon as this finishes…")
            return
        if self._closing:
            return
        self._closing = True
        self.close()

    def wait_and_close(self):
        """Used when the launcher quits: let a running operation finish, then lock."""
        if self._worker:
            self._worker.wait()
        self.close()

    def closeEvent(self, e):
        if self._busy():
            e.ignore()
            self.lock_and_close()
            return
        self._closing = True
        self._lock_timer.stop()
        self._countdown.stop()
        self._dim.close()
        self._stop_thumbs()                       # it reads with the key that lock() wipes
        self._thumb_cache.clear()
        self.store.lock()
        self.m.window_closed(self)
        super().closeEvent(e)


# ── module ────────────────────────────────────────────────────────────────────
class VaultModule(Module):
    key = "vault"
    title = "Ghost Vault"
    icon = "🔒"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.window: VaultWindow | None = None
        self._opening = False

    def start(self):
        self.hotkey("hotkey", self.open, "open vault")

    def stop(self):
        if self.window:
            self.window.wait_and_close()

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.start()

    def tray_actions(self):
        return [("Open Ghost Vault", self.open)]

    def window_closed(self, win):
        if self.window is win:
            self.window = None

    def open(self):
        if self._opening:
            return
        if self.window is not None:
            self.window.show_and_raise()
            return
        self._opening = True
        try:
            store = VaultStore(self.s("storage_path", ""), self.s("decrypt_output", ""), int(self.s("secure_delete_passes", 3)))
            err = store.check_writable()
            if err:
                widgets.error(None, "Ghost Vault",
                              f"The vault folder cannot be written to:\n{store.folder}\n\n{err}\n\n"
                              f"Pick a folder that does not need admin rights in Settings → Ghost Vault.")
                return
            if not store.exists():
                if not self._first_run(store):
                    return
            elif not self._unlock(store):
                return
            with self.themed():
                self.window = VaultWindow(self, store)
            self.window.show_and_raise()
        finally:
            self._opening = False

    def _first_run(self, store: VaultStore) -> bool:
        dlg = PasswordDialog("Create vault password", "Choose a strong password:", confirm=True)
        if dlg.exec() != QDialog.DialogCode.Accepted:
            return False
        pw = dlg.password()
        decoy = None
        d2 = PasswordDialog("Decoy password (optional)",
                            "Optional: a second password that opens a separate, innocent-looking vault.\n"
                            "Leave empty to skip.", confirm=True, allow_empty=True)
        if d2.exec() == QDialog.DialogCode.Accepted and d2.password():
            decoy = d2.password()
        rec = store.create(pw, decoy)
        self._show_recovery(store, rec)
        return True

    def _show_recovery(self, store: VaultStore, rec: str):
        saved = ""
        try:
            store.out.mkdir(parents=True, exist_ok=True)
            f = store.out / "vault_recovery.key"
            f.write_text(f"GHOST VAULT RECOVERY KEY\n{rec}\n", encoding="utf-8")
            saved = str(f)
        except Exception:
            pass
        RecoveryKeyDialog(rec, saved).exec()

    def _unlock(self, store: VaultStore) -> bool:
        for _ in range(5):
            dlg = PasswordDialog("Unlock Ghost Vault", "Password:", forgot=store.recovery_file.exists() or store.is_old_format())
            if dlg.exec() != QDialog.DialogCode.Accepted:
                if dlg.forgot_clicked:
                    return self._recover(store)
                return False
            QApplication.setOverrideCursor(Qt.CursorShape.WaitCursor)
            try:
                mode = store.unlock(dlg.password())
            finally:
                QApplication.restoreOverrideCursor()
            if mode:
                if store.pending_recovery_key:
                    widgets.message(None, "Vault upgraded",
                                    "Your vault was upgraded to the new format. A recovery key has been created; "
                                    "keep it somewhere safe.")
                    self._show_recovery(store, store.pending_recovery_key)
                    store.pending_recovery_key = None
                return True
            widgets.error(None, "Ghost Vault", "Incorrect password.")
        return False

    def _recover(self, store: VaultStore) -> bool:
        if not store.recovery_file.exists():
            widgets.error(None, "Ghost Vault", "This vault has no recovery key file (.recovery). "
                                               "It was created before recovery keys worked; the password is the only way in.")
            return False
        key, ok = widgets.ask_text(None, "Recovery", "Enter your recovery key (XXXX-XXXX-…):")
        if not ok or not key.strip():
            return False
        if not store.unlock_with_recovery(key):
            widgets.error(None, "Ghost Vault", "That recovery key is not valid for this vault.")
            return False
        dlg = PasswordDialog("Set a new password", "New password:", confirm=True)
        if dlg.exec() == QDialog.DialogCode.Accepted:
            store.set_password(dlg.password())
            widgets.message(None, "Ghost Vault", "Password reset. The vault is unlocked.")
        return True
