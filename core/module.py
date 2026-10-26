"""Base class every module in modules/ derives from."""
import logging

from PyQt6.QtCore import QObject


class ModuleContext:
    """What the launcher hands to every module."""
    def __init__(self, app, settings, hotkeys, notify, toast, open_settings, launcher=None):
        self.app = app
        self.settings = settings
        self.hotkeys = hotkeys
        self._notify = notify
        self._toast = toast
        self._open_settings = open_settings
        self.launcher = launcher        # for themes, the dashboard and the overlay; may be None in tests

    def notify(self, title: str, message: str, ms: int = 4000):
        """Tray balloon message (respects the 'tray_messages' setting)."""
        self._notify(title, message, ms)

    def toast(self, title: str, message: str, ms: int = 8000):
        """Tray balloon message that always shows (used by reminders)."""
        self._toast(title, message, ms)

    def open_settings(self, section: str = ""):
        self._open_settings(section)

    def shows_on_start(self, key: str) -> bool:
        """Settings → General → Show on start has this module's panel ticked."""
        return key in (self.settings.get("general.start_panels") or [])


class Module(QObject):
    key = ""          # settings section + hotkey owner name
    title = ""
    icon = ""

    def __init__(self, ctx: ModuleContext):
        super().__init__()
        self.ctx = ctx
        self.settings = ctx.settings
        self.hotkeys = ctx.hotkeys
        self.log = logging.getLogger(self.key or self.__class__.__name__)
        self.running = False

    # ── settings helpers ───────────────────────────────────────────────────
    def s(self, key: str, default=None):
        return self.settings.get(f"{self.key}.{key}", default)

    def hotkey(self, key: str, callback, label: str = "") -> str:
        """Register a hotkey from this module's settings section; returns error text or ''."""
        combo = self.s(key, "")
        err = self.hotkeys.register(self.key, combo, callback, label or key)
        if err and combo:
            self.ctx.notify(self.title, f"Hotkey {combo!r} for {label or key} not registered: {err}")
        return err

    # ── lifecycle (override) ───────────────────────────────────────────────
    def start(self):
        """Create windows, register hotkeys."""

    def stop(self):
        """Hide/destroy windows. Hotkeys are unregistered by the launcher."""

    def apply_settings(self):
        """Settings were saved. Default: full restart of the module."""
        self.hotkeys.unregister_all(self.key)
        self.stop()
        self.start()

    def retheme(self):
        """The theme changed. Rebuild whatever bakes colours in when it is created.

        Default: nothing (windows built on demand pick the new palette up next time they open).
        """

    def tray_actions(self) -> list:
        """[(label, callable), ...] shown in the tray menu while running."""
        return []

    def themed(self):
        """`with self.themed(): panel = MyPanel(self)` builds a window in this module's accent, if the theme
        gives it one (Themes → Settings… → Module accents)."""
        from core import theme
        return theme.module_colours(self.key)

    def actions(self) -> list:
        """[(id, label, callable), ...] for the Dashboard's Tools tab and the command palette."""
        out = []
        for label, fn in self.tray_actions():
            aid = "".join(c if c.isalnum() else "_" for c in label.lower()).strip("_")
            out.append((aid, label, fn))
        return out

    # ── used by the launcher ───────────────────────────────────────────────
    def _start(self):
        if self.running:
            return
        self.running = True
        try:
            self.start()
        except Exception:
            self.running = False
            self.hotkeys.unregister_all(self.key)
            raise

    def _stop(self):
        if not self.running:
            return
        self.running = False
        self.hotkeys.unregister_all(self.key)
        try:
            self.stop()
        except Exception:
            self.log.exception("stop() failed")
