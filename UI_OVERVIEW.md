# Luminescent: UI overview (brief for a UI redesign)

Luminescent is a Windows 11 tray app (Python 3.13 + **PyQt6**, no web views) that keeps lots of small tools a hotkey
away. This file lists every screen and where its look lives, so the whole thing can get one consistent, proper design.

**Goal:** a modern, professional look with a slight "hacker" edge (monospace accents, thin accent lines), inspired by
the GeForce Experience overlay: a translucent dimmed full screen.

**The 2026-10-25 redesign** (mock-ups in `Luminescent UI mockups/`) is built: Geist + Geist Mono (bundled in
`assets/fonts`, loaded by `theme.load_fonts()`), the sparkle logo (`theme.sparkle_path`, PNGs in `assets/`), new
Dark/Light tokens incl. `primary_text` (accent-coloured text), and helpers `theme.qfont(px, weight, mono, spacing)`
and `widgets.label(text, px, weight, mono, spacing, tone)`; label colours come from the `tone` property
(primary/secondary/muted/accent/accent2/danger) so they restyle live. Qt QSS has no letter-spacing: use `qfont`.

## Hard constraints

- **Performance comes first.** No `QGraphicsEffect` (shadows/blur) on big widgets, because it re-renders every child
  on every repaint. Glow is painted by hand once (see `_Backdrop.paintEvent` in `ui/overlay.py`). No timers that run
  while the UI is hidden. Animations are short (≈120 ms) and must turn off in **Performance mode**
  (`settings.get("ui.performance_mode")`).
- **Everything is themeable.** Use the palette tokens in `core/theme.py` (`theme.P.primary`, `bg_base`, `bg_card`,
  `bg_card_hover`, `bg_frame`, `bg_header`, `border`, `text_primary/secondary/muted`, `primary_hover`,
  `primary_muted`, `on_primary`, `folder_color/border`, plus `radius`, `font_family`, `glow`, `background_image`,
  `image_opacity`). Never hard-code colours. There are two built-in themes, Dark (dark grey + blue-purple
  `#7B6CF6`) and Light (white + yellow `#F2B705`), and users can make their own.
- **Live theme switching.** The best place for styles is the global stylesheet `theme.stylesheet()`, using object
  names / dynamic properties (`#tabBox`, `QPushButton[nav="true"]`, `[variant="primary"|"ghost"|"danger"]`), because
  those restyle instantly. Widgets that bake colours into f-strings are rebuilt on theme change via each module's
  `retheme()`.
- **QSS gotchas** (already hit): an ancestor pseudo-state like `#row:hover #child` applies *always*; the CSS
  border-triangle trick draws a grey bar (arrows are generated PNGs, `theme._arrow`); `QScrollArea` viewports paint
  grey unless made transparent; don't set `font-size` on the global `QWidget` rule (it breaks `app.setFont`).
- Floating panels are frameless, always-on-top, translucent windows (`core/widgets.FloatingPanel`). Overlays must
  never steal focus unless opened by the user.

## Shared building blocks (`core/widgets.py`, `core/theme.py`)

- `FloatingPanel`: rounded translucent panel with a header (accent bar, title, header buttons, ✕), drag-by-header,
  edge resize, and an "EDIT" look. Used by Notes, Timer, Calculator, Prompt Log, Hearing, Metadata, Vault, Snippets.
- `DimOverlay`, `AnchorBar` (9 snap-position buttons), `icon_button`, `button(variant)`, styled `confirm` /
  `ask_text` / `menu` dialogs.
- The global stylesheet styles every standard widget: buttons, inputs, combos, spin boxes, checkboxes, lists,
  scrollbars, sliders, menus, tooltips.

## Screens

### 1. Main overlay (`ui/overlay.py`), opened with Ctrl+Shift+Space or a tray click
Full screen, game-bar style: every screen is dimmed with a dot grid (optional theme background image), all
painted once into a cached pixmap. The primary screen carries the logo top-left, the **page switcher** hanging from
the top-centre edge (2 px accent line over pills: Dashboard · Themes · Clipboard · Settings, order user-set, with a
soft glow behind it) and the hotkey hint + round ✕ top-right. The Dashboard floats on the whole screen; the other
pages sit in a centred panel (≤1180×780) below the switcher. Pages are built lazily. Esc, ✕ or a click on empty
space hides it.

- **Dashboard** (`ui/dashboard.py`): **TabBoxes** floating on the screen (x/y are screen pixels since
  dashboard.json `"layout": 2`), with a toolbar pill at the bottom centre (＋ New tab, Tidy up, hint). Each box has a
  header (accent bar, mono caps title, zero-padded count, fold chevron) and rows (APP/DIR/URL/CMD/FILE/PY/TOOL
  badge, name, monospace hotkey hint). Boxes are dragged by the header (8 px grid), folded with a click on the header
  and resized from the right edge. The page also has right-click menus and an **ItemDialog** (name, path + File…/
  Folder…, arguments, hotkey recorder).
