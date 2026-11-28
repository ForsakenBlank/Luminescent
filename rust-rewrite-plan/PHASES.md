# Phases

Every phase of the rewrite, in order. Each one has:

* **Share**: how much of the whole rewrite it is, by size and difficulty (all shares add up to 100 %).
* **Progress when done**: where the overall progress bar sits once the phase is finished.
* **Checkpoints**: progress *inside* the phase (25 / 50 / 75 / 100 %). Overall progress at any moment is
  `progress before this phase + share × checkpoint %`. Example: phase 6 (share 6 %) at its 50 % checkpoint is
  `37 + 6 × 0.5 = 40 %` overall.
* **Model** and **effort**: what to set the session to. "Helpers" are small side jobs a cheaper model can do.
* **Sessions**: a rough count of focused Claude Code sessions.
* **Read first**: the Python files that are the spec for this phase.
* **Done when**: the definition of done. The phase is not finished until every line is true.
* **Prompt**: paste it as the first message of the session. Every prompt is complete on its own.

Quick table:

| # | Phase | Share | Progress when done | Model | Effort |
|---|---|---|---|---|---|
| 0 | Groundwork | 2 % | 2 % | Sonnet 5.5 | medium |
| 1 | Workspace + core infrastructure | 5 % | 7 % | Opus 5.5 | high |
| 2 | UI foundation + widget kit (decision gate) | 7 % | 14 % | Opus 5.5 | xhigh |
| 3 | Platform layer | 9 % | 23 % | Opus 5.5 | xhigh |
| 4 | Themes | 8 % | 31 % | Sonnet 5.5 | high |
| 5 | Overlay shell, palette, cheat sheet, notifications | 6 % | 37 % | Opus 5.5 | high |
| 6 | Dashboard | 6 % | 43 % | Sonnet 5.5 | high |
| 7 | Settings page, profiles, backups | 4 % | 47 % | Sonnet 5.5 | medium |
| 8 | Everyday tools | 10 % | 57 % | Sonnet 5.5 | high |
| 9 | Screen tools | 5 % | 62 % | Opus 5.5 | high |
| 10 | Automation | 9 % | 71 % | Opus 5.5 | xhigh |
| 11 | System modules | 10 % | 81 % | Opus 5.5 then Sonnet 5.5 | high |
| 12 | Audio + media | 5 % | 86 % | Opus 5.5 | high |
| 13 | Privacy, sync, keybinds | 6 % | 92 % | Opus 5.5 | xhigh |
| 14 | Desktop integration | 4 % | 96 % | Sonnet 5.5 | high |
| 15 | Migration, packaging, release | 4 % | 100 % | Opus 5.5 | high |
| 16 | Beyond parity (own track) | own track | own 0 to 100 % | varies | varies |

---

## Phase 0: Groundwork (baseline numbers + golden fixtures)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 2 % | **2 %** | Sonnet 5.5 | medium | 1 to 2 | none |

**Goal.** Before writing any Rust, capture what "correct" and "fast enough" mean. Dump the Python app's specs
(settings schema, automation nodes, tables) as JSON, generate golden input/output fixtures for every piece of
pure logic and every file format, and measure the Python app so the Rust one has numbers to beat.

**Read first.** `core/settings.py`, `core/automation.py` (the `@node` lines and `F` fields), `core/events.py`,
`core/hotkeys.py`, `core/timeparse.py`, `core/convert.py`, `modules/calculator.py`, `modules/reminders.py`
(schedule maths), `core/tones.py`, `core/theme.py`, `core/themes.py`, `modules/vault_store.py`,
`core/atlas_sync.py`, `modules/promptlog.py`, `core/hyprbinds.py`, `core/terminal.py`, `core/backup.py`,
`tests/`, `tools/shots.py`.

**Deliverables.**

* `luminescent-rs/tools/make_fixtures.py`: runs with the Python app's venv, `QT_QPA_PLATFORM=offscreen`,
  `LUMINESCENT_NO_HOOK=1` and a throw-away `LUMINESCENT_HOME`. It fixes the clock and randomness where needed
  and writes `luminescent-rs/fixtures/`:
  * **Specs**: `settings_schema.linux.json` and `settings_schema.windows.json` (reload `core.settings` with
    `IS_WIN` patched, since help texts differ), `settings_defaults.json`, `automation_nodes.json` (every node
    type with ports, fields, defaults, static choices, help, width), `events.json`, `pages.json` (nav pages,
    extra pages, start panels, tray items), `theme_presets.json` (presets, colour roles, built-in theme pack),
    `keys.json` (key aliases, keysyms, modifier masks).
  * **Behaviour cases** (input → expected output): calculator (sums, functions, percentages, bases, pins,
    errors, limits), time parsing, reminder next-fire times for every schedule type, hotkey normalise / pretty /
    to-Hyprland / validate, tones and theme generation, gradient sampling, automation `compare()` and token
    expansion, unit / currency (fixed rates) / date conversions.
  * **Format cases**: a vault made with a known password, decoy and recovery key plus files, with what each file
    decrypts to; Atlas frames sealed with a fixed code and nonce plus record fingerprints; shell history samples
    (fish, bash, zsh, PowerShell) with their parsed rows; Hyprland config samples with parsed binds and their
    descriptions, and edit results; terminal / fish / fastfetch output for sample themes; old settings files with
    their migrated result.
  * `fixtures/sample_home/`: a complete data folder with one realistic example of every data file listed in
    DATA_AND_MIGRATION.md.
* `rust-rewrite-plan/BASELINE.md`: the Python app's numbers (see PERFORMANCE.md for the list) measured headless
  here, plus `luminescent-rs/tools/measure_python.sh` (and a PowerShell twin) for the user to run on their real
  Hyprland machine and on Windows, with a table to paste the results into.
* FEATURE_INVENTORY.md checked against the code, README and CHANGELOG; anything missing added.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 0.5 % | Generator skeleton; all spec dumps written |
| 50 % | 1 % | Behaviour fixtures for all pure logic |
| 75 % | 1.5 % | Format fixtures (vault, Atlas, histories, Hyprland configs, terminal, settings migration) and `sample_home/` |
| 100 % | 2 % | BASELINE.md with headless numbers and the measuring scripts; inventory reviewed |

**Done when.**

* Running the generator twice produces no diff (deterministic).
* Every fixture file has a short README line saying what made it and what it pins down.
* BASELINE.md has the headless numbers and clear steps for the real-machine numbers.
* FEATURE_INVENTORY.md has been checked line by line against the code.

**Watch out for.** Modules that import PyQt6 at the top need the offscreen platform. Anything using `datetime.now()`
or `secrets` needs patching for determinism. Never write into the real data folder.

**Prompt.**

```text
We are rewriting Luminescent (this repo: a Python + PyQt6 tray app) as a native Rust app. The whole plan is in
rust-rewrite-plan/. Read rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 0 section),
rust-rewrite-plan/PROGRESS.md, rust-rewrite-plan/TESTING.md, rust-rewrite-plan/DATA_AND_MIGRATION.md and
rust-rewrite-plan/FEATURE_INVENTORY.md before doing anything. If PROGRESS.md shows Phase 0 already started,
continue from its handoff note instead of starting over.

This session is Phase 0: groundwork. No Rust yet. The goal is to capture what "correct" means as golden fixtures
generated from the Python code, and to measure the Python app.

1. Set up the Python app's environment (python -m venv .venv, pip install -r requirements.txt; skip packages that
   only exist on Windows). Use QT_QPA_PLATFORM=offscreen, LUMINESCENT_NO_HOOK=1 and a throw-away
   LUMINESCENT_HOME for everything. Never touch a real data folder.
2. Write luminescent-rs/tools/make_fixtures.py that writes luminescent-rs/fixtures/ exactly as the Phase 0
   "Deliverables" list says: spec dumps (settings schema for Linux and Windows by reloading core.settings with
   IS_WIN patched, defaults, automation node types, events, pages, theme presets, key tables), behaviour cases
   for all pure logic, format cases (vault, Atlas frames, shell histories, Hyprland configs, terminal output,
   settings migrations) and fixtures/sample_home/ with one realistic example of every data file. Patch the clock
   and randomness so output is deterministic. Import the Python modules directly; do not copy their logic.
3. Measure the Python app headless (smoke test time, time to "ready" from the log, RSS and CPU after 60 s idle)
   and write rust-rewrite-plan/BASELINE.md. Add luminescent-rs/tools/measure_python.sh and measure_python.ps1
   so I can measure on my real Hyprland and Windows machines, with a table in BASELINE.md for my numbers.
4. Check FEATURE_INVENTORY.md line by line against the code, README.md and CHANGELOG.md and add anything missing.

Rules: write code the way a careful human developer would (plain names, comments only where they explain why,
no banner comments, no emoji). Never use em dashes in anything you write. Commit after each checkpoint in the
Phase 0 table with a short plain commit message.

Finish: run the generator twice and confirm there is no diff. Update rust-rewrite-plan/PROGRESS.md: tick the
checkpoints you finished, set the phase and overall percentages, and write a handoff note (what is done, what is
left, anything surprising). If you run low on room, stop at a checkpoint boundary and write the handoff note.
```

---

## Phase 1: Cargo workspace + core infrastructure

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 5 % | **7 %** | Opus 5.5 | high | 2 to 3 | Haiku 5.5 (low): CI YAML and crate README stubs |

**Goal.** The skeleton every later phase hangs off: the workspace, CI on Linux and Windows, and `lum-core` with
paths, storage, logging, settings with profiles, the event bus, the action registry, usage tracking, backups,
fault isolation, single instance, the ctl socket and the command line. At the end the binary starts headless,
loads a real settings file and quits cleanly, and `--smoke` passes in CI on both systems.

**Read first.** `core/__init__.py`, `core/settings.py`, `core/module.py`, `modules/__init__.py`, `core/events.py`,
`core/guard.py`, `core/usage.py`, `core/backup.py`, `Launcher.py` (start-up, logging, single instance, safe mode,
shutdown), `.github/workflows/check.yml`.

**Deliverables.**

* `luminescent-rs/` workspace with the crates from ARCHITECTURE.md (empty ones are fine for now), a pinned stable
  `rust-toolchain.toml`, workspace lints (clippy warnings as errors in CI), `rustfmt.toml`, `deny.toml`.
* `luminescent-rs/CLAUDE.md` copied from `rust-rewrite-plan/CLAUDE-for-rust-project.md` (fill in the commands).
* `.github/workflows/rust.yml`: fmt, clippy, tests and `--smoke` on `ubuntu-22.04` and `windows-latest`,
  only when `luminescent-rs/**` changes. The Python workflows stay as they are.
* `lum-core`:
  * paths: the same rules as `core/__init__.py` (`LUMINESCENT_HOME`, `%LOCALAPPDATA%\Luminescent`,
    `~/.local/share/Luminescent`); a dev build defaults to a separate folder so it can never touch real data.
  * storage: atomic `save_json` (tmp + fsync + rename) and `load_json` (default on missing, `.bad` on unreadable).
  * logging: `tracing` to `data/launcher.log` (rotating 512 KB, 2 backups) and stdout on a terminal.
  * settings: schema loaded from the phase 0 dump (embedded per OS), store that keeps unknown sections and keys,
    `get` / `set` / `section`, change notifications, quiet saves, every `_migrate` rule (tested with the migration
    fixtures), profiles (`profiles.json`, `data/profiles/<id>.json`, shared sections).
  * event bus with the names from `events.json`; action registry (`id`, label, hint, callback).
  * usage (`usage.json`, `activity.jsonl`) and backups (the same file list and zip layout as `core/backup.py`,
    weekly automatic backup keeping 5, import that zips the current files first).
  * `Faults` (6 in a minute switches a part off) and `StartGuard` (`starting.json`, safe mode after two failed
    starts); a panic hook that writes `data/crash.log`.
  * the `Module` trait and registry from ARCHITECTURE.md.
* `luminescent` binary: clap CLI (`--smoke`, `--strip-metadata <paths>` stub, `--safe-mode`, `ctl <command>`),
  single instance (lock file + local socket via `interprocess`), a tokio runtime for services, clean shutdown.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 3.25 % | Workspace, toolchain, lints, CI green with empty crates, CLAUDE.md in place |
| 50 % | 4.5 % | Paths, storage, logging, settings + profiles + migrations, all passing the phase 0 fixtures |
| 75 % | 5.75 % | Event bus, actions, usage, backups, guard, start guard, module trait |
| 100 % | 7 % | Binary with CLI, single instance, ctl socket, `--smoke`; CI green on Linux and Windows |

**Done when.**

* `cargo test --workspace` passes, including every settings and migration fixture.
* Loading `fixtures/sample_home/settings.json` and saving it again changes nothing (byte-for-byte after
  normalising whitespace), unknown keys included.
* A second instance hands its command to the first and exits.
* CI is green on both systems, and the Python CI is untouched.

**Watch out for.** Settings help texts differ per OS. `settings.json` sits in `HOME_DIR`, not `DATA_DIR`. The
Python `_merge` drops unknown keys; the Rust store must keep them (the Python app must still be able to read what
Rust writes, and vice versa).

**Prompt.**

```text
We are rewriting Luminescent (this repo: a Python + PyQt6 tray app) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 1 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/ARCHITECTURE.md, rust-rewrite-plan/DATA_AND_MIGRATION.md, rust-rewrite-plan/TESTING.md and
rust-rewrite-plan/CLAUDE-for-rust-project.md first. If PROGRESS.md shows Phase 1 already started, continue from
its handoff note.

This session is Phase 1: the Cargo workspace and core infrastructure. The Python code is the spec: read
core/__init__.py, core/settings.py, core/module.py, modules/__init__.py, core/events.py, core/guard.py,
core/usage.py, core/backup.py, Launcher.py and .github/workflows/check.yml before writing Rust.

Build everything in the Phase 1 "Deliverables" list:
- luminescent-rs/ as a Cargo workspace with the crates from ARCHITECTURE.md, a pinned stable toolchain,
  workspace lints, rustfmt and cargo-deny config.
- Copy rust-rewrite-plan/CLAUDE-for-rust-project.md to luminescent-rs/CLAUDE.md and fill in the real commands.
- .github/workflows/rust.yml running fmt, clippy (warnings as errors), tests and the --smoke run on ubuntu-22.04
  and windows-latest, only for changes under luminescent-rs/. Leave the Python workflows alone.
- lum-core: paths, atomic JSON storage, tracing logs, the settings store driven by the schema dumped in
  luminescent-rs/fixtures (keep unknown sections and keys, port every _migrate rule, profiles with shared
  sections), event bus, action registry, usage tracking, backups, Faults, StartGuard, panic hook, Module trait.
- The luminescent binary: clap CLI (--smoke, --strip-metadata stub, --safe-mode, ctl), single instance with a
  local socket, a small tokio runtime for services, clean shutdown.

Test against the phase 0 fixtures: settings defaults, migrations, and a load/save round trip of
fixtures/sample_home/settings.json that changes nothing. Develop with LUMINESCENT_HOME pointing at a temp copy,
never a real data folder.

Rules: follow luminescent-rs/CLAUDE.md once it exists. Code should read like a careful human Rust developer wrote
it: plain names, small functions, comments only where they explain why, no banner comments, no emoji. Never use
em dashes in code, comments, docs or commit messages. Commit after each checkpoint.

Finish: cargo fmt, cargo clippy --workspace -- -D warnings, cargo test --workspace, and --smoke must all pass.
Update rust-rewrite-plan/PROGRESS.md (tick checkpoints, set percentages, handoff note) and add any decision you
made to the decision log in ARCHITECTURE.md. Stop at a checkpoint boundary if you run low on room.
```

