//! Doing things to the computer for the automation nodes: run a command line, a script in a terminal, close a
//! program, read the focused window and the volume, lock or shut down, speak, type. Everything here can wait on
//! another program or the network, so callers keep it off the UI thread. The texts are `core/system.py`'s.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The shells the Run command node offers on this system (Settings → Automation → Shell for commands).
pub fn default_shell() -> &'static str {
    if cfg!(windows) { "cmd" } else { "bash" }
}

/// The command that runs one line in `shell`.
pub fn shell_command(shell: &str, line: &str) -> Command {
    imp::shell_command(shell, line)
}

/// Runs `lines` as a script in a terminal window. When it ends, its exit code goes into `marker`. Gives back
/// the script's path, for the caller to remove once the marker is there.
pub fn script_in_terminal(
    cwd: &Path,
    lines: &[String],
    shell: &str,
    stop_on_error: bool,
    marker: &Path,
    configured: &str,
    preferred: &str,
) -> Result<PathBuf, String> {
    let (body, ext, command) = imp::terminal_script(lines, shell, stop_on_error, marker);
    let script = std::env::temp_dir().join(format!("lum-auto-{}{ext}", unique()));
    let newline = if ext == ".sh" { "\n" } else { "\r\n" };
    let text = body.replace('\n', newline) + newline;
    std::fs::write(&script, text).map_err(|e| e.to_string())?;
    let command = command.replace("{script}", &script.to_string_lossy());
    match crate::runner::open_terminal(cwd, &command, configured, preferred) {
        Ok(()) => Ok(script),
        Err(e) => {
            let _ = std::fs::remove_file(&script);
            Err(e)
        }
    }
}

fn unique() -> String {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:x}{:x}", std::process::id(), nanos)
}

/// Closes every running copy of a program. `true` when one was there to close.
pub fn close_program(name: &str, force: bool) -> bool {
    let mut cmd = imp::close_command(name, force);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.status().is_ok_and(|s| s.success())
}

/// The focused window's app (its class, or its program on Windows) and title.
pub fn focused_window() -> (String, String) {
    imp::focused_window().unwrap_or_default()
}

/// Whether the internet answers: a quick connection to two public DNS servers.
pub fn online() -> bool {
    ["1.1.1.1:53", "8.8.8.8:53"].iter().any(|addr| {
        addr.parse().is_ok_and(|a| std::net::TcpStream::connect_timeout(&a, Duration::from_secs(3)).is_ok())
    })
}

/// The default speakers' volume in percent, and whether they are muted.
pub fn volume() -> Option<(f64, bool)> {
    imp::volume()
}

/// `set` (to `amount` %), `up` / `down` (by `amount` %), `mute`, `unmute` or `toggle_mute`.
pub fn set_volume(mode: &str, amount: f64) -> Result<(), String> {
    imp::set_volume(mode, amount.clamp(0.0, 150.0))
}

/// What the Lock / sleep / shut down node offers.
pub const POWER_ACTIONS: [(&str, &str); 7] = [
    ("lock", "Lock the screen"),
    ("screen_off", "Turn the screens off"),
    ("sleep", "Sleep"),
    ("hibernate", "Hibernate"),
    ("logout", "Sign out"),
    ("restart", "Restart the PC"),
    ("shutdown", "Shut down the PC"),
];

pub fn power(action: &str) -> Result<(), String> {
    imp::power(action)
}

/// Reads `text` out loud and waits until it is said.
pub fn say(text: &str) -> Result<(), String> {
    imp::say(text)
}

