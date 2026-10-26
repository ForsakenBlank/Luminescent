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

Windows: .py/.bat/.cmd/.ps1 open in a Windows Terminal tab (or a console window) that stays open,
.pyw runs with pythonw, programs and files use the "open" verb, commands run without a window, and
"run as administrator" asks through the usual UAC prompt.
"""
import os
import sys
import shlex
import shutil
import logging
import subprocess
import webbrowser
from datetime import datetime

from core import DATA_DIR, FROZEN, IS_WIN, terminal

log = logging.getLogger("runner")

COMMAND_LOG = os.path.join(DATA_DIR, "command_log.txt")
CONSOLE_EXT = {".py", ".bat", ".cmd", ".ps1"} if IS_WIN else {".py", ".sh", ".bash", ".zsh", ".fish", ".ps1"}


def python_exe(configured: str = "", windowless: bool = False) -> str:
    if IS_WIN:
        return _python_exe_win(configured, windowless)
    if configured and os.path.isfile(configured):
        return configured
    if not FROZEN:
        return sys.executable
    return shutil.which("python3") or shutil.which("python") or "python3"


def _python_exe_win(configured: str, windowless: bool) -> str:
    if configured and os.path.isfile(configured):
        exe = configured
    elif not FROZEN:
        exe = sys.executable
    else:
        exe = shutil.which("py") or shutil.which("python") or "python"
    folder, name = os.path.split(exe)
    if name.lower() in ("python.exe", "pythonw.exe"):
        want = "pythonw.exe" if windowless else "python.exe"
        if os.path.isfile(os.path.join(folder, want)):
            return os.path.join(folder, want)
    if windowless and name.lower() == "py.exe" and shutil.which("pyw"):
        return shutil.which("pyw")
    return exe


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
        if ext in (".sh", ".desktop", ".appimage", ".exe", ".lnk") or ext in CONSOLE_EXT or \
                (not IS_WIN and _executable(path)):
            return "program"
        return "file"
    return "command" if t else "none"


ICONS = {"url": "🌐", "folder": "📁", "python": "🐍", "program": "⚙", "file": "📄", "command": "›_", "none": "·"}


def _spawn(cmd, **kw):
    """Start a process that outlives Luminescent and never holds its terminal or stdin."""
    kw.setdefault("stdin", subprocess.DEVNULL)
    kw.setdefault("stdout", subprocess.DEVNULL)
    kw.setdefault("stderr", subprocess.DEVNULL)
    if IS_WIN:
        kw.setdefault("creationflags", subprocess.CREATE_NEW_PROCESS_GROUP)
        return subprocess.Popen(cmd, close_fds=True, **kw)
    return subprocess.Popen(cmd, start_new_session=True, close_fds=True, **kw)


def open_path(path: str) -> str:
    """Open a file or folder with its default app. Returns '' or an error message."""
    try:
        if IS_WIN:
            os.startfile(path)
        else:
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
    if IS_WIN:
        return _launch_win(target, args, name, opts or {})
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


# ── Windows ───────────────────────────────────────────────────────────────────
def _as_admin(exe: str, params: str, cwd: str) -> str:
    """Start `exe` elevated (the UAC prompt). Returns '' or an error. Nothing to track afterwards."""
    import ctypes
    r = ctypes.windll.shell32.ShellExecuteW(None, "runas", exe, params, cwd or None, 1)
    return "" if r > 32 else ("Cancelled at the administrator prompt." if r == 5 else f"Could not start it (error {r}).")


def _launch_win(target: str, args: str, name: str, opts: dict):
    target = (target or "").strip().strip('"')
    if not target:
        return "No file or command is set for this item.", None
    path = os.path.expandvars(os.path.expanduser(target))
    try:
        argv = shlex.split(args, posix=False) if args else []
    except ValueError:
        argv = args.split()
    title = name or os.path.basename(path) or target
    admin = bool(opts.get("admin"))
    work = os.path.expandvars(os.path.expanduser((opts.get("cwd") or "").strip()))
    if work and not os.path.isdir(work):
        return f"The working folder does not exist: {work}", None
    no_window = subprocess.CREATE_NO_WINDOW
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
            log_command(cmd)
            if admin:
                return _as_admin("cmd.exe", f'/c {cmd}', work or os.path.expanduser("~")), None
            proc = subprocess.Popen(cmd, shell=True, cwd=work or os.path.expanduser("~"), creationflags=no_window)
            return "", proc
        ext = os.path.splitext(path)[1].lower()
        cwd = work or os.path.dirname(path)
        if ext == ".pyw":
            cmd = [python_exe(opts.get("python_path", ""), windowless=True), path] + argv
            log_command(subprocess.list2cmdline(cmd))
            if admin:
                return _as_admin(cmd[0], subprocess.list2cmdline(cmd[1:]), cwd), None
            return "", subprocess.Popen(cmd, cwd=cwd, close_fds=True)
        if ext in CONSOLE_EXT:
            if ext == ".py":
                cmd = [python_exe(opts.get("python_path", "")), path] + argv
            elif ext == ".ps1":
                cmd = ["powershell.exe", "-NoLogo", "-ExecutionPolicy", "Bypass", "-File", path] + argv
            else:
                cmd = [path] + argv
            log_command(subprocess.list2cmdline(cmd))
            if admin:
                keep = ["powershell.exe", "-NoLogo", "-NoExit", "-Command",
                        "& " + " ".join("'" + a.replace("'", "''") + "'" for a in cmd)]
                return _as_admin(keep[0], subprocess.list2cmdline(keep[1:]), cwd), None
            return _console_win(cmd, cwd, title, opts)
        if ext == ".exe":
            log_command(subprocess.list2cmdline([path] + argv))
            if admin:
                return _as_admin(path, subprocess.list2cmdline(argv), cwd), None
            return "", subprocess.Popen([path] + argv, cwd=cwd, close_fds=True)
        log_command(f"open {path}")
        if admin:
            return _as_admin(path, args, cwd), None
        if argv:
            subprocess.Popen(f'start "" "{path}" {args}', shell=True, cwd=cwd, creationflags=no_window)
            return "", None
        return open_path(path), None
    except Exception as e:
        log.warning("could not start %s: %s", target, e)
        return f"{type(e).__name__}: {e}", None


def _console_win(cmd: list[str], cwd: str, title: str, opts: dict):
    if opts.get("keep_open", True):
        if cmd[0].lower().endswith("powershell.exe"):
            cmd = cmd[:1] + ["-NoExit"] + cmd[1:]
        else:
            # PowerShell rather than cmd /k: cmd strips the outer quotes of "C:\a b\python.exe" "x.py"
            # and every layer (Popen → wt → the tab) re-quotes differently. Single quotes survive all of them.
            call = "& " + " ".join("'" + a.replace("'", "''") + "'" for a in cmd)
            cmd = ["powershell.exe", "-NoLogo", "-NoExit", "-Command", call]
    if opts.get("use_terminal", True) and terminal.wt_exe():
        subprocess.Popen(terminal.command(title, cwd, cmd, opts.get("terminal", "")), close_fds=True,
                         creationflags=subprocess.CREATE_NO_WINDOW)
        return "", None                       # the tab belongs to Terminal: nothing of ours to track
    return "", subprocess.Popen(cmd, cwd=cwd, creationflags=subprocess.CREATE_NEW_CONSOLE, close_fds=True)


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
    if IS_WIN:
        r = subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True,
                           creationflags=subprocess.CREATE_NO_WINDOW)
        return r.returncode == 0
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
    if IS_WIN:
        cmd = ["powershell.exe", "-NoLogo", "-NoExit"] + (["-Command", command] if command else [])
        try:
            if terminal.wt_exe():
                subprocess.Popen(terminal.command("PowerShell", cwd, cmd, configured), close_fds=True,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
            else:
                subprocess.Popen(cmd, cwd=cwd or None, creationflags=subprocess.CREATE_NEW_CONSOLE)
            return ""
        except OSError as e:
            return str(e)
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
