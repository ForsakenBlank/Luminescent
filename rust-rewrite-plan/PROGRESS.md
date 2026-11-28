# Progress

The live tracker. Every session reads this first and updates it last: tick the checkpoints it finished, set the
phase and overall percentages, and write a handoff note.

**Overall progress = progress before the current phase + the phase's share × its checkpoint %.**
Example: Phase 6 (share 6 %, starts at 37 %) at its 50 % checkpoint → 37 + 6 × 0.5 = **40 %**.

## Overall

```
Overall: 65.25 %   [#############.......]   Current phase: Phase 10 at 50 % (every trigger and action wired to the app) (phase 3's Hello tray, phase 5's Looks right, phase 6's Dashboard, phase 7's Settings, phase 8's Daily-drivable alpha and phase 9's two-monitor checks are unconfirmed; 66.5 % with them)
```

| # | Phase | Share | Starts at | Done at | Phase progress | Status |
|---|---|---|---|---|---|---|
| 0 | Groundwork | 2 % | 0 % | 2 % | 100 % | done |
| 1 | Workspace + core infrastructure | 5 % | 2 % | 7 % | 100 % | done |
| 2 | UI foundation + widget kit | 7 % | 7 % | 14 % | 100 % | done (gate provisional, D26) |
| 3 | Platform layer | 9 % | 14 % | 23 % | 75 % (100 % built, waiting on the Hello tray check) | in progress |
| 4 | Themes | 8 % | 23 % | 31 % | 100 % (built, CI to confirm) | done |
| 5 | Overlay shell | 6 % | 31 % | 37 % | 75 % (100 % built, waiting on the Looks right check) | in progress |
| 6 | Dashboard | 6 % | 37 % | 43 % | 75 % (100 % built, waiting on the Dashboard check) | in progress |
| 7 | Settings page | 4 % | 43 % | 47 % | 100 % (built, waiting on the Settings check) | in progress |
| 8 | Everyday tools | 10 % | 47 % | 57 % | 90 % (100 % built, waiting on the Daily-drivable alpha check) | in progress |
| 9 | Screen tools | 5 % | 57 % | 62 % | 75 % (100 % built, waiting on the two-monitor check) | in progress |
| 10 | Automation | 9 % | 62 % | 71 % | 50 % | in progress |
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
- [x] 50 %: hotkeys on Hyprland and Windows, hold keys, stuck-key handling, fixtures pass
- [x] 75 %: tray, notifications, clipboard, autostart, keep awake, idle, sounds
- [ ] 100 %: runner, terminals, desktop entries and icons, Hello tray verified on both systems

### Phase 4: Themes
- [x] 25 %: lum-theme engine, tones / generation / gradient fixtures pass
- [x] 50 %: ThemeStore, live apply, theme hotkeys, import / export, undo / redo
- [x] 75 %: Themes page and the colour picker dialog with gradient mode
- [x] 100 %: theme editor (non-desktop pages), snapshots in three themes

### Phase 5: Overlay shell
- [x] 25 %: backdrop, switcher, show / hide on every monitor, latency measured
- [x] 50 %: chrome layer with edit mode
- [x] 75 %: command palette and cheat sheet
- [ ] 100 %: notification cards, on-screen warning, prewarm, repair, Looks right confirmed

### Phase 6: Dashboard
- [x] 25 %: store, model, migrations, running items and groups
- [x] 50 %: boxes drawn, dragged, resized, folded, snapped, on every screen
- [x] 75 %: selection, tidy with undo, boards, per-screen boards, page and picture boxes
- [ ] 100 %: all drag and drop, dialogs, hotkeys, lock, search, snapshots (built and tested here; ticked once the Dashboard check below is in)

### Phase 7: Settings page
- [x] 25 %: layout, sidebar, search with counts, pinning
- [x] 50 %: a widget for every field type, hotkey recorder with clash warnings
- [x] 75 %: profiles, Save & Apply, Undo, Reset, module restarts
- [x] 100 %: custom widgets, backups UI, snapshots of every section (built and tested here; the Settings check below is on the waiting list)

### Phase 8: Everyday tools
- [x] 25 %: lum-calc with all fixtures, Calculator panel and page, Timers with restore
- [x] 50 %: Notes complete
- [x] 75 %: Reminders complete, Clipboard page and Ctrl+B pop-up
- [ ] 100 %: QR, Today, Usage, Calendar, page boxes (all built and tested here; ticked once the Daily-drivable alpha check below is in)

### Phase 9: Screen tools
- [x] 25 %: capture backends, frozen multi-monitor picker base
- [x] 50 %: colour picker, ruler, text grab
- [x] 75 %: doodle complete
- [ ] 100 %: snapper on both systems, palette entries, tests (all built and tested here; ticked once the two-monitor check below is in)

### Phase 10: Automation
- [x] 25 %: model, engine core, logic and value nodes, expansion; fixtures pass
- [x] 50 %: all triggers and actions wired
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

### 2026-11-28, Phase 10 at 25 %: the automation engine (overall 63.0 %; 64.25 % with the open checks)

Branch `claude/bold-babbage-kscm6j`, started from `main` after `main` was fast-forwarded onto the Phase 9 branch (so `main`
now holds phases 3 to 9). The waiting checks from phases 3 to 9 were left alone.

**Done and green** (fmt, clippy on Linux and the Windows target, `cargo deny check`; 30 tests in lum-automation):
* **`lum-automation`** (no UI): `spec` (all 65 node types read from `fixtures/automation_nodes.json`, embedded in the build, with the
  four Windows help texts swapped in, D61), `doc` (automation.json as raw JSON, repaired in the same order as `_load` so key order
  matches; the first-start example, blank and copied workspaces, `.lumflow` import repaired straight away, D65), `text` (token
  expansion, `compare`, `hm`, `days`, `left`, Python's `%g` and `str()`, `splitlines`, `title`, `re.sub` templates through
  fancy-regex, `expanduser`), `engine` (D60, D62, D63: `Engine` with `step` / `next_deadline` on a `Clock`, `FakeClock` for tests,
  pulses delivered on the next pass through one queue, value pulls with Python's depth limit, the `max_rate` safety stop, pause
  through `Config.enabled`, status lines and progress through `info`, saved node state, variables, `run_node` with Python's
  "paused" / "switched off" cards, `edited` (a field change restarts only a trigger) and `replaced` (undo: everything restarts),
  `watch_pulses` and `take_updates` for the editor, `Host` for the clipboard and `Out` requests) and `runner` (`Automation`: one
  thread asleep until the next timer or a poke, `with` / `read` for the UI, saves 600 ms after the first change through
  `store::Writer`, D64, and once more at `shutdown`).
* Node types that run: every logic node (Wait, If, Gate, Count, Only once, Cooldown, Sequence, Take turns, Random chance, Repeat),
  every value node (Text, Number, Variable, Join text, Maths, Date / time, Clipboard text, Read a file, Replace text, Change text,
  Random number), Every…, Button, Set variable, Show and Note. The other 43 start with the status "not available yet".
* Tests: `tests/fixtures.rs` (compare, days, hm, number formatting, every expansion case, the three load cases with made-up ids
  lined up, and `sample_home/data/automation.json` saving back byte for byte), `tests/engine.rs` (14 graph traces on a fake clock:
  interval timing and progress, next-pass order, pulls, a value loop, the safety stop on a wire loop, Wait, Repeat, a count kept
  across a restart, paused and switched-off cards, edits, pulses only while watched, every logic node, every value node),
  `tests/runner.rs` (the real thread runs, wakes a watching editor, saves on the way out and picks up again) and
  `tests/throughput.rs`.
* Budget, engine side: **1 000 pulses a second** (Repeat every 1 ms through If, Show, Count and Set variable with a pull, 4 000
  deliveries) take 29 ms of work per simulated second in a debug build, the longest pass 0.5 ms. The UI side of the budget is
  measured with the editor (checkpoint 100 %).

**Left for the next checkpoints.**
* The app does not start the engine yet: nothing in `luminescent` or `lum-ui` uses `Automation`. Checkpoint 50 % starts it from the
  launcher (`data/automation.json`, Settings → Automation into `Config`, the clipboard through `lum_platform::clipboard` off the UI
  thread), answers `Out` in `launcher/tools.rs`, and wires the triggers and actions.
* `Out` only has `Notify` so far; the actions add their own variants.

**Surprising, worth knowing.**
* `jiff` 1.x calls it `ZonedDateTime`, not `Zoned`.
* Python's `setdefault("id", _id())` makes an id even when the key is there, so load fixtures can not be matched by feeding ids in
  order; `tests/fixtures.rs` lines up made-up ids instead.
* `rustup target add` must run inside `luminescent-rs/` (the pinned toolchain), or the Windows clippy finds no `core`.

**Next: Phase 10, checkpoint 50 %** (all triggers and actions wired to services and modules).

### 2026-11-27, Phase 9 built to 100 %, the two-monitor check waits on you (overall 60.75 %; 62 % with the check)

Branch `claude/luminescent-phase-9-screen-tools-s65lah`. It started at "Plan: phase 8 at 25 %" (main), so the session first
fast-forwarded it onto `origin/claude/pensive-galileo-6ms969` (phases 3 to 8 at 90 %, nothing lost, no force). **This branch holds
phases 3 to 9. Merge it into `main` (a fast-forward) before the next session starts.** The Hello tray, Looks right, Dashboard,
Settings and Daily-drivable alpha checks are still on the waiting list; nothing asked for them again.

