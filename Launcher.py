"""
Luminescent launcher (formerly UsefulScripts / AllinOne).

Run this (./start.sh, or .venv/bin/python Launcher.py) inside a Hyprland session.
Everything (the overlay with its Dashboard, Themes, Clipboard and Settings, plus
notes, doodle, timers, calculator, vault, reminders, hearing guard...) runs inside
this ONE process; its hotkeys are Hyprland binds (core.hotkeys), and settings.json
drives it all.

    python Launcher.py --smoke     build every page and pop-up once, then quit (used to test a build)
    python Launcher.py --strip-metadata <files or folders>   clean copies into the Metadata save folder
"""
import os
import sys
import time
import zlib
import shutil
import logging
import subprocess
import traceback
from logging.handlers import RotatingFileHandler

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QMessageBox
from PyQt6.QtCore import QTimer, QLockFile, QDir
from PyQt6.QtGui import QIcon, QPixmap, QAction, QFont

from core import (APP_DIR, APP_NAME, DATA_DIR, HOME_DIR, FROZEN, IS_WIN, VERSION, theme, widgets, autostart, terminal,
                  wallpaper, hypr, runner, desktop, usage, backup, events)
from core.settings import Settings, SCHEMA, ALL_PAGES
from core.hotkeys import HotkeyManager, pretty
from core.module import ModuleContext
from core.themes import ThemeStore
import modules as module_registry

LOG_FILE = os.path.join(DATA_DIR, "launcher.log")
ICON_PNG = os.path.join(DATA_DIR, "luminescent.png")
log = logging.getLogger("launcher")


def setup_logging(to_file: bool):
    root = logging.getLogger()
    root.setLevel(logging.INFO)
    fmt = logging.Formatter("%(asctime)s %(levelname)-7s %(name)s: %(message)s", "%H:%M:%S")
    if sys.stdout and sys.stdout.isatty():
        h = logging.StreamHandler(sys.stdout)
        h.setFormatter(fmt)
        root.addHandler(h)
    if to_file:
        fh = RotatingFileHandler(LOG_FILE, maxBytes=512_000, backupCount=2, encoding="utf-8")
        fh.setFormatter(fmt)
        root.addHandler(fh)


def star_pixmap(size: int, light: bool, accent: str = "") -> QPixmap:
    """The Luminescent mark (a four-pointed sparkle) in the theme's accent on a transparent background."""
    return theme.sparkle_pixmap(size, accent or ("#F2B705" if light else "#7B6CF6"), pad=0.04)


def make_icon(light: bool, accent: str = "") -> QIcon:
    icon = QIcon()
    for size in (16, 20, 24, 32, 48, 64, 128, 256):
        icon.addPixmap(star_pixmap(size, light, accent))
    return icon


