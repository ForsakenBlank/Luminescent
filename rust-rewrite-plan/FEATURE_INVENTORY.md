# Feature inventory (the parity checklist)

Everything Luminescent 1.9.21 does, grouped by area, with the phase that rebuilds it and the Python file that is
the spec. Tick a box when the Rust app does it as well as the Python app. Phase 0 checks this list against the
code and adds anything missing; phase 15 goes through it line by line before 2.0 ships.

Sizes for scale (counted in phase 0): about 55 000 lines of Python, 22 modules, 27 settings sections with 269
stored settings (273 fields counting the button-only ones), 65 automation node types, 30+ data files.

Phase 0 checked every line below against the code, README.md and CHANGELOG.md (1.5 to 1.9.21) and added the rows
marked *(added in phase 0)*. The behaviour behind many rows is pinned by a fixture in `luminescent-rs/fixtures/`
(see its README).

## Base app (phases 1, 3, 15)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | One process, one keyboard hook, everything a hotkey away | 1, 3 | `Launcher.py` |
| [ ] | Data folder rules: `~/.local/share/Luminescent`, `%LOCALAPPDATA%\Luminescent`, `LUMINESCENT_HOME` override | 1 | `core/__init__.py` |
| [ ] | Atomic JSON writes; unreadable files kept as `.bad` | 1 | `core/__init__.py` |
| [ ] | Rotating log in `data/launcher.log`, crash log | 1 | `Launcher.py` |
| [ ] | Single instance | 1 | `Launcher.py` |
| [ ] | Module registry; a module that fails to load or start does not stop the rest (toast lists failures) | 1 | `modules/__init__.py`, `core/module.py` |
| [ ] | A module with 6 errors in a minute is switched off for the session; Tray → Modules turns it back on | 1 | `core/guard.py` |
| [ ] | Safe mode after two starts that did not get going; "Restart in safe mode"; "Safe mode: restart normally" | 1 | `core/guard.py` |
| [ ] | Restart always restarts, even when a part is broken | 1 | `Launcher.py` |
| [ ] | Event bus (started, overlay opened / closed, theme changed, dashboard switched, item ran, timer finished, reminder fired, over the limit, clipboard changed, keep awake changed) | 1 | `core/events.py` |
| [ ] | Usage stats (`usage.json`) and today's activity log (`activity.jsonl`) | 1 | `core/usage.py` |
| [ ] | Export / import backup zip; weekly automatic backup keeping the last 5; vault never included | 1, 7 | `core/backup.py` |
| [ ] | Old files are migrated on load: settings (nav bar, panel placement, animations, tray items, show on start), themes.json (first run from the old Appearance section, pre-redesign built-ins, theme pack), dashboard layouts 1 and 2, automation defaults *(added in phase 0)* | 1, 4, 6, 10 | `core/settings.py`, `core/themes.py`, `ui/dashboard.py`, `core/automation.py` |
| [ ] | First start of a downloaded build copies settings and data from the source folder it was built from *(added in phase 0)* | 15 | `core/__init__.py` (`_first_frozen_run`) |
| [ ] | `--smoke` self-test; `--strip-metadata <paths>` | 1, 13 | `Launcher.py` |
| [ ] | Tray icon: the sparkle in the accent colour; click opens the overlay; menu editable (Settings → General → Tray menu) | 3, 7 | `Launcher.py` |
| [x] | Tray tooltip shows running timers (ticks only while a timer runs) | 3, 8 | `Launcher.py` |
| [ ] | Run at login (autostart entry / Run key) | 3 | `core/autostart*.py` |
| [ ] | Keep awake (tray, palette, hotkey) | 3 | `core/keepawake.py` |
| [ ] | Reset stuck keys (tray; also automatic) | 3 | `core/hotkeys*.py` |
| [ ] | Quit everything hotkey with confirm | 3, 7 | `Launcher.py` |
| [ ] | Update check once a day; Tray → Update now (download + swap + restart, or `git pull` from source); "what is new" after restart | 15 | `core/updates.py` |
| [ ] | Launcher entry and icon on Linux; Dolphin service menu; Explorer context menu | 3, 13 | `core/desktop*.py` |
| [ ] | Sounds for timers and reminders: none, chime, alarm, a custom file (pw-play / paplay, else Qt; Windows system sounds and winsound) *(added in phase 0)* | 3 | `core/sounds.py` |

