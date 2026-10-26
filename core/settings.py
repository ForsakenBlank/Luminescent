"""
Settings schema + JSON store.

Every option the launcher and its modules use lives here, described once as a
`Field` so the settings page can build its UI automatically and modules can
read values with `settings.get("doodle.colour")`.

Values are stored in settings.json (next to Launcher.py, or in
~/.local/share/Luminescent for a built binary).  On first run the old config.py (now in
_old/) is imported and its values are migrated.

Settings profiles: settings.json always holds the *active* profile.  The other
profiles are kept in data/profiles/<id>.json and swapped in by
`switch_profile` (a theme can pick the profile it wants).  The launcher-wide
sections in SHARED_SECTIONS are the same in every profile.
"""
import os
import json
import copy
import uuid
import importlib.util
from dataclasses import dataclass, field as dc_field

from PyQt6.QtCore import QObject, pyqtSignal

from core import APP_DIR, HOME_DIR, DATA_DIR, save_json, load_json

SETTINGS_FILE = os.path.join(HOME_DIR, "settings.json")
PROFILES_FILE = os.path.join(DATA_DIR, "profiles.json")
PROFILES_DIR = os.path.join(DATA_DIR, "profiles")
SHARED_SECTIONS = {"general", "ui", "dashboard"}
DEFAULT_PROFILE = "default"


# Field types understood by settings_ui:
#   hotkey  text  dir  file  int  float  opacity  bool  color  choice  custom  widget (not stored)
@dataclass
class Field:
    key: str
    label: str
    type: str
    default: object
    help: str = ""
    choices: list = dc_field(default_factory=list)   # [(value, label), ...] for "choice"
    min: float = 0
    max: float = 9999
    step: float = 1
    file_filter: str = ""                              # for "file"
    restart: bool = False                              # needs restart to fully apply


@dataclass
class Section:
    key: str
    title: str
    icon: str
    description: str
    fields: list


ANCHOR_CHOICES = [
    ("tl", "Top left"), ("tc", "Top centre"), ("tr", "Top right"),
    ("ml", "Middle left"), ("mc", "Centre of screen"), ("mr", "Middle right"),
    ("bl", "Bottom left"), ("bc", "Bottom centre"), ("br", "Bottom right"),
]

SOUND_CHOICES = [
    ("none", "No sound"),
    ("chime", "Chime"),
    ("alarm", "Alarm"),
    ("custom", "Custom sound file"),
]

AUDIO_FILTER = "Audio (*.wav *.mp3 *.ogg *.flac *.m4a);;All files (*)"

NAV_SECTIONS = [("dashboard", "Dashboard"), ("themes", "Themes"), ("clipboard", "Clipboard"), ("settings", "Settings")]
# pages you can add to the overlay's navigation bar (Settings → Overlay → Navigation bar)
EXTRA_PAGES = []   # Notes, Timers, Calculator, Today, Usage pages: not built yet
ALL_PAGES = NAV_SECTIONS + EXTRA_PAGES
REQUIRED_PAGES = ("dashboard", "settings")

# panels that can open by themselves when Luminescent starts (Settings → General → Show on start)
START_PANELS = [("notes", "Notes"), ("hearing", "Hearing Guard")]

# what the tray menu can list (Settings → General → Tray menu)
TRAY_ITEMS = [("open", "Open Luminescent"), ("palette", "Command palette"), ("dashboard", "Dashboard items"),
              ("modules", "Modules submenu"), ("actions", "Module actions (Snippets, Notes, Calculator…)"),
              ("colours", "Recent colours"), ("reset", "Reset stuck keys"), ("settings", "Settings"),
              ("log", "Open log file"), ("restart", "Restart"), ("quit", "Quit")]


