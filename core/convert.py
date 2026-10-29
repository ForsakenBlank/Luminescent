"""
Conversions and date maths for the calculator.

    5 km to mi        20 °c in f        2.5 gb to mb        60 mph to kmh        3 st to kg
    20 usd to eur     (rates from the European Central Bank via frankfurter, fetched once a day)
    today + 45 days   2026-12-25 - today   days until 25/12/2026   days between 1/1/2026 and 1/3/2026

Pints and gallons are UK ones (uspt / usgal for US). Dates are day first (25/12/2026) or ISO (2026-12-25).
"""
import os
import re
import json
import time
import logging
import threading
import urllib.request
from datetime import date, timedelta

from core import DATA_DIR, save_json, load_json

log = logging.getLogger("convert")
RATES_PATH = os.path.join(DATA_DIR, "currency.json")
RATE_URLS = ["https://api.frankfurter.dev/v1/latest?base=EUR", "https://api.frankfurter.app/latest?from=EUR"]


class ConvertError(ValueError):
    pass


# ── units ─────────────────────────────────────────────────────────────────────
# dimension -> {alias: factor to the dimension's base unit}
_DIMS = {
    "length": {"mm": 0.001, "millimetre": 0.001, "millimetres": 0.001, "cm": 0.01, "centimetre": 0.01,
               "centimetres": 0.01, "m": 1, "metre": 1, "metres": 1, "meter": 1, "meters": 1, "km": 1000,
               "kilometre": 1000, "kilometres": 1000, "in": 0.0254, "inch": 0.0254, "inches": 0.0254, '"': 0.0254,
               "ft": 0.3048, "foot": 0.3048, "feet": 0.3048, "yd": 0.9144, "yard": 0.9144, "yards": 0.9144,
               "mi": 1609.344, "mile": 1609.344, "miles": 1609.344, "nmi": 1852, "um": 1e-6, "nm": 1e-9},
    "mass": {"mg": 1e-6, "g": 0.001, "gram": 0.001, "grams": 0.001, "kg": 1, "kilo": 1, "kilos": 1,
             "kilogram": 1, "kilograms": 1, "t": 1000, "tonne": 1000, "tonnes": 1000, "oz": 0.028349523125,
             "ounce": 0.028349523125, "ounces": 0.028349523125, "lb": 0.45359237, "lbs": 0.45359237,
             "pound": 0.45359237, "pounds": 0.45359237, "st": 6.35029318, "stone": 6.35029318},
    "time": {"ms": 0.001, "s": 1, "sec": 1, "secs": 1, "second": 1, "seconds": 1, "min": 60, "mins": 60,
             "minute": 60, "minutes": 60, "h": 3600, "hr": 3600, "hrs": 3600, "hour": 3600, "hours": 3600,
             "day": 86400, "days": 86400, "week": 604800, "weeks": 604800, "wk": 604800,
             "month": 2629746, "months": 2629746, "year": 31556952, "years": 31556952, "yr": 31556952},
    "data": {"bit": 0.125, "bits": 0.125, "b": 1, "byte": 1, "bytes": 1, "kb": 1e3, "mb": 1e6, "gb": 1e9,
             "tb": 1e12, "pb": 1e15, "kib": 1024, "mib": 1024 ** 2, "gib": 1024 ** 3, "tib": 1024 ** 4},
    "volume": {"ml": 0.001, "cl": 0.01, "dl": 0.1, "l": 1, "litre": 1, "litres": 1, "liter": 1, "liters": 1,
               "tsp": 0.00492892, "tbsp": 0.0147868, "cup": 0.236588, "cups": 0.236588, "floz": 0.0295735,
               "pt": 0.568261, "pint": 0.568261, "pints": 0.568261, "gal": 4.54609, "gallon": 4.54609,
               "gallons": 4.54609, "uspt": 0.473176, "usgal": 3.785411784, "m3": 1000, "cm3": 0.001},
    "area": {"mm2": 1e-6, "cm2": 1e-4, "m2": 1, "km2": 1e6, "ha": 1e4, "hectare": 1e4, "hectares": 1e4,
             "acre": 4046.8564224, "acres": 4046.8564224, "ft2": 0.09290304, "sqft": 0.09290304,
             "in2": 0.00064516, "mi2": 2589988.110336},
    "speed": {"m/s": 1, "mps": 1, "km/h": 1 / 3.6, "kmh": 1 / 3.6, "kph": 1 / 3.6, "mph": 0.44704,
              "kn": 0.514444, "knot": 0.514444, "knots": 0.514444, "ft/s": 0.3048},
    "energy": {"j": 1, "kj": 1000, "cal": 4.184, "kcal": 4184, "wh": 3600, "kwh": 3.6e6},
    "power": {"w": 1, "kw": 1000, "hp": 745.699872},
    "pressure": {"pa": 1, "kpa": 1000, "bar": 1e5, "mbar": 100, "psi": 6894.757293, "atm": 101325},
    "angle": {"deg": 0.017453292519943295, "degree": 0.017453292519943295, "degrees": 0.017453292519943295,
              "rad": 1, "radian": 1, "radians": 1},
}
UNITS = {alias: (dim, f) for dim, table in _DIMS.items() for alias, f in table.items()}
TEMPS = {"c": "c", "°c": "c", "celsius": "c", "f": "f", "°f": "f", "fahrenheit": "f", "k": "k", "kelvin": "k"}
SYMBOLS = {"$": "usd", "€": "eur", "£": "gbp", "¥": "jpy"}
_TOKEN = r"[a-zA-Z°\"$€£¥][\w°/]*"
_CONV = re.compile(rf"^(?P<expr>.*?)\s*(?P<src>{_TOKEN})\s+(?:to|in|as|->|=>)\s+(?P<dst>{_TOKEN})$")