---

## Phase 2: UI foundation + widget kit (the decision gate)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 7 % | **14 %** | Opus 5.5 | xhigh | 3 to 4 | none |

**Goal.** Prove the UI framework on real desktops, then build the widget kit everything else uses. First a spike
that checks the hard requirements on Hyprland and Windows 11 and records the numbers. If egui passes, build the
kit: fonts, theme tokens, every basic widget in Luminescent's look, floating panels, dialogs, menus, the
animator, gradient and glow painting, and a headless snapshot harness.

**Read first.** `UI_OVERVIEW.md`, `core/widgets.py`, `core/theme.py` (tokens and stylesheet), `core/anim.py`,
`core/ux.py`, `ui/toast.py` (for card styling), `Luminescent UI mockups/` (the target look), `assets/fonts/`.

**Deliverables.**

* **Spike** (`luminescent --spike <name>` or a `spikes/` example), measuring on Hyprland and Windows 11:
  1. a transparent, frameless, always-on-top, dimmed window covering every monitor;
  2. switching a window between click-through and normal;
  3. opening a window without stealing keyboard focus;
  4. dropping files from Dolphin / Explorer onto a window;
  5. idle CPU and RAM with nothing changing, for `glow` and `wgpu`;
  6. a 300-field form scrolling smoothly;
  7. 400 bezier wires plus 100 animated pulses at 60 fps;
  8. editing a 50 000 character note without lag.
  Results go into `rust-rewrite-plan/SPIKE_RESULTS.md`, and the decision (egui or iced, glow or wgpu) into the
  decision log. On Hyprland, windows can be placed with a minimal `hyprctl dispatch` for the spike; phase 3 does
  it properly.
* **lum-ui foundation**: eframe app with a hidden root viewport; Geist and Geist Mono embedded; a `Tokens`
  struct built from a palette (hard-coded Dark and Light until phase 4) and applied to `egui::Style`; text scale.
* **Widget kit** in Luminescent's look: labels with tones (primary, secondary, muted, accent, accent2, danger),
  mono caps labels with letter spacing, buttons (primary, ghost, danger, icon), text input, multi-line input,
  combo, spin box, slider, opacity slider, checkbox and switch, colour swatch button, hotkey field (visual only for
  now), cards, section headers, tooltips, context menus, the 9-way anchor bar, rounded dialogs (confirm, ask text,
  message), virtualised lists, a dim overlay.
* **FloatingPanel**: frameless always-on-top panel with header (accent bar, title, buttons, ✕), drag by header,
  edge resize, an "edit" look, opacity, remembered geometry.
* **Animator** (styles Caelestia / Smooth / Snappy, speeds, per-kind switches, off in performance mode) and
  painters for linear / radial / sweep gradients and the cached glow.
* **Tests**: `egui_kittest` snapshot harness; a widget gallery page in Dark and Light; `--smoke` builds it.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 8.75 % | eframe skeleton, fonts, tokens, multi-monitor transparent overlay demo |
| 50 % | 10.5 % | All eight spike checks measured on both systems; gate decision recorded |
| 75 % | 12.25 % | Widget kit, FloatingPanel, dialogs, menus, animator, gradient and glow painters |
| 100 % | 14 % | Gallery in both themes, snapshot tests, `--smoke` builds every widget |

**Done when.**

* SPIKE_RESULTS.md has real numbers from both systems and the gate decision is in the decision log.
* Every widget the Python app's stylesheet styles has a Rust twin in the gallery, in Dark and Light.
* Snapshot tests run in CI; idle CPU is 0 % with the gallery open and nothing moving.

**Watch out for.** The cloud session has no display: write the spike so the user can run it with one command and
paste the numbers back. Transparent windows on Windows behave differently on `glow` and `wgpu`. Wayland file
drops into winit windows may not work; if not, record it and plan the fallback (an "Add files…" button plus a
native drop handler later) instead of switching frameworks over it alone.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 2 section), rust-rewrite-plan/PROGRESS.md
(including the Phase 1 handoff note), rust-rewrite-plan/ARCHITECTURE.md (especially D1, the UI framework, and
the decision log D7 to D17), rust-rewrite-plan/PERFORMANCE.md, rust-rewrite-plan/RISKS.md,
rust-rewrite-plan/BASELINE.md and luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 2 already started,
continue from its handoff note instead of starting over. If luminescent-rs/Cargo.toml is missing on your branch,
Phase 1 is not merged yet: fetch the branch claude/fervent-mayer-9vmvr9 and work from it.

This session is Phase 2: prove the UI framework, then build the widget kit. The look is specified by
UI_OVERVIEW.md, core/widgets.py, core/theme.py, core/anim.py, core/ux.py, ui/toast.py and the mock-ups in
"Luminescent UI mockups/". Read them first.

What Phase 1 left for you (details in the handoff note):
- The binary's main thread runs a crossbeam select! loop in crates/luminescent/src/launcher.rs over ctl
  requests, service messages and the usage flush timer. eframe must own the main thread, so move that loop into
  the eframe app: drain the same channels in update(), and give the tokio services a way to call
  ctx.request_repaint() (keep the egui::Context somewhere they can reach) so a ctl request, a signal or a timer
  wakes the UI without polling. Ctrl+C, `luminescent ctl quit` and closing must still shut down cleanly (usage
  flushed, modules stopped, start guard cleared, socket removed); crates/luminescent/tests/instance.rs checks it.
- The Module trait in lum-core has no show(); add the drawing side in lum-ui on top of it (D12).
- Only lum-ui and luminescent may depend on egui. Platform code stays in lum-platform behind cfg(target_os).
- New crates must pass `cargo deny check` (chrono is out, use jiff; see D7). Check Windows-only lint errors
  locally with `rustup target add x86_64-pc-windows-gnu` and
  `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`.
- If eframe needs system libraries to build or test on ubuntu-22.04, add them to .github/workflows/rust.yml
  only; leave check.yml alone.

Part A, the spike: build the eight checks from the Phase 2 Deliverables list as `luminescent --spike <name>`
commands. This container has no display, so make each check runnable by me with one command on Hyprland and on
Windows 11, print its numbers, and tell me exactly what to run (with LUMINESCENT_HOME pointing at a temp folder).
Add `--idle-check <secs>` as TESTING.md describes, reusing the method of luminescent-rs/tools/measure_rust.sh, and
compare idle CPU, wake-ups and RSS with the Phase 1 rows in BASELINE.md (the core alone idles at 0 % CPU and
5 MB). Record what I report in rust-rewrite-plan/SPIKE_RESULTS.md and write the decision (egui or iced, glow or
wgpu) into the decision log in ARCHITECTURE.md. If egui fails a hard check that iced would pass, stop and tell me
before switching.

Part B, once the gate passes: build lum-ui's foundation (eframe app with a hidden root viewport, embedded Geist
fonts from assets/fonts, a Tokens struct from a hard-coded Dark and Light palette applied to egui::Style, text
scale from the ui.text_scale setting), the full widget kit, FloatingPanel, dialogs, menus, the Animator (reading
the animations.* settings) and the gradient and glow painters, all in Luminescent's look. Add an egui_kittest
snapshot harness and a widget gallery in Dark and Light, and make --smoke build every widget headless (it must
still pass in CI on both systems, with no display).

Rules: follow luminescent-rs/CLAUDE.md. Code should read like a careful human Rust developer wrote it: comments
only where they explain why, no banner comments, no emoji. Never use em dashes anywhere. No widget may repaint
or tick while nothing changes. Commit after each checkpoint in the Phase 2 table with a short plain commit
message, and push to your session's branch.

Finish: cargo fmt, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace and --smoke must
all pass, and CI must be green on both systems. Update rust-rewrite-plan/PROGRESS.md (tick checkpoints, set the
phase and overall percentages, write a handoff note with anything surprising) and add any decision you made to
the decision log in ARCHITECTURE.md. Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (Phase 3 from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 3: Platform layer (Hyprland, hotkeys, windows, tray, clipboard, launching)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 9 % | **23 %** | Opus 5.5 | xhigh | 4 to 6 | Haiku 5.5 (low): key name tables from `keys.json` |

**Goal.** Everything that talks to the operating system, behind traits, on both platforms. Ends with the
**Hello tray** milestone: the Rust app sits in the tray and `Ctrl+Shift+Space` opens an (empty) dimmed overlay on
every monitor, on Hyprland and on Windows.

**Read first.** `core/hypr.py`, `core/hotkeys.py`, `core/hotkeys_win.py`, `core/clip.py`, `core/runner.py`,
`core/terminal.py` (terminal detection only), `core/desktop.py`, `core/desktop_win.py`, `core/autostart.py`,
`core/autostart_win.py`, `core/keepawake.py`, `core/system.py`, `core/sounds.py`, `Launcher.py` (tray menu and
tooltip).

**Deliverables.**

* **lum-hypr**: socket requests without spawning `hyprctl`; `lua()`, `dispatch()`, JSON queries (clients,
  monitors, workspaces); the event socket on a tokio task; the FIFO in `$XDG_RUNTIME_DIR/luminescent/` for bind
  callbacks; the LUMI Lua bootstrap reused verbatim from `_bootstrap_code`; window rules and the invisible title
  markers (NO_ACTIVATE, NO_FOCUS, NO_PIN, BLUR, GLASS); reinstalling binds and rules after a config reload;
  cursor position, monitors, `send_shortcut`, `key_down`, foreign binds. Tests against a fake socket.
* **WindowPlacer** trait with a Hyprland backend (the `WindowSync` logic: move windows where the app thinks they
  are, centre dialogs, float on top, follow) and a Windows backend (native positions, no-activate,
  click-through, tool windows).
* **Hotkeys**: key name normalise / pretty / validate / to-Hyprland passing the fixtures; a `HotkeyManager` trait
  with `register(owner, combo, callback, label, non_consuming)`, `register_hold`, `unregister_all(owner)`,
  `listing`, conflict checks (including your own Hyprland binds), `set_suspended`, `reset_keys`, `send`,
  `release_modifiers`. Backends: Hyprland binds through LUMI and the FIFO; Windows with one `WH_KEYBOARD_LL` hook
  on its own thread that only forwards events, with the stuck-key sanitising and 100 ms hold watchdog from
  `hotkeys_win.py`.
* **Tray**: `ksni` on Linux, `tray-icon` on Windows; the sparkle drawn in the accent colour (tiny-skia); the menu
  built from Settings → General → Tray menu; the tooltip with running timers (wired later).
* **Notifications** (`notify-rust`), **clipboard** (text, image, file lists; set; watch for changes; paste into
  the app you were using), **autostart**, **keep awake**, **idle time**, a basic **process list**, **sounds**
  (`rodio`: chime, alarm, custom file).
* **Runner**: every launch rule from `core/runner.py` (scripts in a terminal that stays open, `.pyw`, `.desktop`,
  executables with arguments, files and folders with the default app, URLs, shell commands; admin through pkexec /
  sudo / UAC; working folder; running-process tracking and Stop; `data/command_log.txt`), terminal detection and
  the preferred terminal, desktop entries and real program / file icons.
* The ctl socket now accepts `overlay`, `palette`, `action <id>`.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 16.25 % | lum-hypr (socket, events, FIFO, bootstrap, rules) and the Hyprland WindowPlacer, tested against a fake socket |
| 50 % | 18.5 % | Hotkeys on Hyprland and Windows, hold keys, stuck-key handling; hotkey fixtures pass |
| 75 % | 20.75 % | Tray, notifications, clipboard, autostart, keep awake, idle, sounds |
| 100 % | 23 % | Runner, terminals, desktop entries and icons; **Hello tray** verified by the user on both systems |

**Done when.**

* Hotkey latency (bind fired → callback) is under 5 ms p99, logged by a debug counter.
* A missed key release can not leave a hold key stuck (tested by killing focus mid-hold on Windows).
* Hello tray works on Hyprland and Windows 11, confirmed by the user.
* No platform `cfg` leaks outside `lum-platform` and `lum-hypr`.

**Watch out for.** The Windows hook callback must return immediately or Windows silently removes the hook. Hyprland
throws away runtime binds on every config reload. Keep the Lua byte-identical so it stays proven.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 3 section), rust-rewrite-plan/PROGRESS.md (the
newest handoff notes first), rust-rewrite-plan/ARCHITECTURE.md (D3, D11 to D13 and D18 to D26 in the decision
log), rust-rewrite-plan/SPIKE_RESULTS.md, rust-rewrite-plan/PLATFORM_MATRIX.md, rust-rewrite-plan/CRATES.md,
rust-rewrite-plan/RISKS.md (R1, R4, R6, R16, R17, R19, R20) and luminescent-rs/CLAUDE.md first. If PROGRESS.md
shows Phase 3 already started, continue from its handoff note. If luminescent-rs/crates/lum-ui/src/gallery.rs is
missing on your branch, phases 1 and 2 are not merged yet: fetch the branch claude/gallant-hypatia-5r8le3 and work
from it. If PROGRESS.md shows the phase 2 gate (the 50 % checkpoint, D26) not recorded, stop and ask me for the
spike results first.

This session is Phase 3: the platform layer. The spec is the Python code: core/hypr.py, core/hotkeys.py,
core/hotkeys_win.py, core/clip.py, core/runner.py, core/terminal.py (terminal detection), core/desktop.py,
core/desktop_win.py, core/autostart.py, core/autostart_win.py, core/keepawake.py, core/system.py,
core/sounds.py and the tray parts of Launcher.py. Read them closely; the stuck-key and config-reload handling in
them came from real bugs.

What phase 2 left for you:
- Every window of ours is an immediate egui viewport drawn from the main window's pass (D19): App::show_modules
  in crates/luminescent/src/launcher.rs calls UiModule::show for each running module. The main window must stay
  "visible" to winit (D18). Where there are no viewports (tests, --smoke) the same code draws areas instead.
- lum-platform::desktop already has the screens (Hyprland JSON / EnumDisplayMonitors), the title markers
  (NO_ACTIVATE, NO_FOCUS), a small rule set in a Lua table LUMRS (kept apart from the Python app's LUMI so both
  apps can run at once), the main window's rule and describe_windows(). lum-hypr::socket has request, j/ queries
  and eval. Replace them with the full bootstrap and WindowPlacer; keep the Python Lua byte-identical, decide
  (and record) whether the Rust app's table is LUMI or its own name while both apps may run.
