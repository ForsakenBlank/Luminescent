"""
Performance: a page of widgets for everything the computer is doing, every running app with its CPU, memory, GPU
and VRAM, and the settings that make it faster.

  * Overview: widgets for the processor, memory, each GPU, VRAM, disks, network, temperatures, battery, the apps
    using the most and the system. Add, remove, move and resize them; every one has its own settings (which
    numbers and graphs it shows, its colour, how much history, which GPU…).
  * Processes: every app and script with its icon, grouped by app, sortable, with ending it, its priority and
    which cores it may use.
  * Boost: power mode, CPU governor, boost, top speed, swappiness, NVIDIA power limit, desktop effects… and
    freeing memory and cleaning temporary files.
  * Game mode: one switch for the fastest settings, do not disturb, lower priority for background apps and the
    gaming HUD; it can start by itself when a game runs and puts everything back afterwards.
  * Alerts & rules: a notification when the CPU, memory, VRAM or a temperature stays too high or an app runs away,
    and priorities that are set for an app every time it starts.
  * Startup apps, and the gaming HUD: a small always-on-top readout in a corner of the screen.

The measuring (core/perfstats.py) runs on a worker thread, only while something shows it or alerts, rules or
automatic Game mode need it. The settings it changes are in core/perftweaks.py; the pages are in
ui/performance_ui.py.
"""
import os
import copy
import time
import threading
import logging
from collections import deque

from PyQt6.QtCore import QTimer, QThread, pyqtSignal

from core import DATA_DIR, IS_WIN, save_json, load_json, perfstats, perftweaks
from core import syscontrol as sc
from core.guard import alive
from core.module import Module

log = logging.getLogger("performance")

SAVE_PATH = os.path.join(DATA_DIR, "performance.json")
HISTORY = 900                                     # samples kept per number (15 minutes at one a second)

DEFAULT_WIDGETS = [
    {"type": "cpu", "span": 1}, {"type": "gpu", "span": 1, "source": "main"}, {"type": "memory", "span": 1},
    {"type": "apps", "span": 2, "sort": "cpu", "count": 8}, {"type": "vram", "span": 1, "source": "main"},
    {"type": "network", "span": 1}, {"type": "disk", "span": 1}, {"type": "temps", "span": 1},
]

DEFAULTS = {
    "widgets": None,                      # filled from DEFAULT_WIDGETS
    "columns": 3,
    "interval": 1.0,
    "cpu_mode": "total",                  # total: % of the whole CPU (Task Manager); core: % of one core (top)
    "group_apps": True,
    "proc_columns": ["pid", "cpu", "mem", "gpu", "vram", "disk"],
    "alerts": {
        "on": False,
        "cpu": {"on": True, "over": 90, "for": 30},
        "mem": {"on": True, "over": 90, "for": 20},
        "vram": {"on": True, "over": 95, "for": 20},
        "cpu_temp": {"on": True, "over": 90, "for": 10},
        "gpu_temp": {"on": True, "over": 85, "for": 10},
        "app_cpu": {"on": False, "over": 60, "for": 60},
        "disk_full": {"on": True, "over": 95},
        "battery": {"on": True, "under": 15},
    },
    "rules": [],                          # [{"match": "discord", "priority": "below", "cores": []}]
    "game": {"power": True, "effects": True, "dnd": True, "hud": True, "lower": [], "close": [], "auto": [],
             "free_memory": False},
    "hud": {"elements": ["cpu", "cpu_temp", "gpu", "gpu_temp", "vram", "ram"], "anchor": "tl", "opacity": 0.78,
            "click_through": True, "size": 11, "layout": "column"},
    "game_saved": None,                   # what Game mode changed, kept in case Luminescent stops while it is on
}


def _defaults() -> dict:
    d = copy.deepcopy(DEFAULTS)
    d["widgets"] = copy.deepcopy(DEFAULT_WIDGETS)
    return d


def _merge(base: dict, saved: dict) -> dict:
    for k, v in saved.items():
        if isinstance(v, dict) and isinstance(base.get(k), dict):
            _merge(base[k], v)
        else:
            base[k] = v
    return base