**Done and green** (fmt, clippy on Linux and the Windows target, 626 tests under Xvfb with snapshots, `--smoke`, `cargo deny check`):
* **`lum-screen`** (new crate, no drawing): `colour` (HEX / RGB / HSL exactly as `colorsys` rounds, the nudge, `colours.json` as
  `Recent`; the sample saves back byte for byte), `frame` (a monitor's picture at real pixels, layout point to real pixel, crops,
  PNG), `loupe` (card, zoom, ringed cell, hint), `ruler` (Shift lock, W x H and distance in real pixels, the tag, what is copied),
  `snap` (the 15 spots, gaps, `usable_area`, the cycles, Windows' invisible borders), `ocr` (Tesseract languages, line joining,
  enlarging small boxes, the messages, the review size), `font` (Geist SemiBold through `ab_glyph`), and `doodle`: `Tool`,
  `Settings` (the whole section), `mark` (pen, pixel brush, highlighter, shapes, arrow, text, Clear, painted with tiny-skia),
  `canvas` (one per screen at its real scale, the stroke under way painted as it goes, dirty rectangles, undo by painting the
  marks again over a base that holds marks older than the history), `History` (one for every screen), `hold` (the hold-key rules
  as a state machine; every rule from the Python docstring has a test), `strip` (the tool strip's layout and hits) and `compose`
  (copy screen + drawing). 46 unit tests and the budget test.
* **`lum_platform::capture`**: every monitor at its real pixels through the wlr-screencopy protocol (wayland-client was already in the
  tree), `grim -o NAME -t ppm` when that fails, GDI on Windows; one thread per monitor; a `Fake` for tests. Checked against a real
  headless sway (two outputs, one at scale 2: 640x480 and 800x600 real pixels, right colours, 22 ms for both); the example
  `cargo run -p lum-platform --example grab` writes each monitor to a PNG. Also `ocr` (Tesseract through stdin, no temp file;
  `Windows.Media.Ocr` through the `windows` crate, Tesseract only when Windows has no OCR language; a real Tesseract read the test
  picture back here), `snap` (active window and its monitor's free area, float, size and move in one Lua call, back into tiling;
  Windows: foreground window, work area, restore a maximised window, invisible borders), `desktop::move_cursor`, `active_window`,
  `refocus`, `hotkeys::other_modifiers_down` and `mask_menu_key` (Windows), and `WindowPlacer::cover` (Windows puts a cover window
  exactly over its monitor in real pixels, up to three tries since a move to a monitor with another scale resizes it once more).
* **`lum_ui`**: `Screen.scale`, `window::install_capture` / `install_screens` / `screens_in` (tests put two monitors with different
  scales side by side in one window), `tools::frozen` (each screen's picture in its own window over its monitor, cover prefixes per
  tool, keys to the screen with the mouse, the time from the hotkey to the screens in the log), and the modules, in Python's order:
  **Doodle** (after Notes), **Colour Picker**, **Text Grab**, **Window Snapper**, **Screen Ruler** (after Calculator). The colour
  picker is also the colour dialog's eyedropper (`tools::colorpicker::eyedropper`, the dialog and the picker meet in the context's
  memory, so no owner needs wiring), with Recent colours in the tray (a submenu; `row:` ids run palette rows) and the palette (copy,
  accent, new theme; `Request::ThemeAccent` and `ThemeFromColour`). Text grab has the review panel (Ctrl+Enter copies). The doodle
  has every tool and setting, the strip, the circle pointer, the "N px" label, the wheel (saved quietly a second later through
  `Request::SetSetting`), the text box, blackboard, copy screen + drawing (saved to ~/Pictures/Luminescent when asked), panic, Esc,
  clear on release, start tool, the Windows Alt menu mask, and the keyboard handed back to the window that had it on Hyprland. The
  snapper's every spot is an action (palette, tray, dashboard Tools tab), the four directions cycle.
* **`UiModule`** gained `tray_menu`, `fixed_binds` (Doodle's Esc and Alt+Esc, not consuming) and `hold_setting` / `hold_event`
  (`HoldEvent::Down`, `Up`, `Interrupted`, `Refused`); the launcher registers holds first and tells a module when the key was
  refused (it then toggles, as Python does), and routes `HoldDown`, `HoldUp` and `Interrupted`.
* Tests: `crates/lum-ui/tests/frozen.rs` (3), `screen_tools.rs` (21: picker, ruler, text grab, the eyedropper), `doodle.rs` (12),
  snapper (3), capture, OCR, snap against the fake Hyprland, and `lum-screen`. Pictures in Dark and Light of the loupe, the ruler
  (crosshairs and a measure), text grab (selecting, review) and the doodle (every tool with the strip, the blackboard). `--smoke`
  opens the picker, the ruler and text grab on stand-in screens, closes them with Esc, checks the snapper's actions, and puts the
  doodle's blackboard up and takes it down with panic.
* Budgets: **doodle at 60 fps with 500 strokes**: `crates/lum-screen/tests/doodle_budget.rs` draws over 500 strokes on a 1920 x 1080
  canvas: a frame's work (painting the new piece and copying out what changed) is 11 µs mean, 15 µs p95 in a debug build, and the
  same as with no strokes (ratio 0.99). An undo paints all 500 again in 87 ms (debug), once. **Nothing polling while closed**:
  `--idle-check 20` (debug, Xvfb, llvmpipe) gives 0.08 % of a core, 7.9 wake-ups a second, 2 passes painted and 21 threads, against
  0.12 %, 11.3, 2 and 21 measured the same way at the end of phase 8: the screen tools add no thread and no wake-up while closed. The
  **picker within 150 ms of the hotkey** needs your desktop: the command is under "Waiting on the user".

**Left out on purpose or not done.**
* No `xcap` (D55). On desktops without screencopy or `grim` (KDE, GNOME on Wayland) the screen tools say they could not grab the
  screen; the portal is phase 16.
* The doodle's pointer is painted (egui can not make a cursor from a picture), so moving the mouse while drawing repaints; Python's
  pixmap cursor did not. Only while drawing is on.
* The highlighter's live preview is an egui line, which can darken where it crosses itself until the button comes up (the finished
  stroke is right).
* Panic records a Clear step, so Alt+F after panic brings the drawing back (Python's panic left the undo snapshots and the history
  half in step).
* Copy screen + drawing paints the drawings by hand everywhere but Linux, as Python did; if Windows' GDI copy already shows our
  overlay windows the drawing would show twice (see the check below).
* Not tried on a real desktop: anything with real windows, hotkeys, scaling or Windows (the check below).

**Surprising, worth knowing.**
* `Settings::get` only takes `section.key`; a whole section is `Settings::section`.
* kittest's `run()` lets time pass until nothing wants a repaint, so a 120 ms timer fires inside it; use `step()` to stay before it.
  Modifier keys for a drag are `Event::ModifiersChanged`; a key's own modifiers come with `key_press_modifiers`, and a module should
  read them from the key event (`i.modifiers` may not have them). Two harnesses in one test must not both take pictures.
* `IdTypeMap::remove_temp` needs `Default`; `remove::<T>(id)` does not. Calling `ctx.cumulative_pass_nr()` inside `ctx.data_mut`
  deadlocks.
* egui 0.37 has no `raw_scroll_delta`; wheel notches come from `Event::MouseWheel` (which has `phase` and `source` now).
* The Python label's HSL example (`hsl(246, 89%, 69%)` for #7B6CF6) is not what `colorsys` gives (`hsl(247, 88%, 69%)`); Rust
  matches `colorsys`.
* `sway` and `tesseract-ocr` install fine in this container (`apt-get install -y sway grim tesseract-ocr tesseract-ocr-eng`): headless
  sway (`WLR_BACKENDS=headless WLR_RENDERER=pixman`) is a real screencopy compositor for checks.

**Next: Phase 10, Automation** (Opus 5.5, xhigh), checkpoint 25 % (model, engine core, logic and value nodes, expansion;
fixtures pass). Phase 9's last checkpoint waits for the two-monitor check below; it does not block Phase 10.

### 2026-11-27, Phase 8 built to 100 %, the Daily-drivable alpha check waits on you (overall 56.0 %; 57 % with the check)

Branch `claude/pensive-galileo-6ms969`. It started at "Plan: phase 8 at 25 %", not at the 62 % the prompt expected, so the session
first fast-forwarded it onto `origin/ccr-a4cc2d9e-xg3o3z` (Notes and Reminders; nothing lost, no force). **This branch now holds
phases 3 to 8. Merge it into `main` (a fast-forward) before the next session starts**, or the next one finds Phase 8 at 25 % again.
The Hello tray, Looks right, Dashboard and Settings checks are still on the waiting list; nothing asked for them again.

