# Risks

What could derail the rewrite, how likely it is, and what to do. Review this list at the start of phases 2, 3,
10, 13 and 15, and add new risks as they show up.

| # | Risk | Likelihood | Impact | What to do |
|---|---|---|---|---|
| R1 | **Wayland window placement and focus.** Wayland apps can not place themselves or avoid focus, and winit has no layer-shell support | High | High | Keep the proven Hyprland socket + title marker + rule approach (D3). The phase 2 spike checks it first. Layer-shell surfaces are a phase 16 upgrade, not a blocker |
| R2 | **File drops from the file manager on Wayland** may not reach a winit window | Medium | Low | Checked in the phase 2 spike. Fallback: an "Add files…" button, and a small native `wl_data_device` handler later |
| R3 | **Transparent windows on Windows** behave differently on `glow` and `wgpu` | Medium | Medium | The spike measures both; pick the one that works; keep the other behind a feature flag |
| R4 | **The Windows keyboard hook** gets removed by Windows if its callback is slow; unsafe code | Medium | High | The hook thread only forwards events; a watchdog re-installs the hook if events stop; the sanitising and stuck-key rules from `hotkeys_win.py` are ported as they are; hook code gets its own tests and review |
| R5 | **Vault incompatibility or data loss** | Low | Very high | Byte-level fixtures both ways, read-only first, never point tests at real vaults, Opus at xhigh for phase 13, a human review of every crypto line |
| R6 | **Hyprland changes** (the Lua config is new as of 0.55) | Medium | Medium | All Hyprland code sits in `lum-hypr`; the Lua stays identical to the tested Python version; the version is checked at start and parts that need a newer Hyprland switch off with a message |
| R7 | **Scope creep**: 55 000 lines is a lot, and new ideas are tempting | High | High | Parity first: no new features before phase 16 unless a phase lists them; FEATURE_INVENTORY.md is the contract |
| R8 | **The UI looks "generic egui"** instead of Luminescent | Medium | Medium | Phase 2 builds a full widget kit to the mock-ups before any page; snapshot reviews every phase; Geist fonts and letter spacing from day one |
| R9 | **Running both apps at once** fights over hotkeys and files | Medium | Medium | Single-instance lock per data folder; the Rust dev build defaults to its own folder; docs say to use `LUMINESCENT_HOME` copies |
| R10 | **Root tweaks** (pkexec) doing damage | Low | High | One small helper binary with a fixed allow-list of files and values, no shell, logged; every change has its undo recorded first |
| R11 | **System files outside the data folder** (kitty.conf, Caelestia, Hyprland config) broken by a bug | Low | High | Backup before the first write, restore on stop, tests on temp copies only, the same "undo on config error" as today |
| R12 | **GPU / driver problems** with the renderer (old GPUs, NVIDIA on Wayland) | Medium | Medium | Fall back from `wgpu` to `glow`; a `--software` start option if eframe's software path is usable; log the adapter at start |
| R13 | **AI-written code drifts** in style or quality across 60 sessions | High | Medium | `luminescent-rs/CLAUDE.md` rules, clippy as errors, small crates, fixtures, `/code-review` on every phase PR, the same prompts every time |
| R14 | **Long sessions run out of room** mid-phase | High | Low | Checkpoints are sized for one session each; every session writes a handoff note in PROGRESS.md before stopping |
| R15 | **Atlas app compatibility** | Low | Medium | Wire format fixtures; the Atlas side is not changed; test against a real phone before phase 13 is ticked |
| R16 | **Tray on Linux** needs a StatusNotifierItem host | Low | Low | Same requirement as today; the palette and `luminescent ctl` still work without a tray |
| R17 | **Crates going stale** (tray, battery, tts) | Medium | Low | Wrap each behind a small trait in `lum-platform`; swapping a backend touches one file |
| R18 | **Updating from 1.9.x fails** because the archive layout changed | Low | High | Phase 15 keeps archive names and executable paths and tests the real 1.9.21 updater against the 2.0 release before publishing |

## Kill criteria

Stop and rethink (and write it in the decision log) if:

* the phase 2 gate fails on both egui and iced;
* the overlay can not open within 100 ms on Hyprland after phase 5;
* phase 13 can not open the fixture vault byte-compatibly after two sessions at xhigh;
* the project passes 120 sessions before phase 15.
