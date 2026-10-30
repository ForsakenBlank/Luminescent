# Changelog

## 1.8.1

* **Keys for themes.** Give any theme a key of its own (theme editor → General → KEY) and press it anywhere to switch
  straight to that theme. Next to the **Next theme** key there is now a **Previous theme** key too (Settings → Overlay
  & Interface), and both are in the command palette and the automation Switch theme step.
* **Theme wallpapers work with Caelestia.** Luminescent used to start its own wallpaper program (swaybg) under
  Caelestia's, so which picture you saw was down to luck, and restarting Caelestia shuffled them again. Old copies
  were also left running. Now, when Caelestia draws your wallpaper, the theme's picture goes to Caelestia itself and
  the stray copies are stopped.
* **Caelestia's colours follow the switch.** The wallpaper goes on first and the theme's colours after it, so a theme
  with its own colours always ends up with them. A theme without colours of its own now gets the colours Caelestia
  makes from its wallpaper, instead of old ones from before you changed wallpaper.
* **Switching quickly is safe.** Press a theme key a few times in a row and the last theme you picked is the one you
  end up with, wallpaper, colours and all.
* A theme whose wallpaper file has gone missing now says so, instead of quietly leaving the old one up.

## 1.8.0

* **Window borders with the theme.** A new **Window borders** page in the theme editor (under DESKTOP, with
  Hyprland) sets the line around every app's window: one colour for the window you're using, or two in a gradient
  with an angle, and another colour for every other window. **Use this theme's colours** fills them in from the
  theme's accent and border colour in one click. A theme that leaves them unticked keeps your Hyprland config's own
  colours, and switching to it, or quitting Luminescent, puts those back. Editing your Hyprland config no longer
  wipes them either: they go back on after Hyprland reloads.

## 1.7.0

* **Caelestia's colours make sense now.** All 110 of them are sorted by what they do on screen (main accent,
  second and third accent, backgrounds and surfaces, text and icons, outlines, good and bad, terminal, the colours
  other apps take), each with a plain name and where it shows, for example "Button background: the background of
  most buttons, toggled icon buttons and the slider tracks". Fold the groups you do not need, search by name, by
  where it shows or by #hex, or search "green" to find every colour that *looks* green whatever it is called.
  Hover a colour and it lights up in the preview; click anywhere in the preview to jump to the colour used there.
  The preview is bigger too: the bar, a dashboard drawer, the launcher, notifications, a terminal and the colours
  btop, cava, Zed and Qt apps get.
* **Whole schemes in a few clicks.** Quick tools for Caelestia's colours, the theme's own colours and the terminal:
  * **Main colours**: pick an accent, a background and text (and a second accent, a third, success and error if
    you like) and every other colour is made from them, with real lighter and darker steps, so a white theme still
    has cards, borders and quiet text a few shades apart instead of everything being #FFFFFF.
  * **Harmony**: colours that go together around your accent (matching, opposites, three-way, split, one colour,
    pastel, neon).
  * **From a picture**: the colours of your wallpaper or any picture; click one of them to make it the accent.
  * **Surprise me**: a random scheme that still works. Press Again until you like one.
  * **Adjust all**: sliders for every colour at once: hue, brightness, contrast, colourfulness and warmth.
  * **Follow the accent**: make every green (or any other hue, or every colourful colour) take your accent's hue.
  * **Add variance**: colours that came out the same get a few steps between them.
  * **Light ⇄ dark**: a light scheme from a dark one, or the other way round.
  Every tool shows the colours before and after side by side, lets you choose which groups it may change, and is
  one step to undo.
* **Undo, redo, favourites and recent changes** in the theme editor. ↶ and ↷ at the bottom (Ctrl+Z, Ctrl+Shift+Z or
  Ctrl+Y) step back through every change. **Recently changed** shows old → new for each change with a button to put
  just that one back, and is kept with the theme. Star any colour to pin it at the top.
* **Caelestia's apps follow too.** When a theme gives Caelestia its colours, Caelestia now also re-colours the apps
  it themes (the terminal, btop, GTK and Qt apps, Discord, Spotify...), the same as when it switches scheme itself.
  Before, only the shell changed. Settings → Caelestia & Glass can turn it off.
* **The terminal really changes now.** Setting the terminal colours did nothing for three reasons, all fixed:
  "your own terminal follows the theme" was off, kitty.conf had the include line with a typo
  (`luminescent-theme.con`), and fish loads Caelestia's terminal colours at the start of every shell, painting over
  whatever kitty.conf says. The Terminal page now has a **Where your terminal gets its colours** card that says
  what is in the way and puts it right (the kitty.conf typo is fixed in place, with a copy kept first). With the
  option on, the theme's colours also go into the file Caelestia's fish config loads, and to every terminal that
  is already open, as soon as you save.