SCHEMA = [
    Section("general", "General", "⚙", "Launcher-wide options. These are the same in every settings profile.", [
        Field("run_at_startup", "Run at login", "bool", False,
              "Adds ~/.config/autostart/luminescent.desktop, which UWSM and most sessions start by themselves. "
              "If you also start it from your Hyprland config (exec-once / hl.exec_cmd), remove that so it "
              "does not launch twice."),
        Field("force_quit_hotkey", "Quit everything hotkey", "hotkey", "ctrl+u",
              "Asks for confirmation, then closes Luminescent and every module. The ✕ on the overlay only hides it."),
        Field("settings_hotkey", "Open settings hotkey", "hotkey", "ctrl+alt+s",
              "Opens the overlay straight on the Settings page."),
        Field("tray_messages", "Show notifications", "bool", True,
              "Small notifications when modules are toggled, things are copied or errors happen."),
        Field("notify_style", "Notification style", "choice", "luminescent",
              choices=[("luminescent", "Luminescent cards (top-right, in the theme's colours)"),
                       ("system", "Desktop notifications (Noctalia, mako, dunst…)")],
              help="Reminders and dose warnings always use desktop notifications as well, so they also show up "
                   "in your notification history."),
        Field("start_panels", "Show on start", "custom", ["notes"],
              "Panels that open by themselves when Luminescent starts."),
        Field("tray_items", "Tray menu", "custom", [k for k, _ in TRAY_ITEMS],
              "What the tray icon's right-click menu lists."),
        Field("desktop_entry", "List Luminescent in app launchers and docks", "bool", True,
              "Adds ~/.local/share/applications/luminescent.desktop and its icon, so Noctalia's launcher and dock "
              "show it with the sparkle icon."),
        Field("auto_backup", "Back up automatically once a week", "bool", True,
              "Zips your setup into data/backups (weekly-….zip) and keeps the last 5."),
        Field("pass_keys", "Hotkeys also reach the app you are in", "bool", True,
              "On (as on Windows): Hyprland runs the hotkey and still passes the keys on. Off: Luminescent's "
              "hotkeys are taken away from other apps (Ctrl+B is no longer bold in your editor...).",
              restart=True),
        Field("log_to_file", "Write a log file (data/launcher.log)", "bool", True,
              "Useful for finding out why something went wrong."),
        Field("backup", "Backup", "widget", None),
        Field("quit", "Quit", "widget", None),
    ]),

    Section("ui", "Overlay & Interface", "✦",
            "The Luminescent overlay: a dimmed full-screen hub with the Dashboard, Themes, Clipboard and Settings.", [
        Field("overlay_hotkey", "Open Luminescent", "hotkey", "ctrl+shift+space",
              "Shows the overlay on the Dashboard. Press it again, Esc or ✕ to hide it (it keeps running)."),
        Field("palette_hotkey", "Command palette", "hotkey", "ctrl+alt+space",
              "A search box for every action, dashboard item, note, theme and settings page."),
        Field("cheatsheet_hotkey", "Hotkey cheat sheet", "hotkey", "ctrl+alt+/",
              "Shows every hotkey while held."),
        Field("cheatsheet_toggle", "Cheat sheet: press to show / press again to hide (instead of hold)", "bool", False),
        Field("hotkey_dashboard", "Jump to Dashboard", "hotkey", ""),
        Field("hotkey_themes", "Jump to Themes", "hotkey", ""),
        Field("hotkey_clipboard", "Jump to Clipboard", "hotkey", ""),
        Field("hotkey_settings_page", "Jump to Settings", "hotkey", ""),
        Field("nav_order", "Navigation bar", "custom", [k for k, _ in NAV_SECTIONS],
              "Tick the pages you want in the bar and put them in order. Ctrl+1…9 switch pages while the overlay "
              "is open. Notes, Timers and Calculator pages are the same notes, timers and history as the panels."),
        Field("overlay_screen", "Open the overlay on", "choice", "primary",
              choices=[("primary", "The primary screen"), ("cursor", "The screen with the mouse")]),
        Field("type_to_search", "Type anywhere on the overlay to search", "bool", True,
              "Start typing on the Dashboard or Themes page and the command palette opens with what you typed."),
        Field("show_clock", "Clock and date in the top-left corner", "bool", True),
        Field("show_status", "Status strip (running timers, Hearing Guard, next reminder)", "bool", True),
        Field("blur", "Frosted (blurred) background", "bool", True,
              "Hyprland blurs what is behind the overlay. Off in performance mode."),
        Field("theme_cycle_hotkey", "Next theme hotkey", "hotkey", "",
              "Switches to the next theme in the active theme's folder (or the next of all themes)."),
        Field("overlay_dim", "Dim the screen behind the overlay", "opacity", 0.64),
        Field("click_outside_closes", "Clicking the dimmed area closes the overlay", "bool", True),
        Field("apps_over_overlay", "Apps opened while the overlay is up appear on top of it", "bool", True,
              "Super+E, a dashboard item...: the new window floats, centred, above the overlay, and goes back "
              "into your tiling layout when the overlay closes."),
        Field("performance_mode", "Performance mode", "bool", False,
              "No glow, shadows, fade animations or background images. Use it on a slow PC or while gaming."),
        Field("tray_timers", "Show running timers in the tray tooltip", "bool", True,
              "Hover the tray icon to see what is left. Only updates while a timer runs."),
        Field("wallpaper_engine_exe", "linux-wallpaperengine program", "file", "",
              "Only needed if themes use Wallpaper Engine projects and linux-wallpaperengine is not on PATH.",
              file_filter="All files (*)"),
        Field("terminal_scheme", "Use the Luminescent colours in the terminal", "bool", True,
              "kitty, alacritty and foot windows opened by Luminescent get the theme's colours (passed on the "
              "command line, so your terminal config is not touched)."),
    ]),

    Section("dashboard", "Dashboard & Scripts", "▦", "How dashboard items start.", [
        Field("use_terminal", "Run scripts (.py, .sh...) in a terminal window", "bool", True,
              "Off: they run in the background and their output is not shown."),
        Field("terminal", "Terminal program", "text", "",
              "e.g. kitty, alacritty, foot. Blank: $TERMINAL, else the first one installed."),
        Field("keep_open", "Keep the script's window open after it finishes", "bool", True,
              "So you can read its output and errors."),
        Field("python_path", "Python for scripts", "file", "",
              "Leave blank to use the Python Luminescent runs with (or python3 from PATH for a built binary).",
              file_filter="Python (python python3*);;All files (*)"),
        Field("show_in_tray", "List dashboard items in the tray menu", "bool", True),
        Field("show_icons", "Show program and file icons on items", "bool", True,
              "Taken from the program's app entry, your icon theme or the file type."),
        Field("group_delay_ms", "Pause between the items of a group (ms)", "int", 400, min=0, max=10000),
    ]),

    Section("snippets", "Clipboard (Snippets)", "📋",
            "A folder of files, text and images you can paste with one click. Press the hotkey (or open the "
            "Clipboard page), click a card to paste it. Ctrl+click pastes and presses Enter.", [
        Field("enabled", "Enable Snippets", "bool", True),
        Field("hotkey", "Open hotkey", "hotkey", "ctrl+b"),
        Field("base_path", "Snippets folder", "dir", os.path.join(os.path.expanduser("~"), "Pictures", "Clipboard"),
              "Sub-folders become folder cards. .txt/.md/.py files paste as text; other files are pasted as files."),
        Field("auto_paste", "Paste automatically after clicking", "bool", True,
              "When off, the item is only copied to the clipboard."),
        Field("window_width", "Window width", "int", 960, min=400, max=3000),
        Field("window_height", "Window height", "int", 560, min=300, max=2000),
        Field("card_cols", "Columns in the pop-up", "int", 4, min=1, max=12,
              help="The Clipboard page in the overlay always shows five."),
        Field("name_limit", "Max name length", "int", 20, min=5, max=100),
        Field("overlay_dim", "Dim the screen behind", "opacity", 0.35),
    ]),

    Section("notes", "Notes", "📝", "A floating always-on-top notes / to-do panel.", [
        Field("enabled", "Enable Notes", "bool", True),
        Field("hotkey", "Show / hide hotkey", "hotkey", "alt+n"),
        Field("edit_hotkey", "Edit mode hotkey", "hotkey", "alt+e",
              "Drag the panel by its header any time. Edit mode lets you resize, anchor, add, rename and "
              "delete notes."),
        Field("open_pinned", "Open straight to the pinned note", "bool", True,
              "Showing the panel opens your first pinned note (right-click a note → Pin)."),
        Field("bg_opacity", "Panel opacity", "opacity", 0.85),
        Field("overlay_dim", "Dim the screen in edit mode", "opacity", 0.35),
        Field("markdown", "Markdown formatting (**bold**, # headings, - lists)", "bool", True,
              "Notes are stored as plain text; the 👁 button in a note's header shows it formatted."),
        Field("font_size", "Note font size", "int", 16, min=6, max=40),
        Field("todo_font_size", "To-do font size", "int", 12, min=6, max=40),
        Field("default_width", "Default width", "int", 360, min=150, max=2000),
        Field("default_height", "Default height", "int", 460, min=120, max=2000),
        Field("min_width", "Minimum width", "int", 250, min=100, max=1000),
        Field("min_height", "Minimum height", "int", 180, min=80, max=1000),
        Field("anchor_padding", "Anchor padding (px from screen edge)", "int", 10, min=0, max=200),
    ]),

    Section("doodle", "Screen Doodle", "✏", "Draw on top of everything on screen.", [
        Field("enabled", "Enable Doodle", "bool", True),
        Field("hotkey", "Draw hotkey", "hotkey", "alt",
              "Hold mode: draw while this is held. Toggle mode: press to start/stop drawing. "
              "A bare modifier like alt works well with the two options below."),
        Field("mode", "Mode", "choice", "hold", choices=[("hold", "Hold to draw"), ("toggle", "Toggle on/off")]),
        Field("solo_only", "Only draw when the key is held on its own", "bool", True,
              "Alt+N, Alt+T, Ctrl+Alt+... no longer flash the drawing overlay on: drawing starts after "
              "the key has been held alone for the delay below, and Luminescent's own Alt hotkeys cancel it."),
        Field("solo_delay_ms", "Hold delay before drawing starts (ms)", "int", 120, min=0, max=1000,
              help="0 = instant. 100-150 ms is barely noticeable when you hold the key to draw."),
        Field("clear_on_release", "Clear drawing when the key is released (hold mode)", "bool", False),
        Field("clear_hotkey", "Clear hotkey", "hotkey", "alt+x"),
        Field("undo_hotkey", "Undo hotkey", "hotkey", "alt+f"),
        Field("bg_hotkey", "Toggle blackboard background hotkey", "hotkey", "alt+g"),
        Field("exit_hotkey", "Panic hotkey (clear + stop drawing)", "hotkey", "alt+shift+q",
              "Esc also stops drawing at any time."),
        Field("copy_hotkey", "Copy screen + drawing hotkey", "hotkey", "alt+c",
              "Copies a screenshot of every screen with your drawing on top to the clipboard."),
        Field("save_copies", "Also save those screenshots to ~/Pictures/Luminescent", "bool", False),
        Field("show_tools", "Show the tool strip while drawing", "bool", True,
              "Pen, highlighter, line, rectangle, circle, arrow, text and the fill toggle. Hold Shift while "
              "dragging the pen for a straight line."),
        Field("highlighter_opacity", "Highlighter opacity", "opacity", 0.35),
        Field("text_size", "Text tool font size", "int", 28, min=8, max=200),
        Field("wheel_size", "Mouse wheel changes the brush size while drawing", "bool", True),
        Field("brush_style", "Brush", "choice", "smooth",
              choices=[("smooth", "Smooth (anti-aliased)"), ("pixel", "Pixel (MS Paint style)")]),
        Field("colour", "Colour", "color", "#FF0000"),
        Field("brush_size", "Brush size", "int", 4, min=1, max=60),
        Field("pixel_size", "Pixel size", "int", 4, min=1, max=60),
        Field("bg_opacity", "Blackboard opacity", "opacity", 1.0),
        Field("passive_opacity", "Tint while drawing", "opacity", 0.01,
              "A very faint tint so you can tell drawing mode is on."),
        Field("undo_levels", "Undo levels", "int", 30, min=1, max=200),
    ]),

    Section("timer", "Timer / Stopwatch", "⏱", "Floating timers and stopwatches.", [
        Field("enabled", "Enable Timer", "bool", True),
        Field("new_hotkey", "New timer hotkey", "hotkey", "alt+t"),
        Field("edit_hotkey", "Edit mode hotkey", "hotkey", "alt+r",
              "Edit mode shows reset / switch-mode buttons and the anchor bar."),
        Field("lap_hotkey", "Lap hotkey", "hotkey", "",
              "Records a lap on the stopwatch you started most recently."),
        Field("finish_sound", "Sound when a timer finishes", "choice", "alarm", choices=SOUND_CHOICES),
        Field("finish_sound_file", "Custom sound file", "file", "", file_filter=AUDIO_FILTER),
        Field("flash_on_finish", "Flash the panel when a timer finishes", "bool", True),
        Field("progress", "Countdown progress", "choice", "ring",
              choices=[("ring", "A ring beside the time"), ("bar", "A thin bar along the bottom"), ("none", "None")]),
        Field("restore_on_start", "Restore open timers on start", "bool", True),
        Field("bg_opacity", "Panel opacity", "opacity", 0.85),
        Field("default_width", "Default width", "int", 260, min=120, max=2000),
        Field("default_height", "Default height", "int", 160, min=80, max=2000),
        Field("min_width", "Minimum width", "int", 180, min=100, max=1000),
        Field("min_height", "Minimum height", "int", 120, min=60, max=1000),
        Field("anchor_padding", "Anchor padding", "int", 12, min=0, max=200),
    ]),

    Section("calculator", "Calculator", "🧮",
            "A floating calculator with history. Pin a result and give it a name, then use that name in other "
            "sums (e.g. pin 950*12 as rent, then type rent/4).", [
        Field("enabled", "Enable Calculator", "bool", True),
        Field("hotkey", "Show / hide hotkey", "hotkey", "ctrl+alt+=",
              "Tip: Enter calculates, ↑/↓ walk through history, Ctrl+C with an empty box copies the last result."),
        Field("bg_opacity", "Panel opacity", "opacity", 0.92),
        Field("decimals", "Maximum decimal places", "int", 10, min=0, max=15),
        Field("thousands", "Show thousands separators (1,234,567)", "bool", True),
        Field("angle", "Angles", "choice", "deg", choices=[("deg", "Degrees"), ("rad", "Radians")]),
        Field("history_limit", "History length", "int", 200, min=10, max=2000),
        Field("currency", "Fetch currency rates once a day", "bool", True,
              "For sums like 20 usd to eur. Rates come from the European Central Bank (frankfurter.app); "
              "nothing else is sent."),
    ]),

    Section("colorpicker", "Colour Picker", "🎯",
            "Freeze the screen, zoom in with a loupe and click a pixel to copy its colour. Arrow keys nudge "
            "the cursor 1 px, Esc or right-click cancels.", [
        Field("enabled", "Enable Colour Picker", "bool", True),
        Field("hotkey", "Pick a colour hotkey", "hotkey", "ctrl+alt+c"),
        Field("format", "Copy as", "choice", "hex",
              choices=[("hex", "HEX  #7B6CF6"), ("rgb", "RGB  rgb(123, 108, 246)"), ("hsl", "HSL  hsl(246, 89%, 69%)")]),
        Field("uppercase", "Upper-case hex", "bool", True),
    ]),

    Section("textgrab", "Screen Text Grab", "🔤",
            "Drag a box around any text on screen and it is copied to the clipboard. Uses Tesseract: "
            "sudo pacman -S tesseract tesseract-data-eng (add tesseract-data-<lang> for other languages).", [
        Field("enabled", "Enable Text Grab", "bool", True),
        Field("hotkey", "Grab text hotkey", "hotkey", "ctrl+alt+o"),
        Field("keep_lines", "Keep line breaks", "bool", True),
        Field("upscale", "Enlarge small selections first (better accuracy)", "bool", True),
        Field("review", "Show the text in an editable box before copying", "bool", False,
              "Fix OCR mistakes first, then Copy (Ctrl+Enter)."),
    ]),

    Section("promptlog", "Prompt Log", "⌨",
            "Browse, search and delete your fish, bash or zsh command history, plus everything the Dashboard "
            "launched.", [
        Field("enabled", "Enable Prompt Log", "bool", True),
        Field("hotkey", "Open hotkey", "hotkey", ""),
        Field("dedupe", "Hide repeated commands (newest kept)", "bool", True),
    ]),

    Section("keybinds", "Keybinds", "⌨",
            "Every keybind on the system in one list: your Hyprland binds (with what they really run) and "
            "Luminescent's hotkeys, clashes in red. Add, re-key or remove Hyprland binds; your config file gets "
            "the change (a backup goes to data/hypr-backups) and Hyprland reloads it.", [
        Field("enabled", "Enable Keybinds", "bool", True),
        Field("hotkey", "Open hotkey", "hotkey", "ctrl+alt+k"),
    ]),

    Section("vault", "Ghost Vault", "🔒", "Encrypted file storage with a real password, a decoy password and a recovery key.", [
        Field("enabled", "Enable Vault", "bool", True),
        Field("hotkey", "Open hotkey", "hotkey", "ctrl+alt+q"),
        Field("storage_path", "Vault folder (encrypted blobs live here)", "dir",
              os.path.join(os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share"), ".idx"),
              "Must be writable by your user (no sudo). Not under ~/.cache: cleaners empty that."),
        Field("decrypt_output", "Decrypted files go to", "dir",
              os.path.join(os.path.expanduser("~"), "Documents", "VaultDecrypted")),
        Field("lock_minutes", "Auto-lock after (minutes of inactivity)", "int", 5, min=1, max=240),
        Field("secure_delete_passes", "Overwrite passes on secure delete", "int", 3, min=1, max=10),
    ]),

    Section("metadata", "Metadata Stripper", "🧹", "View and remove metadata from images and videos.", [
        Field("enabled", "Enable Metadata Stripper", "bool", True),
        Field("hotkey", "Open hotkey", "hotkey", "ctrl+shift+m"),
        Field("save_dir", "Save stripped files to", "dir",
              os.path.join(os.path.expanduser("~"), "Documents", "MetaStripped")),
        Field("jpeg_quality", "JPEG quality when re-saving", "int", 95, min=50, max=100),
    ]),

    Section("hearing", "Hearing Guard", "🎧",
            "Estimates how loud your audio is at the ear (A-weighted, like a sound-level meter), keeps it under a dB "
            "limit, and tracks your daily safe listening allowance (NIOSH: 85 dB(A) for 8 h; every +3 dB halves the "
            "time).", [
        Field("enabled", "Enable Hearing Guard", "bool", True),
        Field("hotkey", "Show / hide panel hotkey", "hotkey", "ctrl+alt+h"),
        Field("mode", "Mode", "choice", "limit",
              choices=[("limit", "Limit: dip the volume while over the limit"),
                       ("monitor", "Monitor: only warn")]),
        Field("limit_db", "Limit (estimated dB)", "int", 85, "85 dB(A) is the NIOSH recommended maximum for 8 hours a day.",
              min=60, max=110),
        Field("restore_volume", "Dip the apps, then bring them back when the loud part is over", "bool", True,
              "On: the apps playing (not the master volume, so no volume pop-up) are turned down only while "
              "something is over the limit, and come back a second or two later, never above the volume you set. "
              "Off: Hearing Guard never turns anything up: it lowers the master volume and leaves it there until "
              "you turn it up yourself."),
        Field("max_volume", "Never let master volume go above (%)", "int", 100,
              "A hard cap that needs no calibration.", min=5, max=100),
        Field("calibration", "Default calibration: dB at 100 % volume with a full-scale tone", "int", 105,
              "Used for output devices you have not calibrated. Calibrate… in the panel measures against a phone "
              "sound-meter app and stores a value for each device; 100-115 is typical for headphones.", min=60, max=140),
        Field("warn_dose", "Notify at 50 %, 80 % and 100 % of the daily allowance", "bool", True),
        Field("meter_includes_volume", "Captured audio already includes the master volume", "bool", False,
              "Leave off. PipeWire records the output's monitor before the master volume, so the volume is added on "
              "top. Turn on only if lowering the volume by 6 dB makes the estimate drop by about 12 dB (PipeWire "
              "set up with monitor.channel-volumes = true)."),
    ]),

    Section("reminders", "Reminders", "⏰",
            "Pop-ups, sounds, notifications and theme switches at a set time: once, daily, weekly or every N minutes.", [
        Field("enabled", "Enable Reminders", "bool", True),
        Field("items", "Reminders", "custom", []),
        Field("popup_position", "Default pop-up position", "choice", "br", choices=ANCHOR_CHOICES),
        Field("popup_screen", "Show pop-ups on", "choice", "primary",
              choices=[("primary", "Primary screen"), ("cursor", "Screen with the mouse cursor")]),
        Field("snooze_minutes", "Snooze length (minutes)", "int", 10, min=1, max=240),
        Field("popup_timeout", "Auto-close pop-ups after (seconds, 0 = stay until dismissed)", "int", 0, min=0, max=3600),
        Field("default_sound", "Default sound", "choice", "chime", choices=SOUND_CHOICES),
        Field("default_sound_file", "Default custom sound file", "file", "", file_filter=AUDIO_FILTER),
        Field("catch_up_minutes", "Fire missed reminders if less than (minutes) late", "int", 120,
              "If the PC was asleep or off when a reminder was due, it still fires on wake unless it is later "
              "than this.", min=0, max=1440),
    ]),
]

SECTIONS = {s.key: s for s in SCHEMA}


def defaults() -> dict:
    return {s.key: {f.key: copy.deepcopy(f.default) for f in s.fields if f.type != "widget"} for s in SCHEMA}


# Old config.py constant  ->  new "section.key"
_OLD_MAP = {
    "PRIMARY": "appearance.primary", "PRIMARY_HOVER": "appearance.primary_hover",
    "PRIMARY_MUTED": "appearance.primary_muted", "BG_BASE": "appearance.bg_base",
    "BG_CARD": "appearance.bg_card", "BG_CARD_HOVER": "appearance.bg_card_hover",
    "BG_FRAME": "appearance.bg_frame", "BG_HEADER": "appearance.bg_header",
    "BORDER": "appearance.border", "TEXT_PRIMARY": "appearance.text_primary",
    "TEXT_SECONDARY": "appearance.text_secondary", "TEXT_MUTED": "appearance.text_muted",
    "FOLDER_COLOR": "appearance.folder_color", "FOLDER_BORDER": "appearance.folder_border",
    "RADIUS": "appearance.radius", "FONT_FAMILY": "appearance.font_family",
    "FORCE_QUIT_HOTKEY": "general.force_quit_hotkey",

    "HOTKEY": "snippets.hotkey", "BASE_PATH": "snippets.base_path",
    "WINDOW_WIDTH": "snippets.window_width", "WINDOW_HEIGHT": "snippets.window_height",
    "CARD_COLS": "snippets.card_cols", "FOLDER_CARD_HEIGHT": "snippets.folder_card_height",
    "IMAGE_CARD_HEIGHT": "snippets.image_card_height", "TEXT_CARD_HEIGHT": "snippets.text_card_height",
    "TEXT_PREVIEW_FONT_SIZE": "snippets.text_preview_font_size",
    "FILE_NAME_FONT_SIZE": "snippets.file_name_font_size", "NAME_LIMIT": "snippets.name_limit",

    "NOTE_HOTKEY": "notes.hotkey", "NOTE_EDIT_HOTKEY": "notes.edit_hotkey",
    "NOTE_ANCHOR_PADDING": "notes.anchor_padding", "NOTE_BG_OPACITY": "notes.bg_opacity",
    "NOTE_OVERLAY_DIM": "notes.overlay_dim", "NOTE_MIN_WIDTH": "notes.min_width",
    "NOTE_MIN_HEIGHT": "notes.min_height", "NOTE_DEFAULT_WIDTH": "notes.default_width",
    "NOTE_DEFAULT_HEIGHT": "notes.default_height", "NOTE_FONT_SIZE": "notes.font_size",
    "TODO_LABEL_FONT_SIZE": "notes.todo_font_size",

    "DOODLE_HOTKEY": "doodle.hotkey", "DOODLE_CLEAR_HOTKEY": "doodle.clear_hotkey",
    "DOODLE_BG_HOTKEY": "doodle.bg_hotkey", "DOODLE_UNDO_HOTKEY": "doodle.undo_hotkey",
    "DOODLE_EXIT_HOTKEY": "doodle.exit_hotkey", "DOODLE_MODE": "doodle.mode",
    "DOODLE_BRUSH_STYLE": "doodle.brush_style", "DOODLE_COLOUR": "doodle.colour",
    "DOODLE_BRUSH_SIZE": "doodle.brush_size", "DOODLE_PIXEL_SIZE": "doodle.pixel_size",
    "DOODLE_BG_OPACITY": "doodle.bg_opacity", "DOODLE_PASSIVE_OPACITY": "doodle.passive_opacity",

    "TIMER_NEW_HOTKEY": "timer.new_hotkey", "TIMER_EDIT_HOTKEY": "timer.edit_hotkey",
    "TIMER_MIN_WIDTH": "timer.min_width", "TIMER_MIN_HEIGHT": "timer.min_height",
    "TIMER_DEFAULT_WIDTH": "timer.default_width", "TIMER_DEFAULT_HEIGHT": "timer.default_height",
    "TIMER_BG_OPACITY": "timer.bg_opacity", "TIMER_ANCHOR_PADDING": "timer.anchor_padding",

    "VAULT_HOTKEY": "vault.hotkey", "VAULT_DECRYPT_OUTPUT": "vault.decrypt_output",
    "VAULT_STORAGE_PATH": "vault.storage_path",

    "META_HOTKEY": "metadata.hotkey", "META_SAVE_DIR": "metadata.save_dir",
}


def _migrate_old_config(data: dict, raw: dict) -> bool:
    """Pull values out of the pre-rewrite config.py if it exists. Returns True if anything was migrated.

    Retired sections (appearance → themes.json, file_launcher → dashboard.json) go into `raw`,
    where those migrations look for them.
    """
    for candidate in (os.path.join(APP_DIR, "_old", "config.py"), os.path.join(APP_DIR, "config.py")):
        if not os.path.exists(candidate):
            continue
        try:
            spec = importlib.util.spec_from_file_location("_old_config", candidate)
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
        except Exception:
            continue
        migrated = False
        for old, new in _OLD_MAP.items():
            if hasattr(mod, old):
                sec, key = new.split(".")
                (data[sec] if sec in data else raw.setdefault(sec, {}))[key] = getattr(mod, old)
                migrated = True
        if hasattr(mod, "IMAGE_SIZE"):
            try:
                data["snippets"]["image_width"], data["snippets"]["image_height"] = mod.IMAGE_SIZE
            except Exception:
                pass
        if hasattr(mod, "FILE_SHORTCUTS") and isinstance(mod.FILE_SHORTCUTS, dict):
            raw.setdefault("file_launcher", {})["shortcuts"] = [{"hotkey": k, "path": v} for k, v in mod.FILE_SHORTCUTS.items()]
        return migrated
    return False


def _merge(data: dict, saved: dict, only=None):
    """Copy known keys from `saved` into `data` (unknown sections/keys are dropped)."""
    for sec, vals in (saved or {}).items():
        if sec in data and isinstance(vals, dict) and (only is None or sec in only):
            for k, v in vals.items():
                if k in data[sec]:
                    data[sec][k] = v


def _migrate(data: dict, saved: dict):
    """Settings that moved: the per-module "show on start" boxes became General → Show on start."""
    if "start_panels" not in (saved.get("general") or {}):
        panels = []
        if (saved.get("notes") or {}).get("show_on_start", True):
            panels.append("notes")
        if (saved.get("hearing") or {}).get("show_on_start", False):
            panels.append("hearing")
        data["general"]["start_panels"] = panels


class Settings(QObject):
    """Loads/saves settings.json and notifies listeners when values change."""
    changed = pyqtSignal()
    profile_changed = pyqtSignal(str)
    quietly_changed = pyqtSignal()       # a module stored something (a reminder...) without re-applying everything

    def __init__(self, path: str = SETTINGS_FILE):
        super().__init__()
        self.path = path
        self._data = defaults()
        self.raw_saved: dict = {}      # the file as read at start-up, including retired sections
        self.migrated_from_old_config = False
        self.load()

    # ── persistence ────────────────────────────────────────────────────────
    def load(self):
        data = defaults()
        if os.path.exists(self.path):
            try:
                with open(self.path, "r", encoding="utf-8") as f:
                    saved = json.load(f)
                if isinstance(saved, dict):
                    self.raw_saved = saved
                    _merge(data, saved)
                    _migrate(data, saved)
            except Exception:
                pass
            self._data = data
        else:
            raw = {}
            self.migrated_from_old_config = _migrate_old_config(data, raw)
            self.raw_saved = raw
            self._data = data
            self.save(emit=False)

    def save(self, emit: bool = True):
        save_json(self.path, self._data, indent=2)
        if emit:
            self.changed.emit()

    def save_quiet(self):
        """Store a value a module changed itself (reminders.items...): no settings_applied round, but the
        Settings page reloads it so its next Save does not put the old value back."""
        save_json(self.path, self._data, indent=2)
        self.quietly_changed.emit()

    # ── access ─────────────────────────────────────────────────────────────
    def get(self, dotted: str, default=None):
        sec, _, key = dotted.partition(".")
        try:
            return self._data[sec][key]
        except KeyError:
            return default

    def set(self, dotted: str, value):
        sec, _, key = dotted.partition(".")
        self._data.setdefault(sec, {})[key] = value

    def section(self, key: str) -> dict:
        return self._data.get(key, {})

    def all(self) -> dict:
        return copy.deepcopy(self._data)

    def replace(self, data: dict):
        """Replace everything (used by the settings page on save) and notify listeners."""
        self._data = copy.deepcopy(data)
        self.save()

    def __getitem__(self, dotted: str):
        return self.get(dotted)

    # ── profiles ───────────────────────────────────────────────────────────
    def _profiles_meta(self) -> dict:
        meta = load_json(PROFILES_FILE, None)
        if not isinstance(meta, dict) or not isinstance(meta.get("profiles"), list):
            meta = {"active": DEFAULT_PROFILE, "profiles": [{"id": DEFAULT_PROFILE, "name": "Default"}]}
        if not any(p.get("id") == DEFAULT_PROFILE for p in meta["profiles"]):
            meta["profiles"].insert(0, {"id": DEFAULT_PROFILE, "name": "Default"})
        if not any(p.get("id") == meta.get("active") for p in meta["profiles"]):
            meta["active"] = DEFAULT_PROFILE
        return meta

    def profiles(self) -> list[dict]:
        return [dict(p) for p in self._profiles_meta()["profiles"]]

    def active_profile(self) -> str:
        return self._profiles_meta()["active"]

    def profile_name(self, pid: str) -> str:
        for p in self.profiles():
            if p["id"] == pid:
                return p.get("name") or pid
        return "Default"

    @staticmethod
    def _profile_path(pid: str) -> str:
        return os.path.join(PROFILES_DIR, f"{pid}.json")

    def _own_sections(self) -> dict:
        return {k: copy.deepcopy(v) for k, v in self._data.items() if k not in SHARED_SECTIONS}

    def create_profile(self, name: str) -> str:
        """A new profile starting as a copy of the current settings."""
        meta = self._profiles_meta()
        pid = uuid.uuid4().hex[:10]
        os.makedirs(PROFILES_DIR, exist_ok=True)
        save_json(self._profile_path(pid), self._own_sections(), indent=2)
        meta["profiles"].append({"id": pid, "name": name.strip() or "Profile"})
        save_json(PROFILES_FILE, meta, indent=2)
        return pid

    def rename_profile(self, pid: str, name: str):
        meta = self._profiles_meta()
        for p in meta["profiles"]:
            if p["id"] == pid and name.strip():
                p["name"] = name.strip()
        save_json(PROFILES_FILE, meta, indent=2)

    def delete_profile(self, pid: str):
        if pid == DEFAULT_PROFILE:
            return
        if self.active_profile() == pid:
            self.switch_profile(DEFAULT_PROFILE)
        meta = self._profiles_meta()
        meta["profiles"] = [p for p in meta["profiles"] if p["id"] != pid]
        save_json(PROFILES_FILE, meta, indent=2)
        try:
            os.remove(self._profile_path(pid))
        except OSError:
            pass

    def switch_profile(self, pid: str, emit: bool = True) -> bool:
        """Store the current settings in the active profile and load `pid`. Returns True if anything changed."""
        meta = self._profiles_meta()
        pid = pid or DEFAULT_PROFILE
        if pid == meta["active"] or not any(p["id"] == pid for p in meta["profiles"]):
            return False
        os.makedirs(PROFILES_DIR, exist_ok=True)
        save_json(self._profile_path(meta["active"]), self._own_sections(), indent=2)
        stored = load_json(self._profile_path(pid), None)
        if isinstance(stored, dict):
            data = defaults()
            _merge(data, {k: v for k, v in self._data.items() if k in SHARED_SECTIONS})
            _merge(data, stored, only={k for k in data if k not in SHARED_SECTIONS})
            self._data = data
        meta["active"] = pid
        save_json(PROFILES_FILE, meta, indent=2)
        self.save(emit=emit)
        self.profile_changed.emit(pid)
        return True
