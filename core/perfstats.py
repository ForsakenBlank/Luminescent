"""
Everything the Performance module measures, in one place: CPU (every core, clock, temperature), memory and swap,
every GPU with its load, VRAM, temperature, power, clocks and fan, disks, network, battery, temperature sensors and
every process with its CPU, memory, disk, GPU and VRAM use.

Linux:
  * CPU, memory, disks, network, sensors and processes through psutil.
  * NVIDIA through nvidia-smi (the card, and `pmon` for each process's GPU and VRAM).
  * AMD and Intel from /sys/class/drm (load, VRAM, hwmon temperature, power, clock, fan), and each process's GPU
    and VRAM from the DRM fdinfo the kernel keeps for every open GPU file (/proc/<pid>/fdinfo).
Windows:
  * psutil for CPU, memory, disks, network and processes.
  * NVIDIA through nvidia-smi as on Linux. Every GPU (any make) through Windows' own GPU performance counters
    (\\GPU Engine and \\GPU Process Memory, read with PDH), which give each process's GPU and dedicated VRAM too.

A Sampler keeps what it needs between two calls (to turn counters into rates). sample() is slow-ish (it runs
nvidia-smi and reads /proc), so it is meant to run on a worker thread; the module does that.
"""
import os
import re
import glob
import time
import shutil
import logging
import subprocess

from core import IS_WIN

log = logging.getLogger("perfstats")

try:
    import psutil
except ImportError:                       # requirements.txt has it; without it only the /proc parts work
    psutil = None

_NO_WINDOW = 0x08000000 if IS_WIN else 0
GiB = 1073741824
MiB = 1048576

# programs that run a script: the process is named after the script instead ("python · bot.py")
INTERPRETERS = {"python", "python3", "python2", "pythonw", "node", "nodejs", "bun", "deno", "java", "javaw",
                "ruby", "perl", "php", "bash", "sh", "zsh", "fish", "dash", "lua", "luajit", "wine", "wine64",
                "wine64-preloader", "wine-preloader", "powershell", "pwsh", "cmd", "wscript", "cscript", "dotnet",
                "electron", "py", "uv", "uvx"}


def _read(path: str) -> str:
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def _int(path: str) -> int | None:
    raw = _read(path).strip()
    try:
        return int(raw)
    except ValueError:
        return None


def _num(text: str) -> float | None:
    text = text.strip()
    if not text or text.startswith("[") or text in ("N/A", "-"):
        return None
    try:
        return float(text)
    except ValueError:
        return None


def _run(cmd: list[str], timeout: float = 4) -> str | None:
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, creationflags=_NO_WINDOW)
        return r.stdout if r.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def human_bytes(n: float | None, digits: int = 1) -> str:
    if n is None:
        return "–"
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if abs(n) < 1024 or unit == "TB":
            return f"{n:.0f} {unit}" if unit in ("B", "KB") else f"{n:.{digits}f} {unit}"
        n /= 1024
    return ""


def human_rate(n: float | None) -> str:
    return "–" if n is None else human_bytes(n) + "/s"


def duration(s: float) -> str:
    s = int(s)
    d, s = divmod(s, 86400)
    h, s = divmod(s, 3600)
    m, _ = divmod(s, 60)
    if d:
        return f"{d}d {h}h {m}m"
    if h:
        return f"{h}h {m}m"
    return f"{m}m"


# ══ GPU names (pci.ids) ═══════════════════════════════════════════════════════════
_pci_names: dict = {}


def _pci_name(vendor: str, device: str) -> str:
    """'Raphael [Radeon 610M]' for 0x1002 0x164e, from the system's pci.ids."""
    key = (vendor.lower().replace("0x", ""), device.lower().replace("0x", ""))
    if key in _pci_names:
        return _pci_names[key]
    name = ""
    for path in ("/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids", "/usr/share/pci.ids"):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                in_vendor = False
                for line in f:
                    if not line.startswith("\t") and not line.startswith("#"):
                        in_vendor = line[:4].lower() == key[0]
                    elif in_vendor and line.startswith("\t") and not line.startswith("\t\t"):
                        if line[1:5].lower() == key[1]:
                            name = line[5:].strip()
                            break
            break
        except OSError:
            continue
    _pci_names[key] = name
    return name


VENDORS = {"0x1002": "AMD", "0x10de": "NVIDIA", "0x8086": "Intel"}


