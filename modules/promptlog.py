"""
Prompt Log: browse, search and delete command history.

Sources:
  - fish: ~/.local/share/fish/fish_history ("- cmd: ..." entries with their time and paths)
  - bash: $HISTFILE or ~/.bash_history (one command per line, "#<time>" lines kept with them)
  - zsh:  $HISTFILE or ~/.zsh_history (": <time>:0;command", backslash-continued lines)
  - PowerShell (Windows): PSReadLine's %APPDATA%\\Microsoft\\Windows\\PowerShell\\PSReadLine\\<host>_history.txt
    (ConsoleHost = Windows Terminal / the console); multi-line commands end each continued line with `
  - Luminescent: everything the Dashboard launched (data/command_log.txt).

Deleting rewrites the file atomically. A shell that is open keeps its own
in-memory history (↑) and may write it back when it exits; fish merges, so
delete there too ("history delete") if a command keeps coming back.

Pinned commands (data/promptlog.json) stay at the top of every list. Each row shows
when the command ran, where the history keeps that. "Save to Dashboard" turns a
command into a dashboard item that runs it in a terminal.
"""
import os
import re
import glob
from datetime import datetime

from PyQt6.QtWidgets import (
    QLabel, QLineEdit, QListWidget, QListWidgetItem, QVBoxLayout, QHBoxLayout, QComboBox, QCheckBox,
    QAbstractItemView
)
from PyQt6.QtCore import Qt, QTimer

from core import DATA_DIR, theme, widgets, runner, clip, save_json, load_json
from core.module import Module

PINS_PATH = os.path.join(DATA_DIR, "promptlog.json")

_DATA = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
HISTORIES = [
    ("fish", "fish", os.path.join(_DATA, "fish", "fish_history")),
    ("bash", "bash", os.environ.get("HISTFILE", "") if os.environ.get("SHELL", "").endswith("bash") else ""),
    ("bash", "bash", os.path.expanduser("~/.bash_history")),
    ("zsh", "zsh", os.environ.get("HISTFILE", "") if os.environ.get("SHELL", "").endswith("zsh") else ""),
    ("zsh", "zsh", os.path.expanduser("~/.zsh_history")),
]
LOG_KIND = "log"
PS_DIR = os.path.join(os.environ.get("APPDATA", ""), "Microsoft", "Windows", "PowerShell", "PSReadLine")


def sources() -> list[tuple[str, str]]:
    """(name, path) of every history file that exists, then Luminescent's own log."""
    out, seen = [], set()
    for kind, name, path in HISTORIES:
        if path and os.path.isfile(path) and path not in seen:
            seen.add(path)
            out.append((name, path))
    if os.environ.get("APPDATA"):
        for path in sorted(glob.glob(os.path.join(PS_DIR, "*_history.txt"))):
            host = os.path.basename(path)[:-len("_history.txt")]
            out.append(({"ConsoleHost": "PowerShell (Terminal / console)",
                         "Visual Studio Code Host": "PowerShell in VS Code"}.get(host, f"PowerShell ({host})"), path))
    out.append(("Launched by Luminescent", runner.COMMAND_LOG))
    return out


def kind_of(path: str) -> str:
    if os.path.normcase(path) == os.path.normcase(runner.COMMAND_LOG):
        return LOG_KIND
    base = os.path.basename(path)
    if base.endswith("_history.txt") and "PSReadLine" in path:
        return "powershell"
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
    if kind == "powershell":
        buf = []
        for line in raw:
            buf.append(line)
            if line.endswith("`"):
                continue
            out.append(("\n".join(b[:-1] if b.endswith("`") else b for b in buf), "\n".join(buf)))
            buf = []
        if buf:
            out.append(("\n".join(buf), "\n".join(buf)))
        return [(c, r) for c, r in out if c.strip()]
    stamp = ""
    for line in raw:                                   # bash
        if line.startswith("#") and line[1:].isdigit():
            stamp = line + "\n"
            continue
        if line.strip():
            out.append((line, stamp + line))
        stamp = ""
    return out