- **Themes** (`ui/themes_page.py`): theme cards (preview picture or a painted mini mock-up, plus the name; the
  active card has an accent border and ✓) grouped under collapsible, colour-coded folder headers. The
  **ThemeDialog** has a swatch button for each colour role, a font, radius, glow, background image + opacity
  slider, a card icon, a wallpaper and a settings profile, with a live preview on the right.
- **Clipboard** (`modules/snippets.py` → `SnippetsView`): a breadcrumb + action buttons over a card grid (folder,
  image/video/link thumbnail, text preview, file). Right-click menus. The same view is used in the Ctrl+B pop-up.
- **Settings** (`settings_ui.py` → `SettingsPage`): a settings-profile bar, a left sidebar (search box + sections),
  and pages generated from `core/settings.SCHEMA` (label column + field widgets: hotkey recorder, path picker, spin
  box, opacity slider, colour button, combo, checkbox, nav-order list, backup/quit buttons, reminders editor), with
  a bottom bar (Reset page · status · Undo changes · Save & Apply).

### 2. Pop-ups
- **Command palette** (`ui/palette.py`, Ctrl+Alt+Space): a search box plus a list with a custom-painted delegate
  (label, monospace hint on the right, accent bar on the selected row).
- **Hotkey cheat sheet** (`ui/cheatsheet.py`, hold Ctrl+Alt+/): full-screen dim with a grid of cards, one per
  module (label and monospace hotkey).
- **Colour picker** (`modules/colorpicker.py`): a frozen screenshot with a loupe (11×11 zoom grid, crosshair cell,
  swatch, HEX and RGB).
- **Text grab** (`modules/textgrab.py`): a dimmed frozen screenshot with a drag rectangle and a hint line.
- **Reminder pop-up** (`modules/reminders.py` → `ReminderPopup`): a corner card with Snooze / Dismiss.

### 3. Floating tool panels
- **Notes** (`modules/notes.py`): home list (search, note cards with icon, name and type/progress badge, pinned
  first), a plain-text editor, a to-do list (check boxes, add box, clear done), and edit mode (side buttons, anchor
  bar, dimmed screen).
- **Timer / Stopwatch** (`modules/timer.py`): a big time label, ▶/⏸ + Lap, a monospace lap list, and edit-mode
  buttons (Reset, switch mode). There can be several of these.
- **Calculator** (`modules/calculator.py`): pinned list, history list (sum + result), live preview line, a large
  input, and an optional 5×5 keypad.
- **Prompt Log** (`modules/promptlog.py`): source combo, "Hide repeats", search, a monospace command list, and
  action buttons.
- **Hearing Guard** (`modules/hearing.py`): a big dB readout, level bar, painted charts (safe-time curve, today's
  levels), mode controls and a calibration dialog.
- **Ghost Vault** (`modules/vault.py`): password dialogs, file-card grid, breadcrumb, drop zone, progress.
- **Metadata Stripper** (`modules/metadata.py`): drop area, preview, metadata table, action buttons.
- **Screen Doodle** (`modules/doodle.py`): a transparent full-screen canvas while drawing, a painted **tool strip**
  (pen / line / rect / circle / arrow) at the top of the primary screen, a circle cursor at the real brush size, and
  a "N px" label when the wheel changes size.

### 4. Tray
The tray icon is the sparkle mark drawn in code (`Launcher.star_pixmap` → `theme.sparkle_pixmap`) in the theme
accent on a transparent background. It has a context menu (Open, palette, Dashboard submenu, quick actions, Modules
submenu, recent colours, Settings, Restart, Quit) and a tooltip that shows running timers.

## Seeing your changes

- `python tools/shots.py . <out dir> dark|light` renders the overlay pages and the main panels to PNGs without
  anything appearing on screen. Run it with `LUMINESCENT_HOME=<scratch folder holding a copy of settings.json>` and
  `LUMINESCENT_NO_HOOK=1`, so it uses separate data and installs no keyboard hook. Test both themes.
- `python Launcher.py --smoke` (same env vars, plus `QT_QPA_PLATFORM=offscreen`) builds every page once and exits
  with 0 on success.
- The user's real copy is reloaded with tray → **Restart**. The project is not in git: snapshot the files you change
  into `_old/before-<date>/` first.
- Behaviour changes need a Settings toggle. Offer new feature ideas as questions rather than building them unasked.
