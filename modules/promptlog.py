"""
Prompt Log: browse, search and delete command history.

Sources:
  - fish: ~/.local/share/fish/fish_history ("- cmd: ..." entries with their time and paths)
  - bash: $HISTFILE or ~/.bash_history (one command per line, "#<time>" lines kept with them)
  - zsh:  $HISTFILE or ~/.zsh_history (": <time>:0;command", backslash-continued lines)
  - Luminescent: everything the Dashboard launched (data/command_log.txt).

Deleting rewrites the file atomically. A shell that is open keeps its own
in-memory history (↑) and may write it back when it exits; fish merges, so
delete there too ("history delete") if a command keeps coming back.
"""
import os

from PyQt6.QtWidgets import (
    QLabel, QLineEdit, QListWidget, QListWidgetItem, QVBoxLayout, QHBoxLayout, QComboBox, QCheckBox,
    QAbstractItemView
)
from PyQt6.QtCore import Qt, QTimer

from core import theme, widgets, runner, clip
from core.module import Module

_DATA = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
HISTORIES = [
    ("fish", "fish", os.path.join(_DATA, "fish", "fish_history")),
    ("bash", "bash", os.environ.get("HISTFILE", "") if os.environ.get("SHELL", "").endswith("bash") else ""),
    ("bash", "bash", os.path.expanduser("~/.bash_history")),
    ("zsh", "zsh", os.environ.get("HISTFILE", "") if os.environ.get("SHELL", "").endswith("zsh") else ""),
    ("zsh", "zsh", os.path.expanduser("~/.zsh_history")),
]
LOG_KIND = "log"


def sources() -> list[tuple[str, str]]:
    """(name, path) of every history file that exists, then Luminescent's own log."""
    out, seen = [], set()
    for kind, name, path in HISTORIES:
        if path and os.path.isfile(path) and path not in seen:
            seen.add(path)
            out.append((name, path))
    out.append(("Launched by Luminescent", runner.COMMAND_LOG))
    return out


def kind_of(path: str) -> str:
    if os.path.normcase(path) == os.path.normcase(runner.COMMAND_LOG):
        return LOG_KIND
    base = os.path.basename(path)
    if base == "fish_history":
        return "fish"
    if "zsh" in base:
        return "zsh"
    return "bash"


