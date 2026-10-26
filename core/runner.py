"""
Starting dashboard items: scripts, programs, files, folders, URLs, commands.

    .py / .sh / other executable scripts → a terminal window, kept open afterwards
    .pyw         → Python, no window
    .desktop     → started like the app launcher would (gtk-launch / gio)
    executables  → run directly (with arguments)
    other files, folders → the default app (xdg-open)
    http(s)://…  → the default browser
    anything else → run as a shell command ("gnome-calculator", "code .")

Every launch is appended to data/command_log.txt for the Prompt Log.

launch() also hands back the process it started (the dashboard shows the item as running and can
stop it), takes a working folder, and can run the item as administrator: scripts get `sudo` in
their terminal (so you type the password there), programs and commands go through pkexec.
"""
import os
import sys
import shlex
import shutil
import logging
import subprocess
import webbrowser
from datetime import datetime

from core import DATA_DIR, FROZEN, terminal

log = logging.getLogger("runner")

COMMAND_LOG = os.path.join(DATA_DIR, "command_log.txt")
CONSOLE_EXT = {".py", ".sh", ".bash", ".zsh", ".fish", ".ps1"}


def python_exe(configured: str = "", windowless: bool = False) -> str:
    if configured and os.path.isfile(configured):
        return configured
    if not FROZEN:
        return sys.executable
    return shutil.which("python3") or shutil.which("python") or "python3"


def log_command(text: str):
    try:
        with open(COMMAND_LOG, "a", encoding="utf-8") as f:
            f.write(f"{datetime.now().isoformat(timespec='seconds')}\t{text}\n")
    except OSError:
        pass


def _executable(path: str) -> bool:
    return os.path.isfile(path) and os.access(path, os.X_OK)


def kind_of(target: str) -> str:
    t = (target or "").strip()
    if t.lower().startswith(("http://", "https://", "www.")):
        return "url"
    path = os.path.expandvars(os.path.expanduser(t))
    if os.path.isdir(path):
        return "folder"
    ext = os.path.splitext(path)[1].lower()
    if os.path.isfile(path):
        if ext in (".py", ".pyw"):
            return "python"
        if ext in (".sh", ".desktop", ".appimage") or ext in CONSOLE_EXT or _executable(path):
            return "program"
        return "file"
    return "command" if t else "none"


ICONS = {"url": "🌐", "folder": "📁", "python": "🐍", "program": "⚙", "file": "📄", "command": "›_", "none": "·"}


def _spawn(cmd, **kw):
    """Start a process that outlives Luminescent and never holds its terminal or stdin."""
    kw.setdefault("stdin", subprocess.DEVNULL)
    kw.setdefault("stdout", subprocess.DEVNULL)
    kw.setdefault("stderr", subprocess.DEVNULL)
    return subprocess.Popen(cmd, start_new_session=True, close_fds=True, **kw)


def open_path(path: str) -> str:
    """Open a file or folder with its default app. Returns '' or an error message."""
    try:
        _spawn(["xdg-open", path])
        return ""
    except OSError as e:
        return str(e)


def _elevated(cmd: list[str]) -> list[str]:
    """`cmd` through pkexec, keeping what a graphical program needs to reach your session."""
    keep = [f"{k}={os.environ[k]}" for k in ("DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "XAUTHORITY",
                                              "DBUS_SESSION_BUS_ADDRESS", "QT_QPA_PLATFORM") if os.environ.get(k)]
    return [shutil.which("pkexec") or "pkexec", "env", *keep, *cmd]


def run(target: str, args: str = "", name: str = "", opts: dict | None = None) -> str:
    """Start `target`. Returns '' or an error message. `opts`: see launch()."""
    return launch(target, args, name, opts)[0]


