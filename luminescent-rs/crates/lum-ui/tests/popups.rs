//! The command palette and the hotkey cheat sheet: pictures in Dark and Light, and how the palette searches,
//! moves, fills and runs. Pictures are compared on Linux only, like `snapshots.rs`.

#![allow(clippy::unwrap_used)]

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use lum_ui::Tokens;
use lum_ui::cheatsheet::{CheatSheet, Group};
use lum_ui::palette::{Entry, Palette};

fn index() -> Vec<Entry> {
    vec![
        Entry::new("page:dashboard", "Open Dashboard", "Luminescent", "page"),
        Entry::new("page:themes", "Open Themes", "Luminescent", "page"),
        Entry::new("act:timer:stopwatch", "New stopwatch", "Timer / Stopwatch · Ctrl+Alt+T", "action"),
        Entry::new("act:timer:countdown", "New countdown · 25:00", "Timer / Stopwatch", "action"),
        Entry::new("act:reminders:new", "Remind me at a time…", "Reminders · Ctrl+Alt+R", "action"),
        Entry::new("theme:dark", "Theme: Dark", "active", "theme"),
        Entry::new("theme:light", "Theme: Light", "theme", "theme"),
        Entry::new("settings:timer", "Settings › Timer / Stopwatch", "settings", "settings"),
        Entry::new("lum:Reset stuck keys", "Reset stuck keys", "Luminescent", "luminescent"),
    ]
}

struct State {
    palette: Palette,
    light: bool,
    ready: bool,
    ran: Vec<Entry>,
}

fn setup(ctx: &egui::Context, light: bool) {
    lum_ui::fonts::install(ctx);
    lum_ui::Motion::off().install(ctx);
    if light { Tokens::light() } else { Tokens::dark() }.install(ctx);
}

fn palette(dir: &tempfile::TempDir, light: bool, text: &str) -> Harness<'static, State> {
    let mut palette = Palette::new(dir.path().join("palette.json"));
    palette.open(index(), text, None);
    let builder = Harness::builder().with_size([1000.0, 800.0]).with_pixels_per_point(1.0);
    let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut State| {
            if !state.ready {
                setup(ui.ctx(), state.light);
                state.ready = true;
            }
            if let Some(entry) = state.palette.show(ui.ctx(), &|_| Vec::new()) {
                state.ran.push(entry);
            }
        },
        State { palette, light, ready: false, ran: Vec::new() },
    )
}

fn photograph<S>(harness: &mut Harness<'_, S>, name: &str) {
    harness.run();
    if cfg!(target_os = "linux") {
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(300);
        harness.snapshot_options(name, &options);
    }
}

#[test]
fn palette_dark() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut palette(&dir, false, "time"), "palette_dark");
}

#[test]
fn palette_light() {
    let dir = tempfile::tempdir().unwrap();
    photograph(&mut palette(&dir, true, ""), "palette_light");
}

#[test]
fn typing_narrows_the_list_and_enter_runs_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = palette(&dir, false, "");
    harness.run();
    assert_eq!(harness.state().palette.results().len(), 9);
    harness.event(egui::Event::Text("stopw".into()));
    harness.run();
    assert_eq!(harness.state().palette.results()[0].id, "act:timer:stopwatch");
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(harness.state().ran[0].id, "act:timer:stopwatch");
    assert!(!harness.state().palette.is_open());
    harness.state().palette.flush();
    let saved = std::fs::read_to_string(dir.path().join("palette.json")).unwrap();
    assert_eq!(saved, r#"{"recent": ["act:timer:stopwatch"]}"#);
}

#[test]
fn arrows_move_tab_fills_and_esc_closes() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = palette(&dir, false, "open");
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.run();
    harness.key_press_modifiers(Modifiers::NONE, Key::Tab);
    harness.run();
    let filled = harness.state().palette.text().to_owned();
    assert!(filled.starts_with("Open "), "{filled}");
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!harness.state().palette.is_open());
    assert!(harness.state().ran.is_empty());
}

#[test]
fn the_recent_come_back_first() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("palette.json"), r#"{"recent": ["theme:light"]}"#).unwrap();
    let mut harness = palette(&dir, false, "");
    harness.run();
    assert_eq!(harness.state().palette.results()[0].id, "theme:light");
}

struct Sheet {
    sheet: CheatSheet,
    light: bool,
    ready: bool,
}

fn groups() -> Vec<Group> {
    let rows = |list: &[(&str, &str)]| list.iter().map(|(l, c)| ((*l).to_owned(), (*c).to_owned())).collect();
    vec![
        Group {
            name: "Overlay & Interface".into(),
            on: true,
            rows: rows(&[
                ("Open Luminescent", "ctrl+shift+space"),
                ("Command palette", "ctrl+alt+space"),
                ("Hotkey cheat sheet", "ctrl+alt+/"),
            ]),
        },
        Group { name: "Notes".into(), on: true, rows: rows(&[("Show / hide Notes", "ctrl+alt+n")]) },
        Group { name: "Screen Doodle".into(), on: false, rows: rows(&[("Hold to draw", "alt")]) },
        Group {
            name: "Timer / Stopwatch".into(),
            on: true,
            rows: rows(&[
                ("New stopwatch", "ctrl+alt+t"),
                ("A label long enough to wrap onto a second line", "f9"),
            ]),
        },
        Group { name: "General".into(), on: true, rows: rows(&[("Quit all", "ctrl+u")]) },
    ]
}

