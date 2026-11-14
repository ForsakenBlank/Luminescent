"""
The settings that change how fast the computer runs, for the Performance module's Boost page, plus the things done
to processes (priority, which cores, ending them), startup apps and cleaning temporary files.

Linux: power profiles (powerprofilesctl), the CPU governor, energy preference, boost and top clock, swappiness, the
file cache and transparent huge pages from /sys and /proc, NVIDIA's power limit and persistence mode, AMD's
performance level and Hyprland's effects. Writing to /sys needs root: those go through pkexec, which asks for your
password in the desktop's own dialog (once per change).
Windows: power plans and the processor's top and bottom speed (powercfg), Game Mode, transparency, animations,
background apps and Game Bar recording. Nothing needs administrator rights.
"""
import os
import re
import glob
import shlex
import shutil
import logging
import subprocess

from core import IS_WIN, hypr
from core import syscontrol as sc
from core.syscontrol import Option

log = logging.getLogger("perftweaks")

GROUPS = [("power", "Power", "⚡"), ("cpu", "Processor", "▦"), ("memory", "Memory", "▤"),
          ("gpu", "Graphics card", "◆"), ("effects", "Desktop effects", "✦")]

_NO_WINDOW = 0x08000000 if IS_WIN else 0
CPU = "/sys/devices/system/cpu"


def _read(path: str) -> str:
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read().strip()
    except OSError:
        return ""


def as_root(script: str) -> str:
    """Run a shell script as root through pkexec (the desktop asks for the password). '' or what went wrong."""
    if IS_WIN:
        return "only on Linux"
    if not shutil.which("pkexec"):
        return "pkexec is not installed (sudo pacman -S polkit)"
    try:
        r = subprocess.run(["pkexec", "sh", "-c", script], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError) as e:
        return f"{type(e).__name__}: {e}"
    if r.returncode == 0:
        return ""
    if r.returncode in (126, 127):
        return "cancelled (no password given)"
    return (r.stderr or r.stdout or "it did not work").strip().splitlines()[-1]


def _write_all(paths: list[str], value) -> str:
    if not paths:
        return "this computer does not have that setting"
    v = shlex.quote(str(value))
    return as_root(" && ".join(f"echo {v} > {shlex.quote(p)}" for p in paths))


def _cpu_files(name: str) -> list[str]:
    return sorted(glob.glob(f"{CPU}/cpu[0-9]*/cpufreq/{name}"))


