# Testing

How the rewrite proves it behaves like the Python app. Most of the confidence comes from **golden fixtures**:
inputs run through the real Python code in phase 0, saved with their outputs, and replayed against the Rust code.

## Golden fixtures (phase 0)

`luminescent-rs/tools/make_fixtures.py` imports the Python modules and writes `luminescent-rs/fixtures/`. It must
be deterministic (fixed clock, fixed randomness) so running it twice gives no diff. Re-run it whenever the Python
reference changes during the rewrite.

| Fixture | Python source | Rust test |
|---|---|---|
| `settings_schema.{linux,windows}.json`, `settings_defaults.json` | `core/settings.py` | the Rust schema and defaults equal these |
| `settings_migration_cases.json` | `core/settings.py` (`_merge`, `_migrate`) | old settings → migrated settings |
| `automation_nodes.json` | `core/automation.py` | every node type exists with the same ports and fields |
| `events.json`, `pages.json`, `theme_presets.json`, `keys.json` | various | tables match |
| `calc_cases.json` | `modules/calculator.py` | sum → result or error, with pins and `ans` |
| `convert_cases.json` | `core/convert.py` (fixed rates, fixed today) | conversion → result |
| `timeparse_cases.json` | `core/timeparse.py` | text → duration / target time |
| `reminder_cases.json` | `modules/reminders.py` | schedule + now → next fire time |
| `hotkey_cases.json` | `core/hotkeys.py` | normalise, pretty, to-Hyprland, validate |
| `tones_cases.json`, `gradient_cases.json` | `core/tones.py`, `core/theme.py` | colour maths |
| `automation_compare_cases.json`, `automation_expand_cases.json` | `core/automation.py` | `compare()` and token expansion |
| `vault_fixture/`, `vault_cases.json` | `modules/vault_store.py` | Rust opens the Python vault and reads every file |
| `atlas_cases.json` | `core/atlas_sync.py` | frames byte for byte (fixed nonce), fingerprints |
| `promptlog_cases/` | `modules/promptlog.py` | history file → parsed rows; delete → rewritten file |
| `hyprbinds_cases/` | `core/hyprbinds.py` | config → binds and descriptions; edit → new config text |
| `terminal_cases/` | `core/terminal.py` | theme → kitty, fish, fastfetch, Windows Terminal output |
| `audio_cases/` (phase 12 adds) | `modules/hearing.py` | signal → A-weighted RMS within 1 dB |
| `sample_home/` | all | every data file loads and saves back unchanged |

Rust tests read fixtures through a small helper in a dev-dependency crate so every crate can use them.

## Test layers

1. **Unit tests** in every library crate (`cargo test`), including the fixture replays above.
2. **Property tests** (`proptest`) for parsers: the calculator, time parsing, hotkey strings, hyprbinds, token
   expansion. Nothing may panic or hang on any input.
3. **Engine traces**: automation graphs run against a fake clock and fake services; the test asserts the exact
   sequence of fired ports and texts.
4. **Lua tests**: the workspace-set Lua from `core/workspaces.py` runs under `mlua` against the mock Hyprland from
   `tests/test_workspaces.py`, ported to Rust.
5. **Cross-compatibility**: a Rust-made vault opened by the Python code, `.lumtheme` and `.lumflow` files moved
   both ways, backups made by one app imported by the other.
6. **UI snapshots** with `egui_kittest`: every page, panel and dialog in Dark, Light and one custom gradient theme.
   This replaces `tools/shots.py`. Snapshots are reviewed by a human when they change.
7. **Smoke test**: `luminescent --smoke` builds every page and pop-up once with a throw-away data folder and exits
   0, like the Python `--smoke`.
8. **Idle check**: `luminescent --idle-check 60` fails if anything repaints or polls while hidden.
9. **Benchmarks** (`criterion`): palette matching, calculator, automation pulses, the perf sampler's parsing.

## CI (`.github/workflows/rust.yml`, phase 1)

Runs on `ubuntu-22.04` and `windows-latest` for changes under `luminescent-rs/`:

* `cargo fmt --check`
* `cargo clippy --workspace --all-targets -- -D warnings`
* `cargo test --workspace`
* `luminescent --smoke` with a temp `LUMINESCENT_HOME`: it builds every widget of the kit on a plain
  `egui::Context`, so it needs no display
* the kittest snapshots (`crates/lum-ui/tests/snapshots.rs`), compared on Linux with Mesa's lavapipe
* `crates/luminescent/tests/instance.rs` a second time under Xvfb on Linux, so start-up, hand-over and quitting
  are tested inside eframe too
* `luminescent --idle-check 20` without a display, and on Linux once more under Xvfb with a window (memory is not
  held against llvmpipe, D24)
* `cargo deny check`

The Python `check.yml` keeps running until phase 15 retires it.

## Manual checks (the cloud session has no display)

Each phase that needs a real desktop ends with a short checklist the user runs on Hyprland and on Windows 11,
written into PROGRESS.md by the session, for example:

* Phase 3: tray icon appears in the accent colour; `Ctrl+Shift+Space` opens the dim on every monitor; Esc closes;
  holding Alt for two seconds then alt-tabbing away does not leave anything stuck.
* Phase 5: overlay opens on the monitor with the mouse; page switcher follows `Ctrl+1…9`; notifications stack in
  the chosen corner.
* Phase 9: colour picker, ruler and text grab on two monitors with different scales.
* Phase 11: display change reverts by itself after 15 seconds; game mode puts everything back after a forced quit.
* Phase 13: open a copy of a real vault; edit a copy of a real Hyprland config and see the backup.
* Phase 15: update a real 1.9.21 install to the 2.0 build on both systems.

The session waits for the user's results before ticking a checkpoint that depends on them.

## What never happens in tests

* No test reads or writes the real data folder, the real `~/.config`, a real vault or real shell history.
* No test needs the network (currency rates and update checks use recorded responses).
* No test depends on the current date or time without a fake clock.
