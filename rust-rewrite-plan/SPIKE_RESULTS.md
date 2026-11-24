# Spike results: can egui + eframe carry Luminescent?

The phase 2 gate (ARCHITECTURE.md, D1). Each check is `luminescent --spike <name>`; it prints what to do, measures
what it can by itself, and ends with a `| ... |` row for the tables below. The decision goes into the decision
log in ARCHITECTURE.md once both real desktops have reported.

## How to run them

Release build with both renderers, a throw-away data folder, output saved to a file you can paste back. The
commands are also in PROGRESS.md under "Waiting on the user".

Hyprland (from the repo folder):

```bash
cd luminescent-rs && cargo build --release -p luminescent --features wgpu
```

```bash
export LUMINESCENT_HOME=$(mktemp -d) && for r in glow wgpu; do for s in overlay clickthrough nofocus drop idle form wires note; do ./target/release/luminescent --spike $s --renderer $r 2>&1 | tee -a ~/spike-results.txt; done; done
```

```bash
for r in glow wgpu; do ./target/release/luminescent --renderer $r --idle-check 60 2>&1 | tee -a ~/spike-results.txt; done
```

Windows 11 (PowerShell, from the repo folder):

```powershell
cd luminescent-rs; cargo build --release -p luminescent --features wgpu
```

```powershell
$env:LUMINESCENT_HOME = Join-Path $env:TEMP "lumspike"; foreach ($r in "glow","wgpu") { foreach ($s in "overlay","clickthrough","nofocus","drop","idle","form","wires","note") { .\target\release\luminescent.exe --spike $s --renderer $r 2>&1 | Tee-Object -Append "$env:USERPROFILE\spike-results.txt" } }
```

```powershell
foreach ($r in "glow","wgpu") { .\target\release\luminescent.exe --renderer $r --idle-check 60 2>&1 | Tee-Object -Append "$env:USERPROFILE\spike-results.txt" }
```

What each one asks of you:

| Spike | What to do | Ends |
|---|---|---|
| overlay | look: every screen dim, nothing else of ours visible (no frame, no taskbar button, no 1x1 dot); press Space a few times to time reopening | Esc or 60 s |
| clickthrough | click on the dim in both states; in CLICK-THROUGH your clicks should reach the window behind | 24 s |
| nofocus | click into a text editor and keep typing; the first panel must not take your typing, the second one should | 14 s |
| drop | drag files from Dolphin / Explorer onto the window | Esc or 60 s |
| idle | keep the mouse off the window | 33 s |
| form | let it scroll itself for 8 s, then scroll it yourself and judge the feel | Esc or 30 s |
| wires | watch | 10 s |
| note | let it type for 8 s, then type, select and paste yourself | Esc or 30 s |

Please add a word on what you saw for overlay, clickthrough, nofocus and drop: those can not be measured from
inside the app.

## Hard checks (from D1)

| # | Check | Pass means | Hyprland | Windows 11 |
|---|---|---|---|---|
| 1 | Transparent, frameless, on-top dim on every monitor | every screen dims, nothing extra shows | | |
| 2 | Click-through toggling | clicks reach the window behind while click-through, none caught | | |
| 3 | No focus stealing | typing stays in the other program; the second panel can take it | | |
| 4 | File drops from the file manager | the dropped paths are listed | | |
| 5 | Idle cost | 0 % CPU, under 1 wake-up a second, 0 frames painted | | |
| 6 | 300-field form | smooth scrolling, frames at the refresh rate | | |
| 7 | 400 wires + 100 pulses | 60 fps or the refresh rate | | |
| 8 | 50 000 character note | no visible lag typing (well under 16 ms a key) | | |

## Cloud session (Xvfb, no GPU), 2026-11-24

No real desktop: Xvfb at 1280x800, no window manager, Mesa llvmpipe (software) for both renderers, Intel Xeon,
4 threads. These show the spikes run to the end without crashing and give the CPU side of each check; the
window checks (1 to 4) need a real compositor, and frame rates here are the software renderer's, not the GPU's.

| Spike | Renderer | Result |
|---|---|---|
| 1 overlay | glow / wgpu | first frame of the dim 75 ms / 83 ms after asking (llvmpipe) |
| 2 click-through | glow / wgpu | runs; no window manager, so nothing to click |
| 3 no focus | glow / wgpu | runs; Xvfb has no window manager, so no window ever gets the keyboard |
| 4 file drop | glow / wgpu | runs; nothing can drag files into Xvfb |
| 5 idle, window open | glow | 0.00 % CPU, 0.1 wake-ups/s, 0 frames, 113 MB RSS (35 MB private) |
| 5 idle, window open | wgpu | 0.00 % CPU, 0.1 wake-ups/s, 0 frames, 142 MB RSS (41 MB private) |
| 6 form, 300 fields | glow | 155 fps, p95 7.8 ms between frames, ui code p95 1.5 ms, whole frame p95 3.2 ms |
| 6 form, 300 fields | wgpu | 122 fps, p95 11.2 ms, ui code p95 1.7 ms, whole frame p95 5.4 ms |
| 7 wires + pulses | glow | 18 fps; ui code 0.14 ms, tessellation alone 3.5 ms, the rest is llvmpipe rasterising |
| 7 wires + pulses | wgpu | 20 fps; ui code 0.13 ms, tessellation alone 2.9 ms |
| 8 note, 50 000 chars | glow / wgpu | 0.46 / 0.48 ms per key median, 0.87 / 1.23 ms worst |

`--idle-check 10` (the real app, hidden main window), release:

| Renderer | CPU | Wake-ups/s | RSS | Private | Threads | Painted | Result |
|---|---|---|---|---|---|---|---|
| none (no display) | 0.00 % | 0.3 | 7 MB | 1 MB | 3 | 0 | OK |
| glow on llvmpipe | 0.00 % | 0.3 | 99 MB | 35 MB | 12 | 0 | OK |
| wgpu on llvmpipe (Vulkan) | 0.00 % | 0.3 | 137 MB | 41 MB | 24 | 0 | OK (memory over, software renderer) |

**Where the 114 MB came from.** The phase 2 handoff saw 114 MB RSS in a debug build. In a release build with
glow it is 99 MB, of which 47 MB is `libLLVM` and 10 MB `libgallium` (Mesa's llvmpipe compiles shaders with LLVM
on the CPU) and 8 MB our own binary's pages, all shared file pages. The app's own memory is 35 MB, about 19 MB
of it heap (fonts, the glyph atlas, egui's state). A real GPU driver does not load LLVM, so the real desktops
will show the number that counts. `--idle-check` now prints the private part on Linux and does not fail the
memory budget when the renderer is a software one.

## Hyprland

_Waiting for the user's results._

## Windows 11

_Waiting for the user's results._
