# Crates

Which Rust crate does each job, and what it replaces. Versions are left out on purpose: pin whatever is current
and stable when the phase starts, and record anything surprising in the decision log in ARCHITECTURE.md. Before
adding a crate, check it is maintained, works on both Linux and Windows (or sits behind a `cfg`), and passes
`cargo deny`.

## Python libraries → Rust

| Python today | Used for | Rust | Phase |
|---|---|---|---|
| PyQt6 (widgets, QSS) | all UI | `eframe`, `egui`, `egui_extras` (images, tables) | 2 |
| PyQt6 painting | backdrop, graphs, node editor | `egui::Painter`, meshes; `egui_plot` for graphs | 2 |
| PyQt6 `QSystemTrayIcon` | tray | `ksni` (Linux, StatusNotifierItem over D-Bus), `tray-icon` (Windows) | 3 |
| PyQt6 `QMediaPlayer` | sounds | `rodio` | 3 |
| PyQt6 `QFileSystemWatcher` | file triggers | `notify` | 10 |
| PyQt6 `QLocalSocket` | Hyprland sockets | `std::os::unix::net` / `tokio::net::UnixStream` | 3 |
| Pillow | thumbnails, metadata, icons | `image`, `kamadak-exif` (read), `img-parts` (container edits) | 8, 13 |
| opencv-python | video thumbnails | `ffmpeg` frame grab (external, optional) | 8 |
| numpy | A-weighting FIR, RMS | hand-written IIR biquads; `realfft` only if needed | 12 |
| cryptography | vault, Atlas | `aes-gcm`, `pbkdf2`, `sha2`, `hmac`, `zeroize`, `rand` / `getrandom` | 13 |
| segno | QR codes | `qrcode` (PNG and SVG output) | 8 |
| requests / urllib | currency, link previews, updates, webhooks | `ureq` on worker threads, or `reqwest` on the tokio runtime (pick one in phase 1) | 1, 8 |
| psutil | processes, CPU, disks, network, sensors | `sysinfo`; `procfs` for Linux extras | 11 |
| pulsectl | volumes, sinks | `libpulse-binding` (works with pipewire-pulse) or `pipewire` | 11, 12 |
| keyboard (Windows) | global hook | `windows` crate: `SetWindowsHookExW(WH_KEYBOARD_LL)` on its own thread | 3 |
| pycaw / comtypes | Core Audio | `windows` crate (`IAudioEndpointVolume`, `ISimpleAudioVolume`, `IAudioMeterInformation`), `wasapi` for loopback | 12 |
| winrt-* | media sessions, OCR | `windows` crate (`Windows.Media.Control`, `Windows.Media.Ocr`) | 9, 12 |
| PyInstaller | packaging | plain `cargo build --release`; `embed-resource` or `winresource` for the Windows icon and version | 15 |
| json | storage | `serde`, `serde_json` | 1 |
| logging | logs | `tracing`, `tracing-subscriber`, `tracing-appender` | 1 |
| threading / QTimer | background work | `tokio` (services), `crossbeam-channel` (to the UI), `rayon` (thumbnails, OCR) | 1 |
| ast-walking evaluator | calculator | hand-written Pratt parser in `lum-calc` | 8 |
| datetime | dates and times | `jiff` (chrono is flagged unmaintained by RustSec, see D7) | 1 |
| uuid | ids | `uuid` (v4, cut to 10 hex chars like the Python ids) | 1 |
| zipfile / tarfile | backups, updates | `zip`, `tar`, `flate2` | 1, 15 |
| shlex | command lines | `shlex` | 3 |
| re | everywhere | `regex` | 1 |
| difflib-like fuzzy score | palette | `nucleo-matcher` | 5 |
| Markdown rendering | notes preview | `pulldown-cmark`, `egui_commonmark` | 8 |
| `lua` program in tests | workspace Lua tests | `mlua` with vendored Lua 5.4 | 11 |

## New needs in Rust

