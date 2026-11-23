# Baseline: the Python app's numbers

What Luminescent 1.9.21 (Python + PyQt6) measures today, so every Rust phase has numbers to beat. The budgets
are in [PERFORMANCE.md](PERFORMANCE.md); this file only records what the Python app does.

## Headless, measured in phase 0

Measured in the cloud session with `luminescent-rs/tools/measure_python.sh` (5 timed starts after one warm-up,
then 60 s idle, then the smoke test). No screen: `QT_QPA_PLATFORM=offscreen`, `LUMINESCENT_NO_HOOK=1` (no
Hyprland, so no hotkeys), a fresh data folder. Python 3.13.18, PyQt6 6.12.1 on Qt 6.12.0, Intel Xeon at 2.1 GHz,
4 threads, 16 GB.

| What | Python 1.9.21 (headless) | Rust budget | Notes |
|---|---|---|---|
| Start to "ready" log line | **276 ms** median, 274 ms best (warm cache) | ≤ 250 ms | `import Launcher` alone is 135 ms of that (Qt widgets, `core.widgets`, `core.theme`). Headless skips Hyprland, the tray and hotkeys, so a real start is slower |
| Idle CPU, overlay hidden, 60 s | **0.70 %** of one core | 0.0 % | Not zero: something still ticks while nothing is shown |
| Wake-ups while idle | **1.2 a second** (context switches, all threads) | under 1 a second | Same lead as above; worth finding which timer, so the Rust app does not copy it |
| Idle memory (RSS) after 60 s | **114 MB** | ≤ 60 MB, 100 MB hard limit | Before the overlay has ever been opened |
| Threads / child processes while idle | 2 / 0 | n/a | |
| `--smoke` (builds every page and pop-up once) | **16.4 s** | n/a | Mostly the smoke test's own waits; a check that nothing breaks, not a speed number |
| Program folder size | PyQt6 alone is 273 MB in the venv | ≤ 30 MB | The PyInstaller build is the number to compare in phase 15 |
| Fixture generator (`make_fixtures.py`) | 3.9 s | n/a | |

### The Rust app, phase 1 (headless core, no UI yet)

Measured with `luminescent-rs/tools/measure_rust.sh` in the same way (start to the "ready:" log line, 5 timed
starts after one warm-up, then 60 s idle). Release build, Intel Xeon at 2.3 GHz, 4 threads (a slightly different
cloud machine from the Python row). Phase 1 has no window, tray, hotkeys or modules yet, so this is the cost of
the core: settings, profiles, logging, the start guard, the ctl socket and two service threads. The comparison
that matters comes in phase 3 (Hello tray) and phase 5 (the overlay).

| What | Python 1.9.21 (headless) | Rust, phase 1 (headless) | Rust budget |
|---|---|---|---|
| Start to "ready" log line | 276 ms median, 274 ms best | **17 ms** median, 16 ms best (fresh folder); 17 / 17 ms with a copy of `fixtures/sample_home` | ≤ 250 ms |
| `main()` to ready (`--bench-startup`, inside the process) | n/a | 1.1 to 2.1 ms | n/a |
| Idle CPU, 60 s | 0.70 % | **0.00 %** | 0.0 % |
| Wake-ups while idle | 1.2 a second | **0.0** a second | under 1 a second |
| Idle memory (RSS) | 114 MB | **5 MB** | ≤ 60 MB |
| Threads / child processes | 2 / 0 | 3 / 0 (main + two service threads) | n/a |
| `--smoke` | 16.4 s | 12 to 14 ms | n/a |
| Program size | PyQt6 alone is 273 MB | 2.8 MB (one binary) | ≤ 30 MB |

Most of the 17 ms is the measuring loop itself (it polls the log every 10 ms and starts `grep` each time); the
app is ready about 2 ms after `main()` starts.

### Not measurable without a screen

These need a real desktop and are filled in from the table below, or by the phase that owns them:

| What | Rust budget | How to measure on the Python app |
|---|---|---|
| Overlay open (hotkey to first frame) | ≤ 50 ms | screen recording at 60 fps or more, count frames from the key press to the dim |
| Page switch | ≤ 16 ms | same, `Ctrl+1` to `Ctrl+2` |
| Theme switch | ≤ 16 ms | same, click a theme on the Themes page |
| Hotkey dispatch | ≤ 5 ms p99 | `data/launcher.log` with debug logging, or the Rust phase 3 tracer |
| Memory with the overlay open on two monitors | ≤ 150 MB | RSS from `/proc/<pid>/status` (Linux) or Task Manager (Windows) while it is open |
| Palette search with 2 000 entries | ≤ 5 ms | not measured in Python; the Rust benchmark sets it |
| Dashboard drag with 30 boxes and 300 items | 60 fps | screen recording while dragging |
| Settings page open | ≤ 30 ms | screen recording |
| Performance sampler at 1 Hz | ≤ 1 % of one core, no child processes | CPU of the app with the Performance window open for 60 s (the Python one spawns `nvidia-smi`) |
| Hearing Guard with music / silence | ≤ 0.5 % / 0 % | CPU of the app for 60 s with music playing, then with nothing playing |

## On your machines

Run these on your real setup and paste the row each one prints into the table. They copy your data folder (or
start fresh) into a temporary folder, so your real data is never used or changed. By default the copy runs next to
your normal Luminescent without hotkeys (`LUMINESCENT_NO_HOOK=1`); add `WITH_HOTKEYS=1` / `-WithHotkeys` after
quitting your normal Luminescent to include hotkey set-up in the start time.

Hyprland (from the repo folder, with the `.venv` from the README):

```bash
COPY_FROM=~/.local/share/Luminescent luminescent-rs/tools/measure_python.sh
```

The downloaded build instead of the source:

```bash
COPY_FROM=~/.local/share/Luminescent luminescent-rs/tools/measure_python.sh ~/Luminescent/Luminescent
```

Windows 11 (PowerShell, from the repo folder):

```powershell
powershell -ExecutionPolicy Bypass -File luminescent-rs\tools\measure_python.ps1 -CopyFrom "$env:LOCALAPPDATA\Luminescent"
```

The PowerShell script could not be run in the cloud session (no PowerShell there); if it fails, the error is
the first thing to fix. It has no wake-up count (Windows does not show context switches per process without
extra tools).

| Machine | Desktop | Data | Hotkeys | Start to ready (median / best of 5) | Idle CPU (60 s) | Wake-ups/s | Idle RSS | Threads | Child processes | Smoke test |
|---|---|---|---|---|---|---|---|---|---|---|
| Intel Xeon 2.1 GHz, 4 threads (cloud) | headless | fresh | no | 276 / 274 ms | 0.70 % | 1.2 | 114 MB | 2 | 0 | 16396 ms (ok) |
| _your Hyprland PC_ | | | | | | | | | | |
| _your Windows PC_ | | | | | | | | | | |

Also worth noting by hand on each machine, from a screen recording or a stopwatch feel: how long the overlay takes
to appear the first time and the second time, and how long a theme switch takes on the Themes page.