## Hotkeys and windows (phase 3)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Hyprland binds added at runtime through LUMI Lua, callbacks through a FIFO; nothing written to the user's config | 3 | `core/hypr.py`, `core/hotkeys.py` |
| [ ] | Binds and window rules come back after a Hyprland config reload (also two reloads in a row) | 3 | `core/hypr.py` |
| [ ] | Windows global hook; stuck-key sanitising; 100 ms watchdog for hold keys | 3 | `core/hotkeys_win.py` |
| [ ] | Hold keys (press + release binds, non-consuming), with a watchdog asking whether the key is still down | 3 | `core/hotkeys.py` |
| [ ] | Duplicate combos refused; clashes with your own Hyprland binds detected | 3 | `core/hotkeys.py` |
| [ ] | Key names: aliases, keysyms, media keys, numpad, mouse buttons, raw key codes | 3 | `core/hotkeys.py` |
| [ ] | Window placement on Hyprland through the socket (invisible title markers, rules for no-focus, no-activate, pin, blur, glass) | 3 | `core/hypr.py` |
| [ ] | Windows open on the right monitor even when Hyprland holds a new window for most of a second | 3 | `core/hypr.py` |
| [ ] | Panels open in the middle of the screen with the mouse, or where you left them | 3, 5 | `core/settings.py` (`ui.panel_place`) |
| [ ] | Floating panels: frameless, on top, translucent, drag by header, edge resize, edit look | 2 | `core/widgets.py` |
| [ ] | Clipboard that works from a hotkey: wl-copy / wl-paste on Wayland (no focus needed), Qt's clipboard as the fallback *(added in phase 0)* | 3 | `core/clip.py` |
| [ ] | App icons from the desktop's icon theme (qt6ct, KDE or GNOME theme read by hand); show a file in the file manager *(added in phase 0)* | 3 | `core/desktop.py` |

## Widget kit (phase 2) *(section added in phase 0)*

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Every dialog and message box in Luminescent's look (vault password, keybind editor, item editor, calibration, variables, date picker) | 2 | `core/widgets.py` |
| [ ] | Disabled buttons, fields and check boxes look disabled; long names end in "…" | 2 | `core/widgets.py`, `core/theme.py` |
| [ ] | Sliders jump to where you click; the wheel scrolls the page and only changes a spin box, combo box or slider after it was clicked; spin box text has room from the border | 2 | `core/ux.py` |