def entry_time(kind: str, raw: str) -> datetime | None:
    """When a history entry ran, if its file keeps that (fish, zsh with timestamps, bash with HISTTIMEFORMAT,
    Luminescent's own log)."""
    try:
        if kind == LOG_KIND:
            return datetime.fromisoformat(raw.split("\t", 1)[0])
        if kind == "fish":
            m = re.search(r"^\s+when: (\d+)", raw, re.M)
        elif kind == "zsh":
            m = re.match(r": (\d+):", raw)
        else:
            m = re.match(r"#(\d+)\n", raw)
        return datetime.fromtimestamp(int(m.group(1))) if m else None
    except (ValueError, OSError, OverflowError):
        return None


def _when(t: datetime | None) -> str:
    if t is None:
        return ""
    now = datetime.now()
    if t.date() == now.date():
        return t.strftime("%H:%M")
    if t.year == now.year:
        return f"{t.day} {t:%b} {t:%H:%M}"
    return f"{t.day} {t:%b %Y}"


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
        self.list.setStyleSheet(f"QListWidget {{ font-family: {theme.MONO_CSS}; font-size: {theme.fs(12)}px; }}")
        self.list.itemDoubleClicked.connect(lambda it: self._copy([it]))
        self.list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.list.customContextMenuRequested.connect(self._menu)
        lay.addWidget(self.list, 1)
        self.count = QLabel()
        self.count.setObjectName("lumHint")
        lay.addWidget(self.count)
        btns = QHBoxLayout()
        for text, fn, variant in (("Copy", lambda: self._copy(self.list.selectedItems()), ""),
                                  ("Run in a terminal", self._run, ""),
                                  ("📌 Pin / unpin", self._toggle_pin, ""),
                                  ("Save to Dashboard…", self._to_dashboard, ""),
                                  ("Delete selected…", self._delete, "danger"),
                                  ("Clear all…", self._clear, "danger")):
            b = widgets.button(text, variant)
            b.clicked.connect(fn)
            btns.addWidget(b)
        btns.addStretch()
        lay.addLayout(btns)
        hint = QLabel("Double-click copies, right-click for more. Pinned commands stay at the top of every list. "
                      "Shows your fish, bash and zsh history and what Luminescent launched.")
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

    def _row_text(self, c: str, when: str, pinned: bool) -> str:
        text = c if "\n" not in c else c.replace("\n", "  ⏎ ")
        return ("📌  " if pinned else "") + (f"{when:>13}   " if when else "") + text

    def _fill(self, *_):
        q = self.search.text().strip().lower()
        self.list.clear()
        kind = kind_of(self.path()) if self.path() else ""
        pins = self.m.pins()
        for c in pins:                                        # pinned first, from any history
            if q and q not in c.lower():
                continue
            it = QListWidgetItem(self._row_text(c, "", True))
            it.setData(Qt.ItemDataRole.UserRole, -1)
            it.setData(Qt.ItemDataRole.UserRole + 1, c)
            it.setToolTip(c[:2000])
            self.list.addItem(it)
        seen = set(pins) if self.dedupe.isChecked() else set()
        shown = 0
        for i in range(len(self._cmds) - 1, -1, -1):           # newest first
            c = self._cmds[i]
            if self.dedupe.isChecked():
                if c in seen:
                    continue
                seen.add(c)
            if q and q not in c.lower():
                continue
            when = _when(entry_time(kind, self._entries[i][1]))
            it = QListWidgetItem(self._row_text(c, when, False))
            it.setData(Qt.ItemDataRole.UserRole, i)
            it.setData(Qt.ItemDataRole.UserRole + 1, c)
            it.setToolTip(c[:2000])
            self.list.addItem(it)
            shown += 1
            if shown >= 3000:
                break
        self.count.setText(f"{shown} shown · {len(self._cmds)} in {os.path.basename(self.path())}")

    @staticmethod
    def _cmd(it) -> str:
        return it.data(Qt.ItemDataRole.UserRole + 1) or ""

    def _copy(self, items):
        if not items:
            return
        clip.set_text("\n".join(self._cmd(it) for it in items))
        self.count.setText(f"Copied {len(items)} command(s).")

    def _one(self, what: str) -> str:
        items = self.list.selectedItems()
        if len(items) != 1:
            widgets.message(self, what, "Select one command.")
            return ""
        return self._cmd(items[0])

    def _run(self):
        cmd = self._one("Run")
        if not cmd:
            return
        err = runner.open_terminal(os.path.expanduser("~"), cmd)
        if err:
            widgets.error(self, "Run", err)

    def _toggle_pin(self):
        items = self.list.selectedItems()
        if not items:
            return
        pins = self.m.pins()
        for it in items:
            c = self._cmd(it)
            if c in pins:
                pins.remove(c)
            else:
                pins.append(c)
        self.m.set_pins(pins)
        self._fill()

    def _to_dashboard(self):
        cmd = self._one("Save to Dashboard")
        if cmd:
            self.m.save_to_dashboard(cmd, self)

    def _menu(self, pos):
        it = self.list.itemAt(pos)
        if it is None:
            return
        c = self._cmd(it)
        m = widgets.menu(self)
        m.addAction("Copy", lambda: self._copy([it]))
        m.addAction("Run in a terminal", lambda: runner.open_terminal(os.path.expanduser("~"), c))
        m.addAction("Unpin" if c in self.m.pins() else "📌  Pin to the top",
                    lambda: (self.list.setCurrentItem(it), self._toggle_pin()))
        m.addAction("Save to Dashboard…", lambda: self.m.save_to_dashboard(c, self))
        m.exec(self.list.mapToGlobal(pos))

    def _indices_to_drop(self, items) -> set[int]:
        """Selected rows; with "Hide repeats" on, every copy of a selected command goes."""
        items = [it for it in items if it.data(Qt.ItemDataRole.UserRole) >= 0]
        chosen = {self._cmd(it) for it in items}
        if self.dedupe.isChecked():
            return {i for i, c in enumerate(self._cmds) if c in chosen}
        return {it.data(Qt.ItemDataRole.UserRole) for it in items}

    def _delete(self):
        items = [it for it in self.list.selectedItems() if it.data(Qt.ItemDataRole.UserRole) >= 0]
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

    def pins(self) -> list[str]:
        d = load_json(PINS_PATH, {})
        pins = d.get("pins", []) if isinstance(d, dict) else []
        return [p for p in pins if isinstance(p, str)]

    def set_pins(self, pins: list[str]):
        try:
            save_json(PINS_PATH, {"pins": pins}, indent=2)
        except OSError as e:
            self.ctx.notify(self.title, f"Could not save the pins: {e}")

    def save_to_dashboard(self, cmd: str, parent=None):
        """Make `cmd` a dashboard item (in a box you pick) that runs it in a terminal."""
        launcher = self.ctx.launcher
        if launcher is None:
            return
        dash = launcher.dashboard
        tabs = [t for t in dash.tabs() if t["id"] != "tools"]
        if not tabs:
            tabs = [dash.add_tab("Commands", 96, 136)]
        if len(tabs) == 1:
            tab = tabs[0]
        else:
            title, ok = widgets.ask_choice(parent, "Save to Dashboard", "Which box?", [t["title"] for t in tabs])
            if not ok:
                return
            tab = next(t for t in tabs if t["title"] == title)
        first = cmd.strip().splitlines()[0] if cmd.strip() else cmd
        name, ok = widgets.ask_text(parent, "Save to Dashboard", "Name:", first[:40])
        if not ok:
            return
        dash.add_item(tab["id"], {"kind": "item", "name": name.strip() or first[:40], "path": cmd.strip(),
                                  "terminal": True})
        self.ctx.notify(self.title, f"Added “{name.strip() or first[:40]}” to {tab['title']}.")

    def actions(self):
        return [("open", "Prompt Log (command history)", self.toggle),
                ("terminal", "Open a Luminescent terminal", lambda: runner.open_terminal())]

    def toggle(self):
        if self.panel is None:
            with self.themed():
                self.panel = PromptLogPanel(self)
        if self.panel.isVisible():
            self.panel.hide()
        else:
            self.panel.refresh_sources()
            self.panel.show_and_raise()
            QTimer.singleShot(0, self.panel.search.setFocus)
