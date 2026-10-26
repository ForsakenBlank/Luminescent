# Luminescent (formerly UsefulScripts / AllinOne)

One background app for **Windows 11** and **Linux** (Hyprland, tested on CachyOS) that keeps every little tool a hotkey
away: a GeForce-overlay style
**hub** (Dashboard, Themes, Clipboard, Settings, and optional Notes, Timers, Calculator, Today and Usage pages), plus
**Notes**, **Screen Doodle**, **Timers / Stopwatches**, **Calculator**, **Colour Picker**, **Screen Text Grab**,
**Prompt Log**, **Reminders**, **Hearing Guard**, **Ghost Vault**, the **Metadata Stripper**, a **Window Snapper**, a
**Screen Ruler**, **Media Controls**, a **QR Code** maker and a **System Monitor**. Everything runs in one process with
one keyboard hook.

## Download

The [Releases page](https://github.com/ForsakenBlank/Luminescent/releases/latest) has ready-made builds:

* **Windows**: `Luminescent-windows.zip`. Unzip it anywhere and run `Luminescent.exe`.
* **Linux**: `Luminescent-linux.tar.gz`. Unpack it and run `Luminescent/Luminescent` (Hyprland needed for hotkeys).

A download starts completely fresh. Your settings, notes, timers, themes and the rest live in your own data
folder, not next to the program: `~/.local/share/Luminescent` on Linux, `%LOCALAPPDATA%\Luminescent` on Windows.

**Updating**: Luminescent tells you when a new version is out (tray → Update available). Download it and replace the
old program folder with the new one: your data folder is not touched, so everything is still there.

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

After pulling a new version, install the requirements once more (`.venv/bin/pip install -r requirements.txt`, or
`.venv\Scripts\pip install -r requirements.txt` on Windows).

A few things are Linux-only: the Keybinds panel can edit Hyprland binds (on Windows it lists Luminescent's hotkeys),
the frosted overlay background, and apps opening above the overlay. "Super" in hotkeys is the Windows key.

A **sparkle** appears in the tray in your theme's accent colour. **Click it** (or press **Ctrl+Shift+Space**) to open
the overlay. Right-click it for the dashboard items, quick actions, modules, Settings, Restart and Quit.
Settings → General → **Run at login** starts it when you sign in (an autostart entry on Linux, the Run key on Windows).

## The overlay

`Ctrl+Shift+Space` dims every screen and shows the hub. `Esc`, the ✕ or a click on the dimmed area hides it; the
app keeps running, so it reopens instantly. **Quit everything** lives in Settings → General.

`Ctrl+1…9` switch pages. Start typing on the Dashboard or Themes page and the command palette opens with what you
typed. The top-left corner shows the time, running timers, the Hearing Guard level and the next reminder. Settings →
Overlay & Interface picks the pages in the navigation bar (Notes, Timers, Calculator, Today and Usage can be added),
the screen it opens on and the frosted background.

* **Dashboard**: movable "tab boxes". `＋ New tab`, name it (Scripts, Games, Work…), right-click its header →
  **Add item** and point the item at a script, program, file, folder, URL or command. Double-click an item to run it;
  right-click for Run / Edit / Rename / Change file / Set hotkey / Move / Delete. Drag a header to move a box (it snaps
  to a grid), click the header to fold it, drag its right edge to resize. **Tidy up** lines them all up.
  `.py` / `.sh` scripts open in your terminal (kitty here; kitty, alacritty and foot get the Luminescent colours)
  and stay open so you can read the output (Settings → Dashboard & Scripts). The **Tools** tab runs Luminescent's own actions. Item hotkeys work
  anywhere, overlay open or not. The tray menu lists every item too.
  Tip: add your auto clicker (or any program) as an item, give it a hotkey, and it is one key, one click in the tray
  or one search in the palette away.
  More: several dashboards (Work, Gaming…) from the toolbar; drag items between boxes and drop files from Dolphin
  onto a box; real program and file icons; per item **run as administrator**, a working folder, **ask for
  arguments** and **run a command in a terminal window**; headings and separators; **groups** that run several
  items in order; a lock button and a search box; running items get a green dot and a **Stop**; folder items have
  **Open terminal here**.
* **Themes**: click a theme to apply it everywhere. Right-click → **Settings…** for every colour (full colour wheel),
  font, corner radius, glow, a background image behind the overlay (with an opacity slider), the card icon, a
  **desktop wallpaper** (a picture, or a **Wallpaper Engine** project) and the **settings profile** it uses.
  Group themes in colour-coded folders. Dark and Light are built in and can not be deleted.
  **Import… / Export to a file…** share a theme as a `.lumtheme` file; **From a colour…** makes a whole theme from one
  accent colour; Settings… → **Module accents** gives a module its own colour (Notes in green…). A **Next theme**
  hotkey cycles through the active theme's folder.
* **Clipboard**: your Snippets folder as cards; click to paste. `＋ Save clipboard here` stores whatever is on the
  clipboard (text, an image, copied files) into the folder you are in. Folders can be coloured (right-click).
  The search box looks through every folder. Drag a card onto another to reorder, onto a folder (or the breadcrumb)
  to move it; right-click → **Favourite** puts it in the row at the top. Rest the mouse on a card for a bigger preview.
* **Settings**: every option, grouped by module, with a search box. **Settings profiles** at the top: each profile
  has its own hotkeys, sizes, sounds… and a theme can switch to one. General, Overlay and Dashboard settings are shared.
  A hotkey that clashes is flagged while you record it. General has **Show on start** (which panels open by
  themselves), the **tray menu** editor and a weekly automatic backup (last 5 kept).
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
  zen-browser`, loop binds with their real keys and workspaces) next to Luminescent's hotkeys. Double-click opens the
  line in an editor; Add / Change keys / Remove edit your config (backup in `data/hypr-backups`, undone
  automatically if Hyprland reports an error) and reload Hyprland.
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
* **Your own terminal**: Settings → Overlay & Interface → "Your own terminal follows the theme too" adds one
  include line to kitty.conf on Linux (it asks first and keeps a backup), or gives your Windows Terminal profiles the
  Luminescent colours on Windows.

## Performance

Nothing polls while hidden: the overlay, palette and cheat sheet only exist while shown, the tray tooltip only ticks
while a timer runs, and work that can block (OCR, wallpaper changes) runs on worker threads so the keyboard hook stays
responsive. **Performance mode** (Settings → Overlay & Interface) drops the glow, fade and background images.

## Backups

Settings → General → **Export backup…** zips settings, notes, timers, reminders, hearing history, dashboard, themes,
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
ui/                overlay, dashboard, themes page, command palette, cheat sheet, toasts, Today and Usage pages
modules/           one file per tool (+ vault_store.py: vault crypto, no UI)
data/              your notes, timers, dashboard, themes… (created on first start, never in git)
```

Something not working? Tray → **Open log file**. If a hotkey fires when it should not (e.g. R acts like Alt+R) or
combos stop working, tray → **Reset stuck keys** (it also clears lost key releases by itself).

## Notes on the vault

* It does **not** run as Administrator (an elevated window hides keyboard events from the other tools). If the vault
  folder needs admin rights to write to, the vault says so; pick another folder in Settings → Ghost Vault.
* Real and decoy vaults are completely separate. "Forgot password?" on the unlock dialog uses the recovery key.
