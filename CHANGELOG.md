# Changelog

## 1.5.0

* **The Caelestia shell follows your theme**: applying a theme now recolours the Caelestia bar, launcher,
  dashboard and notifications (its primary and secondary colours, backgrounds and text). A theme can pick its own
  shell colours in its editor → Desktop & Caelestia. Caelestia's own colours are kept, and Settings → Caelestia →
  Restore puts them back.
* **Caelestia has its own Settings page**, near the top: the shell follows the theme, keep it on the theme's colours,
  colour other apps through Caelestia's own theming (GTK, terminals, Hyprland borders, Discord, btop...), the shell's
  look in shell.json (see-through panels and how solid they are, rounding, spacing, padding, text size, animation
  length) and **Glass apps**: make any app see-through, focused and unfocused, with blur and its strength.
* **A new theme editor**: every colour in a list, edited right there with Luminescent's own colour picker, a live
  preview, a sample panel in the theme's colours, a contrast check, Match everything to the accent, Swap dark /
  light and Undo all changes. The other accent shades follow the accent while you pick it.
* **A new colour picker everywhere** (themes, settings, folders, Caelestia files): a colour square, a hue bar, hex
  and R G B boxes, the colour before and after, and swatches from the theme, Caelestia and your recent picks.
* **Every pop-up window** (confirmations, questions, the keybind window, the vault's password box...) is a rounded
  card with an outline and a title bar you can drag, instead of a square window with no edge.
* **Fix**: long notifications are no longer cut off. They grow to fit; a very long one shows its start with "Click
  to read all". Click again or ✕ to dismiss, right-click to copy, and long ones stay up longer.
* **Notifications in Settings → General**: which corner and screen, width, how many at once, how long they stay and
  how solid the background is.
* **Fix**: in the keybind window, pressing Enter after typing a combo no longer starts recording over it. The window
  is laid out in clear sections (keys, mouse button, what it does) without overlapping.
* **More to customise in Settings → Overlay & Interface**: a font, a monospace font and a corner rounding for every
  theme, and the clock (12 or 24 hour, with or without seconds) and date formats.
* Text on an accent colour is picked so it stays readable (dark text on light accents).

## 1.4.0

* **Updates you can see**: Settings → General → Updates shows your version, a Check for updates button and every
  release with Update, Reinstall or Downgrade. A new version also shows as a chip on the overlay.
* **Keybinds, reworked**: every row has ✎ (change keys) and ✕ (remove). Luminescent's own hotkeys change right
  there instead of in Settings. **New keybind** binds keys to opening an app, running a command, any Luminescent
  tool, the volume or other keys, while Luminescent runs. A "My keybinds" filter lists them.
* **Mouse buttons**: side, middle and extra mouse buttons (a Logitech mouse's back and forward buttons...) work
  as keys: click the box in the keybind window with the button, or pick it from the list. Ctrl + a side button
  works too. Linux only.
* **Caelestia & Glass** (new tool): every file in ~/.config/caelestia as a form with switches, numbers and
  colour swatches (or as text), saved with a backup in data/caelestia-backups; Caelestia's colour scheme and
  wallpaper with Make a Luminescent theme from it; and per-app glass: how see-through each app is when focused
  and when not, blur on or off, and Hyprland's blur strength. The glass rules are added while Luminescent runs
  and come back after a config reload; your config file is not changed.
* **Calendar page** (add it under Settings → Overlay & Interface → Navigation bar): a month with dots on busy
  days and each day's Atlas events, Atlas tasks and your Luminescent reminders.
* **Atlas Sync**: your current theme goes to Atlas (Atlas → Settings → Follow Luminescent's theme switches to it),
  themes made in Atlas come here as "Atlas: …", and Atlas's events and tasks come over with every sync. This PC
  then gives you notifications for them like the phone does (Settings → Atlas Sync).
* **Ten new themes** in a More themes folder: Caelestia, Catppuccin Mocha and Latte, Tokyo Night, Gruvbox, Nord,
  Rosé Pine, Dracula, Everforest and Material You.
* **Animation style**: Settings → Animations → Style: Caelestia (things glide with a little overshoot and panels
  rise into place), Smooth or Snappy. **Hover effects**: cards and dashboard boxes light their edge under the mouse.
* **More things for the overlay's edit mode** (✎ next to ✕): running timers, the next reminder, now playing,
  CPU and memory, a greeting with your name and your own text. Your own text and the greeting are edited with
  right-click.
* **Automation**: the drag selection box and selected nodes look like the Dashboard's.
* Clock, chips and every other overlay element stay out of the way of the Automation page.

## 1.3.0

* **Fix**: Notes (Alt+N) and the other panels open on the screen with the mouse, at the same spot they had on
  their old screen, instead of wherever they were last saved (often another monitor). Sticky notes stay put.
  Settings → Overlay & Interface → Open the overlay on: The primary screen keeps the old behaviour.
* **Panels like Notes open**: Settings → Overlay & Interface picks where: where you left them, in the middle of
  the screen, or next to the mouse.
* **Fix**: letting go of Alt (the Doodle key) no longer moves the mouse onto another window.
* **Fix**: holding the draw key with nothing drawn yet could leave the Doodle tool strip unclickable.
* **Doodle: start with a tool**: Settings → Screen Doodle → Start drawing with picks the tool every time you hold
  the draw key (pen, highlighter, shapes, text), or keeps the one you used last.
* **Edit the overlay's layout**: ✎ next to the ✕ lets you move the logo, title, clock, date and status chips anywhere
  on any screen, add more, duplicate them, resize them and freeze them in place (right-click).
* **Fix**: automation notifications no longer come with an error, and a hard crash now leaves its details in
  data/crash.log.
* **Atlas Sync**: swaps notes and timer time with the Atlas app over Wi-Fi. Press Let Atlas connect (tray, command
  palette or Settings → Atlas Sync), then type the address and code into Atlas → Sync. Atlas shows every change
  before anything is written.
* **Fix**: the "wrapped C/C++ object of type QWidget has been deleted" error that could leave the overlay empty and
  stop Restart from working. Windows that vanished before Hyprland placed them no longer break every later window,
  and the overlay rebuilds itself if a page gets into a bad state.
* **Stays up when a part fails**: a module that keeps failing is switched off until the next start, Restart always
  works, and after two failed starts Luminescent starts in **safe mode** (tray → Safe mode: restart normally).
* **Animations** for everything: the overlay, pages, the page switcher, dashboard boxes, theme changes, notifications,
  panels and Automation zoom. Settings → Animations turns each one off, sets the speed, or turns them all off
  (performance mode does too).
* **Faster**: switching themes is several times quicker, and so is opening a page for the first time (Settings
  builds only the section you open).
* **Dashboards on several screens**: drag boxes onto another screen, right-click menus open on the screen you
  clicked, and a screen can always show a dashboard of its own (right-click → This screen shows).
* The overlay now opens on the screen with the mouse (Settings → Overlay & Interface can keep it on the primary one).
* **Tidy up, reworked**: columns that keep every box's width, a grid or rows; keeps the order or sorts by name or
  size; only the selected boxes if any are selected; boxes glide into place; Undo or Ctrl+Z puts them back.
* **Settings search**: matches are highlighted and counted per section, and **Only matching settings** hides the rest.
* **Automation**: holding right-click to move around and zooming at the same time no longer jumps or smears; the
  wheel zooms smoothly around the mouse.

## 1.2.0

* **One-click updates**: tray → Update now downloads the new version, swaps it in and restarts. Your notes and
  settings stay where they are. From a git checkout it runs git pull and installs the requirements.
* **Text size**: Settings → Overlay & Interface → Text size (85% to 150%) for every panel, page, menu and pop-up.
* **Bigger node picker**: adding an Automation node now shows bigger names with what each node does underneath.
* **New Automation nodes**: Set wallpaper (a picture, GIF or video, or the next / a random one from a folder),
  Volume (set, up, down, mute), Lock / sleep / shut down, Keep awake, Open a note, Warning on screen,
  Window focused, Internet on / off.
* **Temperatures and GPU**: the CPU / memory / battery trigger also watches CPU temperature, GPU load and GPU
  temperature, so "CPU above 80 °C" can pop up a warning.
* **Warning on screen**: a big warning in the middle of the screen, above everything, that stays until you click
  Got it. "Show a notification" can use it too.
* **Keep awake**: stops the PC sleeping. In the tray, the command palette, a hotkey and the Automation page.
* **Wallpaper command**: Settings → Overlay & Interface lets your own tool change the wallpaper (for themes and
  the Set wallpaper node). Linux plays GIFs with swww and videos with mpvpaper.
* **Open on the last page**: Settings → Overlay & Interface can open the overlay where you left it.
* **Fix**: Calculator, Notes, Timers and other panels come back above the overlay when you click on it or switch
  pages (they could end up behind the Dashboard).
* This "what is new" message after an update (Settings → General can turn it off).