# ══ Linux GPUs from /sys ══════════════════════════════════════════════════════════
def _sysfs_cards() -> list[dict]:
    """Every GPU in /sys/class/drm that is not driven by nvidia (nvidia-smi does those), with what it reports."""
    out = []
    for card in sorted(glob.glob("/sys/class/drm/card[0-9]*")):
        if "-" in os.path.basename(card):
            continue
        dev = os.path.join(card, "device")
        driver = os.path.basename(os.path.realpath(os.path.join(dev, "driver")))
        if driver in ("nvidia", ""):
            continue
        vendor = _read(os.path.join(dev, "vendor")).strip()
        device = _read(os.path.join(dev, "device")).strip()
        pci = os.path.basename(os.path.realpath(dev))
        g = {"id": f"drm:{pci}", "pci": pci, "driver": driver, "vendor": VENDORS.get(vendor, driver),
             "name": _pci_name(vendor, device) or f"{VENDORS.get(vendor, driver)} GPU",
             "load": None, "vram_used": None, "vram_total": None, "temp": None, "power": None, "power_limit": None,
             "clock": None, "mem_clock": None, "fan": None, "encoder": None, "card": os.path.basename(card)}
        busy = _int(os.path.join(dev, "gpu_busy_percent"))
        if busy is not None:
            g["load"] = float(busy)
        used, total = _int(os.path.join(dev, "mem_info_vram_used")), _int(os.path.join(dev, "mem_info_vram_total"))
        if used is not None and total:
            g["vram_used"], g["vram_total"] = used, total
        vcn = _int(os.path.join(dev, "vcn_busy_percent"))
        if vcn is not None:
            g["encoder"] = float(vcn)
        for hw in glob.glob(os.path.join(dev, "hwmon", "hwmon*")):
            t = _int(os.path.join(hw, "temp1_input"))
            if t is not None:
                g["temp"] = t / 1000
            for name in ("power1_average", "power1_input"):
                pw = _int(os.path.join(hw, name))
                if pw is not None:
                    g["power"] = pw / 1e6
                    break
            cap = _int(os.path.join(hw, "power1_cap"))
            if cap:
                g["power_limit"] = cap / 1e6
            f = _int(os.path.join(hw, "freq1_input"))
            if f is not None:
                g["clock"] = f / 1e6
            f2 = _int(os.path.join(hw, "freq2_input"))
            if f2 is not None:
                g["mem_clock"] = f2 / 1e6
            rpm, rpm_max = _int(os.path.join(hw, "fan1_input")), _int(os.path.join(hw, "fan1_max"))
            if rpm is not None and rpm_max:
                g["fan"] = 100.0 * rpm / rpm_max
            break
        if g["clock"] is None:                                   # Intel
            for p in ("gt_act_freq_mhz", "gt_cur_freq_mhz", "gt/gt0/rps_act_freq_mhz"):
                f = _int(os.path.join(card, p))
                if f is not None:
                    g["clock"] = float(f)
                    break
        # an APU's "VRAM" is a small slice of the RAM: it is the built-in GPU, not the graphics card
        g["integrated"] = driver in ("i915", "xe") and not (g["vram_total"] or 0) > 2 * GiB \
            or (g["vram_total"] is not None and g["vram_total"] < int(1.5 * GiB))
        out.append(g)
    return out


# ══ NVIDIA (nvidia-smi) ═══════════════════════════════════════════════════════════
_NV_FIELDS = ["index", "name", "pci.bus_id", "utilization.gpu", "memory.used", "memory.total", "temperature.gpu",
              "power.draw", "power.limit", "clocks.gr", "clocks.mem", "fan.speed", "utilization.encoder"]


def nvidia_gpus() -> list[dict]:
    if not shutil.which("nvidia-smi"):
        return []
    out = _run(["nvidia-smi", f"--query-gpu={','.join(_NV_FIELDS)}", "--format=csv,noheader,nounits"])
    gpus = []
    for line in (out or "").strip().splitlines():
        parts = [p.strip() for p in line.split(",")]
        if len(parts) < len(_NV_FIELDS):
            continue
        v = dict(zip(_NV_FIELDS, parts))
        used, total = _num(v["memory.used"]), _num(v["memory.total"])
        gpus.append({"id": f"nv:{v['index']}", "index": int(v["index"]), "pci": v["pci.bus_id"].lower()[-12:],
                     "vendor": "NVIDIA", "driver": "nvidia", "name": v["name"], "integrated": False,
                     "load": _num(v["utilization.gpu"]),
                     "vram_used": used * MiB if used is not None else None,
                     "vram_total": total * MiB if total is not None else None,
                     "temp": _num(v["temperature.gpu"]), "power": _num(v["power.draw"]),
                     "power_limit": _num(v["power.limit"]), "clock": _num(v["clocks.gr"]),
                     "mem_clock": _num(v["clocks.mem"]), "fan": _num(v["fan.speed"]),
                     "encoder": _num(v["utilization.encoder"])})
    return gpus


