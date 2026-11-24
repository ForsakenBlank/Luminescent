# Luminescent in Rust: the rewrite plan

This folder is the whole plan for rebuilding Luminescent as a native Rust app. The Python + PyQt6 version stays
in the repo as the reference until the Rust one does everything it does, then it retires.

Nothing in here is code yet. It is the map: what to build, in which order, with which model and effort level,
and the exact prompt to start each phase with.

## Why rewrite

* **Speed.** The Python app starts a full interpreter plus Qt, numpy, cryptography, Pillow and OpenCV, spawns
  helper programs (`nvidia-smi`, `pactl`, `playerctl`, `grim`) for things a native library can answer in
  microseconds, and shares one GIL between the UI and every background job. A Rust build is one binary that
  starts in a fraction of the time, idles at 0 % CPU and uses a fraction of the memory.
* **Reliability.** Most "wrapped C/C++ object has been deleted" style errors, stuck keys and half-built overlays
  come from the Python/Qt object lifetime dance. Rust's ownership model and a cleaner thread design remove whole
  classes of those bugs.
* **Room for deeper features.** Real-time audio, process-level GPU stats, a fast fuzzy palette, native OCR and a
  proper automation engine are all cheaper to do well in Rust (see [NEW_FEATURES.md](NEW_FEATURES.md)).

## Reading order

| File | What it is |
|---|---|
| [PHASES.md](PHASES.md) | **The main document.** Every phase with its share of the work, progress %, model, effort level, checkpoints, definition of done and the prompt to paste |
| [PROGRESS.md](PROGRESS.md) | The tracker. Tick checkpoints here as phases get done; every session reads and updates it |
| [ARCHITECTURE.md](ARCHITECTURE.md) | The target design: UI framework choice, crates, threads, module system, theming, decision log |
| [FEATURE_INVENTORY.md](FEATURE_INVENTORY.md) | Every feature of the current app as a parity checklist, mapped to the phase that rebuilds it |
| [DATA_AND_MIGRATION.md](DATA_AND_MIGRATION.md) | Every data file, its format, the crypto parameters that must match, and how users move over safely |
| [CRATES.md](CRATES.md) | Which Rust crate replaces which Python library or external tool |
| [PLATFORM_MATRIX.md](PLATFORM_MATRIX.md) | How each feature works on Hyprland, other Linux desktops and Windows |
| [PERFORMANCE.md](PERFORMANCE.md) | Performance budgets, how to measure them, and the rules that keep the app fast |
| [TESTING.md](TESTING.md) | Golden fixtures from the Python app, unit/UI tests, smoke test, CI and manual checklists |
| [RISKS.md](RISKS.md) | What can go wrong and what to do about it |
| [NEW_FEATURES.md](NEW_FEATURES.md) | Beyond-parity ideas as a tick list (like IDEAS.md), for after the rewrite reaches parity |
| [CLAUDE-for-rust-project.md](CLAUDE-for-rust-project.md) | Rules for every coding session. Phase 1 copies it to `luminescent-rs/CLAUDE.md` |

