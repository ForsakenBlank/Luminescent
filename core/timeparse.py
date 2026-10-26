"""
Reading times people type: "5m", "1h30", "1h 30m", "90s", "1:30:00", "2.5h", "17:30", "5:30pm".

Used by the timer ("type the time straight into the panel"), quick reminders in the command
palette and the calculator's date maths.
"""
import re
from datetime import datetime, timedelta

_UNITS = {
    "s": 1, "sec": 1, "secs": 1, "second": 1, "seconds": 1,
    "m": 60, "min": 60, "mins": 60, "minute": 60, "minutes": 60,
    "h": 3600, "hr": 3600, "hrs": 3600, "hour": 3600, "hours": 3600,
    "d": 86400, "day": 86400, "days": 86400,
    "w": 604800, "wk": 604800, "week": 604800, "weeks": 604800,
}
_PART = re.compile(r"(\d+(?:[.,]\d+)?)\s*([a-z]+)?", re.I)


def parse_duration(text: str, bare_unit: int = 60) -> float | None:
    """Seconds in a duration like "1h30", "1h 30m", "90s", "2.5h", "1:30:00" (h:m:s) or "25:00" (m:s).
    A bare number counts in `bare_unit` seconds (minutes by default). None if it is not a duration."""
    t = (text or "").strip().lower()
    if not t:
        return None
    if re.fullmatch(r"\d+:\d{1,2}(:\d{1,2})?", t):
        parts = [int(x) for x in t.split(":")]
        if len(parts) == 3:
            return parts[0] * 3600 + parts[1] * 60 + parts[2]
        return parts[0] * 60 + parts[1]
    total, pos, last_unit = 0.0, 0, 0
    t = t.replace(" and ", " ")
    for m in _PART.finditer(t):
        if t[pos:m.start()].strip(" ,"):
            return None                           # something that is not a number+unit in between
        num = float(m.group(1).replace(",", "."))
        unit = (m.group(2) or "").lower()
        if unit:
            if unit not in _UNITS:
                return None
            secs = _UNITS[unit]
        elif last_unit:
            secs = {3600: 60, 60: 1, 86400: 3600, 604800: 86400}.get(last_unit, 1)   # "1h30" = 1 h 30 m
        else:
            secs = bare_unit
        total += num * secs
        last_unit = secs
        pos = m.end()
    if pos == 0 or t[pos:].strip():
        return None
    return total


def parse_clock(text: str, now: datetime | None = None) -> datetime | None:
    """The next time a clock time comes round: "17:30", "5:30pm", "9am", "9" (today, else tomorrow)."""
    now = now or datetime.now()
    m = re.fullmatch(r"(\d{1,2})(?:[:.](\d{2}))?\s*(am|pm)?", (text or "").strip().lower())
    if not m:
        return None
    h, mi, ampm = int(m.group(1)), int(m.group(2) or 0), m.group(3)
    if ampm:
        if not 1 <= h <= 12:
            return None
        h = (h % 12) + (12 if ampm == "pm" else 0)
    if h > 23 or mi > 59:
        return None
    due = now.replace(hour=h, minute=mi, second=0, microsecond=0)
    return due if due > now else due + timedelta(days=1)


def fmt_secs(secs: float) -> str:
    """90 -> "1m 30s", 5400 -> "1h 30m"."""
    secs = int(round(secs))
    h, rem = divmod(secs, 3600)
    m, s = divmod(rem, 60)
    if h:
        return f"{h}h {m:02d}m" if m else f"{h}h"
    if m:
        return f"{m}m {s:02d}s" if s else f"{m}m"
    return f"{s}s"
