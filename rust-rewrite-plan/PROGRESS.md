# Progress

The live tracker. Every session reads this first and updates it last: tick the checkpoints it finished, set the
phase and overall percentages, and write a handoff note.

**Overall progress = progress before the current phase + the phase's share × its checkpoint %.**
Example: Phase 6 (share 6 %, starts at 37 %) at its 50 % checkpoint → 37 + 6 × 0.5 = **40 %**.

## Overall

```
Overall: 2 %   [....................]   Current phase: Phase 0 done, Phase 1 is next
```

| # | Phase | Share | Starts at | Done at | Phase progress | Status |
|---|---|---|---|---|---|---|
| 0 | Groundwork | 2 % | 0 % | 2 % | 100 % | done |
| 1 | Workspace + core infrastructure | 5 % | 2 % | 7 % | 0 % | not started |
| 2 | UI foundation + widget kit | 7 % | 7 % | 14 % | 0 % | not started |
| 3 | Platform layer | 9 % | 14 % | 23 % | 0 % | not started |
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
- [ ] 25 %: workspace, toolchain, lints, CI green, CLAUDE.md in place
- [ ] 50 %: paths, storage, logging, settings + profiles + migrations passing fixtures
- [ ] 75 %: event bus, actions, usage, backups, guard, start guard, module trait
- [ ] 100 %: binary with CLI, single instance, ctl socket, `--smoke`, CI green on both systems

### Phase 2: UI foundation + widget kit
- [ ] 25 %: eframe skeleton, fonts, tokens, multi-monitor transparent overlay demo
- [ ] 50 %: all eight spike checks measured on both systems, gate decision recorded
- [ ] 75 %: widget kit, FloatingPanel, dialogs, menus, animator, gradient and glow painters
- [ ] 100 %: gallery in both themes, snapshot tests, `--smoke` builds every widget

### Phase 3: Platform layer
- [ ] 25 %: lum-hypr and the Hyprland WindowPlacer, tested against a fake socket
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

* **Phase 0 (optional, does not block phase 1):** run the measuring scripts on your Hyprland PC and on Windows 11
  and paste the rows into the table in [BASELINE.md](BASELINE.md). The commands are in BASELINE.md under "On your
  machines". If the PowerShell one errors, paste the error instead; it could not be tested in the cloud session.
