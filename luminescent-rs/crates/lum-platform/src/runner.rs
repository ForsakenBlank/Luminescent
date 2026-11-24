//! Starting dashboard items: scripts, programs, files, folders, URLs and commands (`core/runner.py`).
//!
//! * `.py`, `.sh` and other scripts open in a terminal window that stays open afterwards,
//! * `.pyw` runs with Python and no window,
//! * `.desktop` starts the way the app launcher would (gtk-launch or gio),
//! * executables run directly, with their arguments,
//! * other files and folders open with the default app, `http(s)://` in the browser,
//! * anything else runs as a shell command ("gnome-calculator", "code .").
//!
//! Every launch is appended to `data/command_log.txt` for the Prompt Log. A launch hands back the process it
//! started when there is one to track (the dashboard shows the item as running and can stop it). Run as
//! administrator: scripts get `sudo` in their terminal (so you type the password there), programs and
//! commands go through pkexec. Windows: scripts open in a Windows Terminal tab (or a console window) that
//! stays open, `.pyw` runs with pythonw, programs and files use the "open" verb, commands run without a
//! window, and administrator asks through the usual UAC prompt.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::terminal;

/// What a target is, for its icon and how it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Url,
    Folder,
    Python,
    Program,
    File,
    Command,
    None,
}

impl Kind {
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Url => "🌐",
            Kind::Folder => "📁",
            Kind::Python => "🐍",
            Kind::Program => "⚙",
            Kind::File => "📄",
            Kind::Command => "›_",
            Kind::None => "·",
        }
    }
}

const CONSOLE_EXT: &[&str] =
    if cfg!(windows) { &["py", "bat", "cmd", "ps1"] } else { &["py", "sh", "bash", "zsh", "fish", "ps1"] };

/// How an item starts (the dashboard item's own settings).
#[derive(Debug, Clone)]
pub struct Options {
    pub use_terminal: bool,
    pub keep_open: bool,
    /// A Python to use instead of the one found on PATH.
    pub python: String,
    /// A terminal for this item instead of the preferred one.
    pub terminal: String,
    /// The terminal chosen in Settings → Dashboard & Scripts ("" for automatic).
    pub preferred_terminal: String,
    /// The working folder; empty for the item's own folder.
    pub cwd: String,
    pub admin: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            use_terminal: true,
            keep_open: true,
            python: String::new(),
            terminal: String::new(),
            preferred_terminal: String::new(),
            cwd: String::new(),
            admin: false,
        }
    }
}

fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// `~` and environment variables (`$HOME`, `${HOME}`, and `%USERPROFILE%` on Windows), like Python's
/// `expandvars(expanduser(...))`. Unknown variables are left as written.
pub fn expand(text: &str) -> String {
    let mut s = text.to_owned();
    if s == "~" || s.starts_with("~/") || (cfg!(windows) && s.starts_with("~\\")) {
        s = format!("{}{}", home().display(), &s[1..]);
    }
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let (name, end) = if c == '$' && chars.get(i + 1) == Some(&'{') {
            let close = chars[i + 2..].iter().position(|&c| c == '}').map(|p| p + i + 2);
            match close {
                Some(close) => (chars[i + 2..close].iter().collect::<String>(), close + 1),
                None => (String::new(), i),
            }
        } else if c == '$' {
            let len = chars[i + 1..].iter().take_while(|c| c.is_alphanumeric() || **c == '_').count();
            (chars[i + 1..i + 1 + len].iter().collect(), i + 1 + len)
        } else if c == '%' && cfg!(windows) {
            match chars[i + 1..].iter().position(|&c| c == '%').map(|p| p + i + 1) {
                Some(close) => (chars[i + 1..close].iter().collect(), close + 1),
                None => (String::new(), i),
            }
        } else {
            (String::new(), i)
        };
        match std::env::var(&name) {
            Ok(value) if !name.is_empty() => {
                out.push_str(&value);
                i = end;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn is_url(t: &str) -> bool {
    let l = t.to_lowercase();
    l.starts_with("http://") || l.starts_with("https://") || l.starts_with("www.")
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> bool {
    false
}

pub fn kind_of(target: &str) -> Kind {
    let t = target.trim();
    if is_url(t) {
        return Kind::Url;
    }
    let path = PathBuf::from(expand(t));
    if path.is_dir() {
        return Kind::Folder;
    }
    if path.is_file() {
        let ext = extension(&path);
        if ext == "py" || ext == "pyw" {
            return Kind::Python;
        }
        let program = ["sh", "desktop", "appimage", "exe", "lnk"].contains(&ext.as_str());
        if program || CONSOLE_EXT.contains(&ext.as_str()) || executable(&path) {
            return Kind::Program;
        }
        return Kind::File;
    }
    if t.is_empty() { Kind::None } else { Kind::Command }
}

/// Appends a launch to `data/command_log.txt` (local time, a tab, the command).
pub fn log_command(log: &Path, text: &str) {
    use std::io::Write;
    let now = jiff::ZonedDateTime::now().strftime("%Y-%m-%dT%H:%M:%S").to_string();
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .and_then(|mut f| writeln!(f, "{now}\t{text}"));
    if let Err(e) = written {
        tracing::debug!("could not write {}: {e}", log.display());
    }
}

/// The Python to run scripts with: the configured one, else `python3` / `python` (pythonw on Windows for
/// windowless scripts).
pub fn python(configured: &str, windowless: bool) -> String {
    if !configured.is_empty() && Path::new(configured).is_file() {
        let path = Path::new(configured);
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if cfg!(windows) && (name == "python.exe" || name == "pythonw.exe") {
            let want = if windowless { "pythonw.exe" } else { "python.exe" };
            let other = path.with_file_name(want);
            if other.is_file() {
                return other.to_string_lossy().into_owned();
            }
        }
        return configured.to_owned();
    }
    let names: &[&str] = match (cfg!(windows), windowless) {
        (true, true) => &["pythonw", "pyw", "python"],
        (true, false) => &["python", "py"],
        _ => &["python3", "python"],
    };
    names
        .iter()
        .find_map(|n| terminal::which(n))
        .map_or_else(|| names[0].to_owned(), |p| p.to_string_lossy().into_owned())
}

/// Splits arguments the way a shell would.
pub fn split_args(args: &str) -> Vec<String> {
    if args.trim().is_empty() {
        return Vec::new();
    }
    shlex::split(args).unwrap_or_else(|| args.split_whitespace().map(str::to_owned).collect())
}

#[cfg(not(windows))]
fn joined(cmd: &[String]) -> String {
    shlex::try_join(cmd.iter().map(String::as_str)).unwrap_or_else(|_| cmd.join(" "))
}

/// Starts a process that outlives us, in a process group of its own (so Stop takes everything it started),
/// and never holds our terminal or stdin.
fn spawn(cmd: &[String], cwd: Option<&Path>) -> std::io::Result<Child> {
    let (program, args) = cmd.split_first().ok_or_else(|| std::io::Error::other("nothing to run"))?;
    let mut c = Command::new(program);
    c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = cwd.filter(|d| !d.as_os_str().is_empty()) {
        c.current_dir(dir);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        c.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    c.spawn()
}

/// Opens a file, folder or URL with its default app.
pub fn open_path(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    return win::shell_execute("open", path, "", "").map(|_| ());
    #[cfg(not(windows))]
    spawn(&["xdg-open".to_owned(), path.to_owned()], None).map(|_| ()).map_err(|e| e.to_string())
}

/// `cmd` through pkexec, keeping what a graphical program needs to reach your session.
#[cfg(not(windows))]
fn elevated(cmd: Vec<String>) -> Vec<String> {
    let keep = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XAUTHORITY",
        "DBUS_SESSION_BUS_ADDRESS",
        "QT_QPA_PLATFORM",
    ]
    .iter()
    .filter_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()).map(|v| format!("{k}={v}")));
    let pkexec =
        terminal::which("pkexec").map_or_else(|| "pkexec".to_owned(), |p| p.to_string_lossy().into_owned());
    [pkexec, "env".to_owned()].into_iter().chain(keep).chain(cmd).collect()
}

/// Runs `cmd`, then stays in an interactive shell so its output can be read.
#[cfg(not(windows))]
fn keep_open(cmd: &[String]) -> Vec<String> {
    let script = r#""$@"; code=$?; echo; echo "[finished with exit code $code]"; exec "${SHELL:-sh}""#;
    ["sh", "-c", script, "sh"].iter().map(|s| (*s).to_owned()).chain(cmd.iter().cloned()).collect()
}

/// Starts `target`. Gives back the process to track (None when nothing trackable was started), or the
/// error in the Python wording.
pub fn launch(
    target: &str,
    args: &str,
    name: &str,
    opts: &Options,
    log: &Path,
) -> Result<Option<Child>, String> {
    let target = target.trim().trim_matches('"');
    if target.is_empty() {
        return Err("No file or command is set for this item.".to_owned());
    }
    let work = expand(opts.cwd.trim());
    if !work.is_empty() && !Path::new(&work).is_dir() {
        return Err(format!("The working folder does not exist: {work}"));
    }
    let started = imp::launch(target, args, name, opts, &work, log);
    if let Err(e) = &started {
        tracing::warn!("could not start {target}: {e}");
    }
    started
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn launch(
        target: &str,
        args: &str,
        name: &str,
        opts: &Options,
        work: &str,
        log: &Path,
    ) -> Result<Option<Child>, String> {
        let path = PathBuf::from(expand(target));
        let argv = split_args(args);
        let title = if name.is_empty() {
            path.file_name().map_or_else(|| target.to_owned(), |n| n.to_string_lossy().into_owned())
        } else {
            name.to_owned()
        };
        let err = |e: std::io::Error| e.to_string();
        if is_url(target) {
            let url = if target.contains("://") { target.to_owned() } else { format!("https://{target}") };
            log_command(log, &format!("open {target}"));
            return open_path(&url).map(|()| None);
        }
        if path.is_dir() {
            log_command(log, &format!("open {}", path.display()));
            return open_path(&path.to_string_lossy()).map(|()| None);
        }
        if !path.exists() {
            let cmd = format!("{target} {args}").trim().to_owned();
            let sh = vec!["sh".to_owned(), "-c".to_owned(), cmd.clone()];
            let full = if opts.admin { elevated(sh) } else { sh };
            let dir = if work.is_empty() { super::home() } else { PathBuf::from(work) };
            let child = spawn(&full, Some(&dir)).map_err(err)?;
            log_command(log, &format!("{}{cmd}", if opts.admin { "sudo " } else { "" }));
            return Ok(Some(child));
        }
        let ext = extension(&path);
        let cwd = if work.is_empty() {
            path.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            work.into()
        };
        let file = path.to_string_lossy().into_owned();
        if ext == "pyw" {
            let cmd: Vec<String> = [python(&opts.python, true), file].into_iter().chain(argv).collect();
            let child = spawn(&if opts.admin { elevated(cmd.clone()) } else { cmd.clone() }, Some(&cwd))
                .map_err(err)?;
            log_command(log, &joined(&cmd));
            return Ok(Some(child));
        }
        if ext == "desktop" {
            let cmd: Vec<String> = match (terminal::which("gtk-launch"), terminal::which("gio")) {
                (Some(gtk), _) => {
                    let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    [gtk.to_string_lossy().into_owned(), id].into_iter().chain(argv).collect()
                }
                (None, Some(gio)) => [gio.to_string_lossy().into_owned(), "launch".to_owned(), file]
                    .into_iter()
                    .chain(argv)
                    .collect(),
                (None, None) => vec!["xdg-open".to_owned(), file],
            };
            spawn(&cmd, Some(&cwd)).map_err(err)?;
            log_command(log, &joined(&cmd));
            return Ok(None);
        }
        if CONSOLE_EXT.contains(&ext.as_str()) {
            let mut cmd: Vec<String> = if ext == "py" {
                vec![python(&opts.python, false), file]
            } else if ext == "ps1" {
                let pwsh = terminal::which("pwsh")
                    .map_or_else(|| "pwsh".to_owned(), |p| p.to_string_lossy().into_owned());
                vec![pwsh, "-NoLogo".to_owned(), "-File".to_owned(), file]
            } else if executable(&path) {
                vec![file]
            } else {
                let shell = match ext.as_str() {
                    "bash" => "bash",
                    "zsh" => "zsh",
                    "fish" => "fish",
                    _ => "sh",
                };
                vec![shell.to_owned(), file]
            };
            cmd.extend(argv);
            if opts.admin {
                let sudo = terminal::which("sudo")
                    .map_or_else(|| "sudo".to_owned(), |p| p.to_string_lossy().into_owned());
                cmd.insert(0, sudo);
            }
            let child = console(&cmd, &cwd, &title, opts)?;
            log_command(log, &joined(&cmd));
            return Ok(child);
        }
        if executable(&path) {
            let cmd: Vec<String> = std::iter::once(file).chain(argv).collect();
            let child = spawn(&if opts.admin { elevated(cmd.clone()) } else { cmd.clone() }, Some(&cwd))
                .map_err(err)?;
            log_command(log, &joined(&cmd));
            return Ok(Some(child));
        }
        log_command(log, &format!("open {}", path.display()));
        open_path(&path.to_string_lossy()).map(|()| None)
    }

    fn console(cmd: &[String], cwd: &Path, title: &str, opts: &Options) -> Result<Option<Child>, String> {
        let cmd = if opts.keep_open { keep_open(cmd) } else { cmd.to_vec() };
        if !opts.use_terminal {
            // no window: the output is thrown away
            return spawn(&cmd, Some(cwd)).map(Some).map_err(|e| e.to_string());
        }
        let Some(term) = terminal::find(&opts.terminal, &opts.preferred_terminal) else {
            return Err("No terminal program found. Set one in Settings → Dashboard & Scripts.".to_owned());
        };
        let full = terminal::command(&term, title, cwd, &cmd, &[]);
        spawn(&full, Some(cwd)).map(Some).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    /// What Windows itself would write for this argument list.
    fn cmdline(cmd: &[String]) -> String {
        cmd.iter()
            .map(|a| {
                if !a.is_empty() && !a.contains([' ', '\t', '"']) {
                    return a.clone();
                }
                let mut out = String::from('"');
                let mut slashes = 0;
                for c in a.chars() {
                    match c {
                        '\\' => slashes += 1,
                        '"' => {
                            out.push_str(&"\\".repeat(slashes * 2 + 1));
                            out.push('"');
                            slashes = 0;
                        }
                        _ => {
                            out.push_str(&"\\".repeat(slashes));
                            out.push(c);
                            slashes = 0;
                        }
                    }
                }
                out.push_str(&"\\".repeat(slashes * 2));
                out.push('"');
                out
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// PowerShell rather than `cmd /k`: cmd strips the outer quotes of `"C:\a b\python.exe" "x.py"` and every
    /// layer (us, wt, the tab) quotes differently. Single quotes survive all of them.
    fn powershell_call(cmd: &[String]) -> String {
        format!(
            "& {}",
            cmd.iter().map(|a| format!("'{}'", a.replace('\'', "''"))).collect::<Vec<_>>().join(" ")
        )
    }

    fn run(cmd: &[String], cwd: &Path, flags: u32) -> Result<Child, String> {
        let (program, args) = cmd.split_first().ok_or("nothing to run")?;
        Command::new(program)
            .args(args)
            .current_dir(cwd)
            .creation_flags(flags)
            .spawn()
            .map_err(|e| e.to_string())
    }

    pub fn launch(
        target: &str,
        args: &str,
        name: &str,
        opts: &Options,
        work: &str,
        log: &Path,
    ) -> Result<Option<Child>, String> {
        let path = PathBuf::from(expand(target));
        let argv = split_args(args);
        let title = if name.is_empty() {
            path.file_name().map_or_else(|| target.to_owned(), |n| n.to_string_lossy().into_owned())
        } else {
            name.to_owned()
        };
        if is_url(target) {
            let url = if target.contains("://") { target.to_owned() } else { format!("https://{target}") };
            log_command(log, &format!("open {target}"));
            return open_path(&url).map(|()| None);
        }
        if path.is_dir() {
            log_command(log, &format!("open {}", path.display()));
            return open_path(&path.to_string_lossy()).map(|()| None);
        }
        let home = super::home();
        if !path.exists() {
            let cmd = format!("{target} {args}").trim().to_owned();
            log_command(log, &cmd);
            let dir = if work.is_empty() { home } else { PathBuf::from(work) };
            if opts.admin {
                return win::shell_execute("runas", "cmd.exe", &format!("/c {cmd}"), &dir.to_string_lossy())
                    .map(|_| None);
            }
            let child = Command::new("cmd.exe")
                .raw_arg(format!("/c {cmd}"))
                .current_dir(&dir)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(Some(child));
        }
        let ext = extension(&path);
        let cwd = if work.is_empty() {
            path.parent().map(Path::to_path_buf).unwrap_or_default()
        } else {
            work.into()
        };
        let cwd_text = cwd.to_string_lossy().into_owned();
        let file = path.to_string_lossy().into_owned();
        if ext == "pyw" {
            let cmd: Vec<String> = [python(&opts.python, true), file].into_iter().chain(argv).collect();
            log_command(log, &cmdline(&cmd));
            if opts.admin {
                return win::shell_execute("runas", &cmd[0], &cmdline(&cmd[1..]), &cwd_text).map(|_| None);
            }
            return run(&cmd, &cwd, 0).map(Some);
        }
        if CONSOLE_EXT.contains(&ext.as_str()) {
            let mut cmd: Vec<String> = match ext.as_str() {
                "py" => vec![python(&opts.python, false), file],
                "ps1" => ["powershell.exe", "-NoLogo", "-ExecutionPolicy", "Bypass", "-File"]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .chain([file])
                    .collect(),
                _ => vec![file],
            };
            cmd.extend(argv);
            log_command(log, &cmdline(&cmd));
            if opts.admin {
                let keep = ["-NoLogo", "-NoExit", "-Command"].map(str::to_owned).to_vec();
                let params = cmdline(&[keep, vec![powershell_call(&cmd)]].concat());
                return win::shell_execute("runas", "powershell.exe", &params, &cwd_text).map(|_| None);
            }
            return console(&cmd, &cwd, &title, opts);
        }
        if ext == "exe" {
            log_command(log, &cmdline(&[vec![file.clone()], argv.clone()].concat()));
            if opts.admin {
                return win::shell_execute("runas", &file, &cmdline(&argv), &cwd_text).map(|_| None);
            }
            return run(&[vec![file], argv].concat(), &cwd, 0).map(Some);
        }
        log_command(log, &format!("open {}", path.display()));
        if opts.admin || !argv.is_empty() {
            let verb = if opts.admin { "runas" } else { "open" };
            return win::shell_execute(verb, &file, args, &cwd_text).map(|_| None);
        }
        open_path(&file).map(|()| None)
    }

    fn console(cmd: &[String], cwd: &Path, title: &str, opts: &Options) -> Result<Option<Child>, String> {
        let mut cmd = cmd.to_vec();
        if opts.keep_open {
            if cmd[0].to_lowercase().ends_with("powershell.exe") {
                cmd.insert(1, "-NoExit".to_owned());
            } else {
                cmd = ["powershell.exe", "-NoLogo", "-NoExit", "-Command"]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .chain([powershell_call(&cmd)])
                    .collect();
            }
        }
        if opts.use_terminal
            && let Some(wt) = terminal::wt()
        {
            let term = terminal::find(&opts.terminal, &opts.preferred_terminal).unwrap_or(wt);
            run(&terminal::command(&term, title, cwd, &cmd, &[]), cwd, CREATE_NO_WINDOW)?;
            // the tab belongs to Terminal: nothing of ours to track
            return Ok(None);
        }
        run(&cmd, cwd, CREATE_NEW_CONSOLE).map(Some)
    }
}

#[cfg(windows)]
pub(crate) mod win {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// `ShellExecuteW` with a verb (`open`, or `runas` for the administrator prompt).
    pub fn shell_execute(verb: &str, file: &str, params: &str, dir: &str) -> Result<(), String> {
        let (verb, file, params, dir) = (wide(verb), wide(file), wide(params), wide(dir));
        let dir_ptr = if dir.len() > 1 { dir.as_ptr() } else { std::ptr::null() };
        // SAFETY: every string is nul-terminated and lives for the call; no window owns the prompt.
        let r = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                dir_ptr,
                SW_SHOWNORMAL,
            )
        } as isize;
        match r {
            r if r > 32 => Ok(()),
            5 => Err("Cancelled at the administrator prompt.".to_owned()),
            r => Err(format!("Could not start it (error {r}).")),
        }
    }
}

/// Stops a process `launch` started, with everything it started (its own process group). True if it was
/// still running.
pub fn stop(child: &mut Child) -> bool {
    if !matches!(child.try_wait(), Ok(None)) {
        return false;
    }
    #[cfg(unix)]
    {
        let pid = child.id() as libc::pid_t;
        // SAFETY: plain syscall; the group is the one spawn() put the child in.
        if unsafe { libc::killpg(pid, libc::SIGTERM) } == 0 {
            return true;
        }
        child.kill().is_ok()
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// A new terminal window in `cwd`, optionally running a shell command first (then staying open).
#[cfg(windows)]
pub fn open_terminal(cwd: &Path, command: &str, configured: &str, preferred: &str) -> Result<(), String> {
    let mut cmd = vec!["powershell.exe".to_owned(), "-NoLogo".to_owned(), "-NoExit".to_owned()];
    if !command.is_empty() {
        cmd.extend(["-Command".to_owned(), command.to_owned()]);
    }
    let term = terminal::find(configured, preferred).unwrap_or_else(|| "powershell.exe".into());
    let full = terminal::command(&term, "PowerShell", cwd, &cmd, &[]);
    spawn(&full, Some(cwd)).map(|_| ()).map_err(|e| e.to_string())
}

/// A new terminal window in `cwd`, optionally running a shell command first (then staying open).
#[cfg(not(windows))]
pub fn open_terminal(cwd: &Path, command: &str, configured: &str, preferred: &str) -> Result<(), String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_owned());
    let argv =
        if command.is_empty() { vec![shell] } else { keep_open(&["sh".into(), "-c".into(), command.into()]) };
    let term = terminal::find(configured, preferred).ok_or("No terminal program found.")?;
    spawn(&terminal::command(&term, "Terminal", cwd, &argv, &[]), Some(cwd))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        let dir = tempfile::tempdir().expect("temp");
        let file = |name: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, "x").expect("write");
            p.to_string_lossy().into_owned()
        };
        assert_eq!(kind_of("https://example.com"), Kind::Url);
        assert_eq!(kind_of("www.example.com"), Kind::Url);
        assert_eq!(kind_of(&dir.path().to_string_lossy()), Kind::Folder);
        assert_eq!(kind_of(&file("a.py")), Kind::Python);
        assert_eq!(kind_of(&file("a.sh")), Kind::Program);
        assert_eq!(kind_of(&file("notes.txt")), Kind::File);
        assert_eq!(kind_of("code ."), Kind::Command);
        assert_eq!(kind_of("  "), Kind::None);
        assert_eq!(Kind::Command.icon(), "›_");
    }

    #[test]
    fn variables_and_home_expand() {
        // SAFETY: this test is the only one touching this variable.
        unsafe { std::env::set_var("LUM_RUNNER_TEST", "x y") };
        assert_eq!(expand("$LUM_RUNNER_TEST/a"), "x y/a");
        assert_eq!(expand("${LUM_RUNNER_TEST}b"), "x yb");
        assert_eq!(expand("$NOT_SET_ANYWHERE_42"), "$NOT_SET_ANYWHERE_42");
        assert!(!expand("~/x").starts_with('~'));
        assert_eq!(split_args("-a 'b c' \"d\""), ["-a", "b c", "d"]);
    }

    #[cfg(unix)]
    #[test]
    fn commands_run_logged_and_can_be_stopped() {
        let dir = tempfile::tempdir().expect("temp");
        let log = dir.path().join("command_log.txt");
        let opts = Options { cwd: dir.path().to_string_lossy().into_owned(), ..Options::default() };
        let mut child = launch("sleep", "30", "", &opts, &log).expect("started").expect("tracked");
        assert!(stop(&mut child));
        let _ = child.wait();
        assert!(!stop(&mut child), "already stopped");
        let text = std::fs::read_to_string(&log).expect("logged");
        assert!(text.trim_end().ends_with("\tsleep 30"), "{text}");
        let missing = Options { cwd: "/no/such/folder".into(), ..Options::default() };
        assert_eq!(
            launch("ls", "", "", &missing, &log).err(),
            Some("The working folder does not exist: /no/such/folder".into())
        );
        assert_eq!(
            launch(" ", "", "", &opts, &log).err(),
            Some("No file or command is set for this item.".into())
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn keep_open_runs_a_shell_after() {
        let k = keep_open(&["htop".into()]);
        assert_eq!(&k[..2], ["sh", "-c"]);
        assert_eq!(k.last().map(String::as_str), Some("htop"));
    }
}
