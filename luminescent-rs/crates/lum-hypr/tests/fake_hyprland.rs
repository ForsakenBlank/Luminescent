#![cfg(unix)]

use lum_hypr::events::{self, Event};
use lum_hypr::fake::FakeHyprland;
use lum_hypr::lua::{self, BindOptions};
use lum_hypr::sync::{Want, WindowSync};
use lum_hypr::{fifo, marker};
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("a runtime")
}

#[test]
fn bootstrap_and_binds_go_through_eval() {
    let fake = FakeHyprland::start();
    let hypr = fake.hypr();
    assert!(hypr.available());
    hypr.bootstrap().expect("the fake answers ok");
    hypr.bind("hk1", "CTRL + SHIFT + space", BindOptions::default()).expect("ok");
    let sent = fake.sent();
    assert_eq!(sent[0], format!("eval {}", lua::bootstrap(hypr.names())));
    assert!(sent[0].starts_with("eval LUMRS = LUMRS or {}"));
    assert_eq!(
        sent[1],
        "eval LUMRS.bind(\"CTRL + SHIFT + space|0|0\", \"hk1\", \"CTRL + SHIFT + space\", { description = \"Luminescent\" })"
    );
}

#[test]
fn a_bind_after_a_lost_table_bootstraps_and_tries_again() {
    let fake = FakeHyprland::start();
    let first = std::sync::atomic::AtomicBool::new(true);
    fake.answer(move |c| {
        (c.contains("LUMRS.bind") && first.swap(false, std::sync::atomic::Ordering::SeqCst))
            .then(|| "attempt to index a nil value (global 'LUMRS')".to_owned())
    });
    fake.hypr().bind("hk1", "ALT + Alt_L", BindOptions { release: true, ..Default::default() }).expect("ok");
    let sent = fake.sent();
    assert_eq!(sent.len(), 3);
    assert!(sent[1].starts_with("eval LUMRS = LUMRS or {}"), "bootstrap in between");
    assert_eq!(sent[0], sent[2]);
}

#[test]
fn queries_parse() {
    let fake = FakeHyprland::start();
    fake.set_clients(r#"[{"address": "0x1", "class": "luminescent", "title": "x", "at": [10, 20], "size": [300, 200], "pid": 7, "floating": true, "pinned": true, "monitor": 0, "focusHistoryID": 0, "workspace": {"id": 1}}]"#);
    fake.set_monitors(r#"[{"name": "DP-1", "x": 0, "y": 0, "width": 3840, "height": 2160, "scale": 2.0, "transform": 0, "focused": true, "reserved": [0, 40, 0, 0]},
                          {"name": "HDMI-A-1", "x": 1920, "y": 0, "width": 1920, "height": 1080, "scale": 1.0, "transform": 1, "focused": false, "reserved": []}]"#);
    fake.answer(|c| (c == "j/cursorpos").then(|| r#"{"x": 1500.5, "y": 30}"#.to_owned()));
    let hypr = fake.hypr();
    let c = hypr.client("0x1").expect("the client");
    assert_eq!((c.at, c.size, c.pid), ((10, 20), (300, 200), 7));
    let m = hypr.monitors();
    assert_eq!(m[0].logical_size(), (1920, 1080));
    assert_eq!(m[0].reserved(), [0, 40, 0, 0]);
    assert_eq!(m[1].logical_size(), (1080, 1920));
    assert_eq!(m[1].reserved(), [0; 4]);
    assert_eq!(hypr.cursor_pos(), Some((1500, 30)));
}

#[test]
fn key_down_reads_the_lua_answer() {
    let fake = FakeHyprland::start();
    fake.answer(|c| c.starts_with("repl return").then(|| "true\n".to_owned()));
    assert_eq!(fake.hypr().key_down(&["Alt_L", "Alt_R"]), Some(true));
    assert_eq!(fake.sent()[0], "repl return hl.is_key_down(\"Alt_L\") or hl.is_key_down(\"Alt_R\")");
    assert_eq!(fake.hypr().key_down(&[]), None);
}

#[test]
fn foreign_binds_leave_ours_out() {
    let fake = FakeHyprland::start();
    fake.answer(|c| {
        (c == "j/binds").then(|| {
            r#"[{"key": "Q", "description": "Luminescent"}, {"key": "W", "description": ""}]"#.into()
        })
    });
    let binds = fake.hypr().foreign_binds();
    assert_eq!(binds.len(), 1);
    assert_eq!(binds[0]["key"], "W");
}

#[test]
fn a_config_reload_reinstalls_the_lua_before_it_is_passed_on() {
    let fake = FakeHyprland::start();
    let hypr = fake.hypr();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        runtime().block_on(events::listen(hypr, move |e| tx.send(e).expect("the test waits")))
    });
    assert!(fake.wait_for_listener());
    fake.event("openwindow>>abc,1,luminescent,Luminescent Notes");
    fake.event("configreloaded>>");
    let wait = Duration::from_secs(3);
    assert!(matches!(rx.recv_timeout(wait), Ok(Event::OpenWindow { addr, .. }) if addr == "0xabc"));
    assert_eq!(rx.recv_timeout(wait), Ok(Event::ConfigReloaded));
    assert!(fake.sent().iter().any(|c| c.starts_with("eval LUMRS = LUMRS or {}")), "bootstrap ran first");
}

#[test]
fn lines_in_the_fifo_reach_the_app() {
    let fake = FakeHyprland::start();
    let path = fake.fifo();
    let (tx, rx) = mpsc::channel();
    let listening = path.clone();
    std::thread::spawn(move || {
        runtime().block_on(fifo::listen(&listening, move |line| tx.send(line).expect("the test waits")))
    });
    let end = std::time::Instant::now() + Duration::from_secs(3);
    while !path.exists() && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    // the way the bootstrap's printf writes: open, one line, close
    for id in ["hk1", "hk2"] {
        let mut w = std::fs::OpenOptions::new().write(true).open(&path).expect("open the FIFO");
        writeln!(w, "{id}").expect("write");
    }
    let wait = Duration::from_secs(3);
    assert_eq!(rx.recv_timeout(wait).as_deref(), Ok("hk1"));
    assert_eq!(rx.recv_timeout(wait).as_deref(), Ok("hk2"));
}

#[test]
fn fifo_create_replaces_a_plain_file() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("luminescent").join("events-rs.fifo");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    std::fs::write(&path, "not a fifo").expect("write");
    fifo::create(&path).expect("create");
    use std::os::unix::fs::FileTypeExt;
    assert!(std::fs::metadata(&path).expect("meta").file_type().is_fifo());
    fifo::create(&path).expect("again is fine");
}