# ══ Linux ═════════════════════════════════════════════════════════════════════════
def _linux_cpu() -> list[Option]:
    out = []
    govs = _read(f"{CPU}/cpu0/cpufreq/scaling_available_governors").split()
    names = {"performance": "Performance (always fast)", "powersave": "Power save (as fast as needed)",
             "schedutil": "Schedutil (follows the load)", "ondemand": "On demand", "conservative": "Conservative",
             "userspace": "Userspace"}
    if govs:
        out.append(Option("cpu", "linux:governor", "CPU governor", "choice",
                          get=lambda: _read(f"{CPU}/cpu0/cpufreq/scaling_governor") or None,
                          set=lambda v: _write_all(_cpu_files("scaling_governor"), v),
                          choices=[(g, names.get(g, g)) for g in govs],
                          help="How the processor picks its speed. Performance keeps every core at full clock; on "
                               "AMD and Intel's own drivers Power save still speeds up when something needs it."))
    epps = [e for e in _read(f"{CPU}/cpu0/cpufreq/energy_performance_available_preferences").split()
            if e not in ("default", "custom", "dynamic")]
    if epps:
        nice = {"performance": "Performance", "balance_performance": "Balanced, leaning to speed",
                "balance_power": "Balanced, leaning to saving power", "power": "Save power"}
        out.append(Option("cpu", "linux:epp", "Energy preference", "choice",
                          get=lambda: _read(f"{CPU}/cpu0/cpufreq/energy_performance_preference") or None,
                          set=lambda v: _write_all(_cpu_files("energy_performance_preference"), v),
                          choices=[(e, nice.get(e, e)) for e in epps],
                          help="A hint to the processor. It can't be changed while the governor is Performance."))
    boost_global = f"{CPU}/cpufreq/boost"
    no_turbo = f"{CPU}/intel_pstate/no_turbo"
    if os.path.exists(boost_global) or _cpu_files("boost"):
        files = [boost_global] if os.path.exists(boost_global) else _cpu_files("boost")
        out.append(Option("cpu", "linux:boost", "Boost clocks (Turbo / Precision Boost)", "bool",
                          get=lambda: _read(files[0]) == "1" if _read(files[0]) else None,
                          set=lambda v: _write_all(files, 1 if v else 0),
                          help="Off makes the processor cooler and quieter but slower."))
    elif os.path.exists(no_turbo):
        out.append(Option("cpu", "linux:boost", "Turbo Boost", "bool",
                          get=lambda: _read(no_turbo) == "0" if _read(no_turbo) else None,
                          set=lambda v: _write_all([no_turbo], 0 if v else 1)))
    lo, hi = _read(f"{CPU}/cpu0/cpufreq/cpuinfo_min_freq"), _read(f"{CPU}/cpu0/cpufreq/cpuinfo_max_freq")
    if lo.isdigit() and hi.isdigit():
        out.append(Option("cpu", "linux:max_freq", "Top speed", "int",
                          get=lambda: (int(_read(f"{CPU}/cpu0/cpufreq/scaling_max_freq") or 0) // 1000) or None,
                          set=lambda v: _write_all(_cpu_files("scaling_max_freq"),
                                                   max(int(lo), min(int(hi), int(v) * 1000))),
                          min=int(lo) // 1000, max=int(hi) // 1000, step=100, suffix=" MHz",
                          help="Cap the clock to keep a laptop cool. All the way up is the normal speed."))
    return out


def _sysctl_option(group, key, label, path, lo, hi, help_="", suffix="") -> Option | None:
    if not os.path.exists(path):
        return None
    name = path.replace("/proc/sys/", "").replace("/", ".")
    return Option(group, key, label, "int", get=lambda: int(_read(path) or 0),
                  set=lambda v: as_root(f"sysctl -w {name}={int(v)}"), min=lo, max=hi, help=help_, suffix=suffix)


def _bracketed(path: str) -> str | None:
    m = re.search(r"\[(\w+)\]", _read(path))
    return m.group(1) if m else None


def _linux_memory() -> list[Option]:
    out = [
        _sysctl_option("memory", "linux:swappiness", "Swappiness", "/proc/sys/vm/swappiness", 0, 200,
                       "How readily memory is moved to swap. Lower keeps programs in RAM (snappier with plenty of "
                       "RAM); 60 is the usual default, zram setups often use 100 or more."),
        _sysctl_option("memory", "linux:cache_pressure", "File cache pressure", "/proc/sys/vm/vfs_cache_pressure",
                       1, 500, "Lower keeps the list of files in memory longer (faster file browsing). 100 is the "
                               "default."),
    ]
    thp = "/sys/kernel/mm/transparent_hugepage/enabled"
    if os.path.exists(thp):
        out.append(Option("memory", "linux:thp", "Transparent huge pages", "choice",
                          get=lambda: _bracketed(thp), set=lambda v: _write_all([thp], v),
                          choices=[("always", "Always (some games and databases run faster)"),
                                   ("madvise", "When a program asks (the usual default)"), ("never", "Never")]))
    return [o for o in out if o]


def _nvidia_query(fields: str) -> list[str] | None:
    if not shutil.which("nvidia-smi"):
        return None
    out = sc._run(["nvidia-smi", f"--query-gpu={fields}", "--format=csv,noheader,nounits", "-i", "0"])
    return [p.strip() for p in out.split(",")] if out else None


def _gpu_options() -> list[Option]:
    out = []
    q = _nvidia_query("power.min_limit,power.max_limit,power.limit,persistence_mode")
    if q and len(q) == 4:
        try:
            lo, hi = float(q[0]), float(q[1])
        except ValueError:
            lo = hi = 0
        if hi > lo > 0:
            def get_pl():
                v = _nvidia_query("power.limit")
                try:
                    return int(float(v[0])) if v else None
                except ValueError:
                    return None
            set_pl = (lambda v: sc._run_err(["nvidia-smi", "-pl", str(int(v))])) if IS_WIN else \
                (lambda v: as_root(f"nvidia-smi -pl {int(v)}"))
            out.append(Option("gpu", "nv:power_limit", "NVIDIA power limit", "int", get=get_pl, set=set_pl,
                              min=int(lo), max=int(hi), step=1, suffix=" W",
                              help="Lower runs the card cooler and quieter for a few % less speed; it goes back "
                                   "to the card's own limit after a restart." +
                                   (" Needs Luminescent to run as administrator." if IS_WIN else "")))
        if not IS_WIN:
            out.append(Option("gpu", "nv:persistence", "Keep the NVIDIA driver loaded (persistence mode)", "bool",
                              get=lambda: (_nvidia_query("persistence_mode") or [""])[0] == "Enabled",
                              set=lambda v: as_root(f"nvidia-smi -pm {1 if v else 0}"),
                              help="Games and CUDA apps start a little faster; the card idles a few watts higher."))
    if not IS_WIN:
        for card in sorted(glob.glob("/sys/class/drm/card[0-9]*/device/power_dpm_force_performance_level")):
            name = card.split("/")[4]
            out.append(Option("gpu", f"amd:{name}:level", f"AMD performance level ({name})", "choice",
                              get=lambda c=card: _read(c) or None, set=lambda v, c=card: _write_all([c], v),
                              choices=[("auto", "Automatic"), ("low", "Lowest clocks (cool, quiet)"),
                                       ("high", "Highest clocks (fastest)")],
                              help="Goes back to Automatic after a restart."))
    return out


HYPR_EFFECTS = [("animations:enabled", "Animations", True), ("decoration:blur:enabled", "Blur", True),
                ("decoration:shadow:enabled", "Shadows", True)]


def _hypr_effects() -> list[Option]:
    if not hypr.available():
        return []
    out = [Option("effects", f"hypr:{path}", label, "bool", get=lambda p=path: sc._hypr_read(p),
                  set=lambda v, p=path: sc.hypr_set(p, bool(v)),
                  help="Off saves a little GPU time in games." if path == "animations:enabled" else "")
           for path, label, _on in HYPR_EFFECTS]
    out.append(Option("effects", "hypr:decoration:blur:passes", "Blur passes", "int",
                      get=lambda: sc._hypr_read("decoration:blur:passes"),
                      set=lambda v: sc.hypr_set("decoration:blur:passes", int(v)), min=1, max=6,
                      help="Each pass costs GPU time; 1 or 2 looks nearly the same."))
    out.append(Option("effects", "hypr:render:direct_scanout", "Direct scanout for full-screen games", "choice",
                      get=lambda: sc._hypr_read("render:direct_scanout"),
                      set=lambda v: sc.hypr_set("render:direct_scanout", int(v)),
                      choices=[(0, "Off"), (1, "On"), (2, "Only for games")],
                      help="A full-screen game is shown without Hyprland drawing over it: lower latency."))
    out.append(Option("effects", "hypr:misc:vfr", "Lower the refresh rate when nothing moves", "bool",
                      get=lambda: sc._hypr_read("misc:vfr"), set=lambda v: sc.hypr_set("misc:vfr", bool(v))))
    return out


# ══ Windows ═══════════════════════════════════════════════════════════════════════
_PROC = "54533251-82be-4824-96c1-47b60b740d00"
_PROC_MAX = "bc5038f7-23e0-4960-96da-33abaf5935ec"
_PROC_MIN = "893dee8e-2bef-41e0-89c6-b55d0929964c"
ULTIMATE = "e9a42b02-d5df-448d-aa00-03f14749eb61"


def add_ultimate_plan() -> str:
    """Windows: add the hidden Ultimate Performance power plan (it then shows in the list)."""
    return sc._run_err(["powercfg", "-duplicatescheme", ULTIMATE])


def _win_options() -> list[Option]:
    out = [
        Option("cpu", "win:proc_max", "Top processor speed", "int",
               get=lambda: sc._powercfg_value(_PROC, _PROC_MAX), set=lambda v: sc._powercfg_put(_PROC, _PROC_MAX, v),
               min=5, max=100, step=5, suffix=" %",
               help="Of the current power plan. Below 100 % also switches boost off: cooler and quieter."),
        Option("cpu", "win:proc_min", "Lowest processor speed", "int",
               get=lambda: sc._powercfg_value(_PROC, _PROC_MIN), set=lambda v: sc._powercfg_put(_PROC, _PROC_MIN, v),
               min=0, max=100, step=5, suffix=" %",
               help="Higher keeps the processor ready (less stutter) at the cost of heat and power."),
        sc._reg_option("effects", "win:game_mode", "Game Mode", r"Software\Microsoft\GameBar", "AutoGameModeEnabled",
                       default=1, help="Windows gives the game you're playing more of the processor."),
        sc._reg_option("effects", "win:transparency", "Transparency effects", sc._PERSONALIZE, "EnableTransparency",
                       default=1, after=lambda: sc._broadcast("ImmersiveColorSet")),
        sc._reg_option("effects", "win:background_apps", "Let apps run in the background",
                       r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications", "GlobalUserDisabled",
                       on=0, off=1, default=0, help="Store apps stay closed when you're not using them."),
        sc._reg_option("effects", "win:game_dvr", "Game Bar background recording",
                       r"System\GameConfigStore", "GameDVR_Enabled", default=1,
                       help="Recording the last minutes of a game in the background costs frames."),
    ]
    try:
        get, set_ = sc._spi_bool(0x1042, 0x1043)
        out.insert(4, Option("effects", "win:animations", "Animations in Windows", "bool", get=get, set=set_))
    except Exception:
        pass
    return out


# ══ everything ════════════════════════════════════════════════════════════════════
def _power() -> list[Option]:
    profiles = sc.power_profiles()
    if not profiles:
        return []
    return [Option("power", "power:profile", "Power plan" if IS_WIN else "Power mode", "choice",
                   get=sc.power_profile, set=sc.set_power_profile, choices=profiles,
                   help="Performance runs everything at full speed; Power saver is cooler, quieter and lasts longer "
                        "on battery.")]


def options() -> list[Option]:
    """Every speed setting this computer has. Slow (powercfg, nvidia-smi): call it on a worker thread."""
    out = []
    for part in (_power, _win_options if IS_WIN else _linux_cpu, (lambda: []) if IS_WIN else _linux_memory,
                 _gpu_options, (lambda: []) if IS_WIN else _hypr_effects):
        try:
            out += part()
        except Exception:
            log.exception("could not read the %s settings", getattr(part, "__name__", "?"))
    return out


def best_power_profile() -> str | None:
    """The fastest power mode / plan there is."""
    profiles = sc.power_profiles()
    for want in ("performance", "ultimate", "high"):
        for pid, name in profiles:
            if want in pid.lower() or want in name.lower():
                return pid
    return None


def drop_caches() -> str:
    """Linux: give the file cache back (sync first so nothing is lost)."""
    return as_root("sync; echo 3 > /proc/sys/vm/drop_caches")


def set_dnd(on: bool) -> str:
    """Do not disturb in the notification daemon that is running (Linux): Caelestia, swaync, dunst or mako."""
    if IS_WIN:
        return "not on Windows"
    tries = [["caelestia", "shell", "notifs", "enableDnd" if on else "disableDnd"],
             ["swaync-client", "-dn" if on else "-df"],
             ["dunstctl", "set-paused", "true" if on else "false"],
             ["makoctl", "mode", "-a" if on else "-r", "do-not-disturb"]]
    err = "no notification daemon with do not disturb was found"
    for cmd in tries:
        if shutil.which(cmd[0]):
            err = sc._run_err(cmd)
            if not err:
                return ""
    return err


# ══ processes ═════════════════════════════════════════════════════════════════════
PRIORITIES = [("idle", "Idle"), ("low", "Low"), ("below", "Below normal"), ("normal", "Normal"),
              ("above", "Above normal"), ("high", "High")]
_NICE = {"idle": 19, "low": 15, "below": 5, "normal": 0, "above": -5, "high": -10}


def priority_of(nice) -> str:
    """Our name for a nice value (Linux) or priority class (Windows)."""
    if nice is None:
        return ""
    if IS_WIN:
        import psutil
        return {psutil.IDLE_PRIORITY_CLASS: "idle", psutil.BELOW_NORMAL_PRIORITY_CLASS: "below",
                psutil.NORMAL_PRIORITY_CLASS: "normal", psutil.ABOVE_NORMAL_PRIORITY_CLASS: "above",
                psutil.HIGH_PRIORITY_CLASS: "high", psutil.REALTIME_PRIORITY_CLASS: "high"}.get(nice, "normal")
    for name, n in sorted(_NICE.items(), key=lambda kv: kv[1]):
        if nice <= n:
            return name
    return "idle"


def set_priority(pid: int, level: str, root_ok: bool = True) -> str:
    try:
        import psutil
        p = psutil.Process(pid)
        if IS_WIN:
            cls = {"idle": psutil.IDLE_PRIORITY_CLASS, "low": psutil.IDLE_PRIORITY_CLASS,
                   "below": psutil.BELOW_NORMAL_PRIORITY_CLASS, "normal": psutil.NORMAL_PRIORITY_CLASS,
                   "above": psutil.ABOVE_NORMAL_PRIORITY_CLASS, "high": psutil.HIGH_PRIORITY_CLASS}[level]
            p.nice(cls)
            return ""
        p.nice(_NICE[level])
        return ""
    except KeyError:
        return f"unknown priority {level}"
    except Exception as e:
        if not IS_WIN and root_ok and type(e).__name__ == "AccessDenied":
            return as_root(f"renice -n {_NICE[level]} -p {int(pid)}")
        return f"{type(e).__name__}: {e}"


def set_affinity(pid: int, cores: list[int]) -> str:
    try:
        import psutil
        psutil.Process(pid).cpu_affinity(cores)
        return ""
    except Exception as e:
        if not IS_WIN and type(e).__name__ == "AccessDenied":
            return as_root(f"taskset -pc {','.join(map(str, cores))} {int(pid)}")
        return f"{type(e).__name__}: {e}"


def end_process(pid: int, force: bool = False, root_ok: bool = False) -> str:
    try:
        import psutil
        p = psutil.Process(pid)
        p.kill() if force else p.terminate()
        return ""
    except Exception as e:
        if type(e).__name__ == "NoSuchProcess":
            return ""
        if not IS_WIN and root_ok and type(e).__name__ == "AccessDenied":
            return as_root(f"kill -{9 if force else 15} {int(pid)}")
        return f"{type(e).__name__}: {e}"


def open_location(path: str):
    if not path or not os.path.exists(path):
        return
    if IS_WIN:
        subprocess.Popen(["explorer", "/select,", os.path.normpath(path)], creationflags=_NO_WINDOW)
    else:
        from PyQt6.QtGui import QDesktopServices
        from PyQt6.QtCore import QUrl
        QDesktopServices.openUrl(QUrl.fromLocalFile(os.path.dirname(path)))


# ══ startup apps ══════════════════════════════════════════════════════════════════
def _desktop_entry(path: str) -> dict:
    out, in_main = {}, False
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if line.startswith("["):
                    in_main = line == "[Desktop Entry]"
                    continue
                if in_main and "=" in line:
                    k, _, v = line.partition("=")
                    out.setdefault(k.strip(), v.strip())
    except OSError:
        pass
    return out


def _autostart_dir() -> str:
    return os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "autostart")


