//! Which terminal opens scripts, "Open a terminal" and the Prompt Log: the one set in Settings → Dashboard &
//! Scripts, else `$TERMINAL`, else the first installed of a known list. On Windows, Windows Terminal (a new
//! tab) when it is there, else a console window.
//!
//! The theme colours on the command line (kitty, alacritty, foot) come with phase 14; callers pass them in.

use std::path::{Path, PathBuf};

pub const TERMINALS: [&str; 9] = [
    "kitty",
    "alacritty",
    "foot",
    "wezterm",
    "ghostty",
    "konsole",
    "gnome-terminal",
    "xfce4-terminal",
    "xterm",
];

/// The program, found on PATH.
pub fn which(program: &str) -> Option<PathBuf> {
    let program = program.trim();
    if program.is_empty() {
        return None;
    }
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    let exts: Vec<String> = if cfg!(windows) {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_owned());
        std::iter::once(String::new()).chain(pathext.split(';').map(str::to_lowercase)).collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .find_map(|dir| exts.iter().map(|ext| dir.join(format!("{program}{ext}"))).find(|p| is_runnable(p)))
}

#[cfg(unix)]
fn is_runnable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_runnable(p: &Path) -> bool {
    p.is_file()
}

/// Windows Terminal, if installed.
pub fn wt() -> Option<PathBuf> {
    if cfg!(windows) { which("wt") } else { None }
}

/// The terminal to use: `configured`, then the preferred one from Settings, then `$TERMINAL`, then the list.
/// On Windows: `configured` or the preferred one, else Windows Terminal, else PowerShell's console.
pub fn find(configured: &str, preferred: &str) -> Option<PathBuf> {
    let first_word = |s: &str| s.split_whitespace().next().unwrap_or("").to_owned();
    let mut wanted = vec![first_word(configured), first_word(preferred)];
    if cfg!(windows) {
        return wanted.iter().find_map(|c| which(c)).or_else(wt).or_else(|| Some("powershell.exe".into()));
    }
    wanted.push(first_word(&std::env::var("TERMINAL").unwrap_or_default()));
    wanted.extend(TERMINALS.iter().map(|t| (*t).to_owned()));
    wanted.iter().find_map(|c| which(c))
}

/// The command line that opens a new window of `term` running `argv` in `cwd`, titled `title`. `colours`
/// are the theme's command-line options for that terminal (empty for none).
pub fn command(term: &Path, title: &str, cwd: &Path, argv: &[String], colours: &[String]) -> Vec<String> {
    let t = term.to_string_lossy().into_owned();
    let cwd = cwd.to_string_lossy().into_owned();
    let name = term.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let s = |v: &str| v.to_owned();
    let mut out: Vec<String> = match name.as_str() {
        "kitty" => vec![t, s("--title"), s(title), s("--directory"), cwd],
        "alacritty" => vec![t, s("--title"), s(title), s("--working-directory"), cwd],
        "foot" => vec![t, format!("--title={title}"), format!("--working-directory={cwd}")],
        "wezterm" => return [vec![t, s("start"), s("--cwd"), cwd, s("--")], argv.to_vec()].concat(),
        "ghostty" => vec![t, format!("--title={title}"), format!("--working-directory={cwd}"), s("-e")],
        "konsole" => vec![t, s("--workdir"), cwd, s("-p"), format!("tabtitle={title}"), s("-e")],
        "gnome-terminal" => vec![t, s("--title"), s(title), format!("--working-directory={cwd}"), s("--")],
        "xfce4-terminal" => vec![t, s("--title"), s(title), s("--working-directory"), cwd, s("-x")],
        "wt" | "wt.exe" => {
            // ';' separates wt's sub-commands
            let mut args = vec![t, s("-w"), s("0"), s("new-tab"), s("--title"), title.replace(';', r"\;")];
            args.extend([s("-d"), cwd]);
            args.extend(argv.iter().map(|a| a.replace(';', r"\;")));
            return args;
        }
        _ if cfg!(windows) => return argv.to_vec(),
        _ => vec![t, s("-T"), s(title), s("-e")],
    };
    if matches!(name.as_str(), "kitty" | "alacritty" | "foot") {
        out.extend(colours.iter().cloned());
    }
    if name == "alacritty" {
        out.push(s("-e"));
    }
    out.extend(argv.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_terminal_command_matches_python() {
        let cases = lum_fixtures::read_json("terminal_cases/cases.json");
        let argv: Vec<String> = ["htop", "-d", "5"].map(str::to_owned).to_vec();
        for case in cases["terminal_commands"].as_array().expect("cases") {
            let term = case["terminal"].as_str().expect("name");
            let want: Vec<String> = case["command"]
                .as_array()
                .expect("list")
                .iter()
                .map(|v| v.as_str().expect("text").to_owned())
                .collect();
            let got =
                command(Path::new(&format!("/usr/bin/{term}")), "Title", Path::new("/tmp/work"), &argv, &[]);
            assert_eq!(got, want, "{term}");
        }
    }

    #[test]
    fn windows_terminal_tabs_escape_semicolons() {
        let got = command(Path::new("wt"), "a;b", Path::new("C:/x"), &["echo".into(), "1;2".into()], &[]);
        assert_eq!(got, ["wt", "-w", "0", "new-tab", "--title", r"a\;b", "-d", "C:/x", "echo", r"1\;2"]);
    }

    #[test]
    fn colours_go_before_the_command() {
        let colours = ["-o".to_owned(), "background=#000".to_owned()];
        let got = command(Path::new("kitty"), "T", Path::new("/"), &["sh".into()], &colours);
        assert_eq!(got, ["kitty", "--title", "T", "--directory", "/", "-o", "background=#000", "sh"]);
    }
}