| Need | Crate | Notes |
|---|---|---|
| Lock-free shared snapshots | `arc-swap` | theme tokens, settings snapshot |
| Paths | `directories` | data and config folders |
| CLI | `clap` | `--smoke`, `--strip-metadata`, `ctl` |
| Single instance + ctl socket | `interprocess` (local sockets on both systems), `fs4` (file lock) | |
| Errors | `thiserror` (libraries), `anyhow` (app) | |
| Autostart | `auto-launch` | or keep the hand-written .desktop / Run key code; it is short |
| Notifications | `notify-rust` | D-Bus on Linux, toast on Windows |
| Clipboard | `arboard` (with its Wayland data-control feature) and `wl-clipboard-rs` | watching on Wayland needs the data-control protocol; on Windows `AddClipboardFormatListener` |
| Screen capture | `xcap` | wlr-screencopy directly (via `wayland-client`) if `xcap` is slow on Hyprland |
| Tray icon pixels | `tiny-skia` | draw the sparkle at any size |
| SVG icons | `resvg` | if icon assets move to SVG |
| Desktop entries and icon themes | `freedesktop-desktop-entry`, `freedesktop-icons` | Linux program icons |
| Open with the default app | `open` | files, folders, URLs |
| Speech | `tts` | speech-dispatcher on Linux, WinRT on Windows |
| MPRIS | `mpris` or `zbus` directly | replaces `playerctl` |
| DDC/CI brightness | `ddc-hi` | Linux (i2c-dev) and Windows; replaces `ddcutil` |
| NVIDIA stats | `nvml-wrapper` | replaces spawning `nvidia-smi` every second |
| Battery | `battery` (starship-battery fork if the original is stale) | |
| Windows registry | `winreg` | Run key, Explorer settings, menus |
| Power profiles, logind, inhibit | `zbus` | power-profiles-daemon, systemd-logind inhibit for keep awake |
| X11 fallbacks | `x11rb` | key grabs and window moves on X11 (phase 16) |
| Portals | `ashpd` | GlobalShortcuts and screenshots on KDE / GNOME (phase 16) |
| Layer-shell surfaces | `smithay-client-toolkit` | native Wayland overlays (phase 16) |
| OCR without Tesseract | `ocrs` + `rten` | optional, phase 16 |
| UI tests and snapshots | `egui_kittest` | replaces `tools/shots.py` |
| Other tests | `insta` (snapshots of text output), `proptest` (parsers), `criterion` (benchmarks), `tempfile` | |
| Profiling | `puffin` + `puffin_egui` (frame profiler), `tracy-client` (optional), `dhat` (heap) | dev only |

## External programs: kept or replaced

| Program | Used for today | 2.0 |
|---|---|---|
| `hyprctl` | a few queries | **replaced** by the socket (already mostly the case) |
| `nvidia-smi` | GPU stats, per-process GPU | **replaced** by NVML; kept only as a fallback |
| `pactl` | sinks, volumes | **replaced** by libpulse / PipeWire |
| `parec` | the monitor recording | **replaced** by libpulse / PipeWire capture |
| `playerctl` | media | **replaced** by MPRIS over D-Bus |
| `ddcutil` | monitor brightness | **replaced** by `ddc-hi` |
| `brightnessctl` | laptop backlight | **replaced** by `/sys/class/backlight` (through logind's SetBrightness, so no root) |
| `grim` | screenshots | **replaced** by `xcap` / screencopy; kept as a fallback |
| `wl-copy` / `wl-paste` | clipboard | **replaced** by `wl-clipboard-rs` / `arboard` |
| `notify-send` | notifications | **replaced** by `notify-rust` |
| `powerprofilesctl` | power mode | **replaced** by D-Bus |
| `systemctl`, `loginctl` | lock, sleep, shut down, sessions | **replaced** by logind over D-Bus where possible |
| `gsettings` | GNOME / GTK settings | kept (simple, rare) or `zbus` to dconf |
| `xrandr` | X11 displays | kept on X11 |
| `paplay` / `pw-play` | sounds | **replaced** by `rodio` |
| `tesseract` | OCR on Linux | **kept** for parity; `ocrs` optional in phase 16 |
| `ffmpeg` / `ffprobe` | video metadata and stripping, video thumbnails | **kept** (optional, as today) |
| `wtype` / `ydotool` | typing text on Wayland | kept; virtual-keyboard protocol in phase 16 |
| `swww`, `hyprpaper`, `swaybg`, `mpvpaper`, `linux-wallpaperengine`, `caelestia` | wallpapers, shell | **kept**: they are the wallpaper programs themselves |
| `hyprsunset`, `gammastep`, `wlsunset` | night light | kept |
| `pkexec` | root tweaks | kept, through one small helper binary with an allow-list |
| `kitty` and other terminals | running scripts | kept |
| `xdg-open`, `gtk-launch`, `gio` | opening things | `open` crate first, these as fallbacks |
| `powershell` | a few Windows settings | **replaced** by the `windows` crate and `winreg` where possible |

Every external program is looked up once and cached (`which`), and its absence turns off only the feature that
needs it, with a plain message saying what to install (the Python app's behaviour).