def _fish_unescape(text: str) -> str:
    out, i = [], 0
    while i < len(text):
        c = text[i]
        if c == "\\" and i + 1 < len(text):
            nxt = text[i + 1]
            out.append("\n" if nxt == "n" else nxt)
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def read_entries(path: str) -> list[tuple[str, str]]:
    """[(command, the raw text it is stored as)] in file order (oldest first)."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            raw = f.read().splitlines()
    except FileNotFoundError:
        return []
    kind = kind_of(path)
    out: list[tuple[str, str]] = []
    if kind == "fish":
        block: list[str] = []
        for line in raw + ["- cmd: "]:
            if line.startswith("- cmd: "):
                if block:
                    out.append((_fish_unescape(block[0][len("- cmd: "):]), "\n".join(block)))
                block = [line]
            elif block:
                block.append(line)
        return [(c, r) for c, r in out if c.strip()]
    if kind == "zsh":
        buf: list[str] = []
        for line in raw:
            buf.append(line)
            if line.endswith("\\"):
                continue
            text = "\n".join(buf)
            cmd = text.split(";", 1)[1] if text.startswith(": ") and ";" in text else text
            out.append((cmd.replace("\\\n", "\n"), text))
            buf = []
        return [(c, r) for c, r in out if c.strip()]
    if kind == LOG_KIND:
        return [(line.split("\t", 1)[-1], line) for line in raw if line.strip()]
    stamp = ""
    for line in raw:                                   # bash
        if line.startswith("#") and line[1:].isdigit():
            stamp = line + "\n"
            continue
        if line.strip():
            out.append((line, stamp + line))
        stamp = ""
    return out


def read_commands(path: str) -> list[str]:
    return [c for c, _ in read_entries(path)]


def write_entries(path: str, raw: list[str]):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(raw) + ("\n" if raw else ""))
    try:
        os.chmod(tmp, os.stat(path).st_mode & 0o777)
    except OSError:
        pass
    os.replace(tmp, path)


class PromptLogPanel(widgets.FloatingPanel):
    def __init__(self, m: "PromptLogModule"):
        self.m = m
        super().__init__("Prompt Log", min_size=(520, 360), opacity=0.97, close_hides=True)
        self._cmds: list[str] = []
        lay = QVBoxLayout()
        lay.setContentsMargins(12, 10, 12, 12)
        lay.setSpacing(8)
        top = QHBoxLayout()
        self.source = QComboBox()
        self.source.currentIndexChanged.connect(self.reload)
        top.addWidget(self.source, 1)
        self.dedupe = QCheckBox("Hide repeats")
        self.dedupe.setChecked(bool(m.s("dedupe", True)))
        self.dedupe.toggled.connect(self._fill)
        top.addWidget(self.dedupe)
        lay.addLayout(top)
        self.search = QLineEdit()
        self.search.setPlaceholderText("🔍  Search commands…")
        self.search.setClearButtonEnabled(True)
        self.search.textChanged.connect(self._fill)
        lay.addWidget(self.search)
        self.list = QListWidget()
        self.list.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.list.setStyleSheet(f"QListWidget {{ font-family: {theme.MONO_CSS}; font-size: 12px; }}")
        self.list.itemDoubleClicked.connect(lambda it: self._copy([it]))
        lay.addWidget(self.list, 1)
        self.count = QLabel()
        self.count.setObjectName("lumHint")
        lay.addWidget(self.count)
        btns = QHBoxLayout()
        for text, fn, variant in (("Copy", lambda: self._copy(self.list.selectedItems()), ""),
                                  ("Run in a terminal", self._run, ""),
                                  ("Delete selected…", self._delete, "danger"),
                                  ("Clear all…", self._clear, "danger")):
            b = widgets.button(text, variant)
            b.clicked.connect(fn)
            btns.addWidget(b)
        btns.addStretch()
        lay.addLayout(btns)
        hint = QLabel("Double-click copies. Shows your fish, bash and zsh history and what Luminescent launched.")
        hint.setObjectName("lumHint")
        hint.setWordWrap(True)
        lay.addWidget(hint)
        self.body.addLayout(lay)
        self.resize(760, 560)
        self.center_on(widgets.cursor_screen_geometry())
        self.escape_pressed.connect(self.hide)

    def refresh_sources(self):
        cur = self.source.currentData()
        self.source.blockSignals(True)
        self.source.clear()
        for name, path in sources():
            self.source.addItem(name, path)
        i = self.source.findData(cur)
        self.source.setCurrentIndex(i if i >= 0 else 0)
        self.source.blockSignals(False)
        self.reload()

    def path(self) -> str:
        return self.source.currentData() or ""

    def reload(self, *_):
        self._entries = read_entries(self.path()) if self.path() else []
        self._cmds = [c for c, _ in self._entries]
        self._fill()

    def _fill(self, *_):
        q = self.search.text().strip().lower()
        self.list.clear()
        seen = set()
        shown = 0
        for i in range(len(self._cmds) - 1, -1, -1):           # newest first
            c = self._cmds[i]
            if self.dedupe.isChecked():
                if c in seen:
                    continue
                seen.add(c)
            if q and q not in c.lower():
                continue
            it = QListWidgetItem(c if "\n" not in c else c.replace("\n", "  ⏎ "))
            it.setData(Qt.ItemDataRole.UserRole, i)
            it.setToolTip(c[:2000])
            self.list.addItem(it)
            shown += 1
            if shown >= 3000:
                break
        self.count.setText(f"{shown} shown · {len(self._cmds)} in {os.path.basename(self.path())}")

    def _copy(self, items):
        if not items:
            return
        clip.set_text("\n".join(self._cmds[it.data(Qt.ItemDataRole.UserRole)] for it in items))
        self.count.setText(f"Copied {len(items)} command(s).")

    def _run(self):
        items = self.list.selectedItems()
        if len(items) != 1:
            widgets.message(self, "Run", "Select one command.")
            return
        cmd = self._cmds[items[0].data(Qt.ItemDataRole.UserRole)]
        err = runner.open_terminal(os.path.expanduser("~"), cmd)
        if err:
            widgets.error(self, "Run", err)

    def _indices_to_drop(self, items) -> set[int]:
        """Selected rows; with "Hide repeats" on, every copy of a selected command goes."""
        chosen = {self._cmds[it.data(Qt.ItemDataRole.UserRole)] for it in items}
        if self.dedupe.isChecked():
            return {i for i, c in enumerate(self._cmds) if c in chosen}
        return {it.data(Qt.ItemDataRole.UserRole) for it in items}

    def _delete(self):
        items = self.list.selectedItems()
        if not items:
            return
        drop = self._indices_to_drop(items)
        if not widgets.confirm(self, "Delete", f"Delete {len(drop)} entr{'y' if len(drop) == 1 else 'ies'} from "
                                               f"{os.path.basename(self.path())}?", yes="Delete", danger=True):
            return
        self._write([i for i in range(len(self._cmds)) if i not in drop])

    def _clear(self):
        if not self._cmds:
            return
        if widgets.confirm(self, "Clear all", f"Delete all {len(self._cmds)} commands in "
                                              f"{os.path.basename(self.path())}? This cannot be undone.",
                           yes="Clear all", danger=True):
            self._write([])

    def _write(self, keep: list[int]):
        try:
            write_entries(self.path(), [self._entries[i][1] for i in keep])
        except OSError as e:
            widgets.error(self, "Prompt Log", f"Could not write the file:\n{e}")
        self.reload()


class PromptLogModule(Module):
    key = "promptlog"
    title = "Prompt Log"
    icon = "⌨"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.panel: PromptLogPanel | None = None

    def start(self):
        self.hotkey("hotkey", self.toggle, "prompt log")

    def stop(self):
        if self.panel:
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self.start()

    def retheme(self):
        if self.panel and not self.panel.isVisible():
            self.stop()

    def actions(self):
        return [("open", "Prompt Log (command history)", self.toggle),
                ("terminal", "Open a Luminescent terminal", lambda: runner.open_terminal())]

    def toggle(self):
        if self.panel is None:
            self.panel = PromptLogPanel(self)
        if self.panel.isVisible():
            self.panel.hide()
        else:
            self.panel.refresh_sources()
            self.panel.show_and_raise()
            QTimer.singleShot(0, self.panel.search.setFocus)