class Launcher:
    def __init__(self, app: QApplication):
        self.app = app
        self.settings = Settings()
        setup_logging(bool(self.settings.get("general.log_to_file", True)))
        log.info("=== starting %s (pid %s, %s, data in %s) ===", VERSION, os.getpid(), "exe" if FROZEN else "source",
                 HOME_DIR)
        if self.settings.migrated_from_old_config:
            log.info("migrated values from the old config.py into settings.json")

        theme.load_fonts()
        self.themes = ThemeStore(self.settings)
        self.themes.use_active()
        self._restyle()
        app.setQuitOnLastWindowClosed(False)

        from ui.toast import ToastManager
        from core.updates import UpdateChecker
        self.toasts = ToastManager(self.settings)
        self.updates = UpdateChecker()
        self.updates.found.connect(self._update_found)
        self.hotkeys = HotkeyManager()
        self.hotkeys.pass_keys = bool(self.settings.get("general.pass_keys", True))
        terminal.set_preferred(self.settings.get("dashboard.terminal", ""))
        self.icon = make_icon(theme.is_light(), theme.P.primary)
        app.setWindowIcon(self.icon)
        self.tray = QSystemTrayIcon(self.icon, app)
        self.tray.setToolTip(APP_NAME)
        self.tray.activated.connect(self._tray_activated)
        self.tray.show()

        self.ctx = ModuleContext(app, self.settings, self.hotkeys, self.notify, self.toast, self.open_settings, self)
        self.modules = {}
        classes, self.module_errors = module_registry.load_all()
        for key, cls in classes.items():
            try:
                self.modules[key] = cls(self.ctx)
            except Exception as e:
                log.exception("could not create module %s", key)
                self.module_errors[key] = f"{type(e).__name__}: {e}"

        from ui.dashboard import DashboardStore
        from ui.overlay import MainOverlay
        from ui.cheatsheet import CheatSheet
        from core.automation import AutomationEngine
        self.dashboard = DashboardStore(self)
        self.automation = AutomationEngine(self)
        self.overlay = MainOverlay(self)
        self.cheatsheet = CheatSheet(self)
        self._palette = None

        self._enabled_snapshot = {k: self._enabled(k) for k in self.modules}
        self.settings.changed.connect(self.apply_settings)
        self.themes.applied.connect(self._theme_applied)

        self._register_global_hotkeys()
        for key in self.modules:
            if self._enabled(key):
                self._start_module(key)
        self.dashboard.register_hotkeys()
        self.automation.start()
        self.dashboard.changed.connect(self._build_menu)
        self.automation.changed.connect(self._build_menu)
        self._build_menu()
        self._sync_autostart()
        self._sync_desktop_entry()
        self._write_terminal_scheme()
        QTimer.singleShot(20_000, self._auto_backup)
        QTimer.singleShot(30_000, self._check_updates)
        self._backup_timer = QTimer()
        self._backup_timer.setInterval(6 * 3600 * 1000)
        self._backup_timer.timeout.connect(self._auto_backup)
        self._backup_timer.start()

        self._tip_timer = QTimer()
        self._tip_timer.setInterval(1000)
        self._tip_timer.timeout.connect(self._update_tooltip)
        timer = self.modules.get("timer")
        if timer is not None and hasattr(timer, "activity"):
            timer.activity.connect(self._timers_changed)
            self._timers_changed()

        app.aboutToQuit.connect(self.shutdown)
        QTimer.singleShot(3000, self.overlay.prewarm)     # first open of Settings etc. without the wait
        if self.settings.migrated_from_old_config:
            QTimer.singleShot(1500, lambda: self.toast(
                APP_NAME, "Your old config.py settings were imported. Press "
                          f"{pretty(self.settings.get('ui.overlay_hotkey', ''))} → Settings to edit them."))
        if self.module_errors:
            QTimer.singleShot(2500, lambda: self.toast(
                "Some modules failed to load", "\n".join(f"{k}: {v}" for k, v in self.module_errors.items()), 10000))
        QTimer.singleShot(1500, lambda: events.emit("luminescent.started", VERSION))
        log.info("ready: %s", ", ".join(k for k in self.modules if self.modules[k].running) or "no modules running")

    # ── look ───────────────────────────────────────────────────────────────
    def _restyle(self):
        self.app.setStyleSheet(theme.stylesheet())
        f = QFont(theme.P.font_family)
        f.setPixelSize(13)              # the global QSS must not set font-size, or setFont() breaks
        self.app.setFont(f)

    def _theme_applied(self, t: dict):
        events.emit("theme.changed", t.get("name", ""))
        self._restyle()
        self.icon = make_icon(theme.is_light(), theme.P.primary)
        self.app.setWindowIcon(self.icon)
        self.tray.setIcon(self.icon)
        self._write_terminal_scheme()
        for key, m in self.modules.items():
            if m.running:
                try:
                    m.retheme()
                except Exception:
                    log.exception("retheme failed for %s", key)
        QTimer.singleShot(0, self.overlay.rebuild)
        if self._palette is not None:
            self._palette.close()
            self._palette.deleteLater()
            self._palette = None
        if t.get("wallpaper") and not self.settings.get("ui.performance_mode", False):
            wallpaper.apply(t["wallpaper"], self.settings.get("ui.wallpaper_engine_exe", ""),
                            on_error=lambda msg: log.warning("wallpaper: %s", msg))
        # a theme picks its settings profile; none picked = the default profile
        if self.settings.active_profile() != (t.get("profile") or "default"):
            self.settings.switch_profile(t.get("profile") or "default")
            self.notify(APP_NAME, f"Settings profile: {self.settings.profile_name(self.settings.active_profile())}", 2500)

    def _sync_own_terminal(self, asked: bool = False):
        """Settings → Overlay → "Your own terminal follows the theme": kitty.conf on Linux, a Windows Terminal
        fragment on Windows. Linked (after asking) or unlinked here."""
        if os.environ.get("LUMINESCENT_HOME"):
            return
        want = bool(self.settings.get("ui.own_terminal", False))
        linked = terminal.own_linked()
        if want and not linked and asked:
            what = (f"Luminescent will give your PowerShell and Command Prompt profiles in Windows Terminal its "
                    f"colours (through its own fragment file; your Terminal settings file is not edited)."
                    if IS_WIN else
                    "Luminescent will add one line to ~/.config/kitty/kitty.conf:\n\n"
                    "    include luminescent-theme.conf\n\n"
                    "and keep luminescent-theme.conf in the theme's colours. A copy of your kitty.conf "
                    "goes to data/terminal-backups first.")
            if not terminal.own_installed():
                self.notify(APP_NAME, f"{terminal.OWN_NAME} is not installed, so there is nothing to colour.")
                want = False
            elif widgets.confirm(None, f"Colour your {terminal.OWN_NAME} windows", what + " Go ahead?", yes="Do it"):
                err = terminal.link_own(os.path.join(DATA_DIR, "terminal-backups"))
                if err:
                    self.notify(APP_NAME, f"Could not change {terminal.OWN_NAME}: {err}")
                    want = False
            else:
                want = False
            if not want:
                self.settings.set("ui.own_terminal", False)
                self.settings.save_quiet()
        elif want and linked:
            terminal.write_own()
        elif not want and linked and asked:
            err = terminal.unlink_own()
            if err:
                self.notify(APP_NAME, f"Could not change {terminal.OWN_NAME}: {err}")

    def _write_terminal_scheme(self):
        if self.settings.get("ui.own_terminal", False):
            self._sync_own_terminal()
        if not self.settings.get("ui.terminal_scheme", True):
            return
        try:
            star_pixmap(256, theme.is_light(), theme.P.primary).save(ICON_PNG, "PNG")
        except Exception:
            pass
        terminal.write_fragment(ICON_PNG)

    def apply_theme(self, tid: str):
        self.themes.apply(tid)

    # ── modules ────────────────────────────────────────────────────────────
    def _enabled(self, key: str) -> bool:
        return bool(self.settings.get(f"{key}.enabled", True))

    def _start_module(self, key: str):
        m = self.modules[key]
        try:
            m._start()
            log.info("started %s", key)
        except Exception as e:
            log.exception("module %s failed to start", key)
            self.module_errors[key] = f"{type(e).__name__}: {e}"
            self.notify(m.title, f"Failed to start: {e}")

    def _stop_module(self, key: str):
        self.modules[key]._stop()
        log.info("stopped %s", key)

    def toggle_module(self, key: str, on: bool):
        self.settings.set(f"{key}.enabled", on)
        self.settings.save()          # triggers apply_settings

    def run_action(self, action: str):
        """Run "module:action_id" (Dashboard Tools tab, command palette)."""
        usage.record("tools", action)
        key, _, aid = action.partition(":")
        m = self.modules.get(key)
        if m is None:
            self.notify(APP_NAME, f"{key} is not available ({self.module_errors.get(key, 'not loaded')}).")
            return
        if not m.running:
            self.notify(m.title, f"{m.title} is turned off. Turn it on in Settings → {m.title}.")
            return
        for a_id, label, fn in m.actions():
            if a_id == aid:
                usage.record("tools", action, f"{m.title}: {label}")
                usage.activity("tool", label)
                fn()
                return
        self.notify(m.title, f"{m.title} has no action '{aid}' any more.")

    def apply_settings(self):
        self.hotkeys.unregister_all("launcher")
        self._register_global_hotkeys()
        for key, m in self.modules.items():
            want = self._enabled(key)
            had = self._enabled_snapshot.get(key, False)
            try:
                if want and not had:
                    self._start_module(key)
                elif had and not want:
                    self._stop_module(key)
                elif want and m.running:
                    m.apply_settings()
            except Exception as e:
                log.exception("apply_settings failed for %s", key)
                self.notify(m.title, f"Could not apply settings: {e}")
            self._enabled_snapshot[key] = want
        self.dashboard.register_hotkeys()
        self.automation.apply_settings()
        self.overlay.settings_applied()
        self._build_menu()
        self._sync_autostart()
        self._sync_desktop_entry()
        terminal.set_preferred(self.settings.get("dashboard.terminal", ""))
        self._sync_own_terminal(asked=True)
        if self.settings.get("ui.terminal_scheme", True):
            self._write_terminal_scheme()
        else:
            terminal.remove_fragment()
        self._timers_changed()
        log.info("settings applied")

    def _sync_autostart(self):
        if os.environ.get("LUMINESCENT_HOME"):
            return                    # a test copy must never repoint the real startup entry
        err = autostart.sync(bool(self.settings.get("general.run_at_startup", False)))
        if err:
            self.notify(APP_NAME, f"Could not update the startup entry: {err}")

    def _sync_desktop_entry(self):
        if os.environ.get("LUMINESCENT_HOME"):
            return
        if self.settings.get("general.desktop_entry", True):
            desktop.install_entry()
        else:
            desktop.remove_entry()
        if self.settings.get("metadata.dolphin_menu", False) and self._enabled("metadata"):
            err = desktop.install_service_menu()
            if err:
                self.notify(APP_NAME, f"Could not add the Dolphin menu: {err}")
        else:
            desktop.remove_service_menu()

    def _auto_backup(self):
        """Once a week: zip the setup into data/backups/weekly-<date>.zip and keep the newest five."""
        if not self.settings.get("general.auto_backup", True) or os.environ.get("LUMINESCENT_HOME"):
            return
        import glob
        from datetime import datetime as _dt
        os.makedirs(backup.BACKUP_DIR, exist_ok=True)
        weekly = sorted(glob.glob(os.path.join(backup.BACKUP_DIR, "weekly-*.zip")))
        if weekly and time.time() - os.path.getmtime(weekly[-1]) < 7 * 86400:
            return
        path = os.path.join(backup.BACKUP_DIR, f"weekly-{_dt.now():%Y-%m-%d}.zip")
        try:
            n = backup.export(path)
            log.info("weekly backup: %d files -> %s", n, path)
        except Exception:
            log.exception("weekly backup failed")
            return
        for old in sorted(glob.glob(os.path.join(backup.BACKUP_DIR, "weekly-*.zip")))[:-5]:
            try:
                os.remove(old)
            except OSError:
                pass

    def _check_updates(self, force: bool = False):
        if force or (self.settings.get("general.check_updates", True) and not os.environ.get("LUMINESCENT_HOME")):
            self.updates.check(force)

    def _update_found(self, tag: str, url: str):
        if not tag:
            self.notify(APP_NAME, f"You have the latest version ({VERSION}).", 3000)
            return
        log.info("update available: %s", tag)
        self._update_url = url
        how = ("download it from the release page" if FROZEN or not os.path.isdir(os.path.join(APP_DIR, ".git"))
               else "run git pull in the Luminescent folder")
        self.toast(f"{APP_NAME} {tag} is out", f"You have {VERSION}. Tray → Update available, or {how}. "
                                               "Your notes and settings are kept.")
        self._build_menu()

    def open_update_page(self):
        import webbrowser
        from core.updates import PAGE
        webbrowser.open(getattr(self, "_update_url", "") or PAGE)

    def next_theme(self):
        t = self.themes.next_theme()
        if t is not None:
            self.apply_theme(t["id"])
            self.notify(APP_NAME, f"Theme: {t['name']}", 1500)

    def _register_global_hotkeys(self):
        s = self.settings
        binds = [
            (s.get("general.force_quit_hotkey", ""), self.confirm_quit, "quit all"),
            (s.get("general.settings_hotkey", ""), lambda: self.open_settings(), "settings"),
            (s.get("ui.overlay_hotkey", ""), self.overlay.toggle, "overlay"),
            (s.get("ui.palette_hotkey", ""), self.open_palette, "command palette"),
            (s.get("ui.cheatsheet_hotkey", ""), lambda: self.cheatsheet.trigger(s.get("ui.cheatsheet_hotkey", "")),
             "cheat sheet"),
            (s.get("ui.theme_cycle_hotkey", ""), self.next_theme, "next theme"),
        ]
        from core.settings import NAV_SECTIONS
        for key, _name in NAV_SECTIONS:
            combo = s.get("ui.hotkey_settings_page" if key == "settings" else f"ui.hotkey_{key}", "")
            binds.append((combo, lambda k=key: self.overlay.open(k), f"overlay {key}"))
        for combo, fn, label in binds:
            err = self.hotkeys.register("launcher", combo, fn, label)
            if err and combo:
                self.notify(APP_NAME, f"{label.capitalize()} hotkey not registered: {err}")

    # ── overlay, palette ───────────────────────────────────────────────────
    def open_settings(self, section: str = ""):
        self.overlay.open("settings", section if isinstance(section, str) else "")

    def open_palette(self, text: str = ""):
        if self._palette is None:
            from ui.palette import CommandPalette
            self._palette = CommandPalette(self)
        self._palette.toggle(text if isinstance(text, str) else "")

    def palette_entries(self) -> list:
        from ui.palette import Entry
        out = []
        names = dict(ALL_PAGES)
        for key in self.overlay.nav_order():
            out.append(Entry(f"page:{key}", f"Open {names[key]}", lambda k=key: self.overlay.open(k), "Luminescent", "page"))
        for key, m in self.modules.items():
            if not m.running:
                continue
            for aid, label, fn in m.actions():
                out.append(Entry(f"act:{key}:{aid}", label, fn, m.title, "action"))
            extra = getattr(m, "palette_entries", None)
            if callable(extra):
                try:
                    out += extra(Entry)
                except Exception:
                    log.exception("palette entries from %s failed", key)
        out += self.automation.palette_entries(Entry)
        for board, t in self.dashboard.all_tabs():
            for it in t["items"]:
                if it.get("action") or it.get("kind") in ("separator", "heading"):
                    continue              # the module's own action is already listed
                where = t["title"] if len(self.dashboard.boards()) < 2 else f"{board['name']} › {t['title']}"
                out.append(Entry(f"item:{it['id']}", it["name"], lambda i=it["id"]: self.dashboard.run_item(i),
                                 where + (f" · {pretty(it['hotkey'])}" if it.get("hotkey") else ""), "item"))
        for th in self.themes.themes():
            out.append(Entry(f"theme:{th['id']}", f"Theme: {th['name']}", lambda i=th["id"]: self.apply_theme(i),
                             "active" if th["id"] == self.themes.data["active"] else "theme", "theme"))
        for s in SCHEMA:
            out.append(Entry(f"settings:{s.key}", f"Settings › {s.title}", lambda k=s.key: self.open_settings(k),
                             "settings", "settings"))
        for label, fn in (("Next theme", self.next_theme), ("Reset stuck keys", self.reset_keys),
                          ("Check for updates", lambda: self._check_updates(force=True)),
                          ("Show the hotkey cheat sheet", self._cheat_toggle),
                          ("Open the log file", self.open_log), ("Restart Luminescent", self.restart),
                          ("Quit Luminescent…", self.confirm_quit)):
            out.append(Entry(f"lum:{label}", label, fn, APP_NAME, "luminescent"))
        return out

    def _cheat_toggle(self):
        was = self.settings.get("ui.cheatsheet_toggle", False)
        self.settings.set("ui.cheatsheet_toggle", True)   # from the palette there is no key to hold
        try:
            self.cheatsheet.trigger("")
        finally:
            self.settings.set("ui.cheatsheet_toggle", was)

    # ── tray ───────────────────────────────────────────────────────────────
    def _build_menu(self):
        shown = set(self.settings.get("general.tray_items") or [])
        menu = QMenu()
        head = menu.addAction(f"{APP_NAME} {VERSION}")
        head.setEnabled(False)
        if self.updates.latest:
            menu.addAction(f"⬆  Update available: {self.updates.latest}", self.open_update_page)
        if "open" in shown:
            menu.addAction(f"✦  Open {APP_NAME}   ({pretty(self.settings.get('ui.overlay_hotkey', ''))})",
                           lambda: self.overlay.open("dashboard"))
        if "palette" in shown:
            menu.addAction(f"⌕  Command palette   ({pretty(self.settings.get('ui.palette_hotkey', ''))})",
                           self.open_palette)
        menu.addSeparator()

        if self.settings.get("dashboard.show_in_tray", True) and "dashboard" in shown:
            dash = menu.addMenu("▦  Dashboard")
            several = len(self.dashboard.boards()) > 1
            for board in self.dashboard.boards():
                host = dash.addMenu(board["name"]) if several else dash
                for t in board["tabs"]:
                    sub = host.addMenu(t["title"])
                    runnable = [it for it in t["items"] if it.get("kind", "item") not in ("separator", "heading")]
                    for it in runnable:
                        label = it["name"] + (f"   ({pretty(it['hotkey'])})" if it.get("hotkey") else "")
                        sub.addAction(label, lambda i=it["id"]: self.dashboard.run_item(i))
                    if not runnable:
                        a = sub.addAction("(empty)")
                        a.setEnabled(False)
            dash.addSeparator()
            dash.addAction("Edit the dashboard…", lambda: self.overlay.open("dashboard"))

        if "automation" in shown:
            self.automation.tray_menu(menu)
        if "modules" in shown:
            self._modules_menu(menu)
        for key, m in self.modules.items():
            if "actions" in shown:
                actions = m.tray_actions() if m.running else []
                for label, fn in actions:
                    menu.addAction(f"{m.icon}  {label}", fn)
            extra = getattr(m, "tray_menu", None)
            if m.running and callable(extra) and "colours" in shown:
                extra(menu)
        menu.addSeparator()
        if "reset" in shown:
            menu.addAction("⌨  Reset stuck keys", self.reset_keys)
        if "settings" in shown or not shown:
            menu.addAction("⚙  Settings…", lambda: self.open_settings())
        if "log" in shown:
            menu.addAction("📄  Open log file", self.open_log)
        if "restart" in shown:
            menu.addAction("🔄  Restart", self.restart)
        if "quit" in shown or not shown:
            menu.addSeparator()
            menu.addAction(f"⏻  Quit everything  ({pretty(self.settings.get('general.force_quit_hotkey', ''))})",
                           self.confirm_quit)
        self._menu = menu            # keep a reference or Qt garbage-collects it
        self.tray.setContextMenu(menu)

    def _modules_menu(self, menu: QMenu):
        mods = menu.addMenu("⚙  Modules")
        for key, m in self.modules.items():
            sub = mods.addMenu(f"{m.icon}  {m.title}")
            toggle = QAction("Enabled", sub, checkable=True)
            toggle.setChecked(m.running)
            toggle.toggled.connect(lambda on, k=key: self.toggle_module(k, on))
            sub.addAction(toggle)
            sec = self.settings.section(key)
            hks = [(f.label, sec.get(f.key)) for s in SCHEMA if s.key == key for f in s.fields
                   if f.type == "hotkey" and sec.get(f.key)]
            if hks:
                sub.addSeparator()
                for label, combo in hks:
                    a = sub.addAction(f"{label}:  {pretty(combo)}")
                    a.setEnabled(False)
            sub.addSeparator()
            sub.addAction("Settings…", lambda k=key: self.open_settings(k))
        for key, err in self.module_errors.items():
            if key not in self.modules:
                a = mods.addAction(f"⚠ {key}: failed to load")
                a.setToolTip(err)
                a.setEnabled(False)

    def _tray_activated(self, reason):
        # a double-click also delivers a Trigger first, so reacting to both would open and close it again
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.overlay.toggle()

    def _timers_changed(self):
        timer = self.modules.get("timer")
        running = bool(timer and timer.running and self.settings.get("ui.tray_timers", True) and timer.any_running())
        if running and not self._tip_timer.isActive():
            self._tip_timer.start()
        elif not running:
            self._tip_timer.stop()
        self._update_tooltip()

    def _update_tooltip(self):
        timer = self.modules.get("timer")
        lines = [APP_NAME]
        if timer and timer.running and self.settings.get("ui.tray_timers", True):
            lines += timer.tooltip_lines()
        text = "\n".join(lines)
        if len(text) > 127:                      # keep it short enough for every tray host
            text = text[:126] + "…"
        self.tray.setToolTip(text)

    def notify(self, title: str, message: str, ms: int = 4000):
        log.info("notify: %s - %s", title, message)
        if not self.settings.get("general.tray_messages", True):
            return
        if self.settings.get("general.notify_style", "luminescent") == "luminescent":
            try:
                self.toasts.show(title, message, ms)
                return
            except Exception:
                log.exception("toast failed")
        self._desktop_notify(title, message, ms)

    def toast(self, title: str, message: str, ms: int = 8000):
        self._desktop_notify(title, message, ms)

    def _desktop_notify(self, title: str, message: str, ms: int):
        """A desktop notification (notify-send talks to Noctalia, mako, dunst...); the tray as a fallback."""
        exe = shutil.which("notify-send")
        if exe:
            cmd = [exe, "-a", APP_NAME, "-t", str(int(ms)), title, message]
            if os.path.isfile(ICON_PNG):
                cmd[3:3] = ["-i", ICON_PNG]
            try:
                subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                return
            except OSError:
                pass
        self.tray.showMessage(title, message, QSystemTrayIcon.MessageIcon.Information, ms)

    def open_log(self):
        if os.path.exists(LOG_FILE):
            runner.open_path(LOG_FILE)

    def reset_keys(self):
        """Forget held keys the hook never saw released and stop drawing (if Alt+R etc. misfire)."""
        self.hotkeys.reset_keys()
        doodle = self.modules.get("doodle")
        if doodle is not None and doodle.running:
            doodle.deactivate()
        self.notify(APP_NAME, "Keyboard state reset.", 2000)

    def preview_reminder(self, rem: dict):
        m = self.modules.get("reminders")
        if m is None:
            return
        m.fire(rem, preview=True)

    # ── lifecycle ──────────────────────────────────────────────────────────
    def confirm_quit(self):
        if widgets.confirm(None, f"Quit {APP_NAME}", "Close Luminescent and every module?", yes="Quit"):
            self.app.quit()

    def restart(self):
        log.info("restarting")
        self.shutdown()
        if FROZEN:
            subprocess.Popen([sys.executable], cwd=os.path.dirname(sys.executable), close_fds=True,
                             start_new_session=True)
        else:
            subprocess.Popen([sys.executable, os.path.abspath(__file__)], cwd=APP_DIR, close_fds=True,
                             start_new_session=True)
        os._exit(0)

    def shutdown(self):
        if getattr(self, "_shut", False):
            return
        self._shut = True
        log.info("shutting down")
        usage.flush()
        self.toasts.clear()
        self.overlay.shutdown()             # first: dashboard page boxes write where they were
        self.dashboard.save_now()
        self.automation.shutdown()
        for key in list(self.modules):
            try:
                self.modules[key]._stop()
            except Exception:
                log.exception("error stopping %s", key)
        self.hotkeys.shutdown()
        self.tray.hide()


