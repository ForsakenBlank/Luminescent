# Target architecture

How the Rust version is put together. Phase 1 and 2 build the skeleton described here; later phases fill it.

## Goals, in order

1. **Same app, same data.** Every feature in [FEATURE_INVENTORY.md](FEATURE_INVENTORY.md), reading and writing
   the same files.
2. **Fast and quiet.** Idle at 0 % CPU, nothing polling while hidden, the overlay on screen within a frame of
   the hotkey (budgets in [PERFORMANCE.md](PERFORMANCE.md)).
3. **One failing part never takes the rest down.** Same promise as the Python app's `core/guard.py`.
4. **Easy to change with an AI assistant.** Small crates with clear jobs, plain Rust, tests that pin behaviour.

## D1: the UI framework

**Choice: egui + eframe**, with a decision gate in phase 2 that can switch to iced before anything big is built.

| Need | egui / eframe | iced | Slint | GTK4 (gtk-rs) | Tauri (web view) |
|---|---|---|---|---|---|
| Windows 11 + Linux from one code base | yes | yes | yes | Linux first, Windows painful | yes |
| Several transparent, frameless, always-on-top windows | yes (viewports) | yes (daemon, multi-window) | yes | yes | yes |
| Heavy custom painting (node editor, graphs, gradients, glow) | excellent (Painter, meshes) | good (Canvas with caches) | weak | good (Cairo/Snapshot) | good (Canvas) |
| Live theme switching everywhere | free: immediate mode repaints with the new tokens | good (style functions) | good | CSS reload | CSS |
| Idle cost | 0 when nothing changes (reactive repaint) | 0 | 0 | 0 | a browser engine in memory |
| Memory | low | low | lowest | medium | high (WebView2 / WebKitGTK) |
| How well AI assistants know it | very well, stable API | API changed a lot between versions | fair | fair | very well |
| Click-through windows | `with_mouse_passthrough` | via winit | via winit | input regions | limited |

Why egui wins here:

* Luminescent already paints most of its look by hand (dashboard boxes, the backdrop, gradients, graphs,
  the node editor). egui's painter does that directly, and its immediate mode means "everything is themeable
  and restyles live" costs nothing: every frame reads the current tokens. In Qt that took a global stylesheet,
  `retheme()` on every module and rebuilds.
* No retained widget tree means no "wrapped C/C++ object has been deleted" class of bug.
* `egui_kittest` gives headless UI tests and PNG snapshots, which replaces `tools/shots.py`.
* Assistants write correct egui code reliably, which matters for a rewrite done mostly through prompts.

Where egui is weaker, and the plan's answer:

* **Polished typography**: bundle Geist + Geist Mono, use `extra_letter_spacing` for the mono caps labels,
  build our own widget kit (phase 2) instead of the default look.
* **Large lists**: always `ScrollArea::show_rows` (virtualised), never a thousand widgets.
* **Rich text editing**: plain `TextEdit` plus a Markdown preview (`egui_commonmark`), the same split the
  Python Notes panel has.

The **phase 2 gate** must pass on Hyprland and Windows 11 before phase 3 starts: transparent frameless
always-on-top windows on every monitor, click-through toggling, no focus stealing, file drops from the file
manager, 0 % idle CPU, a 300-field settings form that scrolls smoothly, and 400 wires with 100 animated pulses at
60 fps. If egui fails a check that iced passes, switch to iced and record why in the decision log below.

**Renderer**: eframe can use `glow` (OpenGL) or `wgpu`. Phase 2 measures both. Expect `glow` to win on memory and
on transparent windows on Windows; keep `wgpu` behind a cargo feature.

## Workspace layout

