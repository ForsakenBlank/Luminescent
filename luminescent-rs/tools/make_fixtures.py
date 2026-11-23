"""
Golden fixtures for the Rust rewrite, made by running the Python app's own code.

    .venv/bin/python luminescent-rs/tools/make_fixtures.py

Writes luminescent-rs/fixtures/ from scratch every time. Nothing is copied from the Python code by hand: every
expected value below comes out of the real function, so when the Python reference changes, running this again
updates the fixtures. Run it twice and `git diff` must stay empty; the clock, uuids and random bytes are all
pinned for that.

It never touches a real data folder: HOME, XDG_* and LUMINESCENT_HOME all point into a scratch folder before
anything from the app is imported, because most modules work out their paths at import time.
"""
import os
import sys
import json
import shutil
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(REPO, "luminescent-rs", "fixtures")
SCRATCH = os.path.realpath(tempfile.mkdtemp(prefix="lumfix-"))
HOME = os.path.join(SCRATCH, "home")
LUM_HOME = os.path.join(SCRATCH, "lumhome")
for d in (HOME, LUM_HOME):
    os.makedirs(d)

os.environ.update({
    "HOME": HOME,
    "LUMINESCENT_HOME": LUM_HOME,
    "LUMINESCENT_NO_HOOK": "1",
    "QT_QPA_PLATFORM": "offscreen",
    "XDG_CONFIG_HOME": os.path.join(HOME, ".config"),
    "XDG_DATA_HOME": os.path.join(HOME, ".local", "share"),
    "XDG_STATE_HOME": os.path.join(HOME, ".local", "state"),
    "LOCALAPPDATA": os.path.join(HOME, "AppData", "Local"),
    "TZ": "UTC",                     # Atlas occurrences and shell history times go through local time
    "SHELL": "/bin/fish",
})
for k in ("HISTFILE", "APPDATA", "WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE"):
    os.environ.pop(k, None)
import time  # noqa: E402
time.tzset()
sys.path.insert(0, REPO)

import re  # noqa: E402
import copy  # noqa: E402
import uuid  # noqa: E402
import random  # noqa: E402
import hashlib  # noqa: E402
import importlib  # noqa: E402
import contextlib  # noqa: E402
from datetime import datetime, date, timedelta  # noqa: E402
from types import SimpleNamespace  # noqa: E402

from PyQt6.QtWidgets import QApplication  # noqa: E402

APP = QApplication([])

import core  # noqa: E402
from core import settings as settings_mod, events, hotkeys, timeparse, convert, tones, theme, themes  # noqa: E402
from core import automation, atlas_sync, hyprbinds, terminal, backup, runner, anim  # noqa: E402
from modules import calculator, reminders, vault_store, promptlog, notes as notes_mod, performance  # noqa: E402
from ui import dashboard  # noqa: E402

# Monday 23 November 2026, 14:30. Cases that need another "now" say so.
NOW = datetime(2026, 11, 23, 14, 30, 0)
NOW_TS = NOW.timestamp()          # TZ is UTC, so this is the same everywhere


# Every file written is listed in fixtures/README.md with what made it and what it pins down.
written: list[tuple[str, str, str]] = []


