# Changelog

## 1.5.2

* **Caelestia colours in every theme**: the theme editor has a new Caelestia colours page with every colour of
  Caelestia's scheme. Set any of them for that theme, or Match Caelestia to this theme in one click, and they go
  into Caelestia whenever you switch to it, with a live preview of the bar and launcher. Switching to a theme
  without any puts Caelestia's own colours back, and if Caelestia remakes its scheme (a new wallpaper) the
  theme's colours go back on top.

## 1.5.1

* **Panels and pop-ups open where you are**: a panel opened by its hotkey goes to the middle of the screen with
  the mouse (Settings → Overlay & Interface → Panels like Notes open; "Where I left them" still keeps your spot,
  but never on another monitor or half off the screen). Questions like "Name:", confirmations, the colour picker
  and the theme editor open in the middle of the screen with the mouse too (or of the window that asked, when the
  mouse is on it). New timers, new sticky notes and snapping use that screen instead of always the primary one.
* **Theme folders in any order**: drag a folder's header (Built-in and My themes too), or right-click it → Move
  up / down / to the top / to the bottom. Right-click a theme → Move earlier / later orders the themes inside it.
* **Rounder hover effects**: the ✕ and other icon buttons are round, so the ✕ in a corner no longer pokes out of
  the window when you hover it; small buttons, menu rows and list rows always have rounded corners (unless the
  theme's corner radius is 0); dashboard rows sit inside their box's rounded corners.
* **Colour picker buttons**: Cancel and Done lost their look when the picker was opened from a colour swatch (the
  Caelestia & Glass colours, the theme editor): Done showed black text on a dark button. Fixed.
* **Every dialog in Luminescent's look**: the vault password and recovery key, the keybind editor, the dashboard
  item editor, Hearing Guard calibration, automation variables and the date picker were plain system windows.
* **Disabled buttons look disabled** (a greyed accent button no longer looks clickable), and so do disabled
  fields and check boxes.
* **Tidier fields**: numbers and times in spin boxes are no longer glued to the left edge, date fields with a
  calendar have a proper arrow, drop-down lists have rounded rows, and the reminders editor fits the settings page.
* **Long names end in "…"** on the dashboard instead of being cut off mid-letter.
* **Clipboard page**: when the snippets folder does not exist yet it offers to make it (or pick another one).
* **Notes** has a ＋ next to its search for a new note.

## 1.5.0

* **A new colour picker everywhere** (themes, folders, settings, Caelestia, Snippets): a saturation / brightness
  square, a hue strip, the colour you started with next to the new one (click it to go back), a hex box that also
  takes rgb(…), hsl(…) and colour names, copy, an eyedropper (Luminescent's screen colour picker), how readable
  text is on it, and rows of recent picks, this theme's colours and basics. Arrow keys nudge it.
* **Gradients**: any colour of a theme can be a gradient. Open a colour, switch to Gradient, click the bar to add
  as many colours as you like, drag them to move them (down off the bar to remove one), pick Linear, Radial or
  Sweep and the angle, or start from a preset. Backgrounds, cards, headers, buttons, sliders, borders and the
  page switcher draw them.
* **The theme editor, rebuilt**: bigger, rounded, and in three columns you can resize by dragging the lines
  between them: the parts of the theme, that part's settings, and a live preview. Every module, the notifications
  and the on-screen warnings can now have a whole look of their own (an accent, or any of the colours, each one
  plain or a gradient, and their own corner radius) instead of only an accent. "Copy the look of…" and "Match
  all its colours to it" make that quick.
* **Notifications, redone**: the accent sits inside the card (a soft glow, a stripe, a line along the top, an
  outline or none) instead of a bar that poked out of the corners, and long messages are never cut off: the card
  grows to fit (very long ones show "Click to read it all"). Hover for ✕, right-click to copy the text, errors get
  a red accent. Settings → General has where they appear, width, accent style, how long they stay, how many at
  once, background opacity, text size, the icon, and Show a notification to try it. Their colours are in the theme
  editor. On-screen warnings grow and scroll the same way.
* **Message boxes and questions** (delete?, rename…, errors) in Luminescent's own rounded look; long text scrolls.
* **Settings**: a ☆ on every section (on hover) pins it to the top of the list. Module pages have buttons that do
  what their hotkeys do (open the panel, pick a colour, new timer…), or Turn it on when the module is off.
* **Sliders jump to where you click**, and **the mouse wheel scrolls the page**: a number box, list or slider only
  takes the wheel after you have clicked it, so scrolling down a page no longer changes values on the way past.
* **Dashboard → Add here → Page** (and the navigation bar) now has Hearing Guard, System Monitor and Media.
* **Hearing Guard**: a Record mode that only keeps the readings (no warnings, nothing turned down); Headset's own
  volume, for headphones with a volume of their own, so the estimate follows it (and calibrating takes it into
  account); volumes past 100 % are handled and the cap goes up to 150 %. The Today graph has a dB scale, the limit
  line, finer bars on a short day and the time, level and listening time on hover; the Week graph always shows the
  100 % line and the numbers on hover.
* **Caelestia & Glass → Colours**: every colour of Caelestia's scheme can be changed and applied (a backup
  first). Keep them for that Caelestia scheme (they come back whenever Caelestia uses or remakes it) or for a
  Luminescent theme (written into Caelestia whenever that theme is applied). Caelestia draws its colours flat, so
  gradients can not go into its scheme.
* **Fixes**: a module panel's own accent no longer turns back into the theme's when it goes into edit mode or
  changes opacity; typing invalid JSON in a Caelestia list field no longer throws an error; reminders and dose
  warnings show a Luminescent card as well as the desktop notification when that is the notification style.

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
