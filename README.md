# Luminescent (formerly UsefulScripts / AllinOne)

> [!WARNING]
> **This app is vibe coded.** It was built mostly by chatting with an AI coding assistant, not written line by
> line by an experienced developer. It works, but expect rough edges and bugs. Keep backups of your setup:
> **Settings → General → Backup → Export backup…** saves your settings, notes, timers, reminders, dashboard,
> automations and themes into one zip (Luminescent also makes a weekly one in `data/backups`). The Ghost Vault is
> never part of a backup, so keep your own copy of anything you put in there.

One background app for **Windows 11** and **Linux** (made for Hyprland, tested on CachyOS; see
[Which Linux?](#which-linux) for other distros and desktops) that keeps every little tool a hotkey away: a
GeForce-overlay style **hub** (Dashboard, Automation, Themes, Clipboard, Settings, and optional Notes, Timers,
Calculator, Today and Usage pages), plus
**Notes**, **Screen Doodle**, **Timers / Stopwatches**, **Calculator**, **Colour Picker**, **Screen Text Grab**,
**Prompt Log**, **Reminders**, **Hearing Guard**, **Ghost Vault**, the **Metadata Stripper**, a **Window Snapper**, a
**Screen Ruler**, **Media Controls**, a **QR Code** maker, a **System Monitor** and **Atlas Sync**. Everything runs in one process with
one keyboard hook.

## Download

The [Releases page](https://github.com/ForsakenBlank/Luminescent/releases/latest) has ready-made builds:

* **Windows**: `Luminescent-windows.zip`. Unzip it anywhere and run `Luminescent.exe`.
* **Linux**: `Luminescent-linux.tar.gz`. Unpack it and run `Luminescent/Luminescent` (Hyprland needed for hotkeys;
  the build needs glibc 2.35 or newer, which every distro from 2022 on has).

A download starts completely fresh. Your settings, notes, timers, themes and the rest live in your own data
folder, not next to the program: `~/.local/share/Luminescent` on Linux, `%LOCALAPPDATA%\Luminescent` on Windows.

**Updating**: Luminescent tells you when a new version is out. Tray → **Update now** downloads it, swaps it in and
restarts (also in the command palette: "Update Luminescent now"). Your data folder is not touched, so everything is
still there, and a short "what is new" message shows after the restart. If the program folder is read-only, the
release page opens instead and you replace the folder by hand.

## Run it from source

```
./start.sh           (Linux; first run makes .venv and installs requirements.txt)
Start.bat            (Windows)
```

From source, your setup lives in this folder (`settings.json` and `data/`), which git ignores: `git pull` updates
the code and leaves your notes and settings alone.

**Windows**: needs Python 3.11+ from python.org (tick "Add python.exe to PATH"). The first `Start.bat` sets
everything up. Hotkeys are a global keyboard hook, so nothing needs administrator rights.

**Linux**: run it inside a Hyprland session. Hotkeys are Hyprland binds that Luminescent adds while it runs (nothing
is written to your config), and Hyprland places its windows, so it needs Hyprland; under another compositor the
windows work but there are no global hotkeys. Optional system packages are listed at the end of requirements.txt
(Tesseract for Text Grab, ffmpeg for video metadata, playerctl for Media Controls).

Tray → **Update now** works here too: it runs `git pull` and installs the requirements, then restarts. By hand:
`git pull`, then `.venv/bin/pip install -r requirements.txt` (`.venv\Scripts\pip install -r requirements.txt` on
Windows).

A few things are Linux-only: the Keybinds panel can edit Hyprland binds (on Windows it lists Luminescent's hotkeys),
the frosted overlay background, and apps opening above the overlay. "Super" in hotkeys is the Windows key.

A **sparkle** appears in the tray in your theme's accent colour. **Click it** (or press **Ctrl+Shift+Space**) to open
the overlay. Right-click it for the dashboard items, quick actions, modules, Settings, Restart and Quit.
Settings → General → **Run at login** starts it when you sign in (an autostart entry on Linux, the Run key on Windows).

## Which Linux?

Luminescent is made for **Hyprland** and tested on **CachyOS**. It is plain Python + Qt 6, so it starts on any
distro (Arch, Fedora, Ubuntu, Debian, openSUSE…) and on any desktop (KDE, GNOME, Sway, X11…), but some parts lean on
Hyprland:

| Works everywhere | Needs Hyprland | Needs a wlroots compositor (Hyprland, Sway…) or X11 |
|---|---|---|
| The overlay, Dashboard, Automation, Themes, Clipboard, Settings, every panel opened from the tray or the command palette, Notes, Timers, Calculator, Reminders, Hearing Guard (PipeWire / PulseAudio), Media Controls (playerctl), QR codes, System Monitor, Vault, Metadata | **Global hotkeys** (they are Hyprland binds), so also Doodle's hold-Alt, the Automation **Hotkey** node and hotkeys on dashboard items; windows placed exactly where they belong; the frosted background; apps opening above the overlay; the Keybinds editor | Colour Picker, Text Grab and Doodle's "copy the screen" (they take a screenshot with `grim` on Wayland); the Automation **Type text** node (`wtype`, or `ydotool` anywhere) |

So on another desktop you use it from the **tray icon** (click = overlay, right-click = everything else) instead
of hotkeys. Running from source needs Python 3.11+; the ready-made Linux download needs glibc 2.35 or newer. Only
CachyOS + Hyprland is tested, so other setups may have rough edges.

## The overlay

`Ctrl+Shift+Space` dims every screen and shows the hub. `Esc`, the ✕ or a click on the dimmed area around a panel
hides it; the app keeps running, so it reopens instantly. The Dashboard works like a desktop: clicking it never
closes the overlay. **Quit everything** lives in Settings → General.

**Edit the layout**: the ✎ next to the ✕ lets you drag the logo, the title, the clock, the date and the status chips
anywhere on any screen, so they never sit under your dashboard boxes. **＋ Add** puts more on (a second clock on your
other screen, the sparkle on its own...), and right-click one to **Freeze** it in place, **Duplicate** it, make it
bigger or smaller, move it to another screen or remove it. **Reset** brings back the usual corner; Esc or **Done**
leaves edit mode.

`Ctrl+1…9` switch pages. Start typing on the Dashboard or Themes page and the command palette opens with what you
typed. The top-left corner shows the time, running timers, the Hearing Guard level and the next reminder. Settings →
Overlay & Interface picks the pages in the navigation bar (Notes, Timers, Calculator, Today and Usage can be added),
the screen it opens on (the one with the mouse by default), the frosted background, the **Text size** (85% to 150%,
every panel, page and menu) and whether it opens on the page you were on last.

* **Dashboard**: movable "tab boxes" on every screen. `＋ Add` → **Tab of items**, name it (Scripts, Games, Work…),
  right-click its header →
  **Add item** and point the item at a script, program, file, folder, URL or command. Double-click an item to run it;
  right-click for Run / Edit / Rename / Change file / Set hotkey / Move / Delete. Drag a header to move a box (it snaps
  to a grid), click the header to fold it, drag its right edge to resize. **Tidy up** lines them up and they glide
  into place: in **columns** (every box keeps its width), a **grid** or **rows**, in their current order, by name or
  biggest first (the ▾ next to it, or Settings → Dashboard). With boxes selected only those are tidied, and
  **Undo** / `Ctrl+Z` puts them back.
  `.py` / `.sh` scripts open in your terminal (kitty here; kitty, alacritty and foot get the Luminescent colours)
  and stay open so you can read the output (Settings → Dashboard & Scripts). The **Tools** tab runs Luminescent's own actions. Item hotkeys work
  anywhere, overlay open or not. The tray menu lists every item too.
  Tip: add your auto clicker (or any program) as an item, give it a hotkey, and it is one key, one click in the tray
  or one search in the palette away.
  **Pages as boxes**: `＋ Add` (or right-click the open space → Add here) puts Themes, Clipboard, Settings, Notes,
  Timers, Calculator, Today or Usage on the dashboard as a box you drag and **resize** (right edge, bottom edge,
  corner). Each dashboard remembers its boxes, where they are and what they show (the Settings section, the
  Clipboard folder, the open note…), also after a restart. ⤢ opens the full page, ✕ takes the box off.
  **Select like a desktop**: drag on the open space to draw a selection box, Ctrl/Shift+click headers to add to it;
  drag one selected box and the others come along, `Delete` removes them, `Ctrl+A` selects all, and the right-click
  menu folds, moves or removes the whole selection. **Several screens**: drag a box (or the whole selection) onto
  another screen and drop it there (or right-click → Move to screen); right-click menus and pop-ups open on the
  screen you clicked. **A different dashboard per screen**: right-click the open space on another screen → **This
  screen shows** → Always "Work" (or the dashboards menu → Show on screen); that screen keeps showing it whichever
  dashboard the toolbar switches to, and each dashboard remembers where its boxes are. **Snap**: boxes snap to an 8 px grid while you drag; right-click the open space →
  Snap to a grid (or Settings → Dashboard) changes it, and **Off** moves them pixel by pixel, perfectly smooth.
  A **new dashboard** can start empty, with the Tools tab, or as a copy; switching between them is instant (their
  boxes are kept, not rebuilt), and with many dashboards the toolbar folds them into one menu.
  More: drag items between boxes and drop files from Dolphin
  onto a box; real program and file icons; per item **run as administrator**, a working folder, **ask for
  arguments** and **run a command in a terminal window**; headings and separators; **groups** that run several
  items in order; a lock button and a search box; running items get a green dot and a **Stop**; folder items have
  **Open terminal here**.
* **Themes**: click a theme to apply it everywhere. Right-click → **Settings…** opens the theme editor: every colour
  in a list on the left, picked right there with a colour square, hue bar, hex and R G B boxes and swatches (the
  theme's, Caelestia's, your recent picks), with a live preview, a sample panel and a contrast check. Its other tabs
  hold the font, corner radius, glow, a background image behind the overlay (with an opacity slider), the card icon,
  the **Caelestia shell's** primary, secondary and third colours, a **desktop wallpaper** (a picture, or a
  **Wallpaper Engine** project), the **settings profile** it uses and module accents.
  Group themes in colour-coded folders. Dark and Light are built in and can not be deleted.
  **Import… / Export to a file…** share a theme as a `.lumtheme` file; **From a colour…** makes a whole theme from one
  accent colour; Settings… → **Module accents** gives a module its own colour (Notes in green…). A **Next theme**
  hotkey cycles through the active theme's folder.
* **Clipboard**: your Snippets folder as cards; click to paste. `＋ Save clipboard here` stores whatever is on the
  clipboard (text, an image, copied files) into the folder you are in. Folders can be coloured (right-click).
  The search box looks through every folder. Drag a card onto another to reorder, onto a folder (or the breadcrumb)
  to move it; right-click → **Favourite** puts it in the row at the top. Rest the mouse on a card for a bigger preview.
* **Settings**: every option, grouped by module, with a search box that highlights what you typed and counts the
  matches per section. Tick **Only matching settings** under it to hide every setting that does not mention it. **Settings profiles** at the top: each profile
  has its own hotkeys, sizes, sounds… and a theme can switch to one. General, Overlay and Dashboard settings are shared.
  A hotkey that clashes is flagged while you record it. General has **Show on start** (which panels open by
  themselves), the **tray menu** editor and a weekly automatic backup (last 5 kept).
* **Automation**: a huge workspace you move around in (right- or middle-drag, Space + drag; the wheel zooms) where
  you place nodes and wire them together, like Upload Labs. Drag from a port to another to wire them: **round
  ports** carry pulses ("this happened, now do that"), each carrying a bit of text; **diamond ports** carry values,
  and a field with a ◆ next to it takes its value from the wire instead of its box. Pulses run along the wires
  as glowing dots with a peek at what they carry. Right-click or double-click the space to add a node (or drop a
  wire on the open space to pick one that fits). It all runs in the background, overlay open or not.
  In any text field `{input}` is the text that came in, `{var:name}` a variable, and `{date}`, `{time}`, `{now}`,
  `{clipboard}`, `{newline}` what they say. Example: **Every… 5 s** → **Run command** (a terminal window or the
  background; one command per line, each after the last finishes) → **done** → **Show a notification**.
  * **Triggers**: Every N seconds, At a time of day (16:00, weekdays…), On a date (3 October 2028), Hotkey,
    Button (▶ on the node, the command palette or the tray), Luminescent events (started, overlay opened, theme
    changed, a timer finished, a reminder, Hearing Guard over the limit…), a program starts or stops, a file or
    folder changes, the clipboard changes, idle / back, CPU / GPU load, CPU / GPU temperature, memory or battery over or under a line, a window gets
    the focus, the internet goes away or comes back.
  * **Actions**: Run command, Open a program / file / URL, Close a program, any Luminescent tool, open or close a
    page, run a dashboard item, switch dashboard or theme, start a timer, a notification, a big message, **Ask me**
    (a box pops up, what you type goes on), play a sound, say it out loud, type text, press keys, copy to the
    clipboard, web requests (Discord webhooks, APIs), download a file, copy / move / delete files, write to a file,
    set a variable, switch a workspace on or off, **Set wallpaper** (a picture, GIF or video, or the next / a
    random one from a folder: wire a timer to it for a slideshow), **Volume** (set, up, down, mute), **Lock /
    sleep / shut down**, **Keep awake** on / off, **Open a note**, **Warning on screen** (a big warning in the
    middle of the screen, above everything, until you click Got it) and Show (see what a wire carries).
    Example: **CPU / GPU / temperature** (CPU temperature above 80 °C for 10 s) → **Warning on screen**.
  * **Logic**: Wait, If (contains, equals, regex, > <…), Gate, Count, Only once, Cooldown, Sequence, Take turns,
    Random chance, Repeat. **Values**: Text, Number, Variable, Join text, Maths, Date / time, Clipboard text, Read a
    file, Replace text, Change text (upper case, first line, length…), Random number. And a **Note** to write on.
  * **Workspaces** (bottom left): several, each with its own on/off switch; export one to a `.lumflow` file to
    share it. `Ctrl+Z` / `Ctrl+Y` undo and redo, `Ctrl+C` / `Ctrl+V` / `Ctrl+D` copy, paste and duplicate, `Delete`
    removes, `F` fits everything in view. **Variables** keeps values between restarts. Settings → Automation pauses
    everything (also from the tray or a hotkey) and has a safety stop for wire loops.
* **Notes / Timers / Calculator pages**: the same notes, timers and history as the floating panels, full size.
* **Today**: what you did today (dashboard runs, tools, how long timers ran), and earlier days with ‹ ›.
* **Usage**: your most used dashboard items, hotkeys and tools, and uses per day.

## Hotkeys (defaults, all changeable)

| What | Hotkey | |
|---|---|---|
| Overlay | `Ctrl+Shift+Space` | Dashboard · Themes · Clipboard · Settings (each can get its own jump hotkey) |
| Command palette | `Ctrl+Alt+Space` | Search every action, script, note, theme and settings page. Type a sum to calculate |
| Hotkey cheat sheet | `Ctrl+Alt+/` | Every hotkey, while held |
| Settings | `Ctrl+Alt+S` | Overlay straight on Settings |
| Snippets | `Ctrl+B` | Paste grid. Ctrl+click = paste + Enter |
| Notes | `Alt+N` / `Alt+E` | Show/hide / edit mode. Search box, right-click → Pin |
| Doodle | hold `Alt` | Draw (left) / erase (right). Wheel = brush size, Shift-drag = straight line, tool strip at the top for line, rectangle, circle, arrow |
| Doodle | `Alt+X` / `Alt+F` / `Alt+G` / `Alt+C` | Clear / undo / blackboard / copy screen + drawing |
| Doodle | `Alt+Shift+Q`, `Esc` | Panic / stop drawing |
| Timer | `Alt+T` / `Alt+R` | New timer or stopwatch / edit mode. Stopwatches have a Lap button (lap hotkey optional) |
| Calculator | `Ctrl+Alt+=` | Pop-out calculator with history and named pins |
| Colour picker | `Ctrl+Alt+C` | Loupe; click copies HEX/RGB/HSL, arrows nudge 1 px |
| Text grab | `Ctrl+Alt+O` | Drag a box around text; it is copied (Tesseract OCR) |
| Keybinds | `Ctrl+Alt+K` | Every keybind on the system: your Hyprland binds and Luminescent's, clashes in red |
| Vault | `Ctrl+Alt+Q` | Encrypted vault |
| Metadata | `Ctrl+Shift+M` | Metadata viewer / stripper |
| Hearing Guard | `Ctrl+Alt+H` | dB meter, volume limiter, daily dose |
| Window snapper | `Super+Alt+←/→/↑/↓` | Half, then third, then two thirds (press again). Corners in Settings |
| Screen ruler | `Ctrl+Alt+R` | Drag to measure. Shift keeps it level, click copies W×H |
| Media, QR code, System monitor | (set in Settings) | Also in the command palette and the tray |
| Automation | (set in Settings) | Pause / resume every automation |
| Keep awake | (set in Settings → General) | The PC does not sleep while it is on. Also in the tray and the palette |
| Quit | `Ctrl+U` | Quit everything (asks first) |

Alt-only shortcuts belong to Doodle (Alt is its draw key); Ctrl+Alt combos never flash the drawing on.

## The tools

* **Notes**: Markdown (`**bold**`, `# heading`, `- list`; 👁 shows it formatted), a colour and tags per note
  (`#work` in the search), drag notes and to-do items into a new order, **due dates** on to-do items (right-click)
  that become reminders, **Export** to .md / .txt, and **Stick on the screen** to keep a note as its own small panel.
* **Doodle**: the tool strip also has a **highlighter**, a **text tool** (click, let go of Alt, type, Enter) and a
  fill toggle for rectangles and circles.
* **Timer**: the new-timer hotkey opens a panel with a box: type `5m`, `1h30`, `25m focus` or `at 17:30` (empty =
  stopwatch). ⏲ in a panel types a new time. Countdowns show a progress ring (or a bar).
* **Calculator**: `Enter` calculates; `↑/↓` history; `54 + 10%` = 59.4; `×` `÷` `^` `1,000` `mod` and functions
  (sqrt, sin, log, fact…) work; a sum starting with an operator continues from the last answer (`ans`).
  Right-click a result → **Pin with a name**; the name works in later sums (`rent/4`). Keypad toggle, copy, recalc.
  Conversions: `5 km to mi`, `20 c to f`, `2.5 gb in mb`, `3 st to kg`, `20 usd to eur` (rates fetched once a day).
  Dates: `today + 45 days`, `days until 25/12/2026`, `2026-12-25 - today`. DEC / HEX / BIN / OCT in the header;
  `0x1f`, `&`, `|`, `xor`, `<<` work. Pins go in folders with colours, the history is grouped by day, and
  `Ctrl+Enter` calculates, copies the result and closes (⧉ copies without closing).
* **Colour picker**: the Recent colours in the tray (and palette) can become the theme's accent or a new theme.
* **Colour picker**: freezes the screen, shows an 11×11 zoom. The last 10 colours are in the tray menu and palette.
* **Screen text grab**: Windows uses its own text recognition. Linux uses Tesseract: `sudo pacman -S tesseract tesseract-data-eng` (plus `tesseract-data-<lang>`
  for other languages). Settings → Screen Text Grab can show the text in an edit box first (Ctrl+Enter copies).
* **Prompt Log**: your fish (or bash / zsh) history plus everything the dashboard launched; search, copy, run again in
  a terminal, delete lines or clear it. Each row shows when it ran; **Pin** keeps a command at the top and **Save to
  Dashboard** turns it into an item that runs it in a terminal.
* **Keybinds**: every Hyprland bind from your config with what it really does (`SUPER + G → Open BROWSER:
  zen-browser`, loop binds with their real keys and workspaces) next to Luminescent's hotkeys. Every row has ✎ to
  change its keys and ✕ to remove it; Hyprland binds are edited in your config (backup in `data/hypr-backups`, undone
  automatically if Hyprland reports an error) and Luminescent's own hotkeys change right there. **New keybind** binds
  keys or a **mouse button** (side, middle or extra buttons, e.g. a Logitech mouse's back and forward) to opening an
  app, running a command, any Luminescent tool, the volume or other keys.
* **Caelestia** (its own page in Settings): applying a theme recolours the Caelestia shell (bar, launcher,
  dashboard, notifications) by rewriting `~/.local/state/caelestia/scheme.json`, which Caelestia watches. Its own
  colours are kept in `data/caelestia-backups` and **Restore** puts them back. Optional: keep the shell on the theme's
  colours, colour other apps through Caelestia's own theming (GTK, terminals, Hyprland borders, Discord, btop...),
  and set the shell's look in `shell.json` (see-through panels, rounding, spacing, padding, text size, animation
  length). The **Caelestia & Glass** window edits every file in `~/.config/caelestia` as a form or as text, saved with
  a backup. **Glass apps** (Settings → Caelestia, or the window's Glass tab) make any app see-through: how solid it
  is focused and unfocused, blur on or off, and the blur strength.
* **Calendar** page: Atlas's events and tasks next to your reminders, with notifications on the PC.
* **Reminders** (Settings → Reminders): pop-ups, sounds, notifications, and **Switch theme**, so a 08:00 reminder can
  switch to Light and a 20:00 one to Dark. Also monthly and every-N-days schedules, and quick reminders from the
  command palette: `remind me in 20m to stretch`, `at 17:30 call mum`.
* **Hearing Guard**: estimated dB(A) at the ear, a dip limiter and the NIOSH daily dose. Calibrate once per device
  with a phone sound-meter app (Calibrate… in its panel). Headphone volume wheels are invisible to the PC: calibrate
  with the wheel at max. **WEEK** shows the last seven days, **APPS** every program playing with its own level and
  volume slider.
* **Ghost Vault**: a search box for names anywhere in the vault, and thumbnails for pictures (decrypted in memory,
  never written to disk).
* **Metadata Stripper**: drop a whole folder to strip every image and video in it (to copies, or in place after a
  confirm). Settings → Metadata Stripper adds right-click → **Strip metadata** to Dolphin (Linux) or Explorer
  (Windows, under Show more options).
* **Window Snapper**: the active window to a half, third or corner of its screen (on Hyprland it floats while
  snapped). The undo hotkey puts it back into the tiling layout (Linux) or where it was (Windows).
* **Screen Ruler**, **Media Controls** (any MPRIS player, needs `playerctl`), **QR Code** (the clipboard as a QR code;
  copy it as a picture or save PNG / SVG) and **System Monitor** (CPU, RAM, GPU, network with a minute of history).
* **Atlas Sync**: swaps notes, timer time and themes with the [Atlas](https://github.com/ForsakenBlank/Atlas) app on
  your phone over Wi-Fi, and brings Atlas's calendar events and tasks to the PC. Press **Let Atlas connect** (tray, command palette or Settings), then type the address and code
  into Atlas → Sync. Atlas lists every change in both directions before anything is written. Notes go both ways and
  to-do lists travel as checklists. Timer time adds up: what your timers ran here and what Atlas timer trackers
  logged end up on both. Nothing goes through the internet and everything sent is locked with the code.
* **Wallpapers**: themes and the Set wallpaper node use Noctalia, swww (which plays GIFs), hyprpaper or swaybg on
  Linux, mpvpaper for videos, and Windows' own setting on Windows. Settings → Overlay & Interface → **Wallpaper
  command** hands the file to your own tool instead (`mytool set {file}`).
* **Your own terminal**: Settings → Overlay & Interface → "Your own terminal follows the theme too" adds one
  include line to kitty.conf on Linux (it asks first and keeps a backup), or gives your Windows Terminal profiles the
  Luminescent colours on Windows.

## Performance

Nothing polls while hidden: the overlay, palette and cheat sheet only exist while shown, the tray tooltip only ticks
while a timer runs, and work that can block (OCR, wallpaper changes) runs on worker threads so the keyboard hook stays
responsive. Settings take a moment to build only for the section you open (the rest follow in the background), a
theme change throws the old overlay away before restyling, and the dotted background is painted once per screen.
**Performance mode** (Settings → Overlay & Interface) drops the glow, every animation and background images.

## Animations

The overlay fades in and out, pages and Settings sections fade in, a marker slides along the page switcher,
dashboard boxes glide when you tidy, fold or move them, themes crossfade, notifications and panels fade, and the
Automation page zooms smoothly. **Settings → Animations** has a master switch, a speed (Fast, Normal, Relaxed) and
one switch per animation. Performance mode turns them all off.

Settings → Animations → **Style** picks how things move: **Caelestia** (a little overshoot, panels rise into
place), **Smooth** or **Snappy**. **Hover effects** light up the edge of cards and dashboard boxes under the mouse.

## When something breaks

* An error shows a small "Luminescent error" card and goes into `data/launcher.log`.
* If the overlay itself gets into a bad state (the "wrapped C/C++ object ... has been deleted" kind of error), it is
  thrown away and built again on the spot instead of staying blank.
* A module that keeps failing (6 errors in a minute) is switched off until the next start, so the rest keeps
  working; Tray → Modules turns it back on.
* **Restart** always restarts, even when a part of Luminescent is broken.
* **Safe mode**: if Luminescent does not get going two starts in a row, the next start runs without modules, page
  boxes or automations, so you can still open Settings, read the log and fix things. Tray → **Safe mode: restart
  normally** leaves it. "Restart in safe mode" is also in the command palette.

## Backups

Settings → General → **Export backup…** zips settings, notes, timers, reminders, hearing history, dashboard, automations, themes,
profiles and calculator. **Import backup…** zips your current files to `data/backups/` first, then restarts.
The vault is never included.

## A standalone build

```
.venv/bin/pip install pyinstaller
.venv/bin/python build.py      → dist/Luminescent/Luminescent   (add --test to self-test it)
```

Running from source (`./start.sh`) is simpler and is what is tested. A build keeps its data in
`~/.local/share/Luminescent` and copies this folder's `settings.json` and `data/` there the first time it runs.
After that the two are separate, so use one or the other; they can not run at the same time.

## Layout

```
Launcher.py        entry point: tray, modules, overlay, palette
settings_ui.py     the Settings page (built from core/settings.py)
build.py           builds a standalone binary (optional)
start.sh           runs it from source (makes .venv on the first run)
core/              settings + profiles, hotkeys, theme + theme profiles, widgets, runner, terminal, wallpaper, backup,
                   hypr (Hyprland socket: binds, window placement), hyprbinds (reads/edits your Hyprland binds), clip,
                   convert (calculator units, currency, dates), usage (stats + today's log), desktop (launcher entry,
                   icons, Dolphin menu), timeparse
                   automation (the Automation engine and its node types), events (what happened, for automations)
ui/                overlay, dashboard, automation (the node editor), themes page, command palette, cheat sheet,
                   toasts, Today and Usage pages
modules/           one file per tool (+ vault_store.py: vault crypto, no UI)
data/              your notes, timers, dashboard, themes… (created on first start, never in git)
```

Something not working? Tray → **Open log file**. If a hotkey fires when it should not (e.g. R acts like Alt+R) or
combos stop working, tray → **Reset stuck keys** (it also clears lost key releases by itself).

## Notes on the vault

* It does **not** run as Administrator (an elevated window hides keyboard events from the other tools). If the vault
  folder needs admin rights to write to, the vault says so; pick another folder in Settings → Ghost Vault.
* Real and decoy vaults are completely separate. "Forgot password?" on the unlock dialog uses the recovery key.
