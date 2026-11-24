# Performance

Speed is the main reason for the rewrite, so it is measured from the start, not tuned at the end.

## Where the Python app spends its time today

* **Start-up**: a Python interpreter plus PyQt6, numpy, cryptography, Pillow and OpenCV imports before anything
  shows. The PyInstaller bundle unpacks and loads hundreds of files.
* **Spawned helpers**: `nvidia-smi` (often every second while the Performance or System Monitor panel is up),
  `pactl`, `parec`, `playerctl`, `grim`, `hyprctl` for some queries. Each spawn costs milliseconds of CPU and a
  wake-up.
* **One GIL**: the audio DSP, the perf sampler, thumbnails and the UI all take turns on one interpreter lock, so a
  busy worker makes the UI stutter.
* **Qt restyling**: a theme change re-applies the global stylesheet and rebuilds every widget that bakes colours
  in (`retheme()`), which is visible on big pages.
* **Memory**: an interpreter, Qt's widget tree and numpy arrays sit at well over 100 MB before any data loads.

The Python app already avoids the worst traps (nothing polls while hidden, painted-once backdrops, worker threads
for OCR). The Rust app keeps every one of those rules and removes the costs above.

## Budgets

Phase 0 fills in the "1.9.21" column in BASELINE.md. A Rust phase is not done while one of its budgets fails,
unless the miss is written up with a reason in PROGRESS.md.

| What | How it is measured | Rust budget | Phase |
|---|---|---|---|
| Start to tray ready | process start → "ready" log line, warm disk cache | **≤ 250 ms** (cold cache ≤ 700 ms) | 1, 3, 15 |
| Overlay open | hotkey → first frame of the overlay on screen | **≤ 50 ms**, one frame once prewarmed | 5 |
| Page switch | click → new page fully drawn | ≤ 16 ms (one frame) | 5 |
| Theme switch | apply → everything restyled | ≤ 16 ms | 4 |
| Hotkey dispatch | bind fired / hook event → callback starts | ≤ 5 ms p99 | 3 |
| Idle CPU, overlay hidden | average over 60 s, no timers running | **0.0 %**, under 1 wake-up a second | every phase |
| Idle memory, overlay hidden | RSS after 60 s | **≤ 60 MB** target, 100 MB hard limit | 2, 15 |
| Memory, overlay open on two monitors | RSS | ≤ 150 MB | 5 |
| Palette search | keystroke → results with 2 000 entries | ≤ 5 ms | 5 |
| Dashboard drag | 30 boxes, 300 items | 60 fps, no frame over 16 ms | 6 |
| Settings page | open → first section drawn | ≤ 30 ms; scrolling 60 fps | 7 |
| Performance sampler | 1 Hz, 400 processes, NVIDIA GPU | ≤ 1 % of one core; no child processes | 11 |
| Hearing Guard | music playing / silence | ≤ 0.5 % / 0 % CPU | 12 |
| Automation | pulses per second without a UI hitch | ≥ 1 000 | 10 |
| Doodle | 500 strokes on screen, drawing | 60 fps | 9 |
| Screen picker | hotkey → frozen screen shown | ≤ 150 ms | 9 |
| Release binary | size of the program folder | ≤ 30 MB | 15 |

## Rules that keep it fast

1. **Nothing runs for what is not on screen.** Hidden windows are not repainted. Samplers, meters and players
   start when something shows them or needs them (an alert, a rule, an automation trigger) and stop after.
2. **The UI thread never blocks.** No disk reads beyond small JSON at start, no sockets, no processes, no
   network. Those go to services and come back as events.
3. **Repaint only on change.** egui's reactive mode plus `request_repaint` from services. Animations request
   repaints only while they move. The idle check in CI fails a phase that keeps repainting.
4. **Paint expensive things once.** Backdrop, dot grid, glow, previews and thumbnails become cached textures,
   rebuilt only on theme or size change.
5. **Virtualise long lists.** `ScrollArea::show_rows` for processes, histories, snippets, palette results, binds.
6. **Native APIs over spawned tools** (NVML, PipeWire / libpulse, MPRIS, DDC/CI, logind). A tool that must be
   spawned is spawned on demand, never on a timer, unless the user is looking at its output.
7. **No allocation in hot loops.** The audio callback, the sampler loop and the pulse animation reuse buffers.
8. **Debounced, atomic saves** off the UI thread.
9. **Measure with real tools.** `puffin` for frames, `tracy` when needed, `dhat` for heap, `perf` / Windows
   Performance Analyzer for CPU. Benchmarks with `criterion` for parsers, the palette matcher and the engine.

## How to measure

* `luminescent --bench-startup` (phase 1) prints time to ready.
* `luminescent --idle-check 60` (phase 2) samples its own CPU and RSS for 60 s and fails if it repainted or woke
  up more than the budget allows. CI runs it headless; the user runs it on real desktops.
* A debug overlay (phase 2, dev builds only, toggled from the palette) shows frame time, repaint reason and
  service queue lengths.
* `luminescent-rs/tools/measure.sh` and `measure.ps1` (phase 0 makes the Python versions) produce the same table
  for both apps, so BASELINE.md and the phase 15 report compare like with like.

## Release profile

```toml
[profile.release]
lto = "thin"
codegen-units = 1
panic = "unwind"     # catch_unwind keeps a faulty module from taking the app down
strip = "symbols"
```

Keep `panic = "unwind"`: fault isolation depends on it. Check `opt-level = 3` against `"s"` for size once in
phase 15.