- lum-ui's FloatingPanel and DimOverlay already put the markers in their titles and pass a position and size;
  on Hyprland the placer has to move them (a Wayland window can not place itself). The overlay demo for Hello
  tray can use widgets::dim::scrim, paint::dot_grid, paint::top_halo and paint::panel_glow.
- The tray sparkle is the same path as lum_ui::paint::sparkle (theme.sparkle_path); draw it with tiny-skia.
- The ctl commands overlay, palette, settings and action are parsed already (crates/luminescent/src/ctl.rs).
- Check Windows-only lint errors with the x86_64-pc-windows-gnu clippy target, run windowed tests under Xvfb
  (xvfb-run -a, libxkbcommon-x11-0 installed), and run cargo deny for every new crate (ksni, tray-icon,
  notify-rust, rodio, the clipboard crate): add licence exceptions per crate only, never globally.
- If Hyprland shows the dims or panels opening late or stuttering in the spikes, see R20 before building on it.

Build everything in the Phase 3 Deliverables list: lum-hypr (socket, event socket, FIFO, the LUMI Lua bootstrap
copied verbatim, rules and title markers, reinstall after reload), the WindowPlacer trait with Hyprland and
Windows backends, the HotkeyManager trait with Hyprland and Windows (WH_KEYBOARD_LL on its own thread) backends
passing luminescent-rs/fixtures key cases (then replace the stand-in widgets::hotkey::pretty_parts with the real
key names), tray (ksni / tray-icon), notifications, clipboard, autostart, keep awake, idle time, process list,
sounds, the runner with every launch rule, terminal detection, desktop entries and icons, and the ctl socket
commands. Test lum-hypr against a fake Hyprland socket.

End with the Hello tray milestone: tray icon, and Ctrl+Shift+Space opens an empty dimmed overlay on every
monitor that Esc closes. This container has no desktop, so give me exact steps to verify it on Hyprland and
Windows 11 and wait for my results before ticking the last checkpoint.

Rules: follow luminescent-rs/CLAUDE.md. Unsafe Win32 code gets a short comment saying why it is sound. The hook
callback only forwards events. Code should read like a careful human Rust developer wrote it: comments only where
they explain why, no banner comments, no emoji. Never use em dashes anywhere. Nothing may poll or repaint while
nothing changes. Commit after each checkpoint with a short plain message and push.

Finish: fmt, clippy -D warnings (Linux and the Windows target), tests and --smoke pass, CI green on both systems.
Update rust-rewrite-plan/PROGRESS.md (checkpoints, percentages, handoff note) and the decision log. Stop at a
checkpoint boundary if you run low on room, and write the handoff note first.