fn sheet(light: bool, toggle: bool) -> Harness<'static, Sheet> {
    let mut sheet = CheatSheet::default();
    sheet.open(groups(), Vec::new(), toggle);
    let builder = Harness::builder().with_size([1600.0, 900.0]).with_pixels_per_point(1.0);
    let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut Sheet| {
            if !state.ready {
                setup(ui.ctx(), state.light);
                state.ready = true;
            }
            state.sheet.show(ui.ctx());
        },
        Sheet { sheet, light, ready: false },
    )
}

#[test]
fn cheat_sheet_dark() {
    photograph(&mut sheet(false, false), "cheatsheet_dark");
}

#[test]
fn cheat_sheet_light_toggle_mode() {
    photograph(&mut sheet(true, true), "cheatsheet_light");
}

#[test]
fn esc_closes_the_cheat_sheet_only_in_toggle_mode() {
    let mut hold = sheet(false, false);
    hold.run();
    hold.key_press(Key::Escape);
    hold.run();
    assert!(hold.state().sheet.is_open(), "hold mode goes when the keys are let go");
    let mut toggle = sheet(false, true);
    toggle.run();
    toggle.key_press(Key::Escape);
    toggle.run();
    assert!(!toggle.state().sheet.is_open());
}

struct Messages {
    toasts: lum_ui::toast::Toasts,
    look: lum_ui::toast::Look,
    alerts: lum_ui::alert::Alerts,
    light: bool,
    ready: bool,
    copied: Vec<String>,
}

fn messages(light: bool) -> Harness<'static, Messages> {
    use lum_ui::toast::{Look, Toasts};
    let mut toasts = Toasts::default();
    // the glow in Dark, the stripe in Light; the theme previews show the other styles
    let look = Look { style: if light { "stripe" } else { "glow" }.into(), ..Look::default() };
    toasts.push("Theme: Dark", "", 60_000, &look);
    toasts.push("Calculator", "12 * 7 = 84  (copied)", 60_000, &look);
    toasts.push("Luminescent error", "The overlay was rebuilt to recover.", 60_000, &look);
    toasts.push("⚠ Disk nearly full", "Only 2 GB are left on /home.", 60_000, &look);
    let long = "A long message that goes on. ".repeat(60);
    toasts.push("Reminders", &long, 60_000, &look);
    let mut alerts = lum_ui::alert::Alerts::default();
    alerts.show_alert(
        "CPU is hot",
        "The processor is at 92 °C. Close something heavy or let it cool down.",
        "warning",
        0.0,
        None,
    );
    let builder = Harness::builder().with_size([1400.0, 900.0]).with_pixels_per_point(1.0);
    let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(
        |ui, state: &mut Messages| {
            if !state.ready {
                setup(ui.ctx(), state.light);
                state.ready = true;
            }
            let screen = ui.ctx().content_rect();
            let out = state.toasts.show(ui.ctx(), &state.look, screen);
            state.copied.extend(out.copy);
            state.alerts.show(ui.ctx());
        },
        Messages { toasts, look, alerts, light, ready: false, copied: Vec::new() },
    )
}

#[test]
fn notification_cards_and_the_warning_dark() {
    photograph(&mut messages(false), "messages_dark");
}

#[test]
fn notification_cards_and_the_warning_light() {
    photograph(&mut messages(true), "messages_light");
}

#[test]
fn got_it_closes_the_warning_and_a_right_click_copies_a_card() {
    let mut harness = messages(false);
    harness.run();
    harness.get_by_label("Got it").click();
    harness.run();
    assert!(harness.state().alerts.is_empty());
    let card = harness.get_by_label("Theme: Dark").rect().center();
    harness.hover_at(card);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: card,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert_eq!(harness.state().copied, ["Theme: Dark"]);
}

#[test]
fn the_quit_question_answers_once() {
    use lum_ui::dialog::{AskDialog, DialogWindow, Outcome};
    struct Q {
        window: DialogWindow,
        answers: Vec<Outcome>,
        ready: bool,
    }
    let dialog = AskDialog::confirm(
        egui::Id::unique("q"),
        "Quit Luminescent",
        "Close Luminescent and every module?",
        "Quit",
        "Cancel",
        false,
    );
    let mut harness = Harness::builder().with_size([900.0, 600.0]).build_ui_state(
        |ui, q: &mut Q| {
            if !q.ready {
                setup(ui.ctx(), false);
                q.ready = true;
            }
            let outcome = q.window.show(ui.ctx());
            if outcome != Outcome::Open {
                q.answers.push(outcome);
            }
        },
        Q { window: DialogWindow::new(dialog, None), answers: Vec::new(), ready: false },
    );
    harness.run();
    harness.get_by_label("Quit").click();
    harness.run();
    assert_eq!(harness.state().answers, [Outcome::Accepted("Quit".into())]);
}
