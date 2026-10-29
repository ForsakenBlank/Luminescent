"""
Mini media controls: what is playing (title, artist, cover, progress) with previous / play-pause / next.

Talks to every MPRIS player (Spotify, Firefox, mpv, VLC...) through playerctl
(sudo pacman -S playerctl). On Windows it uses the media sessions Windows knows about (modules/media_win.py). While the panel is open it asks playerctl once a second, without
blocking. Click the player's name to switch between players when several are open. Optional hotkeys
for play/pause, next and previous work with the panel closed too.
"""
import os
import shutil
import logging
import threading
from urllib.parse import urlparse, unquote

from PyQt6.QtWidgets import QHBoxLayout, QVBoxLayout, QLabel, QSizePolicy
from PyQt6.QtCore import Qt, QTimer, QProcess, pyqtSignal
from PyQt6.QtGui import QPixmap

from core import DATA_DIR, IS_WIN, theme, widgets, save_json, load_json
from core.module import Module

log = logging.getLogger("media")
SAVE_PATH = os.path.join(DATA_DIR, "media.json")
SEP = "␟"            # a character no title uses, between the fields playerctl prints
FIELDS = ("playerName", "status", "title", "artist", "album", "mpris:artUrl", "position", "mpris:length")


def _clock(us: int) -> str:
    s = max(0, int(us // 1_000_000))
    return f"{s // 3600}:{s % 3600 // 60:02d}:{s % 60:02d}" if s >= 3600 else f"{s // 60}:{s % 60:02d}"


def parse(out: str) -> list[dict]:
    """playerctl's lines -> [{"player", "status", "title", "artist", "album", "art", "pos", "len"}]."""
    players = []
    for line in out.splitlines():
        parts = line.split(SEP)
        if len(parts) != len(FIELDS):
            continue
        d = dict(zip(("player", "status", "title", "artist", "album", "art", "pos", "len"), parts))
        for k in ("pos", "len"):
            try:
                d[k] = int(float(d[k] or 0))
            except ValueError:
                d[k] = 0
        players.append(d)
    return players


class MediaPanel(widgets.FloatingPanel):
    def __init__(self, m: "MediaModule"):
        self.m = m
        super().__init__("Media", min_size=(300, 150), opacity=float(m.s("bg_opacity", 0.92)),
                         drag_needs_edit_mode=False, close_hides=True)
        self._art_url = None
        body = QHBoxLayout()
        body.setContentsMargins(14, 12, 14, 14)
        body.setSpacing(14)
        self.cover = QLabel()
        self.cover.setFixedSize(84, 84)
        self.cover.setAlignment(Qt.AlignmentFlag.AlignCenter)
        p = theme.P
        self.cover.setStyleSheet(f"background: {p.bg_card}; border: 1px solid {p.border}; border-radius: 8px;"
                                 f" color: {p.text_muted}; font-size: 28px;")
        body.addWidget(self.cover)
        col = QVBoxLayout()
        col.setSpacing(4)
        self.player = widgets.label("", 9.5, 600, mono=True, spacing=0.12, tone="accent")
        self.player.setCursor(Qt.CursorShape.PointingHandCursor)
        self.player.setToolTip("Click to switch to the next player")
        self.player.mousePressEvent = lambda e: m.next_player()
        col.addWidget(self.player)
        self.title = widgets.label("Nothing playing", 14, 600)
        self.title.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        col.addWidget(self.title)
        self.artist = widgets.label("", 12, tone="secondary")
        self.artist.setSizePolicy(QSizePolicy.Policy.Ignored, QSizePolicy.Policy.Preferred)
        col.addWidget(self.artist)
        self.bar = widgets.label("", 10, mono=True, tone="muted")
        col.addWidget(self.bar)
        self.track = widgets.rule()
        self.track.setFixedHeight(2)
        self.fill = QLabel(self.track)
        self.fill.setStyleSheet(f"background: {p.primary};")
        col.addWidget(self.track)
        btns = QHBoxLayout()
        btns.setSpacing(6)
        for text, tip, fn in (("⏮", "Previous", m.previous), ("⏯", "Play / pause", m.play_pause),
                              ("⏭", "Next", m.next)):
            b = widgets.icon_button(text, tip, 34, 15)
            b.clicked.connect(fn)
            btns.addWidget(b)
        btns.addStretch()
        col.addLayout(btns)
        body.addLayout(col, 1)
        self.body.addLayout(body)
        st = m.geometry_saved()
        self.resize(int(st.get("w", 380)), int(st.get("h", 170)))
        if "x" in st and "y" in st:
            self.move(int(st["x"]), int(st["y"]))
            self.clamp_to_screens()
        else:
            self.snap_to("br", 16)
        self.geometry_changed.connect(lambda: m.remember(self.geometry()))
        self.escape_pressed.connect(self.hide)

    def showEvent(self, e):
        super().showEvent(e)
        if getattr(self, "_embedded", False):
            QTimer.singleShot(0, self.m._wake)        # a page coming back on screen

    def show_state(self, d: dict | None, count: int):
        if d is None:
            ok = self.m.has_playerctl()
            self.player.setText("NO PLAYER" if ok else ("WINRT PACKAGES MISSING" if IS_WIN else "PLAYERCTL IS MISSING"))
            self.title.setText("Nothing playing" if ok else
                               ("pip install winrt-Windows.Media.Control" if IS_WIN else "sudo pacman -S playerctl"))
            self.artist.setText("")
            self.bar.setText("")
            self.fill.setFixedWidth(0)
            self._set_art("")
            return
        more = f"  ·  {count} PLAYERS" if count > 1 else ""
        self.player.setText(f"{d['player'].upper()}  ·  {d['status'].upper()}{more}")
        self.title.setText(d["title"] or "(no title)")
        self.artist.setText(" · ".join(x for x in (d["artist"], d["album"]) if x))
        if d["len"] > 0:
            self.bar.setText(f"{_clock(d['pos'])} / {_clock(d['len'])}")
            self.fill.setFixedHeight(2)
            self.fill.setFixedWidth(int(self.track.width() * min(1.0, d["pos"] / d["len"])))
        else:
            self.bar.setText(_clock(d["pos"]) if d["pos"] else "")
            self.fill.setFixedWidth(0)
        self._set_art(d["art"])

    def _set_art(self, url: str):
        if url == self._art_url:
            return
        self._art_url = url
        path = unquote(urlparse(url).path) if url.startswith("file://") else ""
        px = QPixmap(path) if path and os.path.isfile(path) else QPixmap()
        if px.isNull():
            self.cover.setPixmap(QPixmap())
            self.cover.setText("♪")
        else:
            self.cover.setText("")
            self.cover.setPixmap(px.scaled(84, 84, Qt.AspectRatioMode.KeepAspectRatioByExpanding,
                                           Qt.TransformationMode.SmoothTransformation).copy(0, 0, 84, 84))


class MediaModule(Module):
    key = "media"
    title = "Media Controls"
    icon = "♪"
    _win_ready = pyqtSignal(list)            # Windows: the sessions, from the worker thread

    def __init__(self, ctx):
        super().__init__(ctx)
        self._win_ready.connect(self._show_players)
        self._asking = False
        self.panel: MediaPanel | None = None
        self.views: list = []                     # Media pages (overlay / dashboard boxes)
        self.players: list[dict] = []
        self._chosen = ""
        self._proc: QProcess | None = None
        self._poll = QTimer(self)
        self._poll.setInterval(1000)
        self._poll.timeout.connect(self.refresh)

    def start(self):
        self.hotkey("hotkey", self.toggle, "media controls")
        self.hotkey("play_hotkey", self.play_pause, "play / pause")
        self.hotkey("next_hotkey", self.next, "next track")
        self.hotkey("previous_hotkey", self.previous, "previous track")
        if self.ctx.shows_on_start("media") and self.panel is None:
            QTimer.singleShot(1500, self.toggle)

    def stop(self):
        self._poll.stop()
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.start()
        if self.panel:
            self.panel.set_opacity(float(self.s("bg_opacity", 0.92)))

    def retheme(self):
        if self.panel:
            visible = self.panel.isVisible()
            self.stop()
            if visible:
                self.toggle()

    def actions(self):
        return [("toggle", "Media controls", self.toggle), ("play_pause", "Media: play / pause", self.play_pause),
                ("next", "Media: next track", self.next), ("previous", "Media: previous track", self.previous)]

    def tray_actions(self):
        return [("Media controls", self.toggle)]

    # ── panel ──────────────────────────────────────────────────────────────
    def make_page(self):
        view = MediaPanel(self)
        view.hidden.connect(self._maybe_stop)
        self.views.append(view)
        QTimer.singleShot(0, self._wake)
        return widgets.embed(view)

    def _shown(self) -> list:
        from core.guard import alive
        self.views = [v for v in self.views if alive(v)]
        return [v for v in ([self.panel] if self.panel else []) + self.views if alive(v) and v.isVisible()]

    def _wake(self):
        if not self._poll.isActive():
            self.refresh()
            self._poll.start()

    def _maybe_stop(self):
        QTimer.singleShot(0, lambda: None if self._shown() else self._poll.stop())

    def _show_all(self):
        for v in self._shown():
            v.show_state(self.current(), len(self.players))

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = MediaPanel(self)
            self.panel.hidden.connect(self._maybe_stop)
        if self.panel.isVisible() and not self.panel.property("fading_out"):
            self.panel.fade_hide()
        else:
            self.panel.show_and_raise(activate=False)
            self.refresh()
            self._poll.start()

    @staticmethod
    def geometry_saved() -> dict:
        g = load_json(SAVE_PATH, {})
        return g if isinstance(g, dict) else {}

    @staticmethod
    def remember(g):
        try:
            save_json(SAVE_PATH, {"x": g.x(), "y": g.y(), "w": g.width(), "h": g.height()})
        except OSError:
            pass

    # ── playerctl ──────────────────────────────────────────────────────────
    @staticmethod
    def has_playerctl() -> bool:
        """Whether there is a way to read what is playing (playerctl, or Windows' media sessions)."""
        if IS_WIN:
            from modules import media_win
            return media_win.available()
        return bool(shutil.which("playerctl"))

    def _show_players(self, players: list):
        self._asking = False
        self.players = players
        self._show_all()

    def refresh(self):
        if IS_WIN:
            if not self._asking:
                from modules import media_win
                self._asking = True
                threading.Thread(target=lambda: self._win_ready.emit(media_win.query()), daemon=True,
                                 name="media").start()
            return
        if not self.has_playerctl():
            for v in self._shown():
                v.show_state(None, 0)
            return
        if self._proc is not None:
            return                              # the last question is still out
        fmt = SEP.join("{{" + f + "}}" for f in FIELDS)
        self._proc = QProcess(self)
        self._proc.finished.connect(self._answered)
        self._proc.start("playerctl", ["-a", "metadata", "--format", fmt])

    def _answered(self, *_):
        proc, self._proc = self._proc, None
        if proc is None:
            return
        out = bytes(proc.readAllStandardOutput()).decode("utf-8", "replace")
        proc.deleteLater()
        self.players = parse(out)
        self._show_all()

    def current(self) -> dict | None:
        """The chosen player, else the one playing, else the first."""
        if not self.players:
            return None
        chosen = next((p for p in self.players if p["player"] == self._chosen), None)
        return chosen or next((p for p in self.players if p["status"] == "Playing"), self.players[0])

    def next_player(self):
        if len(self.players) > 1:
            names = [p["player"] for p in self.players]
            cur = self.current()["player"]
            self._chosen = names[(names.index(cur) + 1) % len(names)]
            self._show_all()

    def _send(self, command: str):
        if IS_WIN:
            from modules import media_win
            cur = self.current()
            threading.Thread(target=media_win.control, args=(cur["id"] if cur else "", command), daemon=True).start()
            QTimer.singleShot(400, self.refresh)
            return
        if not self.has_playerctl():
            self.ctx.notify(self.title, "Media controls need playerctl: sudo pacman -S playerctl")
            return
        cur = self.current()
        args = (["-p", cur["player"]] if cur else []) + [command]
        QProcess.startDetached("playerctl", args)
        QTimer.singleShot(300, self.refresh)

    def play_pause(self):
        self._send("play-pause")

    def next(self):
        self._send("next")

    def previous(self):
        self._send("previous")