def launch(target: str, args: str = "", name: str = "", opts: dict | None = None):
    """Start `target`. Returns (error or '', the Popen or None when nothing trackable was started).

    `opts`: use_terminal, keep_open, python_path, terminal, cwd (working folder), admin (bool)."""
    opts = opts or {}
    target = (target or "").strip().strip('"')
    if not target:
        return "No file or command is set for this item.", None
    path = os.path.expandvars(os.path.expanduser(target))
    try:
        argv = shlex.split(args) if args else []
    except ValueError:
        argv = args.split()
    title = name or os.path.basename(path) or target
    admin = bool(opts.get("admin"))
    work = os.path.expandvars(os.path.expanduser((opts.get("cwd") or "").strip()))
    if work and not os.path.isdir(work):
        return f"The working folder does not exist: {work}", None
    try:
        if target.lower().startswith(("http://", "https://", "www.")):
            webbrowser.open(target if "://" in target else "https://" + target)
            log_command(f"open {target}")
            return "", None
        if os.path.isdir(path):
            log_command(f"open {path}")
            return open_path(path), None
        if not os.path.exists(path):
            cmd = f"{target} {args}".strip()
            full = _elevated(["sh", "-c", cmd]) if admin else ["sh", "-c", cmd]
            proc = _spawn(full, cwd=work or os.path.expanduser("~"))
            log_command(("sudo " if admin else "") + cmd)
            return "", proc
        ext = os.path.splitext(path)[1].lower()
        cwd = work or os.path.dirname(path)
        if ext == ".pyw":
            cmd = [python_exe(opts.get("python_path", ""), windowless=True), path] + argv
            proc = _spawn(_elevated(cmd) if admin else cmd, cwd=cwd)
            log_command(shlex.join(cmd))
            return "", proc
        if ext == ".desktop":
            launcher = shutil.which("gtk-launch") or shutil.which("gio")
            if launcher and launcher.endswith("gio"):
                cmd = [launcher, "launch", path] + argv
            elif launcher:
                cmd = [launcher, os.path.basename(path)[:-len(".desktop")]] + argv
            else:
                cmd = ["xdg-open", path]
            _spawn(cmd, cwd=cwd)
            log_command(shlex.join(cmd))
            return "", None
        if ext in CONSOLE_EXT:
            if ext == ".py":
                cmd = [python_exe(opts.get("python_path", "")), path] + argv
            elif ext == ".ps1":
                cmd = [shutil.which("pwsh") or "pwsh", "-NoLogo", "-File", path] + argv
            elif _executable(path):
                cmd = [path] + argv
            else:
                shell = {".sh": "sh", ".bash": "bash", ".zsh": "zsh", ".fish": "fish"}.get(ext, "sh")
                cmd = [shell, path] + argv
            if admin:
                cmd = [shutil.which("sudo") or "sudo", *cmd]
            err, proc = _console(cmd, cwd, title, opts)
            log_command(shlex.join(cmd))
            return err, proc
        if _executable(path):
            cmd = [path] + argv
            proc = _spawn(_elevated(cmd) if admin else cmd, cwd=cwd)
            log_command(shlex.join(cmd))
            return "", proc
        log_command(f"open {path}")
        return open_path(path), None
    except Exception as e:
        log.warning("could not start %s: %s", target, e)
        return f"{type(e).__name__}: {e}", None


def _keep_open(cmd: list[str]) -> list[str]:
    """Run `cmd`, then stay in an interactive shell so its output can be read."""
    script = '"$@"; code=$?; echo; echo "[finished with exit code $code]"; exec "${SHELL:-sh}"'
    return ["sh", "-c", script, "sh"] + cmd


def _console(cmd: list[str], cwd: str, title: str, opts: dict):
    if opts.get("keep_open", True):
        cmd = _keep_open(cmd)
    if opts.get("use_terminal", True):
        full = terminal.command(title, cwd, cmd, opts.get("terminal", ""))
        if not full:
            return "No terminal program found. Set one in Settings → Dashboard & Scripts.", None
        return "", _spawn(full, cwd=cwd or None)
    return "", _spawn(cmd, cwd=cwd or None)        # no window: output is discarded


def stop(proc) -> bool:
    """Stop a process launch() started, with everything it started (its own session). True if it was running."""
    import signal
    if proc is None or proc.poll() is not None:
        return False
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except (ProcessLookupError, PermissionError):
        try:
            proc.terminate()
        except OSError:
            return False
    return True


def open_terminal(cwd: str = "", command: str = "", configured: str = "") -> str:
    """A new terminal window in `cwd`, optionally running a shell command first (then staying open)."""
    shell = os.environ.get("SHELL") or "sh"
    argv = _keep_open(["sh", "-c", command]) if command else [shell]
    full = terminal.command("Terminal", cwd, argv, configured)
    if not full:
        return "No terminal program found."
    try:
        _spawn(full, cwd=cwd or None)
        return ""
    except OSError as e:
        return str(e)