```
luminescent-rs/
  Cargo.toml                 workspace, shared lints and profile settings
  CLAUDE.md                  rules for every session (copied from the plan in phase 1)
  crates/
    lum-core/                paths, JSON storage, settings schema + store + profiles, event bus, actions,
                             usage log, backups, fault guard, start guard, logging setup
    lum-theme/               palette tokens, gradients, colour maths (tones), theme store, generation,
                             import/export, per-module looks
    lum-hypr/                Hyprland socket + event socket, the LUMI Lua bootstrap, binds via FIFO,
                             window rules and placement, hyprbinds parser/editor, workspace sets Lua
    lum-platform/            traits + linux/windows backends: hotkeys, clipboard, screen capture, tray,
                             notifications, autostart, keep awake, idle time, launching (runner), terminals,
                             desktop entries and icons, wallpaper, displays, power, processes
    lum-automation/          graph model, engine, every node type's runtime, token expansion
    lum-calc/                calculator parser and evaluator, unit/currency/date conversions, time parsing
    lum-vault/               Ghost Vault storage and crypto (no UI)
    lum-atlas/               Atlas Sync wire protocol, records, shadow store, TCP host
    lum-perf/                system sampling (CPU, memory, GPUs, disks, network, sensors, processes) and tweaks
    lum-audio/               Hearing Guard capture backends, A-weighting, limiter, dose maths, app streams,
                             media session backends, sound playback, speech
    lum-ui/                  the egui app: widget kit, overlay, pages, floating panels, editors
    luminescent/             the binary: wiring, tray, single instance, CLI (--smoke, --strip-metadata, ctl)
  fixtures/                  golden data generated from the Python app (phase 0)
  tools/                     make_fixtures.py, measure scripts, snapshot helpers
```

Dependency direction (an arrow means "may depend on"):

```
luminescent ─► lum-ui ─► lum-automation ─► lum-core
                  │  └─► lum-theme ───────► lum-core
                  ├────► lum-platform ────► lum-hypr ─► lum-core
                  ├────► lum-calc, lum-vault, lum-atlas, lum-perf, lum-audio ─► lum-core
```

Rules: only `lum-ui` and `luminescent` know about egui. Library crates have no UI and are tested on their own,
the way `vault_store.py` already is. Platform-specific code lives behind `cfg(target_os)` inside
`lum-platform`, `lum-perf` and `lum-audio`, never sprinkled through the UI.

## Threads and data flow

```
                ┌──────────────────────────── UI thread (eframe / winit) ───────────────────────────┐
                │ AppState (settings snapshot, theme tokens, module UI state)                       │
                │ every window = one egui viewport: overlays, panels, toasts, HUD, doodle, pickers  │
                └───────▲───────────────────────────────────────────────────────┬──────────────────┘
                        │ AppEvent (crossbeam channel) + ctx.request_repaint()   │ Command
                        │                                                        ▼
  ┌──────────────── services (tokio runtime, 2 worker threads) ────────────────────────────────────┐
  │ Hyprland event socket + bind FIFO · automation engine · Atlas host · HTTP · file watchers ·     │
  │ process spawning · update checker · debounced JSON saving                                       │
  └─────────────────────────────────────────────────────────────────────────────────────────────────┘
  dedicated threads: Windows keyboard hook (own message loop) · audio capture · perf sampler (only
  while something shows it) · OCR / thumbnails / metadata (rayon pool, on demand)
```

* **The UI thread never blocks.** Anything that touches a socket, a process, the disk (beyond a small JSON
  read at start) or the network goes to a service and comes back as an `AppEvent`.
* **Services never touch UI state.** They send events; the UI applies them.
* **Read-mostly shared data** (the active theme tokens, the settings snapshot) sits in an `ArcSwap` so services
  can read it without locks.
* **Saving is debounced** (500 ms, like the Python `schedule_save`) and atomic.
* **Hotkeys**: the Hyprland FIFO reader or the Windows hook only forwards a bind id to the UI thread. The hook
  callback itself must return in microseconds or Windows drops the hook.

## Modules

Each tool is a module, mirroring `core/module.py`:

```rust
pub trait Module {
    fn key(&self) -> &'static str;          // the settings section, the hotkey owner
    fn title(&self) -> &'static str;
    fn start(&mut self, cx: &mut ModuleCx) -> anyhow::Result<()>;
    fn stop(&mut self, cx: &mut ModuleCx);
    fn apply_settings(&mut self, cx: &mut ModuleCx) {
        self.stop(cx);
        let _ = self.start(cx);
    }
    fn actions(&self) -> Vec<ActionSpec> { Vec::new() }     // palette, tray, dashboard Tools tab, automation
    fn on_event(&mut self, _cx: &mut ModuleCx, _ev: &AppEvent) {}
    fn show(&mut self, _ui: &mut UiCx) {}                   // draws its windows each frame they are open
}
```