def nvidia_processes() -> dict[int, dict]:
    """{pid: {"gpu": sm %, "vram": bytes}} from `nvidia-smi pmon`, every GPU added up."""
    out = _run(["nvidia-smi", "pmon", "-c", "1", "-s", "um"], timeout=5)
    if not out:
        return {}
    cols: list[str] = []
    procs: dict[int, dict] = {}
    for line in out.splitlines():
        if line.startswith("#"):
            if "pid" in line and not cols:
                cols = line.lstrip("#").split()
            continue
        parts = line.split()
        if not cols or len(parts) < len(cols) - 1:
            continue
        row = dict(zip(cols, parts))
        try:
            pid = int(row.get("pid", "-"))
        except ValueError:
            continue
        sm, fb = _num(row.get("sm", "-")) or 0.0, _num(row.get("fb", "-")) or 0.0
        p = procs.setdefault(pid, {"gpu": 0.0, "vram": 0})
        p["gpu"] += sm
        p["vram"] += int(fb * MiB)
    return procs


# ══ Linux: each process's GPU from the DRM fdinfo ═════════════════════════════════
class _DrmClients:
    """Turns the engine nanoseconds the kernel counts for every open GPU file into a load per process, and reads
    the VRAM each one holds. Works for amdgpu, i915, xe and most other open drivers (not NVIDIA's)."""

    def __init__(self):
        self.prev: dict = {}            # client id -> (ns, time)
        self.fds: dict[int, tuple] = {}  # pid -> (checked at, [fd numbers on /dev/dri])

    def _drm_fds(self, pid: int, now: float) -> list[str]:
        cached = self.fds.get(pid)
        if cached and now - cached[0] < 10:
            return cached[1]
        found = []
        try:
            for fd in os.listdir(f"/proc/{pid}/fd"):
                try:
                    if os.readlink(f"/proc/{pid}/fd/{fd}").startswith("/dev/dri/"):
                        found.append(fd)
                except OSError:
                    pass
        except OSError:
            pass
        self.fds[pid] = (now, found)
        return found

    def read(self, pids) -> tuple[dict[int, dict], dict[str, float]]:
        """({pid: {"gpu": %, "vram": bytes}}, {pci address: total %}) for the processes given."""
        now = time.monotonic()
        seen, procs, per_card = set(), {}, {}
        alive = set()
        for pid in pids:
            alive.add(pid)
            for fd in self._drm_fds(pid, now):
                info = _read(f"/proc/{pid}/fdinfo/{fd}")
                if "drm-client-id" not in info:
                    continue
                kv = {}
                for line in info.splitlines():
                    k, _, v = line.partition(":")
                    kv[k.strip()] = v.strip()
                cid = (kv.get("drm-pdev", ""), kv.get("drm-client-id"))
                if cid in seen:
                    continue
                seen.add(cid)
                ns = 0
                for key in ("drm-engine-gfx", "drm-engine-render", "drm-engine-rcs"):
                    if key in kv:
                        try:
                            ns = max(ns, int(kv[key].split()[0]))
                        except (ValueError, IndexError):
                            pass
                vram = 0
                for key in ("drm-memory-vram", "drm-resident-vram0", "drm-total-vram0"):
                    if key in kv:
                        parts = kv[key].split()
                        try:
                            mult = {"KiB": 1024, "MiB": MiB, "GiB": GiB}.get(parts[1] if len(parts) > 1 else "", 1)
                            vram = int(parts[0]) * mult
                        except (ValueError, IndexError):
                            pass
                        break
                load = 0.0
                old = self.prev.get(cid)
                if old and now > old[1] and ns >= old[0]:
                    load = min(100.0, 100.0 * (ns - old[0]) / ((now - old[1]) * 1e9))
                self.prev[cid] = (ns, now)
                p = procs.setdefault(pid, {"gpu": 0.0, "vram": 0})
                p["gpu"] = min(100.0, p["gpu"] + load)
                p["vram"] += vram
                per_card[cid[0]] = per_card.get(cid[0], 0.0) + load
        self.prev = {k: v for k, v in self.prev.items() if k in seen}
        self.fds = {k: v for k, v in self.fds.items() if k in alive}
        return procs, {k: min(100.0, v) for k, v in per_card.items()}