def _matches(proc: dict, names: list[str]) -> bool:
    """A process matches a name from a list when its name, program or script starts with it (case ignored)."""
    if not names:
        return False
    hay = [proc.get("name", "").lower(), proc.get("proc", "").lower(),
           os.path.basename(proc.get("exe") or "").lower()]
    for n in names:
        n = n.strip().lower()
        if n and any(h == n or h.startswith(n) or (n in h and len(n) >= 4) for h in hay if h):
            return True
    return False


# ══ the measuring thread ══════════════════════════════════════════════════════════
class _Worker(QThread):
    sampled = pyqtSignal(object)

    def __init__(self, m: "PerformanceModule"):
        super().__init__()
        self.m = m
        self.wake = threading.Event()
        self.quit_flag = False

    def run(self):
        sampler = perfstats.Sampler()
        try:
            while not self.quit_flag:
                want = self.m._want                  # (interval, processes) or None, set on the UI thread
                if want is None:
                    self.wake.wait(0.5)
                    self.wake.clear()
                    continue
                t = time.monotonic()
                try:
                    d = sampler.sample(procs=want[1])
                    self.sampled.emit(d)
                except Exception:
                    log.exception("measuring failed")
                rest = max(0.2, want[0] - (time.monotonic() - t))
                self.wake.wait(rest)
                self.wake.clear()
        finally:
            sampler.close()