* **More for the terminal**: colours in groups (basics, accents 16 to 18, normal, bright) with the quick tools,
  "All from the theme" and "Same as Caelestia's"; the start-up picture's height, the space around it and where it
  goes (left, above, right); a **start-up text** (or take your current fish greeting's), its colour, and an **info
  box**: your own fastfetch setup, none, or Luminescent's, where you pick which lines show (distro, kernel, memory,
  CPU, GPU, battery, colour dots...), their labels and colours, as a box or plain lines.
* **Narrow terminals stay tidy.** Below a width you choose (90 columns to start with), the start-up leaves the
  picture out, puts a smaller one above, or shows only the text, so four terminals side by side no longer wrap
  into a mess. Luminescent does this with its own `conf.d/luminescent-greeting.fish`; your fish_greeting file is not
  changed, and the file goes away when the theme turns the start-up off.
* **Glass for any of Luminescent's windows**: tick which ones (panels, Notes and sticky notes, notifications,
  dialogs, the big warnings) and how see-through they are, per theme, with **Make these the default for every
  theme**. The blur strength is labelled as what it is: one setting for the whole desktop, since Hyprland can only
  turn blur on or off per window.
* **Fix: the colour picker works on both monitors.** The frozen picture of the second monitor opened on the first
  one, a little way down, so the other screen could not be picked from and a strip of the wrong picture showed at
  the top. Each monitor now gets its own, right in its corner. Enter picks the colour under the mouse on whichever
  monitor it is, and no magnifier is left behind on the other one.
* **Fix: doubled gaps after turning the Caelestia extras on or off.** Restarting Caelestia left the old shell
  running next to the new one, so every gap and the space for the bar were reserved twice (windows and the
  overlay's page switcher sat further in than they should). Now every Caelestia shell is stopped first, whichever
  folder it runs from.

## 1.6.0

* **Fix: panels open where they should.** Notes, the calculator and every other panel ignored Settings → Overlay &
  Interface → Panels like Notes open and all sat in the same corner. Hyprland was quietly ignoring every move of a
  pinned window, and the first move came before it had finished placing the window. Panels now open in the middle
  of the screen with the mouse, next to the mouse or where you left them, as you choose, on every monitor, and
  pop-ups (names, confirmations, the colour picker, the theme editor) open in the middle of the screen with the
  mouse. Dragging a panel works the same way.
* **Your whole desktop changes with the theme.** The theme editor has a new DESKTOP group:
  * **Caelestia look**: how see-through Caelestia's panels and sidebar are, rounding, spacing, text size, the screen
    border, fonts (the big one is the lock screen clock), the dashboard picture, the power menu picture (shut down,
    reboot, log out) and how fast they play, the logo, the "no notifications" pictures, the lock screen (wallpaper or
    a blur of the screen, logo colours, notifications), which dashboard pages show, and the desktop clock and
    visualiser. Tick a setting to have the theme set it.
  * **Caelestia extras**: a picture or GIF in place of the lyrics on the dashboard's media page, and a picture or
    GIF behind the lock screen, with how much it is blurred.
  * **Glass**: how see-through every app is (focused and not), the blur strength, and frosted glass for
    Luminescent's own panels and notifications.
  * **Terminal**: every terminal colour, how see-through kitty is, and the picture fastfetch shows when a terminal
    opens.
  Anything a theme leaves alone stays as it is, or goes back to what it was before another theme changed it.
* **Glass for Luminescent's panels and notifications**: Settings → Overlay & Interface → Glass panels and
  notifications blurs what is behind Notes, the other panels and notifications (or set it per theme).
* **Every other app**: Caelestia & Glass → Glass can now make every app see-through, focused and not, besides the
  apps with a row of their own, and **Add the app I click next** adds a window by clicking it.
* **Lock the screen**: a button on the dashboard, a tray entry, a command palette action and a hotkey open
  Caelestia's lock screen.
* **A better gradient editor**: every colour of a gradient is listed with its hex code and place (type or nudge
  them), **＋ Colour** adds one in the biggest gap, arrows set the direction in one click, and **Ideas** offers
  gradients worked out from your colour. A solid colour turned into a gradient starts with a second colour that goes
  with it, and the ◐ next to every theme colour makes it a gradient at once.
* **Pictures and GIFs on the dashboard**: ＋ Add → Picture or GIF. Right-click it for its frame (a panel, just a
  border or nothing), how it fills the box, its corners, how solid it is and a title bar. Drag it anywhere, resize
  it from its edges.
* **Smoother theme changes**: the old look fades into the new one without the screen flashing in between, open
  panels fade too, and the Themes page stays where you scrolled to instead of jumping to the top.
* **Updates in Settings**: with more than six releases, the newest five show and the rest are behind Show more.
* **Fix**: panels no longer keep an empty strip along their bottom edge after opening.

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
