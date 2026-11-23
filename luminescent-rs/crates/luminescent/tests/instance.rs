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

    let deadline = Instant::now() + Duration::from_secs(20);
    while text(&run(home, &["ctl", "ping"]).stdout) != "pong" {
        assert!(Instant::now() < deadline, "the first copy never answered");
        thread::sleep(Duration::from_millis(100));
    }

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
    let deadline = Instant::now() + Duration::from_secs(10);
    let code = loop {
        if let Some(code) = first.try_wait().unwrap() {
            break code;
        }
        assert!(Instant::now() < deadline, "the first copy did not quit");
        thread::sleep(Duration::from_millis(50));
    };
    assert!(code.success());

    let log = fs::read_to_string(home.join("data/launcher.log")).unwrap();
    assert!(log.contains("asked to open: overlay"), "{log}");
    assert!(log.contains("launcher: shutting down"), "{log}");
    assert!(!home.join("data/starting.json").exists(), "a clean quit is not a failed start");
    assert_eq!(run(home, &["ctl", "ping"]).status.code(), Some(2), "nothing is listening any more");
}
