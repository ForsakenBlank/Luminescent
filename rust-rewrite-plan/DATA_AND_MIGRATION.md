# Data and migration

The Rust app must read every file the Python app writes and write the same formats back, so a user can move to
2.0 and, if something goes wrong, back to 1.9.21 without losing anything. This file lists every data file, the
rules that keep them compatible, the crypto parameters that must match exactly, and how the move to 2.0 works.

## Where data lives

| Situation | Settings | Data folder |
|---|---|---|
| Downloaded build (Linux) | `~/.local/share/Luminescent/settings.json` | `~/.local/share/Luminescent/data/` |
| Downloaded build (Windows) | `%LOCALAPPDATA%\Luminescent\settings.json` | `%LOCALAPPDATA%\Luminescent\data\` |
| Running from source (Python) | `<repo>/settings.json` | `<repo>/data/` |
| `LUMINESCENT_HOME` set | `$LUMINESCENT_HOME/settings.json` | `$LUMINESCENT_HOME/data/` |
| **Rust dev builds** | a separate dev folder unless `LUMINESCENT_HOME` is set | so development never touches real data |

Note that `settings.json` sits one level above `data/`.

## Compatibility rules (until phase 15, and ideally for good)

1. **Same file names, same JSON shapes.** Every serde model mirrors the Python dicts.
2. **Keep what you do not understand.** Every model has a `#[serde(flatten)] extra` map so fields from a newer or
   older version survive a load and save. The settings store keeps unknown sections and keys.
3. **Atomic writes**: `<file>.tmp`, fsync, rename. Never truncate in place.
4. **Unreadable file → `<file>.bad`**, then defaults. Never overwrite a file that failed to parse.
5. **Debounce saves** (about 500 ms) for files that change often (automation state, dashboard while dragging).
6. **New Rust-only data goes in new keys or new files**, never by changing the meaning of an old key.
7. **Python drops unknown settings keys when it saves.** That is fine during the transition: Rust-only settings
   are lost if the user goes back to 1.9.21, nothing else is.
8. **Tests**: every file in `luminescent-rs/fixtures/sample_home/` must load and save back unchanged (after
   normalising whitespace and key order where the Python side does not sort).

## Every data file

`H` = the home folder (holds `settings.json`), `D` = `H/data`.

| File | Owner (phase) | What is in it | Notes |
|---|---|---|---|
| `H/settings.json` | lum-core (1) | the active profile's settings, by section | about 265 fields in 27 sections; schema from the phase 0 dump |
| `D/profiles.json` | lum-core (1) | `{"active", "profiles": [{"id", "name"}]}` | `default` always exists |
| `D/profiles/<id>.json` | lum-core (1) | a profile's own sections | shared sections: general, ui, animations, dashboard, automation |
| `D/launcher.log`, `.1`, `.2` | lum-core (1) | log | rotating 512 KB |
| `D/crash.log` | lum-core (1) | crash tracebacks | |
| `D/starting.json` | lum-core (1) | start guard state | safe mode after two failed starts |
| `D/usage.json` | lum-core (1) | `{"items", "hotkeys", "tools": {key: {"label", "n", "last"}}, "days": {date: n}}` | |
| `D/activity.jsonl` | lum-core (1) | one line per action `{"t", "kind", "label", "secs"?}` | append only |
| `D/backups/*.zip` | lum-core (1) | backups | weekly automatic, keep 5; layout below |
| `D/command_log.txt` | lum-platform (3) | everything the dashboard launched | read by Prompt Log |
| `D/desktop_win.json` | lum-platform (3) | Windows launcher entry state | |
| `D/luminescent.png`, `.ico` | lum-platform (3) | rendered app icon | regenerated, not migrated |
| `D/themes.json` | lum-theme (4) | `{"active", "folders": [...], "themes": [...]}` | see shape below |
| `D/theme-images/` | lum-theme (4) | copies of theme pictures | |
| `D/picker.json` | lum-theme (4) | recent colour picks in the colour dialog | |
| `D/palette.json` | lum-ui (5) | `{"recent": [...]}` palette entries | |
| `D/ui/` | (none) | generated arrow and check PNGs for the Qt stylesheet | Qt-only cache; the Rust app paints these, nothing to migrate |
| `D/dashboard.json` | lum-ui (6) | boards, tabs, items (layout 3) | layout 1 and 2 are migrated once |
| `D/notes.json` | notes (8) | `{"notes": [{"id", "name", "type", "content", "items", ...}], "x", "y", "w", "h", "open"}` | to-do items `{"id", "text", "checked", ...}` |
| `D/timers.json` | timer (8) | list of open panel states, running ones with their wall-clock start | git-ignored in the Python repo |
| `D/calculator.json` | lum-calc (8) | history and pins (folders, colours) | |
| `D/currency.json` | lum-calc (8) | cached daily rates | from api.frankfurter.app / .dev |
| `D/reminders_state.json` | reminders (8) | last fired, snoozed until | definitions live in `settings.json` → `reminders.items` |
| `<snippets folder>/.luminescent.json` | snippets (8) | order, favourites, folder colours | lives in the user's snippets folder |
| `D/colours.json` | colour picker (9) | `{"recent": [...]}` last 10 picked colours | |
| `D/automation.json` | lum-automation (10) | `{"version": 1, "active", "vars", "workspaces": [{"id", "name", "enabled", "view", "nodes", "wires"}]}` | nodes `{"id", "type", "x", "y", "title", "off", "fields", "state"}`, wires `{"id", "src": [node, port], "dst": [node, port]}` |
| `D/sysmon.json` | sysmon (11) | panel geometry | |
| `D/performance.json` | lum-perf (11) | widgets, alerts, rules, game mode lists | |
| `D/hearing.json` | lum-audio (12) | calibrations per device, today's levels, week history | |
| `D/media.json` | media (12) | panel geometry | |
| `D/atlas_sync.json` | lum-atlas (13) | the shadow store (fields of every record as last synced) and calendar data | keeps fingerprints stable |
| `D/promptlog.json` | promptlog (13) | `{"pins": [...]}` | |
| `D/hypr-backups/` | lum-hypr (13) | backups of edited Hyprland config files | |
| `D/caelestia-backups/` | caelestia (14) | backups of Caelestia files | |
| `D/terminal-backups/` | terminal (14) | backups of kitty.conf, Windows Terminal files, fish files | |

