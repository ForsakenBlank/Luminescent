# Changelog

## 1.3.0

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
