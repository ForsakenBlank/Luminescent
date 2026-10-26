# Luminescent: ideas checklist

Put an `x` in the box of each one you want (`- [x]`), then hand this file back and I'll build only the ticked ones.
Anything you don't tick is left alone. Ideas you already said no to (laser pen, Pomodoro/presets, clipboard history,
game mode, quiet hours, activation key, a built-in auto clicker) are not in here.

## Overlay & navigation
- [x] Type anywhere on the overlay to start searching (the command palette opens with what you typed)
- [ ] Remember the last page instead of always opening on Dashboard (as a setting)
- [x] Ctrl+1…4 switches overlay pages while it's open
- [x] Small clock + date in the overlay's top-left corner
- [x] Status strip in the overlay: running timers, Hearing Guard level, next reminder
- [x] Choose which screen the overlay opens on (primary / the one with the mouse)
- [ ] Overlay size setting (compact / normal / full screen)
- [x] Blur-style frosted background (Windows acrylic, off in performance mode)
- [x] Extra and editable (you can add or remove pages in the main section) pages in the nav bar you can switch on (e.g. Notes, Timers, Calculator as full pages)

## Dashboard
- [x] Drag an item from one tab box into another
- [x] Drag files from Explorer onto a tab box to add them as items (shrinks the overlay while dragging)
- [x] Item icons taken from the real program/file icon instead of emoji
- [ ] Item colour tags / a custom emoji per item
- [x] Per-item "run as administrator" option
- [x] Per-item working folder setting
- [x] "Ask for arguments" items (pops up a box each time you run it)
- [x] Separators and small headings inside a tab box
- [x] Multiple dashboards (e.g. Work / Gaming) switchable from the toolbar
- [x] Lock layout button (stops accidental dragging)
- [x] Search/filter box on the dashboard that highlights matching items
- [ ] Show a "last run" time and run count on each item (tooltip)
- [x] Mark an item as running and offer "Stop" for scripts started from it
- [x] Item groups: one item that runs multiple items in order

- [x] Tidy up starts below the clock and status chips (no more overlap with the reminder chip)
- [x] Clicking the dashboard never closes the overlay; drag on it for a selection box, like a desktop
- [x] Themes, Settings, Clipboard, Notes… as resizable boxes on the dashboard (they remember their page and spot)
- [x] New dashboards can start empty; switching dashboards is instant; the toolbar no longer overflows
- [x] Dashboard boxes on every screen (multi-monitor)
- [x] Snap-to-grid setting for moving boxes (off = smooth)

## Automation
- [x] Automation page in the nav bar: a big workspace of nodes wired together (Upload Labs style), with pulses along the wires
- [x] Triggers: every N seconds, time of day, date, hotkey, button, Luminescent events, programs, files, clipboard, idle, CPU/RAM/battery
- [x] Actions: commands (background or terminal), programs, files, web requests, notifications, ask me, type text, keys, sounds, speech, Luminescent tools and pages
- [x] Logic and values: wait, if, gate, count, once, cooldown, sequence, repeat, variables, join text, maths, date/time
- [x] Several workspaces with on/off switches, undo/redo, copy/paste, export/import

## Themes
- [x] Import / export a single theme as a file (to share themes)
- [x] Generate a theme from one accent colour (picks matching shades for you)
- [ ] Generate a theme from your current wallpaper's colours
- [ ] Follow Windows' light/dark mode automatically (pick a theme for each)
- [x] Per-module colour overrides inside a theme (e.g. Notes in a different accent)
- [ ] A few more built-in themes (e.g. Midnight Blue, Forest, Rose, High Contrast)
- [x] Theme switch hotkey (cycles through a folder of themes)
- [ ] Undo the last theme change

## Clipboard (Snippets)
- [x] Search box across all snippet folders
- [x] Drag cards to reorder them / move them into folders
- [x] Favourites row at the top of the root folder
- [ ] Number keys 1–9 paste the first nine cards while the grid is open
- [ ] Paste as plain text option for rich content
- [x] Preview a card on hover (bigger image / full text)
- [ ] Card size slider right in the page (instead of only in Settings)
- [ ] Text snippets with placeholders (e.g. `{date}`, `{clipboard}`) filled in when pasted

## Notes
- [x] Markdown-style formatting (bold, headings, lists) with a preview toggle
- [x] Note colours / tags
- [x] Drag to reorder notes and to-do items
- [x] To-do due dates that create a reminder
- [ ] Quick-add hotkey: type a line, it goes into a chosen note without opening the panel
- [ ] Undo after deleting a note (a few seconds to bring it back)
- [x] Export a note to a .txt / .md file
- [x] Multiple Notes panels on screen at once (pin a note as its own sticky)