### Files outside the data folder that Luminescent writes

All of these are backed up before the first write and put back when no longer needed (phase 14 keeps that
behaviour exactly):

* `~/.config/autostart/luminescent.desktop`, Windows Run key (autostart)
* Dolphin service menu, Explorer context menu entries (metadata stripper)
* `~/.config/kitty/kitty.conf` include line, Windows Terminal fragment
* Caelestia's fish sequences file, `~/.config/fish/conf.d/luminescent-greeting.fish`,
  `~/.config/fastfetch/luminescent.jsonc`
* `~/.config/caelestia/*`, `~/.local/state/caelestia/scheme.json`, `~/.config/quickshell/caelestia/` (extras),
  `~/.local/state/luminescent/caelestia.json`
* Hyprland config files (only through the Keybinds editor, with backups and undo on error)
* the Ghost Vault folder (the user picks it; never in backups)

### themes.json shape

```
{"active": id,
 "folders": [{"id", "name", "colour", "collapsed"}],
 "themes":  [{"id", "name", "folder", "builtin", "colours": {role: "#hex"}, "gradients": {role: spec},
              "font_family", "radius", "glow", "icon", "background_image", "image_opacity", "wallpaper",
              "profile", "modules": {key: accent or {accent, colours, gradients, radius}},
              "toasts": {...}, "caelestia": {"colours": {...}, "shell": {...}, "extras": {...}}, ...}]}
```

A gradient spec is `{"type": "linear" | "radial" | "conical", "angle": degrees, "stops": [[0..1, "#RRGGBB"], ...]}`.

### dashboard.json shape

```
{"layout": 3, "active": board id, "locked": false, "screens": {screen name: board id},
 "boards": [{"id", "name",
             "tabs": [{"id", "kind", "title", "x", "y", "w", "h", "collapsed", "screen", "page", "state",
                       "items": [{"id", "kind", "name", "path", "args", "hotkey", "action",
                                  "cwd", "admin", "ask_args", "terminal", "group"}]}]}]}
```

### Backup zip layout

The same entries as `core/backup.py`: `settings.json`, `data/notes.json`, `data/timers.json`,
`data/reminders_state.json`, `data/hearing.json`, `data/dashboard.json`, `data/themes.json`,
`data/calculator.json`, `data/colours.json`, `data/palette.json`, `data/profiles.json`, `data/automation.json`,
and `data/profiles/*.json`. Import refuses a zip without `settings.json`, zips the current files into
`data/backups/` first, then restarts. 2.0 may add files to the list (usage, Atlas, performance) but must still
import 1.9.x backups.