/// Types `text` into the window that has the keyboard. `Ok(false)` when nothing here can type, so the caller
/// pastes it instead.
pub fn type_text(text: &str) -> Result<bool, String> {
    imp::type_text(text)
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use crate::terminal;

    /// Runs a program and gives back what it printed, or `None` when it could not run or failed.
    fn output(program: &str, args: &[&str]) -> Option<String> {
        let out = Command::new(program).args(args).stdin(Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn quote(s: &str) -> String {
        shlex::try_quote(s).map(|q| q.into_owned()).unwrap_or_else(|_| format!("'{s}'"))
    }

    pub fn shell_command(shell: &str, line: &str) -> Command {
        let exe = terminal::which(shell).unwrap_or_else(|| PathBuf::from("/bin/sh"));
        let mut cmd = Command::new(exe);
        cmd.arg("-c").arg(line);
        cmd
    }

    pub fn terminal_script(
        lines: &[String],
        shell: &str,
        stop: bool,
        marker: &Path,
    ) -> (String, &'static str, String) {
        let exe =
            terminal::which(shell).map_or_else(|| "sh".to_owned(), |p| p.to_string_lossy().into_owned());
        // fish has no `set -e`
        let body = if Path::new(&exe).file_name().is_some_and(|n| n == "fish") {
            lines.join(if stop { " && " } else { "\n" })
        } else {
            format!("{}{}", if stop { "set -e\n" } else { "" }, lines.join("\n"))
        };
        let command = format!("{} {{script}}; echo $? > {}", quote(&exe), quote(&marker.to_string_lossy()));
        (body, ".sh", command)
    }

    pub fn close_command(name: &str, force: bool) -> Command {
        let mut cmd = Command::new("pkill");
        if force {
            cmd.arg("-9");
        }
        cmd.args(["-x", name]);
        cmd
    }

    pub fn focused_window() -> Option<(String, String)> {
        #[cfg(target_os = "linux")]
        {
            let w = lum_hypr::Hypr::from_env()?.query("activewindow")?;
            let text = |key: &str| w.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
            Some((text("class"), text("title")))
        }
        #[cfg(not(target_os = "linux"))]
        None
    }

    pub fn volume() -> Option<(f64, bool)> {
        if terminal::which("wpctl").is_some()
            && let Some(out) = output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])
            // "Volume: 0.45 [MUTED]"
            && let Some(v) = out.split_whitespace().nth(1).and_then(|v| v.parse::<f64>().ok())
        {
            return Some((v * 100.0, out.contains("MUTED")));
        }
        if terminal::which("pactl").is_some() {
            let vol = output("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).unwrap_or_default();
            let mute = output("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).unwrap_or_default();
            let pct = vol.split_whitespace().find_map(|p| p.strip_suffix('%')?.parse::<f64>().ok());
            return pct.map(|p| (p, mute.contains("yes")));
        }
        None
    }

    fn mute_arg(mode: &str) -> &'static str {
        match mode {
            "mute" => "1",
            "unmute" => "0",
            _ => "toggle",
        }
    }

    pub fn set_volume(mode: &str, amount: f64) -> Result<(), String> {
        let muting = matches!(mode, "mute" | "unmute" | "toggle_mute");
        let sign = match mode {
            "up" => "+",
            "down" => "-",
            _ => "",
        };
        let (program, args): (&str, Vec<String>) = if terminal::which("wpctl").is_some() {
            let sink = "@DEFAULT_AUDIO_SINK@".to_owned();
            if muting {
                ("wpctl", vec!["set-mute".into(), sink, mute_arg(mode).into()])
            } else {
                let level = format!("{:.2}{sign}", amount / 100.0);
                ("wpctl", vec!["set-volume".into(), "-l".into(), "1.5".into(), sink, level])
            }
        } else if terminal::which("pactl").is_some() {
            let sink = "@DEFAULT_SINK@".to_owned();
            if muting {
                ("pactl", vec!["set-sink-mute".into(), sink, mute_arg(mode).into()])
            } else {
                let level = format!("{sign}{}%", python_g(amount));
                ("pactl", vec!["set-sink-volume".into(), sink, level])
            }
        } else {
            return Err("neither wpctl (PipeWire) nor pactl is installed".into());
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        output(program, &args).map(|_| ()).ok_or_else(|| format!("{program} failed"))
    }

    /// `f"{x:g}"` for the whole and simple numbers a volume is.
    fn python_g(x: f64) -> String {
        if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x}") }
    }

    fn hyprland() -> bool {
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|s| !s.is_empty())
    }

    fn run(cmd: &[&str]) -> Result<(), String> {
        output(cmd[0], &cmd[1..]).map(|_| ()).ok_or_else(|| format!("{} failed", cmd.join(" ")))
    }

    pub fn power(action: &str) -> Result<(), String> {
        match action {
            "lock" => {
                if hyprland() && terminal::which("hyprlock").is_some() {
                    let mut cmd = Command::new("hyprlock");
                    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::CommandExt;
                        cmd.process_group(0);
                    }
                    return cmd.spawn().map(|_| ()).map_err(|e| e.to_string());
                }
                run(&["loginctl", "lock-session"])
            }
            "screen_off" => {
                if hyprland() {
                    // the classic dispatcher first, then the Lua config's name for it
                    if run(&["hyprctl", "dispatch", "dpms", "off"]).is_ok() {
                        return Ok(());
                    }
                    #[cfg(target_os = "linux")]
                    if let Some(h) = lum_hypr::Hypr::from_env() {
                        return h.lua(&lum_hypr::lua::dispatch(r#"hl.dsp.dpms({ action = "off" })"#));
                    }
                }
                if terminal::which("xset").is_some() && std::env::var_os("DISPLAY").is_some() {
                    return run(&["xset", "dpms", "force", "off"]).map_err(|_| "xset failed".into());
                }
                Err("turning the screens off needs Hyprland or X11".into())
            }
            "sleep" => run(&["systemctl", "suspend"]),
            "hibernate" => run(&["systemctl", "hibernate"]),
            "restart" => run(&["systemctl", "reboot"]),
            "shutdown" => run(&["systemctl", "poweroff"]),
            "logout" => {
                if terminal::which("uwsm").is_some() && std::env::var_os("UWSM_FINALIZE_VARNAMES").is_some() {
                    run(&["uwsm", "stop"])
                } else if let Some(session) = std::env::var("XDG_SESSION_ID").ok().filter(|s| !s.is_empty()) {
                    run(&["loginctl", "terminate-session", &session])
                } else {
                    run(&["hyprctl", "dispatch", "exit"])
                }
            }
            other => Err(format!("unknown action {other}")),
        }
    }

    pub fn say(text: &str) -> Result<(), String> {
        let exe = ["spd-say", "espeak-ng", "espeak"]
            .iter()
            .find_map(|name| terminal::which(name))
            .ok_or("install speech-dispatcher or espeak-ng")?;
        let mut cmd = Command::new(&exe);
        if exe.file_name().is_some_and(|n| n == "spd-say") {
            // wait until it is said, so done comes after the words
            cmd.arg("-w");
        }
        cmd.arg(text).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.status().map(|_| ()).map_err(|e| e.to_string())
    }

    pub fn type_text(text: &str) -> Result<bool, String> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let tool = if wayland && terminal::which("wtype").is_some() {
            Some(("wtype", vec!["--", text]))
        } else if terminal::which("ydotool").is_some() {
            Some(("ydotool", vec!["type", "--", text]))
        } else {
            None
        };
        let Some((program, args)) = tool else { return Ok(false) };
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| true)
            .map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;

    use super::*;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn shell_command(shell: &str, line: &str) -> Command {
        if shell == "powershell" {
            let mut cmd = Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", line]);
            cmd.creation_flags(CREATE_NO_WINDOW);
            return cmd;
        }
        let mut cmd = Command::new("cmd.exe");
        // cmd reads its own command line, so it gets it exactly as typed
        cmd.raw_arg(format!("/d /s /c \"{line}\""));
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    pub fn terminal_script(
        lines: &[String],
        shell: &str,
        stop: bool,
        marker: &Path,
    ) -> (String, &'static str, String) {
        let marker = marker.to_string_lossy();
        if shell == "powershell" {
            let check = if stop { "\nif ($LASTEXITCODE) { exit $LASTEXITCODE }" } else { "" };
            let body: Vec<String> = lines.iter().map(|l| format!("{l}{check}")).collect();
            let command = format!(
                "powershell -NoProfile -ExecutionPolicy Bypass -File '{{script}}'; $LASTEXITCODE | Out-File -Encoding ascii '{marker}'"
            );
            return (body.join("\n"), ".ps1", command);
        }
        let check = if stop { "\nif errorlevel 1 exit /b %errorlevel%" } else { "" };
        let body: Vec<String> = lines.iter().map(|l| format!("{l}{check}")).collect();
        let command = format!("cmd /c '{{script}}'; $LASTEXITCODE | Out-File -Encoding ascii '{marker}'");
        (format!("@echo off\n{}", body.join("\n")), ".cmd", command)
    }

    pub fn close_command(name: &str, force: bool) -> Command {
        let exe = if name.to_lowercase().ends_with(".exe") { name.to_owned() } else { format!("{name}.exe") };
        let mut cmd = Command::new("taskkill");
        cmd.args(["/IM", &exe]);
        if force {
            cmd.arg("/F");
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    pub fn focused_window() -> Option<(String, String)> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        };
        // SAFETY: plain calls on the foreground window; every buffer is sized for the call that fills it, and
        // the process handle is closed below.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let len = GetWindowTextLengthW(hwnd).max(0) as usize;
            let mut title = vec![0u16; len + 1];
            let got = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
            let title = String::from_utf16_lossy(&title[..got]);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut app = String::new();
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !process.is_null() {
                let mut path = vec![0u16; 1024];
                let mut size = path.len() as u32;
                if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut size) != 0
                {
                    let full = String::from_utf16_lossy(&path[..size as usize]);
                    app = Path::new(&full)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or(full);
                }
                CloseHandle(process);
            }
            Some((app, title))
        }
    }

    // The speakers' volume is Windows' audio endpoint COM interface, which the sound settings of a later phase
    // bring; until then the node says so.
    pub fn volume() -> Option<(f64, bool)> {
        None
    }

    pub fn set_volume(_mode: &str, _amount: f64) -> Result<(), String> {
        Err("not available yet".into())
    }

    pub fn power(action: &str) -> Result<(), String> {
        use windows_sys::Win32::System::Power::SetSuspendState;
        use windows_sys::Win32::System::Shutdown::LockWorkStation;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BROADCAST, PostMessageW, SC_MONITORPOWER, WM_SYSCOMMAND,
        };
        match action {
            // SAFETY: no arguments; it only asks Windows to lock.
            "lock" => match unsafe { LockWorkStation() } {
                0 => Err("Windows refused to lock".into()),
                _ => Ok(()),
            },
            "screen_off" => {
                // SendMessage would wait for every window; Post does not
                // SAFETY: a broadcast message with plain numbers for arguments.
                unsafe { PostMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER as usize, 2) };
                Ok(())
            }
            "sleep" | "hibernate" => {
                // SAFETY: plain flags; Windows suspends after the call returns.
                unsafe { SetSuspendState(action == "hibernate", false, false) };
                Ok(())
            }
            "logout" | "restart" | "shutdown" => {
                let args: &[&str] = match action {
                    "logout" => &["/l"],
                    "restart" => &["/r", "/t", "0"],
                    _ => &["/s", "/t", "0"],
                };
                Command::new("shutdown")
                    .args(args)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            other => Err(format!("unknown action {other}")),
        }
    }

    pub fn say(text: &str) -> Result<(), String> {
        let safe = text.replace('\'', "''");
        let script = format!(
            "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).Speak('{safe}')"
        );
        Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Each character as its own Unicode key press, a few milliseconds apart (the `keyboard` package's
    /// `write(text, delay=0.005)`).
    pub fn type_text(text: &str) -> Result<bool, String> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
        };
        let key = |unit: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        for unit in text.replace('\n', "\r").encode_utf16() {
            let inputs = [key(unit, false), key(unit, true)];
            // SAFETY: the array lives for the call and each INPUT is a fully set up keyboard event.
            unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(true)
    }
}