## Screen Doodle
- [x] Text tool (click and type on screen)
- [x] Highlighter pen (wide, semi-transparent)
- [ ] Spotlight mode (dims everything except a circle around the cursor)
- [ ] Redo (alongside undo)
- [x] Fill option for rectangles/circles
- [ ] Choose the colour from the tool strip with the mouse (no colour hotkeys)
- [ ] Remember the last tool between sessions
- [ ] Save the drawing only (transparent PNG), without the screenshot

## Timer / Stopwatch
- [x] Type the time straight into the panel (e.g. "5m", "1h30") instead of a dialog
- [ ] +1 min / +5 min buttons on a running timer
- [ ] Export laps to the clipboard / CSV
- [ ] Compact mini mode (just the digits, no frame)
- [ ] Repeat a timer automatically when it finishes (loop)
- [ ] Pause/resume all timers hotkey
- [x] Progress ring/bar on countdown timers

## Calculator
- [x] Unit conversions (km↔mi, °C↔°F, kg↔lb, GB↔MB…)
- [x] Currency conversion (fetches rates once a day)
- [x] Date maths ("today + 45 days", days between two dates)
- [x] Hex / binary / decimal display toggle for programmers
- [x] Pin folders or colours to group pinned calculations
- [x] Show history grouped by day
- [x] Copy result as you type with Ctrl+Enter (calculate + copy + close) and a copy result button to clipboard

## Colour picker / Text grab
- [ ] Colour picker: copy in more formats (RGBA, CMYK, Qt/C# code)
- [ ] Colour picker: show a contrast check between two picked colours
- [x] Colour picker: save picked colours into a theme
- [x] Text grab: option to open the result in a small editable box before copying
- [ ] Text grab: grab from the last area again (repeat hotkey)
- [ ] Text grab: translate the grabbed text (needs internet)

## Prompt Log / Terminal
- [x] Pin favourite commands to the top
- [x] Save a command as a Dashboard item in one click
- [x] Show the date/time of each Luminescent launch in the list
- [x] "Open Terminal here" action for any dashboard folder item
- [x] Apply the Luminescent scheme to your default Terminal profile (optional, asks first)

## Reminders
- [x] Quick reminder from the palette ("remind me in 20m to …")
- [ ] Reminder actions: run a dashboard item, open a note, start a timer
- [x] Monthly / "every N days" schedules
- [ ] Upcoming reminders list in the overlay

## Hearing Guard
- [x] Weekly history chart (average level, dose per day)
- [ ] Tray icon tint when you're over the limit
- [x] Per-app volume view (which program is loudest)

## Vault / Metadata
- [x] Vault: search inside the vault
- [x] Vault: thumbnails for images (decrypted in memory only)
- [x] Metadata: strip a whole folder at once
- [x] Metadata: right-click "Strip metadata" in Explorer (adds a shell menu entry)

## Settings & general QoL
- [x] Hotkey conflict warning while you record (not only on Save)
- [ ] "Disable all hotkeys" toggle hotkey (pause Luminescent temporarily)
- [ ] First-run welcome tour of the overlay and main hotkeys
- [ ] What's-new popup after an update
- [x] Auto backup once a week (keeps the last 5)
- [ ] Settings sync folder (e.g. OneDrive) so two PCs share setup
- [x] Per-module "show on start" for every panel in one place
- [x] Tray menu editor (pick what appears in the tray menu)
- [x] Toast notifications in Luminescent style instead of Windows balloons
- [ ] Sound pack setting (one set of sounds used everywhere)
- [x] Usage stats page (most used items, hotkeys, tools)

## New tools
- [x] Window snapper: hotkeys to move/resize the active window into halves, thirds, corners
- [ ] Always-on-top toggle for any window (hotkey)
- [x] Quick screen ruler (measure pixels between two points)
- [ ] Mouse jiggler / keep-awake toggle (stops the PC sleeping)
- [x] Mini media controls panel (play/pause/next + current song)
- [ ] Quick folder jumper (search your favourite folders and open them)
- [ ] Text expander: type a short code (e.g. `;addr`) and it turns into saved text
- [ ] Clipboard text cleaner: remove formatting / fix spacing / change case via hotkey
- [x] QR code maker from the clipboard (shows a QR on screen) with a copy to clipboard as image and also save as button.
- [x] System monitor widget (CPU, RAM, GPU, network) as a floating panel
- [x] Focus/session log: what you worked on today from dashboard runs and timers


also there is a bit of weird bugs like when i went onto calculator then calculator settings then i clicked back onto the dashboard tab, the calculator was behind the dashboard overlay so maybe some way to make all the luminescent tabs come back up to the dashboard.