# New features (beyond parity)

Ideas for after 2.0 reaches parity, in the same tick-list style as IDEAS.md: put an `x` in the box of each one you
want (`- [x]`) and phase 16 builds only the ticked ones, one at a time, each behind a Settings toggle. Anything not
ticked is left alone. Ideas you already said no to (laser pen, Pomodoro / presets, clipboard history, quiet hours,
activation key, a built-in auto clicker) are not in here.

Each line ends with its size (S = one session, M = two or three, L = four or more) and the model and effort to
use. Progress for this track = ticked features done ÷ ticked features.

## Things Rust makes cheap

### Works on more desktops
- [ ] **Hotkeys on KDE and GNOME** through the GlobalShortcuts portal, and on X11 through key grabs, so global
  hotkeys are not Hyprland-only any more. (M · Opus 5.5 high)
- [ ] **Native Wayland overlays** (layer-shell surfaces) for the overlay, doodle, HUD and notifications: no window
  rules needed, works on Sway, river and KDE too. (L · Opus 5.5 xhigh)
- [ ] **Screenshots through the portal** on KDE and GNOME, so the colour picker, ruler and text grab work there.
  (S · Sonnet 5.5 high)
- [ ] **Typing text without wtype / ydotool** through the virtual-keyboard protocol. (M · Opus 5.5 high)
- [ ] **Frosted overlay on Windows 11** (acrylic or Mica behind the dimmed screen, like Hyprland's blur), with
  Settings → Overlay → Frosted background working on both systems. (S · Opus 5.5 high)

### Speed you can feel
- [ ] **Instant search everywhere**: the palette also finds apps (desktop entries / Start menu), files in folders
  you choose, and every setting, with typo-tolerant matching. (M · Sonnet 5.5 high)
- [ ] **Built-in OCR** with `ocrs` (no Tesseract to install), plus "grab the last area again" and a language
  picker. (M · Sonnet 5.5 high)
- [ ] **Lossless metadata stripping**: remove EXIF / GPS / XMP from JPEG, PNG and WebP without re-encoding, so
  pictures keep their exact pixels. Re-encoding stays as an option. (S · Sonnet 5.5 medium)
- [ ] **Performance history**: keep a day (or a week) of CPU, GPU, temperature and memory history on disk, with
  charts you can scroll back through. (M · Sonnet 5.5 high)

### Automation, deeper
- [ ] **A Lua script node**: write a few lines of Lua that get the pulse's text and return a new one, sandboxed,
  with a time limit. (M · Opus 5.5 high)
- [ ] **Sub-graphs**: fold a group of nodes into one reusable node with its own inputs and outputs. (L · Opus 5.5
  xhigh)
- [ ] **Run history and step-through**: a log of every pulse with timings, breakpoints on wires, and "run one
  step". (M · Opus 5.5 high)
- [ ] **Typed values**: numbers, yes / no, lists and JSON on value wires, with a JSON path node. (M · Opus 5.5 high)
- [ ] **More triggers**: USB device plugged in, a monitor connected, a Bluetooth device connected, a media track
  changed, a notification arrived. (M · Sonnet 5.5 high)

### Ghost Vault, stronger
- [ ] **Argon2id for new vaults** and chunked encryption so big files never sit whole in memory (old vaults keep
  opening as they are). (M · Opus 5.5 xhigh)
- [ ] **Auto-lock** after a set idle time or when the screen locks. (S · Sonnet 5.5 medium)

### Control from anywhere
- [ ] **`luminescent ctl` everything**: every action, page and module from the command line, for scripts and
  other compositors' binds, with shell completions. (S · Sonnet 5.5 medium)
- [ ] **Settings sync folder** (a folder you pick, such as Syncthing or OneDrive) so two PCs share their setup,
  with a conflict view. (M · Opus 5.5 high)

## Still open from IDEAS.md

These were in IDEAS.md and never ticked. Listed again so they are not forgotten; tick them here if you want them
now.

- [ ] Overlay size setting (compact / normal / full screen). (S · Sonnet 5.5 medium)
- [ ] Item colour tags / a custom emoji per dashboard item. (S · Sonnet 5.5 medium)
- [ ] "Last run" time and run count on each dashboard item (tooltip). (S · Haiku 5.5 medium)
- [ ] Follow the system's light / dark mode automatically (pick a theme for each). (S · Sonnet 5.5 medium)
- [ ] A few more built-in themes (Midnight Blue, Forest, Rose, High Contrast). (S · Sonnet 5.5 medium)
- [ ] Number keys 1 to 9 paste the first nine cards while the Clipboard grid is open. (S · Haiku 5.5 medium)
- [ ] Paste as plain text for rich content. (S · Sonnet 5.5 medium)
- [ ] Card size slider in the Clipboard page. (S · Haiku 5.5 medium)
- [ ] Quick-add hotkey: type a line, it goes into a chosen note without opening the panel. (S · Sonnet 5.5 medium)
- [ ] Doodle: spotlight mode (dim everything except a circle around the cursor). (S · Sonnet 5.5 medium)
- [ ] Doodle: redo. (S · Sonnet 5.5 medium)
- [ ] Doodle: pick the colour from the tool strip with the mouse. (S · Sonnet 5.5 medium)
- [ ] Doodle: remember the last tool between sessions. (S · Haiku 5.5 low)
- [ ] Doodle: save the drawing only (transparent PNG). (S · Sonnet 5.5 medium)
- [ ] Timer: +1 min / +5 min buttons on a running timer. (S · Haiku 5.5 medium)
- [ ] Timer: export laps to the clipboard / CSV. (S · Haiku 5.5 medium)
- [ ] Timer: compact mini mode (just the digits). (S · Sonnet 5.5 medium)
- [ ] Timer: repeat automatically when it finishes. (S · Sonnet 5.5 medium)
- [ ] Pause / resume all timers hotkey. (S · Haiku 5.5 medium)
- [ ] Colour picker: more formats (RGBA, CMYK, Qt / C# code). (S · Haiku 5.5 medium)
- [ ] Colour picker: contrast check between two picked colours. (S · Sonnet 5.5 medium)
- [ ] Text grab: translate the grabbed text (needs internet). (M · Sonnet 5.5 high)
- [ ] Reminder actions: run a dashboard item, open a note, start a timer. (S · Sonnet 5.5 medium)
- [ ] Upcoming reminders list in the overlay. (S · Sonnet 5.5 medium)
- [ ] Hearing Guard: tray icon tint when over the limit. (S · Haiku 5.5 medium)
- [ ] "Disable all hotkeys" toggle hotkey. (S · Sonnet 5.5 medium)
- [ ] First-run welcome tour of the overlay and the main hotkeys. (M · Sonnet 5.5 high)
- [ ] Sound pack setting (one set of sounds used everywhere). (S · Sonnet 5.5 medium)