def _smoke(launcher: "Launcher", app: QApplication):
    """Build every overlay page and pop-up once, then quit with 0 (1 on any error)."""
    errors = []
    sys.excepthook = lambda t, e, tb: errors.append("".join(traceback.format_exception(t, e, tb)))
    steps = [lambda k=k: launcher.overlay.open(k) for k, _ in ALL_PAGES]
    steps += [launcher.open_palette, launcher.open_palette, launcher.overlay.hide,
              lambda: launcher.apply_theme("light"), lambda: launcher.apply_theme("dark")]

    def run(i=0):
        if i < len(steps):
            try:
                steps[i]()
            except Exception:
                errors.append(traceback.format_exc())
            QTimer.singleShot(250, lambda: run(i + 1))
            return
        from PyQt6.QtWidgets import QLabel
        errors.extend(f"module {k} failed: {v}" for k, v in launcher.module_errors.items())
        for k, _ in ALL_PAGES:
            page = launcher.overlay.page(k)
            if page is not None and any("could not be built" in lbl.text() for lbl in page.findChildren(QLabel)):
                errors.append(f"the {k} page could not be built (see the log)")
        log.info("SMOKE %s", "OK" if not errors else "FAILED\n" + "\n".join(errors))
        print("SMOKE", "OK" if not errors else "FAILED\n" + "\n".join(errors), flush=True)
        app.exit(1 if errors else 0)
    QTimer.singleShot(500, run)