Two more files appear here as the work goes: `BASELINE.md` (the Python app's measured numbers, made in phase 0)
and `SPIKE_RESULTS.md` (the UI framework test results, made in phase 2).

## The phases at a glance

"Share" is how much of the whole rewrite that phase is (by size and difficulty). "Progress when done" is where
the overall bar sits once that phase is finished. Sessions are rough counts of focused Claude Code sessions.

| # | Phase | Share | Progress when done | Model | Effort | Sessions |
|---|---|---|---|---|---|---|
| 0 | Groundwork: baseline numbers + golden fixtures from the Python app | 2 % | **2 %** | Sonnet 5.5 | medium | 1 to 2 |
| 1 | Cargo workspace + core infrastructure (paths, settings, profiles, logging, guard, CLI) | 5 % | **7 %** | Opus 5.5 | high | 2 to 3 |
| 2 | UI foundation spike + widget kit (framework decision gate) | 7 % | **14 %** | Opus 5.5 | xhigh | 3 to 4 |
| 3 | Platform layer: Hyprland, hotkeys, windows, tray, clipboard, launching | 9 % | **23 %** | Opus 5.5 | xhigh | 4 to 6 |
| 4 | Themes: engine, Themes page, theme editor, colour picker dialog | 8 % | **31 %** | Sonnet 5.5 | high | 3 to 5 |
| 5 | Overlay shell, command palette, cheat sheet, notifications | 6 % | **37 %** | Opus 5.5 | high | 3 to 4 |
| 6 | Dashboard | 6 % | **43 %** | Sonnet 5.5 | high | 3 to 4 |
| 7 | Settings page, profiles, backups | 4 % | **47 %** | Sonnet 5.5 | medium | 2 to 3 |
| 8 | Everyday tools: Notes, Timers, Calculator, Reminders, Clipboard, QR, Today, Usage, Calendar | 10 % | **57 %** | Sonnet 5.5 | high | 5 to 7 |
| 9 | Screen tools: Doodle, Colour picker, Text grab, Ruler, Snapper | 5 % | **62 %** | Opus 5.5 | high | 3 to 4 |
| 10 | Automation engine + node editor | 9 % | **71 %** | Opus 5.5 | xhigh | 4 to 6 |
| 11 | System Monitor, Performance, System Settings, Workspace sets | 10 % | **81 %** | Opus 5.5 then Sonnet 5.5 | high | 5 to 7 |
| 12 | Hearing Guard, Media controls, sounds, speech | 5 % | **86 %** | Opus 5.5 | high | 3 to 4 |
| 13 | Ghost Vault, Metadata Stripper, Atlas Sync, Prompt Log, Keybinds | 6 % | **92 %** | Opus 5.5 | xhigh | 3 to 4 |
| 14 | Desktop integration: Caelestia, Glass, borders, terminal, wallpapers | 4 % | **96 %** | Sonnet 5.5 | high | 2 to 3 |
| 15 | Migration, packaging, updater, parity audit, release 2.0 | 4 % | **100 %** | Opus 5.5 | high | 2 to 4 |
| 16 | Beyond parity: new in-depth features (its own track) | own track | own 0 to 100 % | Sonnet 5.5 / Opus 5.5 | medium to high | ongoing |

Roughly 50 to 70 sessions in total for phases 0 to 15.

### Milestones

| Progress | Milestone | What you can do with it |
|---|---|---|
| 23 % | **Hello tray** | The Rust app sits in the tray, a hotkey opens an empty overlay on every screen |
| 37 % | **Looks right** | Overlay, page switcher, palette, cheat sheet and notifications look like Luminescent, in any theme |
| 57 % | **Daily-drivable alpha** | Dashboard, Settings, Themes, Notes, Timers, Calculator, Reminders and Clipboard all work |
| 96 % | **Feature-complete beta** | Everything the Python app does, the Rust app does |
| 100 % | **2.0 release** | Packaged downloads, one-click update from 1.9.x, Python version retired |

## How to run a phase

1. Open a Claude Code session on this repo.
2. Set the session's model and effort level to what the phase table says.
3. Copy the phase's prompt from [PHASES.md](PHASES.md) and paste it as the first message.
4. The session reads the plan, works through the phase's checkpoints in order, commits after each one and
   updates [PROGRESS.md](PROGRESS.md).
5. If a session runs out of room it stops at a checkpoint and writes a handoff note in PROGRESS.md. Start the
   next session with the same prompt; it picks up from the note.
6. When the phase's "done when" list is all true, review the PR (the `/code-review` command helps), merge, and
   move on.

**When a phase struggles**: if a checkpoint fails twice, raise the effort one step (medium → high → xhigh). If it
still fails on Sonnet, switch that checkpoint to Opus. Save `max` for a single stubborn problem, not a whole phase.

**Models**: the tiers matter more than the names. "Opus" rows need the strongest model you have (architecture,
OS hooks, crypto, concurrency, anything where a subtle mistake costs data or stability). "Sonnet" rows are bulk
porting with clear reference code and tests. Haiku 5.5 is listed for small mechanical helper jobs inside phases
(transcribing tables, writing fixture lists, changelog text). Other models can be swapped in at the same tier.

## Rules for every session

These are repeated in [CLAUDE-for-rust-project.md](CLAUDE-for-rust-project.md), which every prompt points at.

* **The Python app is the spec.** When the plan and the Python code disagree about behaviour, the Python code
  wins unless the phase says the behaviour changes on purpose.
* **Parity first, new features later.** No new features before phase 16 unless a phase lists them.
* **Never break user data.** Read the existing data files, write the same formats back, keep unknown fields.
  Develop against a copied data folder (`LUMINESCENT_HOME=/tmp/lumhome`), never the real one.
* **Write code like a careful human Rust developer.** No banner comments, no comments that restate the code, no
  emoji in code, no filler names. Comments are welcome when they explain *why* (the Python code's plain, friendly
  comment style is the model). No em dashes anywhere: code, comments, docs, commit messages.
* **User-facing text keeps the current voice**: short, plain, friendly British English ("colour", "favourite"),
  no jargon, the same labels the Python app uses.
* **Every behaviour change gets a Settings toggle**, and feature ideas are offered as questions, not built unasked
  (the rule the Python project already follows).
* **Performance is a feature.** Nothing polls while hidden, nothing blocks the UI thread, and every phase checks
  its numbers against [PERFORMANCE.md](PERFORMANCE.md).

## Where the code goes

The Rust project lives in `luminescent-rs/` at the root of this repo (a Cargo workspace), next to the Python
app, so every session can read the reference code and the golden-fixture generator can import it. Phase 15
moves the release workflow over to the Rust build. Develop phases on branches off `main` and merge each one
when its "done when" list passes.
