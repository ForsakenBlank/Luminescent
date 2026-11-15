"""Snippet placeholders and "back to the theme before". Run: .venv/bin/python -m pytest tests -q"""
from datetime import datetime

from modules.snippets import fill_placeholders


def test_placeholders():
    now = datetime(2026, 11, 15, 14, 5)
    assert fill_placeholders("Sent {date} at {time} ({weekday})", "", now) == "Sent 2026-11-15 at 14:05 (Sunday)"
    assert fill_placeholders("> {clipboard}\n{datetime}", "hello", now) == "> hello\n2026-11-15 14:05"
    code = 'print(f"{name} {x:>2}") {}'
    assert fill_placeholders(code, "clip", now) == code                 # other braces are left alone
    assert fill_placeholders("{clipboard}", "{date}", now) == "{date}"  # what was copied is not filled in again