fn notes(marker_char: Option<char>) -> String {
    let mut t = "Luminescent Notes".to_owned();
    t.extend(marker_char);
    t
}

fn client_at(addr: &str, at: (i32, i32)) -> String {
    format!(
        r#"[{{"address": "{addr}", "class": "luminescent", "title": "t", "at": [{}, {}], "size": [400, 300]}}]"#,
        at.0, at.1
    )
}

#[test]
fn a_new_window_is_moved_where_the_app_wants_it_and_kept_there() {
    let fake = FakeHyprland::start();
    // Hyprland's own spot at first, as it does with two monitors
    fake.set_clients(&client_at("0x9", (0, 0)));
    let (sync, _thread) = WindowSync::start(fake.hypr());
    let title = notes(Some(marker::NO_ACTIVATE));
    sync.want(Want { title: title.clone(), at: Some((100, 200)), focus: false });
    sync.event(&Event::parse(&format!("openwindow>>9,1,luminescent,{title}")));
    let pinned_move = lua::move_window("0x9", 100, 200, true);
    assert!(fake.wait_for(|c| c == format!("eval {pinned_move}")).is_some());
    // the settle checks see it is still wrong and move it again
    let moves = || fake.sent().iter().filter(|c| c.contains("window.move")).count();
    let end = std::time::Instant::now() + Duration::from_secs(3);
    while moves() < 2 && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(moves() >= 2, "moved again while Hyprland held on to it");
    fake.set_clients(&client_at("0x9", (100, 200)));
    std::thread::sleep(Duration::from_millis(700));
    let settled = moves();
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(moves(), settled, "no more moves once it stayed put");

    // the app moves it: one move, straight away
    fake.clear();
    sync.want(Want { title: title.clone(), at: Some((150, 200)), focus: false });
    let next = lua::move_window("0x9", 150, 200, true);
    assert!(fake.wait_for(|c| c == format!("eval {next}")).is_some());
    // the same wish again does nothing
    sync.want(Want { title, at: Some((150, 200)), focus: false });
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.sent().iter().filter(|c| c.contains("window.move")).count(), 1);
    sync.stop();
}

#[test]
fn a_window_that_maps_first_is_found_when_the_wish_arrives() {
    let fake = FakeHyprland::start();
    fake.set_clients(&client_at("0x5", (30, 40)));
    let (sync, _thread) = WindowSync::start(fake.hypr());
    let title = notes(Some(marker::NO_PIN));
    sync.event(&Event::parse(&format!("openwindow>>5,1,luminescent,{title}")));
    sync.want(Want { title: notes(None), at: Some((30, 40)), focus: true });
    let unpinned = lua::move_window("0x5", 30, 40, false);
    assert!(fake.wait_for(|c| c == format!("eval {unpinned}")).is_some(), "matched without the markers");
    assert!(fake.wait_for(|c| c == format!("eval {}", lua::focus("0x5"))).is_some());
    sync.stop();
}

#[test]
fn other_programs_and_closed_windows_are_left_alone() {
    let fake = FakeHyprland::start();
    let (sync, _thread) = WindowSync::start(fake.hypr());
    sync.event(&Event::parse("openwindow>>7,1,kitty,Luminescent Notes"));
    sync.want(Want { title: notes(None), at: Some((1, 2)), focus: false });
    sync.event(&Event::parse("openwindow>>8,1,luminescent,Luminescent Notes"));
    sync.event(&Event::parse("closewindow>>8"));
    sync.raise(&notes(None));
    std::thread::sleep(Duration::from_millis(200));
    let sent = fake.sent();
    assert!(!sent.iter().any(|c| c.contains("0x7")), "kitty is not ours");
    assert!(!sent.iter().any(|c| c.contains("alter_zorder")), "closed windows are forgotten");
    sync.stop();
}

#[test]
fn float_on_top_centres_another_programs_window() {
    let fake = FakeHyprland::start();
    fake.set_clients(r#"[{"address": "0x3", "class": "kitty", "at": [0, 0], "size": [200, 100]}]"#);
    let (sync, _thread) = WindowSync::start(fake.hypr());
    sync.float_on_top("0x3", lum_hypr::sync::Area { x: 0, y: 0, w: 1920, h: 1080 });
    assert!(fake.wait_for(|c| c.contains("hl.dsp.focus")).is_some());
    let sent = fake.sent();
    assert!(sent.iter().any(|c| c == &format!("eval {}", lua::resize("0x3", 1200, 800))));
    assert!(sent.iter().any(|c| c == &format!("eval {}", lua::move_other("0x3", 360, 140))));
    sync.stop();
}
