"""
Timer / Stopwatch: floating panels, as many as you like.

Time is measured with a monotonic clock (the old version added 10 ms per tick
and drifted badly).  A finished timer plays a sound, flashes, and shows DONE.
Open panels are saved to data/timers.json and restored on start, still running:
a running panel stores the wall-clock time it started, so a restart (or PC
reboot) carries on counting, and a timer that ran out meanwhile shows DONE.

Stopwatches have laps (button or the lap hotkey, which laps the stopwatch you
started last). Running timers are listed in the tray icon's tooltip.
"""
import os
import time

from PyQt6.QtWidgets import (
    QWidget, QLabel, QPushButton, QVBoxLayout, QHBoxLayout, QFrame, QSizePolicy, QDialog,
    QSpinBox, QFormLayout, QCheckBox, QComboBox, QLineEdit, QScrollArea
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal

from core import DATA_DIR, theme, widgets, sounds, save_json, load_json
from core.module import Module

SAVE_PATH = os.path.join(DATA_DIR, "timers.json")
RUN_GREEN = "#3FBF74"


def fmt_clock(ms: float, show_ms: bool) -> tuple[str, str]:
    """The big display: (12:48, .36), with hours only when there are some (1:02:48)."""
    ms = max(0, int(ms))
    total_s = ms // 1000
    h, m, sec = total_s // 3600, (total_s % 3600) // 60, total_s % 60
    main = f"{h}:{m:02d}:{sec:02d}" if h else f"{m:02d}:{sec:02d}"
    return main, (f".{(ms % 1000) // 10:02d}" if show_ms else "")


def fmt_ms(ms: float, show_ms: bool = True) -> str:
    ms = max(0, int(ms))
    total_s = ms // 1000
    text = f"{total_s // 3600:02d}:{(total_s % 3600) // 60:02d}:{total_s % 60:02d}"
    return text + (f".{(ms % 1000) // 10:02d}" if show_ms else "")


class NewTimerDialog(QDialog):
    def __init__(self, parent=None, mode: str = "stopwatch"):
        super().__init__(parent)
        self.setWindowTitle("New Timer / Stopwatch")
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        self.setMinimumWidth(320)

        lay = QVBoxLayout(self)
        lay.setSpacing(12)
        lay.setContentsMargins(20, 20, 20, 20)
        form = QFormLayout()
        form.setSpacing(8)

        self.name = QLineEdit()
        self.name.setPlaceholderText("Optional label, e.g. Tea")
        form.addRow("Label:", self.name)

        self.mode = QComboBox()
        self.mode.addItems(["Stopwatch", "Timer"])
        self.mode.setCurrentIndex(1 if mode == "timer" else 0)
        self.mode.currentIndexChanged.connect(self._mode_changed)
        form.addRow("Mode:", self.mode)

        self.h = QSpinBox(); self.h.setRange(0, 99); self.h.setSuffix(" h")
        self.mi = QSpinBox(); self.mi.setRange(0, 59); self.mi.setSuffix(" m")
        self.se = QSpinBox(); self.se.setRange(0, 59); self.se.setSuffix(" s")
        self.mi.setValue(5)
        row = QHBoxLayout()
        row.setSpacing(6)
        for w in (self.h, self.mi, self.se):
            row.addWidget(w)
        self.duration = QFrame()
        self.duration.setLayout(row)
        widgets.style(self.duration, "background: transparent; border: none;")
        form.addRow("Duration:", self.duration)

        self.ms = QCheckBox("Show milliseconds")
        form.addRow("", self.ms)
        lay.addLayout(form)

        btns = QHBoxLayout()
        cancel = widgets.button("Cancel")
        cancel.clicked.connect(self.reject)
        ok = widgets.button("Create", "primary")
        ok.clicked.connect(self.accept)
        ok.setDefault(True)
        btns.addStretch()
        btns.addWidget(cancel)
        btns.addWidget(ok)
        lay.addLayout(btns)
        self._mode_changed(self.mode.currentIndex())

    def showEvent(self, e):
        super().showEvent(e)
        QTimer.singleShot(0, lambda: widgets.bring_to_front(self))   # opened by a hotkey: take focus

    def _mode_changed(self, idx):
        self.duration.setVisible(idx == 1)

    def config(self) -> dict:
        return {
            "label": self.name.text().strip(),
            "mode": "timer" if self.mode.currentIndex() == 1 else "stopwatch",
            "duration_ms": (self.h.value() * 3600 + self.mi.value() * 60 + self.se.value()) * 1000,
            "show_ms": self.ms.isChecked(),
        }


class TimerPanel(widgets.FloatingPanel):
    def __init__(self, manager: "TimerModule", cfg: dict):
        self.m = manager
        self.cfg = cfg
        self.mode = cfg.get("mode", "stopwatch")
        self.show_ms = bool(cfg.get("show_ms", False))
        self.duration_ms = int(cfg.get("duration_ms", 0))
        super().__init__(self._title_text(),
                         min_size=(int(manager.s("min_width", 180)), int(manager.s("min_height", 120))),
                         opacity=float(manager.s("bg_opacity", 0.85)),
                         drag_needs_edit_mode=False, close_hides=False)
        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self.set_title(self._title_text())          # adds the label as the subtitle

        self._running = False
        self._accum_ms = 0.0          # elapsed while not running
        self._started_at = 0.0        # monotonic timestamp when last started
        self._finished = False

        # header: a green dot while running, the label after the title
        self.run_dot = QFrame()
        self.run_dot.setFixedSize(6, 6)
        self.run_dot.setStyleSheet(f"background: {RUN_GREEN}; border-radius: 3px;")
        self.run_dot.hide()
        hl = self.header.layout()
        hl.insertWidget(hl.indexOf(self.subtitle_label) + 1, self.run_dot)

        # body: the big time, its caption (timers), the pill buttons
        top = QFrame()
        widgets.style(top, "background: transparent; border: none;")
        tl = QVBoxLayout(top)
        tl.setContentsMargins(18, 16, 18, 16)
        tl.setSpacing(14)
        self.time_label = QLabel()
        self.time_label.setProperty("mono", True)
        self.time_label.setTextFormat(Qt.TextFormat.RichText)
        self.time_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.time_label.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.time_label.setCursor(Qt.CursorShape.PointingHandCursor)
        self.time_label.mousePressEvent = lambda e: self.play_pause() if e.button() == Qt.MouseButton.LeftButton else None
        tl.addWidget(self.time_label, 1)
        self.caption = widgets.label("", 12, tone="muted")
        self.caption.setAlignment(Qt.AlignmentFlag.AlignCenter)
        tl.addWidget(self.caption)

        play_row = QHBoxLayout()
        play_row.setSpacing(8)
        play_row.addStretch()
        self.play_btn = QPushButton()
        self.play_btn.setProperty("variant", "pill")
        self.play_btn.setProperty("accent", True)
        self.play_btn.setFixedHeight(34)
        self.play_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.play_btn.clicked.connect(self.play_pause)
        self.lap_btn = QPushButton("Lap")
        self.lap_btn.setProperty("variant", "pill")
        self.lap_btn.setStyleSheet("QPushButton { background: transparent; }")
        self.lap_btn.setFixedHeight(34)
        self.lap_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.lap_btn.clicked.connect(self.lap)
        play_row.addWidget(self.play_btn)
        play_row.addWidget(self.lap_btn)
        play_row.addStretch()
        tl.addLayout(play_row)

        self.edit_controls = QFrame()
        widgets.style(self.edit_controls, "background: transparent; border: none;")
        el = QHBoxLayout(self.edit_controls)
        el.setContentsMargins(0, 0, 0, 0)
        el.setSpacing(6)
        reset = widgets.button("Reset", "ghost")
        reset.clicked.connect(self.reset)
        self.switch_btn = widgets.button("", "ghost")
        self.switch_btn.clicked.connect(self.switch_mode)
        el.addStretch()
        el.addWidget(reset)
        el.addWidget(self.switch_btn)
        el.addStretch()
        self.edit_controls.hide()
        tl.addWidget(self.edit_controls)
        self.body.addWidget(top, 1)

        # laps: newest first, a hairline above
        self.laps: list[float] = [float(x) for x in cfg.get("laps", []) if isinstance(x, (int, float))]
        self.lap_box = QFrame()
        widgets.style(self.lap_box, f"background: transparent; border: none; border-top: 1px solid {theme.P.border};")
        self.lap_scroll = QScrollArea()
        self.lap_scroll.setWidgetResizable(True)
        self.lap_scroll.setFrameShape(QFrame.Shape.NoFrame)
        self.lap_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.lap_scroll.setMaximumHeight(96)
        lb = QVBoxLayout(self.lap_box)
        lb.setContentsMargins(18, 8, 18, 12)
        lb.addWidget(self.lap_scroll)
        self.body.addWidget(self.lap_box)

        # timers: a 2 px progress line along the bottom
        self.progress = QFrame()
        self.progress.setFixedHeight(2)
        self.progress.setStyleSheet(f"background: {theme.P.bg_frame};")
        self._fill = QFrame(self.progress)
        self._fill.setStyleSheet(f"background: {theme.P.primary};")
        self.body.addWidget(self.progress)

        self._tick = QTimer(self)
        self._tick.setInterval(31 if self.show_ms else 200)
        self._tick.timeout.connect(self._update)

        self.resize(int(cfg.get("w", manager.s("default_width", 260))),
                    int(cfg.get("h", manager.s("default_height", 160))))
        if "x" in cfg and "y" in cfg:
            self.move(int(cfg["x"]), int(cfg["y"]))
            self.clamp_to_screens()
        else:
            self.center_on()
        self.geometry_changed.connect(manager.save_panels)
        self._refresh_switch_text()
        self._refresh_laps()
        self._restore_progress(cfg)
        self._set_playing(self._running)
        self._update()

    # ── helpers ────────────────────────────────────────────────────────────
    def _title_text(self) -> str:
        return "Timer" if self.mode == "timer" else "Stopwatch"

    def set_title(self, text: str, sub: str = ""):
        super().set_title(text, sub or self.cfg.get("label") or "")

    def _refresh_switch_text(self):
        self.switch_btn.setText("→ Timer" if self.mode == "stopwatch" else "→ Stopwatch")

    def elapsed_ms(self) -> float:
        if self._running:
            return self._accum_ms + (time.monotonic() - self._started_at) * 1000.0
        return self._accum_ms

    def state(self) -> dict:
        g = self.geometry()
        st = {**self.cfg, "mode": self.mode, "show_ms": self.show_ms, "duration_ms": self.duration_ms,
              "x": g.x(), "y": g.y(), "w": g.width(), "h": g.height(), "laps": list(self.laps),
              "elapsed_ms": self.elapsed_ms(), "running": self._running, "finished": self._finished}
        if self._running:
            # wall clock, not monotonic: it has to mean the same thing after a reboot
            st["saved_at"] = time.time()
        return st

    def _restore_progress(self, cfg: dict):
        try:
            elapsed = max(0.0, float(cfg.get("elapsed_ms", 0)))
            if cfg.get("running") and "saved_at" in cfg:
                elapsed += max(0.0, time.time() - float(cfg["saved_at"])) * 1000.0
        except (TypeError, ValueError):
            return
        self._accum_ms = elapsed
        if cfg.get("finished") and self.mode == "timer":
            self._finished = True
            self._accum_ms = float(self.duration_ms)
            self._done_look(True)
        elif cfg.get("running"):
            if self.mode == "timer" and elapsed >= self.duration_ms:
                QTimer.singleShot(1500, self._finish)    # ran out while the launcher was closed
                self._accum_ms = float(self.duration_ms)
                return
            self._started_at = time.monotonic()
            self._running = True
            self._tick.start()

    @property
    def running(self) -> bool:
        return self._running

    def tooltip_line(self) -> str:
        label = self.cfg.get("label") or ("Timer" if self.mode == "timer" else "Stopwatch")
        if self.mode == "timer":
            left = self.duration_ms - self.elapsed_ms()
            return f"{label} — {'DONE' if self._finished else fmt_ms(left, False) + ' left'}"
        return f"{label} — {fmt_ms(self.elapsed_ms(), False)}"

    # ── laps ───────────────────────────────────────────────────────────────
    def lap(self):
        if self.mode != "stopwatch" or (not self._running and self.elapsed_ms() <= 0):
            return
        self.laps.append(self.elapsed_ms())
        self._refresh_laps()
        self.m.save_panels()

    def _refresh_laps(self):
        stopwatch = self.mode == "stopwatch"
        self.lap_btn.setVisible(stopwatch)
        self.lap_box.setVisible(stopwatch and bool(self.laps))
        self.progress.setVisible(not stopwatch)
        self.caption.setVisible(not stopwatch)
        holder = QWidget()
        widgets.style(holder, "background: transparent;")
        v = QVBoxLayout(holder)
        v.setContentsMargins(0, 0, 0, 0)
        v.setSpacing(6)
        prev = 0.0
        rows = []
        for i, t in enumerate(self.laps, 1):
            rows.append((f"{i:02d}", "".join(fmt_clock(t - prev, True)), "".join(fmt_clock(t, True))))
            prev = t
        for n, lap, total in reversed(rows):           # newest on top
            r = QHBoxLayout()
            r.setSpacing(0)
            a = widgets.label(n, 12, mono=True, tone="muted")
            a.setFixedWidth(44)
            r.addWidget(a)
            r.addWidget(widgets.label(lap, 12, mono=True), 1)
            r.addWidget(widgets.label(total, 12, mono=True, tone="muted"))
            v.addLayout(r)
        v.addStretch()
        old = self.lap_scroll.takeWidget()
        self.lap_scroll.setWidget(holder)
        if old is not None:
            old.deleteLater()

    def _set_playing(self, on: bool):
        self.play_btn.setText("❚❚  Pause" if on else ("▶  Start" if self.elapsed_ms() <= 0 or self._finished
                                                       else "▶  Resume"))
        self.run_dot.setVisible(on)

    def _done_look(self, done: bool):
        self.time_label.setProperty("tone", "accent2" if done else "primary")
        self.time_label.style().unpolish(self.time_label)
        self.time_label.style().polish(self.time_label)
        self.set_title(self._title_text() + ("  —  DONE" if done else ""))

    # ── controls ───────────────────────────────────────────────────────────
    def play_pause(self):
        if self._running:
            self._accum_ms = self.elapsed_ms()
            self._running = False
            self._tick.stop()
            self._set_playing(False)
        else:
            if self.mode == "timer" and (self._finished or self.duration_ms <= 0):
                self.reset()
                if self.duration_ms <= 0:
                    return
            self._started_at = time.monotonic()
            self._running = True
            self._tick.start()
            self._set_playing(True)
            self.m.last_started = self
        self._update()
        self.m.save_panels()
        self.m.activity.emit()

    def reset(self):
        self._tick.stop()
        self._running = False
        self._finished = False
        self._accum_ms = 0.0
        self.laps = []
        self._refresh_laps()
        self._done_look(False)                      # drop "DONE"
        self._set_playing(False)
        self._update()
        self.m.save_panels()
        self.m.activity.emit()

    def switch_mode(self):
        self.mode = "timer" if self.mode == "stopwatch" else "stopwatch"
        self.set_title(self._title_text())
        self._refresh_switch_text()
        self.reset()                                # also clears the laps and shows/hides the Lap button
        self.m.save_panels()

    def set_edit_mode(self, on: bool):
        super().set_edit_mode(on)
        self.edit_controls.setVisible(on)

    # ── ticking ────────────────────────────────────────────────────────────
    def _update(self):
        ms = self.elapsed_ms()
        if self.mode == "timer":
            ms = self.duration_ms - ms
            if ms <= 0 and self._running:
                ms = 0
                self._finish()
        main, frac = fmt_clock(ms, self.show_ms)
        small = max(10, int(self._time_px * 0.6))
        self.time_label.setText(f'{main}<span style="font-size:{small}px; color:{theme.P.text_muted}">{frac}</span>'
                                if frac else main)
        if self.mode == "timer":
            total = max(1, self.duration_ms)
            done = min(1.0, max(0.0, self.elapsed_ms() / total))
            self._fill.setGeometry(0, 0, int(self.progress.width() * done), 2)
            label = self.cfg.get("label") or ""
            self.caption.setText(f"of {fmt_clock(self.duration_ms, False)[0]}" + (f" · {label}" if label else ""))

    def _finish(self):
        self._running = False
        self._finished = True
        self._accum_ms = float(self.duration_ms)
        self._tick.stop()
        self._set_playing(False)
        self._done_look(True)
        self.show_and_raise(activate=False)         # don't steal the keyboard from whatever you are typing in
        self.m.save_panels()
        self.m.activity.emit()
        if self.m.s("flash_on_finish", True):
            self.flash(8)
        sounds.play(self.m.s("finish_sound", "alarm"), self.m.s("finish_sound_file", ""))

    _time_px = 44

    def resizeEvent(self, e):
        super().resizeEvent(e)
        self._time_px = max(18, min(int(self.width() / (5.6 if self.show_ms else 4.4)), self.height() // 4, 72))
        self.time_label.setFont(theme.qfont(self._time_px, 500, mono=True, spacing=-0.02))
        self._update()

    def closeEvent(self, e):
        self._tick.stop()
        self.m.panel_closed(self)
        super().closeEvent(e)


class TimerModule(Module):
    key = "timer"
    title = "Timer / Stopwatch"
    icon = "⏱"
    activity = pyqtSignal()          # something started, stopped, finished, opened or closed

    def __init__(self, ctx):
        super().__init__(ctx)
        self.last_started: TimerPanel | None = None
        self._panels: list[TimerPanel] = []
        self._edit = False
        self._anchor_bar: widgets.AnchorBar | None = None
        self._dialog_open = False
        self._stopping = False

    def _hotkeys(self):
        self.hotkey("new_hotkey", self.new_timer, "new timer")
        self.hotkey("edit_hotkey", self.toggle_edit, "timer edit mode")
        self.hotkey("lap_hotkey", self.lap, "lap")

    def start(self):
        self._hotkeys()
        if self.s("restore_on_start", True):
            self._restore()

    def stop(self):
        self.save_panels()
        self._stopping = True
        for p in list(self._panels):
            p.close()
        self._panels = []
        self._stopping = False
        if self._anchor_bar:
            self._anchor_bar.close()
            self._anchor_bar = None
        self._edit = False
        self.last_started = None
        self.activity.emit()

    def retheme(self):
        """Panels bake their colours in: save them, close them and bring them back (still running)."""
        self.save_panels()
        edit = self._edit
        self.stop()
        self._restore()
        if edit:
            self.toggle_edit()

    def tray_actions(self):
        return [("New timer / stopwatch", self.new_timer)]

    def actions(self):
        return [("new_timer", "New timer", lambda: self.new_timer("timer")),
                ("new_stopwatch", "New stopwatch", lambda: self.new_timer("stopwatch")),
                ("edit", "Timer edit mode (move, reset, snap)", self.toggle_edit),
                ("lap", "Lap the last stopwatch", self.lap)]

    # ── state for the launcher ─────────────────────────────────────────────
    def any_running(self) -> bool:
        return any(p.running for p in self._panels)

    def tooltip_lines(self) -> list[str]:
        return [p.tooltip_line() for p in self._panels if p.running or p._finished]

    def lap(self):
        target = self.last_started if self.last_started in self._panels else None
        if target is None or target.mode != "stopwatch":
            target = next((p for p in reversed(self._panels) if p.mode == "stopwatch" and p.running), None)
        if target is not None:
            target.lap()

    # ── panels ─────────────────────────────────────────────────────────────
    def new_timer(self, mode: str = "stopwatch"):
        if self._dialog_open:
            return
        self._dialog_open = True
        try:
            dlg = NewTimerDialog(mode=mode if isinstance(mode, str) else "stopwatch")
            if dlg.exec() != QDialog.DialogCode.Accepted:
                return
            self._add_panel(dlg.config())
        finally:
            self._dialog_open = False

    def _add_panel(self, cfg: dict):
        panel = TimerPanel(self, cfg)
        panel.show()
        panel.set_edit_mode(self._edit)
        self._panels.append(panel)
        if panel.running:
            self.last_started = panel
        self.save_panels()
        self.activity.emit()

    def panel_closed(self, panel: TimerPanel):
        if panel in self._panels:
            self._panels.remove(panel)
        if self.last_started is panel:
            self.last_started = None
        if self.running and not self._stopping:
            self.save_panels()
            self.activity.emit()

    def apply_settings(self):
        """Only hotkeys change live; running timers are left alone."""
        self.hotkeys.unregister_all(self.key)
        self._hotkeys()

    def save_panels(self):
        # atomic: the old open("w") + dump left a 0-byte timers.json whenever it was interrupted,
        # and every open timer was lost on the next start
        try:
            save_json(SAVE_PATH, [p.state() for p in self._panels], indent=2)
        except Exception:
            self.log.exception("could not save timers.json")

    def _restore(self):
        saved = load_json(SAVE_PATH, [])
        for cfg in saved if isinstance(saved, list) else []:
            try:
                self._add_panel(cfg)
            except Exception:
                self.log.exception("could not restore timer %r", cfg)

    # ── edit mode ──────────────────────────────────────────────────────────
    def toggle_edit(self):
        self._edit = not self._edit
        for p in self._panels:
            p.set_edit_mode(self._edit)
            if self._edit:
                p.raise_()
        if self._edit:
            if self._anchor_bar is None:
                self._anchor_bar = widgets.AnchorBar("Snap timers to:")
                self._anchor_bar.anchor_chosen.connect(self._snap_all)
            self._anchor_bar.show()
            self._anchor_bar.raise_()
        elif self._anchor_bar:
            self._anchor_bar.hide()

    def _snap_all(self, key: str):
        pad = int(self.s("anchor_padding", 12))
        offset = 0
        for p in self._panels:
            pos = widgets.anchor_position(p.width(), p.height(), key, pad)
            # stack multiple panels so they do not sit on top of each other
            if key[0] == "t":
                pos.setY(pos.y() + offset)
            elif key[0] == "b":
                pos.setY(pos.y() - offset)
            else:
                pos.setX(pos.x() + (offset if key[1] == "l" else -offset))
            p.move(pos)
            offset += p.height() + 8 if key[0] != "m" else p.width() + 8
        self.save_panels()