* `ModuleCx` gives access to settings, the hotkey manager (owner = module key), notifications, the action
  registry, the event bus and the services.
* The registry is a static list like `modules/__init__.py`. A module that fails to start is recorded and
  shown in a toast; the rest start.
* **Actions** are one registry (`id`, label, hint, callback) shared by the command palette, the tray, the
  dashboard Tools tab and the automation "Luminescent tool" node. That is how the Python app already works.

## Windows on screen (the OS kind)

* One egui **viewport per window**: the overlay backdrop per monitor, each floating panel, toasts, the HUD,
  the doodle canvas per monitor, the frozen-screen pickers per monitor.
* A `WindowPlacer` trait hides the platform:
  * **Hyprland**: windows cannot place themselves on Wayland, so (exactly like `core/hypr.py`) each window gets
    an invisible title marker and the placer moves it through the Hyprland socket when it opens, plus window
    rules from the LUMI Lua table for no-focus, no-activate, pinning, blur and glass.
  * **Windows**: native positions, `WS_EX_NOACTIVATE`, `WS_EX_TRANSPARENT` for click-through, `WS_EX_TOOLWINDOW`
    to stay off the taskbar.
  * **Other Linux**: X11 positions work natively; other Wayland compositors get best effort (phase 16 adds
    layer-shell surfaces, see NEW_FEATURES.md).
* Windows that are not on screen do not exist or are hidden and not repainted. Nothing ticks for them.

## Persistence

* Every data file has a serde model in the crate that owns it. Every model keeps fields it does not know:

```rust
#[derive(Serialize, Deserialize)]
struct Note {
    id: String,
    name: String,
    #[serde(default)]
    content: String,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,   // keeps fields a newer or older version wrote
}
```

* `lum_core::store::save_json` writes to `<file>.tmp`, fsyncs, renames. `load_json` returns the default on a
  missing file and moves an unreadable one to `<file>.bad`, the same contract as the Python helpers.
* Settings are a schema (sections of fields with type, default, choices, help) generated from the Python
  `SCHEMA` dump in phase 0, stored as JSON with unknown sections and keys kept. Profiles work as today:
  `settings.json` holds the active profile, `data/profiles/<id>.json` the others, shared sections stay shared.
* Details of every file: [DATA_AND_MIGRATION.md](DATA_AND_MIGRATION.md).

## Errors and fault isolation

* Library crates return `thiserror` errors; the app uses `anyhow` with context.
* Module callbacks (hotkeys, actions, `show`, `on_event`) run inside `catch_unwind`. A panic is logged with its
  backtrace, counted per module, and shown as a small "Luminescent error" toast. Six faults in a minute switch
  the module off for the session (the Python `Faults` rule).
* **Start guard**: `data/starting.json` notes each start; two starts in a row that never reach "ready + 20 s"
  mean the next start is safe mode (no modules, no page boxes, automations paused).
* A panic hook writes `data/crash.log`.
* Logging uses `tracing`, to `data/launcher.log` (rotating, 512 KB × 3) and to stdout when it is a terminal.

## Theming pipeline

```
themes.json ─► ThemeStore ─► Palette (16 colour roles + radius, font, glow, image, gradients)
                                 │
                                 ├─► Tokens (ArcSwap) ─► every widget reads tokens each frame
                                 ├─► egui::Style / Visuals rebuilt on change (one call)
                                 └─► per-module looks: a scoped token override while drawing that module
```

* Gradients (linear, radial, sweep) are painted as meshes; the overlay glow and dot grid are rendered once per
  monitor into a cached texture and only redrawn on theme or size change (the Python `_Backdrop` rule).
* Text scale (85 % to 150 %) multiplies every font size in one place.
* Performance mode turns off glow, background images and every animation.

## Animation

One `Animator` with the three styles from `core/anim.py` (Caelestia with a small overshoot, Smooth, Snappy),
three speeds and a switch per animation kind. It only asks for repaints while something is moving, and returns
the end value at once when animations are off.

## IPC and the command line