def startup_apps() -> list[dict]:
    """[{id, name, command, enabled, where, icon}] that start when you log in."""
    return _win_startup() if IS_WIN else _linux_startup()


def _linux_startup() -> list[dict]:
    apps: dict[str, dict] = {}
    dirs = [d for d in (os.environ.get("XDG_CONFIG_DIRS") or "/etc/xdg").split(":") if d]
    for base in [os.path.join(d, "autostart") for d in reversed(dirs)] + [_autostart_dir()]:
        for path in sorted(glob.glob(os.path.join(base, "*.desktop"))):
            e = _desktop_entry(path)
            fid = os.path.basename(path)
            only = e.get("OnlyShowIn", "")
            desk = os.environ.get("XDG_CURRENT_DESKTOP", "")
            if only and not any(d and d in only.split(";") for d in desk.split(":")):
                continue
            enabled = e.get("Hidden", "false").lower() != "true" and \
                e.get("X-GNOME-Autostart-enabled", "true").lower() != "false"
            apps[fid] = {"id": fid, "name": e.get("Name", fid[:-8]), "command": e.get("Exec", ""),
                         "enabled": enabled, "where": "you" if base == _autostart_dir() else "the system",
                         "icon": e.get("Icon", ""), "path": path}
    return sorted(apps.values(), key=lambda a: a["name"].lower())