**Done and green** (fmt, clippy on Linux and the Windows target, every test under Xvfb with snapshots, `--smoke`, `cargo deny check`):
* **Clipboard (Snippets).** `lum_core::snippets`: what a file is (`classify`: image, video, link, colour, code, text, file, from the
  extension and the first 600 characters), the folder listing (favourites row, folders by name, clips in the kept order), search
  across every folder (names and what text clips say, 200 matches), `Meta` over `.luminescent.json` (folder colours, favourites,
  order, `path_moved` after a move or rename; the sample file saves back byte for byte), moving and copying (`move_into` picks a
  free name and refuses a folder into itself), `fill_placeholders`, `save_clipboard` (files, then a picture, then text, named as
  Python names them), ages and sizes. `lum_platform::trash` (gio, trash-put or kioclient5; PowerShell on Windows) and
  `lum_platform::frame` (a video's first frame through `ffmpeg`, optional). `lum_ui::tools::snippets`: `view.rs` draws the
  breadcrumb, search (Ctrl+F, 220 ms pause), New folder / Open folder / Paste here, FAVOURITES, folder and clip cards (text, code,
  link, colour swatch, file glyph, picture) with only the rows in view; drag a card onto a folder, a folder in the breadcrumb or
  another card; right-click menus with the Python labels; hover preview after 650 ms; Backspace goes up. `mod.rs` is the module:
  the Ctrl+B pop-up (960 x 560 by default, dim behind it, closes on click, Esc or use), the "clipboard" overlay page (five columns, fewer in a
  small box), "No snippets folder yet" with Make this folder / Use another folder…, listing, thumbnails (`thumbs.rs`, EXIF turned,
  fitted to 720 x 520), copy, paste, save and Trash all on threads. Pasting waits 220 ms, Ctrl+click adds Enter at 340 ms and reopens
  the pop-up in the same folder at 450 ms. Actions "Open Snippets" and "Save the clipboard to Snippets", the hotkey and a tray entry.
  New `Request::SendKeys` and `Request::SetSetting`, answered in `launcher/tools.rs`.
* **QR code.** `lum_core::qr` (the `qrcode` crate: smallest version that fits, level M below 2 000 bytes and L above, 2 900 bytes at
  most, the Python wording for every message, an RGBA picture and an SVG writer) and `lum_ui::tools::qr`: the panel with the white code
  card, the line under it, the text box (code follows 250 ms after typing stops), From clipboard, Copy image (picture on the
  clipboard), Save as… (PNG, or SVG when the name ends in .svg), the hotkey and the "QR code of the clipboard" action. A real
  decoder (`rqrr`, dev dependency) reads the text back from the picture and from what the panel draws on screen.
* **Today, Usage, Calendar.** `lum_core::overview` (summary, timeline kinds, most used, day titles, "5m ago", the 30 day series, the
  top 25 rows) and `lum_calc::calendar` (the Atlas events and tasks in `atlas_sync.json`, repeat rules, a day's agenda with
  reminders, the month's weeks; all 12 `occurrences` cases of `atlas_cases.json` pass). `lum_ui::tools::overview` draws the three
  pages into any rect (they are not modules: the app's page closure calls `Overview::show`, so a dashboard box is the same call).
  Today: four boxes, timeline by the hour, MOST USED, ‹ ›. Usage: the 30 day bars, three columns with bars, Reset… (asks first).
  Calendar: the month with a dot on busy days, the day's agenda (Atlas events with their colour, tasks, reminders), the Atlas line,
  Sync with Atlas. In a small box each page stacks its parts.
* Tests: `crates/lum-ui/tests/snippets.rs` (20), `qr.rs` (7), `overview.rs` (22), plus 22 in `lum_core::snippets`, 9 in `qr`, 9 in
  `overview`, 7 in `lum_calc::calendar`, and the thumbnail, trash, frame and file-writing tests. Snapshots in Dark and Light of each
  panel and page, and of each page as a small box. `--smoke` now opens the Snippets and QR actions and draws the clipboard, today,
  stats and calendar pages.

* Numbers: `--idle-check 12` (debug build, Xvfb, llvmpipe; the 12 seconds include the one currency fetch 8 s after the start) gives 0.12 % of
  a core and 11.3 wake-ups a second with 2 passes painted, and 21 threads. The commit before this session, measured the same way
  in a second build, gives 0.13 %, 10.9, 2 painted and 20 threads, so the tools added here cost one thread (the Snippets file writer)
  and nothing else while closed. A longer run without the fetch is what the budget is judged on.

**Left out on purpose or not done.**
* Snippets: dragging a card out to another app (egui can not start an outgoing drag; Python handed the file over); link previews on
  cards (Python's thumbnail worker could fetch them, but the grid only ever asked for pictures and videos, so none ever showed);
  video thumbnails need `ffmpeg` (optional; Python used OpenCV); the hover preview stays inside the window (Qt's tooltip window could
  stick out of the pop-up); an animated GIF or WebP shows its first frame; a dashboard box does not remember its folder (`PageFrame`
  has no state, as with Notes); a rename onto a name that exists is refused (Python's `os.rename` replaced the file on Linux); a moved
  folder's own order list is not moved with it (as Python). The top-level order is stored under `.` as Python writes it; a file
  that has `""` there (the sample does) is read and kept under that name.
* QR: segno's SVG and PNG bytes differ from `qrcode`'s (same picture, may differ in mask or version choice; both scan). There is no
  page or box for it (Python has none).
* Today and Usage read the log when the page comes up (a scan of up to 2 MB on the UI thread, once per show) and do not refresh
  while open, as Python.
* Calendar: **Sync with Atlas opens Settings → Atlas Sync** until phase 13 (the Rust app has no Atlas module; Python's button starts
  hosting). The stored calendar is shown only while `atlas_sync.enabled` is on, as Python shows it while its module runs.
  Notifications for events about to start are phase 13.
* Not tried on a real desktop: the Ctrl+B hotkey and pasting into another app, the Wayland clipboard, Trash and Recycle Bin, the file
  dialogs, dragging with a real mouse, the dim window behind the pop-up. See "Waiting on the user".

**Surprising, worth knowing.**
* Text drawn with the painter has no accessibility node: give painted rows `widget_info` (`Role::ListItem` or `Button`) or kittest can
  not find them. `DimOverlay` in the harness paints over the panel it dims, so the pop-up snapshots set the dim to 0.
* A card below the fold can not be clicked by kittest (the test window was too short for the second row); the pop-up test clicks a
  card in the first row.
* Python's `_rel` of the top folder is `.` while the sample `.luminescent.json` has an empty name for it. Python never reads the
  sample's key; Rust reads both.
* `Counter.most_common` keeps first-seen order for ties; `top_rows` and `most_used` do too. The Python agenda sorts tasks (key 1)
  before timed events (key = milliseconds); kept.
* The `atlas_cases.json` repeat cases keep only the first 30 times of a long repeat; the test compares that prefix.
* jiff 1.0: `to_zoned_datetime` (not `to_zoned`), `ZonedDateTime`, `Date::first_of_month`, `DateTime::since((Unit::Second, other))`.
* `apt-get install` of the build packages failed until `apt-get update` ran; `cargo install cargo-deny --locked` takes a few minutes.
* The QR commit has one clippy lint inside a test (`manual_is_multiple_of`), fixed in the next commit.

**Next: Phase 9, the screen tools** (Opus 5.5, high). Before it, the Daily-drivable alpha check below (a day of use with a copied data
folder); it does not block starting Phase 9 but the 57 % milestone waits for it.

### 2026-11-27, Phase 8 at 62 %: Notes and Reminders (overall 53.2 %)

Branch `ccr-a4cc2d9e-xg3o3z`. The session first merged `origin/ccr-f1a7eeae-uxz8sm` (phases 3 to 8 at 25 %) into `main` by
fast-forward and pushed it, as asked, so `main` now has everything up to the Calculator and Timers. **This branch has Notes and
Reminders on top of that; merge it into `main` before the next session.** The Hello tray, Looks right, Dashboard and Settings
checks are still on the waiting list; nothing asked for them again.

**Done and green** (fmt, clippy on Linux and the Windows target, every test under Xvfb with snapshots, `--smoke`, `cargo deny check`):
* `lum_core::notes` (`notes.json` over raw JSON, so unknown keys and key order survive): `Notes`, `Note`, `Item`, search with
  `#tag`, `parse_tags`, `due_text` / `overdue`, Export text, the 12 second Undo trash, `Notes::order` (pinned first), `reorder`,
  sticky places read as a list or as `{x, y, w, h, screen}` and written in the shape found, `open` read as an index or an id and
  written as an index, `todo_changes` / `sync_for` (what Reminders must do when a due date changes). The sample file saves back
  byte for byte. 18 tests.
* `lum_ui::tools::notes` (`mod.rs`, `view.rs`, `markdown.rs`): the panel (list with cards, drag to reorder, Undo line, note
  text, 👁 preview, to-do rows with tick box, handle, inline edit on double click, due dates, right-click menus, progress line,
  Clear done), any number of sticky notes (own panels, note colour on the header tick, saved places), edit mode (side buttons,
  dim of the screens, anchor bar), the overlay page (list left, note right; one column under 640 px so it fits a dashboard box),
  palette rows `note:<id>`, tray entry, hotkeys, Export through the save dialog on a worker thread. `markdown.rs` parses with
  `pulldown-cmark` into a small block tree (tested) and draws headings, emphasis, strike, code, links (only http, https and
  mailto open), lists, tasks, quotes, tables, rules.
* `lum_calc::reminder`: `next_due` for every schedule type, `describe`, `new_reminder`, `parse_quick` (with `fancy-regex`,
  because Python's patterns use lookahead), the `State` engine (`prime`, `tick` with the catch-up window, snooze, "last in the
  future", `next_reminder`, `next_wake`), `Edit` / `apply_edit` / `quick_reminder`. All 150 next-due, 41 describe and 23 quick
  cases pass; `reminders_state.json` saves back byte for byte and the schedule signature equals Python's `json.dumps(sort_keys)`.
* `lum_ui::tools::reminders`: the module wakes at the moment the next reminder is due (never polls every 10 s), fires (event
  `reminder.fired`, theme switch, sound repeated every 3 s, desktop notification, pop-up), the pop-up window (card in the toasts'
  colours, rises into place at bottom anchors, runs out by itself, Snooze with the 5 to 60 minute menu, Done, ✕, stacking per
  anchor, primary or cursor screen), startup reminders once per run, palette rows `remind:quick:...`, `todo_reminder` for Notes,
  `preview_reminder`, the `⏰` status chip (`UiModule::chips`), tray entry "Reminders…".
* Settings → Reminders editor (`settings/reminders.rs`): list with Add, Duplicate, Delete; REMINDER, WHEN and HOW IT TELLS YOU
  cards; changes only the keys that were edited; Browse for the sound file; "Test this reminder now". `Env.themes` and
  `Out.preview_reminder` are new.
* App side: `Request::TodoReminder`, `ReminderEdit`, `ApplyTheme`, `SystemNotice`; edits to the list are stored through
  `save_quiet`; widget kit gets `drag_rows` (drag to reorder), `when` (date and time picker) and `AskDialog::ask_choice` /
  `ask_datetime`; `FloatingPanel` gets a back arrow, a header height and a coloured tick; `tools::edit::keep_on_screen`.
* Tests: `crates/lum-ui/tests/notes.rs` and `reminders.rs` (snapshots Dark and Light of the list, to-do, text, preview, sticky, edit
  mode, page and pop-up; typing, ticking, clearing, adding, search, delete with Undo, reminders dropped with a note, a reminder
  going off, Snooze, quick reminders, to-do reminders, theme switch), `settings.rs` (editor add, duplicate, delete, edit one key,
  test button, two pictures with the sample reminders), `--smoke` check "everyday tools".

**Left out on purpose or not done.**
* Notes: no Delete in a sticky's menu (Python has none either); the page's last open note is not kept in a dashboard box
  (`PageFrame` has no state); the page's list has no drag (Python's has none); the 👁 preview has no images; Export has no
  `.txt` filter choice beyond the dialog's two filters.
* Reminders: the pop-up does not answer Esc or Enter (it never takes the keyboard, so Qt's key handler could not either in
  practice); the date field in the editor is a text box (`YYYY-MM-DD`) and the time a text box (`HH:MM`), not Qt's calendar pop-up;
  a missed "once" reminder fires only inside the catch-up window, as before; the Settings page shows the new list after
  `QuietlySaved` only when it reloads, as the other quiet saves.
* `＋` (U+FF0B) is not in the bundled fonts, so the Python labels "＋ New note" and "＋ Add" are "+ New note" and "+ Add".
* Not tried on a real desktop: global hotkeys, sound, the pop-up windows on Hyprland and Windows, Export's file dialog, the dim and
  anchor bar of Notes' edit mode, sticky windows.

**Surprising, worth knowing.**
* `egui_kittest` panics on a glyph no font has while the app draws a box: probe suspicious characters with
  `ctx.set_missing_glyph_policy(MissingGlyphPolicy::Panic)` after two passes (fonts apply from the second one).
* `ui.scope` is no place to set `interact_size.y` for a `horizontal_wrapped` row: the row height is taken when the row starts.
* Python's `datetime.fromisoformat("2026-11-24T09:00")` and jiff's `DateTime` parse agree for every file in the fixtures; a due date
  six days away still reads as a weekday ("Mon 08:05"), seven as "01 Dec 08:05".
* `cargo deny` is not preinstalled here: `cargo install cargo-deny --locked`.

**Next: Clipboard (Snippets)** (checkpoint 75 %): `modules/snippets.py` (1 313 lines). Plan it as `lum-core` or `lum-calc`-side data
(`.luminescent.json` in the snippets folder, folder colours, favourites, order, placeholders), then `tools/snippets` (the page and
the Ctrl+B pop-up grid, thumbnails on a worker thread, breadcrumb, search across folders, "+ Save clipboard here", drag to reorder
and into folders, hover preview, paste into the app you were using through `lum_platform`). Then QR, Today, Usage, Calendar.

### 2026-11-27, Phase 8 at 25 %: lum-calc, Calculator and Timers (overall 49.5 %)

Branch `ccr-f1a7eeae-uxz8sm`. It started at `main` (phases 1 and 2), not at the phase 7 branch the prompt expected, so this
session first fast-forwarded it onto `origin/ccr-6cfe5063-2qz557` (phases 3 to 7, nothing lost, no force). **This branch
now holds phases 3 to 8 so far, and `main` has only 1 and 2. Merge it before the next session starts**, or the next
one will find Phase 3 at 0 % again. The Hello tray, Looks right, Dashboard and Settings checks are still on the waiting
list; nothing asked for them again.

**Done and green** (fmt, clippy on Linux and the Windows target, every test under Xvfb with snapshots, `--smoke`,
`cargo deny check`):
* `lum-calc` (new content): `num` (Python's int and float rules on `num-bigint`: `/` is always a decimal, `//` and `%`
  follow the divisor, whole numbers as big as they like), `eval` (hand-written lexer, Pratt-style parser with Python's
  precedence and `-2**2`, tree walk, limits `MAX_POW` 10 000 and shifts 4 096, `fact` 1 000, nesting depth 150; strings,
  lists, comparisons, attributes and other calls read fine and answer "not allowed in a sum" as Python does), `funcs`,
  `format` (`fmt`, `pin_name`), `convert` (157 units, temperatures, symbols, currency, date sums), `rates` (`ureq` into
  `currency.json`, at most once a day), `timeparse`, `book` (history, pins, panel over raw JSON, `compute` with `ans`,
  pins, conversions and dates, `reformat`, pin, rename, recalc, folders and colours), `timer` (the timer state
  machine on `Instant`, laps, restore from the wall clock, `timers.json` keeping unknown keys, `parse_entry`).
  Tests: every case of `calc_cases.json` (206 sums x both angle modes, 576 formats, 48 computes, 12 plain, pin names),
  `convert_cases.json` (106 conversions with and without rates, the unit table, 38 date sums, 9 dates),
  `timeparse_cases.json` (78 durations, 38 clocks, 16 `fmt_secs`), and `sample_home/data/calculator.json` saved back
  byte for byte. On Linux floats must match to the last digit; other systems allow 1e-13.
* `lum_ui::module`: `Services` and `Request` (what a module asks of the app), `Inbox<Cmd>` (commands from actions,
  hotkeys and worker threads), `Bind`, and on `UiModule`: `binds`, `bind_pressed`, `page_key`, `page`, `reset_page`,
  `tray_actions`, `status_lines`, `palette_entries`, `palette_live`, `run_palette`. `ModuleHost::running_refs` and
  `running_module`. The app side is `launcher/tools.rs`: hotkeys registered after start, Save and "Turn it on" (owner =
  the module key), requests answered once per pass, module rows in the palette and the tray menu, the tray tooltip made
  again every 15 s while a timer counts, pages found by key in `show_overlay` (Settings' module info is made first so the
  page can borrow the module), the overlay's chips from `status_lines`, repair resets a module's page.
* `lum_ui::tools`: `calculator` (CalcPanel as `View::show` into any rect, so the panel, the page and a dashboard box
  share it; pins in folders with colours, history grouped by day, DEC / HEX / BIN / OCT, keypad, live preview, up and
  down through the history, Ctrl+Enter, Ctrl+C on an empty box, right-click menus, the dialogs, `⋯` menu, palette sums and
  pins, "Update currency rates"), `timer` (panels with ring or bar, laps, DONE title and tone, flash, sound, `timer.finished`,
  activity lines, typing a time into the panel or the page, ⏲, edit mode with the Reset and switch buttons and the anchor
  bar, restore still running, a late finish 1.5 s after the start, the Timers page with cards), `edit` (anchor bar window,
  `snap_all`), `clip`. Widget kit: `IconShape` (Copy, Keypad, Clock, Eye drawn with the painter) and
  `IconButton::shape`; `FloatingPanel::flash`.
* Tests: `crates/lum-ui/tests/calculator.rs` and `timer.rs` (snapshots of both panels and pages in Dark and Light,
  the keypad, edit mode with the bar style; typing, keypad, Ctrl+Enter asking the overlay to close, a bad sum, the
  palette, a typed time, a refused time, restore of a running timer, a timer that ran out while closed, closing),
  `--smoke` check "calculator and timers".
* Numbers: idle with both modules on and the currency fetch off is 0.01 % CPU and 0.2 wake-ups a second (debug build,
  Xvfb); the one fetch 8 s after the start shows as extra wake-ups in a short `--idle-check`, as in Python.

**Left out on purpose or not done.**
* The panel's "DONE" and the run dot: the green dot sits with the header buttons, not after the title; the flash blinks the
  border only (the Python one restyled the whole panel the same way).
* Pins and history values past 64 bits (`fact(100)`) are written to `calculator.json` as the nearest decimal; Python wrote the
  exact whole number. They still calculate exactly while the app runs.
* `(-8)**0.5` says "no finite answer"; Python answered with a complex number the panel could not show anyway.
* Calculator pins are not draggable into folders (only the menu); the page has no fade between pages.
* The `calculator.panel` "visible" flag is saved when the panel is toggled; the tray entry is "Calculator" under Settings
  → General → Tray menu → actions.
* Not tried on a real desktop: the global hotkeys, the sound, the flash, edit mode's anchor window and focus on DONE.

**Surprising, worth knowing.**
* **The branch premise was wrong** (see the first paragraph). Check `git log` and the Overall line before building.
* `ureq` with rustls pulls in `ring` (a C compiler for the Windows clippy run: `gcc-mingw-w64-x86-64`) and `webpki-roots`,
  whose licence CDLA-Permissive-2.0 is now allowed in `deny.toml` (Mozilla's list of certificate authorities).
* Python's `round(2.5)` through the calculator's `round(x, 0)` gives `2.0`, a decimal; `fmt_secs` of a negative time uses
  floor division (`-5` is `-1h 59m`); both kept.
* egui: a single-line `TextEdit` has lost focus by the time you ask after Enter; `Ui::id` and `Id::new` are deprecated; the
  panel's Esc comes back only when no widget has the keyboard, so the timer's typing box handles Esc through the panel's
  `escape` flag.
* `UPDATE_SNAPSHOTS=1` does not rewrite a picture that is within the tolerance; delete the PNG when a layout moved a little.

**Next: Notes** (checkpoint 50 %): `modules/notes.py` (1 475 lines) is the biggest tool in the phase. Plan it as `lum-core`
or `lum-calc`-side data (`notes.json`, to-do items, tags, due dates, export, markdown), then `tools/notes` (the panel with
the 👁 preview, sticky notes that open several at once, edit mode with the anchor bar from `tools::edit` and the dim from
`widgets::DimOverlay`, drag to reorder, Undo for 12 s after deleting) and the Notes page and box. Due dates create
reminders, so leave a `Request` for that until Reminders lands. Markdown rendering: `pulldown-cmark` (CRATES.md).

### 2026-11-27, Phase 7 built to 100 % (overall 41.75 %; 43.75 % with every check in, 47 % is the phase's own end)

Branch `ccr-6cfe5063-2qz557`. It started at `main` (phases 1 and 2), so this session first merged
`claude/epic-sagan-5cj237`, which holds phases 3 to 6: **this branch now holds phases 3 to 7, and `main` has only 1
and 2. Merge it before starting phase 8.** Phase 3's Hello tray, phase 5's Looks right and phase 6's Dashboard checks
are still on the waiting list; nothing asked for them again.

**Done and green** (fmt, clippy on Linux and the Windows target, every test under Xvfb with snapshots, `--smoke`):
* `lum_ui::settings` (`SettingsUi`): the page drawn into whatever `ui.max_rect()` it is given (the overlay panel or a
  dashboard box), from `Schema`. Profile bar (switch, New profile, Rename, Delete, the settings path), sidebar with
  search (`/` focuses it), "Only matching settings" (stored as `ui.settings_search_only`), a count per section ("N found"),
  highlighted matches in labels, help texts, titles and the sidebar, pinned sections (`ui.settings_pinned`, star on hover),
  sections built lazily (normalised when first shown, the rest one per frame until all are built, then no repaints),
  rows of label and field with a hairline, the footer (Reset page, status, Undo changes, Save & Apply). Under 640 px wide
  the sidebar becomes a search box and a section list above the page; the profile bar scrolls sideways.
* A widget for every field type: hotkey recorder (`widgets::recorded_key` and `widgets::combo_of` moved into the kit, the
  dashboard item dialog and the theme editor use them; Record, click the caps, clear; keys pause through
  `Out.suspend_keys`; clash warnings from the other hotkey fields on the page, `Env.key_user` for dashboard items and
  theme keys, and the validator), text, folder and file (Browse… through the file dialog wrapper, Qt filters turned into
  extension lists), int, float, opacity slider, check box with its help beside it, colour (the phase 4 dialog, applied on
  Done), choice (as wide as its longest entry), and the custom widgets: navigation bar order (needs its own state, see
  below), Show on start, Tray menu, Backup (Export / Import through the file dialogs, a check and a question before it
  replaces anything, restore and restart), Quit everything and Restart, "Show a notification" (shows a card with the
  values on the page, saved or not: `Look::from_values`), Updates (the version only; checking and installing is phase 15).
  The Reminders list is a slot with a line of text until phase 8.
* A module's section shows its buttons while it runs and "Turn it on" while it is off (`Env.module`); a module that failed
  to start shows why.
* Save & Apply: checks hotkeys used twice (a message listing them), writes through `Settings::replace_silently`, and hands
  the app `Out.saved` with the sections that changed. `App::settings_saved` re-registers the hotkeys, makes the overlay
  config, cards and tray again, syncs run-at-login (not for a test copy), restyles, and calls the new
  `ModuleHost::apply_changed`, which restarts **only the modules whose section changed** (start and stop still follow
  `enabled`). Fields marked restart ask "Restart Luminescent now?". Profile switches, new, rename and delete work as in
  Python; unsaved changes are asked about first; a profile switch applies every section.
* The app: `launcher/settings.rs`, the "settings" page in `show_overlay`, palette entries `settings:<section>` and
  `ctl settings <section>` open on that section, `AppMsg::SettingsFile` and `AppMsg::Backup` (zip work runs on the
  tokio pool), `with_env` split into `EnvData` so the page can hold the settings mutably.
* Tests: `crates/lum-ui/tests/settings.rs` (every field of both schema dumps gets the widget `control_of` says, the Dashboard
  and Scripts fields with their help texts, save writes what Python would, undo and reset, search and only-matching, pins,
  recording and clash warnings, profiles, turn it on and module buttons, the notification and quit buttons, backup
  dialogs, the switcher list, small boxes, the page going away while recording, no repaints when settled; snapshots of
  every section in Dark and Light at 980 x 620, two tall ones with the buttons and lists, and the small box), model tests in
  `settings/model.rs`, `apply_changed` in lum-core, `--smoke` check "settings page".

**Left out on purpose or not done.**
* The hotkey field is not typed into (the Python box also takes text such as `code:123` and bare modifiers); click or Record
  only, as in the dashboard dialog and theme editor. Clash with the user's own Hyprland binds is not shown live (the
  hotkey thread owns that query; asking it from the UI thread would block); registering still refuses them with the
  Python wording.
* The section change does not fade (Python's 150 ms).
* The colour field applies on Done only (no live preview); no eyedropper request is handled from this dialog yet.
* Updates widget: version line and a disabled button (phase 15). Reminders editor: slot (phase 8). Keybind lists: phase 13.

**Surprising, worth knowing.**
* Python's `hk.normalize` writes "super" as "windows", so the first Save in Python rewrites the Snapper's default hotkeys.
  Rust does the same (parity), but decides what restarts by comparing with how the page showed the values at first, so
  that rewrite alone restarts nothing.
* The page draws into `ui.max_rect()`; the rect `sub` receives is the window in desktop coordinates (for dialogs).
* The dashboard store is borrowed mutably while its boxes draw, so the Settings page checks dashboard hotkeys against a
  map the app refreshes whenever it registers them (`App::dashboard_keys`).
* The nav-bar editor keeps pages that are switched off in their place, which the stored list cannot say, so it has its own
  state (`NavState`) that starts from the stored list.
* Sidebar entries and checkboxes need accessible names for kittest; `Checkbox::name` was added. Labels such as "Notes" and
  "Restart" occur many times on the page, so tests look up by role and label.
* This container needed `libasound2-dev pkg-config libxkbcommon-x11-0 mesa-vulkan-drivers xvfb` installed first.

### 2026-11-26, Phase 6 built to 100 %, the Dashboard check waits on you (overall 37.75 %; 43 % with all three checks)

Branch `claude/epic-sagan-5cj237`. It started at `main`, which has none of phases 3 to 5, so it was fast-forwarded onto
`origin/claude/keen-keller-1quy00` (phases 3, 4 and 5) first: **this branch holds phases 3 to 6, and `main` has only 1
and 2. Merge it (or `keen-keller` plus this) before starting phase 7.** Phase 3's Hello tray check and phase 5's Looks
right check are still on the waiting list; nothing asked for them again.

**Done and green** (fmt, clippy on Linux and the Windows target, every test under Xvfb with snapshots, `--smoke`):
* `lum_core::dashboard` (`DashboardStore`, D39): `dashboard.json` over raw JSON with typed views; every `_load` rule (layout 1
  and 2 moved over once, defaults filled, bad boards dropped, pins checked, the first start with the Tools tab and the
  File Launcher shortcuts); boards (add as empty / with Tools / copy, rename, move, delete, pins per screen); boxes (tab,
  page, picture); items (add, update, delete with group clean-up, move by drop / delta, sort, files dropped in);
  geometry saved through the background writer; `hotkey_users`. Tests: all five cases of
  `dashboard_migration_cases.json` (random ids compared as placeholders), the sample home saved back byte for byte,
  item moves, copied boards, pins, dropped files, snapping with halves to even.
* `lum_ui::dashboard` (`DashboardUi`): the page drawn once per screen from the overlay's `PageSlot` (now also carries the
  screen's name, its canvas, `to_desktop`, every screen and `chrome_bottom`). Tab boxes (accent bar, mono caps title,
  zero-padded count, chevron, fold that glides, right-edge resize), rows (badge or real icon off the UI thread, ROOT, green
  dot, hotkey hint, search highlight, tooltips as Python), page boxes (the app's page function draws them; ↗ and ✕;
  resizable from edge and corner), picture boxes (fit / fill / stretch, GIFs play, frame, corners, opacity, title bar), the
  toolbar (dashboards as pills or one menu when many, + Add, Tidy up and its menu, Undo for 12 s, Lock, search with the match
  count and Enter runs the first), selection band, Ctrl / Shift click, Ctrl+A, Delete, Esc, Ctrl+Z, tidy up in
  columns / grid / rows by position / name / size (`tidy.rs`, pure and tested), drag with snap (`round_ties_even`),
  drags of boxes and rows across screens (D41), files dropped on a box, every menu of the Python page with its labels,
  the item dialog (name, File… / Folder…, arguments, ask, working folder, admin, terminal, hotkey recorder with clash
  warning, group members) and every question (new tab, rename, heading, new dashboard, delete, remove).
* The app (`launcher/dashboard.rs`): items run (programs, files, folders, links, commands, terminal commands, actions, groups
  with `dashboard.group_delay_ms`, arguments asked in a dialog of their own), Stop, a process table checked every 1.5 s
  (D42), usage logging and `dashboard.item_run`, `dashboard.switched`, item hotkeys under owner `dashboard` for every
  board with the overlay closed, palette rows `item:<id>`, the tray's Dashboard submenu and "Edit the dashboard…", Open
  terminal here, Show in file manager, "Apps opened while the overlay is up appear on top of it" (Hyprland
  `float_on_top` / `unfloat`, `Desktop::float_foreign`), repair resets the page's state.
* Numbers (release, no GPU): dragging a box with 30 boxes and 300 items on screen takes **1.78 ms a frame** (budget 16 ms).
* Tests: `crates/lum-ui/tests/dashboard.rs` (snapshots Dark, Light, empty; drag and snap, lock, fold, double click, band
  select and Delete, tidy and undo, search and Enter, row drop between boxes, the row menu to the item dialog and a
  save, the 30 x 300 drag, and a two-screen harness: a pinned board on the second screen, a box dragged across it),
  `--smoke` check "dashboard".

**Left out on purpose or not done.**
* Animated WebP shows its first frame; GIFs play (decoded on a thread, at most 400 frames, shown by the clock, a repaint
  only at each frame's end while the box is drawn).
* A page box saves its state only once a page has state to give (`DashboardStore::set_page_state` is ready); only Themes is
  a real page so far, the rest are placeholders.
* The Tools tab is made from the registered actions, so a first start before any module exists has an empty one (Python
  lists every module's actions, running or not).
* "Add files…" takes one file (the dialog wrapper picks one); group members are reordered with ↑ ↓ buttons, not dragged;
  boxes do not fade in when they appear or a board is switched to; the Caelestia "Lock screen" button is phase 14.
* Glyphs: `⤢` has no font, the "full page" button is `↗`. SVG icons are not drawn (rows keep their badge).
* Not tried on a real desktop: dragging across real monitors (the hand-over uses Hyprland's cursor), files dropped from a
  file manager, Windows. See "Waiting on the user".

**Surprising, worth knowing.**
* The dashboard is drawn once per screen with the same state, so anything it does per frame (the pointer on the desktop,
  the record of what was drawn) is kept per frame, not per call (`begin_pass` swaps the records on a new frame number).
* Python's `round()` rounds halves to even and it matters: 228 / 8 is 28.5, so the first box goes at 224, and a box
  dropped at 140 snaps to 144.
* A drag asks for a repaint after every move, so kittest's `run` never settles during one: use `step()`. A double click needs
  `ctx.options_mut(|o| o.input_options.max_double_click_delay = 5.0)` in tests (a harness frame is a quarter of a second),
  `focus()` then `run()` before `type_text`, and rows and menu entries now have accessible names.
* `egui::WidgetType` is deprecated (`egui::Role`); the Edition 2024 `impl Iterator` rules need `+ use<'a>` on views that
  hand out iterators.

### 2026-11-26, Phase 5 built to 100 %, the Looks right check waits on you (overall 33.25 %; 37 % with both checks)

Branch `claude/keen-keller-1quy00` (phases 3, 4 and 5; main has none of them yet). The 100 % checkpoint is ticked
once your Hyprland and Windows 11 results are in (steps under "Waiting on the user"); then phase 5 is at 100 % and
overall at 34.75 % (37 % with phase 3's Hello tray check too).

**Done and green** (fmt, clippy on Linux and the Windows target, every test under Xvfb with snapshots, `--smoke`,
`cargo deny`, `--idle-check` in release under Xvfb: 0 frames painted after the overlay was opened and hidden).
* `lum_ui::toast`: Luminescent cards (position, width, the five accent styles with the glow at the top-left corner
  as in Python, how long they stay with the extra time for long text, most on screen, see-through, text size,
  icon, the error / warning accent from the title, hover keeps it, ✕ on hover, a click closes or opens a long one,
  right-click copies, the Notifications look of the theme). `AppNotifier` queues them behind the same `Notifier`
  (`general.notify_style`, `general.tray_messages`); `toast()` always goes to the desktop as well. The theme
  previews draw their card with the same `toast::paint_card`.
* `lum_ui::alert`: the warning in the middle of the screen with the mouse (levels, icons, Got it, Enter / Esc /
  Space, the count down, one per title, the Warnings look of the theme). `App::alert` is what automation and
  performance alerts will call.
* `lum_ui::dialog::DialogWindow`: a dialog over the screen with the mouse when nothing of ours is open. Quit
  everything (tray, `general.force_quit_hotkey` "quit all", the palette's "Quit Luminescent…") asks first.
  Restart and Restart in safe mode (tray, palette; "Safe mode: restart normally" in safe mode) shut down, let go of
  the lock and start again (`lum_platform::instance::relaunch`). Tray → Settings… opens the Settings page.
* Prewarm 3 s after start (D33); the whole overlay is thrown away and rebuilt if drawing it panics (D34).
* `ctl notify <title> | <text>` and `ctl alert <title> | <text>`, to try them and for scripts.
* Fixed on the way: without Hyprland no screens are listed, and the cards fell back to the 1x1 main window and
  panicked in `clamp` (min above max). Every window now falls back to `window::fallback_screen` (the main window's
  monitor size).
* Records: D33 to D38 in ARCHITECTURE.md, FEATURE_INVENTORY.md overlay lines, the Windows acrylic idea in
  NEW_FEATURES.md.

**Measured here** (release, Xvfb, llvmpipe, 1920x1080): first frame of the overlay 29 to 52 ms after asking; idle
0.01 % CPU, 0.3 wake-ups/s, 0 frames painted after it was hidden; 2 000 palette entries well under 5 ms. A real GPU
should do better; your numbers decide whether prewarm needs to keep windows alive (below).

**Left out on purpose.**
* Apps opened while the overlay is up floating above it (`ui.apps_over_overlay`, Python `_foreign_opened`): needs the
  Hyprland `openwindow` events for foreign windows; it belongs with the dashboard running items (phase 6).
* The theme crossfade on rebuild (Python lays a picture of the old look over the new one): egui restyles in one
  frame, so there is nothing to hide.
* Palette rows for dashboard items, notes, colours, pins, automations, sums and quick reminders come with their
  phases (`App::palette_entries`, `run_palette_entry`, `Palette::show`'s `extra`). Check for updates and Update now
  come with phase 15.
* Chip lines (timers, Hearing Guard, reminders, media, CPU and memory) come with their modules (`ChromeData`).
* If your open latency is over 50 ms on a real desktop: keep the overlay's windows alive and hidden between opens
  (X11 and Windows can hide a window; Wayland can not, see the phase 2 note), or open them at prewarm off screen.

### 2026-11-26, Phase 5 at 75 % (overall 33.25 %)

`lum_ui::palette` (nucleo-matcher over haystacks made once per open, the recent list in `palette.json` through a
background writer, rows like the Python delegate, ↑↓ PgUp PgDn, Tab, Enter, Esc, closes when it loses the keyboard;
results follow the typing in the same pass; 2 000 entries well under the 5 ms budget) and `lum_ui::cheatsheet`
(cards per settings section, OFF greyed, hold or toggle mode). `HotkeyManager::watch_release` reports when the
cheat sheet's keys are let go (every 50 ms on the hotkey thread, only while it is up; Hyprland that can not tell
switches it to toggle mode, as Python). The app's index: pages, module actions, themes, settings sections and the
`lum:` entries Python has, except updates (phase 15) and restart / quit (next checkpoint). Sums and quick reminders
come with phase 8 through `Palette::show`'s `extra`.

### 2026-11-26, Phase 5 at 50 % (overall 31.75 %)

`overlay/chrome.rs`: the chrome layer (`ui.chrome_items`, every kind, the default layout, ✎ edit mode with the bar,
+ Add, Reset, Done, drag on the dashboard's snap grid with Python's round-half-to-even, the right-click menu with
Freeze, Duplicate, Edit text… / Your name…, Size, Move to screen, Remove, Done editing; Esc leaves edit mode first).
The app gives it the time and date (`ChromeData`) and repaints once a minute while the overlay is up. Chip lines
(timers, Hearing Guard, reminders, media, CPU) are empty until those modules exist.

### 2026-11-26, Phase 5 at 25 % (overall 30.25 %; 31.75 % with phase 3's Hello tray check)

Branch `claude/keen-keller-1quy00`, which starts from phase 4's branch (it holds phases 3 and 4; main does not yet).
`lum_ui::overlay` is the real overlay now: `MainOverlay` (one window per screen, the cover titles, fade in and out,
the panel, `backdrop.rs`, the switcher with its sliding marker, ✕ and the hotkey hint, Ctrl+1…9, Esc, a click on
the dim around a panel page, typing on the Dashboard or Themes page asks for the palette, a page that panics is
reported for repair), `pages.rs` (the page lists, `nav_order`, placeholders). The Themes page draws in the panel;
`ThemesUi` keeps the editor and the colour dialog as windows. `ctl overlay <page>`, the "Jump to" hotkeys and
`general.settings_hotkey` open pages; the last page is remembered. `ctl status` has `overlay_open_ms`, and
`--idle-check` opens and hides the overlay before it measures (Xvfb, debug: first frame 53 ms, then 0 frames).
Snapshots `overlay_*`; the Themes and editor snapshots changed because the page is no longer in a window.

### 2026-11-26, Phase 4 at 100 % (overall 28.75 %, 31 % once phase 3's Hello tray check is confirmed)

Branch `claude/exciting-brown-fjwlud`. Phase 3's Hello tray check is still "unsure" from the user and stays under
"Waiting on the user"; nothing asks for it again.

**Done and green** (fmt, clippy on Linux and the Windows target, all tests under Xvfb, `--smoke`, `cargo deny`):
* The Themes window: `lum_ui::themes::ThemesUi` (a `FloatingPanel` holding `ThemesPage`, the shared `ColourDialog` with
  `picker.json` and `colours.json`, and the editor). The app draws it from `show_modules` and opens it with
  `ctl overlay themes` (the page switcher is phase 5). File dialogs go through `lum_platform::dialogs`, the answer
  comes back as `AppMsg::ThemeFile` into `ThemesUi::file_chosen`, notices go to the Notifier, and `Out.suspend_keys`
  suspends the hotkeys while the editor records a key. `themes::Env` carries what the window needs from the app
  (profile names, who owns a key, toast style, whether Hyprland and Caelestia are there).
* The editor (`themes/editor/`: `mod.rs` state, undo / redo, save / cancel, left and right columns; `pages.rs` General,
  Colours, When it is applied, a part's own look, desktop stubs; `preview.rs`; `tools.rs` the eight quick tools; `values.rs`
  reading and writing the raw theme). Undo / redo uses `lum_theme::history::History` (a 700 ms burst is one step; a quick
  tool is always one). Recently changed (lum scope only), favourites (`store.set_favourite`), Save keeps the recent list
  with the theme, Cancel and Esc and the ✕ ask first when something changed. The desktop pages (Caelestia colours and look,
  Glass, Window borders, Terminal) are stubs that say they come later; Caelestia ones are hidden until the module exists.
* Tests: `crates/lum-ui/tests/themes.rs` (snapshots of the page in Dark, Light and a gradient theme "Dusk"; the editor's
  General and Colours pages in the same three; a module page and a quick tool; the colour dialog solid in Light and
  gradient in Dark; behaviour: Save keeps a quick tool, Ctrl+Z takes it back, Cancel asks, a new theme is drawn by the very
  next pass). `themes::demo` (a store and window of their own) serves the tests and `--smoke`, which now walks every
  editor page, every quick tool and the colour dialog in three looks and checks `ctl overlay themes`.
* Records: D31 (rfd with xdg-portal only) and D32 (`Theme` is the raw JSON, saves through a background writer) were already
  in the decision log; nothing new to add. FEATURE_INVENTORY Themes lines ticked except the desktop ones.

**Fixed on the way.** The page asked for a repaint every frame (only when an action ran now; the kittest `run` caught it).
`＋` has no glyph in any bundled font and panics in egui's layout: use `+`. A long mono line in the colour dialog widened
the whole dialog and pushed Done off the edge (it wraps now). `FloatingPanel` is 85 % opaque by default, so panels that
sit over others (the editor, the quick tool, the colour dialog) set `opacity = 1.0`.

**Left out on purpose.**
* Font is a text box (egui has no list of the system's fonts); the theme's font family is stored but the app only has
  the bundled Geist until a later phase loads system fonts.
* The "From a picture" tool asks for a file with the system dialog; the Caelestia wallpaper shortcut is phase 14.
* The eyedropper button in the colour dialog waits for phase 9's screen picker (`ColourDialog::take_screen_request`).
* "Switching restyles within one frame" is tested as: the pass after `Tokens::install` already reads the new tokens (a
  headless pass), not as a timed measurement on a real desktop.
* Not tried on a real desktop: the editor and quick tool as separate windows (embedded rendering is what the tests see),
  key recording in the General page, the file dialogs. See "Waiting on the user".

### 2026-11-25, Phase 4 at 50 % (overall 24.75 %), stopped early because the session's usage ran out

Branch `claude/exciting-brown-fjwlud` (phase 3's branch `claude/hopeful-pascal-8kj34a` is merged into it). Phase 3's Hello tray
check: the user answered "unsure" for both Hyprland and Windows 11, so phase 3's 100 % stays unticked and the check stays
under "Waiting on the user".

**Done and green** (fmt, clippy on Linux and the Windows target, all tests, `--smoke`, `cargo deny`):
* `lum-theme`: `colour` (Rgb), `tones` (all of tones.py), `gradient`, `palette` (16 roles, presets, generate,
  colours_from_roles, per-module `Section` looks), `picker` (parse incl. colour names, HSV, contrast, partner, ideas,
  presets, recent picks in picker.json), `theme` (`Theme` wraps the raw JSON object so unknown keys survive), `store`
  (`ThemeStore` over themes.json: every `_load` migration, pack, apply and previous, folders, group order and folding,
  next / previous, from accent / colours, set_accent, reset, export / import `.lumtheme`, favourites, recent changes,
  theme hotkeys list; saves go through `lum_core::store::Writer`, a background writer, `flush()` at quit), `history`
  (undo / redo `History`, `Recent`, `note_change`).
* Tests: `tests/engine.rs` (every tones, generation, gradient, section fixture) and `tests/store.rs` (all themes
  migration cases, sample_home themes.json round trip byte for byte, `.lumtheme` Rust export equals Python's bytes, Python
  export imports like Python). New fixtures `themes_share_cases.json` and `fixtures/lumtheme/` (made by
  `themes_share_cases()` in tools/make_fixtures.py; the `.venv` builds here: `python3 -m venv .venv && .venv/bin/pip install
  PyQt6 Pillow cryptography numpy segno requests psutil pulsectl`, run with `QT_QPA_PLATFORM=offscreen`).
* App: `App.themes` (ThemeStore), `tokens()`, `restyle()` / `take_restyle()` (Shell::logic installs the new Tokens
  into egui before the pass, so a switch is one frame), tray icon follows the accent, `theme.changed` event, settings
  profile switch with the "Settings profile: X" notice, next / previous theme hotkeys (`ui.theme_cycle_hotkey`,
  `ui.theme_prev_hotkey`) and per-theme hotkeys under owner "themes", Refused notices, `ctl theme next|previous|back|<name>`,
  `status` shows the theme, smoke check "themes switch and restyle".
* `lum-ui`: `theme.rs` now builds from `lum_theme::Palette` (re-exports `Role` and `GradientKind`; Palette gained
  `font_family`, `background_image`, `image_opacity`; `Tokens::of_theme`, `Tokens::for_section(key)`); `picker.rs`
  (the colour dialog with Solid / Gradient, SV square, hue strip, old / new, hex, readability, chips, gradient bar, stop
  rows, ideas, presets, `swatch()`), `widgets/segmented.rs`, `themes/previews.rs` (overlay, panel, toast, alert and
  `paint_luminescent` mock-ups with a `Reg` of where colours are used). These compile and pass clippy but have no
  tests or snapshots yet and have never been drawn.
* `lum-platform::dialogs` (rfd 0.18 with only `xdg-portal`, no wayland / gtk): `open_file` and `save_file` on a thread,
  result by callback.

**Written but NOT compiled** (not declared in `themes/mod.rs`, so the build ignores them): `lum-ui/src/themes/page.rs`
(the Themes page: toolbar, groups with fold / drag to reorder / drop a card on a folder, cards, add tile, menus, dialogs,
file jobs) and `themes/pictures.rs` (background-thread picture textures; needs the `image` dependency already added to
lum-ui). page.rs needs a clean-up pass: remove the stray `unused`, `paint_hint`, empty `Outcome::Cancelled` branch and the
nonsense `Tokens::of(ctx).p.primary.to_hex()` check in `run` (just call `ctx.request_repaint()` after acts), then declare
both modules in `themes/mod.rs` and fix what the compiler says.

**Left for phase 4** (in order):
1. Finish and compile the Themes page; a `ThemesUi` owner in `themes/mod.rs` holding the page, a `FloatingPanel` window
   ("Themes"), the shared `ColourDialog` (files `picker.json`, `colours.json` in the data folder) and later the editor;
   it dispatches picker events to the page or editor by `PickerEvent.target`. The App shows it from `show_modules` and opens
   it with `ctl overlay themes` (the overlay has no page switcher until phase 5); file asks go to
   `lum_platform::dialogs`, answers come back as an `AppMsg` and into `page.file_chosen`; `PageOut.notify` goes to the Notifier.
2. Theme editor (75 % to 100 %): own `FloatingPanel` window (resizable, three columns: parts list, settings, live
   preview), General (name, font, key with clash check, radius, glow, background image + opacity, card icon), Colours
   (role rows with ◐ gradient button, star favourites, Recently changed card, quick tools Main colours / Harmony /
   From a picture / Surprise me / Adjust all / Follow the accent / Add variance / Light ⇄ dark with before / after, as
   `BulkDialog` in ui/colour_tools.py lines 853 to 1230), "When it is applied" (wallpaper, settings profile), PARTS
   (Notifications, Warnings on screen) and MODULES sections (accent, shades, backgrounds, text, own radius, Copy the look
   of…, Match all its colours to it, Reset this part), DESKTOP pages as stubs for phase 14, Undo / redo (Ctrl+Z, Ctrl+Shift+Z,
   Ctrl+Y) with `lum_theme::history::History` (700 ms burst = one step), Save / Cancel with "Unsaved changes" confirm, the
   recent list saved with `store.set_recent_changes` on Save. Port `ThemeDialog` in ui/themes_page.py lines 860 to 1776.
3. Snapshot tests in `crates/lum-ui/tests/snapshots.rs` style for the Themes page and the editor in Dark, Light and a custom
   gradient theme (see how the gallery does `Harness`); a picker test (gradient mode); a "switch restyles in one frame" check
   (install tokens, next headless pass already uses them).
4. Add the page, picker and editor to `--smoke` (headless, embedded viewports like `lum_ui::gallery::build_headless`).
5. Records: D31 (rfd with xdg-portal only, file dialogs on a thread) and D32 (`Theme` keeps the raw JSON object, store saves
   through a background writer, `themes.changed` / `themes.applied` are `ThemeEvent`s the app drains in `poll`) into the
   ARCHITECTURE.md decision log, FEATURE_INVENTORY.md lines for Themes, `ctl theme` in the docs, percentages, final handoff.
   `cargo deny` passed with rfd (only a duplicate thiserror warning).

**Surprising.** Python's `_load` keeps weird shapes alive (a theme without `builtin` becomes `builtin: true` through
`setdefault` from the Dark base; the file is only rewritten on first run, a migration or the pack), so the Rust port copies that.
Python's `clean_gradient` accepts string numbers and drops bad stops. `Ui::id()` is deprecated in egui 0.37 (use
`Id::unique(...)`). rfd's default `wayland` feature needs `wayland-sys` and pkg-config; turn default features off.

### 2026-11-24, Phase 3 built to 100 %, the Hello tray check waits on you (overall 20.75 %)

Everything in the phase is built, green and pushed (fmt, clippy on Linux and the Windows target, all tests,
`--smoke`, the Xvfb instance tests, `cargo deny`, `--idle-check` under Xvfb: 0 % CPU, 0.3 wake-ups/s, nothing
painted). The 100 % checkpoint is ticked only after your Hyprland and Windows 11 results (steps under "Waiting on
the user"); then the phase is at 100 % and overall at 23 %.

**Done since 75 %.** lum-platform: `terminal` (find, the command for nine Linux terminals and Windows Terminal
tabs; every `terminal_cases` command passes), `runner` (every launch rule of `core/runner.py` on both systems,
pkexec / sudo / UAC, working folder, keep open, Stop by process group or taskkill, `command_log.txt`, open a
terminal), `apps` (installed apps from `.desktop` files, real icons: freedesktop icon themes and MIME icons on
Linux, `SHGetFileInfoW` pixels on Windows; reveal in the file manager through `FileManager1` with fallbacks).
lum-ui: `overlay` (the empty backdrop on every screen: scrim at `ui.overlay_dim`, dot grid, top halo; the first
screen takes the keyboard; Esc or a click closes). The app: tray icon and menu from `general.tray_items` (Open,
Command palette as a stub, Keep awake, Reset stuck keys, Open log file, Quit everything), tooltip, tray click
toggles the overlay; the launcher's hotkeys (overlay, palette, keep awake) with the Python labels and the
"hotkey not registered" notification; keep awake; desktop notifications for module messages; `ctl overlay`
opens it; `ctl status` shows `hotkey_latency_p99_ms`; `--spike hotkeys` for the latency and stuck-key checks.

**Left out on purpose (written down so the next phases pick them up).**
* Tray → Restart and Settings, and "Quit all" (`general.force_quit_hotkey`, Ctrl+U by default) need the
  confirmation dialog and the settings page: phase 5 and 7. Quit everything in the tray quits at once for now.
* The launcher's own `.desktop` entry and icons, and autostart sync at start: phase 15 (D30), since the Python app
  writes the same files.
* Clipboard change watching: phase 8 (D30). Terminal colours on the command line: phase 14 (callers pass them).
* The Windows hook re-install watchdog from R4 is not built: the callback only forwards, so Windows has no reason
  to drop it. Add it if the Windows check ever shows hotkeys going dead.

**Surprising.**
* winit's `set_cursor_hittest` sets the Wayland input region without committing the surface, so click-through
  only takes effect with the next painted frame; the dim now repaints once after a change (D26).
* zbus 6 dropped its blocking API; ksni still uses zbus 5. Keep awake and reveal use zbus 5 inside a small tokio
  runtime on their own threads.
* rodio 0.23 renamed its types (`DeviceSinkBuilder::open_default_sink`, `Player`).
* ksni is Unlicense: one `cargo deny` exception, for that crate only. rodio needs `libasound2-dev` (CI installs it).
* A Unix path test ran on Windows CI (lum-hypr's code is built everywhere): it is now `cfg(unix)`.
* Building the Python `.venv` here for `make_fixtures.py` needs `libegl1 libgl1 libxkbcommon0 libfontconfig1
  libdbus-1-3` first; then the generator reproduces every committed fixture exactly.

### 2026-11-24, Phase 3 at 75 % (overall 20.75 %)

**Done.** lum-platform: `tray` (menu model, the sparkle drawn with tiny-skia from lum-ui's path with the 4 %
margin, tooltip cut at 127 characters; ksni on Linux, tray-icon on Windows, D30), `notify` (notify-rust),
`clipboard` (arboard with Wayland data-control: text, pictures, file lists, all three at once), `autostart`
(XDG entry with the Python quoting; the Run key with the old `UsefulScripts` clean-up), `keepawake` (logind
inhibitor over zbus 5; `SetThreadExecutionState` on its own thread), `sounds` (rodio; freedesktop chime and alarm,
Windows system sounds, custom files, stop), `system` (idle time: `GetLastInputInfo`, or the pointer standing still
through Hyprland as in Python; process names from /proc or Toolhelp). `cargo deny`: ksni is Unlicense, allowed
for that crate only. CI installs `libasound2-dev` for rodio.

**Left for 100 %.** Wire the tray, keep awake and notifications into the app (menu from `general.tray_items`,
tooltip, click toggles the overlay), the runner, terminals, desktop entries and icons, the ctl commands, the
Hello tray overlay and the user's check. Clipboard watching is phase 8 (D30).

### 2026-11-24, Phase 3 at 50 % (overall 18.5 %)

**Done.** `lum_platform::hotkeys`: `keys` (normalise, pretty with Linux and Windows names, keysym, to_hyprland,
validate; every case in `hotkey_cases.json` and the `keys.json` tables pass), `vk` (Windows key codes both ways,
punctuation from the keyboard layout on Windows), `HotkeyManager` (register, register_hold, unregister_all,
owner_of, listing, combos, set_suspended, reset_keys, send, release_modifiers, the latency counter, D29),
`hyprland` (binds, bare modifiers on release, four binds per modifier hold key, clash with your own binds,
`always` checks its modifiers, reinstall after a reload, binds lifted while suspended, the trusted 100 ms hold
check) and `windows` + `matcher` (the hook thread, the stuck-key rules, auto-repeat, hold interrupts, the check
only while a key is held, SendInput tagged so the hook skips it). lum-ui's hotkey caps use the real names. The app
owns the manager (dry until the desktop starts) and records hotkey use like the Python app.

**Not yet measured.** Hotkey latency p99 on real desktops (`HotkeyManager::latency_p99_ms`, logged at debug every
50 hotkeys) and the "kill focus mid-hold" check on Windows: both are in the Hello tray steps.

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

* **Phase 9, the screen tools on two monitors with different scales (blocks the 100 % checkpoint):** quit the Python app first (same
  hotkeys). Hyprland: two monitors with different scales (e.g. `hyprctl keyword monitor DP-2,preferred,auto,1.5`), `grim` installed
  as the fallback, `tesseract tesseract-data-eng` for Text Grab. Build and run with a copied data folder:
  `cd luminescent-rs && cargo build --release -p luminescent`, then
  `LUMINESCENT_HOME=/tmp/lumhome ./target/release/luminescent`. On **each** monitor:
  **Colour picker** Ctrl+Alt+C: the frozen screen must cover the monitor exactly and be sharp (not blurred or offset); the loupe's
  pixels must match what is under the mouse at real pixels; arrows move 1 px, Shift+arrows 10; click copies; Esc and right-click
  cancel; move the mouse to the other monitor (no loupe left behind, keys follow). Then Settings → Themes → a colour → the
  eyedropper. Tray → Recent colours → Use as the theme's accent. **Ruler** Ctrl+Alt+R: drag 100 layout px on the scaled monitor
  (it should say 150 px there at 1.5), Shift, Ctrl+release, Enter. **Text grab** Ctrl+Alt+O over some text, with and without
  "Show the text in an editable box". **Doodle**: hold Alt (not Alt+Tab, not Alt+N) and draw on both monitors with every tool on the
  strip, right-drag to rub out, the wheel, Alt+F undo (last stroke wherever it was), Alt+X, Alt+G, Alt+C (paste the picture
  somewhere), Alt+Shift+Q, Esc while holding, the text tool (click, let go of Alt, type, Enter); check that after letting go of Alt
  the window you were in still has the keyboard and the mouse did not jump. **Snapper** with a tiled window: Super+Alt+Left three
  times, Up twice, Down, then palette "Snap window: back into the tiling layout"; on both monitors (the bar must stay clear).
  **Timing:** press the colour picker hotkey ten times (Esc between) and run
  `grep "frozen screens up" /tmp/lumhome/data/launcher.log` (budget 150 ms; paste the lines).
  **Windows 11**, two monitors at 100 % and 150 %: the same, with `$env:LUMINESCENT_HOME="$env:TEMP\lumhome"` and
  `.\target\release\luminescent.exe`, then `Select-String "frozen screens up" "$env:TEMP\lumhome\data\launcher.log"`. On Windows
  also: Alt held to draw must not open the menu bar of the app underneath when you let go; snapping a maximised window; "Snap
  window: put it back where it was"; and whether Alt+C's picture shows the drawing twice (darker highlighter) or once. Say what
  looked or behaved wrong next to the Python version.

* **Phase 8, the Daily-drivable alpha (blocks the 100 % checkpoint and the milestone):** quit the Python app first. Build with
  `cd luminescent-rs && cargo build --release -p luminescent`, copy your real data into a throw-away home
  (`mkdir /tmp/lumhome && cp ~/.local/share/Luminescent/settings.json /tmp/lumhome/ && cp -r ~/.local/share/Luminescent/data /tmp/lumhome/`;
  Windows: `%LOCALAPPDATA%\Luminescent`), run `LUMINESCENT_HOME=/tmp/lumhome ./target/release/luminescent`, and use it for a day.
  Specifically: **Clipboard:** press Ctrl+B in another app, click a text card (it should paste there) and Ctrl+click one (paste, Enter,
  pop-up back in the same folder); Paste here with text, a picture and a copied file; drag a card onto a folder and onto another
  card; right-click a card (Favourite, Rename, Show in the file manager, Delete goes to the Trash) and a folder (Colour…); search;
  rest the mouse on a picture. Open the Clipboard page in the overlay and as a dashboard box. **QR code:** set a hotkey in Settings →
  QR Code, copy a link, press it, scan it with a phone, Copy image, Save as… a PNG and an SVG. **Today, Usage, Calendar:** add them to
  the overlay's navigation bar in Settings → Overlay and look at each, also as dashboard boxes. Say what looks or behaves wrong
  next to the Python version.

* **Phase 8 so far (does not block the next tool):** on Hyprland and Windows 11 with a copied data folder, press the
  Calculator and new timer hotkeys, let a short timer finish (sound, blinking border, the panel must not take the keyboard),
  quit and start again with a timer running, and try timer edit mode's anchor bar. The user's day with the alpha comes at
  the end of the phase.
* **Phase 8, Notes and Reminders (does not block the next tool):** press the Notes hotkey (Alt+N), make a note and a to-do list,
  give a task a due date (right-click), stick a note on the screen and move it, try the edit mode hotkey (Alt+E: dim, anchor bar,
  side buttons), delete a note and press Undo, export one. In the palette type "remind me in 1m to test", wait for the pop-up
  (sound, Snooze, Done), and in Settings → Reminders add a reminder, press "Test this reminder now". Windows: do it again.

* **Phase 7 (blocks the 100 % checkpoint):** quit the Python app first. Build with
  `cd luminescent-rs && cargo build --release -p luminescent`, copy your real `settings.json` and `profiles.json` into a
  throw-away `LUMINESCENT_HOME` (`data/` for profiles) and run `./target/release/luminescent`. Open Settings (Ctrl+Alt+S) and
  check: every section opens and looks like the Python page; search "tray" (matches highlighted, counts in the sidebar,
  Only matching settings); press `/`; pin a section with the star; change a hotkey with Record (the other hotkeys must
  stop firing while it waits) and try one that is already used (warning, and Save refuses); Save & Apply and check the
  change shows at once (tray menu, dashboard snap, a hotkey); make, switch, rename and delete a profile; Export backup,
  then Import it (it should ask, then restart); Show a notification with an unsaved Notification style; make the
  overlay's panel small or put Settings in a dashboard box and check it still works. Say what looks or behaves wrong
  next to the Python version.

* **Phase 6 (blocks the 100 % checkpoint):** quit the Python app first (same hotkeys). Build with
  `cd luminescent-rs && cargo build --release -p luminescent`, copy your real `dashboard.json` into a throw-away
  `LUMINESCENT_HOME` (`data/dashboard.json`) and run `./target/release/luminescent`. Open the overlay and check on the
  Dashboard: your boxes are where they were, with real icons; drag a header (snaps to 8 px) and a right edge; click a header
  to fold; drag a box and a row onto your other monitor and back (the drop should land under the mouse); drop a file from
  the file manager on a box; Ctrl+click two headers and drag them together; drag on empty space to select, Ctrl+A,
  Delete; Tidy up and Ctrl+Z; right-click a row, then Edit… and set a hotkey; **close the overlay and press that hotkey**;
  double-click a program and a group; Lock; type in the search box; make a second dashboard and "This screen shows" on
  the other monitor; tray → Dashboard; open an app (Super+E) while the overlay is up (it should float above it) and close
  the overlay (it should tile again). Say what looked or behaved wrong next to the Python version.

* **Phase 5 (blocks the 100 % checkpoint and the Looks right milestone):** quit the Python app first (same
  hotkeys). Build with `cd luminescent-rs && cargo build --release -p luminescent`, then with a throw-away
  `LUMINESCENT_HOME` run `./target/release/luminescent`. Press Ctrl+Shift+Space ten times (open, close), then run
  `./target/release/luminescent ctl status` and paste `overlay_open_ms` and `hotkey_latency_p99_ms` (the budget is
  50 ms for both together). Then `ctl quit` and `./target/release/luminescent --idle-check 20` (it opens and hides the
  overlay first, then must paint 0 frames). Look at: the dim and dots on every screen, the switcher with its sliding
  marker, Ctrl+1…9, the Themes page in the panel, ✎ edit mode (drag the clock, right-click a chip), typing on Themes
  (opens the palette), Ctrl+Alt+Space (the palette), holding Ctrl+Alt+/ (the cheat sheet, gone when you let go),
  `luminescent ctl notify Hello | a card`, `luminescent ctl alert Hot | a warning`, tray → Quit everything (asks
  first). Say what looked or behaved wrong next to the mock-ups.

* **Phase 4 (optional, does not block phase 5):** with a throw-away `LUMINESCENT_HOME`, run the app and
  `luminescent ctl overlay themes`. Click a theme (the whole app and the tray sparkle should follow at once), drag a card
  onto a folder, right-click a card → Settings…, change a colour (the preview follows), open a colour and switch to
  Gradient, try a quick tool and Ctrl+Z, press Save, then Cancel after a change (it should ask). In General, click
  Record and press a key combination, and press Browse… for a picture. Say what looked or behaved wrong.

* **Phase 3 (the user said "unsure" for both systems on 2026-11-25; still blocks the 100 % checkpoint and the Hello tray milestone):** the Hello tray steps for Hyprland and
  Windows 11, as given in the session's final message (also below in short). Quit the Python app first: both use
  Ctrl+Shift+Space. Build with `cd luminescent-rs && cargo build --release -p luminescent`, run
  `./target/release/luminescent` with a throw-away `LUMINESCENT_HOME`, then check: the sparkle in the tray, its
  tooltip and menu; Ctrl+Shift+Space dims every monitor and Esc (or a click) closes it; a tray click toggles it;
  `luminescent ctl overlay` opens it; Keep awake shows up in `systemd-inhibit --list` (Hyprland) or
  `powercfg /requests` (Windows); after `hyprctl reload` the hotkey still works; then `--spike hotkeys` (latency
  and the F8 stuck-key check, which is the "kill focus mid-hold" test on Windows) and `--spike clickthrough
  --renderer glow` with the mouse moving (D26). Paste the output and say what you saw.

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