Last, give me the prompt for the next phase (Phase 4 from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 4: Themes (engine, Themes page, theme editor, colour picker dialog)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 8 % | **31 %** | Sonnet 5.5 | high | 3 to 5 | none |

**Goal.** Themes work exactly like today: every colour role, gradients, folders, import/export, generation from a
colour, quick tools with undo, per-module looks, theme hotkeys, live switching in one frame. The theme editor's
desktop pages (Caelestia look, Glass, borders, terminal) are stubs until phase 14.

**Read first.** `core/theme.py`, `core/themes.py`, `core/tones.py`, `core/colourpick.py`, `ui/themes_page.py`,
`ui/colour_tools.py`, the Themes parts of `README.md`.

**Deliverables.**

* **lum-theme**: `Palette` with all 16 colour roles, radius, font, glow, background image and opacity, gradients
  (clean, sample at t, mid colour); all of `core/tones.py` (lighter / darker steps, harmony, generate a theme from
  an accent, light ⇄ dark, adjust all, follow the accent, add variance, from a picture) passing the fixtures;
  per-module looks (an accent, or a whole palette and radius) for modules, notifications and on-screen warnings.
* **ThemeStore** over `themes.json`: built-in Dark and Light that can not be deleted, the built-in pack, folders
  with colours and collapse state, apply (tokens swap in one frame, the settings profile switch, the wallpaper
  hook for phase 14), import and export `.lumtheme`, "From a colour…", next / previous theme in the folder, back
  to the theme before, a hotkey per theme, `themes.changed` and `themes.applied` events.
* **Undo / redo** for every theme change, and "Recently changed" (old → new, put just that one back).
* **Themes page**: cards with a painted preview or the theme's picture, folder headers, drag themes into folders,
  right-click menus, the active card's accent border and ✓.
* **Colour picker dialog** (used everywhere a colour is picked): saturation / brightness square, hue strip, old
  and new colour, hex box that also takes `rgb()`, `hsl()` and names, eyedropper hook (phase 9 provides the screen
  pick), text readability on the colour, recent picks (`picker.json`) and theme colours, Gradient mode (stops on a
  bar, drag to move, drag off to remove, Linear / Radial / Sweep, angle, presets, Ideas, ◐ to turn any colour into
  a gradient).
* **Theme editor**: three resizable columns (parts of the theme, that part's settings, live preview) for colours,
  fonts, radius, glow, background image, card icon, wallpaper, settings profile, module looks, notification and
  warning looks, "Copy the look of…", "Match all its colours to it"; desktop pages as stubs.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 25 % | lum-theme engine; tones, generation and gradient fixtures pass |
| 50 % | 27 % | ThemeStore, live apply, theme hotkeys, import/export, undo/redo |
| 75 % | 29 % | Themes page and the colour picker dialog with gradient mode |
| 100 % | 31 % | Theme editor (non-desktop pages); snapshots in Dark, Light and a custom gradient theme |

**Done when.**

* Every tones / generation / gradient fixture passes.
* Loading and saving `fixtures/sample_home/data/themes.json` changes nothing.
* Switching themes restyles everything on screen within one frame, measured.
* A `.lumtheme` exported by the Python app imports in Rust and the other way round.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 4 section), rust-rewrite-plan/PROGRESS.md (the
newest handoff notes first, and "Waiting on the user"), rust-rewrite-plan/ARCHITECTURE.md (the theming pipeline
and D18 to D30 in the decision log), rust-rewrite-plan/DATA_AND_MIGRATION.md and luminescent-rs/CLAUDE.md first.
If PROGRESS.md shows Phase 4 already started, continue from its handoff note. If
luminescent-rs/crates/lum-platform/src/hotkeys/ is missing on your branch, phase 3 is not merged yet: fetch the
branch claude/hopeful-pascal-8kj34a and work from it. If PROGRESS.md shows the phase 3 Hello tray check still
waiting, ask me for my Hyprland and Windows 11 results first, record them (tick phase 3's 100 % when they pass,
fix what they show in lum-platform or lum-hypr first if something fails), then carry on with phase 4.

This session is Phase 4: themes. The spec is core/theme.py, core/themes.py, core/tones.py, core/colourpick.py,
ui/themes_page.py, ui/colour_tools.py and the Themes section of README.md. Read them first.

What phase 3 left for you:
- lum_ui::theme already has Palette (Dark and Light), Gradient, Tokens (installed into the egui context with
  Tokens::install, read with Tokens::of) and the tones; phase 4 replaces the fixed Dark start in
  crates/luminescent/src/shell.rs with the ThemeStore.
- The tray icon is lum_platform::tray::sparkle_icons(rgb) in the accent; the app passes a fixed ACCENT
  (crates/luminescent/src/launcher.rs) and keeps the Tray handle. When a theme applies, call
  tray.set_icons(sparkle_icons(new accent)), as Launcher.py does.
- Hotkeys are events, not callbacks (D29): register with HotkeyManager::register(owner, combo, id, label, opts)
  and handle HotkeyEvent::Fired in App::hotkey. The launcher's own binds live in App::register_hotkeys; add
  "next theme" (ui.theme_cycle_hotkey) and "previous theme" (ui.theme_prev_hotkey) there, and the per-theme
  hotkeys under the owner "themes", with the Python labels. A Refused event means the keys are in the user's own
  Hyprland config.
- Desktop notifications go through the app's Notifier (lum_platform::notify); Luminescent's own cards come in
  phase 5.
- lum_hypr::lua::glass_rule and Hypr::glass_rule exist for phase 14; window borders are not ported yet.
- Every viewport goes through lum_ui::window::shown (the placer, D28); new windows of ours must too.
- Check Windows-only lint errors with the x86_64-pc-windows-gnu clippy target, run windowed tests under Xvfb
  (xvfb-run -a; install libxkbcommon-x11-0 xvfb libegl1 libgl1-mesa-dri mesa-vulkan-drivers libasound2-dev
  pkg-config), and run cargo deny for every new crate (licence exceptions per crate only). The Python .venv for
  tools/make_fixtures.py builds here once libegl1 libgl1 libxkbcommon0 libfontconfig1 libdbus-1-3 are installed.

Build everything in the Phase 4 Deliverables list: the lum-theme engine (palette, gradients, all of tones.py,
per-module looks), ThemeStore over themes.json with every feature listed, undo/redo with "Recently changed", the
Themes page, the shared colour picker dialog with gradient mode, and the theme editor with its desktop pages as
stubs for phase 14. Use the widget kit from phase 2 and the Tokens pipeline so a theme switch restyles everything
in one frame, the tray icon included.

Tests: every tones, generation and gradient case in luminescent-rs/fixtures must pass; themes.json from
fixtures/sample_home must round-trip unchanged; .lumtheme files must move both ways between the Python and Rust
apps. Add snapshot tests of the Themes page and the editor in Dark, Light and a custom gradient theme.

Rules: follow luminescent-rs/CLAUDE.md. User-facing text uses the same labels as the Python app, in its plain
British English. Code should read like a careful human Rust developer wrote it: comments only where they explain
why, no banner comments, no emoji. Never use em dashes anywhere. Nothing may poll or repaint while nothing
changes. Commit after each checkpoint with a short plain message and push.

Finish: fmt, clippy -D warnings (Linux and the Windows target), tests and --smoke pass, CI green on both systems.
Update rust-rewrite-plan/PROGRESS.md (checkpoints, percentages, handoff note), the decision log and the matching
lines in FEATURE_INVENTORY.md. Stop at a checkpoint boundary if you run low on room, and write the handoff note
first.

Last, give me the prompt for the next phase (Phase 5 from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 5: Overlay shell, command palette, cheat sheet, notifications

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 6 % | **37 %** | Opus 5.5 | high | 3 to 4 | none |

**Goal.** The hub itself: the full-screen overlay on every monitor with its cached backdrop, page switcher,
editable chrome (logo, clock, status chips), the command palette, the hold-to-show cheat sheet, Luminescent's
notification cards and the big on-screen warning. Ends with the **Looks right** milestone.

**Read first.** `ui/overlay.py`, `ui/chrome.py`, `ui/palette.py`, `ui/cheatsheet.py`, `ui/toast.py`,
`ui/alert.py`, `Launcher.py` (overlay, palette and settings hotkeys, prewarm, repair), the overlay sections of
`README.md` and `UI_OVERVIEW.md`.

**Deliverables.**

* **MainOverlay**: one viewport per monitor; the dim, dot grid, background image and glow painted once into a
  cached texture per monitor; the page switcher hanging from the top centre with the sliding marker and its glow;
  nav order and extra pages from settings (Dashboard and Settings always there); Esc, ✕ and a click on the dimmed
  area hide it; `Ctrl+1…9`; opens on the screen with the mouse (or primary); optionally on the last page; frosted
  background (Hyprland blur marker, acrylic on Windows); pages built the first time they show; prewarm a few
  seconds after start; repair (throw away and rebuild) when a page faults.
* **Chrome layer** (`ui.chrome_items`): logo, sparkle, title, clock, time, date, status chips, running timers,
  edit mode (drag anywhere on any screen, ＋ Add, Freeze, Duplicate, bigger / smaller, move to screen, remove,
  Reset, Done / Esc).
* Typing on the Dashboard or Themes page opens the palette with the typed text.
* **Command palette**: fuzzy search with `nucleo-matcher` over pages, actions, dashboard items, themes, settings
  sections, notes, colours, calculator pins and reminders; groups in mono caps on the left and hints on the right;
  inline sums (Enter copies); "remind me in 20m to…" quick reminders (wired to phase 8); recent entries in
  `palette.json`.
* **Cheat sheet**: hold the hotkey, a full-screen grid of cards per module with their hotkeys.
* **Notifications**: Luminescent cards (position, width, accent style glow / stripe / top / outline / none, how
  long they stay, most on screen, background opacity, text size, icon, hover keeps it up, ✕, right-click copies,
  long text grows the card) or desktop notifications; reminders and dose warnings always also go to the desktop.
* **On-screen warning** ("Warning on screen", until Got it) used by automation and performance alerts.
* Placeholder pages for pages not built yet, so navigation is complete.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 32.5 % | Backdrop, switcher, show / hide on every monitor; open latency measured against the budget |
| 50 % | 34 % | Chrome layer with edit mode |
| 75 % | 35.5 % | Command palette and cheat sheet |
| 100 % | 37 % | Notification cards, on-screen warning, prewarm, repair; snapshots; **Looks right** confirmed |

**Done when.** Hotkey → overlay visible within the budget in PERFORMANCE.md; nothing repaints while the overlay
is hidden; the palette returns results within one frame for 2 000 entries; snapshots match the mock-ups' layout.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 5 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/ARCHITECTURE.md, rust-rewrite-plan/PERFORMANCE.md and luminescent-rs/CLAUDE.md first. If
PROGRESS.md shows Phase 5 already started, continue from its handoff note.

This session is Phase 5: the overlay shell. The spec is ui/overlay.py, ui/chrome.py, ui/palette.py,
ui/cheatsheet.py, ui/toast.py, ui/alert.py, the overlay, palette and prewarm parts of Launcher.py, and the overlay
sections of README.md and UI_OVERVIEW.md. Read them first; the mock-ups in "Luminescent UI mockups/" show the look.

Build everything in the Phase 5 Deliverables list: MainOverlay with one viewport per monitor and a cached
backdrop texture, the page switcher, the editable chrome layer, type-to-search, the command palette with
nucleo-matcher, the cheat sheet, Luminescent notification cards plus desktop notifications, the on-screen
warning, placeholder pages, prewarm and repair. Use the WindowPlacer and HotkeyManager from phase 3.

What phases 3 and 4 left for you:
- Phase 3's Hello tray check is still unconfirmed (the user said "unsure"); leave it on the waiting list and do not ask
  again unless the user brings it up. The empty overlay (lum_ui::overlay) and its dims exist; replace them with the real one.
- The Themes window is a floating panel for now: lum_ui::themes::ThemesUi (App.themes_ui), drawn from App::show_modules
  and opened by `ctl overlay themes`. Make the Themes page a real page of the overlay (ThemesPage::ui needs a PageCx; the
  editor and colour dialog stay floating windows) and add the page switcher entry. lum_ui::themes::MODULES is the list of
  modules with their icons and titles; reuse it for nav items and the palette. Typing on the Themes page opens the palette.
- Notifications: App has an AppNotifier (launcher.rs) that only does desktop notifications; build Luminescent's own cards
  behind the same Notifier trait. Tray → Restart / Settings and "Quit all" (general.force_quit_hotkey) were left out until
  the confirmation dialog and settings page exist.
- Gotchas found in phase 4: `＋` (U+FF0B) and a few other symbols have no glyph in any bundled font and panic egui's layout
  (use ASCII `+`; the list is in the phase 2 note); an egui Area draws nothing on its first pass, so headless smoke checks
  must run two passes before asserting; FloatingPanel is 85 % opaque, set `opacity = 1.0` on anything that sits over other
  panels; egui_kittest's `Harness::run` fails when a page repaints every frame, which is a useful idle test for each new
  page; use `Id::unique`, not `Id::new`; Windows CI stops at the first failing crate, so a Windows-only failure in an early
  crate hides later ones (look at the log, not just the badge).

Measure hotkey-to-visible latency on Hyprland and Windows (I will run the command you give me) and compare it with
PERFORMANCE.md. Nothing may repaint or tick while the overlay is hidden; prove it with the idle check.

Rules: follow luminescent-rs/CLAUDE.md. Same labels and wording as the Python app. Never use em dashes anywhere.
Code should read like a careful human wrote it. Commit after each checkpoint.

Finish: fmt, clippy -D warnings, tests (including snapshots), --smoke. Update rust-rewrite-plan/PROGRESS.md and
FEATURE_INVENTORY.md. Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 6: Dashboard

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 6 % | **43 %** | Sonnet 5.5 | high | 3 to 4 | Opus 5.5 (high) if cross-screen drag and drop gets stuck |

**Goal.** The dashboard works like a desktop, exactly as today: boxes of items on every screen, page boxes,
picture boxes, selection, tidy up with undo, several dashboards, a dashboard per screen, drag and drop of items,
files and boxes, item hotkeys that work everywhere.

**Read first.** `ui/dashboard.py` (all of it), the Dashboard section of `README.md`, `core/runner.py` (already
ported), `core/usage.py`.

**Deliverables.**

* **DashboardStore** over `dashboard.json` (layout 3; move layout 1 and 2 files over once, as the Python code
  does): boards, tabs of kind `items` / `page` / picture, items of kind `item` / `group` / `heading` /
  `separator` and `action` items; the built-in Tools tab; `screens` (a board pinned to a screen).
* **Boxes**: tab boxes (accent bar, mono caps title, zero-padded count, fold chevron, resize from the right edge),
  page boxes (any boxable page, resizable from edges and corner, remembering its state), picture boxes (picture
  or GIF, frame, fill, corners, opacity, title bar).
* **Items**: badge (APP / DIR / URL / CMD / FILE / PY / TOOL), real icon, hotkey hint, running dot and Stop,
  double-click runs, right-click Run / Edit / Rename / Change file / Set hotkey / Move / Delete, Open terminal here
  for folders; ItemDialog (name, path with File… / Folder…, arguments, hotkey recorder, working folder, run as
  admin, ask for arguments, run in a terminal window); groups run their items in order.
* **Canvas**: selection rectangle, Ctrl / Shift + click, drag a selection together, Delete, Ctrl+A, right-click
  menus for the selection and the open space; snap grid (8 px default, Off for pixel moves); Tidy up (columns /
  grid / rows; by position / name / size; only the selection when there is one; glide animation; Undo / Ctrl+Z);
  lock; search box that highlights matches; boards in the toolbar folding into a menu when there are many; new
  board (empty / with Tools / copy); "This screen shows" per screen.
* **Drag and drop**: rows within and between boxes, files from the file manager onto a box, boxes and selections
  onto another screen.
* Item hotkeys registered with owner `dashboard` for every board whether the overlay is open or not; runs logged
  to usage and the `dashboard.item_run` event; every item also listed in the tray menu.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 38.5 % | Store, model, migrations, running items and groups |
| 50 % | 40 % | Boxes drawn, dragged, resized, folded, snapped, on every screen |
| 75 % | 41.5 % | Selection, tidy with undo, boards, per-screen boards, page and picture boxes |
| 100 % | 43 % | All drag and drop, dialogs, hotkeys, lock, search; snapshots |

**Done when.** `dashboard.json` from `sample_home` round-trips unchanged; a dashboard of 30 boxes and 300 items
drags at 60 fps; item hotkeys fire with the overlay closed.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 6 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/DATA_AND_MIGRATION.md and luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 6
already started, continue from its handoff note.

This session is Phase 6: the dashboard. The spec is ui/dashboard.py (read all of it) and the Dashboard section of
README.md. The runner, hotkeys and window placement already exist from phase 3; the overlay from phase 5.

What phase 5 left for you:
- Phase 3's Hello tray check and phase 5's Looks right check may still be open; leave them on the waiting list and do
  not ask again unless the user brings them up.
- The overlay draws pages through the function the app passes to `MainOverlay::show` (`App::show_overlay` in
  launcher.rs): the Dashboard is a full page (`PageSlot` covers the free part of the screen, under the chrome), and
  every other screen gets a slot with `extra_screen: Some(name)` while the Dashboard is shown. Replace the
  placeholder there with the real page and keep its state in the app. Return `PageFrame { busy: true }` while a
  dialog of the page has the keyboard. A page that panics comes back as `Out::repair`: reset its state in
  `App::repair_page`. Clicking empty dashboard space never closes the overlay (already so); typing on it already
  opens the palette.
- Boxes start below the chrome: `ChromeLayer::bottom_on_host` (the logo, clock and chips, with room for two more
  chips). The chrome and the dashboard share `dashboard.snap`; drag snapping must use `round_ties_even` (Python's
  `round()` rounds halves to even).
- Palette: add `item:<id>` rows in `App::palette_entries` (hint `<tab>` or `<board> › <tab>`, plus ` · <hotkey>`,
  kind "item"; skip separators, headings and action items) and run them in `App::run_palette_entry`. Tray: the
  Dashboard submenu (`dashboard.show_in_tray`, "dashboard" in `general.tray_items`) and "Edit the dashboard…".
- "Apps opened while the overlay is up appear on top of it" (`ui.apps_over_overlay`, Python `_foreign_opened`) was
  left for this phase: float foreign windows that open while the overlay is up, unfloat them when it closes.
- Gotchas found in phase 5: an egui menu or pop-up is drawn inside its window, so a small window (a card) can not
  show one; without Hyprland no screens are listed, so use `window::fallback_screen`; `f32::clamp` panics when its
  bounds cross (screen sizes can be tiny in tests), use `.min().max()`; to recolour a galley lay it out with
  `Color32::PLACEHOLDER`; `ui.put` places a widget in an exact rect; kittest finds only widgets with a
  `widget_info` label (use `Role::Tab`, `ListItem`, `Alert` as fits); text typed into a `TextEdit` changes it during
  the pass, so refresh anything derived from it right after the widget, not before.

Build everything in the Phase 6 Deliverables list: DashboardStore over dashboard.json with the layout migrations,
tab / page / picture boxes, items of every kind with the ItemDialog, the canvas with selection, snapping, tidy up
with undo, lock and search, several boards and per-screen boards, drag and drop of rows, files and boxes across
screens, item hotkeys that work with the overlay closed, usage logging and the dashboard.item_run event.

Tests: round-trip fixtures/sample_home/data/dashboard.json unchanged, migrate a layout 1 and a layout 2 file
the way the Python code does, and add snapshot tests. Measure dragging 30 boxes with 300 items.

Rules: follow luminescent-rs/CLAUDE.md. Same labels as the Python app. Never use em dashes anywhere. Code should
read like a careful human wrote it. Commit after each checkpoint.

Finish: fmt, clippy -D warnings, tests, --smoke. Update rust-rewrite-plan/PROGRESS.md and FEATURE_INVENTORY.md.
Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 7: Settings page, profiles, backups

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 4 % | **47 %** | Sonnet 5.5 | medium | 2 to 3 | none |

**Goal.** The Settings page generated from the schema, with search, pinning, profiles, the hotkey recorder, every
custom widget, and Save & Apply that restarts only what changed.

**Read first.** `settings_ui.py`, `core/settings.py`, `core/backup.py`, the Settings section of `README.md`.

**Deliverables.**

* Profile bar (switch, new, rename, delete; a theme can pick the profile it uses).
* Sidebar: search that highlights matches and counts them per section, "Only matching settings", ☆ to pin a
  section to the top, sections built lazily.
* A widget for every field type: hotkey recorder (with a live clash warning), text, folder, file, int, float,
  opacity slider, switch, colour (opens the phase 4 dialog), choice; custom widgets: navigation bar order, Show on
  start, the tray menu editor, Backup (export, import, the weekly backups), Quit everything, "Show a
  notification" test, and slots for module editors (the reminders editor arrives in phase 8, keybind lists in 13).
* A module's page shows buttons for its actions, or **Turn it on** when it is off.
* Bottom bar: Reset page, status line, Undo changes, Save & Apply. Saving restarts only modules whose section
  changed; fields marked `restart` say so.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 44 % | Page layout, sidebar, search with counts, pinning |
| 50 % | 45 % | A widget for every field type, the hotkey recorder with clash warnings |
| 75 % | 46 % | Profiles, Save & Apply, Undo, Reset, module restarts |
| 100 % | 47 % | Custom widgets, backups UI; snapshots of every section in both themes |

**Done when.** Every one of the ~265 fields in the schema dump shows up with the right widget; changing a value
and saving writes the same JSON the Python app would; search finds every label and help text.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 7 section), rust-rewrite-plan/PROGRESS.md and
luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 7 already started, continue from its handoff note.

This session is Phase 7: the Settings page. The spec is settings_ui.py, core/settings.py, core/backup.py and the
Settings section of README.md. The settings store and profiles already exist in lum-core (phase 1).

What phases 3 to 6 left for you:
- Phase 3's Hello tray, phase 5's Looks right and phase 6's Dashboard checks may still be open; leave them on the
  waiting list and do not ask again unless the user brings them up. The work branch holds phases 3 to 6 (main only 1
  and 2), so the user merges before this session starts.
- Pages are drawn by the `sub` closure in `App::show_overlay` (launcher.rs): `match key { "themes" => ..., other =>
  placeholder }`. Add "settings" there. The same closure draws a page that is a box on the dashboard, so the Settings
  page must work in any size from 360 x 220 up and draw into the rect it is given. Return `PageFrame { busy: true }`
  while a dialog or a key recording has the keyboard. A page that panics comes back as `Out::repair`: reset its state
  in `App::repair_page`.
- Settings that other parts already read each frame, so a change shows at once without a restart: `dashboard.snap`,
  `dashboard.tidy_style`, `dashboard.tidy_order`, `dashboard.show_icons` (the page), `dashboard.group_delay_ms`,
  `dashboard.use_terminal`, `dashboard.keep_open`, `dashboard.terminal`, `dashboard.python_path` (when an item runs).
  After Save the app has to refresh what is made from settings: `App::settings_changed()`, the tray
  (`refresh_tray`, for `dashboard.show_in_tray` and `general.tray_items`) and the overlay's `Config`.
- The hotkey recorder: `lum_ui::widgets::HotkeyField` draws it, `lum_ui::themes::editor::combo_of(modifiers, key)` turns
  a key press into a stored combo, `lum_platform::hotkeys::keys::{normalize, validate, pretty}` check it, the app's
  `HotkeyManager::owner_of(combo)` says what else uses it, and `Out.suspend_keys` stops hotkeys firing while one is
  recorded. The dashboard's item dialog does it in `dashboard/dialogs.rs`; move the shared part into the kit.
- More from Notes and Reminders: `tools/notes` is the shape for a tool with a data file, a panel, many small panels, a page and
  a box (`view.rs` draws into any rect, `mod.rs` owns the data); `tools/reminders` is the shape for a module that wakes at a
  moment (`request_repaint_after` until the next due time) and shows pop-up windows (`popup.rs`, drawn like the toasts). A
  module that wants Settings changed sends a `Request` (`ReminderEdit`) and the app applies it; one that needs another module
  asks through a `UiModule` method with a default (`todo_reminder`, `preview_reminder`, `chips` for the status strip). New
  widgets: `widgets::drag_rows` (drag to reorder a list, one dimension), `widgets::when` (date and time picker),
  `AskDialog::ask_choice` / `ask_datetime`; `FloatingPanel::set_back`, `header_height`, `set_accent`; `tools::edit::keep_on_screen`;
  `tools::notes::markdown` draws Markdown. A custom Settings editor lives in `settings/<name>.rs` (see `reminders.rs`).
  Data: `lum_core::notes` and `lum_calc::reminder` are the models for a file kept over raw JSON with typed fields and
  "write the key if it has a value or the file already had it".
- Glyphs: `egui_kittest` panics where the app would draw a box. Fonts lack `＋` (U+FF0B), `⧉`, `⌗` and `⏲`; use `+` or a drawn
  `IconShape`. Probe a new character with `ctx.set_missing_glyph_policy(MissingGlyphPolicy::Panic)` after two passes.
- Gotchas: an egui menu or pop-up is drawn inside its window; `f32::clamp` panics when its bounds cross, use
  `.min().max()`; `ui.put` places a widget in an exact rect; text typed into a `TextEdit` changes it during the pass;
  `⤢ ＋ ⌕ ⌗ ⏯ ⏲ ⏻ ␟ ⓘ ⧉` have no glyph in any bundled font and panic in layout (use `↗` and `+`); `egui::WidgetType` is
  `egui::Role`; give custom-painted widgets `response.widget_info(...)` so kittest can find them (menu entries and
  dashboard rows have it now). In tests: a drag or a recording asks for a frame every frame, so use `step()` there, not
  `run()`; a double click needs `ctx.options_mut(|o| o.input_options.max_double_click_delay = 5.0)`; call `focus()` and
  `run()` before `type_text`.
- The settings schema dump has a Dashboard & Scripts section (use_terminal, terminal, keep_open, python_path,
  show_in_tray, show_icons, group_delay_ms, tidy_style, tidy_order, snap): all of it must show up with its help text.

Build everything in the Phase 7 Deliverables list: the schema-driven page with the profile bar, sidebar search
with match counts, "Only matching settings", pinned sections, lazy sections, a widget for every field type
including the hotkey recorder with live clash warnings, the custom widgets, module action buttons or "Turn it
on", and the bottom bar with Save & Apply that restarts only the modules whose section changed.

Tests: a test that every field in luminescent-rs/fixtures/settings_schema.*.json gets a widget, a test that
saving after a change writes what the Python app would, and snapshot tests of every section in Dark and Light.

Rules: follow luminescent-rs/CLAUDE.md. Labels and help texts come from the schema dump, unchanged. Never use em
dashes anywhere. Code should read like a careful human wrote it. Commit after each checkpoint.

Finish: fmt, clippy -D warnings, tests, --smoke. Update rust-rewrite-plan/PROGRESS.md and FEATURE_INVENTORY.md.
Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 8: Everyday tools (Notes, Timers, Calculator, Reminders, Clipboard, QR, Today, Usage, Calendar)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 10 % | **57 %** | Sonnet 5.5 | high | 5 to 7 (one tool or two per session) | Haiku 5.5 (low): unit and function tables for the calculator from the fixtures |

**Goal.** The tools people use every day, each as a floating panel, an overlay page where the Python app has one,
and a dashboard page box. Ends with the **Daily-drivable alpha** milestone.

**Read first.** `modules/notes.py`, `modules/timer.py`, `core/timeparse.py`, `modules/calculator.py`,
`core/convert.py`, `modules/reminders.py`, `modules/snippets.py`, `modules/qr.py`, `ui/today.py`, `ui/stats.py`,
`ui/calendar_page.py`, the tool sections of `README.md`.

**Deliverables.**

* **lum-calc**: a hand-written parser and evaluator (no `eval`, limits so `9**9**9` can not hang), `×`, `÷`, `^`,
  `%` rules (`54 + 10%` = 59.4), `1,000`, `mod`, functions and constants, degrees / radians, `ans` and sums that
  start with an operator, named pins as variables, bases (`0x1f`, `0b101`, `&`, `|`, `xor`, `<<`, `>>`, DEC / HEX /
  BIN / OCT display), unit conversions, currency (rates fetched once a day from the same API, `currency.json`),
  date maths; time parsing (`5m`, `1h30`, `25m focus`, `at 17:30`). All calculator, conversion and time fixtures
  pass.
* **Calculator**: panel and page with history grouped by day, pins in coloured folders, keypad toggle, live
  preview, ↑ / ↓ history, Ctrl+Enter calculates, copies and closes, ⧉ copy; `calculator.json`.
* **Timers**: as many panels as you like, monotonic timing, restored still running after a restart
  (`timers.json`), countdown ring or bar, stopwatch laps (lap hotkey laps the last started), DONE flash and sound,
  ⏲ to type a new time, running timers in the tray tooltip and the status chip, `timer.finished` event, time
  logged to usage.
* **Notes**: notes and to-do lists, Markdown with the 👁 preview, colours and tags (`#work` in search), drag to
  reorder notes and items, due dates that create reminders, Export .md / .txt, sticky notes (several at once),
  pinned first, Undo for 12 s after deleting, edit mode with anchor bar and dim; `notes.json`; Notes page and box.
* **Reminders**: every schedule type (once, daily, weekly, monthly with last-day rule, every N days, interval with
  optional hours, at startup), the reminder pop-up (Snooze 5 min to 1 h, Done, ✕, the line that runs out, rising
  into place), sounds, desktop notification, Switch theme action, quick reminders from the palette, the Settings
  editor for reminders; definitions in `settings.json`, state in `reminders_state.json`; `reminder.fired` event.
  Next-fire fixtures pass.
* **Clipboard (Snippets)**: the page and the Ctrl+B pop-up grid, thumbnails on a worker thread (images, video
  frames, link previews), breadcrumb, Backspace up, search across folders, ＋ Save clipboard here (text, image,
  files), drag to reorder and into folders, folder colours, favourites row, hover preview, click pastes into the
  app you were using, Ctrl+click also presses Enter and reopens, placeholders (`{date}`, `{time}`, `{datetime}`,
  `{weekday}`, `{clipboard}`) when switched on; `.luminescent.json` in the snippets folder.
* **QR code**: the clipboard as a QR code, editable text, copy as picture, save PNG or SVG.
* **Today, Usage, Calendar** pages: today's log with ‹ › for earlier days, most used items / hotkeys / tools with
  uses per day, Atlas events and tasks next to reminders (read from stored data now, live once phase 13 lands).

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 49.5 % | lum-calc with all fixtures, Calculator panel and page, Timers with restore (done) |
| 50 % | 52 % | Notes complete, with sticky notes, undo delete and export (done; Reminders is done too, so the phase stands at 62 % and 53.2 %) |
| 75 % | 54.5 % | Reminders complete; Clipboard page and Ctrl+B pop-up (done) |
| 100 % | 57 % | QR, Today, Usage, Calendar; page boxes for all (all built, the phase stands at 90 % and 56 %); **Daily-drivable alpha** confirmed |

**Done when.** Every calculator, time and reminder fixture passes; each tool's data file from `sample_home`
round-trips unchanged; the user has used the alpha for a day with a copied data folder.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 8 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/DATA_AND_MIGRATION.md and luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 8
already started, continue from its handoff note and take the next tool in the checkpoint order.

This session is Phase 8: the everyday tools. Calculator, timers, Notes and Reminders are built. Work through the rest in the
checkpoint order: Clipboard (Snippets) first, then QR, then Today, Usage and Calendar. For each tool, read its Python
file first (modules/calculator.py with core/convert.py and core/timeparse.py, modules/timer.py, modules/notes.py,
modules/reminders.py, modules/snippets.py, modules/qr.py, ui/today.py, ui/stats.py, ui/calendar_page.py) and its
section of README.md, then build everything the Phase 8 Deliverables list says for it: the logic in its crate
with tests, the floating panel, the overlay page where there is one, and the dashboard page box.

Before anything else check the branch: `git log --oneline -3` must show the Phase 8 commits ("Plan: phase 8 at 62 %",
"Reminders: ...", "Notes: ..."). If PROGRESS.md says less than Phase 8 at 62 %, the branch is main without the later work:
it is on origin/ccr-a4cc2d9e-xg3o3z (or whichever branch the handoff note names), so fast-forward onto it
(`git merge --ff-only origin/<branch>`) and say so.

What phases 3 to 7 left for you:
- Phase 3's Hello tray, phase 5's Looks right, phase 6's Dashboard and phase 7's Settings checks may still be open;
  leave them on the waiting list and do not ask again unless the user brings them up.
- A module is a `UiModule` in `modules()` (launcher.rs, in the order of modules/__init__.py); its actions are
  registered by the host, so the palette, tray, dashboard Tools tab and the Settings page's buttons ("Turn it on" while
  off) pick them up with no extra wiring. Its Settings section is already drawn from the schema; a changed section
  restarts only that module (`apply_changed`, `Module::apply_settings` is a stop and start unless you override it).