* Single instance: a lock in the data folder plus a local socket (`$XDG_RUNTIME_DIR/luminescent/ctl.sock`, a
  named pipe on Windows).
* `luminescent ctl overlay`, `luminescent ctl palette`, `luminescent ctl action <id>` send commands to the
  running app. This gives any compositor or script a way in, even without Hyprland binds.
* `luminescent --smoke` builds every page and pop-up once and exits 0 (CI uses it, as now).
* `luminescent --strip-metadata <paths>` keeps the Dolphin / Explorer menu working.

## Decision log

Add a row whenever a phase makes or changes a decision.

| # | Decision | Why | Phase |
|---|---|---|---|
| D1 | egui + eframe for the UI, gated in phase 2 | Custom painting, live theming, idle cost, assistant familiarity | 0 |
| D2 | Keep every data format byte-compatible until phase 15 | Users can switch back and forth; no migration risk while building | 0 |
| D3 | Keep the Hyprland socket + Lua (LUMI) approach for binds, rules and workspace sets | It works today, and the Lua is reused as-is | 0 |
| D4 | tokio for background services, crossbeam channels to the UI | Many small IO jobs; one runtime is simpler than many threads | 0 |
| D5 | Native libraries over spawned tools where a good crate exists (NVML, PipeWire, MPRIS over D-Bus, DDC/CI) | Spawning a process per sample is most of the current idle cost | 0 |
| D6 | The Rust project lives in `luminescent-rs/` in this repo | Sessions can read the reference code and generate fixtures from it | 0 |
| D7 | `jiff` for dates and times, not `chrono` | RustSec flags chrono as unmaintained (RUSTSEC-2026-0543), so `cargo deny` fails on it; jiff is maintained, handles local time zones well, and few files used dates yet | 1 |
| D8 | Data files are written byte for byte like Python's `json.dump` (`lum_core::pyjson`: same separators, indents, `\uXXXX` escapes, float `repr`) and keep key order (`serde_json` with `preserve_order`) | Switching between the two apps never rewrites a whole file; every file in `fixtures/sample_home` saves back unchanged | 1 |
| D9 | The settings store keeps unknown sections and keys; a settings.json that can not be read is left alone at load (as Python does) and kept as `settings.json.bad` on the first save | Rule 2 and 4 of DATA_AND_MIGRATION.md; Python would overwrite the damaged file on its next save | 1 |
| D10 | Debug builds use `Luminescent-dev` as their home unless `LUMINESCENT_HOME` is set, and their ctl socket name is tagged with the home path | Development can never touch the real setup, and a dev copy can run next to the real app | 1 |
| D11 | One copy per data folder through an OS file lock on `data/instance.lock` plus the ctl socket (Unix socket in `$XDG_RUNTIME_DIR/luminescent/`, a named pipe on Windows). One line in, one answer back, then the connection closes. A second start sends `overlay` and exits | The lock frees itself when the process dies, so no stale locks; the Python QLockFile in /tmp is not shared, so the two apps can still run at once (phase 15 decides whether to detect the Python one) | 1 |
| D12 | The `Module` trait in lum-core has no `show`; lum-ui adds drawing on top in phase 2. lum-core also has `ModuleHost` (start, stop, apply settings, fault counting) | lum-core has no UI dependency; the host is testable headless | 1 |
| D13 | The single-instance lock, the ctl socket and quit signals live in lum-platform | Keeps every `cfg(target_os)` out of the binary and the UI | 1 |
| D14 | A tool action run counts once in usage.json | Python's `run_action` records it twice (once without a label, once with), which doubles the Usage page counts; the file format is unchanged | 1 |
| D15 | The pre-1.0 `config.py` import (`_migrate_old_config`) is not ported | It only ran for setups from before settings.json existed; any 1.9.x setup already has settings.json | 1 |
| D16 | HTTP (currency rates, update checks) will use `ureq` inside `spawn_blocking`, not `reqwest` | Few, small requests; ureq pulls in far less and keeps the binary small. Nothing in phase 1 needed HTTP yet | 1 |
| D17 | `--smoke` and `--bench-startup` use a throw-away data folder unless `LUMINESCENT_HOME` is set | A test run never writes into a real or dev setup | 1 |