def main():
    if "--strip-metadata" in sys.argv:          # Dolphin's right-click menu: no window, no lock needed
        from modules.metadata import cli
        i = sys.argv.index("--strip-metadata")
        return cli(sys.argv[i + 1:])
    # Native Wayland: Qt then knows the real screen layout, and core.hypr places the windows.
    # (Under XWayland Hyprland maps multi-monitor coordinates inconsistently.)
    if os.environ.get("WAYLAND_DISPLAY"):
        os.environ.setdefault("QT_QPA_PLATFORM", "wayland")
    app = QApplication(sys.argv)
    app.setApplicationName(APP_NAME)
    desktop.apply_icon_theme()
    app.setDesktopFileName(hypr.APP_ID)        # the Wayland app id = the class Hyprland's window rules match
    smoke = "--smoke" in sys.argv

    # a build and the source share one lock (two sets of binds would fight); only a test copy
    # with its own LUMINESCENT_HOME may run next to the real one
    tag = f"-{zlib.crc32(HOME_DIR.encode()):08x}" if os.environ.get("LUMINESCENT_HOME") else ""
    lock = QLockFile(os.path.join(QDir.tempPath(), f"usefulscripts-launcher{tag}.lock"))
    lock.setStaleLockTime(0)
    if not lock.tryLock(3000):        # a restart hands over within a second or two
        if smoke:
            print("SMOKE skipped: already running", flush=True)
            return 2
        QMessageBox.information(None, APP_NAME, "Luminescent is already running (check the tray, or press "
                                                "Ctrl+Shift+Space).")
        return 0

    hypr.init(app)                  # only now: it replaces the binds of an earlier instance
    launcher = None
    last_balloon = [0.0]

    def excepthook(exc_type, exc, tb):
        # PyQt aborts the process on an unhandled exception in a slot unless a hook is installed.
        text = "".join(traceback.format_exception(exc_type, exc, tb))
        logging.getLogger("unhandled").error(text)
        # One balloon per 15 s at most: showing a toast from inside a mouse handler can
        # swallow the drag in progress, so repeated errors must not spam it.
        if launcher and time.monotonic() - last_balloon[0] > 15:
            last_balloon[0] = time.monotonic()
            launcher.notify(f"{APP_NAME} error", f"{exc_type.__name__}: {exc}\n(see data/launcher.log)", 8000)

    sys.excepthook = excepthook
    launcher = Launcher(app)
    if smoke:
        _smoke(launcher, app)
    code = app.exec()
    lock.unlock()
    return code


if __name__ == "__main__":
    sys.exit(main())