def set_startup(app: dict, on: bool) -> str:
    return _win_set_startup(app, on) if IS_WIN else _linux_set_startup(app, on)


def _linux_set_startup(app: dict, on: bool) -> str:
    own = os.path.join(_autostart_dir(), app["id"])
    try:
        os.makedirs(_autostart_dir(), exist_ok=True)
        if os.path.abspath(app["path"]) != os.path.abspath(own):        # a system one: our copy overrides it
            shutil.copyfile(app["path"], own)
        with open(own, "r", encoding="utf-8", errors="replace") as f:
            lines = [ln for ln in f.read().splitlines()
                     if not ln.startswith(("Hidden=", "X-GNOME-Autostart-enabled="))]
        at = next((i + 1 for i, ln in enumerate(lines) if ln.strip() == "[Desktop Entry]"), len(lines))
        lines.insert(at, f"Hidden={'false' if on else 'true'}")
        with open(own, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
        return ""
    except OSError as e:
        return str(e)


_RUN = r"Software\Microsoft\Windows\CurrentVersion\Run"
_APPROVED = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved"


def _win_startup() -> list[dict]:
    import winreg
    out = []
    for hive, hname in ((winreg.HKEY_CURRENT_USER, "you"), (winreg.HKEY_LOCAL_MACHINE, "every user")):
        try:
            k = winreg.OpenKey(hive, _RUN)
        except OSError:
            continue
        i = 0
        while True:
            try:
                name, cmd, _t = winreg.EnumValue(k, i)
            except OSError:
                break
            i += 1
            out.append({"id": f"run:{hname}:{name}", "name": name, "command": str(cmd),
                        "enabled": _win_approved(hive, "Run", name), "where": hname, "icon": "", "hive": hive,
                        "kind": "Run", "value": name})
    folder = os.path.join(os.environ.get("APPDATA", ""), r"Microsoft\Windows\Start Menu\Programs\Startup")
    for path in sorted(glob.glob(os.path.join(folder, "*"))):
        name = os.path.basename(path)
        if name.lower() == "desktop.ini":
            continue
        out.append({"id": f"folder:{name}", "name": os.path.splitext(name)[0], "command": path,
                    "enabled": _win_approved(winreg.HKEY_CURRENT_USER, "StartupFolder", name), "where": "you",
                    "icon": "", "hive": winreg.HKEY_CURRENT_USER, "kind": "StartupFolder", "value": name})
    return sorted(out, key=lambda a: a["name"].lower())


def _win_approved(hive, kind: str, name: str) -> bool:
    import winreg
    try:
        with winreg.OpenKey(hive, f"{_APPROVED}\\{kind}") as k:
            data = winreg.QueryValueEx(k, name)[0]
            return not (isinstance(data, bytes) and data and data[0] % 2 == 1)   # 03 / 01 = off, 02 / 06 = on
    except OSError:
        return True


def _win_set_startup(app: dict, on: bool) -> str:
    import winreg
    try:
        with winreg.CreateKeyEx(app["hive"], f"{_APPROVED}\\{app['kind']}", 0, winreg.KEY_SET_VALUE) as k:
            winreg.SetValueEx(k, app["value"], 0, winreg.REG_BINARY, bytes([2 if on else 3] + [0] * 11))
        return ""
    except OSError as e:
        return f"{e} (apps for every user need Luminescent to run as administrator)"


# ══ temporary files ═══════════════════════════════════════════════════════════════
def temp_places() -> list[tuple[str, str]]:
    """[(name, folder)] that are safe to empty."""
    if IS_WIN:
        return [("Temporary files", os.environ.get("TEMP", "")), ("Thumbnail cache", os.path.join(
            os.environ.get("LOCALAPPDATA", ""), r"Microsoft\Windows\Explorer"))][:1]
    cache = os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache")
    return [("Thumbnail cache", os.path.join(cache, "thumbnails"))]


def folder_size(path: str) -> int:
    total = 0
    for root, _dirs, files in os.walk(path):
        for f in files:
            try:
                total += os.path.getsize(os.path.join(root, f))
            except OSError:
                pass
    return total


def empty_folder(path: str) -> tuple[int, int]:
    """Delete what is in it (not the folder). Files in use are skipped. (bytes freed, files skipped)"""
    freed = skipped = 0
    if not path or not os.path.isdir(path):
        return 0, 0
    for root, dirs, files in os.walk(path, topdown=False):
        for f in files:
            fp = os.path.join(root, f)
            try:
                size = os.path.getsize(fp)
                os.remove(fp)
                freed += size
            except OSError:
                skipped += 1
        for d in dirs:
            try:
                os.rmdir(os.path.join(root, d))
            except OSError:
                pass
    return freed, skipped