- Pages are drawn by the `sub` closure in `App::show_overlay`: a module that returns its page key from
  `UiModule::page_key` and draws in `UiModule::page(ui, rect, cx)` is found there by key, into `ui.max_rect()`, in any
  size from 360 x 220 up (the same closure draws a dashboard page box). Return `PageFrame { busy: true }` while a dialog
  or key recording has the keyboard. A page that panics comes back as `Out::repair`: reset its state in
  `UiModule::reset_page`. File dialogs go out through the page's `Out` and come back as an `AppMsg` (see
  `AppMsg::SettingsFile`); slow work runs on a thread or the tokio runtime and reports back through the module's `Inbox`.
- A custom Settings field gets its editor in `settings/custom.rs` (`custom_control`) and `model::has_editor`; the reminders
  editor (`settings/reminders.rs`) is the example. Settings a module reads each frame show at once; after Save the app calls
  `settings_saved`.
- The hotkey recorder is in the kit: `HotkeyField`, `recorded_key`, `combo_of`; `Out.suspend_keys` pauses the hotkeys.

What phase 8 so far (calculator, timers, Notes, Reminders) left for you, read `lum_ui::module` and `lum_ui::tools` first:
- A tool lives in `crates/lum-ui/src/tools/<tool>/` and the logic that does not draw lives in `lum-calc` or `lum-core`
  with its own tests. Copy the shape of `tools/calculator` (module, `view.rs`) and `tools/timer`.
