# Platform matrix

How each part works on each kind of desktop. "Hyprland" is the main Linux target (tested on CachyOS), "Other
Linux" covers wlroots compositors (Sway, river), KDE, GNOME and X11, and "Windows" is Windows 11. Cells say how
the Rust app does it; **bold** marks something 1.9.21 can not do today that the Rust plan adds (mostly phase 16).

## Core

| Part | Hyprland | Other Linux | Windows |
|---|---|---|---|
| Global hotkeys | Runtime binds through LUMI Lua, callbacks through a FIFO | **`luminescent ctl` from your own binds (phase 3)**; **X11 key grabs, the GlobalShortcuts portal on KDE / GNOME (phase 16)**; none otherwise (tray works) | One `WH_KEYBOARD_LL` hook on its own thread |
| Hold keys (Doodle's Alt) | Press + release binds, non-consuming, 100 ms "still down?" check | X11 grabs (phase 16); otherwise not available | Hook + 100 ms watchdog with `GetAsyncKeyState` |
| Mouse-button binds | Hyprland mouse binds | X11 grabs (phase 16) | Low-level mouse hook |
| Window placement | Socket `movewindowpixel` after open, title markers, LUMI rules | X11: native; Wayland: compositor decides; **layer-shell surfaces for overlays (phase 16)** | Native positions |
| No focus stealing | Rules from title markers | X11: `_NET_WM` hints; Wayland: varies | `WS_EX_NOACTIVATE` |
| Click-through | Empty input region (winit `set_cursor_hittest(false)`) | Same | `WS_EX_TRANSPARENT` + layered |
| Always on top / pinned | Pin rule; NO_PIN for the overlay backdrops | X11 hint; Wayland: varies | `HWND_TOPMOST` |
| Frosted background | BLUR / GLASS markers → Hyprland blur rule | KWin blur hint on KDE (phase 16) | DWM acrylic / mica (`DwmSetWindowAttribute`) |
| Tray | StatusNotifierItem (`ksni`): needs a tray host such as Caelestia's bar or waybar | Same; GNOME needs the AppIndicator extension | `tray-icon` |
| Notifications | D-Bus (`notify-rust`): Noctalia, mako, dunst, Caelestia | Same | Windows toasts |
| Autostart | `~/.config/autostart/luminescent.desktop` | Same | `HKCU\...\Run` |
| Keep awake | logind inhibit over D-Bus | Same | `SetThreadExecutionState` |
| Idle time | `ext-idle-notify` Wayland protocol, or Hyprland events | Same protocol; X11 `XScreenSaver` | `GetLastInputInfo` |
| Clipboard read / write | `wl-clipboard-rs` / `arboard` | X11 via `arboard` | `arboard` |
| Clipboard change trigger | data-control protocol | X11 XFixes | `AddClipboardFormatListener` |
| Paste into the app you were using | `sendshortcut` through the socket | `wtype` / `ydotool`; X11 XTest | `SendInput` |
| Type text / press keys (automation) | `wtype` or `ydotool`; **virtual-keyboard protocol (phase 16)** | Same; X11 XTest | `SendInput` |
| Launching items | Rules from `core/runner.py`; terminals kitty, alacritty, foot, wezterm, konsole, gnome-terminal… | Same | Windows Terminal tab or console; `ShellExecuteExW` with `runas` for admin |
| Run as administrator | pkexec (programs), `sudo` in the terminal (scripts) | Same | UAC prompt |
| Program and file icons | freedesktop icon themes + .desktop entries | Same | `SHGetFileInfoW` / `IShellItemImageFactory` |
| File manager menu | Dolphin service menu | Dolphin; Nautilus scripts (phase 16) | Explorer context menu (registry) |

## Screen tools

| Part | Hyprland | Other Linux | Windows |
|---|---|---|---|
| Screen capture (picker, ruler, text grab, doodle copy) | `xcap` or wlr-screencopy; `grim` fallback | wlroots: screencopy; KDE / GNOME: screenshot portal (phase 16); X11: `xcap` | `xcap` (DXGI) |
| Cursor position outside our windows | Socket `cursorpos` | X11: query pointer; Wayland: only inside our windows | `GetCursorPos` |
| OCR | Tesseract (CLI); **`ocrs` built in (phase 16)** | Same | Windows OCR (`Windows.Media.Ocr`) |
| Window snapper | float + move / resize through the socket; back into tiling | X11: EWMH moves; Wayland: not available | `SetWindowPos` in the work area |

## System

| Part | Hyprland | Other Linux | Windows |
|---|---|---|---|
| CPU, memory, disks, network, sensors | `sysinfo` + `/proc`, hwmon | Same | `sysinfo` |
| NVIDIA GPU + per-process | NVML (`nvidia-smi` fallback) | Same | NVML |
| AMD / Intel GPU + per-process | `/sys/class/drm`, hwmon, DRM fdinfo | Same | PDH `GPU Engine` / `GPU Process Memory` counters |
| Process priority / cores | `setpriority`, `sched_setaffinity` | Same | `SetPriorityClass`, `SetProcessAffinityMask` |
| Boost tweaks | sysfs via the pkexec helper, power-profiles-daemon over D-Bus, Hyprland options through the socket | sysfs + D-Bus; desktop effects per desktop | powercfg / registry / Game Mode keys |
| Game mode | All of the above + Hyprland effects off + do not disturb | Partly (no Hyprland effects) | Power plan, Game Mode, notifications off |
| Startup apps | autostart folder entries | Same | Run keys + Startup folder |
| Displays | Hyprland `monitor` keyword through the socket; Copy as config writes `monitors.lua` lines | X11: `xrandr`; Wayland others: not available (KDE `kscreen-doctor` in phase 16) | `ChangeDisplaySettingsExW`, `SetDisplayConfig` |
| Brightness | DDC/CI (`ddc-hi`), backlight via logind | Same | DDC/CI (`ddc-hi`), WMI for laptops |
| Night light | hyprsunset / gammastep / wlsunset | gammastep / wlsunset; X11 redshift | Gamma ramp |
| Mouse, keyboard, desktop options | Hyprland options through the socket | gsettings on GNOME; others limited | `SystemParametersInfoW`, registry |
| Keep my changes after a reload | Reapply on the `configreloaded` event | n/a | n/a (Windows keeps changes) |
| Workspace sets | Lua inside Hyprland (0.55+ with a Lua config) | not available | not available |
| Lock / sleep / shut down | logind over D-Bus; Caelestia lock screen when present | logind | `LockWorkStation`, `SetSuspendState`, `ExitWindowsEx` |
| Wallpaper | Caelestia, Noctalia, swww, hyprpaper, swaybg, mpvpaper, linux-wallpaperengine, custom command | swww, swaybg, custom command; KDE / GNOME via their tools (phase 16) | `SystemParametersInfoW`; Wallpaper Engine CLI |
| Window borders, glass | Hyprland options and rules | not available | not available (DWM border colour in phase 16) |
| Caelestia integration | full | if Caelestia runs | n/a |
| Terminal theming | kitty include, fish sequences, fastfetch | Same | Windows Terminal fragment |

## Audio and media

| Part | Hyprland | Other Linux | Windows |
|---|---|---|---|
| "What you hear" capture | PipeWire / PulseAudio monitor of the default sink | Same | WASAPI loopback, endpoint peak meter as stand-in |
| Master volume in dB | libpulse / PipeWire | Same | `IAudioEndpointVolume` |
| Limiter dips | Master volume | Same | Each app session (`ISimpleAudioVolume`), never above the user's own level |
| Per-app levels and volumes | Sink inputs with peak detection | Same | Session meters (free) |
| Media controls | MPRIS over D-Bus | Same | Global System Media Transport Controls |
| Speech | speech-dispatcher (`tts`) | Same | WinRT / SAPI (`tts`) |
| Sounds | `rodio` | Same | `rodio` |

## What "not available" means in the app

When a part can not work on the current desktop, the Rust app does what 1.9.21 does: the feature stays visible
with a one-line explanation of what it needs ("Needs Hyprland", "Install tesseract for Text Grab"), and
everything else keeps working. The README's "Which Linux?" table is regenerated from this file in phase 15.