# ══ Windows: GPU performance counters (PDH) ═══════════════════════════════════════
class _Pdh:
    """Windows' \\GPU Engine and \\GPU Process Memory counters: every GPU's load and each process's share."""
    COUNTERS = {"engine": r"\GPU Engine(*)\Utilization Percentage",
                "proc_mem": r"\GPU Process Memory(*)\Dedicated Usage",
                "adapter_mem": r"\GPU Adapter Memory(*)\Dedicated Usage"}

    def __init__(self):
        import ctypes
        from ctypes import wintypes
        self.ct, self.wt = ctypes, wintypes
        self.pdh = ctypes.WinDLL("pdh")
        self.query = None
        self.handles: dict = {}
        self.opened = 0.0

        class VALUE(ctypes.Structure):
            _fields_ = [("CStatus", wintypes.DWORD), ("doubleValue", ctypes.c_double)]

        class ITEM(ctypes.Structure):
            _fields_ = [("szName", ctypes.c_wchar_p), ("FmtValue", VALUE)]
        self.ITEM = ITEM

    def _open(self):
        self.close()
        q = self.wt.HANDLE()
        if self.pdh.PdhOpenQueryW(None, None, self.ct.byref(q)) != 0:
            raise OSError("PdhOpenQuery failed")
        self.query = q
        for key, path in self.COUNTERS.items():
            h = self.wt.HANDLE()
            if self.pdh.PdhAddEnglishCounterW(q, path, None, self.ct.byref(h)) == 0:
                self.handles[key] = h
        self.pdh.PdhCollectQueryData(q)
        self.opened = time.monotonic()

    def close(self):
        if self.query is not None:
            self.pdh.PdhCloseQuery(self.query)
        self.query, self.handles = None, {}

    def _array(self, key: str) -> list[tuple[str, float]]:
        h = self.handles.get(key)
        if h is None:
            return []
        size, count = self.wt.DWORD(0), self.wt.DWORD(0)
        PDH_FMT_DOUBLE, PDH_MORE_DATA = 0x200 | 0x8000, 0x800007D2
        r = self.pdh.PdhGetFormattedCounterArrayW(h, PDH_FMT_DOUBLE, self.ct.byref(size), self.ct.byref(count), None)
        if r != PDH_MORE_DATA or not size.value:
            return []
        buf = (self.ct.c_byte * size.value)()
        r = self.pdh.PdhGetFormattedCounterArrayW(h, PDH_FMT_DOUBLE, self.ct.byref(size), self.ct.byref(count), buf)
        if r != 0:
            return []
        items = self.ct.cast(buf, self.ct.POINTER(self.ITEM))
        return [(items[i].szName or "", items[i].FmtValue.doubleValue) for i in range(count.value)
                if items[i].FmtValue.CStatus in (0, 1)]

    def read(self) -> tuple[dict[int, dict], dict[str, dict]]:
        """({pid: {"gpu": %, "vram": bytes}}, {luid: {"load": %, "vram_used": bytes}})."""
        if self.query is None or time.monotonic() - self.opened > 60:     # new processes' counters show up
            self._open()
            time.sleep(0.25)
        if self.pdh.PdhCollectQueryData(self.query) != 0:
            return {}, {}
        procs: dict[int, dict] = {}
        engines: dict[tuple, float] = {}        # (luid, engine type) -> % added up over processes
        for name, value in self._array("engine"):
            m = re.match(r"pid_(\d+)_luid_(0x[0-9A-Fa-f]+_0x[0-9A-Fa-f]+)_phys_\d+_eng_\d+_engtype_(.*)", name)
            if not m:
                continue
            pid, luid, kind = int(m.group(1)), m.group(2).lower(), m.group(3)
            engines[(luid, kind)] = engines.get((luid, kind), 0.0) + value
            if kind in ("3D", "Compute_0", "Compute_1", "Graphics_1", "Cuda"):
                p = procs.setdefault(pid, {"gpu": 0.0, "vram": 0})
                p["gpu"] = min(100.0, p["gpu"] + value)
        for name, value in self._array("proc_mem"):
            m = re.match(r"pid_(\d+)_luid_(0x[0-9A-Fa-f]+_0x[0-9A-Fa-f]+)", name)
            if m:
                procs.setdefault(int(m.group(1)), {"gpu": 0.0, "vram": 0})["vram"] += int(value)
        adapters: dict[str, dict] = {}
        for (luid, _kind), value in engines.items():
            a = adapters.setdefault(luid, {"load": 0.0, "vram_used": None})
            a["load"] = max(a["load"], min(100.0, value))           # the busiest engine, as Task Manager shows
        for name, value in self._array("adapter_mem"):
            m = re.match(r"luid_(0x[0-9A-Fa-f]+_0x[0-9A-Fa-f]+)", name)
            if m:
                adapters.setdefault(m.group(1).lower(), {"load": 0.0, "vram_used": None})["vram_used"] = int(value)
        return procs, adapters