# ══ the module ════════════════════════════════════════════════════════════════════
class PerformanceModule(Module):
    key = "performance"
    title = "Performance"
    icon = "📊"

    def __init__(self, ctx):
        super().__init__(ctx)
        self.data = self._load()
        self.views: list = []             # Performance pages (overlay page, panel)
        self.panel = None
        self.hud = None
        self.last: dict | None = None
        self.history: dict[str, deque] = {}
        self.listeners: list = []         # callables(sample) that want every sample
        self._want = None
        self._worker: _Worker | None = None
        self.game_on = False
        self.game_auto = False
        self._over_since: dict = {}       # alert key -> when it went over
        self._alerted: dict = {}          # alert key -> when it last notified
        self._ruled: set = set()          # (pid, rule) already applied
        self._lowered: dict = {}          # pid -> nice before Game mode lowered it
        self._wants_timer = QTimer(self)
        self._wants_timer.setInterval(1000)
        self._wants_timer.timeout.connect(self._refresh_wants)

    # ── storage ─────────────────────────────────────────────────────────────────
    def _load(self) -> dict:
        saved = load_json(SAVE_PATH, {})
        d = _merge(_defaults(), saved if isinstance(saved, dict) else {})
        if not isinstance(d.get("widgets"), list):
            d["widgets"] = copy.deepcopy(DEFAULT_WIDGETS)
        for i, w in enumerate(d["widgets"]):
            w.setdefault("id", f"w{i}_{int(time.time() * 1000) % 100000}")
        return d

    def save(self):
        try:
            save_json(SAVE_PATH, self.data)
        except OSError:
            log.exception("could not save %s", SAVE_PATH)

    def new_widget_id(self) -> str:
        ids = {w.get("id") for w in self.data["widgets"]}
        n = len(ids)
        while f"w{n}" in ids:
            n += 1
        return f"w{n}"

    # ── lifecycle ───────────────────────────────────────────────────────────────
    def start(self):
        self._hotkeys()
        if self.data.get("game_saved"):                 # it stopped while Game mode was on: put things back
            QTimer.singleShot(3000, self._restore_after_crash)
        self._worker = _Worker(self)
        self._worker.sampled.connect(self._got)
        self._worker.start()
        self._wants_timer.start()
        if self.ctx.shows_on_start("performance"):
            QTimer.singleShot(500, self.toggle)
        self._refresh_wants()

    def _hotkeys(self):
        self.hotkey("hotkey", self.toggle, "performance")
        self.hotkey("game_hotkey", self.toggle_game, "game mode")
        self.hotkey("hud_hotkey", self.toggle_hud, "gaming HUD")

    def stop(self):
        self._wants_timer.stop()
        if self.game_on:
            self.set_game(False, quiet=True)
        if self._worker is not None:
            self._worker.quit_flag = True
            self._worker.wake.set()
            self._worker.wait(3000)
            self._worker = None
        for w in (self.panel, self.hud):
            if w is not None and alive(w):
                w.close()
                w.deleteLater()
        self.panel = self.hud = None

    def apply_settings(self):
        self.hotkeys.unregister_all(self.key)
        self._hotkeys()
        if self.panel is not None and alive(self.panel):
            self.panel.set_opacity(float(self.s("bg_opacity", 0.95)))

    def retheme(self):
        if self.panel is not None and alive(self.panel):
            visible = self.panel.isVisible()
            self.panel.close()
            self.panel.deleteLater()
            self.panel = None
            if visible:
                self.toggle()
        if self.hud is not None and alive(self.hud):
            on = self.hud.isVisible()
            self.hud.close()
            self.hud.deleteLater()
            self.hud = None
            if on:
                self.toggle_hud()

    def actions(self):
        return [("toggle", "Performance (CPU, GPU, VRAM, processes, boost)", self.toggle),
                ("processes", "Performance: processes", lambda: self.open_page("processes")),
                ("game", "Game mode on / off", self.toggle_game),
                ("hud", "Gaming HUD on / off", self.toggle_hud),
                ("boost", "Performance: boost settings", lambda: self.open_page("boost"))]

    def tray_actions(self):
        return [("Performance", self.toggle), ("Game mode on / off", self.toggle_game),
                ("Gaming HUD", self.toggle_hud)]

    # ── windows ─────────────────────────────────────────────────────────────────
    def make_page(self):
        from core import widgets
        from ui.performance_ui import PerformancePanel
        with self.themed():
            view = PerformancePanel(self)
        self.views.append(view)
        QTimer.singleShot(0, self._refresh_wants)
        return widgets.embed(view)

    def toggle(self):
        from ui.performance_ui import PerformancePanel
        if self.panel is None or not alive(self.panel):
            with self.themed():
                self.panel = PerformancePanel(self)
        if self.panel.isVisible():
            self.panel.hide()
        else:
            self.panel.show_and_raise()
        QTimer.singleShot(0, self._refresh_wants)

    def open_page(self, key: str):
        if self.panel is None or not alive(self.panel) or not self.panel.isVisible():
            self.toggle()
        self.panel.show_page(key)

    def toggle_hud(self, on: bool | None = None):
        from ui.performance_ui import Hud
        if self.hud is None or not alive(self.hud):
            with self.themed():
                self.hud = Hud(self)
        want = (not self.hud.isVisible()) if on is None else on
        if want:
            self.hud.show_hud()
            if self.last:
                self.hud.update_from(self.last)
        else:
            self.hud.hide()
        QTimer.singleShot(0, self._refresh_wants)

    def rebuild_hud(self):
        if self.hud is not None and alive(self.hud):
            on = self.hud.isVisible()
            self.hud.close()
            self.hud.deleteLater()
            self.hud = None
            if on:
                self.toggle_hud(True)

    # ── who needs numbers ───────────────────────────────────────────────────────
    def _shown_views(self) -> list:
        self.views = [v for v in self.views if alive(v)]
        out = [v for v in self.views if v.isVisible()]
        if self.panel is not None and alive(self.panel) and self.panel.isVisible():
            out.append(self.panel)
        return out

    def _background_needs(self) -> tuple[bool, bool]:
        """(measure in the background, with processes) for alerts, rules and automatic Game mode."""
        a = self.data["alerts"]
        alerts = bool(a.get("on"))
        rules = bool(self.data.get("rules"))
        auto = bool(self.data["game"].get("auto"))
        return (alerts or rules or auto or self.game_on,
                rules or auto or (alerts and a["app_cpu"].get("on")) or (self.game_on and bool(self.data["game"].get("lower"))))

    def _refresh_wants(self):
        views = self._shown_views()
        hud = self.hud is not None and alive(self.hud) and self.hud.isVisible()
        bg, bg_procs = self._background_needs()
        interval = max(0.5, float(self.data.get("interval", 1.0) or 1.0))
        if views or hud:
            procs = bg_procs or any(v.wants_processes() for v in views)
            self._want = (interval, procs)
        elif bg:
            self._want = (max(interval, 2.0), bg_procs)
        else:
            self._want = None
        if self._want and self._worker is not None:
            self._worker.wake.set()

    # ── a new sample ────────────────────────────────────────────────────────────
    def _remember(self, key: str, value):
        if value is None:
            return
        h = self.history.get(key)
        if h is None:
            h = self.history[key] = deque(maxlen=HISTORY)
        h.append(float(value))

    def series(self, key: str, n: int) -> list[float]:
        h = self.history.get(key)
        return list(h)[-n:] if h else []

    def _got(self, d: dict):
        self.last = d
        cpu, mem = d["cpu"], d["mem"]
        self._remember("cpu", cpu["usage"])
        for i, c in enumerate(cpu["cores"]):
            self._remember(f"core:{i}", c)
        self._remember("cpu_temp", cpu.get("temp"))
        self._remember("mem", mem["percent"])
        self._remember("swap", 100.0 * mem["swap_used"] / mem["swap_total"] if mem["swap_total"] else 0.0)
        for g in d["gpus"]:
            self._remember(f"gpu:{g['id']}:load", g.get("load"))
            if g.get("vram_total"):
                self._remember(f"gpu:{g['id']}:vram", 100.0 * (g.get("vram_used") or 0) / g["vram_total"])
            self._remember(f"gpu:{g['id']}:temp", g.get("temp"))
        self._remember("disk:read", d["disk"]["read"])
        self._remember("disk:write", d["disk"]["write"])
        self._remember("net:down", d["net"]["down"])
        self._remember("net:up", d["net"]["up"])
        temps = [t for _c, _l, t in d["sensors"]] + [g["temp"] for g in d["gpus"] if g.get("temp") is not None]
        self._remember("hottest", max(temps) if temps else None)
        for v in self._shown_views():
            try:
                v.update_from(d)
            except Exception:
                log.exception("a Performance page could not show the numbers")
        if self.hud is not None and alive(self.hud) and self.hud.isVisible():
            self.hud.update_from(d)
        for fn in list(self.listeners):
            try:
                fn(d)
            except Exception:
                log.exception("a Performance listener failed")
        try:
            self._alerts(d)
            if "procs" in d:
                self._rules(d["procs"])
                self._auto_game(d["procs"])
                if self.game_on:
                    self._lower_background(d["procs"])
        except Exception:
            log.exception("alerts / rules failed")

    def proc_cpu(self, p: dict) -> float:
        """A process's CPU % the way Options → CPU % for apps says."""
        if self.data.get("cpu_mode") == "core":
            return p["cpu"]
        n = (self.last or {}).get("cpu", {}).get("logical") or os.cpu_count() or 1
        return p["cpu"] / n

    # ── alerts ──────────────────────────────────────────────────────────────────
    def _alert(self, key: str, over: bool, seconds: float, title: str, text: str):
        now = time.monotonic()
        if not over:
            self._over_since.pop(key, None)
            return
        since = self._over_since.setdefault(key, now)
        if now - since < seconds or now - self._alerted.get(key, -1e9) < 600:
            return
        self._alerted[key] = now
        self.ctx.toast(title, text, 8000)

    def _alerts(self, d: dict):
        a = self.data["alerts"]
        if not a.get("on"):
            return
        cpu, mem = d["cpu"], d["mem"]
        r = a["cpu"]
        if r.get("on"):
            self._alert("cpu", cpu["usage"] >= r["over"], r["for"], "Processor busy",
                        f"The CPU has been at {cpu['usage']:.0f} % for {r['for']} s.")
        r = a["mem"]
        if r.get("on"):
            self._alert("mem", mem["percent"] >= r["over"], r["for"], "Memory nearly full",
                        f"{perfstats.human_bytes(mem['used'])} of {perfstats.human_bytes(mem['total'])} in use "
                        f"({mem['percent']:.0f} %).")
        r = a["cpu_temp"]
        if r.get("on") and cpu.get("temp") is not None:
            self._alert("cpu_temp", cpu["temp"] >= r["over"], r["for"], "Processor hot",
                        f"The CPU is at {cpu['temp']:.0f} °C.")
        for g in d["gpus"]:
            r = a["gpu_temp"]
            if r.get("on") and g.get("temp") is not None:
                self._alert(f"gpu_temp:{g['id']}", g["temp"] >= r["over"], r["for"], "GPU hot",
                            f"{g['name']} is at {g['temp']:.0f} °C.")
            r = a["vram"]
            if r.get("on") and g.get("vram_total") and not g.get("integrated"):
                pct = 100.0 * (g.get("vram_used") or 0) / g["vram_total"]
                self._alert(f"vram:{g['id']}", pct >= r["over"], r["for"], "VRAM nearly full",
                            f"{g['name']}: {perfstats.human_bytes(g['vram_used'])} of "
                            f"{perfstats.human_bytes(g['vram_total'])} ({pct:.0f} %). Games may stutter.")
        r = a["disk_full"]
        if r.get("on"):
            for dr in d["disk"]["drives"]:
                self._alert(f"disk:{dr['mount']}", dr["percent"] >= r["over"], 0, "Drive nearly full",
                            f"{dr['mount']} is {dr['percent']:.0f} % full "
                            f"({perfstats.human_bytes(dr['total'] - dr['used'])} free).")
        r = a["battery"]
        b = d.get("battery")
        if r.get("on") and b:
            self._alert("battery", b["percent"] <= r["under"] and not b["plugged"], 0, "Battery low",
                        f"{b['percent']:.0f} % left.")
        r = a["app_cpu"]
        if r.get("on") and "procs" in d:
            hot = set()
            for p in d["procs"]:
                if self.proc_cpu(p) >= r["over"]:
                    hot.add(p["pid"])
                    self._alert(f"app:{p['pid']}", True, r["for"], "An app is using a lot of CPU",
                                f"{p['name']} (PID {p['pid']}) has used {self.proc_cpu(p):.0f} % of the CPU for "
                                f"over {r['for']} s.")
            for k in [k for k in self._over_since if k.startswith("app:") and int(k[4:]) not in hot]:
                self._over_since.pop(k, None)

    # ── rules ───────────────────────────────────────────────────────────────────
    def _rules(self, procs: list[dict]):
        rules = self.data.get("rules") or []
        if not rules:
            return
        alive_pids = {p["pid"] for p in procs}
        self._ruled = {k for k in self._ruled if k[0] in alive_pids}
        for p in procs:
            for i, rule in enumerate(rules):
                if (p["pid"], i) in self._ruled or not _matches(p, [rule.get("match", "")]):
                    continue
                self._ruled.add((p["pid"], i))
                if rule.get("priority") and rule["priority"] != "normal":
                    err = perftweaks.set_priority(p["pid"], rule["priority"], root_ok=False)
                    if err:
                        log.info("rule %s: priority of %s: %s", rule.get("match"), p["pid"], err)
                if rule.get("cores"):
                    err = perftweaks.set_affinity(p["pid"], [int(c) for c in rule["cores"]])
                    if err:
                        log.info("rule %s: cores of %s: %s", rule.get("match"), p["pid"], err)

    # ── Game mode ───────────────────────────────────────────────────────────────
    def toggle_game(self):
        self.set_game(not self.game_on)

    def _auto_game(self, procs: list[dict]):
        names = self.data["game"].get("auto") or []
        if not names:
            return
        running = [p for p in procs if _matches(p, names)]
        if running and not self.game_on:
            self.game_auto = True
            self.set_game(True, why=running[0]["name"])
        elif not running and self.game_on and self.game_auto:
            self.set_game(False)

    def _lower_background(self, procs: list[dict]):
        names = self.data["game"].get("lower") or []
        for p in procs:
            if p["pid"] not in self._lowered and _matches(p, names):
                self._lowered[p["pid"]] = p.get("nice")
                perftweaks.set_priority(p["pid"], "low", root_ok=False)

    def set_game(self, on: bool, quiet: bool = False, why: str = ""):
        from modules.syscontrol import in_thread
        g = self.data["game"]
        if on == self.game_on:
            return
        self.game_on = on
        if not on:
            self.game_auto = False
        if on:
            def work():
                saved = {"power": None, "hypr": {}, "win": {}, "dnd": False}
                if g.get("power"):
                    best = perftweaks.best_power_profile()
                    now = sc.power_profile()
                    if best and best != now:
                        saved["power"] = now
                        sc.set_power_profile(best)
                if g.get("effects"):
                    if IS_WIN:
                        for o in perftweaks._win_options():
                            if o.key in ("win:transparency", "win:animations"):
                                v = o.get()
                                if v:
                                    saved["win"][o.key] = v
                                    o.set(False)
                            elif o.key == "win:game_mode" and not o.get():
                                saved["win"][o.key] = False
                                o.set(True)
                    else:
                        from core import hypr
                        if hypr.available():
                            for path, _label, _on in perftweaks.HYPR_EFFECTS:
                                v = sc._hypr_read(path)
                                if v:
                                    saved["hypr"][path] = v
                                    sc.hypr_set(path, False)
                if g.get("dnd") and not IS_WIN:
                    saved["dnd"] = not perftweaks.set_dnd(True)
                closed = []
                if g.get("close"):
                    import psutil
                    for p in psutil.process_iter(["pid", "name", "exe"]):
                        info = {"name": p.info.get("name") or "", "proc": p.info.get("name") or "",
                                "exe": p.info.get("exe") or ""}
                        if _matches(info, g["close"]) and p.pid != os.getpid():
                            if not perftweaks.end_process(p.pid):
                                closed.append(info["name"])
                if g.get("free_memory") and not IS_WIN:
                    perftweaks.drop_caches()
                return saved, closed

            def done(res):
                if not isinstance(res, tuple):
                    self.ctx.notify("Game mode", f"Not everything could be changed: {res}")
                    return
                saved, closed = res
                self.data["game_saved"] = saved
                self.save()
                if not quiet:
                    text = "Fastest settings on" + (f" for {why}" if why else "") + "."
                    if closed:
                        text += f" Closed {', '.join(sorted(set(closed)))}."
                    self.ctx.notify("Game mode on", text)
            in_thread(work, done)
            if g.get("hud"):
                self.toggle_hud(True)
        else:
            saved = self.data.get("game_saved") or {}
            self.data["game_saved"] = None
            self.save()
            in_thread(lambda: self._restore_game(saved), None)
            for pid, nice in list(self._lowered.items()):
                if nice is not None:
                    perftweaks.set_priority(pid, perftweaks.priority_of(nice) or "normal", root_ok=False)
            self._lowered = {}
            if g.get("hud") and self.hud is not None and alive(self.hud):
                self.toggle_hud(False)
            if not quiet:
                self.ctx.notify("Game mode off", "Everything is back as it was.")
        self._refresh_wants()
        for v in self._shown_views():
            v.game_changed()

    def _restore_after_crash(self):
        from modules.syscontrol import in_thread
        saved = self.data.get("game_saved") or {}
        self.data["game_saved"] = None
        self.save()
        in_thread(lambda: self._restore_game(saved), None)

    def _restore_game(self, saved: dict):
        """Put back what Game mode changed (runs on a worker thread, or at start after a crash)."""
        if not saved:
            return
        try:
            if saved.get("power"):
                sc.set_power_profile(saved["power"])
            for path, v in (saved.get("hypr") or {}).items():
                sc.hypr_set(path, v)
            if saved.get("win"):
                by_key = {o.key: o for o in perftweaks._win_options()}
                for key, v in saved["win"].items():
                    if key in by_key:
                        by_key[key].set(v)
            if saved.get("dnd"):
                perftweaks.set_dnd(False)
        except Exception:
            log.exception("could not put back what Game mode changed")
