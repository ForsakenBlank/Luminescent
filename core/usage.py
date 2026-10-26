"""
Usage stats and the day's activity log.

  data/usage.json      how often each dashboard item, hotkey and tool was used, and when last
                       {"items": {key: {"label", "n", "last"}}, "hotkeys": {...}, "tools": {...},
                        "days": {"2026-10-26": n}}
  data/activity.jsonl  one line per thing you did: {"t", "kind", "label", "secs"?}
                       kind: run (dashboard item), tool, timer (a timer or stopwatch that ran
                       for `secs`). The Today page (focus log) reads it back.

Writes are batched (a few seconds after the last use), so a hotkey costs a dict update.
Nothing personal leaves the machine; the files are only read by the Usage and Today pages.
"""
import os
import json
import time
import logging
from datetime import datetime, date, timedelta

from PyQt6.QtCore import QTimer

from core import DATA_DIR, save_json, load_json

log = logging.getLogger("usage")

USAGE_PATH = os.path.join(DATA_DIR, "usage.json")
ACTIVITY_PATH = os.path.join(DATA_DIR, "activity.jsonl")
KEEP_DAYS = 120

_data: dict | None = None
_pending: list[str] = []
_timer: QTimer | None = None


def _load() -> dict:
    global _data
    if _data is None:
        d = load_json(USAGE_PATH, {})
        d = d if isinstance(d, dict) else {}
        for k in ("items", "hotkeys", "tools", "days"):
            if not isinstance(d.get(k), dict):
                d[k] = {}
        _data = d
    return _data


def _schedule():
    global _timer
    if _timer is None:
        _timer = QTimer()
        _timer.setSingleShot(True)
        _timer.setInterval(4000)
        _timer.timeout.connect(flush)
    if not _timer.isActive():
        _timer.start()


def record(kind: str, key: str, label: str = ""):
    """Count one use. kind: items, hotkeys or tools."""
    if not key:
        return
    d = _load()
    bucket = d.setdefault(kind, {})
    e = bucket.setdefault(key, {"label": label or key, "n": 0, "last": ""})
    e["n"] = int(e.get("n", 0)) + 1
    e["last"] = datetime.now().isoformat(timespec="seconds")
    if label:
        e["label"] = label
    today = date.today().isoformat()
    d["days"][today] = int(d["days"].get(today, 0)) + 1
    _schedule()


def activity(kind: str, label: str, secs: float | None = None, when: datetime | None = None):
    """Add a line to the activity log (the Today page)."""
    entry = {"t": (when or datetime.now()).isoformat(timespec="seconds"), "kind": kind, "label": label}
    if secs is not None:
        entry["secs"] = round(float(secs), 1)
    _pending.append(json.dumps(entry, ensure_ascii=False))
    _schedule()


def flush():
    if _timer is not None:
        _timer.stop()
    if _data is not None:
        cutoff = (date.today() - timedelta(days=KEEP_DAYS)).isoformat()
        for k in [k for k in _data["days"] if k < cutoff]:
            del _data["days"][k]
        try:
            save_json(USAGE_PATH, _data, indent=1)
        except OSError:
            log.exception("could not save usage.json")
    if _pending:
        lines = list(_pending)
        _pending.clear()
        try:
            with open(ACTIVITY_PATH, "a", encoding="utf-8") as f:
                f.write("\n".join(lines) + "\n")
        except OSError:
            log.exception("could not write activity.jsonl")
        _trim_activity()


def _trim_activity():
    """Keep about KEEP_DAYS of activity (checked at most once a day)."""
    try:
        if os.path.getsize(ACTIVITY_PATH) < 2_000_000:
            return
        cutoff = (date.today() - timedelta(days=KEEP_DAYS)).isoformat()
        with open(ACTIVITY_PATH, "r", encoding="utf-8", errors="replace") as f:
            keep = [ln for ln in f if ln[7:17] >= cutoff]
        tmp = ACTIVITY_PATH + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            f.writelines(keep)
        os.replace(tmp, ACTIVITY_PATH)
    except OSError:
        pass


def stats() -> dict:
    return _load()


def reset():
    global _data
    _data = {"items": {}, "hotkeys": {}, "tools": {}, "days": {}}
    flush()


def day_activity(day: date | None = None) -> list[dict]:
    """Every activity entry of one day (today by default), oldest first, including unsaved ones."""
    day_s = (day or date.today()).isoformat()
    out = []
    try:
        with open(ACTIVITY_PATH, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                if day_s not in line[:20]:
                    continue
                try:
                    out.append(json.loads(line))
                except ValueError:
                    pass
    except OSError:
        pass
    for line in _pending:
        try:
            e = json.loads(line)
            if e.get("t", "").startswith(day_s):
                out.append(e)
        except ValueError:
            pass
    return sorted(out, key=lambda e: e.get("t", ""))


def since(t0: float) -> float:
    return max(0.0, time.monotonic() - t0)