def _win_adapters() -> list[dict]:
    """[{name, vram_total}] of the display adapters Windows knows (the registry; no admin needed)."""
    try:
        import winreg
    except ImportError:
        return []
    out = []
    base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"
    try:
        root = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, base)
    except OSError:
        return []
    i = 0
    while True:
        try:
            sub = winreg.EnumKey(root, i)
        except OSError:
            break
        i += 1
        if not sub.isdigit():
            continue
        try:
            k = winreg.OpenKey(root, sub)
            name = winreg.QueryValueEx(k, "DriverDesc")[0]
            try:
                total = int(winreg.QueryValueEx(k, "HardwareInformation.qwMemorySize")[0])
            except OSError:
                raw = winreg.QueryValueEx(k, "HardwareInformation.MemorySize")[0]
                total = int.from_bytes(raw, "little") if isinstance(raw, bytes) else int(raw)
            if "Basic" not in name and "Virtual" not in name:
                out.append({"name": name, "vram_total": total})
        except (OSError, ValueError, TypeError):
            continue
    return out


# ══ the sampler ═══════════════════════════════════════════════════════════════════
class Sampler:
    """sample() returns everything at once; it keeps the last counters so rates come out right."""

    def __init__(self):
        self.t = 0.0
        self.net = None
        self.disk = None
        self.net_start = None
        self.drives: list = []
        self.drives_at = 0.0
        self.sensors: list = []
        self.sensors_at = 0.0
        self.proc_meta: dict = {}         # (pid, created) -> (name, exe, cmd, user)
        self.proc_io: dict = {}           # pid -> (read+written bytes, time)
        self.drm = _DrmClients() if not IS_WIN else None
        self.pdh = None
        self.pdh_failed = False
        self.win_adapters: list | None = None
        self.cpu_name = cpu_name()
        if psutil:
            psutil.cpu_percent(None)
            psutil.cpu_percent(None, percpu=True)

    # ── machine-wide ────────────────────────────────────────────────────────────
    def _cpu(self) -> dict:
        out = {"usage": 0.0, "cores": [], "freq": None, "freq_max": None, "temp": None, "load_avg": None,
               "logical": os.cpu_count() or 1, "physical": None, "name": self.cpu_name}
        if not psutil:
            return out
        out["cores"] = psutil.cpu_percent(None, percpu=True)
        out["usage"] = sum(out["cores"]) / max(1, len(out["cores"]))
        out["physical"] = psutil.cpu_count(False)
        try:
            f = psutil.cpu_freq()
            if f:
                out["freq"], out["freq_max"] = f.current, f.max or None
        except Exception:
            pass
        try:
            out["load_avg"] = os.getloadavg() if not IS_WIN else None
        except OSError:
            pass
        try:
            st = psutil.cpu_stats()
            out["ctx_switches"] = st.ctx_switches
        except Exception:
            pass
        return out

    def _sensors(self, now: float) -> list[tuple[str, str, float]]:
        """[(group, label, °C)], read every few seconds."""
        if now - self.sensors_at < 3 and self.sensors:
            return self.sensors
        self.sensors_at = now
        out = []
        if psutil and hasattr(psutil, "sensors_temperatures"):
            try:
                for chip, items in psutil.sensors_temperatures().items():
                    for i, t in enumerate(items):
                        if t.current and 0 < t.current < 150:
                            out.append((chip, t.label or (chip if len(items) == 1 else f"{chip} {i + 1}"), t.current))
            except Exception:
                pass
        self.sensors = out
        return out

    @staticmethod
    def cpu_temp(sensors) -> float | None:
        for chip in ("k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz"):
            temps = [t for c, label, t in sensors if c == chip and label.lower() in ("tctl", "tdie", "package id 0",
                                                                                      chip, "cpu", "")]
            temps = temps or [t for c, _l, t in sensors if c == chip]
            if temps:
                return max(temps)
        return None

    def _memory(self) -> dict:
        if not psutil:
            return {"used": 0, "total": 0, "available": 0, "cached": 0, "percent": 0.0, "swap_used": 0,
                    "swap_total": 0}
        vm, sw = psutil.virtual_memory(), psutil.swap_memory()
        used = vm.total - vm.available
        return {"used": used, "total": vm.total, "available": vm.available,
                "cached": getattr(vm, "cached", 0) + getattr(vm, "buffers", 0), "percent": vm.percent,
                "swap_used": sw.used, "swap_total": sw.total}

    def _disks(self, now: float, dt: float) -> dict:
        out = {"read": 0.0, "write": 0.0, "drives": self.drives}
        if not psutil:
            return out
        try:
            io = psutil.disk_io_counters()
            if io and self.disk and dt > 0:
                out["read"] = max(0, io.read_bytes - self.disk[0]) / dt
                out["write"] = max(0, io.write_bytes - self.disk[1]) / dt
            if io:
                self.disk = (io.read_bytes, io.write_bytes)
        except Exception:
            pass
        if now - self.drives_at > 30 or not self.drives:
            self.drives_at = now
            drives, seen = [], set()
            try:
                for part in psutil.disk_partitions(all=False):
                    if part.device in seen or part.fstype in ("squashfs", "tmpfs", "devtmpfs", "overlay", "") \
                            or part.mountpoint.startswith(("/snap", "/boot", "/var/lib", "/run", "/proc", "/sys")):
                        continue
                    seen.add(part.device)
                    try:
                        u = psutil.disk_usage(part.mountpoint)
                    except OSError:
                        continue
                    drives.append({"mount": part.mountpoint, "device": part.device, "fs": part.fstype,
                                   "used": u.used, "total": u.total, "percent": u.percent})
            except Exception:
                pass
            self.drives = drives
            out["drives"] = drives
        return out

    def _network(self, dt: float) -> dict:
        out = {"down": 0.0, "up": 0.0, "recv": 0, "sent": 0}
        if not psutil:
            return out
        try:
            c = psutil.net_io_counters()
        except Exception:
            return out
        if self.net_start is None:
            self.net_start = (c.bytes_recv, c.bytes_sent)
        if self.net and dt > 0:
            out["down"] = max(0, c.bytes_recv - self.net[0]) / dt
            out["up"] = max(0, c.bytes_sent - self.net[1]) / dt
        self.net = (c.bytes_recv, c.bytes_sent)
        out["recv"], out["sent"] = c.bytes_recv - self.net_start[0], c.bytes_sent - self.net_start[1]
        return out

    @staticmethod
    def _battery() -> dict | None:
        if not psutil or not hasattr(psutil, "sensors_battery"):
            return None
        try:
            b = psutil.sensors_battery()
        except Exception:
            return None
        if b is None:
            return None
        left = b.secsleft if isinstance(b.secsleft, (int, float)) and b.secsleft >= 0 else None
        return {"percent": b.percent, "plugged": bool(b.power_plugged), "secs_left": left}

    # ── GPUs ────────────────────────────────────────────────────────────────────
    def _gpus(self, pdh_adapters: dict | None) -> list[dict]:
        gpus = nvidia_gpus()
        if not IS_WIN:
            gpus = _sysfs_cards() + gpus
        elif pdh_adapters:
            gpus += self._win_other_gpus(gpus, pdh_adapters)
        # the graphics card first, the built-in one after it
        gpus.sort(key=lambda g: (bool(g.get("integrated")), -(g.get("vram_total") or 0)))
        return gpus

    def _win_other_gpus(self, nv: list, adapters: dict) -> list[dict]:
        if self.win_adapters is None:
            self.win_adapters = _win_adapters()
        others = [a for a in self.win_adapters if "nvidia" not in a["name"].lower()]
        # the adapters with VRAM in use are real ones; the NVIDIA one is already counted
        luids = sorted(adapters.items(), key=lambda kv: -(kv[1].get("vram_used") or 0))
        if nv:
            luids = luids[1:] if len(luids) > len(others) else luids
        out = []
        for i, (luid, a) in enumerate(luids[:len(others) or 1]):
            info = others[i] if i < len(others) else {"name": "GPU", "vram_total": None}
            total = info.get("vram_total") or None
            out.append({"id": f"win:{luid}", "vendor": info["name"].split()[0], "driver": "windows",
                        "name": info["name"], "load": a["load"], "vram_used": a.get("vram_used"),
                        "vram_total": total, "temp": None, "power": None, "power_limit": None, "clock": None,
                        "mem_clock": None, "fan": None, "encoder": None,
                        "integrated": bool(total and total < 1.5 * GiB) or "Intel" in info["name"]})
        return out

    # ── processes ───────────────────────────────────────────────────────────────
    def _meta(self, p) -> tuple:
        try:
            key = (p.pid, p.create_time())
        except Exception:
            key = (p.pid, 0)
        m = self.proc_meta.get(key)
        if m is not None:
            return m
        name = exe = user = ""
        cmd: list = []
        try:
            name = p.name()
        except Exception:
            pass
        try:
            exe = p.exe()
        except Exception:
            pass
        try:
            cmd = p.cmdline()
        except Exception:
            pass
        try:
            user = p.username()
        except Exception:
            pass
        display = display_name(name, exe, cmd)
        m = (display, exe, cmd, user, name)
        self.proc_meta[key] = m
        return m

    def _processes(self, now: float, gpu_procs: dict) -> list[dict]:
        if not psutil:
            return []
        out = []
        alive = set()
        own_io = {}
        for p in psutil.process_iter():
            try:
                with p.oneshot():
                    display, exe, cmd, user, name = self._meta(p)
                    cpu = p.cpu_percent(None)
                    mem = p.memory_info()
                    rss = getattr(mem, "private", 0) or mem.rss if IS_WIN else mem.rss
                    threads = p.num_threads()
                    try:
                        nice = p.nice()
                    except Exception:
                        nice = None
                    status = p.status()
                    ppid = p.ppid()
            except (psutil.NoSuchProcess, psutil.ZombieProcess):
                continue
            except psutil.AccessDenied:
                display, exe, cmd, user, name = self._meta(p)
                cpu, rss, threads, nice, status, ppid = 0.0, 0, 0, None, "?", 0
            except Exception:
                continue
            if p.pid == 0 and IS_WIN:                     # the System Idle Process is not a program
                continue
            if not IS_WIN and (p.pid == 2 or ppid == 2):     # Linux kernel threads (kworker, rcu…): not apps
                continue
            alive.add(p.pid)
            disk = None
            try:
                io = p.io_counters()
                total = io.read_bytes + io.write_bytes
                old = self.proc_io.get(p.pid)
                if old and now > old[1]:
                    disk = max(0, total - old[0]) / (now - old[1])
                own_io[p.pid] = (total, now)
            except Exception:
                pass
            g = gpu_procs.get(p.pid, {})
            out.append({"pid": p.pid, "name": display, "proc": name, "exe": exe, "cmd": cmd, "user": user,
                        "cpu": cpu, "mem": rss, "threads": threads, "nice": nice, "status": status, "ppid": ppid,
                        "disk": disk, "gpu": g.get("gpu", 0.0), "vram": g.get("vram", 0)})
        self.proc_io = own_io
        if len(self.proc_meta) > 4 * max(100, len(alive)):
            self.proc_meta = {k: v for k, v in self.proc_meta.items() if k[0] in alive}
        return out

    # ── everything ──────────────────────────────────────────────────────────────
    def sample(self, procs: bool = False, proc_gpu: bool = True) -> dict:
        now = time.monotonic()
        dt = now - self.t if self.t else 0.0
        self.t = now
        gpu_procs: dict = {}
        pdh_adapters = None
        drm_cards: dict = {}
        if IS_WIN and not self.pdh_failed:
            try:
                if self.pdh is None:
                    self.pdh = _Pdh()
                gpu_procs, pdh_adapters = self.pdh.read()
            except Exception:
                log.warning("Windows GPU counters could not be read", exc_info=True)
                self.pdh_failed = True
        gpus = self._gpus(pdh_adapters)
        if procs and proc_gpu:
            if any(g["vendor"] == "NVIDIA" for g in gpus):
                nv = nvidia_processes()
                if IS_WIN:                               # PDH already has every GPU; NVIDIA's VRAM is the real one
                    for pid, v in nv.items():
                        gpu_procs.setdefault(pid, {"gpu": 0.0, "vram": 0})["vram"] = max(
                            gpu_procs[pid]["vram"], v["vram"])
                else:
                    gpu_procs = nv
        if not IS_WIN and self.drm is not None and psutil and any(g["driver"] != "nvidia" for g in gpus) \
                and (procs and proc_gpu or any(g["load"] is None and g["driver"] != "nvidia" for g in gpus)):
            try:
                drm, drm_cards = self.drm.read(psutil.pids())
                for pid, v in drm.items():
                    p = gpu_procs.setdefault(pid, {"gpu": 0.0, "vram": 0})
                    p["gpu"] = min(100.0, p["gpu"] + v["gpu"])
                    p["vram"] += v["vram"]
            except Exception:
                log.debug("DRM fdinfo could not be read", exc_info=True)
        for g in gpus:                                    # Intel has no load file: the fdinfo total stands in
            if g["load"] is None and g.get("pci") in drm_cards:
                g["load"] = drm_cards[g["pci"]]
        sensors = self._sensors(now)
        cpu = self._cpu()
        cpu["temp"] = self.cpu_temp(sensors)
        out = {"time": time.time(), "dt": dt, "cpu": cpu, "mem": self._memory(), "gpus": gpus,
               "disk": self._disks(now, dt), "net": self._network(dt), "battery": self._battery(),
               "sensors": sensors, "system": self._system()}
        if procs:
            out["procs"] = self._processes(now, gpu_procs)
            out["system"]["procs"] = len(out["procs"])
            out["system"]["threads"] = sum(p["threads"] for p in out["procs"])
        return out

    @staticmethod
    def _system() -> dict:
        out = {"uptime": None, "procs": None, "threads": None}
        if psutil:
            try:
                out["uptime"] = time.time() - psutil.boot_time()
            except Exception:
                pass
            try:
                out["procs"] = len(psutil.pids())
            except Exception:
                pass
        return out

    def close(self):
        if self.pdh is not None:
            try:
                self.pdh.close()
            except Exception:
                pass


