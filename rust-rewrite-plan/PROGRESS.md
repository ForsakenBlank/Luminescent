# Progress

The live tracker. Every session reads this first and updates it last: tick the checkpoints it finished, set the
phase and overall percentages, and write a handoff note.

**Overall progress = progress before the current phase + the phase's share × its checkpoint %.**
Example: Phase 6 (share 6 %, starts at 37 %) at its 50 % checkpoint → 37 + 6 × 0.5 = **40 %**.

## Overall

```
Overall: 16.25 %   [###.................]   Current phase: Phase 3 at 25 %
```

| # | Phase | Share | Starts at | Done at | Phase progress | Status |
|---|---|---|---|---|---|---|
| 0 | Groundwork | 2 % | 0 % | 2 % | 100 % | done |
| 1 | Workspace + core infrastructure | 5 % | 2 % | 7 % | 100 % | done |
| 2 | UI foundation + widget kit | 7 % | 7 % | 14 % | 100 % | done (gate provisional, D26) |
| 3 | Platform layer | 9 % | 14 % | 23 % | 25 % | in progress |
| 4 | Themes | 8 % | 23 % | 31 % | 0 % | not started |
| 5 | Overlay shell | 6 % | 31 % | 37 % | 0 % | not started |
| 6 | Dashboard | 6 % | 37 % | 43 % | 0 % | not started |
| 7 | Settings page | 4 % | 43 % | 47 % | 0 % | not started |
| 8 | Everyday tools | 10 % | 47 % | 57 % | 0 % | not started |
| 9 | Screen tools | 5 % | 57 % | 62 % | 0 % | not started |
| 10 | Automation | 9 % | 62 % | 71 % | 0 % | not started |
| 11 | System modules | 10 % | 71 % | 81 % | 0 % | not started |
| 12 | Audio + media | 5 % | 81 % | 86 % | 0 % | not started |
| 13 | Privacy, sync, keybinds | 6 % | 86 % | 92 % | 0 % | not started |
| 14 | Desktop integration | 4 % | 92 % | 96 % | 0 % | not started |
| 15 | Migration, packaging, release | 4 % | 96 % | 100 % | 0 % | not started |
| 16 | Beyond parity (own track) | n/a | n/a | n/a | 0 of 0 ticked features | waiting for 2.0 |

## Milestones

