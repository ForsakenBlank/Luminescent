"""
Keep awake: stops the screen going dark and the PC going to sleep while it is on.

Windows asks for it with SetThreadExecutionState. Linux holds a systemd-inhibit lock (idle + sleep) for
as long as it is on; hypridle, KDE and GNOME all respect it. Turned on from the tray, the command
palette, its hotkey (Settings → General) or the Automation "Keep awake" node.
"""
import shutil
import logging
import subprocess

from PyQt6.QtCore import QObject, pyqtSignal

from core import APP_NAME, IS_WIN

log = logging.getLogger("keepawake")


class KeepAwake(QObject):
    changed = pyqtSignal(bool)

    def __init__(self):
        super().__init__()
        self.on = False
        self.why = ""
        self._proc: subprocess.Popen | None = None

    def set(self, on: bool, why: str = "") -> str:
        """Turns it on or off. Returns an error text, or "" when it worked."""
        if on == self.on:
            self.why = why or self.why
            return ""
        err = self._start(why) if on else self._stop()
        if err:
            log.warning("keep awake: %s", err)
            return err
        self.on = on
        self.why = why if on else ""
        log.info("keep awake %s", "on" if on else "off")
        self.changed.emit(on)
        return ""

    def toggle(self, why: str = "") -> str:
        return self.set(not self.on, why)

    def _start(self, why: str) -> str:
        if IS_WIN:
            import ctypes
            # ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
            if not ctypes.windll.kernel32.SetThreadExecutionState(0x80000000 | 0x1 | 0x2):
                return "Windows said no"
            return ""
        exe = shutil.which("systemd-inhibit")
        if not exe:
            return "systemd-inhibit is not installed"
        try:
            self._proc = subprocess.Popen([exe, "--what=idle:sleep", f"--who={APP_NAME}",
                                           f"--why={why or 'Keep awake is on'}", "--mode=block", "sleep", "infinity"],
                                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as e:
            return str(e)
        return ""

    def _stop(self) -> str:
        if IS_WIN:
            import ctypes
            ctypes.windll.kernel32.SetThreadExecutionState(0x80000000)
            return ""
        if self._proc is not None:
            self._proc.terminate()
            try:
                self._proc.wait(2)
            except subprocess.TimeoutExpired:
                self._proc.kill()
            self._proc = None
        return ""

    def shutdown(self):
        if self.on:
            self._stop()
            self.on = False


_instance: KeepAwake | None = None


def get() -> KeepAwake:
    global _instance
    if _instance is None:
        _instance = KeepAwake()
    return _instance
