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
| D18 | The main window stays "visible" to winit: a transparent, click-through 1x1 window placed off screen (on Hyprland the LUMRS rule keeps it unfocused in a corner), never `Visible(false)` | eframe paints a hidden main window straight from `new_events`, outside the event loop context, so an immediate viewport first opened from that pass gets no window ("the user callback was never called" panic), and every repaint of an immediate viewport is a repaint of its parent | 2 |
| D19 | Every window of ours (panels, dims, later the overlay and toasts) is an immediate viewport drawn from the main window's pass; where there are no viewports (tests, `--smoke`) the same code draws areas in one window. Dialogs are modal layers inside the window that opens them | Module state stays plain `&mut` with no `Arc<Mutex>`; with repaint-on-change the shared pass costs nothing while idle. Phase 5 measures the overlay with several windows open and can move hot windows to deferred viewports | 2 |
| D20 | lum-ui bundles "Luminescent Symbols" (a renamed DejaVu Sans subset: arrows, shapes, dingbats, technical symbols, box drawing) and egui's monochrome emoji fonts as the fallback after Geist. A missing glyph draws a box in the app and panics in kittest tests | Qt finds ✕ ✓ ✦ ⚙ in system fonts; egui only has what it is given, and 0.37's default fonts no longer carry them. `＋`, `⌕ ⌗ ⏯ ⏲ ⏻ ␟ ⓘ ⤢ ⧉` and five emoji have no free font we can bundle: the phases that port those screens paint them or use another character | 2 |
| D21 | `ModuleHost` is generic over the module type (`ModuleHost<M: ?Sized + Module = dyn Module>`); lum-ui's `UiModule: Module` adds `show(&mut UiCx)` and the app's host holds `dyn UiModule`. A panic while drawing counts as a fault like any other | Follows D12: lum-core stays free of egui and its tests keep plain modules | 2 |
| D22 | The Animator is immediate mode: `anim::value(ctx, id, target, kind, ms, ease)` gives the value to show and asks for a frame only while it is still moving; `anim::small` (120 ms) is for state changes inside a control and follows only the master switch and performance mode. Styles and speeds are `core/anim.py`'s | Nothing to start, stop or keep alive, and an idle window can not tick by accident | 2 |
| D23 | Gradients are meshes clipped to the rounded rectangle with a 1 px fringe, cached per rect and gradient; the panel glow (at a quarter resolution), the top halo and the dot grid are textures made once per size and colour | Sharp at any scale with no texture per gradient, and the "paint it once" rule of `_Backdrop` | 2 |
| D24 | `--idle-check` names the renderer and its adapter, prints private memory on Linux, and does not fail the memory budget on a software renderer (llvmpipe, lavapipe, WARP) | llvmpipe alone maps about 57 MB of LLVM and Gallium into the process (the 114 MB the first run saw); the app's own memory was 35 MB | 2 |
| D25 | kittest snapshots of the gallery and the dialogs are compared on Linux only (lavapipe in CI), with a little slack (threshold 1.5, 200 pixels); other systems build and step the same UI without a picture | Software renderers smooth edges differently, and CI's Windows runners may have no adapter | 2 |
| D26 | **The gate: keep egui + eframe** (provisional, by the user's call), with glow as the default renderer and wgpu behind the `wgpu` feature. Hyprland check 2 (click-through) failed on wgpu and has no valid glow result; checks 1, 3 to 8 on Hyprland and every Windows 11 check are still open. Phase 3 owns click-through: windows that are click-through from the start get it before they are first shown (as the Python app does), a runtime toggle is followed by a repaint of that window so the surface commits, and the Hello tray steps re-run `--spike clickthrough` (glow, mouse moving) and the open checks on both systems. If click-through still fails there, try a second frame after the toggle, then trace how Hyprland picks the window under the pointer, before anything bigger | The failure is below the toolkit: the trace shows winit sending the right request, and iced uses the same winit, so switching would not change it. The Hello tray milestone itself needs no click-through | 3 |
| D27 | The Rust app's Lua lives in its own table `LUMRS` and reports through `$XDG_RUNTIME_DIR/luminescent/events-rs.fifo` while the Python app may run (until phase 15). The text is `core/hypr.py`'s, built the same way and renamed after (`lum_hypr::lua::Names`); `hypr_cases.json` proves the `LUMI` version is byte-identical. Both apps keep the class `luminescent`, so each one's rules (the same rules) also match the other's windows | Sharing `LUMI` would let each app's bootstrap switch off the other's binds and point them at its own FIFO. Phase 15 decides whether to move to `LUMI` once the Python app retires | 3 |
| D28 | Windows are placed by title: `WindowPlacer::shown(title, at, focus)` is called every frame a window is drawn and is free when nothing changed. Hyprland: lum-hypr's `WindowSync` worker thread (moves after `openwindow`, the 2.5 s settle checks, focus, raise, float on top), fed by the event socket. Windows: winit places; the backend adds `WS_EX_NOACTIVATE` to `NO_ACTIVATE` / `NO_FOCUS` windows and takes the foreground with `AttachThreadInput`. Full-screen windows (dims, later the overlay backdrops) are titled `<prefix> on <monitor> ` and placed by cover rules installed at start, after monitor changes and after reloads | egui viewports have no handle we can reach, but their titles are ours and unique. Socket calls stay off the UI thread. A cover rule must exist before the window maps, which a per-frame wish can not guarantee | 3 |
| D29 | Hotkeys are events, not callbacks: `HotkeyManager` (UI thread) keeps the books and hands out `HotkeyEvent`s (fired with the registered id, hold down / up, interrupted, refused). Backends run on their own threads: Hyprland binds through `LUMRS` with ids coming back through the FIFO (socket calls, the 5 s cached clash check against your own binds and the 100 ms hold check on a worker); on Windows one `WH_KEYBOARD_LL` thread whose callback only forwards the key and whether it was already down, and a matcher thread (`hotkeys::matcher`, plain logic tested on Linux) with the Python stuck-key rules. A clash with your own Hyprland bind arrives as `Refused` a moment after `register` returns | Module state stays on the UI thread with no locks, the UI never waits on a socket, and the hook can not be slow. The checks run only while a key is held |
| D30 | The OS services that can wait on another process live on threads of their own: the tray (ksni's blocking service on Linux, tray-icon with a message loop on Windows, both fed by a channel), notifications (a short thread each), keep awake (one thread holds the logind lock or the Windows execution state, which belongs to the calling thread), sounds (a thread per sound, rodio). The clipboard uses arboard with the Wayland data-control protocol, so it works without our window having the keyboard; it can wait on the clipboard's owner, so callers keep it off the UI thread. Autostart is built but the app does not sync it on start while the Python app is in use (it would point the Python app's entry at the Rust binary); phase 15 turns that on. Clipboard change watching moves to phase 8, where the clipboard history needs it | The UI never waits on D-Bus, the shell or an audio device, and nothing stays open between uses | 3 |
| D31 | File dialogs are the system's: `rfd` with only the `xdg-portal` feature (no wayland-sys, no GTK), run on a thread of their own with a callback; they live in `lum_platform::dialogs` | The portal matches the user's desktop on Hyprland, Windows uses the common dialogs, and nothing waits on the UI thread | 4 |
| D32 | `Theme` is the raw JSON object with typed getters, so unknown keys and key order survive; `ThemeStore` saves through `lum_core::store::Writer` (a background thread, `flush()` at quit) and reports what happened as `ThemeEvent`s the app drains in `poll` | themes.json must round-trip byte for byte with the Python app, and saving must not touch the disk on the UI thread | 4 |
| D33 | The overlay's backdrop is a few textures made once (the dot tile, the halo, the panel glow per panel size, the theme picture decoded on a thread at up to 3840 px) drawn as a handful of quads per screen, not one full-screen texture per screen; `prewarm` makes them (and lays out the switcher's and chrome's text) 3 s after start through a painter that draws nowhere | The same "paint it once" rule as `_Backdrop` with no 33 MB texture per 4K screen and no CPU composite on every theme switch; opening and switching pages stay a few quads | 5 |
| D34 | `MainOverlay` does not own pages: the app passes a function that draws the page asked for into the space it gets (`PageSlot`), each call inside `catch_unwind`; a page that panics is reported (`Out::repair`) and the app throws its state away, and a panic in the overlay itself throws the whole overlay away (`repair` in Python). Pages say `busy` while a dialog of theirs has the keyboard | Page state stays in the app with plain `&mut` borrows (D19), and the overlay stays testable on its own with stand-in pages | 5 |
| D35 | The command palette's rows are ids (`page:`, `act:`, `theme:`, `settings:`, `lum:`, later `item:`, `calc:`, `remind:`), not callbacks; the app runs what comes back. Matching is nucleo-matcher's `Pattern` (each word fuzzy) over `label hint`, with recent entries lifted by `rank × 2`; results refresh in the same pass as the keystroke | No closures that borrow the app, the same ids `palette.json` already holds, and the 5 ms budget with room to spare | 5 |
| D36 | The cheat sheet's hold mode asks the hotkey manager to watch its combo (`watch_release`): the Hyprland worker or the Windows matcher thread checks every 50 ms while it is up and reports `Released`, or `ReleaseUnknown` when Hyprland can not tell at the first check (it then works in toggle mode, as Python) | Nothing polls while it is hidden, and the UI thread never asks Hyprland's socket itself | 5 |
| D37 | Luminescent cards sit behind the same `Notifier` trait: `AppNotifier` queues them (a mutex, then a wake-up) and the UI thread draws each card as a small window of its own. Right-click copies at once and the card says "Copied" | A menu is drawn inside its window, and a card's window is only as big as the card | 5 |
| D38 | No acrylic behind the overlay on Windows, and the toggle's help text stays "Linux (Hyprland) only for now" | Parity: the Python app has no acrylic either; NEW_FEATURES.md has it as an idea | 5 |
| D39 | The dashboard's data is the JSON document itself (`lum_core::dashboard::DashboardStore`), read through `TabView` / `ItemView` / `BoardView`, not a serde model | The Python app writes the same file; boxes and items this version does not know must survive a save, keys keep their order and odd shapes (`"state": "bad"`) are repaired the way `_load` repairs them; the sample home saves back byte for byte | 6 |
| D40 | The page changes the store only through acts applied at the end of a pass (`Act`), and the app does what the page asks through `dashboard::Out` (run, stop, terminal, settings, event) | Drawing borrows the store; menus and dialogs of an immediate UI would otherwise edit what they are drawing from | 6 |
| D41 | A drag that leaves its screen carries on: the window it began in keeps the mouse while the button is down, so the desktop position (Hyprland's cursor, else that window's pointer plus its place on the desktop) says which screen a box or row is over and the drop goes there | There is one window per screen and nothing can be dragged out of a window; Python hands over to Qt's drag and drop for the same reason | 6 |
| D42 | Running items are a process table the app looks at every 1.5 s while any is left; the row's green dot reads that table, and groups queue their next item with a due time that `poll` wakes for | No polling when nothing runs; Python does the same with a timer | 6 |
| D43 | Each tool is a `UiModule` in `lum-ui/src/tools/<tool>/` and draws into any rect: the panel, the overlay page and a dashboard box call the same `View::show`. Its logic that needs no drawing is in `lum-calc` or `lum-core` with fixtures | One drawing function serves three places, so the panel, the page and the box can not drift apart, and the logic is tested without a window | 8 |
| D44 | Commands reach a module through an `Inbox<Cmd>` (actions, hotkeys, threads) and what it wants from the app goes back as `Request`s through `Services`; `UiModule` declares its hotkeys (`binds`), page, palette rows, tray entries and status lines | Actions are `Fn + Send + Sync` closures and cannot reach a module's state; the app already owns hotkeys, the tray, usage and events, so a module asks instead of holding them | 8 |
| D45 | Calculator numbers are Python's: whole numbers on `num-bigint`, `/` always a decimal, floats checked to the last digit against the fixtures. JSON keeps 64 bits, so a bigger whole number in a data file is the nearest decimal | The history files Python wrote and the fixtures use these rules; `arbitrary_precision` in serde_json would change every number in the workspace | 8 |
| D46 | HTTP uses `ureq` with rustls and the webpki roots; `CDLA-Permissive-2.0` (the roots list) is allowed in `deny.toml` | Currency rates are the first network call; checking the server's certificate needs a list of authorities | 8 |
| D47 | The Reminders module sleeps until the moment the next reminder is due (at most 60 s, to notice a changed clock), instead of looking every 10 s as Python does | "Nothing polls": a tool with no window open must not wake the app six times a minute | 8 |
| D48 | A module never writes Settings itself: it sends a `Request` (`ReminderEdit`, `ApplyTheme`) and the app applies it, storing the reminders list with `save_quiet` so the Settings page reloads it | Settings is owned by the app; a second writer would put an old list back on the next Save | 8 |
| D49 | `fancy-regex` (pure Rust, MIT) reads the quick-reminder patterns, which use lookahead; every other pattern in the workspace stays on `regex` | The patterns are the Python spec and the `regex` crate has no lookahead | 8 |
| D50 | Notes keeps `open` as an index and sticky places as a list of four numbers when it writes, and keeps the shape it found (an object with a screen name, an id) when the file already had that | Python 1.9.21 reads only the index and the list; going back must not break the notes file | 8 |
| D51 | The QR Code tool uses the `qrcode` crate (no default features) and writes its own PNG and SVG from the modules; a real decoder (`rqrr`, dev dependency only) reads the picture back in the tests | `segno` has no Rust twin and its bytes can not be matched; what matters is that a phone reads it, which a decoder in the tests proves | 8 |
| D52 | Snippets sends a card to the Trash and shows a video's first frame by running the desktop's own tools (`gio trash`, `trash-put`, `kioclient5`; PowerShell's recycle call on Windows; optional `ffmpeg`), through `lum_platform::trash` and `lum_platform::frame`, always off the UI thread | A Trash crate pulls in a lot for one call and the tools know each disk's Trash folder; `ffmpeg` is already the optional video helper in CRATES.md | 8 |
| D53 | Today, Usage and Calendar are pages, not modules: `lum_ui::tools::overview::Overview` is held by the app and called from the page closure by key, with `Usage`, the Reminders list and the path of `atlas_sync.json` in an `Env`; they read the log when the page comes up and never poll | They have no hotkey, panel or action of their own, so a `UiModule` would be empty; the same call draws the overlay page and a dashboard box | 8 |
| D54 | Snippets keeps the meta file in memory, saves it through a background `Writer` and hands each listing job a copy; listing, search, thumbnails, copying, pasting, saving the clipboard and the Trash run on threads and answer through the module's `Inbox`; pasting is `Request::SendKeys` with Python's delays (220 ms, Enter at 340 ms, reopen at 450 ms) | Nothing may wait for the disk, the clipboard or another program on the UI thread; the delays are what lets focus go back to the window that is pasted into | 8 |
| D55 | Screenshots come from the wlr-screencopy protocol through `wayland-client` (already in the tree for winit), `grim` when that fails, and GDI on Windows; no `xcap` | Two monitors in 22 ms on headless sway with no new native libraries; `grim` is what Python used, so nothing that worked stops working | 9 |
| D56 | The doodle keeps marks, not pictures: each screen's canvas is painted with tiny-skia at the monitor's real scale, a frame uploads only the changed rectangle, and undo paints the marks again over a base that holds the ones older than the history | Python kept up to 30 copies of each screen (about 1 GB at 4K); a frame costs the same with 500 strokes as with none | 9 |
| D57 | Text grab hands Tesseract the picture on stdin and uses `Windows.Media.Ocr` through the `windows` crate | No temporary file left behind; no Python winrt packages to install on Windows | 9 |
| D58 | A module can own a hold key and fixed keys (`UiModule::hold_setting`, `hold_event`, `fixed_binds`); the doodle's hold rules are a state machine in `lum-screen` that returns effects | The rules came from real bugs and are tested without windows or timers; the launcher only routes `HoldDown`, `HoldUp` and `Interrupted` | 9 |
| D59 | The colour dialog borrows the screen colour picker through the context's memory (`eyedropper::ask`, `take_answer`, `available`) | The dialog has several owners (Themes, Settings, Snippets); none of them needs wiring | 9 |
