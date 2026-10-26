#!/bin/sh
# Starts Luminescent (first run: creates .venv and installs the Python packages).
cd "$(dirname "$0")" || exit 1
if [ ! -x .venv/bin/python ]; then
    python3 -m venv .venv && .venv/bin/pip install -r requirements.txt || exit 1
fi
exec .venv/bin/python Launcher.py "$@"