- [ ] 23 %: **Hello tray** (tray icon, hotkey opens the empty overlay on every monitor)
- [ ] 37 %: **Looks right** (overlay, palette, cheat sheet, notifications in Luminescent's look)
- [ ] 57 %: **Daily-drivable alpha** (dashboard, settings, themes, notes, timers, calculator, reminders, clipboard)
- [ ] 96 %: **Feature-complete beta** (everything 1.9.21 does)
- [ ] 100 %: **2.0 release**

## Checkpoints

### Phase 0: Groundwork
- [x] 25 %: generator skeleton, all spec dumps
- [x] 50 %: behaviour fixtures for all pure logic
- [x] 75 %: format fixtures and `sample_home/`
- [x] 100 %: BASELINE.md, measuring scripts, inventory reviewed

### Phase 1: Workspace + core infrastructure
- [x] 25 %: workspace, toolchain, lints, CI green, CLAUDE.md in place
- [x] 50 %: paths, storage, logging, settings + profiles + migrations passing fixtures
- [x] 75 %: event bus, actions, usage, backups, guard, start guard, module trait
- [x] 100 %: binary with CLI, single instance, ctl socket, `--smoke`, CI green on both systems

### Phase 2: UI foundation + widget kit
- [x] 25 %: eframe skeleton, fonts, tokens, multi-monitor transparent overlay demo
- [x] 50 %: gate decision recorded (D26: egui kept; only Hyprland click-through measured, the rest re-run with Hello tray)
- [x] 75 %: widget kit, FloatingPanel, dialogs, menus, animator, gradient and glow painters (built before the gate, see the note)
- [x] 100 %: gallery in both themes, snapshot tests, `--smoke` builds every widget (built before the gate)

### Phase 3: Platform layer
- [x] 25 %: lum-hypr and the Hyprland WindowPlacer, tested against a fake socket
- [ ] 50 %: hotkeys on Hyprland and Windows, hold keys, stuck-key handling, fixtures pass
- [ ] 75 %: tray, notifications, clipboard, autostart, keep awake, idle, sounds
- [ ] 100 %: runner, terminals, desktop entries and icons, Hello tray verified on both systems

### Phase 4: Themes
- [ ] 25 %: lum-theme engine, tones / generation / gradient fixtures pass
- [ ] 50 %: ThemeStore, live apply, theme hotkeys, import / export, undo / redo
- [ ] 75 %: Themes page and the colour picker dialog with gradient mode
- [ ] 100 %: theme editor (non-desktop pages), snapshots in three themes

### Phase 5: Overlay shell
- [ ] 25 %: backdrop, switcher, show / hide on every monitor, latency measured
- [ ] 50 %: chrome layer with edit mode
- [ ] 75 %: command palette and cheat sheet
- [ ] 100 %: notification cards, on-screen warning, prewarm, repair, Looks right confirmed

### Phase 6: Dashboard
- [ ] 25 %: store, model, migrations, running items and groups
- [ ] 50 %: boxes drawn, dragged, resized, folded, snapped, on every screen
- [ ] 75 %: selection, tidy with undo, boards, per-screen boards, page and picture boxes
- [ ] 100 %: all drag and drop, dialogs, hotkeys, lock, search, snapshots

### Phase 7: Settings page
- [ ] 25 %: layout, sidebar, search with counts, pinning
- [ ] 50 %: a widget for every field type, hotkey recorder with clash warnings
- [ ] 75 %: profiles, Save & Apply, Undo, Reset, module restarts
- [ ] 100 %: custom widgets, backups UI, snapshots of every section

### Phase 8: Everyday tools
- [ ] 25 %: lum-calc with all fixtures, Calculator panel and page, Timers with restore
- [ ] 50 %: Notes complete
- [ ] 75 %: Reminders complete, Clipboard page and Ctrl+B pop-up
- [ ] 100 %: QR, Today, Usage, Calendar, page boxes, Daily-drivable alpha confirmed

### Phase 9: Screen tools
- [ ] 25 %: capture backends, frozen multi-monitor picker base
- [ ] 50 %: colour picker, ruler, text grab
- [ ] 75 %: doodle complete
- [ ] 100 %: snapper on both systems, palette entries, tests

### Phase 10: Automation
- [ ] 25 %: model, engine core, logic and value nodes, expansion; fixtures pass
- [ ] 50 %: all triggers and actions wired
- [ ] 75 %: editor canvas, nodes, wires, picker, field editors
- [ ] 100 %: pulse animation, undo / redo, clipboard, workspaces and `.lumflow`, perf check

### Phase 11: System modules
- [ ] 25 %: sampler on Linux and Windows within budget
- [ ] 50 %: Performance Overview, Processes, HUD, System Monitor panel
- [ ] 75 %: Boost, Game mode, Alerts & rules, Startup apps, pkexec helper
- [ ] 100 %: System Settings in full, workspace sets with mlua tests

### Phase 12: Audio + media
- [ ] 25 %: capture on both systems, DSP with accuracy fixtures
- [ ] 50 %: limiter, cap, dose, history, APPS
- [ ] 75 %: Hearing Guard UI with calibration
- [ ] 100 %: Media controls on both systems, speech

### Phase 13: Privacy, sync, keybinds
- [ ] 25 %: lum-vault with cross-compatibility both ways, vault UI
- [ ] 50 %: Metadata Stripper, CLI, file manager menus
- [ ] 75 %: Atlas host passing fixtures, UI, calendar feed
- [ ] 100 %: Prompt Log and Keybinds, config edits verified on a copy

### Phase 14: Desktop integration
- [ ] 25 %: wallpaper backends wired into themes and automation
- [ ] 50 %: terminal, fish and fastfetch output passing fixtures
- [ ] 75 %: Caelestia module, look and extras
- [ ] 100 %: Glass, borders, theme editor desktop pages, Feature-complete beta

### Phase 15: Migration, packaging, release
- [ ] 25 %: first-run migration, safety backup, load checks
- [ ] 50 %: release workflow, 1.9.x → 2.0 update tested on both systems
- [ ] 75 %: parity audit complete
- [ ] 100 %: performance report, docs, release 2.0

## Handoff notes

Newest first. Each note: date, phase and checkpoint, what is done, what is left, anything surprising, and any
manual check waiting on the user.

### 2026-11-24, Phase 3 at 25 % (overall 16.25 %)

**Done.** lum-hypr: `lua` (the bootstrap, binds, moves, glass, cover and root rules; byte-identical to the Python
text, checked by the new `fixtures/hypr_cases.json`), `marker` (all five), `socket::Hypr` (request, lua, repl
values, queries, clients, monitors with reserved space, cursor, key_down, send_shortcut, foreign binds, bind with
the "table lost in a reload" retry), `events` (event socket on tokio, bootstrap again on `configreloaded` before
the event is passed on, reconnect with back-off), `fifo` (mkfifo, read-write open so it never sees EOF), `sync`
(`WindowSync`: one worker thread, the settle checks, late wishes, float on top) and `fake` (a stand-in Hyprland,
feature `fake`). lum-platform: `window` (the `WindowPlacer` trait; Hyprland, Windows and no-op backends) and
`desktop::Desktop` (start, placer, events as `DesktopEvent`, shutdown). lum-ui: `window` (placer in the context,
`Screen`); the panel and the dim call the placer; dims are titled per monitor and placed by cover rules; a dim
repaints once after click-through changes (D26). The app starts the desktop before eframe and routes its events
through the mailbox. D27 (`LUMRS` table and FIFO), D28 (placing by title, cover rules).

**Left.** Checkpoints 50 to 100 %. The `.venv` for `make_fixtures.py` builds here once `libegl1 libgl1
libxkbcommon0 libfontconfig1 libdbus-1-3` are installed.

### 2026-11-24, Phase 2 gate recorded (phase 2 at 100 %, overall 14 %)

The user ran the spikes on Hyprland with a local session, which stopped at click-through: wgpu caught 22 clicks
while click-through, glow gave no valid result. The user chose to carry on. D26 keeps egui (iced shares winit,
so it would not fix this) and hands click-through to the phase 3 window placer. The other Hyprland checks and all
of Windows 11 move into the Hello tray verification. Details in SPIKE_RESULTS.md. Phases 1 and 2 are merged into
main with this note.

### 2026-11-24, Phase 2: green, Part B built, the gate waits on the spike results (overall 8.75 %)

Checkpoint 25 % is ticked. The 75 % and 100 % work (Part B) is built and ticked, but the phase stays at 25 % until
the user runs the spikes on Hyprland and Windows 11 and the 50 % gate (egui or iced, glow or wgpu) is recorded.
Part B was started while waiting, as the prompt allowed; if the gate went to iced it would be redone. Once the
results are in and egui passes, the phase is at 100 % and overall at 14 %.

**Done.**
* Green: fmt, clippy (Linux and the Windows cross-target), all tests, `--smoke`, `cargo deny` (egui's fallback
  fonts are OFL / Ubuntu Font licence: an exception for `epaint_default_fonts` only). CI green on both systems.
  `rust.yml` installs `libxkbcommon-x11-0 xvfb libegl1 libgl1-mesa-dri mesa-vulkan-drivers` on Ubuntu, runs
  `tests/instance.rs` again under Xvfb (so start, hand-over, `ctl quit` and Ctrl+C are tested inside eframe), and
  runs `--idle-check 20` without and with a window.
* Every spike runs to the end under Xvfb with glow and wgpu (numbers in SPIKE_RESULTS.md). Fixed on the way: each
  report printed twice, `overlay` and `note` never ended (a deadline nobody woke up for; a repaint asked for in
  `raw_input_hook` is dropped when the pass starts), `nofocus` panicked (below).
* `tests/instance.rs` checks a clean shutdown for `ctl quit` and for Ctrl+C, and with a display that it really
  ran inside eframe.
* lum-ui: `text` (tones, mono caps with letter spacing), `widgets/` (buttons in seven variants plus the round
  icon button, text input and area, combo, spin box, slider, opacity slider, check box, switch, colour button,
  hotkey field (look only), cards and surfaces, section headers, rules, tooltips, menu entries, list rows,
  virtualised lists, the 9-way anchor bar, the dim), `panel.rs` (FloatingPanel), `dialog.rs` (message, error,
  confirm, choose, ask text), `anim.rs` (the Animator), `paint.rs` (gradient meshes, cached glow, halo, dot grid,
  sparkle), `module.rs` (`UiModule`, `UiCx`), `gallery.rs`. Painted widgets report their role and name to
  accessibility (kittest checks it).
* `--spike gallery`; snapshots `crates/lum-ui/tests/snapshots/` (gallery and two dialogs, Dark and Light);
  `--smoke` builds the gallery headless in both themes with the panel and every dialog open.
* The app draws its modules: `App::show_modules` runs `UiModule::show` for each running module in the main
  window's pass; tokens (text size, hover, performance mode) and motion come from settings at start.

**Surprising, worth knowing.**
* **eframe paints a hidden main window from outside the event loop** (`new_events` → `check_redraw_requests`), so
  an immediate viewport first opened from that pass has no event loop to make its window with and egui panics
  ("the user callback was never called"). And a repaint of an immediate viewport is redirected to its parent. So
  the main window must stay "visible" to winit: it is now a transparent, click-through 1x1 window off screen
  (D18). On Hyprland the rule no longer sets opacity 0, so Hyprland keeps sending it frame callbacks (R20).
* **The 114 MB** was llvmpipe: 47 MB of `libLLVM` and 10 MB of `libgallium` mapped into the process. Release with
  glow: 99 MB RSS, 35 MB private; wgpu: 137 MB, 41 MB private; no window: 7 MB. `--idle-check` now names the
  adapter, prints private memory and does not fail memory on a software renderer (D24).
* **Tessellating 400 wires takes about 3 ms**; under llvmpipe the frame takes 40 ms, all of it rasterising on the
  CPU. The wires spike now prints tessellation alone, so the real desktops' numbers separate the two.
* **egui 0.37's default fonts lack ✕ ✓ ✦ and most symbols**, and egui only enables them with `default_fonts`
  (it came in through eframe before). lum-ui bundles a DejaVu subset as "Luminescent Symbols"
  (`luminescent-rs/assets/fonts`, made with fontTools from DejaVuSans.ttf: every symbol the Python UI uses plus
  the Arrows, Math, Technical, Enclosed, Box, Block, Shapes, Misc Symbols, Dingbats and Misc Arrows blocks) and
  turns on egui's monochrome emoji fonts (D20). Not covered by any free font: `＋ ⌕ ⌗ ⏯ ⏲ ⏻ ␟ ⓘ ⤢ ⧉ 🗂 🗜 🛟 🧮 🧹`.
* egui 0.37: `TextEdit::margin` is ignored once you give it a `frame`, put the margin on the frame; `ui.put`
  moves the cursor, `ui.place` does not; `WidgetType` is now `egui::Role` (accesskit); a pass's
  `textures_delta` must be applied or cleared (`lum_ui::headless_pass` does it); egui runs one extra frame after
  the last repaint request, so "quiet" means quiet on the pass after; a horizontal layout in a child of endless
  height works out NaN. `Id::unique` is deterministic (hashes its argument), despite the name.
* kittest needs every widget to have an accessible name: give inputs a hint or `.name(...)`.
* Under Xvfb there is no window manager or compositor: nothing ever has the keyboard (so `nofocus` "fails"),
  click-through and drops can not be tried, and a transparent dim draws black.
* There is no toggle switch in the Python app; `Switch` exists because the phase list asked for one.

**Left for phase 2.** The user's spike results (commands in SPIKE_RESULTS.md and under "Waiting on the user"); record them in
SPIKE_RESULTS.md, write the gate decision as D26, tick 50 %, set the phase to 100 % and overall to 14 %. If a hard
check fails on egui, say which and stop before switching. If Hyprland lags opening the dims or the panel, see R20.

### 2026-11-23, Phase 2 started (work in progress, no checkpoint ticked yet, overall 7 %)

The session ran out of room part way to the 25 % checkpoint. Everything below is committed and builds
(`cargo build -p luminescent`); fmt has run; **clippy, `cargo test --workspace`, the Windows cross-clippy and CI have
not been run on this work yet**. Do those first.

**Done (not yet verified by tests/CI).**
* Workspace deps: egui / eframe 0.37 (glow, wayland, x11, default_fonts), egui_kittest 0.37 (snapshot + wgpu).
  The binary has a `wgpu` cargo feature for the renderer comparison. `cargo deny check` not run yet.
* `lum-hypr::socket`: minimal Hyprland control socket client (request, `j/` queries, `eval` Lua, `lstr`), tested
  against a fake socket.
* `lum-platform`: `desktop` (monitors from Hyprland / EnumDisplayMonitors, window rules in a `LUMRS` Lua table so
  they never clash with the Python app's `LUMI`, hidden-root rule, cover rules per monitor, `describe_windows`,
  `has_display`, `places_windows_by_rule`), `process` (CPU via getrusage / GetProcessTimes, RSS, threads,
  context switches), and `ipc::serve` now takes a `Wake` callback so a ctl request wakes the UI.
* `lum-ui`: `fonts` (Geist / Geist Mono embedded from `assets/fonts`, one family per weight, egui fonts as symbol
  fallback) and `theme` (Dark / Light `Palette` from `core/theme.py`, `Gradient`, `mix` / `alpha`, `Tokens` with
  r / rs / rb, text scale clamped 0.8 to 1.6, tones, scrim, and `Tokens::style()` applied to egui's Style;
  `Tokens::of(ctx)` reads them back). Unit tests pass.
* Binary: `launcher.rs` no longer blocks on `select!` when there is a display. `shell.rs` runs eframe with a hidden
  1x1 transparent root; `App::logic` calls `App::poll()` (drains ctl + messages, usage flush via
  `request_repaint_after`), quitting sends `Close` to the root. `Waker` / `Mailbox` call `ctx.request_repaint()`
  from services. No display (or `--headless` / `LUMINESCENT_HEADLESS=1`), or a renderer that fails or panics,
  falls back to the old windowless loop (`run_headless`). Shutdown is idempotent.
* `--idle-check <secs>` (`idle.rs`): settles 3 s, samples the process, prints a block plus a table row, exits 1 when
  over budget (CPU >= 0.1 %, >= 1 wake-up/s, any painted frame, RSS over 100 MB).
* `--spike <name>` (`spike/`): overlay, clickthrough, nofocus, drop (windows.rs), idle, form, wires, note
  (perf.rs), `list`; `--renderer glow|wgpu`, `--secs N`. Each prints numbers and a `| ... |` row for
  SPIKE_RESULTS.md. None of them has been run yet, not even under Xvfb.

**Surprising.**
* eframe 0.37 has `App::logic` (runs on `request_repaint` even while hidden) and `App::ui(&mut Ui)`;
  viewport callbacks also get `&mut Ui`. `CentralPanel::show_inside` is deprecated (use `show(ui, ..)`).
  `Id::new` is deprecated (`Id::unique` / `IdSalt`). `DroppedFile` is a trait (`path()`, `bytes()`).
* eframe always calls `set_visible(true)` on the root after its first painted frame, so `with_visible(false)` does
  nothing; the shell sends `Visible(false)` on frame one (it runs after that show). winit can not hide windows on
  Wayland at all, so on Hyprland the root relies on the `LUMRS` rule (1x1, opacity 0, no focus). The overlay spike
  prints `describe_windows()` so the user can see whether anything of ours is on screen.
* Hiding a viewport on Wayland means destroying it, so the overlay spike times reopen (create + first frame); that
  number decides the phase 5 prewarm design.
* winit panics (not errors) when `libxkbcommon-x11.so` is missing; CI on ubuntu-22.04 needs `libxkbcommon-x11-0`
  (and `xvfb` + mesa for a windowed idle check, `mesa-vulkan-drivers` for kittest snapshots via wgpu). Add them to
  `.github/workflows/rust.yml` only.
* First Xvfb run (debug build, llvmpipe): `--idle-check 5` gave 0.01 % CPU, 0.4 wake-ups/s, 0 frames painted,
  but **114 MB RSS** (llvmpipe and a debug build inflate it). Measure a release build; if RSS stays high, compare
  wgpu and check what glow / Mesa allocate.

**Left for phase 2.** Run clippy / tests / Windows cross-clippy / deny, fix; update `instance.rs` if needed (CI has
no display, so it takes the windowless path); CI deps; give the user the spike commands and record results in
SPIKE_RESULTS.md; then Part B (widget kit, FloatingPanel, dialogs, menus, Animator, gradient and glow painters,
kittest snapshots, gallery, `--smoke` building every widget headless with a plain `egui::Context`).

### 2026-11-23, Phase 1 finished (100 %, overall 7 %)

**Done.**
* `luminescent-rs/` is a Cargo workspace with every crate from ARCHITECTURE.md (the later ones are empty), Rust
  1.97.0 pinned in `rust-toolchain.toml`, workspace lints (clippy warnings fail CI), `rustfmt.toml`,
  `clippy.toml`, `deny.toml`, and `luminescent-rs/CLAUDE.md` with the real commands.
* `.github/workflows/rust.yml`: fmt, clippy `-D warnings`, tests and `--smoke` on ubuntu-22.04 and
  windows-latest, plus `cargo deny`, only for changes under `luminescent-rs/`. `check.yml` is untouched.
* `lum-fixtures` (dev-dependency): reads fixtures, `{"int"}` / `{"float"}` numbers, hex, `{"error", "message"}`
  outcomes, placeholders, whitespace-free comparison, copying fixture folders into temp dirs.
* `lum-core`: paths, `pyjson` (Python's `json.dump` byte for byte), atomic `store`, rotating `logging` in the
  Python line format, `clock` (fake clock for tests), settings schema embedded per OS, the store with every
  `_migrate` rule and profiles, `events`, `actions`, `usage`, `backup` (export, check, restore, weekly keep 5),
  `guard` (`safely`, `Faults`, `StartGuard`, panic hook to crash.log), `module` (`Module`, `ModuleHost`).
* `lum-platform`: single-instance file lock, the ctl socket (Unix socket / named pipe), quit signals.
* `luminescent` binary: clap CLI (`--smoke`, `--bench-startup`, `--safe-mode`, `--strip-metadata` stub,
  `ctl ping|status|overlay|palette|settings|action <id>|quit`), a 2-thread tokio runtime for services, a second
  start that hands over and exits, clean shutdown (usage flushed, modules stopped, start guard cleared, socket
  removed).
* Tests: defaults on both systems, all 19 migration cases (known keys exact, unknown keys kept), the 8 profile
  steps file for file, backup members and refusals, a Python-made backup restored byte for byte, all 24 JSON
  files in `sample_home` saved back byte for byte, and the real binary driven end to end (smoke, second start,
  ctl, quit). 53 tests.
* Numbers (BASELINE.md): 17 ms start to ready (Python 276 ms), 0.00 % idle CPU, 0 wake-ups, 5 MB RSS (Python
  114 MB), 2.8 MB binary. `tools/measure_rust.sh` measures the same way as `measure_python.sh`.

**Left.** Nothing in phase 1. No module is ported yet: `modules()` in `crates/luminescent/src/launcher.rs` is the
list each later phase adds to.

**Surprising, worth knowing in later phases.**
* **Phase 0 fixes.** The repo's root `.gitignore` (`settings.json`, `data/`) had kept `sample_home/settings.json`
  and all of `sample_home/data/` out of git; `luminescent-rs/.gitignore` now re-includes them (the rule can not
  live in `fixtures/`, which the generator wipes). And `settings_defaults.json` held the Linux defaults twice,
  because `importlib.reload` hands back the same module object; the generator now reads defaults inside its loop.
  So the phase 0 note is wrong: Windows defaults differ in `automation.shell` (`cmd`) and `vault.storage_path`.
* **chrono is flagged unmaintained by RustSec**, so `cargo deny` failed; dates use `jiff` 1.0 now (D7). jiff 1.0
  renamed `Zoned` to `ZonedDateTime`; most examples online still show the 0.2 names. `zip` is at 9.x.
* **Write files with `lum_core::store::save_json(path, value, style)` and the style the Python module uses**:
  settings and profiles `Style::indented(2)`, usage `indented(1)`, many small files `Style::COMPACT`. The
  `sample_home` test shows which file uses which. Serde models must keep key order (`preserve_order` is on) and
  unknown fields.
* Python's `_migrate` raises part way through on a section that is not an object (`"ui": "not a dict"`) and keeps
  what it already changed; `settings/migrate.rs` does the same on purpose.
* Python restarts a module that tripped the fault limit on the very next settings save (its snapshot is set to
  off, so "on" counts as a change). Kept for parity; it may be worth asking the user whether that is wanted.
* Python counts every tool run twice in usage.json; Rust counts once (D14).
* The main thread runs a crossbeam `select!` loop over ctl requests, service messages and the usage flush timer.
  **Phase 2 must move that into eframe's update**: drain the same two channels each frame, and give services a
  way to call `ctx.request_repaint()` so a ctl request or a timer wakes the UI without polling.
* `clippy.toml` allows unwrap in `#[test]` functions only; integration test files under `tests/` start with
  `#![allow(clippy::unwrap_used)]` for their helpers.
* To catch Windows-only lint errors before CI: `rustup target add x86_64-pc-windows-gnu` and
  `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` (an unused import
  behind `cfg(unix)` broke one CI run).
* Copying `sample_home` as a data folder starts in safe mode: its `starting.json` records a failed start. Delete
  that file first (the measuring script does).
* A second start waits up to 3 s for the lock before handing over, like Python's `tryLock(3000)`, so tests that
  start a second copy take a few seconds.

### 2026-11-23, Phase 0 finished (100 %, overall 2 %)

**Done.**
* `luminescent-rs/tools/make_fixtures.py` writes `luminescent-rs/fixtures/` (1.4 MB, 31 entries, every one listed
  in `fixtures/README.md` with what made it and what it pins down). Run it with the repo's `.venv`; it takes about
  4 s and two runs give no diff. It needs Qt's system libraries (`libegl1 libgl1 libxkbcommon0 libfontconfig1
  libdbus-1-3`, as in `check.yml`); `opencv-python` and `pulsectl` are not needed.
* Spec dumps: settings schema for Linux and Windows, defaults, automation nodes, events, pages, theme presets and
  the theme pack, key tables.
* Behaviour cases: calculator (sums in degrees and radians, every base and format, the panel's compute with ans,
  pins, conversions and dates), conversions, date sums, time parsing, reminders (next fire time for every schedule
  type, descriptions, quick reminders), hotkeys, tones, gradients and generated themes, automation compare and
  token expansion.
* Format cases: settings migrations and profiles step by step, themes.json, dashboard.json and automation.json
  migrations, a real vault with decoy plus a no-AAD-index vault and a pre-rewrite `.hash` vault, Atlas keys, frames
  and fingerprints plus a whole hello / records / apply conversation, shell histories, a Lua Hyprland config with
  every edit's exact result, terminal / kitty / fish / fastfetch / Windows Terminal output, backups, and
  `sample_home/` with every data file.
* `rust-rewrite-plan/BASELINE.md` with the headless numbers, plus `measure_python.sh` and `measure_python.ps1`.
* FEATURE_INVENTORY.md checked against the code, README and CHANGELOG: 11 rows and a "Widget kit" section added,
  counts corrected (269 stored settings, not about 265).
* `.gitattributes` keeps every file under `luminescent-rs/fixtures/` byte for byte (no line-ending changes on
  Windows), since vault blobs, histories and configs are compared exactly.

**Left.** Nothing in phase 0 itself. The real-machine numbers are a manual check (below); they are not needed to
start phase 1.

**Surprising, worth knowing in later phases.**
* The plan folder lived only on the branch `ccr-fc892c9d-4jloxi`, not on `main`; this phase's branch brings it
  over.
* The hidden Python app is not idle: 0.7 % of a core and about 1.2 wake-ups a second with nothing on screen. Find
  the timer before copying behaviour in phase 3 or 5.
* Start to ready headless is already 276 ms in Python (imports are lazy). The Rust budget of 250 ms is not a big
  win on its own; the gains are memory (114 MB now) and everything after start.
* Settings defaults are the same on Linux and Windows; only help texts and a few labels differ.
* Python's `_merge` drops unknown settings keys. `settings_migration_cases.json` records what Python does; the Rust
  test must compare the known keys and check separately that unknown ones survive.
* `ui/dashboard.py` crashes migrating a layout-1 file whose items list holds a non-object (a real but unlikely bug
  in the old code). The fixture leaves that case out; the Rust migration should just skip such items.
* `core/hyprbinds.py` only uses string values from Hyprland's Lua registry, so binds made in a numeric loop show
  "workspace …". The fixture pins that; improving it is a phase 16 idea, not parity.
* `tones.surprise()` depends on Python's random number generator. The fixture shows its shape; the Rust port only
  needs to match `harmony()`, which is pinned exactly.
* The Windows Terminal fragment only exists in the Windows branch of `core/terminal.py`; the generator reloads the
  module with `IS_WIN` patched to record it.
* All time-dependent fixtures use TZ=UTC and Monday 2026-11-23 14:30. Rust tests that read shell history times or
  Atlas repeat rules must run in UTC too (or convert).

## Waiting on the user

Manual checks on real desktops that a session asked for and is waiting on.

* **Phase 2 leftovers (fold into the phase 3 Hello tray check):** the open spike checks (Hyprland: overlay, no
  focus, drop, idle, form, wires, note and click-through again on glow; Windows 11: all of them) and one pass
  through `--spike gallery` on each. Commands in [SPIKE_RESULTS.md](SPIKE_RESULTS.md).

* **Phase 1 (optional, does not block phase 2):** run `luminescent-rs/tools/measure_rust.sh` on your Hyprland PC
  (it builds the release binary first) and paste the row into BASELINE.md next to the Python one. There is no
  PowerShell version yet; on Windows, `cargo run --release -p luminescent -- --bench-startup` gives the in-process
  start time.

* **Phase 0 (optional, does not block phase 1):** run the measuring scripts on your Hyprland PC and on Windows 11
  and paste the rows into the table in [BASELINE.md](BASELINE.md). The commands are in BASELINE.md under "On your
  machines". If the PowerShell one errors, paste the error instead; it could not be tested in the cloud session.
