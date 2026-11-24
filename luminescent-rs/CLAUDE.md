# Luminescent (Rust)

This is the Rust rewrite of Luminescent, a tray app for Windows 11 and Linux (made for Hyprland) that keeps small
tools a hotkey away. The Python + PyQt6 app at the repo root is the reference. The plan is in
`../rust-rewrite-plan/`: read its README.md, the current phase in PHASES.md, and PROGRESS.md at the start of every
session.

## Commands

Run these from `luminescent-rs/`. The toolchain is pinned in `rust-toolchain.toml`; rustup picks it up by itself.

```
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
LUMINESCENT_HOME=$(mktemp -d) cargo run -p luminescent -- --smoke    # starts headless, checks every part, exits 0
cargo run --release -p luminescent -- --bench-startup                 # prints the time from start to ready
LUMINESCENT_HOME=/tmp/lumhome cargo run -p luminescent                # runs until Ctrl+C or `ctl quit`
LUMINESCENT_HOME=/tmp/lumhome cargo run -p luminescent -- ctl ping    # talks to the running copy
cargo deny check                                                      # licences and advisories (CI runs it)
cargo run --release -p luminescent -- --idle-check 20                 # idle CPU, wake-ups, memory and painted frames
cargo run -p luminescent -- --spike list                              # the UI checks; --spike gallery shows every widget
UPDATE_SNAPSHOTS=1 cargo test -p lum-ui --test snapshots              # after a deliberate change to the look
```

To catch Windows-only lint errors before CI: `rustup target add x86_64-pc-windows-gnu` and
`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. To try windows in this
container: `xvfb-run -a cargo run -p luminescent -- --spike gallery` (it needs `libxkbcommon-x11-0`; snapshots
need `mesa-vulkan-drivers`).

The golden fixtures come from the Python app. From the repo root, with the `.venv` from the main README (it
needs Qt's system libraries, see `check.yml`):

```
.venv/bin/python luminescent-rs/tools/make_fixtures.py
```

## How to work

* **The Python code is the spec.** Read the Python file for whatever you are building before writing Rust. When
  the plan and the Python code disagree about behaviour, the Python code wins unless the phase says otherwise.
* **Parity first.** Do not add features the Python app does not have unless the current phase lists them. If an
  idea comes up, write it in `../rust-rewrite-plan/NEW_FEATURES.md` as an unticked line and carry on.
* **Never touch real user data.** Run with `LUMINESCENT_HOME` pointing at a temp copy. Tests only use temp
  folders and `fixtures/`.
* **Keep data compatible.** Same file names and JSON shapes as the Python app; every serde model keeps unknown
  fields (`#[serde(flatten)] extra`); atomic writes; unreadable files become `.bad`.
* **Work in checkpoints.** Commit after each checkpoint of the phase, then update `../rust-rewrite-plan/PROGRESS.md`
  (tick it, set the percentages, write a short handoff note). If you are running low on room, stop at a checkpoint
  boundary and write the handoff note first.
* **Ask before** deleting or moving the Python app, changing release workflows, touching files outside the repo,
  or anything that can not be undone.
* **No display here.** The cloud session can not see a desktop. When something needs a real Hyprland or Windows
  desktop, give the user exact commands and wait for their results.

## How the code should read

* Like a careful, experienced Rust developer wrote it by hand. Plain names, small functions, straightforward
  control flow, no clever tricks without a reason.
* Comments explain *why*, never restate *what*. The Python code's comments are a good model: plain, friendly,
  often a sentence about the bug or system quirk that made the code look the way it does. Do not delete that
  knowledge when porting; carry the useful comments over.
* No banner or divider comments, no "This function does X" doc comments on obvious functions, no emoji in code,
  no TODO lists in comments (put them in PROGRESS.md).
* **Never use em dashes** in code, comments, docs, commit messages or UI text. Use commas, full stops, colons or
  brackets instead.
* `unsafe` only where an OS API needs it, each block with a one-line `// SAFETY:` comment.
* Library crates return `thiserror` errors; the app uses `anyhow` with context. No `unwrap()` outside tests and
  truly impossible cases (and then `expect("why it can not fail")`).
* Platform code only inside `lum-platform`, `lum-hypr`, `lum-perf` and `lum-audio`, behind `cfg(target_os)`.
* Commit messages: short, plain, present tense, saying what changed for the app ("Dashboard: drag boxes between
  screens"), like the existing git history.

## User-facing text

* The same labels, wording and hotkey names as the Python app. Help texts come from the settings schema dump.
* Plain, friendly British English ("colour", "favourite", "centre"), short sentences, no jargon.
* Every behaviour change gets a Settings toggle.

## The UI (lum-ui)

* Build screens from the kit in `crates/lum-ui/src/widgets/`, `panel.rs` and `dialog.rs`; colours and sizes come
  from `Tokens::of(ctx)` every frame, never hard-coded. New widgets go into the gallery (`gallery.rs`) so the
  snapshots and `--smoke` cover them.
* Animate with `lum_ui::anim::value` / `value_from` (a kind from Settings → Animations) or `anim::small`; they
  repaint only while moving. Never ask for repaints on a timer to poll for changes.
* Long lists use `widgets::virtual_list`. Expensive pictures are cached (`paint.rs` shows how).
* egui centres rows inside the height it is given: a horizontal or right-to-left layout in an unbounded child
  works out NaN. Wrap such rows in `ui.horizontal` or give the child a finite height.

## Performance rules

* Nothing runs, polls or repaints for what is not on screen.
* The UI thread never blocks on disk, sockets, processes or the network.
* Paint expensive things once and cache them; virtualise long lists.
* Native APIs over spawned programs.
* Check the phase's budgets in `../rust-rewrite-plan/PERFORMANCE.md` before calling a checkpoint done.

## Map

```
crates/lum-core        paths, storage, settings + profiles, events, actions, usage, backups, guard, logging
crates/lum-theme       palette, gradients, tones, theme store
crates/lum-hypr        Hyprland sockets, LUMI Lua, binds, rules, hyprbinds, workspace sets
crates/lum-platform    hotkeys, windows, tray, notifications, clipboard, capture, runner, autostart, system
crates/lum-automation  graph model, engine, node types
crates/lum-calc        calculator, conversions, time parsing
crates/lum-vault       Ghost Vault storage and crypto
crates/lum-atlas       Atlas Sync protocol and host
crates/lum-perf        system sampling and tweaks
crates/lum-audio       Hearing Guard, media sessions, sounds, speech
crates/lum-ui          egui app: widget kit, overlay, pages, panels
crates/luminescent     the binary
crates/lum-fixtures    test-only helper that reads fixtures/ (int / float / hex / error outcomes)
fixtures/              golden data from the Python app
tools/                 fixture generator, measuring scripts
```