def _to_kelvin(v: float, u: str) -> float:
    return v + 273.15 if u == "c" else (v - 32) * 5 / 9 + 273.15 if u == "f" else v


def _from_kelvin(v: float, u: str) -> float:
    return v - 273.15 if u == "c" else (v - 273.15) * 9 / 5 + 32 if u == "f" else v


def convert(text: str, evaluate, rates: dict | None):
    """(value, unit label) for "5 km to mi", "20 usd in eur"... None when the text is not a conversion.

    `evaluate(expr)` works out the amount, so "2*5 km to mi" and "rent eur to usd" work too."""
    t = text.strip()
    for sym, code in SYMBOLS.items():            # "$20 to eur" -> "20 usd to eur"
        t = re.sub(rf"^\{sym}\s*([\d.,]+)", rf"\1 {code}", t)
        t = re.sub(rf"\s\{sym}$", f" {code}", t)
    m = _CONV.match(t)
    if not m:
        return None
    src, dst = m.group("src").lower(), m.group("dst").lower()
    src, dst = SYMBOLS.get(src, src), SYMBOLS.get(dst, dst)
    expr = m.group("expr").strip() or "1"
    known = lambda u: u in UNITS or u in TEMPS or (rates and u.upper() in rates)
    looks_currency = lambda u: len(u) == 3 and u.isalpha()
    if not (known(src) or looks_currency(src)) or not (known(dst) or looks_currency(dst)):
        return None
    amount = evaluate(expr)
    if src in TEMPS and dst in TEMPS:
        return _from_kelvin(_to_kelvin(amount, TEMPS[src]), TEMPS[dst]), "°" + TEMPS[dst].upper() \
            if TEMPS[dst] != "k" else "K"
    if src in UNITS and dst in UNITS:
        (d1, f1), (d2, f2) = UNITS[src], UNITS[dst]
        if d1 != d2:
            raise ConvertError(f"can't convert {d1} ({src}) to {d2} ({dst})")
        return amount * f1 / f2, m.group("dst")
    if looks_currency(src) and looks_currency(dst) and src not in UNITS and dst not in UNITS:
        if not rates:
            raise ConvertError("currency rates are not loaded yet (needs internet once a day)")
        a, b = rates.get(src.upper()), rates.get(dst.upper())
        if a is None or b is None:
            raise ConvertError(f"unknown currency {(src if a is None else dst).upper()}")
        return amount / a * b, dst.upper()
    raise ConvertError(f"can't convert {src} to {dst}")


# ── currency rates ────────────────────────────────────────────────────────────
_rates: dict = {}
_lock = threading.Lock()


def rates() -> dict:
    """{CODE: units per euro}; loaded from data/currency.json, {} until the first fetch."""
    with _lock:
        if not _rates:
            saved = load_json(RATES_PATH, {})
            if isinstance(saved, dict) and isinstance(saved.get("rates"), dict):
                _rates.update(saved["rates"])
                _rates["EUR"] = 1.0
        return dict(_rates)


def rates_date() -> str:
    saved = load_json(RATES_PATH, {})
    return saved.get("date", "") if isinstance(saved, dict) else ""


def refresh_rates(force: bool = False):
    """Fetch today's rates in the background, at most once a day."""
    try:
        if not force and os.path.exists(RATES_PATH) and time.time() - os.path.getmtime(RATES_PATH) < 86400:
            rates()
            return
    except OSError:
        pass
    threading.Thread(target=_fetch, daemon=True, name="currency").start()