## Overlay and navigation (phase 5)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Full-screen overlay on every monitor: dim, dot grid, optional background image, glow; painted once per screen *(each layer a texture made once, D33)* | 5 | `ui/overlay.py` |
| [x] | Page switcher hanging from the top centre with sliding marker; order and extra pages set in Settings | 5 | `ui/overlay.py` |
| [ ] | Pages: Dashboard, Automation, Themes, Clipboard, Settings; extra: Notes, Timers, Calculator, Today, Usage, Calendar, Hearing Guard, System Monitor, Media, System Settings, Performance *(phase 5: all in the switcher, Themes built, the rest placeholders until their phases)* | 5 | `core/settings.py` |
| [x] | Esc / ✕ / click on the dimmed area hides; the Dashboard never closes it on click | 5, 6 | `ui/overlay.py` |
| [x] | `Ctrl+1…9` switch pages; a jump hotkey per page | 5 | `ui/overlay.py` |
| [x] | Opens on the screen with the mouse (or primary); optionally on the last page | 5 | `ui/overlay.py` |
| [x] | Frosted background (Hyprland blur / Windows acrylic) *(Hyprland blur; Windows has no acrylic in Python either, see D38)* | 5 | `ui/overlay.py` |
| [x] | Text size 85 % to 150 % everywhere | 2, 5 | `core/theme.py` |
| [x] | Performance mode drops glow, animations, background images | 2, 5 | `core/anim.py` |
| [x] | Type on the Dashboard or Themes page to open the palette with what you typed | 5 | `ui/overlay.py` |
| [x] | Chrome layer: logo, sparkle, title, clock, time, date, status chips, running timers; edit mode to drag, add, freeze, duplicate, resize, move to screen, remove, reset | 5 | `ui/chrome.py` |
| [ ] | Status chips: running timers, Hearing Guard level, next reminder *(phase 5: the chips are drawn from `ChromeData`; running timers are in since phase 8, the rest come with phases 8 (reminder), 11, 12)* | 5 | `ui/chrome.py` |
| [x] | Pages built lazily; prewarm after start; repair (rebuild) when a page breaks | 5 | `ui/overlay.py` |
| [ ] | Command palette: actions, scripts, notes, themes, settings sections, colours, pins, reminders; sums; quick reminders; recent entries *(phase 5: pages, actions, themes, settings, Luminescent entries, recent; sums and quick reminders phase 8, notes 8, colours 9, pins 8, automations 10; phase 6: dashboard items are in)* | 5 | `ui/palette.py` |
| [x] | Hotkey cheat sheet while held | 5 | `ui/cheatsheet.py` |
| [x] | Luminescent notification cards: position, width, accent style, duration, max on screen, opacity, text size, icon, hover keeps, ✕, right-click copies, grows to fit *(right-click copies at once and says so on the card: a menu can not leave a card's small window, D37)* | 5 | `ui/toast.py` |
| [x] | Desktop notifications option; reminders and dose warnings always go to the desktop too | 5 | `ui/toast.py` |
| [x] | Warning on screen (centre, above everything, until Got it) | 5 | `ui/alert.py` |

## Animations (phases 2, 5)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Overlay fade, page fade, sliding marker, box glide, theme crossfade, notification and panel fades, automation zoom | 2, 5 | `core/anim.py` |
| [ ] | Master switch, speed (Fast, Normal, Relaxed), a switch per animation | 2, 7 | `core/anim.py` |
| [ ] | Styles: Caelestia (overshoot, rise), Smooth, Snappy | 2 | `core/anim.py` |
| [ ] | Hover effects light up card and box edges | 2 | `core/theme.py` |

## Dashboard (phase 6)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Tab boxes of items on every screen; header with accent bar, mono caps title, count, fold | 6 | `ui/dashboard.py` |
| [x] | Items: script, program, file, folder, URL, command, Luminescent action; badge and real icon *(phase 6: SVG icons are not drawn, those rows keep their badge)* | 6 | `ui/dashboard.py` |
| [x] | Item options: arguments, hotkey, working folder, run as administrator, ask for arguments, run in a terminal window | 6 | `ui/dashboard.py` |
| [x] | Groups run several items in order; headings and separators | 6 | `ui/dashboard.py` |
| [x] | Running items show a green dot and Stop | 6 | `ui/dashboard.py` |
| [x] | Folder items: Open terminal here | 6 | `ui/dashboard.py` |
| [x] | Drag a header to move (snap grid, default 8 px, Off = pixel moves), click to fold, drag right edge to resize | 6 | `ui/dashboard.py` |
| [x] | Tidy up: columns / grid / rows, by position / name / size, only the selection, glide, Undo / Ctrl+Z | 6 | `ui/dashboard.py` |
| [x] | Page boxes (any page as a resizable box that remembers its state) *(phase 6: built; only Themes is a real page so far, and a page box saves its state once a page has some to give)* | 6 | `ui/dashboard.py` |
| [x] | Picture boxes (picture or GIF; frame, fill, corners, opacity, title bar) *(phase 6: a GIF plays while its box is on screen; animated WebP shows its first frame)* | 6 | `ui/dashboard.py` |
| [x] | Selection rectangle, Ctrl / Shift click, multi-drag, Delete, Ctrl+A, menus for the selection | 6 | `ui/dashboard.py` |
| [x] | Several dashboards; new = empty / with Tools / copy; instant switching; toolbar folds into a menu | 6 | `ui/dashboard.py` |
| [x] | A different dashboard per screen ("This screen shows") | 6 | `ui/dashboard.py` |
| [x] | Drag items between boxes, files in from the file manager, boxes onto another screen | 6 | `ui/dashboard.py` |
| [x] | Lock button, search box that highlights | 6 | `ui/dashboard.py` |
| [x] | Item hotkeys work anywhere; every item in the tray menu and the palette | 6 | `ui/dashboard.py` |
| [x] | Tools tab of Luminescent's own actions *(phase 6: made from the actions that are registered, so it fills up as modules arrive)* | 6 | `ui/dashboard.py` |
| [ ] | Scripts open in your terminal (kitty, alacritty, foot get the theme colours) and stay open | 3, 6 | `core/runner.py`, `core/terminal.py` |

## Themes (phases 4, 14)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Click a theme to apply it everywhere, live | 4 | `ui/themes_page.py` |
| [x] | Built-in Dark and Light (can not be deleted) plus the built-in pack | 4 | `core/themes.py` |
| [x] | Folders with colours, collapse, drag themes into folders | 4 | `ui/themes_page.py` |
| [x] | Theme editor: three resizable columns, every colour role, font, radius, glow, background image + opacity, card icon, wallpaper, settings profile | 4 | `ui/themes_page.py` |
| [x] | Looks per module, for notifications and for on-screen warnings; Copy the look of…; Match all its colours to it | 4 | `ui/themes_page.py` |
| [ ] | Colour picker dialog: SV square, hue strip, old / new, hex / rgb / hsl / names, eyedropper (the screen pick comes with phase 9), readability, recent and theme colours | 4, 9 | `core/colourpick.py` |
| [x] | Gradients for any colour: stops, Linear / Radial / Sweep, angle, presets, Ideas, ◐ shortcut | 4 | `core/colourpick.py` |
| [x] | Quick tools: Main colours, Harmony, From a picture, Surprise me, Adjust all, Follow the accent, Add variance, Light ⇄ dark, with before / after | 4 | `ui/colour_tools.py`, `core/tones.py` |
| [x] | Undo / redo of any theme change; Recently changed with put-back | 4 | `ui/colour_tools.py` |
| [x] | Import / export `.lumtheme`; From a colour… | 4 | `core/themes.py` |
| [x] | Next / previous theme hotkeys (round the folder); a key per theme; Back to the theme before | 4 | `core/themes.py` |
| [x] | A theme switches its settings profile | 4, 7 | `core/themes.py` |
| [ ] | Desktop wallpaper per theme (picture, GIF, video, Wallpaper Engine project); red line when the file is gone (the box and the red line are built; applying it is phase 14) | 4, 14 | `core/wallpaper.py` |
| [ ] | Caelestia colours per theme, grouped and named, searchable, favourites, hover / click preview | 14 | `ui/colour_tools.py` |
| [ ] | Caelestia look per theme (transparency, fonts, sizes, pictures, logo, lock screen, desktop clock) | 14 | `modules/caelestia_look.py` |
| [ ] | Caelestia extras: picture instead of lyrics, picture or GIF behind the lock screen | 14 | `modules/caelestia_look.py` |
| [ ] | Glass per theme: app transparency, which Luminescent windows are glass, blur strength | 14 | `ui/theme_extras.py` |
| [ ] | Window borders per theme (active colour or gradient, inactive colour) | 14 | `ui/theme_extras.py` |
| [ ] | Terminal per theme: colours, kitty opacity, fastfetch picture and position, start-up text, info lines, narrow rules | 14 | `core/terminal.py` |

## Settings (phase 7)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Every option grouped by module, generated from the schema | 7 | `settings_ui.py` |
| [x] | Search that highlights and counts matches per section; "Only matching settings" | 7 | `settings_ui.py` |
| [x] | Settings profiles (own hotkeys, sizes, sounds); General, Overlay, Animations, Dashboard and Automation shared | 1, 7 | `core/settings.py` |
| [x] | Hotkey recorder with live clash warning | 7 | `settings_ui.py` |
| [x] | Pin sections with ☆ | 7 | `settings_ui.py` |
| [x] | Module pages have buttons for its actions, or Turn it on | 7 | `settings_ui.py` |
| [x] | Show on start, tray menu editor, notifications settings and "Show a notification" test | 7 | `settings_ui.py` |
| [x] | Reset page, Undo changes, Save & Apply; sections built lazily | 7 | `settings_ui.py` |

## Clipboard / Snippets (phase 8)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Snippets folder as cards (folders, images, videos, links, text, files); click to paste into the app you were using *(phase 8: videos need `ffmpeg`; the grid is drawn into the pop-up, the page and a box)* | 8 | `modules/snippets.py` |
| [x] | Ctrl+B pop-up grid; Ctrl+click pastes + Enter and reopens | 8 | `modules/snippets.py` |
| [x] | ＋ Save clipboard here (text, image, files) *(the button says "Paste here", as the Python grid does)* | 8 | `modules/snippets.py` |
| [x] | Search across folders; folder colours; favourites row; drag to reorder or into folders; hover preview *(a card can not be dragged out to another app, egui has no outgoing drag)* | 8 | `modules/snippets.py` |
| [x] | Thumbnails on a background thread (images, video frames, link previews) *(pictures and videos; link previews were never shown by the Python grid either)* | 8 | `modules/snippets.py` |
| [x] | Placeholders in text snippets (`{date}`, `{time}`, `{datetime}`, `{weekday}`, `{clipboard}`) | 8 | `modules/snippets.py` |

## Notes (phase 8)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Notes and to-do lists in a floating panel; show / hide and edit mode hotkeys | 8 | `modules/notes.py` |
| [x] | Markdown with 👁 preview | 8 | `modules/notes.py` |
| [x] | Colours and tags; search by name, content and `#tag` | 8 | `modules/notes.py` |
| [x] | Drag notes and to-do items to reorder | 8 | `modules/notes.py` |
| [x] | Due dates on to-do items become reminders | 8 | `modules/notes.py` |
| [x] | Export .md / .txt | 8 | `modules/notes.py` |
| [x] | Stick on the screen (several sticky notes) | 8 | `modules/notes.py` |
| [x] | Pin notes; the first pinned note opens on show (setting) | 8 | `modules/notes.py` |
| [x] | Undo for 12 s after deleting (asks first from a sticky with the panel closed) | 8 | `modules/notes.py` |
| [x] | Notes page and dashboard box | 8 | `modules/notes.py` |

## Timers (phase 8)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | As many timer / stopwatch panels as you like; monotonic timing | 8 | `modules/timer.py` |
| [x] | Type `5m`, `1h30`, `25m focus`, `at 17:30`, empty = stopwatch; ⏲ types a new time | 8 | `core/timeparse.py` |
| [x] | Restored still running after a restart or reboot; a timer that ran out shows DONE | 8 | `modules/timer.py` |
| [x] | Progress ring or bar; DONE flash and sound | 8 | `modules/timer.py` |
| [x] | Stopwatch laps; lap hotkey laps the one started last | 8 | `modules/timer.py` |
| [x] | Timers page and box; time logged to Today | 8 | `modules/timer.py` |

## Calculator (phase 8)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Safe evaluator with limits; `×`, `÷`, `^`, `%` rules, `1,000`, `mod`, functions, constants, degrees / radians | 8 | `modules/calculator.py` |
| [x] | `ans` and sums starting with an operator; ↑ / ↓ history | 8 | `modules/calculator.py` |
| [x] | Pins with names usable in sums; pin folders and colours | 8 | `modules/calculator.py` |
| [x] | History grouped by day; keypad toggle; copy; Ctrl+Enter calculates, copies, closes | 8 | `modules/calculator.py` |
| [x] | Conversions (`5 km to mi`, `20 c to f`, `2.5 gb in mb`, `3 st to kg`), currency (daily rates) | 8 | `core/convert.py` |
| [x] | Dates (`today + 45 days`, `days until 25/12/2026`, `2026-12-25 - today`) | 8 | `core/convert.py` |
| [x] | DEC / HEX / BIN / OCT display; `0x1f`, `&`, `|`, `xor`, `<<`, `>>` | 8 | `modules/calculator.py` |
| [x] | Calculator page and box; sums in the palette | 8 | `modules/calculator.py` |

## Reminders (phase 8)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Once, daily, weekly, monthly (31st = last day), every N days, interval (optionally between hours), at startup | 8 | `modules/reminders.py` |
| [x] | Reminder pop-up: Snooze 5 min to 1 h, Done, ✕, running-out line, rises into place | 8 | `modules/reminders.py` |
| [x] | Sounds, desktop notification, Switch theme action | 8 | `modules/reminders.py` |
| [x] | Quick reminders from the palette (`remind me in 20m to stretch`, `at 17:30 call mum`) | 5, 8 | `ui/palette.py` |
| [x] | Reminders editor in Settings (the slot is there, phase 7); runtime state kept apart from settings | 7, 8 | `modules/reminders.py` |

## Other everyday pages (phase 8)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | QR code from the clipboard; editable; copy picture; save PNG / SVG | 8 | `modules/qr.py` |
| [x] | Today: what you did today, earlier days with ‹ › | 8 | `ui/today.py` |
| [x] | Usage: most used items, hotkeys and tools; uses per day | 8 | `ui/stats.py` |
| [x] | Calendar page: Atlas events and tasks (from the stored sync) next to reminders *(phase 8; the Sync with Atlas button opens its Settings until phase 13)* | 8 | `ui/calendar_page.py` |
| [ ] | Calendar: Atlas sync from the page, and notifications on the PC for events about to start and tasks due | 13 | `modules/atlas_sync.py` |

## Screen tools (phase 9)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [x] | Doodle: hold Alt to draw (left) / erase (right); solo delay; other hotkeys cancel it; "Start drawing with" a chosen tool or the last one | 9 | `modules/doodle.py` |
| [x] | Doodle tools: pen, highlighter, line, rectangle, circle, arrow, text, fill; Shift = straight line; wheel = size | 9 | `modules/doodle.py` |
| [x] | Doodle: clear, undo (shared across screens), blackboard, copy screen + drawing, panic, Esc | 9 | `modules/doodle.py` |
| [x] | Colour picker: frozen screens, 11 × 11 loupe, formats, arrow nudge, recent 10, to theme accent or new theme | 9 | `modules/colorpicker.py` |
| [x] | Text grab: drag a box, OCR (Windows OCR / Tesseract), optional edit box | 9 | `modules/textgrab.py` |
| [x] | Screen ruler: drag to measure, Shift level, click copies W × H, Ctrl+click distance, arrows nudge | 9 | `modules/ruler.py` |
| [x] | Window snapper: halves, thirds, two thirds, corners, cycling; undo back to tiling / previous spot | 9 | `modules/snapper.py` |

## Automation (phase 10)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Endless workspace: pan (right / middle drag, Space + drag), smooth wheel zoom | 10 | `ui/automation.py` |
| [ ] | Round flow ports, diamond value ports, ◆ pins on fields | 10 | `ui/automation.py` |
| [ ] | Pulses run along wires as glowing dots with a peek at what they carry | 10 | `ui/automation.py` |
| [ ] | Add nodes by right-click, double-click, or dropping a wire on open space (filtered) | 10 | `ui/automation.py` |
| [ ] | 13 triggers, 30 actions, 10 logic nodes, 11 values, the note (65 types) | 10 | `core/automation.py` |
| [ ] | Tokens: `{input}`, `{var:name}`, `{date}`, `{time}`, `{now}`, `{clipboard}`, `{newline}` | 10 | `core/automation.py` |
| [ ] | Runs in the background, overlay open or not | 10 | `core/automation.py` |
| [ ] | Workspaces with on / off switches; export / import `.lumflow` | 10 | `core/automation.py` |
| [ ] | Undo / redo, copy / paste / duplicate, Delete, F to fit | 10 | `ui/automation.py` |
| [ ] | Variables kept between restarts | 10 | `core/automation.py` |
| [ ] | Pause everything (Settings, tray, hotkey); safety stop for wire loops | 10 | `core/automation.py` |
| [ ] | Buttons in the palette and the tray | 10 | `core/automation.py` |
| [ ] | System readings and controls the nodes use: volume, lock / sleep / screen off, temperatures, GPU load, the focused window, internet check (slow parts on a worker thread) *(added in phase 0)* | 10 | `core/system.py` |

## Prompt Log and Keybinds (phase 13)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | History from fish, bash, zsh, PowerShell and Luminescent's own launches, with times | 13 | `modules/promptlog.py` |
| [ ] | Search, hide repeats, copy, run again in a terminal, delete lines, clear | 13 | `modules/promptlog.py` |
| [ ] | Pin commands; Save to Dashboard | 13 | `modules/promptlog.py` |
| [ ] | Every Hyprland bind with what it really does, next to Luminescent's hotkeys; clashes in red | 13 | `core/hyprbinds.py` |
| [ ] | Edit keys and action, remove, add; backups; undo on config error | 13 | `core/hyprbinds.py` |
| [ ] | My keybinds: keys or mouse buttons (up to Extra button 12, raw key codes), open app / command / tool / media / keys, "works while other keys are held" | 13 | `modules/keybinds.py` |

## Ghost Vault and Metadata (phase 13)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Encrypted vault with real and decoy passwords, recovery key, Forgot password? | 13 | `modules/vault_store.py` |
| [ ] | File grid, breadcrumb, drop zone, progress, search names, thumbnails in memory only | 13 | `modules/vault.py` |
| [ ] | Never runs elevated; clear message when the folder is not writable | 13 | `modules/vault.py` |
| [ ] | Metadata viewer / stripper for images and videos; folders to copies or in place after a confirm | 13 | `modules/metadata.py` |
| [ ] | Right-click "Strip metadata" in Dolphin / Explorer | 13 | `modules/metadata.py` |

## Hearing Guard and Media (phase 12)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Estimated dB(A) at the ear; calibration per device; headset's own volume | 12 | `modules/hearing.py` |
| [ ] | Limit mode (dip and recover), hard cap up to 150 %, record mode | 12 | `modules/hearing.py` |
| [ ] | NIOSH daily dose; DAY / WEEK / APPS views; per-app levels and volume sliders | 12 | `modules/hearing.py` |
| [ ] | Panel, page, dashboard box, status chip, over-the-limit event | 12 | `modules/hearing.py` |
| [ ] | Media controls for any MPRIS player / Windows media session: title, artist, cover, progress, switch player | 12 | `modules/media.py` |
| [ ] | Optional play / next / previous hotkeys; page and box | 12 | `modules/media.py` |

## System (phase 11)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | System Monitor panel: CPU, RAM, GPU, network with a minute of history | 11 | `modules/sysmon.py` |
| [ ] | Performance Overview widgets, all editable, 1 to 4 columns | 11 | `ui/performance_ui.py` |
| [ ] | Processes: icons, CPU, memory, GPU, VRAM, disk, threads, priority, user; grouped, searchable, sortable; end, force quit, priority, cores, rules, game mode | 11 | `ui/performance_ui.py` |
| [ ] | Boost: power mode, governor, EPP, boost, top speed, swappiness, cache pressure, THP, NVIDIA power, AMD level, Hyprland effects, free memory, clean temp (Windows equivalents) | 11 | `core/perftweaks.py` |
| [ ] | Game mode (switch or key, auto when a game runs, puts everything back) | 11 | `modules/performance.py` |
| [ ] | Alerts & rules; Startup apps; gaming HUD | 11 | `modules/performance.py` |
| [ ] | GPUs: NVIDIA, AMD and Intel on Linux; any GPU on Windows; per-app GPU and VRAM | 11 | `core/perfstats.py` |
| [ ] | System Settings: displays (drag, resolution, refresh, scale, rotation, main, VRR, mirror, off, Identify, 15 s revert, Copy as config) | 11 | `core/syscontrol.py` |
| [ ] | Brightness (DDC/CI, laptop backlight) and night light | 11 | `core/syscontrol.py` |
| [ ] | Mouse, touchpad, keyboard; windows & desktop (Hyprland); Explorer & taskbar (Windows); appearance; power; sound; This computer | 11 | `core/syscontrol.py` |
| [ ] | Power & sleep: login screen, screen off and sleep times (Caelestia idle, hypridle, GNOME), lock on sleep, stay awake while playing or plugged in; Windows hibernate and password after sleep *(added in phase 0)* | 11 | `core/syscontrol.py` |
| [ ] | Keep my changes after a Hyprland reload; Back to my Hyprland config | 11 | `modules/syscontrol.py` |
| [ ] | Workspace sets: per-monitor sets, slide the way you pressed, named workspaces in the round, wrap modes, Ctrl+Super+1…9, move keys, scroll, persistent, Caelestia bar, first monitor | 11 | `core/workspaces.py` |
| [ ] | Workspace sets, 1.9.x details: at the end of a set go round / carry on / stop; switching looks like slide the way you pressed / fade / none / Hyprland's own; Up and Down slide vertically; turning sets on leaves windows on their screen (renumbered); monitors plugged in or out re-work the sets; old settings carried over *(added in phase 0)* | 11 | `core/workspaces.py` |

## Sync and desktop (phases 13, 14)

| Done | Feature | Phase | Spec |
|---|---|---|---|
| [ ] | Atlas Sync over Wi-Fi: notes both ways, timer time adds up, themes, calendar events and tasks, skipped dates; encrypted with the code | 13 | `core/atlas_sync.py` |
| [ ] | Caelestia & Glass module: shell config as forms or text with backups, scheme colours, glass per app, Add the app I click next, Lock the screen | 14 | `modules/caelestia.py` |
| [ ] | Caelestia re-themes its apps (terminals, btop, GTK / Qt apps, Discord, Spotify) when a theme gives it colours; saved scheme colours come back when Caelestia rewrites its scheme *(added in phase 0)* | 14 | `modules/caelestia.py` |
| [ ] | Wallpaper backends: Caelestia, Noctalia, swww, hyprpaper, swaybg, mpvpaper, linux-wallpaperengine, Windows; custom command | 14 | `core/wallpaper*.py` |
| [ ] | Your own terminal follows the theme (kitty include line, Windows Terminal profiles, Caelestia fish sequences) | 14 | `core/terminal.py` |
| [ ] | Start-up text in fish and the fastfetch info box per theme | 14 | `core/terminal.py` |