- Actions, hotkeys, palette rows and worker threads reach a module through its `Inbox<Cmd>` (`inbox.action(id, label,
  || Cmd::X)` makes an action); the module takes its commands at the top of `show` / `page`. What a module wants from the
  app (a card, an event, the overlay closed, a usage or activity line, the tray's tooltip redone) goes back as a
  `Request` through `Services::send`; the app answers them in `launcher/tools.rs`. Hotkeys: return them from
  `UiModule::binds` (the setting inside the section, an id, a label) and handle `bind_pressed`; the app registers them
  after start, after Save and after "Turn it on". Palette rows: `palette_entries` (always) and `palette_live(text)`;
  `run_palette` runs a row whose id starts with the module's prefix (add the prefix to `run_palette_entry`, as `calc:`).
  The tray menu takes `tray_actions`, the tray tooltip and the overlay's chips take `status_lines`.
- Panels: `FloatingPanel::show_with(ctx, header, body)`; `flash(n)` blinks its border. Dialogs from a panel go through
  `tools::dialog_window`. Edit mode with the nine anchors: `tools::edit::anchor_window` and `snap_all` (Notes adds the
  dim). Copying: `tools::clip::copy` (never on the UI thread). Icons the fonts lack are drawn: `IconButton::shape(
  IconShape::Copy | Keypad | Clock | Eye)`, so `⧉ ⌗ ⏲` are no longer a problem there.
- Data files are kept over raw JSON (`Map<String, Value>` wrapped in typed getters) so unknown fields and key order
  survive; save through `lum_core::store::Writer` with `pyjson::to_string(.., Style::indented(n))`. The sample file must
  save back byte for byte (see `the_sample_calculator_file_is_saved_back_unchanged`).
- Gotchas: an egui menu or pop-up is drawn inside its window; `f32::clamp` panics when its bounds cross, use
  `.min().max()`; `ui.put` places a widget in an exact rect; `egui::WidgetType` is `egui::Role`; give custom-painted
  widgets `response.widget_info(...)` so kittest can find them; `Ui::id` is deprecated, pass an `Id` salt down;
  `Id::new` is deprecated, use `Id::unique`; jiff 1.0 names it `ZonedDateTime`; a single-line `TextEdit` loses focus on
  Enter, so test `response.lost_focus()` with the key, or `consume_key` before drawing it. In tests: a drag or a
  recording asks for a frame every frame, so use `step()`, not `run()`; call `focus()` and `run()` before `type_text`;
  look widgets up by role and label (`get_by_role_and_label`), names repeat; a widget below the fold can not be clicked,
  so make the test window tall; `UPDATE_SNAPSHOTS=1` leaves a picture alone while it is within the tolerance, so delete
  the PNG first when a layout moved a little. This container needs `libasound2-dev pkg-config libxkbcommon-x11-0
  mesa-vulkan-drivers xvfb` installed (and `gcc-mingw-w64-x86-64` for the Windows clippy run because of `ring`, and
  `cargo install cargo-deny --locked` for the licence check).

Tests: all calculator, conversion, time and reminder fixtures in luminescent-rs/fixtures must pass, and every
tool's data file from fixtures/sample_home must round-trip unchanged. Snapshot each panel in Dark and Light.

Rules: follow luminescent-rs/CLAUDE.md. Same labels, hotkeys and wording as the Python app. The calculator is a
hand-written parser, never an eval. Never use em dashes anywhere. Code should read like a careful human wrote it.
Commit after each tool.

Finish each session: fmt, clippy -D warnings (and the Windows target), tests, --smoke, cargo deny check. Update
rust-rewrite-plan/PROGRESS.md (checkpoints, percentages, handoff note naming the next tool) and FEATURE_INVENTORY.md.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 9: Screen tools (Doodle, Colour picker, Text grab, Ruler, Snapper)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 5 % | **62 %** | Opus 5.5 | high | 3 to 4 | none |

**Goal.** Everything that freezes or draws over the screen, on every monitor, with correct scaling.

**Read first.** `modules/doodle.py`, `modules/colorpicker.py`, `modules/textgrab.py`, `modules/ruler.py`,
`modules/snapper.py`, the matching settings sections, `core/hypr.py` (float / move / resize).

**Deliverables.**

* **Screen capture backend**: every monitor grabbed once at its real scale (`xcap`; on Hyprland use wlr-screencopy
  if `xcap` is slow; Windows through `xcap`), shown as a frozen full-screen viewport per monitor.
* **Colour picker**: loupe of 11 × 11 pixels at 13×, ringed centre cell, swatch, HEX and RGB; click copies in the
  chosen format; arrows nudge 1 px; Esc or right-click cancels; last 10 colours (`colours.json`) in the tray and
  palette, which can become the theme accent or a new theme; also the eyedropper for the phase 4 colour dialog.
* **Text grab**: frozen dimmed screens, drag a box, OCR on a worker (Windows OCR through the `windows` crate;
  Tesseract on Linux as today), optional edit box (Ctrl+Enter copies).
* **Ruler**: crosshairs, drag to measure W × H and the distance in real pixels, Shift keeps it level, click copies
  W × H, Ctrl+click the distance, arrows 1 px (Shift 10 px).
* **Doodle**: transparent canvas per monitor, click-through when not drawing; hold-Alt via `register_hold` with
  the solo delay and cancel-on-other-hotkey rules; tools (pen, highlighter, line, rectangle, circle, arrow, text,
  fill toggle), Shift for straight lines, wheel for brush size with the "N px" label, a circle cursor of the real
  size, one undo history across screens, clear, blackboard, copy screen + drawing, panic and Esc, passive opacity,
  every doodle setting.
* **Snapper**: Hyprland (float, then move and size to half, third, two thirds or a corner inside the reserved
  area with a gap; pressing again cycles; back into the tiling layout) and Windows (work area, restore maximised
  first, Windows 11 invisible borders allowed for; back to where it was); every spot in the palette.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 58.25 % | Capture backends and the frozen multi-monitor picker base |
| 50 % | 59.5 % | Colour picker, ruler, text grab |
| 75 % | 60.75 % | Doodle complete |
| 100 % | 62 % | Snapper on both systems; palette entries; tests |

**Done when.** Each tool works on a two-monitor setup with different scales (user-verified); doodle drawing stays
at 60 fps with 500 strokes; picker opens within 150 ms of the hotkey.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 9 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/PLATFORM_MATRIX.md, rust-rewrite-plan/PERFORMANCE.md and luminescent-rs/CLAUDE.md first. If
PROGRESS.md shows Phase 9 already started, continue from its handoff note and take the next checkpoint in order.

Before anything else check the branch: `git log --oneline -3` must show the end of Phase 8 ("Today, Usage and
Calendar: ...", "QR code: ...", "Clipboard: ..."). If PROGRESS.md says less than "Phase 8 at 90 %", the branch is main
without the later work: it is on origin/claude/pensive-galileo-6ms969 (or whichever branch the handoff note names), so
fast-forward onto it (`git merge --ff-only origin/<branch>`) and say so.

This session is Phase 9: the screen tools. The spec is modules/doodle.py (946 lines), modules/colorpicker.py,
modules/textgrab.py, modules/ruler.py, modules/snapper.py, their settings sections in core/settings.py, and the window
moving parts of core/hypr.py. Read each Python file first, with its section of README.md; the doodle's hold-key rules
came from real bugs. Work in the checkpoint order (capture backends and the frozen multi-monitor picker base, then the
colour picker, ruler and text grab, then the doodle, then the snapper) and build everything the Phase 9 Deliverables list
says: the screen capture backend with one frozen viewport per monitor at its real scale, the colour picker (also the
eyedropper for the theme colour dialog), text grab with Windows OCR and Tesseract on Linux, the ruler, the doodle with
every tool and setting, and the snapper on Hyprland and Windows. The logic that does not draw (colour formats and
nudging, measuring, the snap geometry, the doodle's stroke model and undo history) goes in a crate with its own tests;
capture, window moving and the hold key go in `lum-platform` / `lum-hypr` behind `cfg(target_os)`.

What phases 3 to 8 left for you:
- Phase 3's Hello tray, phase 5's Looks right, phase 6's Dashboard, phase 7's Settings and phase 8's Daily-drivable
  alpha checks may still be open; leave them on the waiting list and do not ask again unless the user brings them up.
- A module is a `UiModule` in `modules()` (launcher.rs, in the order of modules/__init__.py: doodle sits after notes,
  colorpicker, textgrab, snapper and ruler after calculator, in that file's order); its actions are registered by the
  host, so the palette, tray, dashboard Tools tab and the Settings page's buttons ("Turn it on" while off) pick them up
  with no extra wiring. Its Settings section is already drawn from the schema; a changed section restarts only that
  module (`Module::apply_settings` is a stop and start unless you override it).
- Copy the shape of `crates/lum-ui/src/tools/`: `calculator`, `timer`, `notes` (many small panels, edit mode with the
  dim and anchor bar), `reminders` (pop-up windows), `snippets` (a pop-up over a dim, worker threads, drag and drop) and
  `qr` (one panel). Commands from actions, hotkeys and worker threads reach a module through its `Inbox<Cmd>`
  (`inbox.action(id, label, || Cmd::X)`); what it wants from the app goes back as a `Request` through `Services::send`
  (a card, an event, the overlay closed, a usage or activity line, `SendKeys`, `SetSetting`, `ApplyTheme`), answered in
  `launcher/tools.rs`. Hotkeys come from `UiModule::binds` + `bind_pressed`; `HotkeyManager::register_hold(owner, key,
  interruptible)` and `watch_release` already exist for the doodle's hold-Alt. Palette rows: `palette_entries`,
  `palette_live` and `run_palette`; the tray takes `tray_actions`.
- Screens: `lum_ui::window::screens()` gives `Screen { name, rect, work }` in desktop pixels and has NO scale yet. Add it
  (Hyprland's monitors report one; Windows has per-monitor DPI) before capturing at real pixels. `DimOverlay` is one
  see-through window per screen (click-through or not); `window::placer(ctx)` / `shown` / `crate::window::fallback_screen`
  place windows; cover titles (`desktop::cover_title`) make Hyprland open a window full size on its own screen, and
  `marker::NO_ACTIVATE` keeps it from taking the keyboard.
- The colour dialog already has the eyedropper hook: `ColourDialog::screen_picker`, `take_screen_request` and
  `picked_from_screen`, and `colours.json` (the last 10 picks) is the file it reads next to `picker.json`.
  `lum_platform::clipboard` has `set_text`, `set_picture` and `set_files`; copying never happens on the UI thread
  (`tools::clip::copy`). `lum_platform::dialogs` has `open_file`, `save_file` and `pick_folder`; `lum_platform::frame`
  runs `ffmpeg` for a video frame; `tools::draw` has painter helpers (one line cut to a width, mono caps, a card).
- Data files are kept over raw JSON so unknown fields and key order survive; save through `lum_core::store::Writer`
  with `pyjson::to_string(.., Style::indented(n))`. Every file from fixtures/sample_home must save back byte for byte
  (see `the_sample_meta_file_is_saved_back_unchanged` for the shape of the test).
- Gotchas: an egui menu or pop-up is drawn inside its window; `f32::clamp` panics when its bounds cross, use
  `.min().max()`; `ui.put` places a widget in an exact rect; `egui::WidgetType` is `egui::Role`; give custom-painted
  widgets AND painted text rows `response.widget_info(...)` so kittest can find them; `Ui::id` and `Id::new` are
  deprecated, use `Id::unique((salt, ...))` and put the rect's position in it when two boxes can show the same page; jiff
  1.0 names are `ZonedDateTime`, `to_zoned_datetime`, `DateTime::since((Unit::Second, other))`; fonts lack `＋` (U+FF0B),
  `⧉`, `⌗` and `⏲`, so use `+` or `IconButton::shape(IconShape::...)` and probe a new character with
  `ctx.set_missing_glyph_policy(MissingGlyphPolicy::Panic)` after two passes. In tests: a drag or a recording asks for a
  frame every frame, so use `step()`, not `run()`; call `focus()` and `run()` before `type_text`; look widgets up by role
  and label; a widget below the fold can not be clicked, so make the test window tall; `DimOverlay` paints over the
  panel it dims in the harness, so set the dim to 0 for a picture; `UPDATE_SNAPSHOTS=1` leaves a picture alone while it is
  within the tolerance, so delete the PNG first when a layout moved a little. This container needs
  `apt-get update` and then `apt-get install -y libasound2-dev pkg-config libxkbcommon-x11-0 mesa-vulkan-drivers xvfb`
  (and `gcc-mingw-w64-x86-64` for the Windows clippy run because of `ring`, `rustup target add
  x86_64-pc-windows-gnu`, and `cargo install cargo-deny --locked` for the licence check).

This container has no display. Test capture against a fake and the geometry with unit tests, snapshot every panel in Dark
and Light, and give me exact commands to try each tool on two monitors with different scales on Hyprland and on Windows
11; wait for my results before ticking the last checkpoint. Check the budgets: doodle drawing at 60 fps with 500 strokes
(write the test), the picker open within 150 ms of the hotkey (give me the command to measure it), nothing polling while
a tool is closed (`--idle-check`).

Rules: follow luminescent-rs/CLAUDE.md. Same labels, hotkeys and behaviour as the Python app. Never use em dashes
anywhere. Code should read like a careful human wrote it. Commit after each checkpoint.

Finish each session: fmt, clippy -D warnings (and the Windows target), tests, --smoke, cargo deny check. Update
rust-rewrite-plan/PROGRESS.md (checkpoints, percentages, handoff note naming the next checkpoint) and
FEATURE_INVENTORY.md. Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 10: Automation engine + node editor

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 9 % | **71 %** | Opus 5.5 | xhigh | 4 to 6 | Sonnet 5.5 (high) for the simple value and action nodes once the engine is done |

**Goal.** The Automation page: the engine (all 65 node types, pulses, value wires, variables, rate limiting)
running in the background, and the node editor with animated pulses.

**Read first.** `core/automation.py` (all of it), `ui/automation.py` (all of it), `core/events.py`, the
Automation section of `README.md`, `luminescent-rs/fixtures/automation_nodes.json`.

**Deliverables.**

* **lum-automation (no UI)**: the `automation.json` model (version 1, workspaces with on / off, view, nodes,
  wires, variables); a `NodeRuntime` trait; pulses delivered on the next loop pass so a node wired into itself
  can not recurse; value pulls through ◆ pins; token expansion (`{input}`, `{var:name}`, `{date}`, `{time}`,
  `{now}`, `{clipboard}`, `{newline}`, other braces left alone); `MAX_TEXT`; the `automation.max_rate` safety stop
  that switches a workspace off; pause everything (setting, tray, hotkey); node status lines and progress; saved
  node state; runs on the services runtime and sends `Command`s for anything that needs the UI.
* **All 65 node types** from `automation_nodes.json`, grouped as the Python file groups them:
  * triggers (13): every N seconds, time of day, date, hotkey, button, Luminescent event, program starts / stops,
    file or folder changes, clipboard changes, idle / back, CPU / GPU / temperature / memory / battery thresholds,
    window focused, internet on / off;
  * actions (30): run command (background or terminal, one command per line, each after the last), open, close a
    program, Luminescent tool, open / close Luminescent, run a dashboard item, switch dashboard, switch theme,
    start a timer, workspace on / off, notification, big message, ask me, play a sound, say it out loud, type
    text, press keys, copy to clipboard, web request, download, copy / move / delete, write to a file, warning on
    screen, set wallpaper, volume, lock / sleep / shut down, keep awake, open a note, show; plus set variable;
  * logic (10): wait, if, gate, count, only once, cooldown, sequence, take turns, random chance, repeat;
  * values (11): text, number, variable, join text, maths, date / time, clipboard text, read a file, replace
    text, change text, random number;
  * other (1): the note.
  Threshold triggers start the phase 11 sampler only while such a trigger exists.
* **Editor**: an endless canvas (right / middle drag or Space + drag to pan, wheel zooms smoothly), round flow
  ports and diamond value ports, bezier wires, drag between compatible ports, drop a wire on empty space to pick a
  node that fits, the node picker (search, categories), field editors for every field kind, ◆ pins, glowing pulse
  dots with a peek at what they carry, status lines and progress bars, selection, undo / redo, copy / paste /
  duplicate, Delete, `F` to fit, the workspaces panel (switches, rename, export / import `.lumflow`), the
  variables panel, ▶ on button nodes.
* Buttons in the palette and the tray; the Automation page in the nav bar.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 64.25 % | Model, engine core, logic and value nodes, token expansion; compare / expand fixtures pass |
| 50 % | 66.5 % | All triggers and actions wired to services and modules |
| 75 % | 68.75 % | Editor canvas, nodes, wires, picker, field editors |
| 100 % | 71 % | Pulse animation, undo / redo, clipboard, workspaces and `.lumflow`, palette and tray; perf check |

**Done when.** `automation.json` from `sample_home` round-trips and runs; graph trace tests with a fake clock
pass; 1 000 pulses a second do not hitch the UI; a wire loop trips the safety stop instead of hanging.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 10 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/ARCHITECTURE.md (threads and data flow, the decision log), rust-rewrite-plan/PERFORMANCE.md and
luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 10 already started, continue from its handoff note and take
the next checkpoint in order.

Before anything else check the branch: `git log --oneline -3` must show the end of Phase 9 ("Plan: phase 9 at 75 %",
"Snapper: ...", "Doodle: ..."). If PROGRESS.md says less than "Phase 9 at 75 %", the branch is main without the later
work: it is on origin/claude/luminescent-phase-9-screen-tools-s65lah (or whichever branch the handoff note names), so
fast-forward onto it (`git merge --ff-only origin/<branch>`) and say so.

This session is Phase 10: automation. The spec is core/automation.py and ui/automation.py (read both in full),
core/events.py, the Automation section of README.md, and luminescent-rs/fixtures/automation_nodes.json (all 65 node
types with their ports and fields), automation_compare_cases.json, automation_expand_cases.json and
automation_load_cases.json.

Build everything in the Phase 10 Deliverables list, in checkpoint order. First lum-automation with no UI (the crate exists
as an empty shell): the automation.json model over raw JSON (unknown fields and key order survive; the sample file saves
back byte for byte through `lum_core::store` and `pyjson::to_string`), the engine with next-pass pulse delivery, value
pulls, token expansion, the max_rate safety stop, pause, status and saved state, on the services runtime with a fake
clock for tests. Then every node type, wired to the services and modules that exist (stub any that need a later phase
behind a clear "not available yet" status). Then the node editor with pulse animation, undo / redo, clipboard,
workspaces and .lumflow export / import.

What phases 3 to 9 left for you:
- Phase 3's Hello tray, phase 5's Looks right, phase 6's Dashboard, phase 7's Settings, phase 8's Daily-drivable alpha and
  phase 9's two-monitor checks may still be open; leave them on the waiting list and do not ask again unless the user
  brings them up.
- A module is a `UiModule` in `modules()` (launcher.rs, in the order of modules/__init__.py). Actions it registers reach
  the palette, tray, dashboard Tools tab and Settings with no extra wiring; nodes that "run a Luminescent tool" can call
  `App::run_action("<module>:<id>")` (every screen tool and the snapper's 15 spots are actions now). Commands from
  actions, hotkeys and worker threads reach a module through its `Inbox<Cmd>`; what it wants from the app goes back as a
  `Request` through `Services::send` (Notify, Toast, Emit an event, SendKeys, SetSetting, ApplyTheme, ThemeAccent,
  ThemeFromColour...), answered in `launcher/tools.rs`. Hotkeys: `UiModule::binds` + `bind_pressed`, `fixed_binds` for
  keys that are not settings, `hold_setting` + `hold_event` for a hold key. Tray: `tray_actions`, or `tray_menu` for
  submenus (an entry id `row:<id>` runs the module's palette row). Palette: `palette_entries`, `palette_live`,
  `run_palette`. A page is `page_key` + `page`.
- Screens: `lum_ui::window::screens()` (with `scale` now), `screens_in(ctx)` (tests install their own with
  `install_screens`), `DimOverlay`, `tools::frozen` (screenshots of every monitor, `lum_platform::capture`),
  `WindowPlacer::cover` for a window that must cover a monitor exactly on Windows. `lum_platform::snap`, `ocr`,
  `clipboard` (text, picture, files), `dialogs`, `desktop::cursor` / `move_cursor` exist for action nodes.
- Copy the shape of `crates/lum-ui/src/tools/` (`doodle` is the most complex: overlays per screen, state machine in a
  crate, `Act`s collected during a frame and applied after). Keep the logic that draws nothing in its own crate with tests
  (here lum-automation), and anything that talks to the OS in lum-platform / lum-hypr behind `cfg(target_os)`, never on the
  UI thread (sockets, processes and the clipboard go to worker threads or the services runtime).
- Gotchas: `Settings::get` takes `section.key` only, a whole section is `Settings::section`; `ctx.data_mut` must not call
  other `ctx` methods inside (deadlock); `IdTypeMap::remove_temp` needs `Default`, use `remove::<T>(id)`; egui 0.37 has no
  `raw_scroll_delta` (read `Event::MouseWheel`, which has `phase` and `source`); `f32::clamp` panics when its bounds
  cross, use `.min().max()`; give custom-painted widgets AND painted text `response.widget_info(...)` so kittest can find
  them; `Id::unique((salt, ...))`, not `Id::new`; fonts lack `＋`, `⧉`, `⌗` and `⏲`, probe new characters with
  `MissingGlyphPolicy::Panic`. In tests: kittest's `run()` lets time pass until nothing wants a repaint (a short timer fires
  inside it), so use `step()` around timers and drags; modifiers for a drag are `Event::ModifiersChanged`, and code should
  read a key's modifiers from its `Event::Key`; one harness per test when taking pictures; worker threads need a wait loop
  with a generous limit (the full suite under Xvfb is slow); delete a PNG before `UPDATE_SNAPSHOTS=1` when a layout moved a
  little. This container needs `apt-get update` and then `apt-get install -y libasound2-dev pkg-config libxkbcommon-x11-0
  mesa-vulkan-drivers xvfb gcc-mingw-w64-x86-64` (plus `rustup target add x86_64-pc-windows-gnu` and
  `cargo install cargo-deny --locked`); `sway`, `grim` and `tesseract-ocr` also install if a real compositor helps.

Tests: compare() and expansion fixtures, graph trace tests driven by a fake clock, a loop that must trip the safety stop,
a round trip of fixtures/sample_home/data/automation.json, snapshots of the editor in Dark and Light, and a benchmark of
1 000 pulses a second without a UI hitch. Check the budgets in PERFORMANCE.md (nothing polling while no workspace needs
it: `--idle-check`).

Rules: follow luminescent-rs/CLAUDE.md. Node titles, field labels and help texts come from the fixtures, unchanged. Never
use em dashes anywhere. Code should read like a careful human wrote it. Commit after each checkpoint.

Finish each session: fmt, clippy -D warnings (and the Windows target), tests, --smoke, cargo deny check. Update
rust-rewrite-plan/PROGRESS.md (checkpoints, percentages, handoff note naming the next checkpoint) and
FEATURE_INVENTORY.md. Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 11: System Monitor, Performance, System Settings, Workspace sets

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 10 % | **81 %** | Opus 5.5 for the sampler, tweaks and system backends; Sonnet 5.5 for the pages | high | 5 to 7 | Haiku 5.5 (low): option tables (labels, ranges) from the Python files |

**Goal.** Everything about the computer itself: the small System Monitor panel, the Performance window (widgets,
processes, boost, game mode, alerts and rules, startup apps, gaming HUD), System Settings (displays, brightness,
night light, input, desktop, appearance, power, sound, about) and workspace sets.

**Read first.** `modules/sysmon.py`, `modules/performance.py`, `core/perfstats.py`, `core/perftweaks.py`,
`ui/performance_ui.py`, `modules/syscontrol.py`, `core/syscontrol.py`, `core/workspaces.py`,
`tests/test_workspaces.py`, `tests/test_system_1_8_3.py`, the matching sections of `README.md`.

**Deliverables.**

* **lum-perf sampler** on its own thread, running only while something shows it or alerts, rules, automatic game
  mode or automation thresholds need it: CPU (every core, clock, temperature), memory and swap, GPUs (NVIDIA
  through NVML with `nvidia-smi` as fallback, AMD and Intel from `/sys/class/drm`, hwmon and each process's DRM
  fdinfo, every GPU on Windows through PDH counters), disks, network, sensors, battery, processes (icon, script
  names like `python · bot.py`, Wine names, CPU, memory, GPU, VRAM, disk, threads, priority, user) and process
  actions (end, force quit, priority, cores, open folder, copy command line).
* **Tweaks**: everything in `perftweaks.py` on Linux and Windows. Root changes go through one small pkexec helper
  with an allow-list, never a free-form root shell.
* **Performance window**: Overview widgets (every type, the widget editor, 1 to 4 columns, add / move / widen /
  duplicate / remove, `performance.json`), Processes (grouped tree, search, sort, tinted cells, right-click
  actions), Boost, Game mode (switch and key, start by itself when a listed game runs, lower priority and close
  lists, puts everything back even after a crash), Alerts & rules, Startup apps, the gaming HUD (click-through,
  corner, on top of games).
* **System Monitor panel** with a minute of history (`sysmon.json`).
* **System Settings**: displays (drag with snapping, resolution, refresh, scale, rotation, main display, variable
  refresh, mirroring, off, Identify, Apply with the 15 s keep-or-revert, Copy as config), brightness (DDC/CI
  through `ddc-hi`, laptop backlight), night light (hyprsunset / gammastep / wlsunset, the colour ramp on Windows),
  mouse / touchpad / keyboard, windows & desktop (Hyprland), Explorer & taskbar (Windows), appearance, power,
  sound devices and volume, This computer with Copy all; Keep my changes after a Hyprland reload; Back to my
  Hyprland config.
* **Workspace sets**: the Lua from `core/workspaces.py` generated the same way, its settings page, and the
  existing Lua tests ported to Rust with `mlua` (no system `lua` needed).

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 73.5 % | Sampler on Linux and Windows inside its CPU budget |
| 50 % | 76 % | Performance Overview, Processes, HUD, System Monitor panel |
| 75 % | 78.5 % | Boost, Game mode, Alerts & rules, Startup apps, the pkexec helper |
| 100 % | 81 % | System Settings in full; workspace sets with the mlua tests passing |

**Done when.** Sampling once a second with 400 processes stays under 1 % CPU; no `nvidia-smi` process is spawned
when NVML is available; display changes always revert after 15 s without a confirm; the workspace Lua tests pass.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 11 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/ARCHITECTURE.md (threads and data flow, the decision log, D66 to D71 above all), rust-rewrite-plan/PLATFORM_MATRIX.md,
rust-rewrite-plan/CRATES.md, rust-rewrite-plan/PERFORMANCE.md and luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 11 already
started, continue from its handoff note and take the next checkpoint in order.

Before anything else check the branch: `git log --oneline -3` must show "Plan: phase 10 done" and "Automation: pulses, undo,
workspaces...". If PROGRESS.md says less than "Phase 10 at 100 %", the branch is main without the later work: it is on
origin/claude/phase-10-automation (or whichever branch the handoff note names), so fast-forward onto it
(`git merge --ff-only origin/<branch>`) and say so. If your checkout has uncommitted work of the user's on another branch, leave it
alone and work in a git worktree on a new branch cut from origin/main.

This session is Phase 11: the system modules. The spec is modules/sysmon.py, modules/performance.py, core/perfstats.py,
core/perftweaks.py, ui/performance_ui.py, modules/syscontrol.py, core/syscontrol.py, core/workspaces.py, tests/test_workspaces.py and
tests/test_system_1_8_3.py. Read the ones for the checkpoint you are on before writing code.

Build everything in the Phase 11 Deliverables list, in checkpoint order: the lum-perf sampler (NVML first, nvidia-smi only as a
fallback, DRM fdinfo, PDH on Windows) that only runs while something needs it; the Performance window, HUD and System Monitor panel;
the tweaks with one pkexec helper that has an allow-list; System Settings; workspace sets with the Lua tests ported to mlua.

What phase 10 left for you:
- `lum_perf::levels` (CPU between two readings, memory, battery on Linux and Windows) is the stand-in the automation threshold trigger
  reads through the app's `AppHost::level` (`luminescent/src/launcher/automation.rs`). When the sampler exists, let the trigger read it
  (start the sampler only while a threshold trigger runs: the engine's `Watch::Levels` subscribers say when) and give it `cpu_temp`,
  `gpu` and `gpu_temp` (they answer `Level::NotYet`, "not available yet", today). Keep the texts of `core/automation.py`.
- `lum_platform::control` has the volume (wpctl / pactl on Linux; Windows says "not available yet" until the endpoint COM interface
  comes) and power actions; System Settings' sound devices and volume should reuse and extend it, and then the Volume node works on
  Windows too.
- The app answers automation requests in `launcher/automation.rs`; a module is a `UiModule` in `modules()` (launcher.rs); commands
  reach it through its `Inbox<Cmd>`, requests go back through `Services::send`; copy the shape of `crates/lum-ui/src/tools/` and
  `crates/lum-ui/src/automation/` (an egui layer with a transform when something must zoom with real widgets inside).
- What phases 3 to 10 left: the Hello tray, Looks right, Dashboard, Settings, Daily-drivable alpha, two-monitor and automation desktop
  checks may still be open; leave them on the waiting list and do not ask again unless the user brings them up.

Gotchas: `Settings::get` takes `section.key` only; `ctx.data_mut` must not call other `ctx` methods inside; `IdTypeMap::remove_temp`
needs `Default`, use `remove::<T>(id)`; egui 0.37 has no `raw_scroll_delta` (read `Event::MouseWheel`, which has `phase` and `source`),
uses `egui::Role` (not `WidgetType`), `Context::egui_wants_keyboard_input`, `Ui::scope_id`, `Id::unique((salt, ...))`; `f32::clamp`
panics when its bounds cross; give painted widgets and text `response.widget_info(...)`; fonts lack `＋`, `⧉`, `⌗` and `⏲`, probe new
characters with `MissingGlyphPolicy::Panic`; kittest's `run()` lets timers fire, use `step()`; it steps a quarter of a second a frame
(raise `max_double_click_delay` for double clicks); one harness per test when taking pictures; worker threads need a wait loop with a
generous limit; a popup opened by a click sees that click in the same frame. `jiff` 1.x calls it `ZonedDateTime`. In a cloud
container: `apt-get update`, then `apt-get install -y libasound2-dev pkg-config libxkbcommon-x11-0 mesa-vulkan-drivers xvfb
gcc-mingw-w64-x86-64`, `cargo install cargo-deny --locked`, and `rustup target add x86_64-pc-windows-gnu` inside luminescent-rs/. On
the user's own desktop install nothing system-wide: run tests with `HYPRLAND_INSTANCE_SIGNATURE` unset, expect the eight screen tool
pictures to differ without lavapipe, and if `x86_64-w64-mingw32-gcc` is missing run the Windows clippy only for crates without ureq.

Measure the sampler with 400 processes and compare it with PERFORMANCE.md. Anything that changes system settings must be reversible
the same way the Python code makes it reversible; display changes must revert by themselves after 15 seconds. Give the user commands
to verify on Hyprland and Windows 11 (there is no real desktop in a cloud session, and on the user's desktop do not change their
displays or run root helpers without asking first).

Rules: follow luminescent-rs/CLAUDE.md. Same labels as the Python app. Never use em dashes anywhere. Code should read like a careful
human wrote it. Commit after each checkpoint.

Finish each session: fmt, clippy -D warnings (and the Windows target), tests, --smoke, cargo deny check. Update
rust-rewrite-plan/PROGRESS.md (checkpoints, percentages, handoff note naming the next checkpoint) and FEATURE_INVENTORY.md. Stop at a
checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you learned in
this phase) and say which model and effort level to set. You have my permission to merge: once the session's work is committed,
every check passes and your branch is a fast-forward of main, push it to main yourself (`git push origin HEAD:main`) and tell me
you did; if it is not a fast-forward, stop and ask me. Include this same last paragraph at the end of the prompt you give me, so
every phase hands over the next one and keeps this permission.
```

---

## Phase 12: Hearing Guard, Media controls, sounds, speech

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 5 % | **86 %** | Opus 5.5 | high | 3 to 4 | none |

**Goal.** Real-time audio done natively: Hearing Guard's dB estimate, limiter, cap, dose and history on PipeWire /
PulseAudio and Windows, plus Media controls on MPRIS and Windows media sessions.

**Read first.** `modules/hearing.py`, `modules/hearing_win.py`, `modules/media.py`, `modules/media_win.py`,
`core/sounds.py`, the Hearing Guard and Media sections of `README.md`.

**Deliverables.**

* **lum-audio capture**: the default output's monitor on Linux (PipeWire, or PulseAudio through
  `libpulse-binding`), recorded only while the sink is running and stopped after a few seconds of silence; WASAPI
  loopback on Windows with the endpoint peak meter as stand-in while it starts.
* **DSP**: A-weighting (IIR biquads to IEC 61672, checked within 1 dB of the Python FIR across 31.5 Hz to 16 kHz
  using fixture signals), RMS in dBFS, master volume in dB, calibration per output device, the headset's own volume,
  estimated dB(A).
* **Hearing Guard logic**: limit mode (dip the master volume on Linux, the app sessions on Windows, slow
  recovery), the hard cap up to 150 %, record mode, NIOSH dose, today's levels and the week (`hearing.json`), the
  APPS view with each stream's peak and its own volume slider, the `hearing.over_limit` event, the status chip.
* **Hearing Guard UI**: big dB readout, level bar, the safe-time curve, DAY / WEEK / APPS, mode controls, the
  calibration dialog; panel, page and dashboard box.
* **Media controls**: MPRIS over D-Bus (players list, switch player, title, artist, cover, progress,
  previous / play-pause / next), Windows media sessions through the `windows` crate (media keys as fallback),
  optional hotkeys, panel / page / box, `media.json`.
* **Speech** for the automation "Say it out loud" node (`tts` crate).

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 82.25 % | Capture on both systems and the DSP with its accuracy fixtures |
| 50 % | 83.5 % | Limiter, cap, dose, history, APPS levels and volumes |
| 75 % | 84.75 % | Hearing Guard UI with calibration |
| 100 % | 86 % | Media controls on both systems, speech |

**Done when.** The dB estimate matches the Python app within 1 dB on the same audio; Hearing Guard uses under 0.5 %
CPU while music plays and nothing while silent; the limiter never leaves the volume dipped after the app quits.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 12 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/CRATES.md, rust-rewrite-plan/PERFORMANCE.md and luminescent-rs/CLAUDE.md first. If
PROGRESS.md shows Phase 12 already started, continue from its handoff note.

This session is Phase 12: audio and media. The spec is modules/hearing.py (read the long docstring carefully: it
explains the dB estimate), modules/hearing_win.py, modules/media.py, modules/media_win.py and core/sounds.py.

Build everything in the Phase 12 Deliverables list: lum-audio capture (PipeWire or PulseAudio monitor on Linux,
WASAPI loopback on Windows), the DSP with A-weighting checked against fixture signals generated from the Python
FIR, the Hearing Guard logic and UI, Media controls on MPRIS and Windows media sessions, and speech for the
automation node. The audio thread must never allocate or lock in its hot loop.

If the phase 0 fixtures lack the audio test signals, add them to luminescent-rs/tools/make_fixtures.py first
(sine sweeps and pink noise through the Python A-weighting and RMS code).

Rules: follow luminescent-rs/CLAUDE.md. Same labels as the Python app. The limiter must restore volumes on quit
and on a crash-restart, as the Python code does. Never use em dashes anywhere. Code should read like a careful
human wrote it. Commit after each checkpoint.

Finish: fmt, clippy -D warnings, tests, --smoke. Update rust-rewrite-plan/PROGRESS.md and FEATURE_INVENTORY.md.
Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 13: Ghost Vault, Metadata Stripper, Atlas Sync, Prompt Log, Keybinds

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 6 % | **92 %** | Opus 5.5 | xhigh | 3 to 4 | none |

**Goal.** The parts where a mistake costs data: the encrypted vault (must open every existing vault), metadata
stripping, the Atlas phone sync protocol, shell history editing and Hyprland config editing.

**Read first.** `modules/vault_store.py`, `modules/vault.py`, `modules/metadata.py`, `core/atlas_sync.py`,
`modules/atlas_sync.py`, `tests/test_atlas_sync.py`, `modules/promptlog.py`, `core/hyprbinds.py`,
`modules/keybinds.py`, DATA_AND_MIGRATION.md (crypto parameters).

**Deliverables.**

* **lum-vault**, byte-compatible with today's vaults: `.salt`, `.master`, `.dmaster`, `.recovery`, `.meta`,
  `.dmeta` and the blob files; PBKDF2-HMAC-SHA256 with 480 000 rounds; AES-256-GCM with the 12-byte nonce in front;
  the exact AAD strings (`master`, `dmaster`, `recovery`, `index`, none for blobs, none for pre-rewrite indexes);
  recovery keys formatted as hex groups of 4 and turned into a key with SHA-256(salt + cleaned key); decoy vaults
  completely separate; secure delete for moved originals; keys zeroised on lock. Operations: create, unlock,
  unlock with recovery, change passwords, regenerate the recovery key, add files and folders (copy or move),
  decrypt to a folder or temporarily, read into memory, rename, move, delete, search names.
* **Vault UI**: password dialogs with Forgot password?, file-card grid, breadcrumb, drop zone, progress, search,
  thumbnails decrypted in memory only. The vault never runs elevated and never goes into backups.
* **Metadata Stripper**: the metadata table (EXIF, GPS, ICC, XMP; video through `ffprobe`), strip images with the
  same re-encode and quality rules as today (orientation applied, animated images kept), videos through `ffmpeg`,
  whole folders to copies or in place after a confirm, the save folder, `--strip-metadata` on the command line, the
  Dolphin service menu and the Explorer context menu.
* **Atlas Sync**: the TCP host on port 47830, `ATLS` frames (type byte, big-endian length), PBKDF2-SHA256 of the
  code with salt `atlas-sync-1` and 120 000 rounds, AES-GCM, records and fingerprints with the shadow store in
  `atlas_sync.json`, notes both ways (to-dos as checklists), timer time adding up, themes, calendar events and
  tasks into the Calendar page (with skipped dates), the "Let Atlas connect" UI with address and 6-digit code.
  `tests/test_atlas_sync.py` ported.
* **Prompt Log**: fish, bash, zsh, PowerShell and Luminescent's own log with times, search, Hide repeats, copy,
  run again in a terminal, delete lines (atomic rewrite), clear, pins (`promptlog.json`), Save to Dashboard.
* **Keybinds**: Hyprland binds parsed with their real meaning (`hyprbinds.py` passing its fixtures: sources,
  variables, loop binds, descriptions), Luminescent's hotkeys, clashes in red, edit keys and action, remove
  (comment out), add; backups in `data/hypr-backups` and automatic undo when Hyprland reports a config error; My
  keybinds (keys or mouse buttons up to Extra button 12, does: app / command / tool / media / keys, "works while
  other keys are held"); on Windows, Luminescent's hotkeys and custom binds through the hook.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 87.5 % | lum-vault with cross-compatibility tests both ways, and the vault UI |
| 50 % | 89 % | Metadata Stripper, the CLI and the file manager menus |
| 75 % | 90.5 % | Atlas host passing its fixtures, the UI and the calendar feed |
| 100 % | 92 % | Prompt Log and Keybinds, with config edits verified on a copy of a real config |

**Done when.** The fixture vault made by Python opens in Rust and a vault made by Rust opens in Python; Atlas
frames match the fixtures byte for byte (given the same nonce); every hyprbinds fixture passes; no test writes
outside a temp folder.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 13 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/DATA_AND_MIGRATION.md (the crypto parameters), rust-rewrite-plan/RISKS.md and
luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 13 already started, continue from its handoff note.

This session is Phase 13: vault, metadata, Atlas Sync, Prompt Log and Keybinds. These parts can destroy user data
if they are wrong, so read the spec closely: modules/vault_store.py, modules/vault.py, modules/metadata.py,
core/atlas_sync.py, modules/atlas_sync.py, tests/test_atlas_sync.py, modules/promptlog.py, core/hyprbinds.py and
modules/keybinds.py.

Build everything in the Phase 13 Deliverables list, in checkpoint order. lum-vault must be byte-compatible with
existing vaults: same files, PBKDF2 rounds, nonce layout and AAD strings as vault_store.py. Prove it both ways:
the Python-made vault in luminescent-rs/fixtures opens in Rust, and add a small script under
luminescent-rs/tools that opens a Rust-made vault with the Python code. Atlas frames must match the fixtures byte
for byte given the same nonce. Hyprland config edits always back up first and undo themselves on a config error.

Never point any test or manual check at a real vault, real shell history or real Hyprland config: use temp copies.

Rules: follow luminescent-rs/CLAUDE.md. Use the RustCrypto crates (aes-gcm, pbkdf2, sha2) and zeroize keys. Same
labels as the Python app. Never use em dashes anywhere. Code should read like a careful human wrote it. Commit
after each checkpoint.

Finish: fmt, clippy -D warnings, tests, --smoke. Update rust-rewrite-plan/PROGRESS.md and FEATURE_INVENTORY.md.
Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 14: Desktop integration (Caelestia, Glass, borders, terminal, wallpapers)

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 4 % | **96 %** | Sonnet 5.5 | high | 2 to 3 | Haiku 5.5 (low): the Caelestia colour names and groups tables |

**Goal.** Everything a theme changes outside Luminescent: the Caelestia shell (colours, look, extras), glass
windows, window borders, the terminal (kitty, Windows Terminal, fish, fastfetch) and the wallpaper. Ends with the
**Feature-complete beta** milestone.

**Read first.** `modules/caelestia.py`, `modules/caelestia_look.py`, `ui/theme_extras.py`, `ui/colour_tools.py`
(Caelestia page), `core/terminal.py`, `core/wallpaper.py`, `core/wallpaper_win.py`, `core/hypr.py` (borders,
glass rule), the matching README sections.

**Deliverables.**

* **Wallpapers**: Caelestia, Noctalia, swww (GIFs), hyprpaper, swaybg, mpvpaper (videos), linux-wallpaperengine
  and Wallpaper Engine projects, Windows' own setting, the custom wallpaper command (`mytool set {file}`), moving
  wallpapers stopped when a still one is set; the automation Set wallpaper node wired up.
* **Terminal**: the kitty include line (asks first, keeps a backup, fixes a broken line), Windows Terminal
  fragment, Caelestia's fish sequences file updated and sent to open terminals, the start-up text in
  `~/.config/fish/conf.d/luminescent-greeting.fish` and `~/.config/fastfetch/luminescent.jsonc` (removed as soon
  as the theme stops); all terminal fixtures pass.
* **Caelestia & Glass**: every file in `~/.config/caelestia` as a form or text with backups; scheme colours
  (grouped, named, searchable, favourites, hover to light up the preview), Apply to Caelestia with a backup, keep
  colours per scheme or per theme, put my colours back; glass per app (focused / unfocused opacity, blur), every
  other app, Add the app I click next, blur strength; Lock the screen.
* **Caelestia look per theme**: `shell.json` keys with the old values remembered, and the extras (the patched
  `~/.config/quickshell/caelestia` copy made of links plus two patched files, the state file in
  `~/.local/state/luminescent/caelestia.json`, left alone when an update no longer fits the patch).
* **Window borders** per theme (active colour or two-colour gradient, inactive colour; put back after a reload).
* The theme editor's desktop pages filled in (Caelestia look, Glass, Window borders, Terminal) and the Caelestia
  quick tools.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 93 % | Wallpaper backends wired into theme apply and the automation node |
| 50 % | 94 % | Terminal, fish and fastfetch output passing fixtures |
| 75 % | 95 % | Caelestia module, look and extras |
| 100 % | 96 % | Glass, borders and the theme editor's desktop pages; **Feature-complete beta** |

**Done when.** Every file written outside Luminescent's data folder is backed up first and put back when the theme
stops using it, exactly as the Python app does; terminal fixtures pass; the user has switched themes on their real
Caelestia setup without surprises.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app. Read
rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 14 section), rust-rewrite-plan/PROGRESS.md,
rust-rewrite-plan/PLATFORM_MATRIX.md and luminescent-rs/CLAUDE.md first. If PROGRESS.md shows Phase 14 already
started, continue from its handoff note.

This session is Phase 14: desktop integration. The spec is modules/caelestia.py, modules/caelestia_look.py,
ui/theme_extras.py, the Caelestia page in ui/colour_tools.py, core/terminal.py, core/wallpaper.py,
core/wallpaper_win.py and the border and glass parts of core/hypr.py.

Build everything in the Phase 14 Deliverables list: wallpaper backends, terminal theming (kitty, Windows
Terminal, fish, fastfetch), the Caelestia & Glass module, Caelestia looks and extras per theme, window borders,
and the theme editor's desktop pages that phase 4 left as stubs.

Every file outside Luminescent's data folder must be backed up before the first write and restored when the
theme stops using it, exactly like the Python code. Test against temp copies of ~/.config, never the real one.
Terminal output must match the fixtures in luminescent-rs/fixtures.

Rules: follow luminescent-rs/CLAUDE.md. Same labels as the Python app. Never use em dashes anywhere. Code should
read like a careful human wrote it. Commit after each checkpoint.

Finish: fmt, clippy -D warnings, tests, --smoke. Update rust-rewrite-plan/PROGRESS.md and FEATURE_INVENTORY.md.
Stop at a checkpoint boundary if you run low on room.

Last, give me the prompt for the next phase (the next phase from rust-rewrite-plan/PHASES.md, updated with anything you
learned in this phase), say which model and effort level to set, and tell me if the branch needs merging first.
Include this same last paragraph at the end of the prompt you give me, so every phase hands over the next one.
```

---

## Phase 15: Migration, packaging, updater, parity audit, release 2.0

| Share | Progress when done | Model | Effort | Sessions | Helpers |
|---|---|---|---|---|---|
| 4 % | **100 %** | Opus 5.5 | high | 2 to 4 | Haiku 5.5 (low): CHANGELOG and README wording drafts |

**Goal.** Users of 1.9.x move to 2.0 with one click and lose nothing; the release workflow builds the Rust app;
every feature is checked off; the numbers beat the baseline; the Python app retires.

**Read first.** `core/updates.py`, `build.py`, `.github/workflows/release.yml`, `core/__init__.py`
(`_first_frozen_run`), `core/backup.py`, DATA_AND_MIGRATION.md, PERFORMANCE.md, BASELINE.md.

**Deliverables.**

* **First run of 2.0**: find the existing data (the data folder; or ask for a source-run folder, like
  `_first_frozen_run`), make `data/backups/before-2.0-<date>.zip` of everything before the first write, check every
  file loads, show a short "what is new in 2.0" message and how to go back.
* **Packaging**: `.github/workflows/release.yml` builds the Rust app. Keep the archive names
  (`Luminescent-windows.zip`, `Luminescent-linux.tar.gz`) and the executable path (`Luminescent.exe`,
  `Luminescent/Luminescent`) so the 1.9.x updater can install 2.0 by itself. Windows icon and version info
  embedded; Linux built on Ubuntu 22.04 for the glibc 2.35 baseline; the release text still comes from
  CHANGELOG.md.
* **Updater**: the daily check and one-click update from `core/updates.py`, rebuilt in Rust; tested from a real
  1.9.21 build to 2.0 on both systems.
* **Parity audit**: every line of FEATURE_INVENTORY.md ticked or written up as a known gap with a reason; every
  fixture passing; PERFORMANCE.md numbers measured and compared with BASELINE.md in a short report.
* **Docs**: README.md, UI_OVERVIEW.md and CHANGELOG.md (2.0) rewritten for the Rust app; the Python app tagged
  `v1.9.21-final` and moved to `legacy-python/` (or removed, the user decides); the Python CI retired.

**Checkpoints.**

| In phase | Overall | What is done |
|---|---|---|
| 25 % | 97 % | First-run migration, the safety backup, load checks |
| 50 % | 98 % | Release workflow and the 1.9.x → 2.0 update path tested on both systems |
| 75 % | 99 % | Parity audit complete, gaps fixed or written up |
| 100 % | 100 % | Performance report, docs, release 2.0 |

**Done when.** A 1.9.21 install on Windows and on Linux updates itself to 2.0 and everything is still there; the
inventory is complete; every budget in PERFORMANCE.md is met or explained.

**Prompt.**

```text
We are rewriting Luminescent (a Python + PyQt6 tray app in this repo) as a native Rust app, and this is the last
phase. Read rust-rewrite-plan/README.md, rust-rewrite-plan/PHASES.md (Phase 15 section),
rust-rewrite-plan/PROGRESS.md, rust-rewrite-plan/DATA_AND_MIGRATION.md, rust-rewrite-plan/PERFORMANCE.md,
rust-rewrite-plan/BASELINE.md, rust-rewrite-plan/FEATURE_INVENTORY.md and luminescent-rs/CLAUDE.md first. If
PROGRESS.md shows Phase 15 already started, continue from its handoff note.

This session is Phase 15: migration, packaging, the updater, the parity audit and the 2.0 release. The spec is
core/updates.py, build.py, .github/workflows/release.yml, core/__init__.py (_first_frozen_run) and
core/backup.py.

Build everything in the Phase 15 Deliverables list. The release archives must keep their names and executable
paths so the 1.9.x updater can install 2.0 by itself. The first run of 2.0 must back up everything before it
writes anything. Then go through FEATURE_INVENTORY.md line by line: tick what works, fix what does not, and write
up any gap with a reason. Measure every number in PERFORMANCE.md and write a short comparison with BASELINE.md.

Ask me before deleting or moving the Python app, before changing the release workflow on main, and before
publishing a release.

Rules: follow luminescent-rs/CLAUDE.md. Never use em dashes anywhere. Code and docs should read like a careful
human wrote them. Commit after each checkpoint.

Finish: fmt, clippy -D warnings, tests, --smoke, and a dry run of the release workflow. Update
rust-rewrite-plan/PROGRESS.md to 100 % with a final note.
```

---

## Phase 16: Beyond parity (its own track)

| Share | Progress | Model | Effort | Sessions |
|---|---|---|---|---|
| own track | measured in [NEW_FEATURES.md](NEW_FEATURES.md) by ticked items done | per feature (listed there) | per feature | ongoing |

Once 2.0 ships, pick features from [NEW_FEATURES.md](NEW_FEATURES.md) by ticking their boxes. Each one is a small
phase of its own with the model and effort listed next to it. Progress for this track is "ticked features done ÷
ticked features". Use this prompt for each feature:

```text
Luminescent is now a Rust app in luminescent-rs/. Read rust-rewrite-plan/NEW_FEATURES.md,
rust-rewrite-plan/ARCHITECTURE.md, rust-rewrite-plan/PERFORMANCE.md and luminescent-rs/CLAUDE.md first.

Build this feature from NEW_FEATURES.md: <paste the feature's line here>.

Start by writing a short plan (which crates change, the settings it needs, how it stays off when switched off,
how it is tested) and wait for my OK. Every new behaviour gets a Settings toggle and is off by default unless I
say otherwise. Keep idle cost at zero when the feature is off. Same plain British English wording as the rest of
the app.

Rules: follow luminescent-rs/CLAUDE.md. Never use em dashes anywhere. Code should read like a careful human wrote
it. Commit when done; tick the feature in NEW_FEATURES.md, add it to CHANGELOG.md, and update the Phase 16 line
in rust-rewrite-plan/PROGRESS.md.
```