## Crypto parameters that must match exactly

### Ghost Vault (`modules/vault_store.py`)

| Item | Value |
|---|---|
| Files in the vault folder | `.salt`, `.master`, `.dmaster` (optional), `.recovery`, `.meta`, `.dmeta`, blob files named by 32 hex chars |
| Key derivation | PBKDF2-HMAC-SHA256, **480 000** rounds, 32-byte key, salt from `.salt` |
| Cipher | AES-256-GCM, a random **12-byte nonce in front** of ciphertext + tag |
| AAD for `.master` | `b"master"` (wraps the file key with the password key) |
| AAD for `.dmaster` | `b"dmaster"` (wraps the decoy file key) |
| AAD for `.recovery` | `b"recovery"` (wraps the file key with the recovery key) |
| AAD for `.meta` / `.dmeta` | `b"index"`; **pre-rewrite indexes used no AAD**, so try with, then without |
| AAD for blobs | none; each file is sealed whole |
| Recovery key text | random hex, upper case, groups of 4 joined by `-` |
| Recovery key → key | SHA-256(salt + the key with only letters and digits, upper case) |
| Index (decrypted) | JSON `{"tree": {name: "<blob hex>" or {subfolder}}, "perm": {"a/b/name": "<decrypted copy path>"}}` |
| Writes | temp file with a random suffix + fsync + rename |
| Moving files in | secure delete of the original (3 overwrite passes) after the index is saved |

Real and decoy vaults are completely independent. Rust uses `aes-gcm`, `pbkdf2`, `sha2` and `zeroize`.

Later (phase 16) new vaults may use Argon2id and chunked encryption for big files, but **existing vaults must
always open**, and the format version must be recorded so the Python app is never handed a vault it can not read
without a warning.

### Atlas Sync (`core/atlas_sync.py`)

| Item | Value |
|---|---|
| Transport | TCP, Luminescent only hosts, port **47830**, one connection per request |
| Frame | `b"ATLS"` + 1 type byte + 4-byte big-endian length + body; max 512 MiB |
| Types | `0` = sealed JSON, `1` = plain error message |
| Key | PBKDF2-HMAC-SHA256(code, salt `b"atlas-sync-1"`, **120 000** rounds, 32 bytes) |
| Seal | AES-GCM, 12-byte nonce first, tag last, no AAD |
| Code | 6 random digits |
| Protocol | version 1; accepts kinds `item`, `log`, `event`, `task`, `theme` and items `NOTE`, `TRACKER:TIMER` |
| Fingerprints | ignore `updated` and `created`; the shadow store sends fields back unchanged until the source changes |

Port `tests/test_atlas_sync.py` and test against the phase 0 frames. The Atlas app (`ForsakenBlank/Atlas`,
`sync/SyncWire.kt`) is the other side and must keep working without changes.

## Moving users to 2.0 (phase 15)

1. **Same place, same files.** 2.0 uses the same data folder as the 1.9.x downloads.
2. **Safety backup first.** Before its first write, 2.0 zips the whole data folder and `settings.json` into
   `data/backups/before-2.0-<date>.zip` (everything, not only the usual backup list; never the vault).
3. **Load check.** Every file is loaded; anything that fails is reported in a "some things could not be read"
   card, and kept as `.bad` rather than replaced.
4. **Source-run users** (Python from a git checkout keep their data in the repo folder): the first run offers to
   copy `settings.json` and `data/` from a folder they pick, like `_first_frozen_run` does.
5. **Updating itself.** The 1.9.x updater downloads `Luminescent-windows.zip` / `Luminescent-linux.tar.gz` from
   the latest release, swaps the program folder and starts `Luminescent.exe` / `Luminescent/Luminescent`. The 2.0
   release keeps those names and paths, so 1.9.x installs update to 2.0 with one click.
6. **Going back.** Install 1.9.21 again; the data still works (rule 7 above), or restore the safety backup.

## During development

* Always run the Rust app with `LUMINESCENT_HOME` pointing at a copy (`cp -r ~/.local/share/Luminescent
  /tmp/lumhome`). Both apps can not share one data folder at the same time: they would fight over hotkeys and
  overwrite each other's files.
* Fixtures for tests live in `luminescent-rs/fixtures/`; tests must never read or write outside a temp folder.
