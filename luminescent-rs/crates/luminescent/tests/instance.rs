//! The built binary: --smoke, one copy per data folder, a second start handing over, ctl, a clean quit.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn luminescent(home: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_luminescent"));
    cmd.args(args).env("LUMINESCENT_HOME", home).env_remove("LUMINESCENT_SAFE_MODE");
    cmd
}

fn run(home: &Path, args: &[&str]) -> Output {
    luminescent(home, args).output().unwrap()
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).trim().to_owned()
}

/// With a display (CI runs this file again under Xvfb) the app runs inside eframe; without one it waits on
/// its channels. Both must shut down the same way.
fn has_display() -> bool {
    cfg!(windows)
        || ["WAYLAND_DISPLAY", "DISPLAY"].iter().any(|v| std::env::var_os(v).is_some_and(|v| !v.is_empty()))
}

fn wait_for_ping(home: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while text(&run(home, &["ctl", "ping"]).stdout) != "pong" {
        assert!(Instant::now() < deadline, "the first copy never answered");
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_exit(child: &mut std::process::Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(code) = child.try_wait().unwrap() {
            return code;
        }
        assert!(Instant::now() < deadline, "it did not quit");
        thread::sleep(Duration::from_millis(50));
    }
}

/// What a clean shutdown leaves behind, whichever way it was asked for.
fn assert_shut_down_cleanly(home: &Path) {
    let log = fs::read_to_string(home.join("data/launcher.log")).unwrap();
    assert!(log.contains("launcher: shutting down"), "{log}");
    if has_display() && cfg!(target_os = "linux") {
        assert!(log.contains("drawing with "), "with a display it runs inside eframe: {log}");
    }
    assert!(!home.join("data/starting.json").exists(), "a clean quit is not a failed start");
    assert_eq!(run(home, &["ctl", "ping"]).status.code(), Some(2), "nothing is listening any more");
}

#[test]
fn smoke_passes_on_a_fresh_folder() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(dir.path(), &["--smoke"]);
    assert!(out.status.success(), "{}\n{}", text(&out.stdout), text(&out.stderr));
    assert_eq!(text(&out.stdout), "SMOKE OK");
    let log = fs::read_to_string(dir.path().join("data/launcher.log")).unwrap();
    assert!(log.contains(" INFO    launcher: ready: "), "{log}");
    assert!(!log.contains("ERROR") && !log.contains("WARNING"), "{log}");
}

#[test]
fn a_second_start_hands_over_and_ctl_quits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let mut first = luminescent(home, &[]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    wait_for_ping(home);

    let second = run(home, &[]);
    assert!(second.status.success());
    assert!(text(&second.stdout).contains("already running"), "{}", text(&second.stdout));

    let smoke = run(home, &["--smoke"]);
    assert_eq!(smoke.status.code(), Some(2), "smoke steps aside for a running copy");
    assert_eq!(text(&smoke.stdout), "SMOKE skipped: already running");

    let status = text(&run(home, &["ctl", "status"]).stdout);
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status["pid"], first.id());

    let unknown = run(home, &["ctl", "action", "nothing:here"]);
    assert_eq!(unknown.status.code(), Some(1));

    assert_eq!(text(&run(home, &["ctl", "quit"]).stdout), "bye");
    assert!(wait_for_exit(&mut first).success());

    let log = fs::read_to_string(home.join("data/launcher.log")).unwrap();
    assert!(log.contains("asked to open: overlay"), "{log}");
    assert_shut_down_cleanly(home);
}

#[cfg(unix)]
#[test]
fn ctrl_c_quits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let mut app = luminescent(home, &[]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    wait_for_ping(home);
    let sent = Command::new("kill").args(["-INT", &app.id().to_string()]).status().unwrap();
    assert!(sent.success());
    assert!(wait_for_exit(&mut app).success());
    let log = fs::read_to_string(home.join("data/launcher.log")).unwrap();
    assert!(log.contains("quitting: asked to by the system"), "{log}");
    assert_shut_down_cleanly(home);
}
