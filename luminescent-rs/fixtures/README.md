# Golden fixtures

Made by `luminescent-rs/tools/make_fixtures.py` from the Python app's own code (do not edit by hand; run it again when the Python reference changes). Run it twice and nothing changes: the clock is 2026-11-23T14:30:00 (a Monday) in UTC, uuids, salts, nonces and recovery keys come from fixed seeds, and scratch paths are written as `<SCRATCH>`, `<HOME>` or `<LUMINESCENT_HOME>`.

Numbers that must survive exactly (calculator values, units) are written as `{"int": "..."}` or `{"float": "repr"}`. Byte strings are lower-case hex. A case that raises in Python records `{"error": type, "message": text}` instead of `ok`.

| File | Made by | Pins down |
|---|---|---|
| `atlas_cases.json` | core/atlas_sync.py with fixed nonces, uuids and clock (now_ms = 1795444200000) | keys, sealed frames byte for byte, fingerprints Atlas must agree with, repeat rules, and a full hello / records / apply conversation through Host.answer |
| `automation_compare_cases.json` | core/automation.py compare, _days, _hm, _fmt | the If node's comparisons, schedule day lists, clock text and how numbers print |
| `automation_expand_cases.json` | core/automation.py AutomationEngine.expand with a fixed clock and clipboard | {input}, {var:name}, {date}, {time}, {now}, {newline}, {clipboard}; other braces left alone |
| `automation_load_cases.json` | core/automation.py AutomationEngine._load | automation.json as it loads: defaults, dropped nodes and wires, the first-start example |
| `automation_nodes.json` | core/automation.py TYPES (every @node line), CATEGORIES, _OPS | every node type with its ports, fields, defaults, static choices (or the name of the function that lists them at run time), help and width |
| `backup_cases.json` | core/backup.py export and check on sample_home and on hand-made zips | which files a backup holds and which zips an import refuses |
| `calc_cases.json` | modules/calculator.py evaluate, fmt, pin_name, CalculatorModule.compute/plain; trig in degrees and radians | sum -> value or the exact error message; result text in every base, decimals and thousands setting; continuing from ans, pins as variables, conversions and date sums through the panel |
| `convert_cases.json` | core/convert.py convert, date_math, parse_date with fixed rates and today = 2026-11-23 | unit, temperature and currency conversions with their unit label; date sums and day counts; errors |
| `dashboard_migration_cases.json` | ui/dashboard.py DashboardStore._load with two stand-in modules | dashboard.json layout 1 and 2 migrations, defaults filled in, first-start Tools tab |
| `events.json` | core/events.py NAMES | the event names the bus carries and what the Automation page calls them |
| `gradient_cases.json` | core/theme.py gradients, generate, accent_roles, clean_section, section_palette; core/themes.py colours_from_roles | gradient cleaning and sampling, the Qt brush text, generated themes from one accent, per-module looks |
| `hotkey_cases.json` | core/hotkeys.py normalize, pretty (Linux and Windows names), keysym, to_hyprland, validate; core/hyprbinds.py key_label, mods_of | how a typed combo is stored, shown, bound in Hyprland and checked |
| `hyprbinds_cases/cases.json` | core/hyprbinds.py tokenize, parse_file, load, edit, change_keys, delete, add with Hyprland stubbed | Lua config parsing, binds matched to their hl.bind line, readable actions, and the exact text every edit writes (or the error it gives) |
| `hyprbinds_cases/runtime_binds.json` | hand-written: what Hyprland reports for the sample config | the input load() combines with the files |
| `keys.json` | core/hotkeys.py and core/hyprbinds.py key tables | key name aliases, xkb keysyms, modifier masks, mouse buttons, Hyprland dispatcher labels |
| `pages.json` | core/settings.py page and tray tables, core/anim.py choices | overlay pages, which can be in the nav bar, panels that can open on start, tray menu entries |
| `promptlog_cases/cases.json` | modules/promptlog.py kind_of, read_entries, entry_time, write_entries on the sample files next to it | fish, bash, zsh, PowerShell and command-log parsing (multi-line entries, timestamps) and what deleting rows writes back |
| `reminder_cases.json` | modules/reminders.py next_due, describe, parse_quick, new_reminder | the next time every schedule type fires after a given moment (None = never); schedule text; quick reminders typed in the palette |
| `sample_home/` | core/settings.py, core/themes.py, ui/dashboard.py, core/automation.py, modules/notes.py, core/atlas_sync.py, core/backup.py, rest by hand in the shapes the modules read | one realistic example of every data file; each must load and save back unchanged. Paths in it say <SCRATCH>/sample_home where the folder itself is meant |
| `settings_defaults.json` | core/settings.py defaults() on each system | what settings.json holds on a first start (widget fields are never stored) |
| `settings_migration_cases.json` | core/settings.py Settings.load (_merge + _migrate) on each saved file | old or odd settings.json files and the settings they load as |
| `settings_profile_cases.json` | core/settings.py profiles: create, switch, rename, delete, a damaged profiles.json | every file on disk after each step, and which sections follow the profile |
| `settings_schema.linux.json` | core/settings.py SCHEMA, reloaded with core.IS_WIN = False | every section and field on linux: key, label, type, default, help, choices, limits, restart flag |
| `settings_schema.windows.json` | core/settings.py SCHEMA, reloaded with core.IS_WIN = True | every section and field on windows: key, label, type, default, help, choices, limits, restart flag |
| `terminal_cases/cases.json` | core/terminal.py scheme, colour_args, write_kitty_theme, sequences, greeting_script, _fastfetch_config, set_fastfetch_logo, command, and the Windows Terminal fragment (module reloaded as Windows) | the exact text Luminescent writes into kitty, fish, fastfetch and Windows Terminal for a theme |
| `theme_presets.json` | core/theme.py, core/themes.py, core/tones.py tables | the built-in Dark and Light colours, the pre-redesign colours they migrate from, colour roles, the 'More themes' pack with the colours it generates |
| `themes_migration_cases.json` | core/themes.py ThemeStore._load (first run, old Appearance, pre-redesign colours, theme pack) | themes.json as it loads and as it is written back |
| `timeparse_cases.json` | core/timeparse.py parse_duration, parse_clock, fmt_secs | typed durations to seconds (None when not a duration), clock times to the next due moment, seconds to short text |
| `tones_cases.json` | core/tones.py (OKLCH maths, tones, harmonies, whole-set changes, Caelestia schemes) | colour maths to the last hex digit |
| `vault_cases.json` | modules/vault_store.py with seeded randomness (salts, keys, nonces and the recovery key are fixed) | the Rust vault must open these folders with these passwords and the recovery key, decrypt every file to these bytes, and migrate the two old layouts |
| `vault_fixture/` | modules/vault_store.py (see vault_cases.json) | real vault with a decoy, a no-AAD-index vault and a pre-rewrite .hash vault |