def _fetch():
    for url in RATE_URLS:
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "Luminescent"})
            with urllib.request.urlopen(req, timeout=15) as r:
                d = json.loads(r.read().decode("utf-8"))
            if isinstance(d.get("rates"), dict) and d["rates"]:
                save_json(RATES_PATH, {"date": d.get("date", ""), "base": "EUR", "rates": d["rates"]}, indent=1)
                with _lock:
                    _rates.clear()
                    _rates.update({k.upper(): float(v) for k, v in d["rates"].items()})
                    _rates["EUR"] = 1.0
                log.info("currency rates for %s", d.get("date"))
                return
        except Exception as e:
            log.info("currency rates from %s failed: %s", url, e)


# ── dates ─────────────────────────────────────────────────────────────────────
_DATE = r"(?:today|now|tomorrow|yesterday|\d{4}-\d{1,2}-\d{1,2}|\d{1,2}[/.]\d{1,2}[/.]\d{2,4})"
_SPAN = r"(?P<n>\d+)\s*(?P<u>days?|d|weeks?|wks?|w|months?|mo|years?|yrs?|y)"
_DATE_RULES = [
    ("shift", re.compile(rf"^(?P<a>{_DATE})\s*(?P<op>[+-])\s*{_SPAN}$", re.I)),
    ("shift", re.compile(rf"^(?:in\s+)?{_SPAN}(?:\s+from\s+(?P<a>{_DATE}|now))?$", re.I)),
    ("shift_back", re.compile(rf"^{_SPAN}\s+(?:ago|before\s+(?P<a>{_DATE}))$", re.I)),
    ("diff", re.compile(rf"^(?P<a>{_DATE})\s*-\s*(?P<b>{_DATE})$", re.I)),
    ("until", re.compile(rf"^days?\s+(?:until|till|to)\s+(?P<b>{_DATE})$", re.I)),
    ("since", re.compile(rf"^days?\s+since\s+(?P<b>{_DATE})$", re.I)),
    ("between", re.compile(rf"^days?\s+between\s+(?P<a>{_DATE})\s+and\s+(?P<b>{_DATE})$", re.I)),
    ("show", re.compile(r"^(?P<a>today|tomorrow|yesterday)$", re.I)),
]


def parse_date(s: str) -> date:
    s = (s or "today").strip().lower()
    today = date.today()
    if s in ("today", "now"):
        return today
    if s == "tomorrow":
        return today + timedelta(days=1)
    if s == "yesterday":
        return today - timedelta(days=1)
    try:
        if "-" in s:
            y, m, d = (int(x) for x in s.split("-"))
        else:
            d, m, y = (int(x) for x in re.split(r"[/.]", s))
            if y < 100:
                y += 2000
        return date(y, m, d)
    except ValueError:
        raise ConvertError(f"not a date: {s}")


def _add(d: date, n: int, unit: str) -> date:
    u = unit.lower()
    if u.startswith(("d",)):
        return d + timedelta(days=n)
    if u.startswith("w"):
        return d + timedelta(weeks=n)
    months = n * 12 if u.startswith("y") else n
    y, m = divmod(d.month - 1 + months, 12)
    y += d.year
    m += 1
    last = [31, 29 if y % 4 == 0 and (y % 100 or y % 400 == 0) else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m - 1]
    return date(y, m, min(d.day, last))


def fmt_date(d: date) -> str:
    return f"{d:%a} {d.day} {d:%b %Y}"


def _days(n: int) -> str:
    return f"{n:,} day{'s' if abs(n) != 1 else ''}" + (f" ({n / 7:.1f} weeks)" if abs(n) >= 14 else "")


def date_math(text: str):
    """(value, text) for date sums: a date gives (None, "Tue 9 Dec 2026"), a gap in days gives (45, "45 days").
    None when the text is not a date sum."""
    t = text.strip()
    for kind, rx in _DATE_RULES:
        m = rx.match(t)
        if not m:
            continue
        g = m.groupdict()
        if kind == "show":
            return None, fmt_date(parse_date(g["a"]))
        if kind in ("shift", "shift_back"):
            n = int(g["n"]) * (-1 if kind == "shift_back" or g.get("op") == "-" else 1)
            return None, fmt_date(_add(parse_date(g.get("a") or "today"), n, g["u"]))
        a = parse_date(g.get("a") or "today")
        b = parse_date(g["b"])
        if kind == "since":
            n = (date.today() - b).days
        elif kind == "diff":
            n = (a - b).days
        else:
            n = (b - a).days
        return n, _days(n)
    return None