def scrub(value):
    """Scratch paths become placeholders so the output does not depend on where tempfile put things."""
    if isinstance(value, str):
        return value.replace(LUM_HOME, "<LUMINESCENT_HOME>").replace(HOME, "<HOME>").replace(SCRATCH, "<SCRATCH>")
    if isinstance(value, dict):
        return {scrub(k): scrub(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [scrub(v) for v in value]
    if isinstance(value, (set, frozenset)):
        return sorted(scrub(v) for v in value)
    if isinstance(value, bytes):
        return value.hex()
    return value


def write_json(rel: str, data, made_by: str, pins: str):
    path = os.path.join(OUT, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(scrub(data), indent=1, ensure_ascii=False) + "\n")
    written.append((rel, made_by, pins))


def write_text(rel: str, text: str, made_by: str = "", pins: str = ""):
    path = os.path.join(OUT, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(scrub(text))
    if made_by:
        written.append((rel, made_by, pins))


def read_text(path: str) -> str:
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


def read_json(path: str):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


# ── pinning the clock and randomness ──────────────────────────────────────────

class FrozenDateTime(datetime):
    frozen = NOW

    @classmethod
    def now(cls, tz=None):
        return cls.frozen if tz is None else cls.frozen.replace(tzinfo=tz)

    @classmethod
    def today(cls):
        return cls.frozen


class FrozenDate(date):
    @classmethod
    def today(cls):
        return FrozenDateTime.frozen.date()


@contextlib.contextmanager
def frozen_clock(*modules, at: datetime = NOW):
    """`datetime.now()`, `date.today()` and `time.time()` all say `at` inside the listed modules."""
    saved = []
    old_frozen = FrozenDateTime.frozen
    FrozenDateTime.frozen = at
    fake_time = SimpleNamespace(**{k: getattr(time, k) for k in dir(time) if not k.startswith("_")})
    fake_time.time = lambda: at.timestamp()
    fake_time.strftime = lambda fmt, t=None: time.strftime(fmt, time.localtime(at.timestamp()) if t is None else t)
    for m in modules:
        for name, fake in (("datetime", FrozenDateTime), ("date", FrozenDate), ("time", fake_time)):
            if hasattr(m, name):
                saved.append((m, name, getattr(m, name)))
                setattr(m, name, fake)
    try:
        yield
    finally:
        for m, name, old in reversed(saved):
            setattr(m, name, old)
        FrozenDateTime.frozen = old_frozen


@contextlib.contextmanager
def seeded(seed: str):
    """uuid4, os.urandom and everything in `secrets` come from a fixed seed."""
    rng = random.Random(seed)
    saved = (uuid.uuid4, os.urandom, random._urandom)
    uuid.uuid4 = lambda: uuid.UUID(int=rng.getrandbits(128), version=4)
    os.urandom = rng.randbytes
    random._urandom = rng.randbytes
    try:
        yield rng
    finally:
        uuid.uuid4, os.urandom, random._urandom = saved


def fresh_dir(path: str) -> str:
    shutil.rmtree(path, ignore_errors=True)
    os.makedirs(path)
    return path


def outcome(fn, *args, **kw):
    """{"ok": value} or {"error": type, "message": text}: a case records an exception like any other answer."""
    try:
        return {"ok": fn(*args, **kw)}
    except Exception as e:
        return {"error": type(e).__name__, "message": str(e)}


def number(v):
    """Python ints can be bigger than any JSON reader likes and floats lose their exact text in some readers, so
    both go out as their repr."""
    if isinstance(v, bool) or v is None:
        return v
    if isinstance(v, int):
        return {"int": str(v)}
    if isinstance(v, float):
        return {"float": repr(v)}
    return v


# ── specs ─────────────────────────────────────────────────────────────────────

def schema_dump(mod) -> list:
    out = []
    for s in mod.SCHEMA:
        out.append({"key": s.key, "title": s.title, "icon": s.icon, "description": s.description,
                    "fields": [{"key": f.key, "label": f.label, "type": f.type, "default": f.default, "help": f.help,
                                "choices": [list(c) for c in f.choices], "min": f.min, "max": f.max, "step": f.step,
                                "file_filter": f.file_filter, "restart": f.restart} for f in s.fields]})
    return out


def settings_for(is_win: bool):
    """core.settings as it is on Linux or on Windows (help texts and some labels differ)."""
    core.IS_WIN = is_win
    try:
        return importlib.reload(settings_mod)
    finally:
        core.IS_WIN = sys.platform == "win32"


def specs():
    tables = {}
    for name, is_win in (("windows", True), ("linux", False)):     # linux last, so the module ends as it began
        mod = settings_for(is_win)
        tables[name] = mod
        write_json(f"settings_schema.{name}.json", {
            "shared_sections": sorted(mod.SHARED_SECTIONS), "default_profile": mod.DEFAULT_PROFILE,
            "field_types": ["hotkey", "text", "dir", "file", "int", "float", "opacity", "bool", "color", "choice",
                            "custom", "widget", "hidden"],
            "sections": schema_dump(mod)},
            "core/settings.py SCHEMA, reloaded with core.IS_WIN = " + str(is_win),
            f"every section and field on {name}: key, label, type, default, help, choices, limits, restart flag")
    write_json("settings_defaults.json", {name: mod.defaults() for name, mod in tables.items()},
               "core/settings.py defaults() on each system",
               "what settings.json holds on a first start (widget fields are never stored)")

    auto_nodes = []
    for tid, t in automation.TYPES.items():
        fields = []
        for f in t.fields:
            if callable(f.choices):
                choices = {"dynamic": f.choices.__name__}
            else:
                choices = [list(c) for c in (f.choices or [])]
            fields.append({"key": f.key, "label": f.label, "kind": f.kind, "default": f.default, "choices": choices,
                           "pin": f.pin, "hint": f.hint})
        auto_nodes.append({"id": tid, "title": t.title, "category": t.cat, "class": t.cls.__name__,
                           "ins": t.ins, "outs": t.outs, "values": t.values, "fields": fields, "help": t.help,
                           "width": t.width, "live": bool(getattr(t.cls, "live", False))})
    write_json("automation_nodes.json", {
        "categories": [{"id": c, "label": label, "colour": col} for c, label, col in automation.CATEGORIES],
        "compare_ops": [list(o) for o in automation._OPS],
        "max_text": automation.MAX_TEXT,
        "token_pattern": automation._TOKEN.pattern,
        "port_prefixes": {"flow_in": "in:", "flow_out": "out:", "value": "val:", "field": "field:"},
        "nodes": auto_nodes},
        "core/automation.py TYPES (every @node line), CATEGORIES, _OPS",
        "every node type with its ports, fields, defaults, static choices (or the name of the function that "
        "lists them at run time), help and width")

    write_json("events.json", [{"name": n, "label": label} for n, label in events.NAMES],
               "core/events.py NAMES", "the event names the bus carries and what the Automation page calls them")

    s = tables["linux"]
    write_json("pages.json", {
        "nav_sections": [list(x) for x in s.NAV_SECTIONS], "extra_pages": [list(x) for x in s.EXTRA_PAGES],
        "required_pages": list(s.REQUIRED_PAGES), "start_panels": [list(x) for x in s.START_PANELS],
        "tray_items": [list(x) for x in s.TRAY_ITEMS], "anchor_choices": [list(x) for x in s.ANCHOR_CHOICES],
        "sound_choices": [list(x) for x in s.SOUND_CHOICES], "audio_filter": s.AUDIO_FILTER,
        "animation_kinds": scrub(anim.KINDS), "animation_speeds": scrub(anim.SPEEDS),
        "animation_styles": scrub(anim.STYLES)},
        "core/settings.py page and tray tables, core/anim.py choices",
        "overlay pages, which can be in the nav bar, panels that can open on start, tray menu entries")

    pal = theme.Palette()
    write_json("theme_presets.json", {
        "presets": theme.PRESETS, "old_presets": theme.OLD_PRESETS,
        "colour_roles": [list(x) for x in theme.COLOUR_ROLES],
        "gradient_types": [list(x) for x in theme.GRADIENT_TYPES],
        "palette_defaults": {k: v for k, v in pal.__dict__.items()},
        "danger": theme.DANGER, "mono_family": theme.MONO_FAMILY,
        "look_keys": list(themes.LOOK_KEYS),
        "pack_version": themes.PACK_VERSION,
        "pack": [{"name": n, "roles": roles, "radius": r, "colours": themes.colours_from_roles(roles)}
                 for n, roles, r in themes.PACK],
        "tones_harmonies": [list(h) for h in tones.HARMONIES],
        "tones_main": [list(m) for m in tones.MAIN],
        "tones_families": scrub(tones.FAMILIES),
        "calculator_pin_colours": [list(c) for c in calculator.PIN_COLOURS]},
        "core/theme.py, core/themes.py, core/tones.py tables",
        "the built-in Dark and Light colours, the pre-redesign colours they migrate from, colour roles, the "
        "'More themes' pack with the colours it generates")

    write_json("keys.json", {
        "aliases": hotkeys._ALIASES, "modifiers": hotkeys.MODIFIERS, "modifier_keysyms": hotkeys._MOD_KEYSYMS,
        "modmask": hotkeys._MODMASK, "keysyms": hotkeys._KEYSYMS,
        "mouse_buttons": [list(b) for b in hotkeys.MOUSE_BUTTONS],
        "hypr_mod_bits": [list(b) for b in hyprbinds.MOD_BITS], "hypr_mod_names": hyprbinds._MOD_NAMES,
        "hypr_keycodes": {str(k): v for k, v in hyprbinds._KEYCODES.items()}, "hypr_mouse": hyprbinds._MOUSE,
        "hypr_actions": hyprbinds._ACTIONS, "hypr_directions": {str(k): v for k, v in hyprbinds._DIRECTIONS.items()}},
        "core/hotkeys.py and core/hyprbinds.py key tables",
        "key name aliases, xkb keysyms, modifier masks, mouse buttons, Hyprland dispatcher labels")


# ── behaviour: pure logic ─────────────────────────────────────────────────────

CALC_SUMS = [
    "1+2", "2*3+4", "2*(3+4)", "10/4", "10//4", "-7//2", "7 mod 3", "-7 mod 3", "2^10", "2**0.5", "2^-1",
    "1,000,000 + 1", "1,000.5*2", "12,34", "3 × 4 ÷ 2", "5 − 2", "50%", "200*15%", "54 + 10%", "54 - 10%",
    "(10)%", "0x1F + 1", "0b101", "0o17", "0xff & 0x0f", "6 | 1", "6 xor 3", "6 and 3", "6 or 1", "1 << 10",
    "1024 >> 3", "~5", "1.5 & 1", "1 << 5000", "sqrt(16)", "sqrt(2)", "cbrt(-27)", "abs(-3.5)", "round(3.14159, 2)",
    "round(2.5)", "round(3.5)", "floor(-2.5)", "ceil(2.1)", "sin(30)", "cos(60)", "tan(45)", "asin(0.5)",
    "acos(0.5)", "atan(1)", "ln(e)", "log(1000)", "log2(8)", "exp(1)", "fact(10)", "factorial(20)", "fact(2.5)",
    "fact(-1)", "fact(1001)", "min(3, 1, 2)", "max(3, 1, 2)", "hypot(3, 4)", "pi", "tau", "e", "2pi", "2*pi",
    "9**9**9", "10^5000", "1.0001^100000", "1/0", "0/0", "sqrt(-1)", "log(0)", "", "   ", "1+", "foo", "x+1",
    "__import__('os')", "(1).real", "[1, 2]", "'a'", "True", "1e3", "1.5e-3", "1e309", "0.1+0.2",
    "123456789*987654321", "2^64", "-2^63", "1/3", "2/3*3", "100000000000000000000", "1e15", "1e-11", "1e-9",
    "ans*2", "rent/4", "v_2026_bonus + 1", "ans", "1 2", "(", ")",
]

FMT_VALUES = [0, 1, -1, 1234, -1234567, 10 ** 15, 10 ** 20, 0.5, 1 / 3, -2 / 3, 1234.5678, 0.1 + 0.2, 1e15, 1.5e15,
              1e-11, 1e-10, 2.5e-7, 123456789.123456789, 255, -255, 3.0, 1e300, 2 ** 64, 2 ** 64 - 1]


def calc_cases():
    variables = {"ans": 12, "rent": 950 * 12, "v_2026_bonus": 7.5}
    sums = []
    for deg in (True, False):
        calculator._angle["deg"] = deg
        for s in CALC_SUMS:
            r = outcome(calculator.evaluate, s, variables)
            if "ok" in r:
                r["ok"] = number(r["ok"])
            sums.append({"sum": s, "degrees": deg, "prepared": calculator._prepare(s), **r})
    calculator._angle["deg"] = True

    fmts = []
    for v in FMT_VALUES:
        for decimals in (10, 2, 0):
            for thousands in (True, False):
                for base in ("dec", "hex", "bin", "oct"):
                    fmts.append({"value": number(v), "decimals": decimals, "thousands": thousands, "base": base,
                                 "text": calculator.fmt(v, decimals, thousands, base)})

    names = ["Rent", "rent", "my rent!", "2026 bonus", "  spaced  out ", "Ünïcode", "", "___", "a-b-c", "pi"]
    pins = [{"name": n, "var": calculator.pin_name(n)} for n in names]

    # The panel's compute(): continuation from the last answer, pins as variables, conversions and dates.
    class Calc:
        compute = calculator.CalculatorModule.compute
        continued = calculator.CalculatorModule.continued
        variables = calculator.CalculatorModule.variables
        fmt = calculator.CalculatorModule.fmt
        base = calculator.CalculatorModule.base
        plain = calculator.CalculatorModule.plain

        def __init__(self, base="dec", currency=True):
            self.data = {"history": [{"expr": "6*7", "result": "42", "value": 42, "t": "2026-11-23 14:00"}],
                         "pins": [{"name": "Rent", "var": "rent", "value": 11400, "result": "11,400"}],
                         "panel": {"base": base}}
            self.settings = {"decimals": 10, "thousands": True, "currency": currency}

        def s(self, key, default=None):
            return self.settings.get(key, default)

    rates = fixed_rates()
    computes = []
    with frozen_clock(convert, calculator):
        saved_rates = convert._rates
        convert._rates = rates
        try:
            for base in ("dec", "hex"):
                for text in ["*2", "+8", "/0", "^2", "-1", "ans+1", "rent/4", "rent / 12 + ans", "5 km to mi",
                             "2*5 km to mi", "100 usd in eur", "$20 to gbp", "rent eur to usd", "1 xyz to abc",
                             "5 kg to m", "100 c to f", "0 k to c", "today", "tomorrow", "today + 45 days",
                             "days until 25/12/2026", "1/1/2027 - 2026-11-23", "3 weeks ago", "1 month from 31/1/2026"]:
                    c = Calc(base)
                    r = outcome(c.compute, text)
                    if "ok" in r:
                        value, result, expr = r["ok"]
                        r["ok"] = {"value": number(value), "result": result, "expr": expr}
                    computes.append({"text": text, "base": base, **r})
            plain = []
            for h in ({"value": 1234567.5, "result": "1,234,567.5"}, {"value": 255, "result": "255"},
                      {"value": None, "result": "Tue 9 Dec 2026"}, {"value": 3.1, "result": "3.1 mi"}):
                for base in ("dec", "hex", "bin"):
                    plain.append({"entry": {**h, "value": number(h["value"])}, "base": base, "plain": Calc(base).plain(h)})
        finally:
            convert._rates = saved_rates

    write_json("calc_cases.json", {
        "limits": {"max_pow": calculator.MAX_POW, "max_fact": calculator.MAX_FACT},
        "functions": sorted(calculator.FUNCS), "constants": {k: number(v) for k, v in calculator.CONSTS.items()},
        "reserved": sorted(calculator.RESERVED), "variables_for_sums": {k: number(v) for k, v in variables.items()},
        "sums": sums, "fmt": fmts, "pin_names": pins, "compute": computes, "plain": plain,
        "compute_setup": {"history": "one row: 6*7 = 42", "pins": "rent = 11400", "today": NOW.date().isoformat(),
                          "rates": "currency.json in sample_home"}},
        "modules/calculator.py evaluate, fmt, pin_name, CalculatorModule.compute/plain; trig in degrees and radians",
        "sum -> value or the exact error message; result text in every base, decimals and thousands setting; "
        "continuing from ans, pins as variables, conversions and date sums through the panel")


def fixed_rates() -> dict:
    return {"EUR": 1.0, "USD": 1.08, "GBP": 0.85, "JPY": 162.5, "CHF": 0.95, "SEK": 11.4, "INR": 90.1}


def convert_cases():
    rates = fixed_rates()
    plain_eval = lambda e: calculator.evaluate(e, {"rent": 1000})
    conv = []
    for text in ["5 km to mi", "5km to mi", "1 mile in km", "3 ft to m", "12 in to cm", '12 " to cm', "1 kg to lb",
                 "1 st to kg", "1 l to floz", "1 gal to l", "1 usgal to l", "1 m2 to sqft", "1 acre to ha",
                 "100 km/h to mph", "30 knots to km/h", "1 kwh to j", "1 kcal to kj", "1 hp to kw", "1 bar to psi",
                 "1 atm to kpa", "180 deg to rad", "1 day to hours", "1 year to days", "1 gib to mb", "8 bits to b",
                 "100 c to f", "32 f to c", "0 k to c", "-40 °c to °f", "100 celsius to kelvin", "5 km to kg",
                 "2*5 km to mi", "rent eur to usd", "100 usd in eur", "100 usd -> gbp", "100 usd => jpy",
                 "100 usd as chf", "$20 to eur", "€5 to $", "£10 to usd", "¥1000 to eur", "20 abc to usd",
                 "20 usd to xyz", "5 km", "to mi", "km to mi", "5 km to", "hello world", "5 KM TO MI", "5 Km to Mi",
                 "1 mm to nm", "1 um to mm", "5 km to mi to ft"]:
        for with_rates in (True, False):
            r = outcome(convert.convert, text, plain_eval, rates if with_rates else {})
            if "ok" in r and r["ok"] is not None:
                r["ok"] = [number(r["ok"][0]), r["ok"][1]]
            conv.append({"text": text, "rates": with_rates, **r})

    dates = []
    with frozen_clock(convert):
        for text in ["today", "now", "tomorrow", "yesterday", "Today", "today + 45 days", "today+1d", "today - 2 weeks",
                     "2026-01-31 + 1 month", "31/1/2026 + 1 month", "29/2/2024 + 1 year", "2024-02-29 + 4 years",
                     "in 3 days", "3 days", "10 weeks from 1/1/2026", "2 months from now", "5 years ago",
                     "2 weeks before 2026-12-25", "2026-12-25 - 2026-11-23", "1/1/2027 - today", "25/12/26 - today",
                     "days until 25/12/2026", "day till 2027-01-01", "days to tomorrow", "days since 1/1/2026",
                     "days between 2026-01-01 and 2026-12-31", "days between 1.1.2026 and 1.3.2026",
                     "31/2/2026 + 1 day", "2026-13-01 + 1 day", "today + 1 fortnight", "1 + 1", "", "today + 100000 years",
                     "in 1 mo", "in 2 yrs", "in 3 wks", "today + 0 days", "10/10/10 + 1 day"]:
            r = outcome(convert.date_math, text)
            if "ok" in r and r["ok"] is not None:
                r["ok"] = [number(r["ok"][0]), r["ok"][1]]
            dates.append({"text": text, **r})
        parsed = [{"text": t, **{k: (str(v) if k == "ok" else v) for k, v in outcome(convert.parse_date, t).items()}}
                  for t in ["today", "tomorrow", "2026-3-7", "7/3/2026", "7.3.26", "31/2/2026", "x", "", "1/1/99"]]

    write_json("convert_cases.json", {
        "today": NOW.date().isoformat(), "rates": rates, "evaluate_variables": {"rent": 1000},
        "units": {k: [d, number(f)] for k, (d, f) in convert.UNITS.items()}, "temps": convert.TEMPS,
        "symbols": convert.SYMBOLS, "conversions": conv, "date_math": dates, "parse_date": parsed},
        "core/convert.py convert, date_math, parse_date with fixed rates and today = " + NOW.date().isoformat(),
        "unit, temperature and currency conversions with their unit label; date sums and day counts; errors")


def timeparse_cases():
    durations = []
    for text in ["5m", "5 m", "1h30", "1h 30m", "1h30m", "90s", "2.5h", "2,5h", "1:30:00", "25:00", "1:5", "5",
                 "5.5", "", "   ", "abc", "5x", "1h and 30m", "1 hour 30 minutes", "2 days", "1w2d", "1d2",
                 "1w 2", "3 hrs", "10 secs", "1:60", "1:2:3:4", "5m garbage", "m5", "1h, 30m", "0", "0s",
                 "100000000h", "1.5.5", "1 h 30", "45 min", "1 wk", "1 week", "1H30M"]:
        for bare in (60, 1):
            durations.append({"text": text, "bare_unit": bare, "seconds": timeparse.parse_duration(text, bare)})
    clocks = []
    for at in (NOW, datetime(2026, 11, 23, 23, 59, 30)):
        for text in ["17:30", "5:30pm", "9am", "9", "12am", "12pm", "0:00", "23:59", "24:00", "13pm", "0am", "9.15",
                     "9:5", "14:30", "14:29", "", "noon", " 7 pm ", "7PM"]:
            got = timeparse.parse_clock(text, at)
            clocks.append({"text": text, "now": at.isoformat(), "due": got.isoformat() if got else None})
    fmts = [{"seconds": s, "text": timeparse.fmt_secs(s)}
            for s in [0, 0.4, 0.5, 1.5, 59, 59.6, 60, 61, 90, 3599, 3600, 3660, 5400, 86400, 90061, -5]]
    write_json("timeparse_cases.json", {"units": timeparse._UNITS, "durations": durations, "clocks": clocks,
                                        "fmt_secs": fmts},
               "core/timeparse.py parse_duration, parse_clock, fmt_secs",
               "typed durations to seconds (None when not a duration), clock times to the next due moment, "
               "seconds to short text")


def reminder_cases():
    schedules = [
        {"type": "once", "date": "2026-11-23", "time": "15:00"},
        {"type": "once", "date": "2026-11-23", "time": "14:30"},
        {"type": "once", "date": "2026-11-20", "time": "09:00"},
        {"type": "once", "date": "not a date", "time": "09:00"},
        {"type": "daily", "time": "09:00"},
        {"type": "daily", "time": "14:30"},
        {"type": "daily", "time": "23:59:59"},
        {"type": "daily", "time": "bad"},
        {"type": "daily"},
        {"type": "weekly", "days": [0, 2, 4], "time": "08:00"},
        {"type": "weekly", "days": [0], "time": "14:30"},
        {"type": "weekly", "days": [], "time": "08:00"},
        {"type": "weekly", "days": [6], "time": "20:00"},
        {"type": "monthly", "monthday": 31, "time": "10:00"},
        {"type": "monthly", "monthday": 23, "time": "14:30"},
        {"type": "monthly", "monthday": 0, "time": "10:00"},
        {"type": "monthly", "monthday": 99, "time": "10:00"},
        {"type": "every_days", "every_days": 3, "date": "2026-11-01", "time": "07:00"},
        {"type": "every_days", "every_days": 2, "date": "2026-12-01", "time": "07:00"},
        {"type": "every_days", "every_days": 0, "time": "07:00"},
        {"type": "every_days", "every_days": 7, "date": "", "time": "07:00"},
        {"type": "interval", "every_minutes": 30},
        {"type": "interval", "every_minutes": 0},
        {"type": "interval", "every_minutes": 90, "window": True, "from": "09:00", "to": "17:00"},
        {"type": "interval", "every_minutes": 60, "window": True, "from": "15:00", "to": "18:00"},
        {"type": "interval", "every_minutes": 600, "window": True, "from": "09:00", "to": "17:00"},
        {"type": "interval", "every_minutes": 60, "window": True, "from": "22:00", "to": "02:00"},
        {"type": "interval", "every_minutes": 15, "window": True, "from": "22:00", "to": "14:00"},
        {"type": "startup", "startup_delay": 20},
        {"type": "nonsense"},
    ]
    afters = [NOW, datetime(2026, 11, 30, 23, 59, 59), datetime(2026, 2, 28, 12, 0), datetime(2024, 2, 29, 9, 0),
              datetime(2026, 12, 31, 23, 0)]
    nexts = []
    for sch in schedules:
        for after in afters:
            got = reminders.next_due({"schedule": sch}, after)
            nexts.append({"schedule": sch, "after": after.isoformat(), "due": got.isoformat() if got else None})
    describes = [{"schedule": sch, "text": reminders.describe({"schedule": sch})} for sch in schedules] + \
        [{"schedule": {"type": "monthly", "monthday": d, "time": "09:00"},
          "text": reminders.describe({"schedule": {"type": "monthly", "monthday": d, "time": "09:00"}})}
         for d in (1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 31)]
    quick = []
    for text in ["remind me in 20m to stretch", "in 1h30 call mum", "remind me to stretch in 20m",
                 "remind me at 17:30 to call mum", "at 9am standup", "tomorrow at 9 dentist",
                 "remind me to call mum at 17:30", "remind me to call mum tomorrow at 8am", "remind me about the oven in 5 minutes",
                 "remind me in 5", "in 5 m", "in 2 hours that the bins go out.", "remind me in 0s to x",
                 "remind me at 25:00 to x", "at 13pm lunch", "hello there", "short", "in 1:30 tea",
                 "remind me in 10 min min", "Remind Me In 10 Minutes To Do Things", "in 1h 30m nap", "in 1.5h nap",
                 "remind me that it is late at 11pm"]:
        got = reminders.parse_quick(text, NOW)
        quick.append({"text": text, "now": NOW.isoformat(), "result": [got[0].isoformat(), got[1]] if got else None})
    with seeded("reminder"), frozen_clock(reminders):
        new = reminders.new_reminder()
    write_json("reminder_cases.json", {"next_due": nexts, "describe": describes, "parse_quick": quick,
                                       "new_reminder": new, "snooze_choices": list(reminders.SNOOZE_CHOICES)},
               "modules/reminders.py next_due, describe, parse_quick, new_reminder",
               "the next time every schedule type fires after a given moment (None = never); schedule text; "
               "quick reminders typed in the palette")


def hotkey_cases():
    combos = ["", "ctrl+shift+space", "Ctrl + Shift+M", "CTRL+ALT+/", "ctrl++", "ctrl+plus", "alt", "Alt",
              "left alt", "right ctrl+x", "control+escape", "win+e", "super+return", "cmd+q", "meta+del",
              "ctrl+pgup", "ctrl+page down", "ctrl+pageup", "shift+f12", "f24", "f25", "ctrl+num 5", "num +",
              "ctrl+num enter", "mouse:275", "ctrl+mouse:276", "code:191", "ctrl+code:10", "xf86audioplay",
              "ctrl+alt+£", "ctrl+shift+a+b", "ctrl+unknownkey", "ctrl+é", "ctrl+1", "ctrl+shift", "shift+alt",
              "ctrl+alt+delete", "print screen", "ctrl+'", "ctrl+\\", "ctrl+,", "windows", "ctrl+up", "  ctrl + k  ",
              "ctrl+ins", "ctrl+apps", "menu", "ctrl+tab", "ctrl+backspace", "ctrl+~", "ctrl+mouse:999",
              "ctrl+mouse:abc", "ctrl+left windows+k"]
    rows = []
    for c in combos:
        row = {"combo": c, "normalize": hotkeys.normalize(c), "pretty_linux": hotkeys.pretty(c),
               "is_modifier": hotkeys.is_modifier(c), "keysym": hotkeys.keysym(c),
               "keysyms_of": hotkeys.keysyms_of(c), "validate": hotkeys.validate(c)}
        row["to_hyprland"] = hotkeys.to_hyprland(c) if c else []
        hotkeys.IS_WIN = True
        try:
            row["pretty_windows"] = hotkeys.pretty(c)
            row["keysym_windows"] = hotkeys.keysym(c)
        finally:
            hotkeys.IS_WIN = False
        rows.append(row)
    labels = [{"key": k, "label": hyprbinds.key_label(k)}
              for k in ["code:10", "code:19", "code:86", "code:999", "mouse:272", "mouse_down", "Q", "Return"]]
    masks = [{"mask": m, "mods": list(hyprbinds.mods_of(m))} for m in (0, 1, 4, 8, 64, 65, 77, 255)]
    write_json("hotkey_cases.json", {"combos": rows, "hypr_key_labels": labels, "hypr_mods_of": masks},
               "core/hotkeys.py normalize, pretty (Linux and Windows names), keysym, to_hyprland, validate; "
               "core/hyprbinds.py key_label, mods_of",
               "how a typed combo is stored, shown, bound in Hyprland and checked")


SAMPLE_COLOURS = ["#7B6CF6", "#F2B705", "#131317", "#FFFFFF", "#000000", "#808080", "#E5484D", "#30A46C",
                  "#3E63DD", "#CBA6F7", "#1E1E2E", "#EFF1F5", "#FE8019", "#88C0D0", "#ff00ff", "#00FFFF"]


def tones_cases():
    per = []
    for c in SAMPLE_COLOURS:
        L, C, h = tones.oklch(c)
        per.append({"colour": c, "oklch": [L, C, h], "tone": tones.tone(c), "is_light": tones.is_light(c),
                    "family": tones.family(c), "readable_on": tones.readable_on(c),
                    "readable_on_hint": tones.readable_on(c, "#7B6CF6"),
                    "with_tone_20": tones.with_tone(c, 20), "with_tone_80": tones.with_tone(c, 80),
                    "shift_tone_10": tones.shift_tone(c, 10), "shift_tone_-25": tones.shift_tone(c, -25),
                    "from_oklch_back": tones.from_oklch(L, C, h), "is_hex": tones.is_hex(c)})
    pairs = []
    for a, b in [("#7B6CF6", "#131317"), ("#FFFFFF", "#000000"), ("#F2B705", "#FFFFFF"), ("#E5484D", "#30A46C")]:
        pairs.append({"a": a, "b": b, "contrast": tones.contrast(a, b), "mix_0.25": tones.mix(a, b, 0.25),
                      "mix_0.5": tones.mix(a, b, 0.5)})
    at = [{"hue": h, "chroma": c, "tone": t, "colour": tones.at_tone(h, c, t)}
          for h in (0, 90, 145, 255, 330) for c in (0.0, 0.05, 0.2, 0.4) for t in (0, 10, 50, 90, 100)]
    bad_hex = [{"value": v, "is_hex": tones.is_hex(v)} for v in ["#abc", "#ABCDEF", "ABCDEF", "#GGGGGG", None, 5, "#abcdef0"]]
    harmonies = []
    for acc in ("#7B6CF6", "#F2B705", "#30A46C"):
        for kind, _n, _d in tones.HARMONIES:
            for light in (False, True):
                harmonies.append({"accent": acc, "kind": kind, "light": light,
                                  "main": tones.harmony(acc, kind, light)})
    harmonies.append({"accent": "#7B6CF6", "kind": "matching", "light": False, "background": "#202020",
                      "text": "#EEEEEE", "main": tones.harmony("#7B6CF6", "matching", False, "#202020", "#EEEEEE")})
    surprises = [{"seed": s, "light": light, "main": tones.surprise(light, random.Random(s))}
                 for s in (1, 2, 3) for light in (False, True)]
    dark = theme.PRESETS["dark"]
    light = theme.PRESETS["light"]
    adjust = []
    for kw in ({"hue": 40}, {"brightness": 30}, {"brightness": -30}, {"contrast_": 50}, {"contrast_": -50},
               {"saturation": 60}, {"saturation": -100}, {"warmth": 50}, {"warmth": -50},
               {"hue": 180, "brightness": 10, "contrast_": 20, "saturation": 10, "warmth": 10, "pivot": 0.5}):
        adjust.append({"colours": "dark preset", "args": kw, "result": tones.adjust(dark, **kw)})
    main = {"accent": "#7B6CF6", "accent2": "#F76B15", "accent3": "", "background": "#131317", "text": "",
            "success": "", "error": ""}
    write_json("tones_cases.json", {
        "colours": per, "pairs": pairs, "at_tone": at, "is_hex": bad_hex, "harmonies": harmonies,
        "surprise_note": "surprise() draws from random.Random(seed); a Rust port only needs the same harmony maths, "
                         "these show the shape of the result",
        "surprise": surprises,
        "adjust": adjust,
        "flip": [{"colours": n, "result": tones.flip(c)} for n, c in (("dark preset", dark), ("light preset", light))],
        "spread": [{"values": v, "light": lt, "result": tones.spread(v, lt)}
                   for v in (["#FFFFFF"] * 4, ["#101010", "#101010", "#202020"]) for lt in (False, True)],
        "follow_hue": [{"families": sorted(f), "target": t, "result": tones.follow_hue(dark, f, t, ("bg_base",))}
                       for f, t in (({"all"}, "#30A46C"), ({"blues", "purples"}, "#E5484D"))],
        "main_from_scheme": [{"scheme": s, "main": tones.main_from_scheme(s)}
                             for s in ({}, {"primary": "#cba6f7", "surface": "#141218", "onSurface": "#e6e0e9",
                                            "secondary": "#ccc2dc", "tertiary": "#efb8c8", "error": "#f2b8b5"})],
        "caelestia_scheme": [{"main": m, "scheme": tones.caelestia_scheme(m)}
                             for m in (main, {"accent": "#F2B705", "background": "#FFFFFF"})],
        "lum_roles": [{"main": m, "roles": tones.lum_roles(m)}
                      for m in (main, {"accent": "#F2B705", "background": "#FFFFFF"}, {"accent": "#CBA6F7"})],
        "main_from_roles": [{"roles": r, "main": tones.main_from_roles(r)} for r in (dark, light, {})]},
        "core/tones.py (OKLCH maths, tones, harmonies, whole-set changes, Caelestia schemes)",
        "colour maths to the last hex digit")


GRADIENTS = [
    {"type": "linear", "angle": 90, "stops": [[0, "#7b6cf6"], [1, "#F2B705"]]},
    {"type": "radial", "angle": 0, "stops": [[1, "#000000"], [0, "#FFFFFF"], [0.5, "#FF0000"]]},
    {"type": "conical", "angle": 450, "stops": [[0, "#112233"], [0.3, "#445566"], [1.2, "#778899"]]},
    {"type": "diagonal", "angle": "x", "stops": [[0, "#112233"], [1, "#445566"]]},
    {"type": "linear", "angle": -45, "stops": [[0, "#112233"], ["bad", "#445566"], [0.5, "nothex"], [1, "#FFFFFF"]]},
    {"type": "linear", "stops": [[0, "#112233"]]},
    {"type": "linear", "angle": 135, "stops": [[0.5, "#000000"], [0.5, "#FFFFFF"]]},
    "not a gradient",
    None,
]


def gradient_cases():
    cleaned = []
    for g in GRADIENTS:
        c = theme.clean_gradient(g)
        row = {"input": g, "clean": c}
        if c:
            row["at"] = {str(t): theme.gradient_at(c, t) for t in (-0.5, 0, 0.1, 0.25, 0.5, 0.75, 0.9, 1, 1.5)}
            row["mid"] = theme.gradient_mid(c)
            row["css"] = theme.gradient_css(c)
            row["css_half"] = theme.gradient_css(c, 0.5)
        cleaned.append(row)
    lines = [{"angle": a, "line": list(theme._line(a))} for a in (0, 45, 90, 135, 180, 270, 30)]
    generated = [{"accent": a, "light": lt, "colours": theme.generate(a, lt)}
                 for a in ("#7B6CF6", "#F2B705", "#808080", "#30A46C") for lt in (False, True)]
    accent = [{"accent": a, "bg": bg, "roles": theme.accent_roles(a, bg)}
              for a in ("#7B6CF6", "#FFE066") for bg in ("#131317", "#FFFFFF")]
    sections = []
    for v in ["#e5484d", "nope", {"accent": "#30a46c", "colours": {"bg_base": "#101010", "nope": "#FFFFFF",
                                                                   "border": "bad"}, "radius": 99},
              {"gradients": {"primary": GRADIENTS[0], "bg_base": GRADIENTS[5]}}, {"radius": "4"}, {}, {"radius": ""}]:
        clean = theme.clean_section(v)
        row = {"input": v, "clean": clean}
        if clean:
            p = theme.section_palette(theme.Palette(), clean)
            row["palette"] = p.__dict__
        sections.append(row)
    misc = {
        "luminance": {c: theme.luminance(c) for c in SAMPLE_COLOURS},
        "mix": [{"a": a, "b": b, "t": t, "result": theme.mix(a, b, t)}
                for a, b in (("#7B6CF6", "#FFFFFF"), ("#000000", "#FF8800")) for t in (0, 0.12, 0.5, 1)],
        "rgba": [{"colour": c, "alpha": a, "css": theme.rgba(c, a)} for c in ("#7B6CF6",) for a in (0, 0.35, 1, 2)],
        "derive_primary_text": [{"colours": c, "result": theme.derive_primary_text(c)}
                                for c in (theme.PRESETS["dark"], theme.PRESETS["light"], {"primary": "#30A46C"})],
        "colours_from_roles": [{"roles": r, "colours": themes.colours_from_roles(r)}
                               for r in ({"primary": "#CBA6F7", "bg": "#141218", "card": "#211F26"},
                                         {"primary": "#8839EF", "bg": "#EFF1F5", "text": "#4C4F69"},
                                         {"primary": "bad"}, {"primary": "#7B6CF6"})],
    }
    write_json("gradient_cases.json", {"gradients": cleaned, "linear_line": lines, "generate": generated,
                                       "accent_roles": accent, "module_sections": sections, **misc},
               "core/theme.py gradients, generate, accent_roles, clean_section, section_palette; "
               "core/themes.py colours_from_roles",
               "gradient cleaning and sampling, the Qt brush text, generated themes from one accent, per-module looks")


def automation_cases():
    compares = []
    for a, op, b in [("Hello World", "equals", "hello world"), (" x ", "equals", "x"), ("a", "not_equals", "A"),
                     ("Hello", "contains", "ELL"), ("Hello", "starts", "he"), ("Hello", "ends", "LO"),
                     ("abc123", "regex", r"\d+"), ("ABC", "regex", "abc"), ("x", "regex", "("), ("", "empty", ""),
                     ("  ", "empty", ""), ("x", "not_empty", ""), ("", "not_empty", ""), ("10", "gt", "9"),
                     ("1,5", "lt", "2"), ("2", "ge", "2.0"), ("a", "le", "1"), ("3", "le", " 3 "), ("x", "unknown", "")]:
        compares.append({"a": a, "op": op, "b": b, "result": automation.compare(a, op, b)})

    eng = SimpleNamespace(data={"vars": {"name": "Ada", "count": "3", "spaced name": "yes"}})
    expands = []
    old_clip = automation.clipboard_text
    automation.clipboard_text = lambda: "copied text"
    try:
        with frozen_clock(automation):
            for text in ["plain", "{input}", "Hi {var:name}!", "{var: name }", "{var:missing}", "{var:spaced name}",
                         "{date} {time}", "{now}", "a{newline}b", "{clipboard}", "{unknown} {input", "awk '{print $1}'",
                         "{{input}}", "{INPUT}", "{var:}", "{var:a{b}"]:
                for inp in ("", "from the pulse"):
                    expands.append({"text": text, "input": inp,
                                    "result": automation.AutomationEngine.expand(eng, text, inp)})
    finally:
        automation.clipboard_text = old_clip
    days = [{"text": t, "days": sorted(automation._days(t))}
            for t in ["", "every day", "Daily", "weekdays", "weekends", "mon", "Mon, Wed, Fri", "fri-mon", "tue-thu",
                      "monday tuesday", "nonsense", "sat-sun, mon", "wed-wed"]]
    hm = [{"text": t, "result": automation._hm(t)} for t in ["9:30", "09.05", "23:59", "24:00", "9:60", " 7:15 ", "915"]]
    fmts = [{"value": v, "text": automation._fmt(v)} for v in (0, 1.0, -3, 2.5, 1 / 3, 1e20, 123456789.0, 0.000012345)]
    write_json("automation_compare_cases.json", {"compare": compares, "days": days, "hm": hm, "number_fmt": fmts},
               "core/automation.py compare, _days, _hm, _fmt",
               "the If node's comparisons, schedule day lists, clock text and how numbers print")
    write_json("automation_expand_cases.json", {"now": NOW.isoformat(), "clipboard": "copied text",
                                                "vars": eng.data["vars"], "cases": expands},
               "core/automation.py AutomationEngine.expand with a fixed clock and clipboard",
               "{input}, {var:name}, {date}, {time}, {now}, {newline}, {clipboard}; other braces left alone")


# ── formats ───────────────────────────────────────────────────────────────────

def settings_migration_cases():
    path = os.path.join(SCRATCH, "settings-case.json")
    cases = []

    def run(name: str, saved, note: str = ""):
        if os.path.exists(path):
            os.remove(path)
        if saved is not None:
            with open(path, "w", encoding="utf-8") as f:
                f.write(saved if isinstance(saved, str) else json.dumps(saved))
        s = settings_mod.Settings(path)
        row = {"name": name, "saved": saved, "result": s.all()}
        if saved is None:
            row["written"] = read_json(path)
        if note:
            row["note"] = note
        cases.append(row)

    run("no file: defaults, written straight away", None)
    run("empty object", {})
    run("not JSON", "{this is not json", "the file is left as it is and defaults are used")
    run("a list instead of an object", "[1, 2, 3]")
    run("unknown sections and keys", {"general": {"tray_messages": False, "made_up": 1}, "nonsense": {"a": 1},
                                      "ui": "not a dict"},
        "Python drops unknown keys when it merges; the Rust store must keep them (see DATA_AND_MIGRATION.md)")
    run("nav bar from before Automation", {"ui": {"nav_order": ["dashboard", "themes", "settings"]},
                                           "general": {"tray_items": ["open", "settings", "quit"]}})
    run("nav bar not starting with Dashboard", {"ui": {"nav_order": ["themes", "dashboard", "settings"]}})
    run("nav bar that already has Automation", {"ui": {"nav_order": ["dashboard", "settings"], "hotkey_automation": ""}})
    run("panels reopened where they were", {"ui": {"panel_place": "kept"}})
    run("panels already migrated", {"ui": {"panel_place": "kept", "panel_place_v": 1}})
    run("animations from the automation section", {"automation": {"animate": False}, "ui": {"overlay_screen": "primary"}})
    run("animations section present", {"automation": {"animate": False}, "animations": {},
                                       "ui": {"overlay_screen": "primary"}})
    run("tray without Keep awake", {"general": {"tray_items": ["open", "quit"]}})
    run("tray with the keep-awake hotkey key", {"general": {"tray_items": ["open", "quit"], "keep_awake_hotkey": ""}})
    run("show-on-start boxes", {"notes": {"show_on_start": False}, "hearing": {"show_on_start": True}})
    run("show-on-start defaults", {"notes": {}})
    run("start panels already set", {"general": {"start_panels": ["sysmon"]}})
    current = settings_mod.defaults()
    current["general"]["tray_messages"] = False
    current["ui"]["text_scale"] = 1.2
    current["ui"]["panel_place_v"] = 1
    current["doodle"]["colour"] = "#FF00AA"
    run("a 1.9.21 file round-trips unchanged", current)
    write_json("settings_migration_cases.json", cases,
               "core/settings.py Settings.load (_merge + _migrate) on each saved file",
               "old or odd settings.json files and the settings they load as")


def profile_cases():
    home = fresh_dir(os.path.join(SCRATCH, "profiles-case"))
    old = (settings_mod.SETTINGS_FILE, settings_mod.PROFILES_FILE, settings_mod.PROFILES_DIR)
    settings_mod.PROFILES_FILE = os.path.join(home, "data", "profiles.json")
    settings_mod.PROFILES_DIR = os.path.join(home, "data", "profiles")
    os.makedirs(os.path.join(home, "data"))
    steps = []

    def snapshot(what: str, s):
        files = {}
        for root, _dirs, names in os.walk(home):
            for n in sorted(names):
                p = os.path.join(root, n)
                files[os.path.relpath(p, home).replace(os.sep, "/")] = read_json(p)
        steps.append({"step": what, "active": s.active_profile(), "profiles": s.profiles(), "files": files})

    try:
        with seeded("profiles"):
            s = settings_mod.Settings(os.path.join(home, "settings.json"))
            snapshot("first start", s)
            pid = s.create_profile("  Work  ")
            snapshot("created Work (a copy of the current settings)", s)
            s.set("doodle.colour", "#00FF00")
            s.set("general.tray_messages", False)
            s.save(emit=False)
            snapshot("changed doodle.colour (own) and general.tray_messages (shared)", s)
            s.switch_profile(pid, emit=False)
            snapshot("switched to Work", s)
            s.set("doodle.colour", "#123456")
            s.save(emit=False)
            s.switch_profile("default", emit=False)
            snapshot("changed doodle.colour in Work, back to Default", s)
            s.rename_profile(pid, "Office")
            s.switch_profile("nope", emit=False)
            snapshot("renamed Work to Office; switching to a missing profile does nothing", s)
            s.switch_profile(pid, emit=False)
            s.delete_profile(pid)
            snapshot("deleted the active profile: back on Default", s)
            with open(settings_mod.PROFILES_FILE, "w") as f:
                json.dump({"active": "gone", "profiles": [{"id": "x", "name": "X"}]}, f)
            snapshot("profiles.json without Default and a missing active profile", s)
    finally:
        settings_mod.SETTINGS_FILE, settings_mod.PROFILES_FILE, settings_mod.PROFILES_DIR = old
    write_json("settings_profile_cases.json", {"shared_sections": sorted(settings_mod.SHARED_SECTIONS), "steps": steps},
               "core/settings.py profiles: create, switch, rename, delete, a damaged profiles.json",
               "every file on disk after each step, and which sections follow the profile")


def themes_migration_cases():
    cases = []
    path = themes.PATH

    class Store:
        _load = themes.ThemeStore._load
        _first_run = staticmethod(themes.ThemeStore._first_run)

    def run(name: str, saved, raw_settings=None):
        if os.path.exists(path):
            os.remove(path)
        if saved is not None:
            with open(path, "w", encoding="utf-8") as f:
                json.dump(saved, f)
        with seeded("themes-" + name):
            data = Store()._load(SimpleNamespace(raw_saved=raw_settings or {}))
        cases.append({"name": name, "saved": saved, "old_appearance": (raw_settings or {}).get("appearance"),
                      "result": data, "written": read_json(path) if os.path.exists(path) else None})

    run("first start", None)
    run("first start from old Appearance (custom colours)", None,
        {"appearance": {"mode": "custom", "primary": "#FF8800", "bg_base": "#101010", "font_family": "Inter",
                        "radius": 6}})
    run("first start from old Appearance (light mode)", None, {"appearance": {"mode": "light", "radius": 4}})
    old_dark = {"id": "dark", "name": "Dark", "builtin": True, "colours": dict(theme.OLD_PRESETS["dark"]),
                "font_family": "Segoe UI"}
    run("untouched built-in from before the redesign", {"active": "dark", "themes": [old_dark], "pack": 1})
    edited = copy.deepcopy(old_dark)
    edited["colours"]["primary"] = "#00FF00"
    run("edited built-in is left alone", {"active": "dark", "themes": [edited], "pack": 1})
    run("missing keys are filled, missing Light comes back, pack added once",
        {"active": "mine", "folders": [], "themes": [{"id": "mine", "name": "Mine", "colours": {"primary": "#123456"}},
                                                      {"name": "no id"}, "not a theme"]})
    run("pack already there, active theme gone", {"active": "gone", "themes": [], "pack": 1})
    write_json("themes_migration_cases.json", cases,
               "core/themes.py ThemeStore._load (first run, old Appearance, pre-redesign colours, theme pack)",
               "themes.json as it loads and as it is written back")


def dashboard_migration_cases():
    class Module:
        def __init__(self, icon, acts):
            self.icon = icon
            self.acts = acts

        def actions(self):
            return [(a, label, None) for a, label in self.acts]

    launcher = SimpleNamespace(
        modules={"notes": Module("📝", [("toggle", "Show / hide Notes")]),
                 "timer": Module("⏱", [("new", "New timer"), ("edit", "Edit a timer")])},
        settings=SimpleNamespace(raw_saved={}))

    class Store:
        _load = dashboard.DashboardStore._load
        _first_run = dashboard.DashboardStore._first_run
        _to_screen_layout = dashboard.DashboardStore._to_screen_layout
        tools_tab = dashboard.DashboardStore.tools_tab

        def __init__(self):
            self.launcher = launcher

    cases = []

    def run(name: str, saved, raw=None):
        if os.path.exists(dashboard.PATH):
            os.remove(dashboard.PATH)
        if saved is not None:
            with open(dashboard.PATH, "w", encoding="utf-8") as f:
                json.dump(saved, f)
        launcher.settings.raw_saved = raw or {}
        with seeded("dash-" + name):
            data = Store()._load()
        cases.append({"name": name, "saved": saved, "raw_settings": raw, "result": data})

    run("first start (Tools tab from the modules' actions)", None)
    run("first start with old File Launcher shortcuts", None,
        {"file_launcher": {"shortcuts": [{"hotkey": "ctrl+alt+f", "path": "/home/me/Downloads/"},
                                         {"path": "  "}, {"path": "/usr/bin/firefox"}]}})
    run("layout 1: panel-relative tabs", {"tabs": [
        {"id": "tools", "title": "Tools", "x": 16, "y": 16, "w": 232,
         "items": [{"id": "a", "name": "📝 Show / hide Notes", "action": "notes:toggle"},
                   {"id": "b", "name": "⏱New timer", "action": "timer:new"}, {"name": "Firefox", "path": "/usr/bin/firefox"}]},
        {"title": "Mine", "x": 300, "y": 40, "items": []}]})
    run("layout 2: one tab list becomes the Main board", {"layout": 2, "tabs": [{"id": "t1", "title": "A", "x": 5}]})
    run("layout 3 with a page box, a bad board and a pinned screen", {
        "layout": 3, "active": "nope", "locked": True,
        "screens": {"DP-1": "main", "HDMI-A-1": "gone", "5": "main"},
        "boards": [{"id": "main", "name": "Main", "tabs": [{"id": "p", "kind": "page", "page": "notes"},
                                                          {"id": "q", "kind": "page", "state": "bad"}]},
                   "junk", {"tabs": []}]})
    write_json("dashboard_migration_cases.json", {"fake_modules": {k: {"icon": m.icon, "actions": m.acts}
                                                                   for k, m in launcher.modules.items()},
                                                  "cases": cases},
               "ui/dashboard.py DashboardStore._load with two stand-in modules",
               "dashboard.json layout 1 and 2 migrations, defaults filled in, first-start Tools tab")


def automation_load_cases():
    class Engine:
        _load = automation.AutomationEngine._load
        _example = automation.AutomationEngine._example
        _blank = staticmethod(automation.AutomationEngine._blank)

    cases = []

    def run(name, saved):
        if os.path.exists(automation.PATH):
            os.remove(automation.PATH)
        if saved is not None:
            with open(automation.PATH, "w", encoding="utf-8") as f:
                json.dump(saved, f)
        with seeded("auto-" + name):
            cases.append({"name": name, "saved": saved, "result": Engine()._load()})

    run("first start: the switched-off example", None)
    run("no workspaces left", {"workspaces": ["junk"], "vars": []})
    run("unknown node types and loose wires are dropped, fields get their defaults", {
        "version": 1, "active": "w1", "vars": {"x": "1"},
        "workspaces": [{"id": "w1", "name": "Mine", "nodes": [
            {"id": "a", "type": "interval", "fields": {"seconds": 2}},
            {"id": "b", "type": "no_such_node"},
            {"type": "notify", "state": "bad"}],
            "wires": [{"src": ["a", "out:tick"], "dst": ["b", "in:run"]}, {"id": "w", "src": ["a", "out:tick"],
                                                                           "dst": ["a", "in:stop"]}, "junk"]}]})
    write_json("automation_load_cases.json", cases,
               "core/automation.py AutomationEngine._load",
               "automation.json as it loads: defaults, dropped nodes and wires, the first-start example")


def vault_cases():
    root = os.path.join(OUT, "vault_fixture")
    shutil.rmtree(root, ignore_errors=True)
    src = fresh_dir(os.path.join(SCRATCH, "vault-src"))
    files = {"hello.txt": b"Hello from the Python vault.\n", "notes/todo.md": "- [ ] Rust rewrite\n- [x] fixtures\n".encode(),
             "notes/deep/data.bin": bytes(range(256)) * 3, "empty.txt": b"", "ünïcode name.txt": "café ☕\n".encode()}
    for rel, data in files.items():
        p = os.path.join(src, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(data)

    real_pw, decoy_pw = "correct horse battery staple", "decoy password"
    out_dir = os.path.join(SCRATCH, "vault-out")
    real = os.path.join(root, "real")
    with seeded("vault-real"):
        v = vault_store.VaultStore(real, out_dir)
        recovery = v.create(real_pw, decoy_pw)
        v.add_files([os.path.join(src, "hello.txt"), os.path.join(src, "notes"), os.path.join(src, "empty.txt"),
                     os.path.join(src, "ünïcode name.txt")], [], move=False)
        v.index["perm"]["hello.txt"] = "/home/me/Decrypted/hello.txt"      # inside the sealed index, so no scratch path
        v.save_index()
        real_index = copy.deepcopy(v.index)
        v.lock()
        assert v.unlock(decoy_pw) == "decoy"
        v.add_files([os.path.join(src, "hello.txt")], [], move=False)
        decoy_index = copy.deepcopy(v.index)
        v.lock()

    def blobs_of(tree, prefix=""):
        out = {}
        for name, val in tree.items():
            if isinstance(val, dict):
                out.update(blobs_of(val, prefix + name + "/"))
            else:
                out[prefix + name] = val
        return out

    def plain(rel_in_src: str) -> dict:
        data = files[rel_in_src]
        try:
            return {"text": data.decode("utf-8")} if data and b"\0" not in data else {"hex": data.hex()}
        except UnicodeDecodeError:
            return {"hex": data.hex()}

    contents = {}
    for name, blob in blobs_of(real_index["tree"]).items():
        contents[name] = {"blob": blob, **plain(name)}
    decoy_contents = {name: {"blob": blob, **plain(name)} for name, blob in blobs_of(decoy_index["tree"]).items()}

    # A vault from before the index got its "index" AAD: Rust must try with, then without, and save it back with.
    legacy = os.path.join(root, "legacy_index")
    with seeded("vault-legacy"):
        v = vault_store.VaultStore(legacy, out_dir)
        legacy_recovery = v.create("old password")
        v.add_files([os.path.join(src, "hello.txt")], [], move=False)
        key = v.key
        legacy_index = copy.deepcopy(v.index)
        vault_store._write_atomic(v.index_file, vault_store._seal(key, json.dumps(v.index).encode()))
        v.lock()

    # The pre-rewrite layout: .hash files and the password-derived key as the file key. Unlocking it migrates in
    # place, so the case keeps the "before" folder and a migrated copy.
    old = os.path.join(root, "pre_rewrite")
    os.makedirs(old)
    with seeded("vault-old"):
        salt = os.urandom(32)
        pw_key = vault_store.derive_key("ancient", salt)
        with open(os.path.join(old, ".salt"), "wb") as f:
            f.write(salt)
        with open(os.path.join(old, ".hash"), "w") as f:
            f.write(hashlib.sha256(salt + b"ancient").hexdigest())
        blob = uuid.uuid4().hex
        with open(os.path.join(old, blob), "wb") as f:
            f.write(vault_store._seal(pw_key, files["hello.txt"]))
        with open(os.path.join(old, ".meta"), "wb") as f:
            f.write(vault_store._seal(pw_key, json.dumps({"files": {"hello.txt": blob}, "decoy_files": {},
                                                          "perm_decrypted": {}}).encode()))
        migrated = os.path.join(SCRATCH, "pre_rewrite_migrated")
        shutil.copytree(old, migrated)
        v = vault_store.VaultStore(migrated, out_dir)
        mode = v.unlock("ancient")
        migrated_index = copy.deepcopy(v.index)
        migrated_recovery = getattr(v, "pending_recovery_key", "")
        migrated_files = sorted(os.listdir(migrated))

    with seeded("vault-primitives"):
        salt = bytes(range(32))
        prim = {
            "pbkdf2_rounds": vault_store.PBKDF2_ITERS,
            "derive_key": [{"password": p, "salt": salt.hex(), "key": vault_store.derive_key(p, salt).hex()}
                           for p in ("", "password", "pässwörd ☕")],
            "recovery_key_to_bytes": [{"text": t, "salt": salt.hex(),
                                       "key": vault_store.recovery_key_to_bytes(t, salt).hex()}
                                      for t in ("ABCD-EF01-2345-6789-ABCD-EF01-2345-6789",
                                                " abcd ef01 2345 6789 abcd ef01 2345 6789 ", "abcd-ef01")],
            "format_recovery_key": [{"raw": r, "text": vault_store.format_recovery_key(r)}
                                    for r in ("0123456789abcdef0123456789abcdef", "abc", "")],
            "unique_name": [{"name": n, "existing": e, "result": vault_store.unique_name(n, e)}
                            for n, e in (("a.txt", []), ("a.txt", ["a.txt"]), ("a.txt", ["a.txt", "a_1.txt"]),
                                         ("noext", ["noext"]), (".hidden", [".hidden"]))],
        }
        k = bytes(range(32))
        sealed = []
        for aad in (b"", b"master", b"index"):
            out = vault_store._seal(k, b"sealed text", aad)
            sealed.append({"key": k.hex(), "plain": "sealed text", "aad": aad.decode(), "sealed": out.hex()})
        prim["seal"] = sealed

    write_json("vault_cases.json", {
        "real": {"folder": "vault_fixture/real", "password": real_pw, "decoy_password": decoy_pw,
                 "recovery_key": recovery, "index": real_index, "files": contents,
                 "decoy_index": decoy_index, "decoy_files": decoy_contents,
                 "aad": {".master": "master", ".dmaster": "dmaster", ".recovery": "recovery", ".meta": "index",
                         ".dmeta": "index", "blobs": ""}},
        "legacy_index": {"folder": "vault_fixture/legacy_index", "password": "old password",
                         "recovery_key": legacy_recovery, "index": legacy_index,
                         "note": ".meta is sealed without AAD; opening it should succeed and save it back with AAD"},
        "pre_rewrite": {"folder": "vault_fixture/pre_rewrite", "password": "ancient", "unlock_mode": mode,
                        "index_after": migrated_index, "files_after": migrated_files,
                        "recovery_key_made": migrated_recovery,
                        "note": "unlocking migrates in place: .hash goes, .master and .recovery appear, the old "
                                "{files, perm_decrypted} index becomes {tree, perm} with AAD. Test on a copy."},
        "primitives": prim},
        "modules/vault_store.py with seeded randomness (salts, keys, nonces and the recovery key are fixed)",
        "the Rust vault must open these folders with these passwords and the recovery key, decrypt every file to "
        "these bytes, and migrate the two old layouts")
    written.append(("vault_fixture/", "modules/vault_store.py (see vault_cases.json)",
                    "real vault with a decoy, a no-AAD-index vault and a pre-rewrite .hash vault"))


class FakeSocket:
    def __init__(self, incoming: bytes = b""):
        self.sent = b""
        self.incoming = incoming

    def sendall(self, data: bytes):
        self.sent += data

    def recv(self, n: int) -> bytes:
        part, self.incoming = self.incoming[:n], self.incoming[n:]
        return part


def atlas_cases():
    old_now = atlas_sync.now_ms
    atlas_sync.now_ms = lambda: int(NOW_TS * 1000)
    try:
        with seeded("atlas"), frozen_clock(atlas_sync):
            keys = [{"code": c, "key": atlas_sync.derive_key(c).hex()} for c in ("000000", "123456", "987654")]
            key = atlas_sync.derive_key("123456")
            seals = []
            for plain in (b"{}", json.dumps({"action": "hello", "name": "Pixel"}).encode()):
                sealed = atlas_sync.seal(key, plain)
                sock = FakeSocket()
                atlas_sync.write_frame(sock, atlas_sync.SEALED, sealed)
                seals.append({"code": "123456", "plain": plain.decode(), "sealed": sealed.hex(), "frame": sock.sent.hex(),
                              "read_back": atlas_sync.unseal(key, atlas_sync.read_frame(FakeSocket(sock.sent))[1]).decode()})
            err_sock = FakeSocket()
            atlas_sync.write_frame(err_sock, atlas_sync.ERROR, b"Wrong code")
            bad_frames = []
            for name, data in (("wrong magic", b"NOPE\x00\x00\x00\x00\x00"),
                               ("negative length", b"ATLS\x00\xff\xff\xff\xff"),
                               ("too big", b"ATLS\x00" + (atlas_sync.MAX_FRAME + 1).to_bytes(4, "big")),
                               ("cut short", b"ATLS\x00\x00\x00\x00\x10abc")):
                r = outcome(atlas_sync.read_frame, FakeSocket(data))
                bad_frames.append({"name": name, "bytes": data.hex(), **{k: v for k, v in r.items() if k != "ok"}})

            records = [
                {"kind": "item", "uid": "n1", "fields": {"type": "NOTE", "name": "Ideas", "note": {"text": "x"},
                                                         "updated": 5, "created": 1}},
                {"kind": "item", "uid": "n1", "fields": {"type": "NOTE", "name": "Ideas", "note": {"text": "x"},
                                                         "updated": 99, "created": 2}},
                {"kind": "item", "uid": "n2", "fields": {"type": "NOTE", "note": {"text": "y", "markdown": False}}},
                {"kind": "item", "uid": "n2", "fields": {"type": "NOTE", "note": {"text": "y"}}},
                {"kind": "item", "uid": "n3", "fields": {"type": "NOTE", "note": {"text": "y", "markdown": True}}},
                {"kind": "event", "uid": "e1", "fields": {"title": "Ünïcode ☕", "skippedDays": None, "z": [1, {"b": 2, "a": 1}]}},
                {"kind": "event", "uid": "e1", "fields": {"title": "Ünïcode ☕", "z": [1, {"a": 1, "b": 2}]}},
                {"kind": "log", "uid": "l1", "fields": {"value": 1800.0, "durationSeconds": 1800, "note": None}},
            ]
            prints = [{"record": r, "canonical": atlas_sync.canonical(r["fields"]), "fingerprint": atlas_sync.fingerprint(r)}
                      for r in records]
            colours = [{"colour": c, "int": atlas_sync.colour_to_int(c)} for c in ("#E5484D", "e5484d", "#FFF", "", "#000000")]
            ints = [{"int": i, "colour": atlas_sync.int_to_colour(i)} for i in (None, -1702579, -16777216, 0, 16777215)]
            notes = [{"id": "n1", "name": "Ideas", "type": "note", "content": "build a boat", "items": [],
                      "colour": "#E5484D", "pinned": True, "tags": ["Work", "boat"]},
                     {"id": "n2", "name": "Shop", "type": "todo", "content": "", "tags": [], "colour": "", "pinned": False,
                      "items": [{"id": "a", "text": "milk", "checked": True, "due": ""},
                                {"id": "b", "text": "eggs", "checked": False, "due": "2026-11-24T09:00"}]}]
            texts = [{"note": n, "text": atlas_sync.note_text(n), "signature": list(atlas_sync.local_signature(n))}
                     for n in notes]
            checklists = [{"text": t, "is_checklist": atlas_sync.is_checklist(t),
                           "items": atlas_sync.checklist_items(t, notes[1]["items"])}
                          for t in ("- [x] milk\n- [ ] eggs (due 2026-11-24 09:00)\n- [X] bread", "plain text",
                                    "", "- [ ] a\nnot a line", "- [ ] trailing \n\n- [x] b")]
            day = 86_400_000
            start = int(datetime(2026, 11, 2, 9, 0).timestamp() * 1000)     # a Monday
            occ = []
            for rule, extra in (("NONE", {}), ("DAILY", {}), ("WEEKDAYS", {}), ("WEEKLY", {}), ("FORTNIGHTLY", {}),
                                ("MONTHLY", {}), ("YEARLY", {}), ("DAILY", {"repeatUntil": start + 3 * day}),
                                ("DAILY", {"skippedDays": f"{start // day + 1}, {start // day + 2}"}), ("ODD", {})):
                ev = {"startsAt": start, "repeatRule": rule, **extra}
                occ.append({"event": ev, "from": start, "to": start + 400 * day,
                            "occurrences": atlas_sync.occurrences(ev, start, start + 400 * day)[:30]})
            month_end = int(datetime(2026, 1, 31, 8, 0).timestamp() * 1000)
            leap = int(datetime(2024, 2, 29, 8, 0).timestamp() * 1000)
            for ev in ({"startsAt": month_end, "repeatRule": "MONTHLY"}, {"startsAt": leap, "repeatRule": "YEARLY"}):
                occ.append({"event": ev, "from": ev["startsAt"], "to": ev["startsAt"] + 800 * day,
                            "occurrences": atlas_sync.occurrences(ev, ev["startsAt"], ev["startsAt"] + 800 * day)})
            theme_rec = atlas_sync.atlas_theme("Dark", theme.PRESETS["dark"], True)
            roles = atlas_sync.theme_roles(theme_rec)

            # a whole conversation through Host.answer, as the phone would have it (no sockets)
            store_path = os.path.join(SCRATCH, "atlas_sync.json")
            if os.path.exists(store_path):
                os.remove(store_path)
            store = atlas_sync.Store(store_path)
            conv_notes = copy.deepcopy(notes)
            activity = [{"t": "2026-11-23T10:30:00", "kind": "timer", "label": "Coding", "secs": 1800.0},
                        {"t": "2026-11-23T11:00:00", "kind": "run", "label": "Firefox"}]
            added = []

            def write(fn):
                added.extend(fn(conv_notes))

            theme_fields = atlas_sync.atlas_theme("Dark", theme.PRESETS["dark"], True)
            host = atlas_sync.Host(store, "test-pc", local=lambda: (conv_notes, activity + added, theme_fields),
                                   write=write)
            convo = []

            def ask(request):
                reply = host.answer(request)
                convo.append({"request": request, "reply": copy.deepcopy(reply)})
                return reply

            ask({"action": "hello", "name": "Pixel", "kind": "Phone", "deviceId": "phone"})
            got = ask({"action": "records", "peerId": "phone", "name": "Pixel"})
            recs = {atlas_sync.key_of(r): r for r in got["records"]}
            log_rec = next(r for r in recs.values() if r["kind"] == "log")
            shop_edit = copy.deepcopy(recs["item/lum-n2"])
            shop_edit["fields"]["note"]["text"] = "- [x] milk\n- [x] eggs\n- [ ] bread"
            new_note = {"kind": "item", "uid": "atlas-note-1",
                        "fields": dict(atlas_sync.item_fields("NOTE", "From phone"), note={"text": "hi"})}
            phone_log = {"kind": "log", "uid": "atlas-log-1", "fields": {
                "trackerId": log_rec["fields"]["trackerId"], "timestamp": int(NOW_TS * 1000) - 3_600_000,
                "value": 1200.0, "durationSeconds": 1200, "note": None}}
            event = {"kind": "event", "uid": "ev-1", "fields": {"title": "Dentist", "startsAt": int(NOW_TS * 1000) + day,
                                                                "repeatRule": "NONE"}}
            atlas_theme = {"kind": "theme", "uid": "atlas-ocean", "fields": atlas_sync.atlas_theme("Ocean", {
                "primary": "#3E63DD", "bg_base": "#0B1020"}, True)}
            upserts = [shop_edit, new_note, phone_log, event, atlas_theme]
            final = {k: atlas_sync.fingerprint(r) for k, r in recs.items()}
            for r in upserts:
                final[atlas_sync.key_of(r)] = atlas_sync.fingerprint(r)
            ask({"action": "apply", "syncId": "x1", "peerId": "phone", "name": "Pixel", "kind": "Phone",
                 "upserts": upserts, "deletes": [{"kind": "item", "uid": "lum-n1"}], "relinks": [], "baseline": final})
            ask({"action": "apply", "syncId": "x1", "peerId": "phone", "upserts": [], "deletes": []})
            ask({"action": "records", "peerId": "phone", "name": "Pixel"})
            unknown = outcome(host.answer, {"action": "dance"})
            events_, tasks_ = atlas_sync.calendar(store)
    finally:
        atlas_sync.now_ms = old_now

    write_json("atlas_cases.json", {
        "constants": {"port": atlas_sync.PORT, "magic": atlas_sync.MAGIC.decode(), "salt": atlas_sync.SALT.decode(),
                      "rounds": atlas_sync.ROUNDS, "max_frame": atlas_sync.MAX_FRAME, "protocol": atlas_sync.PROTOCOL,
                      "accepts": atlas_sync.ACCEPTS, "sealed": atlas_sync.SEALED, "error": atlas_sync.ERROR,
                      "current_theme": atlas_sync.CURRENT_THEME,
                      "ignored_in_fingerprint": sorted(atlas_sync.IGNORED_IN_FINGERPRINT)},
        "now_ms": int(NOW_TS * 1000), "timezone": "UTC",
        "derive_key": keys, "seal": seals, "error_frame": err_sock.sent.hex(), "bad_frames": bad_frames,
        "fingerprints": prints, "colour_to_int": colours, "int_to_colour": ints, "note_text": texts,
        "checklists": checklists, "occurrences": occ, "atlas_theme": theme_rec, "theme_roles": roles,
        "baseline_digest": {"baseline": final, "digest": atlas_sync.baseline_digest(final)},
        "conversation": {"device_name": "test-pc", "notes_before": notes, "activity": activity, "theme": theme_fields,
                         "exchanges": convo, "unknown_action": unknown, "notes_after": conv_notes,
                         "activity_added": added, "store_after": store.state, "incoming_themes": store.incoming_themes,
                         "calendar": {"events": events_, "tasks": tasks_}}},
        "core/atlas_sync.py with fixed nonces, uuids and clock (now_ms = " + str(int(NOW_TS * 1000)) + ")",
        "keys, sealed frames byte for byte, fingerprints Atlas must agree with, repeat rules, and a full "
        "hello / records / apply conversation through Host.answer")


HISTORIES = {
    "fish_history": (
        "- cmd: ls -la\n  when: 1763900000\n"
        "- cmd: echo \"multi\\nline\" \\\\ done\n  when: 1763900060\n  paths:\n    - /tmp\n"
        "- cmd: \n  when: 1763900100\n"
        "- cmd: git status\n  when: 1700000000\n"
        "- cmd: cargo build --release\n"),
    "bash_history": "#1763900000\nls -la\nnohist\n\n#1763900060\ngit push\n#notanumber\necho hi\n",
    ".zsh_history": (": 1763900000:0;ls -la\n: 1763900005:2;echo one \\\ntwo\nplain line without stamp\n"
                     ": 1763900010:0;\n: 1763900020:0;vim ~/.zshrc\n"),
    "PSReadLine/ConsoleHost_history.txt": "Get-Process\nWrite-Host `\n  \"hello\"\n\nls C:\\\ncd ..`\n",
    "command_log.txt": "2026-11-23T10:00:00\tfirefox\n2026-11-23T10:05:30\tkitty -e htop\n\nbroken line\n",
}


def promptlog_cases():
    folder = fresh_dir(os.path.join(SCRATCH, "histories"))
    cases = []
    for name, text in HISTORIES.items():
        if name == "command_log.txt":
            path = runner.COMMAND_LOG
        else:
            path = os.path.join(folder, *name.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        kind = promptlog.kind_of(path)
        entries = promptlog.read_entries(path)
        rows = []
        for cmd, raw in entries:
            t = promptlog.entry_time(kind, raw)
            rows.append({"command": cmd, "raw": raw, "time": t.isoformat() if t else None})
        keep = [raw for i, (_c, raw) in enumerate(entries) if i % 2 == 0]
        promptlog.write_entries(path, keep)
        after = read_text(path)
        promptlog.write_entries(path, [])
        emptied = read_text(path)
        write_text(f"promptlog_cases/{name.replace('/', '__')}", text)
        cases.append({"file": name.replace("/", "__"), "kind": kind, "entries": rows,
                      "delete_odd_rows": {"kept_raw": keep, "file_after": after}, "delete_all": emptied})
    write_json("promptlog_cases/cases.json", {
        "timezone": "UTC", "note": "kind comes from the file name: fish_history, *zsh*, PSReadLine/*_history.txt, "
                                   "Luminescent's data/command_log.txt, anything else is bash",
        "cases": cases},
        "modules/promptlog.py kind_of, read_entries, entry_time, write_entries on the sample files next to it",
        "fish, bash, zsh, PowerShell and command-log parsing (multi-line entries, timestamps) and what deleting "
        "rows writes back")


HYPR_MAIN = '''-- main config
require("variables")
require("binds")
hl.config({ general = { gaps_in = 4 } })
'''

HYPR_VARS = '''mainMod = "SUPER"
BROWSER = "zen-browser"
local TERMINAL = "kitty"
FILES = "dolphin"
'''

HYPR_BINDS = '''local menu = "fuzzel"
local function ws(n) return mainMod .. " + " .. n end

hl.bind(mainMod .. " + Q", hl.dsp.exec_cmd(TERMINAL))
hl.bind(mainMod .. " + G", hl.dsp.exec_cmd("uwsm app -- " .. BROWSER))
hl.bind(mainMod .. " + C", hl.dsp.window.close())
hl.bind("SUPER + SHIFT + E", hl.dsp.exit())
hl.bind(mainMod .. " + F", hl.dsp.window.fullscreen({ mode = 1 }))
hl.bind(mainMod .. " + left", hl.dsp.focus({ direction = "left" }))
hl.bind("ALT + R", hl.dsp.exec_cmd(menu .. " --drun"))  -- a comment
hl.bind("CTRL + ALT + T", function() hl.dsp.exec_cmd("notify-send hi") end)
--[[ hl.bind("SUPER + X", hl.dsp.exit()) ]]
-- hl.bind("SUPER + Y", hl.dsp.exit())
for i = 1, 3 do
    local key = i
    hl.bind(mainMod .. " + " .. key, hl.dsp.focus({ workspace = i }))
end
hl.bind(mainMod .. " + mouse:272", hl.dsp.window.drag(), { mouse = true })
hl.bind("SUPER + code:10", hl.dsp.focus({ workspace = 1 })); hl.bind("SUPER + D", hl.dsp.exec_cmd(FILES))
hl.bind("SUPER + V", hl.dsp.exec_cmd("cliphist list | " .. menu .. " --dmenu | cliphist decode | wl-copy"))
'''

HYPR_CONF = '''# hyprlang binds are listed and can be opened, not edited
bind = SUPER, B, exec, firefox
bindl = , XF86AudioPlay, exec, playerctl play-pause
'''


def hypr_runtime() -> list:
    """What `hyprctl -j binds` (plus Luminescent's Lua registry dump) would say for HYPR_BINDS, in config order."""
    def lua(n, mask, key, values=None, **flags):
        return {"modmask": mask, "submap": "", "key": key, "keycode": 0, "dispatcher": "__lua", "arg": str(n),
                "description": "", "lua": {"kind": "C", "values": values or []}, **flags}
    rows = [lua(1, 64, "Q", ["kitty"]), lua(2, 64, "G", ["uwsm app -- zen-browser"]), lua(3, 64, "C"),
            lua(4, 65, "E"), lua(5, 64, "F"), lua(6, 64, "left"), lua(7, 8, "R", ["fuzzel --drun"]),
            {"modmask": 12, "key": "T", "keycode": 0, "dispatcher": "__lua", "arg": "8", "description": "",
             "lua": {"kind": "L", "source": os.path.join(hyprbinds.CONFIG_DIR, "binds.lua"), "line": 11}},
            lua(9, 64, "1", [1.0]), lua(10, 64, "2", [2.0]), lua(11, 64, "3", [3.0]),
            lua(12, 64, "mouse:272", [], mouse=True),
            {"modmask": 64, "key": "", "keycode": 10, "dispatcher": "__lua", "arg": "13", "description": "",
             "lua": {"kind": "C", "values": [1.0]}},
            lua(14, 64, "D", ["dolphin"]), lua(15, 64, "V", []),
            {"modmask": 64, "key": "B", "keycode": 0, "dispatcher": "exec", "arg": "firefox", "description": ""},
            {"modmask": 0, "key": "XF86AudioPlay", "keycode": 0, "dispatcher": "exec", "arg": "playerctl play-pause",
             "description": "", "locked": True},
            {"modmask": 12, "key": "K", "keycode": 0, "dispatcher": "exec", "arg": "luminescent",
             "description": core.hypr.BIND_DESC}]
    return rows


def hyprbinds_cases():
    cfg = fresh_dir(hyprbinds.CONFIG_DIR)
    originals = {"hyprland.lua": HYPR_MAIN, "variables.lua": HYPR_VARS, "binds.lua": HYPR_BINDS,
                 "hyprland.conf": HYPR_CONF}

    def reset():
        for name, text in originals.items():
            with open(os.path.join(cfg, name), "w", encoding="utf-8", newline="\n") as f:
                f.write(text)

    reset()
    for name, text in originals.items():
        write_text(f"hyprbinds_cases/{name}", text)
    runtime = hypr_runtime()
    write_json("hyprbinds_cases/runtime_binds.json", runtime,
               "hand-written: what Hyprland reports for the sample config", "the input load() combines with the files")

    tokens = [{"src": s, "tokens": [[t.kind, t.text, t.line] for t in hyprbinds.tokenize(s)]}
              for s in ('local x = "a" .. \'b\'', "a = 0x1F + 2.5e3 -- c\nb=[[long\nstring]]",
                        'hl.bind(m .. " + Q", f({a = 1}))', 'x = "esc\\"aped\\n"', "--[==[ block ]==] y = 1")]
    parsed = []
    for path in hyprbinds.config_files():
        pf = hyprbinds.parse_file(path)
        parsed.append({"file": os.path.basename(path), "variables": pf.variables, "globals": pf.globals_,
                       "calls": [{"line": c.line, "end_line": c.end_line, "args": [c.arg_text(i) for i in range(len(c.args))],
                                  "in_loop": c.in_loop, "keys": c.keys_text(), "static_keys": scrub(hyprbinds._static_keys(c)),
                                  "dispatcher": hyprbinds._dispatcher(c)[0]} for c in pf.calls]})

    stub = SimpleNamespace(query=lambda what: runtime if what == "binds" else [], request=lambda *a, **k: "",
                           BIND_DESC=core.hypr.BIND_DESC)
    old_hypr, old_rt = hyprbinds.hypr, hyprbinds.runtime_binds
    hyprbinds.hypr = stub
    hyprbinds.runtime_binds = lambda: copy.deepcopy(runtime)
    try:
        def rows():
            return hyprbinds.load([("ctrl+shift+space", "launcher", "Open Luminescent"), ("alt", "doodle", "Doodle"),
                                   ("super+c", "notes", "Clashes with Close window")])

        listed = [{"keys": b.keys, "mods": list(b.mods), "key": b.key, "action": b.action, "detail": b.detail,
                   "origin": b.origin, "owner": b.owner, "file": os.path.basename(b.file), "line": b.line,
                   "flags": b.flags, "release": b.release, "conflict": b.conflict, "editable": b.editable,
                   "combo": b.combo, "runs_command": hyprbinds.runs_command(b)} for b in rows()]

        edits = []

        def edit_case(name, fn):
            reset()
            binds = rows()
            err = fn(binds)
            edits.append({"name": name, "error": err,
                          "files_after": {n: read_text(os.path.join(cfg, n)) for n in originals
                                          if read_text(os.path.join(cfg, n)) != originals[n]}})

        by = lambda binds, keys: next(b for b in binds if b.keys == keys and b.origin == "hyprland")
        edit_case("change keys keeps the mainMod .. form", lambda bs: hyprbinds.change_keys(by(bs, "SUPER + C"),
                                                                                           "SUPER + SHIFT + C"))
        edit_case("change keys to another modifier", lambda bs: hyprbinds.change_keys(by(bs, "SUPER + C"), "ALT + C"))
        edit_case("edit keys and command", lambda bs: hyprbinds.edit(by(bs, "SUPER + Q"), "SUPER + RETURN", 'foot "-e" htop'))
        edit_case("delete comments the call out", lambda bs: hyprbinds.delete(by(bs, "SUPER + SHIFT + E")))
        edit_case("delete refuses a shared line", lambda bs: hyprbinds.delete(by(bs, "SUPER + D")))
        edit_case("delete refuses a loop", lambda bs: hyprbinds.delete(by(bs, "SUPER + 2")))
        edit_case("add goes to the file with most binds", lambda bs: hyprbinds.add("SUPER + N", 'notify-send "hi"'))

        def changed_on_disk(bs):
            b = by(bs, "SUPER + C")
            with open(b.call.file, "a", encoding="utf-8") as f:
                f.write("-- changed\n")
            return hyprbinds.change_keys(b, "SUPER + X")
        edit_case("file changed since it was listed", changed_on_disk)

        def config_error(bs):
            stub.query = lambda what: ["binds.lua:3: syntax error"] if what == "configerrors" else runtime
            try:
                return hyprbinds.change_keys(by(bs, "SUPER + C"), "SUPER + X")
            finally:
                stub.query = lambda what: runtime if what == "binds" else []
        edit_case("Hyprland reports an error: the old file is put back", config_error)
        reset()
        conf_lines = [{"key": k, "mods": list(m), "arg": a, "found": [os.path.basename(p), n]}
                      for (k, m, a) in (("B", ("SUPER",), "firefox"), ("XF86AudioPlay", (), ""), ("Z", (), ""))
                      for p, n in [hyprbinds.find_conf_line(k, m, a)]]
    finally:
        hyprbinds.hypr, hyprbinds.runtime_binds = old_hypr, old_rt

    write_json("hyprbinds_cases/cases.json", {
        "added_mark": hyprbinds.ADDED_MARK, "files_in_order": [os.path.basename(p) for p in hyprbinds.config_files()],
        "tokens": tokens, "parsed": parsed, "luminescent_binds": [["ctrl+shift+space", "launcher", "Open Luminescent"],
                                                                 ["alt", "doodle", "Doodle"],
                                                                 ["super+c", "notes", "Clashes with Close window"]],
        "listed": listed, "edits": edits, "find_conf_line": conf_lines},
        "core/hyprbinds.py tokenize, parse_file, load, edit, change_keys, delete, add with Hyprland stubbed",
        "Lua config parsing, binds matched to their hl.bind line, readable actions, and the exact text every edit "
        "writes (or the error it gives)")


TERMINAL_THEMES = {
    "dark": {"values": theme.PRESETS["dark"], "extras": {}},
    "light": {"values": theme.PRESETS["light"], "extras": {"opacity": 0.85}},
    "custom": {"values": themes.colours_from_roles({"primary": "#FE8019", "bg": "#282828", "card": "#3C3836",
                                                    "text": "#EBDBB2", "muted": "#BDAE93", "border": "#504945"}),
               "extras": {"colours": {"red": "#FB4934", "colour16": "#FABD2F", "nope": "#000000", "blue": "bad"},
                          "opacity": 5, "logo": "~/Pictures/logo.png", "logo_width": 500, "logo_height": "x",
                          "logo_position": "top", "narrow": {"below": 10, "then": "text"},
                          "greeting": {"on": True, "text": "Welcome back\nit's 'Monday'", "colour": "purple",
                                       "info": "luminescent", "style": "box", "args": "--pipe false",
                                       "lines": [{"type": "os", "label": "distro", "colour": "brightBlue"},
                                                 {"type": "memory", "label": "ram", "colour": "nope"},
                                                 {"type": "colors", "label": ""}, {"type": "unknown"}]}}},
    "plain greeting": {"values": theme.PRESETS["dark"],
                       "extras": {"greeting": {"on": True, "text": "", "style": "plain", "info": "own"},
                                  "narrow": {"below": 120, "then": "top"}}},
}

FASTFETCH = '''// my fastfetch config
{
    "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
    "logo": {
        "source": "arch", // the distro logo
        "padding": { "top": 1 }
    },
    "modules": ["title", "os", "kernel"]
}
'''


def terminal_cases():
    old_sub = terminal.subprocess
    terminal.subprocess = SimpleNamespace(run=lambda *a, **k: None, DEVNULL=None)
    cases = []
    try:
        for name, spec in TERMINAL_THEMES.items():
            theme.use(spec["values"])
            terminal.set_theme(spec["extras"])
            terminal.write_fragment()
            row = {"name": name, "colours": spec["values"], "theme_extras": spec["extras"],
                   "theme_settings": terminal.theme_settings(), "scheme": terminal.scheme(),
                   "kitty_args": terminal.colour_args("kitty"), "alacritty_args": terminal.colour_args("alacritty"),
                   "foot_args": terminal.colour_args("foot"), "sequences": terminal.sequences(),
                   "logo_value": terminal.logo_value(), "greeting_script": terminal.greeting_script()}
            terminal.write_kitty_theme()
            row["kitty_theme_conf"] = read_text(terminal.KITTY_THEME)
            if terminal.theme_settings().get("greeting"):
                row["fastfetch_config"] = terminal._fastfetch_config(terminal.theme_settings()["greeting"])
            cases.append(row)
        terminal.remove_fragment()

        theme.use(theme.PRESETS["dark"])
        terminal.set_theme({})
        ff = []
        os.makedirs(os.path.dirname(terminal.FASTFETCH_CONF), exist_ok=True)
        for value in ('{"type": "auto", "source": "/x.png", "width": 30}', None):
            for start in (FASTFETCH, '{\n    "modules": []\n}\n', "not json at all"):
                with open(terminal.FASTFETCH_CONF, "w", encoding="utf-8") as f:
                    f.write(start)
                err = terminal.set_fastfetch_logo(value, os.path.join(SCRATCH, "ff-backups"))
                ff.append({"value": value, "before": start, "error": err, "after": read_text(terminal.FASTFETCH_CONF),
                           "logo_read_back": terminal.fastfetch_logo()})
        includes = []
        for conf in ("include luminescent-theme.conf\n", "font_size 11\ninclude luminescent-theme.con\n", "font_size 11\n"):
            os.makedirs(terminal.KITTY_DIR, exist_ok=True)
            with open(terminal.KITTY_CONF, "w", encoding="utf-8") as f:
                f.write(conf)
            includes.append({"kitty_conf": conf, "state": terminal.kitty_include_state()})
        commands = []
        for term in ("kitty", "alacritty", "foot", "wezterm", "ghostty", "konsole", "gnome-terminal",
                     "xfce4-terminal", "xterm"):
            old_find = terminal.find
            terminal.find = lambda configured="", t=term: "/usr/bin/" + t
            try:
                commands.append({"terminal": term, "command": terminal.command("Title", "/tmp/work", ["htop", "-d", "5"])})
            finally:
                terminal.find = old_find
    finally:
        terminal.subprocess = old_sub

    # Windows Terminal: the fragment exists only in the Windows branch of the module, so reload it as Windows
    os.makedirs(os.environ["LOCALAPPDATA"], exist_ok=True)
    core.IS_WIN = True
    try:
        wt = importlib.reload(terminal)
        windows = []
        for name in ("dark", "custom"):
            spec = TERMINAL_THEMES[name]
            theme.use(spec["values"])
            wt.set_theme(spec["extras"])
            for own in (False, True):
                wt._own = own
                wt.write_fragment()
                windows.append({"name": name, "updates_own_profiles": own, "fragment": read_json(wt.FRAGMENT),
                                "command": wt.command("Title", "C:\\work", ["htop;x"])})
        windows_fragment_path = wt.FRAGMENT
    finally:
        core.IS_WIN = sys.platform == "win32"
        importlib.reload(terminal)
    theme.use(theme.PRESETS["dark"])
    write_json("terminal_cases/cases.json", {
        "names": terminal.NAMES, "slots": terminal.SLOTS, "info_modules": [list(m) for m in terminal.INFO_MODULES],
        "themes": cases, "fastfetch_logo_edits": ff, "kitty_include_state": includes, "terminal_commands": commands,
        "windows_terminal": {"fragment_path": windows_fragment_path, "cases": windows}},
        "core/terminal.py scheme, colour_args, write_kitty_theme, sequences, greeting_script, _fastfetch_config, "
        "set_fastfetch_logo, command, and the Windows Terminal fragment (module reloaded as Windows)",
        "the exact text Luminescent writes into kitty, fish, fastfetch and Windows Terminal for a theme")


def backup_cases(sample: str):
    old = (backup.HOME_DIR, backup.DATA_DIR)
    backup.HOME_DIR, backup.DATA_DIR = sample, os.path.join(sample, "data")
    try:
        zpath = os.path.join(SCRATCH, "backup.zip")
        n = backup.export(zpath)
        members = backup.check(zpath)
        bad = []
        import zipfile
        for name, entries in (("no settings.json", {"data/notes.json": "{}"}),
                              ("damaged file", {"settings.json": "{}", "data/notes.json": "{nope"}),
                              ("extra files are ignored", {"settings.json": "{}", "data/evil.json": "{}",
                                                           "../escape.json": "{}", "data/profiles/x.json": "{}",
                                                           "data/profiles/a/b.json": "{}"})):
            p = os.path.join(SCRATCH, "bad.zip")
            with zipfile.ZipFile(p, "w") as z:
                for k, v in entries.items():
                    z.writestr(k, v)
            bad.append({"name": name, "entries": sorted(entries), **outcome(backup.check, p)})
    finally:
        backup.HOME_DIR, backup.DATA_DIR = old
    write_json("backup_cases.json", {"files": backup.FILES, "sample_home_members": sorted(members), "count": n,
                                     "check": bad},
               "core/backup.py export and check on sample_home and on hand-made zips",
               "which files a backup holds and which zips an import refuses")


# ── sample_home: one realistic example of every data file ─────────────────────

def sample_home():
    home = fresh_dir(os.path.join(SCRATCH, "sample_home"))
    data = os.path.join(home, "data")
    os.makedirs(os.path.join(data, "profiles"))
    old_paths = (settings_mod.PROFILES_FILE, settings_mod.PROFILES_DIR)
    settings_mod.PROFILES_FILE = os.path.join(data, "profiles.json")
    settings_mod.PROFILES_DIR = os.path.join(data, "profiles")
    snippets_dir = os.path.join(home, "snippets")
    try:
        with seeded("sample-settings"):
            s = settings_mod.Settings(os.path.join(home, "settings.json"))
            s.create_profile("Gaming")
            s.set("general.tray_messages", True)
            s.set("ui.text_scale", 1.1)
            s.set("snippets.base_path", snippets_dir)
            s.set("doodle.colour", "#FF5A8A")
            s.set("reminders.items", [
                {"id": "r-water", "title": "Drink some water", "message": "", "enabled": True,
                 "schedule": {"type": "interval", "time": "09:00", "date": "2026-11-01", "days": [0, 1, 2, 3, 4],
                              "every_minutes": 60, "window": True, "from": "09:00", "to": "18:00", "startup_delay": 10,
                              "monthday": 1, "every_days": 2},
                 "actions": {"popup": True, "position": "default", "sound": "default", "sound_file": "",
                             "sound_repeat": 1, "toast": True}},
                {"id": "r-bins", "title": "Bins out", "message": "Green bin this week", "enabled": True,
                 "schedule": {"type": "weekly", "time": "19:30", "days": [1]},
                 "actions": {"popup": True, "position": "default", "sound": "chime", "sound_file": "",
                             "sound_repeat": 2, "toast": False}}])
            s.save(emit=False)
    finally:
        settings_mod.PROFILES_FILE, settings_mod.PROFILES_DIR = old_paths

    def put(rel, value, indent=2):
        path = os.path.join(home, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        core.save_json(path, value, indent=indent)

    themes_path = themes.PATH
    if os.path.exists(themes_path):
        os.remove(themes_path)

    class Themes:
        _load = themes.ThemeStore._load
        _first_run = staticmethod(themes.ThemeStore._first_run)

    with seeded("sample-themes"):
        td = Themes()._load(SimpleNamespace(raw_saved={}))
        mine = themes.new_theme("Sunset", td["themes"][0])
        mine.update({"colours": {**theme.PRESETS["dark"], **theme.generate("#F76B15", False)},
                     "gradients": {"primary": {"type": "linear", "angle": 135.0,
                                               "stops": [[0.0, "#F76B15"], [1.0, "#E5484D"]]}},
                     "modules": {"notes": "#30A46C", "timer": {"accent": "#3E63DD", "colours": {}, "gradients": {},
                                                               "radius": 14}},
                     "toasts": {"style": "glow"}, "hotkey": "ctrl+alt+1", "radius": 12,
                     "terminal": {"opacity": 0.9, "greeting": {"on": True, "text": "hi", "info": "own"}},
                     "caelestia": {"colours": {"primary": "#F76B15"}, "shell": {}, "extras": {}}})
        folder = {"id": "f-mine", "name": "Mine", "colour": "#F76B15", "collapsed": False}
        mine["folder"] = folder["id"]
        td["folders"].insert(0, folder)
        td["themes"].append(mine)
        td["active"] = mine["id"]
    put("data/themes.json", td)

    class Dash:
        _load = dashboard.DashboardStore._load
        _first_run = dashboard.DashboardStore._first_run
        _to_screen_layout = dashboard.DashboardStore._to_screen_layout
        tools_tab = dashboard.DashboardStore.tools_tab

        def __init__(self):
            acts = SimpleNamespace(icon="", actions=lambda: [("open", "Open Snippets", None),
                                                             ("save_clipboard", "Save the clipboard to Snippets", None)])
            notes_m = SimpleNamespace(icon="", actions=lambda: [("toggle", "Show / hide Notes", None)])
            self.launcher = SimpleNamespace(modules={"snippets": acts, "notes": notes_m},
                                            settings=SimpleNamespace(raw_saved={}))

    if os.path.exists(dashboard.PATH):
        os.remove(dashboard.PATH)
    with seeded("sample-dashboard"):
        dd = Dash()._load()
        dd["boards"][0]["tabs"].append({
            "id": "apps", "kind": "items", "title": "Apps", "x": 392, "y": 136, "w": 264, "h": 520, "collapsed": False,
            "screen": "", "items": [
                {"id": "ff", "kind": "item", "name": "Firefox", "path": "/usr/bin/firefox", "args": "--private-window",
                 "hotkey": "ctrl+alt+f", "action": "", "cwd": "", "admin": False, "ask_args": False, "terminal": False,
                 "group": []},
                {"id": "h1", "kind": "heading", "name": "Work", "path": "", "args": "", "hotkey": "", "action": "",
                 "cwd": "", "admin": False, "ask_args": False, "terminal": False, "group": []},
                {"id": "grp", "kind": "group", "name": "Morning", "path": "", "args": "", "hotkey": "", "action": "",
                 "cwd": "", "admin": False, "ask_args": False, "terminal": False, "group": ["ff", "htop"]},
                {"id": "htop", "kind": "item", "name": "htop", "path": "htop", "args": "", "hotkey": "", "action": "",
                 "cwd": "~", "admin": False, "ask_args": True, "terminal": True, "group": []},
                {"id": "sep", "kind": "separator", "name": "", "path": "", "args": "", "hotkey": "", "action": "",
                 "cwd": "", "admin": False, "ask_args": False, "terminal": False, "group": []}]})
        dd["boards"][0]["tabs"].append({"id": "notes-box", "kind": "page", "title": "Notes", "x": 688, "y": 136,
                                        "w": 420, "h": 360, "collapsed": False, "screen": "DP-1", "page": "notes",
                                        "state": {}, "items": []})
        dd["boards"][0]["tabs"].append({"id": "pic", "kind": "picture", "title": "Cat", "x": 688, "y": 520, "w": 300,
                                        "h": 200, "collapsed": True, "screen": "", "path": "/home/me/Pictures/cat.png",
                                        "frame": "border", "fit": "fill", "items": []})
        dd["boards"].append({"id": "b-games", "name": "Games", "tabs": []})
        dd["screens"] = {"HDMI-A-1": "b-games"}
    put("data/dashboard.json", dd)

    class Engine:
        _load = automation.AutomationEngine._load
        _example = automation.AutomationEngine._example
        _blank = staticmethod(automation.AutomationEngine._blank)

    if os.path.exists(automation.PATH):
        os.remove(automation.PATH)
    with seeded("sample-automation"):
        ad = Engine()._load()
        ad["vars"] = {"greeting": "Hello", "count": "4"}
        ws = Engine._blank("Clipboard tidy")
        a = {"id": "ev", "type": "event", "x": 0, "y": 0, "title": "", "off": False, "state": {},
             "fields": {**automation.default_fields("event"), "event": "clipboard.changed"}}
        b = {"id": "if", "type": "if", "x": 300, "y": 0, "title": "Long?", "off": False, "state": {},
             "fields": {**automation.default_fields("if"), "op": "regex", "b": "^https?://"}}
        c = {"id": "nt", "type": "notify", "x": 600, "y": 0, "title": "", "off": True, "state": {"last": "x"},
             "fields": {**automation.default_fields("notify"), "title": "Link copied", "message": "{input}"}}
        ws["nodes"] = [a, b, c]
        ws["wires"] = [{"id": "w1", "src": ["ev", "out:fire"], "dst": ["if", "in:in"]},
                       {"id": "w2", "src": ["if", "out:yes"], "dst": ["nt", "in:run"]}]
        ws["view"] = {"x": -120, "y": 40, "zoom": 0.8}
        ad["workspaces"].append(ws)
    put("data/automation.json", ad)

    with seeded("sample-notes"):
        nd = notes_mod._default_data(360, 460)
        nd["notes"][0].update({"content": "# Ideas\n\nTry the **Rust** build.", "pinned": True, "colour": "#7B6CF6",
                               "tags": ["work"]})
        nd["notes"].append({"id": "todo1", "name": "Shopping", "type": "todo", "content": "", "pinned": False,
                            "colour": "", "tags": [],
                            "items": [{"id": "i1", "text": "milk", "checked": True, "due": ""},
                                      {"id": "i2", "text": "bread", "checked": False, "due": "2026-11-24T09:00"}]})
        nd["open"] = "todo1"
        nd["stickies"] = {"todo1": {"x": 40, "y": 60, "w": 280, "h": 300, "screen": "DP-1"}}
    put("data/notes.json", nd)

    put("data/timers.json", [
        {"label": "Focus", "mode": "timer", "show_ms": False, "duration_ms": 1500000, "x": 1500, "y": 80, "w": 260,
         "h": 160, "laps": [], "elapsed_ms": 300000.0, "running": True, "finished": False, "saved_at": NOW_TS},
        {"label": "", "mode": "stopwatch", "show_ms": True, "duration_ms": 0, "x": 1500, "y": 260, "w": 260, "h": 160,
         "laps": [12345.0, 23456.5], "elapsed_ms": 40000.0, "running": False, "finished": False}])
    put("data/calculator.json", {
        "history": [{"expr": "950*12", "result": "11,400", "value": 11400, "t": "2026-11-22 18:02"},
                    {"expr": "5 km to mi", "result": "3.1068559612 mi", "value": 3.1068559611866697, "t": "2026-11-23 09:15"},
                    {"expr": "today + 45 days", "result": "Thu 7 Jan 2027", "value": None, "t": "2026-11-23 09:16"}],
        "pins": [{"name": "Rent", "var": "rent", "expr": "950*12", "value": 11400, "result": "11,400",
                  "folder": "Money", "colour": "#30A46C"}],
        "panel": {"visible": True, "base": "dec", "keypad": False}}, indent=1)
    put("data/currency.json", {"date": NOW.date().isoformat(), "base": "EUR", "rates": fixed_rates()}, indent=1)
    put("data/reminders_state.json", {
        "r-water": {"last": "2026-11-23T14:00:00", "sched": json.dumps(
            s.get("reminders.items")[0]["schedule"], sort_keys=True)},
        "r-bins": {"last": "2026-11-17T19:30:00", "sched": json.dumps(s.get("reminders.items")[1]["schedule"],
                                                                      sort_keys=True), "snoozed": "2026-11-23T14:45:00"}})
    put("data/usage.json", {
        "items": {"ff": {"label": "Firefox", "n": 12, "last": "2026-11-23T09:01:00"}},
        "hotkeys": {"ctrl+shift+space": {"label": "Open Luminescent", "n": 40, "last": "2026-11-23T14:20:11"}},
        "tools": {"calculator": {"label": "Calculator", "n": 3, "last": "2026-11-23T09:16:00"}},
        "days": {"2026-11-22": 30, "2026-11-23": 25}}, indent=1)
    with open(os.path.join(data, "activity.jsonl"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(json.dumps(e, ensure_ascii=False) for e in (
            {"t": "2026-11-23T09:01:00", "kind": "run", "label": "Firefox"},
            {"t": "2026-11-23T10:30:00", "kind": "timer", "label": "Focus", "secs": 1500.0},
            {"t": "2026-11-23T11:00:00", "kind": "reminder", "label": "Drink some water"})) + "\n")
    put("data/hearing.json", {"days": {"2026-11-22": {"dose": 0.412345, "listen": 7260.0,
                                                      "minutes": {"600": [72.4, 60.0], "601": [68.1, 45.5]}},
                                       "2026-11-23": {"dose": 0.05, "listen": 900.0, "minutes": {"540": [70.0, 60.0]}}},
                               "calibrations": {"alsa_output.usb-Headphones": {"offset": -6.5, "name": "Headphones"}}},
        indent=None)
    put("data/performance.json", performance._defaults(), indent=None)
    put("data/sysmon.json", {"x": 1200, "y": 400, "w": 420, "h": 320}, indent=None)
    put("data/media.json", {"x": 40, "y": 900, "w": 380, "h": 140}, indent=None)
    put("data/colours.json", {"recent": ["#7B6CF6", "#FF5A8A", "#30A46C"]}, indent=None)
    put("data/picker.json", {"recent": ["#F76B15", "#E5484D"]}, indent=None)
    put("data/palette.json", {"recent": ["action:notes:toggle", "page:settings", "theme:dark"]}, indent=None)
    put("data/promptlog.json", {"pins": ["cargo build --release", "git status"]})
    put("data/desktop_win.json", {"version": "1.9.21", "exe": "C:\\Users\\me\\Luminescent\\Luminescent.exe"}, indent=None)
    with open(os.path.join(data, "command_log.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(HISTORIES["command_log.txt"])
    with open(os.path.join(data, "starting.json"), "w", encoding="utf-8") as f:
        json.dump({"failed": 1, "at": NOW_TS}, f)
    with open(os.path.join(data, "launcher.log"), "w", encoding="utf-8", newline="\n") as f:
        f.write("14:30:00 INFO    launcher: === starting 1.9.21 (pid 1234, source, data in /home/me/Luminescent) ===\n"
                "14:30:01 INFO    launcher: started notes\n"
                "14:30:01 INFO    launcher: ready: notes, timer, calculator\n")
    with open(os.path.join(data, "crash.log"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n=== 1.9.21 started 2026-11-23 14:30:00 ===\n")

    old_now = atlas_sync.now_ms
    atlas_sync.now_ms = lambda: int(NOW_TS * 1000)
    try:
        with seeded("sample-atlas"):
            p = os.path.join(data, "atlas_sync.json")
            st = atlas_sync.Store(p)
            recs = st.records(nd["notes"], [], None)
            st.remember("phone", "Pixel", "Phone", {atlas_sync.key_of(r): atlas_sync.fingerprint(r) for r in recs}, recs)
    finally:
        atlas_sync.now_ms = old_now

    put("snippets/.luminescent.json", {"folder_colours": {"Work": "#3E63DD"}, "order": {"": ["Work", "hello.txt"]},
                                       "favourites": ["hello.txt"]})
    os.makedirs(os.path.join(snippets_dir, "Work"), exist_ok=True)
    with open(os.path.join(snippets_dir, "hello.txt"), "w", encoding="utf-8") as f:
        f.write("Hello {clipboard}, sent {date}.\n")

    dst = os.path.join(OUT, "sample_home")
    shutil.rmtree(dst, ignore_errors=True)
    for root, _dirs, names in os.walk(home):
        for n in sorted(names):
            src = os.path.join(root, n)
            write_text(os.path.join("sample_home", os.path.relpath(src, home)), read_text(src))

    # A weekly backup of the copy (its paths already scrubbed), with fixed times inside the zip so every run
    # makes the same bytes
    for root, _dirs, names in os.walk(dst):
        for n in names:
            os.utime(os.path.join(root, n), (NOW_TS, NOW_TS))
    old = (backup.HOME_DIR, backup.DATA_DIR)
    backup.HOME_DIR, backup.DATA_DIR = dst, os.path.join(dst, "data")
    try:
        os.makedirs(os.path.join(dst, "data", "backups"))
        backup.export(os.path.join(dst, "data", "backups", "weekly-20261116.zip"))
    finally:
        backup.HOME_DIR, backup.DATA_DIR = old
    written.append(("sample_home/", "core/settings.py, core/themes.py, ui/dashboard.py, core/automation.py, "
                                    "modules/notes.py, core/atlas_sync.py, core/backup.py, rest by hand in the "
                                    "shapes the modules read",
                    "one realistic example of every data file; each must load and save back unchanged. Paths in it "
                    "say <SCRATCH>/sample_home where the folder itself is meant"))
    return dst


# ── README ────────────────────────────────────────────────────────────────────

def readme():
    lines = ["# Golden fixtures", "",
             "Made by `luminescent-rs/tools/make_fixtures.py` from the Python app's own code (do not edit by hand; "
             "run it again when the Python reference changes). Run it twice and nothing changes: the clock is "
             f"{NOW.isoformat()} (a Monday) in UTC, uuids, salts, nonces and recovery keys come from fixed seeds, and "
             "scratch paths are written as `<SCRATCH>`, `<HOME>` or `<LUMINESCENT_HOME>`.", "",
             "Numbers that must survive exactly (calculator values, units) are written as `{\"int\": \"...\"}` or "
             "`{\"float\": \"repr\"}`. Byte strings are lower-case hex. A case that raises in Python records "
             "`{\"error\": type, \"message\": text}` instead of `ok`.", "",
             "| File | Made by | Pins down |", "|---|---|---|"]
    for rel, made_by, pins in sorted(written):
        lines.append(f"| `{rel}` | {made_by} | {pins} |")
    write_text("README.md", "\n".join(lines) + "\n")


def main():
    if os.path.isdir(OUT):
        shutil.rmtree(OUT)
    os.makedirs(OUT)
    try:
        specs()
        calc_cases()
        convert_cases()
        timeparse_cases()
        reminder_cases()
        hotkey_cases()
        tones_cases()
        gradient_cases()
        automation_cases()
        settings_migration_cases()
        profile_cases()
        themes_migration_cases()
        dashboard_migration_cases()
        automation_load_cases()
        vault_cases()
        atlas_cases()
        promptlog_cases()
        hyprbinds_cases()
        terminal_cases()
        home = sample_home()
        backup_cases(home)
        readme()
    finally:
        shutil.rmtree(SCRATCH, ignore_errors=True)
    print(f"wrote {len(written)} fixtures to {OUT}")


if __name__ == "__main__":
    main()