def main_gpu(gpus: list[dict]) -> dict | None:
    """The graphics card that matters: a real one before the one built into the CPU, the most VRAM first."""
    return gpus[0] if gpus else None


def cpu_name() -> str:
    if IS_WIN:
        try:
            import winreg
            k = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0")
            return str(winreg.QueryValueEx(k, "ProcessorNameString")[0]).strip()
        except OSError:
            return ""
    m = re.search(r"^model name\s*:\s*(.+)$", _read("/proc/cpuinfo"), re.M)
    return m.group(1).strip() if m else ""


def display_name(name: str, exe: str, cmd: list) -> str:
    """What to call a process: the script for interpreters ("python · bot.py"), the program otherwise."""
    cmd0 = os.path.basename(cmd[0].replace("\\", "/")).split(" ")[0] if cmd and cmd[0] else ""
    base = name or (os.path.basename(exe) if exe else "") or cmd0
    if cmd0.lower().endswith(".exe") and not base.lower().endswith(".exe"):
        base = cmd0                                   # a Windows program under Wine (its thread renamed it)
    elif len(name) == 15 and cmd0.startswith(name):   # Linux cuts names at 15 letters
        base = cmd0
    stem = re.sub(r"(\.exe|\d+(\.\d+)*)$", "", base.lower())
    if stem in INTERPRETERS:
        if "-m" in cmd:
            i = cmd.index("-m")
            if i + 1 < len(cmd):
                return f"{base} · {cmd[i + 1]}"
        for arg in cmd[1:]:
            if arg.lower() in ("-c", "-e", "/c", "/k", "-command", "-encodedcommand", "--eval"):
                break                                 # inline code, not a script
            if arg.startswith("-"):
                continue
            script = os.path.basename(arg.replace("\\", "/"))
            if script:
                return f"{base} · {script}"
            break
    return base or "?"
