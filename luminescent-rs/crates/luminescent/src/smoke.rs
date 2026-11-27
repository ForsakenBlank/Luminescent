//! `--smoke`: start, check every part once, then quit with 0 (1 on any error). CI runs it on Linux and Windows.
//! Each phase adds its pages, panels and pop-ups here, as the Python `_smoke` builds every page.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lum_core::actions::Action;
use lum_core::backup;

use crate::launcher::App;

type Check = fn(&mut App) -> Result<(), String>;

const CHECKS: &[(&str, Check)] = &[
    ("settings save and load back", settings_round_trip),
    ("profiles", profiles),
    ("event bus", event_bus),
    ("actions", actions),
    ("backup", backup_zip),
    ("ctl socket", ctl_socket),
    ("widget kit in Dark and Light", widget_kit),
    ("hotkeys and the tray menu", hello_tray),
    ("the overlay: open, switch pages, close", overlay),
    ("command palette and cheat sheet", palette_and_cheat_sheet),
    ("notification cards, the warning, prewarm and quit", messages),
    ("themes switch and restyle", themes),
    ("themes window, editor and colour dialog", themes_window),
    ("dashboard: boxes, items, palette, tray and running", dashboard),
    ("settings page: every section, save and apply", settings_page),
    ("everyday tools: panels, pages, palette", everyday_tools),
    ("screen tools: frozen screens, picker, ruler, text grab", screen_tools),
];

pub fn run(app: &mut App) -> u8 {
    let mut errors: Vec<String> = Vec::new();
    for (name, check) in CHECKS {
        let outcome = lum_core::guard::safely(name, || check(app));
        match outcome {
            Some(Ok(())) => {}
            Some(Err(e)) => errors.push(format!("{name}: {e}")),
            None => errors.push(format!("{name}: panicked (see the log)")),
        }
    }
    errors.extend(app.host.errors().into_iter().map(|(key, e)| format!("module {key} failed: {e}")));
    let report = if errors.is_empty() { "OK".to_owned() } else { format!("FAILED\n{}", errors.join("\n")) };
    tracing::info!("SMOKE {report}");
    println!("SMOKE {report}");
    u8::from(!errors.is_empty())
}

fn settings_round_trip(app: &mut App) -> Result<(), String> {
    let before = app.settings.all().clone();
    app.settings.save(false).map_err(|e| e.to_string())?;
    app.settings.reload();
    if app.settings.all() != &before {
        return Err("settings.json read back differently".into());
    }
    Ok(())
}

fn profiles(app: &mut App) -> Result<(), String> {
    let s = &mut app.settings;
    let start = s.active_profile();
    let id = s.create_profile("Smoke test").map_err(|e| e.to_string())?;
    s.switch_profile(&id, false).map_err(|e| e.to_string())?;
    if s.active_profile() != id {
        return Err("could not switch to a new profile".into());
    }
    s.delete_profile(&id).map_err(|e| e.to_string())?;
    if s.active_profile() != start || s.profiles().iter().any(|p| p.id == id) {
        return Err("deleting the profile did not go back to the one before".into());
    }
    Ok(())
}

fn event_bus(app: &mut App) -> Result<(), String> {
    let heard = app.events.subscribe();
    app.emit("theme.changed", "smoke");
    match heard.try_recv() {
        Ok(event) if event.name == "theme.changed" && event.text == "smoke" => Ok(()),
        other => Err(format!("expected the event back, got {other:?}")),
    }
}

fn actions(app: &mut App) -> Result<(), String> {
    let ran = Arc::new(AtomicBool::new(false));
    let flag = ran.clone();
    app.actions
        .set_owner("smoke", vec![Action::from_label("Check", move || flag.store(true, Ordering::SeqCst))]);
    let ok = app.actions.run("smoke:check").map(|_| ()).map_err(|e| e.to_string());
    app.actions.remove_owner("smoke");
    ok?;
    if !ran.load(Ordering::SeqCst) {
        return Err("the action did not run".into());
    }
    Ok(())
}

fn backup_zip(app: &mut App) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let zip = dir.path().join("smoke.zip");
    let count = backup::export(&app.paths, &zip).map_err(|e| e.to_string())?;
    let members = backup::check(&zip).map_err(|e| e.to_string())?;
    if members.len() != count {
        return Err(format!("wrote {count} files but the zip lists {}", members.len()));
    }
    Ok(())
}

/// Builds every widget, the panel and each dialog on a context with no window, so it passes in CI on both
/// systems with no display.
fn widget_kit(_app: &mut App) -> Result<(), String> {
    lum_ui::gallery::build_headless()
}

/// The Themes page, every page of the editor, every quick tool and the colour dialog, in three looks, in a
/// folder of their own so the real themes stay as they are.
fn themes_window(app: &mut App) -> Result<(), String> {
    let dir = app.paths.home.join("smoke-themes");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    lum_ui::themes::demo::build_headless(&dir)?;
    let (answer, _) = app.handle_ctl("overlay themes");
    if answer != "ok" || !app.overlay.is_open() || app.overlay.current() != "themes" {
        return Err(format!("ctl overlay themes did not open the Themes page ({answer})"));
    }
    app.hide_overlay();
    let ctx = headless_context();
    for _ in 0..2 {
        lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
    }
    if app.overlay.is_visible() {
        return Err("the overlay did not go away".into());
    }
    Ok(())
}

/// Talks to its own ctl socket. The answer comes from this thread, so the request goes from another one.
fn ctl_socket(app: &mut App) -> Result<(), String> {
    let endpoint = app.endpoint.clone();
    let client = std::thread::spawn(move || endpoint.request("ping", Duration::from_secs(5)));
    let request =
        app.ctl_requests.recv_timeout(Duration::from_secs(5)).map_err(|_| "no request arrived".to_owned())?;
    let (answer, _) = app.handle_ctl(&request.line);
    let _ = request.reply.send(answer);
    match client.join() {
        Ok(Ok(answer)) if answer == "pong" => Ok(()),
        Ok(Ok(answer)) => Err(format!("answered {answer:?}")),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("the client thread panicked".into()),
    }
}

/// The Hello tray parts that need no desktop: the hotkey books, `ctl overlay`, the tray menu and icon, and the
/// overlay drawn without windows until Esc closes it.
fn hello_tray(app: &mut App) -> Result<(), String> {
    use lum_platform::hotkeys::Options;
    app.hotkeys.register("smoke", "ctrl+shift+f12", "x", "smoke", Options::default())?;
    if app.hotkeys.owner_of("Ctrl + Shift + F12") != Some("smoke") {
        return Err("the hotkey was not registered".into());
    }
    app.hotkeys.unregister_all("smoke");
    let menu = app.tray_menu();
    if !menu.iter().any(|m| matches!(m, lum_platform::tray::MenuItem::Action { id, .. } if id == "open")) {
        return Err("the tray menu has no Open".into());
    }
    if lum_platform::tray::sparkle_icon(32, [0x7B, 0x6C, 0xF6]).rgba.iter().all(|&b| b == 0) {
        return Err("the tray icon is empty".into());
    }
    Ok(())
}

fn headless_context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(true);
    lum_ui::setup(&ctx, lum_ui::Tokens::dark());
    lum_ui::Motion::off().install(&ctx);
    ctx
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn input(events: &[egui::Event]) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))),
        events: events.to_vec(),
        ..Default::default()
    }
}

/// The overlay drawn without windows: `ctl overlay` opens it, Ctrl+3 goes to the third page, typing on the
/// Themes page asks for the palette, Esc closes it and the page it was on is remembered.
fn overlay(app: &mut App) -> Result<(), String> {
    use egui::{Key, Modifiers};
    let (answer, _) = app.handle_ctl("overlay");
    if answer != "ok" || !app.overlay.is_open() {
        return Err(format!("ctl overlay answered {answer:?}"));
    }
    let ctx = headless_context();
    let pass = |app: &mut App, events: &[egui::Event]| {
        let output = lum_ui::headless_pass(&ctx, input(events), |ui| app.show_modules(ui.ctx()));
        ctx.tessellate(output.shapes, output.pixels_per_point).len()
    };
    pass(app, &[]);
    if pass(app, &[]) == 0 {
        return Err("the overlay drew nothing".into());
    }
    if app.open_times.is_empty() {
        return Err("the open was not timed".into());
    }
    pass(app, &[key(Key::Num3, Modifiers::CTRL)]);
    if app.overlay.current() != "themes" {
        return Err(format!("Ctrl+3 went to {}", app.overlay.current()));
    }
    pass(app, &[]);
    let empty = app.overlay_config.chrome_items.is_empty();
    app.overlay.chrome.set_editing(true, empty);
    pass(app, &[]);
    pass(app, &[]);
    if app.settings.get("ui.chrome_items").and_then(|v| v.as_array()).map_or(0, Vec::len) != 3 {
        return Err("editing the layout did not save the default one".into());
    }
    pass(app, &[key(Key::Escape, Modifiers::NONE)]);
    if app.overlay.chrome.editing || !app.overlay.is_open() {
        return Err("Esc did not just leave edit mode".into());
    }
    for page in ["settings", "clipboard", "automation", "dashboard"] {
        app.open_overlay(page);
        pass(app, &[]);
        pass(app, &[]);
    }
    let (answer, _) = app.handle_ctl("overlay nowhere");
    if !answer.starts_with("error:") {
        return Err(format!("an unknown page answered {answer:?}"));
    }
    pass(app, &[key(Key::Escape, Modifiers::NONE)]);
    pass(app, &[]);
    if app.overlay.is_visible() {
        return Err("Esc did not close the overlay".into());
    }
    if app.settings.get_str("ui.last_page") != Some("dashboard") {
        return Err("the page it was on was not remembered".into());
    }
    Ok(())
}

/// The palette finds a theme by a few letters and Enter applies it; the cheat sheet lists the overlay's
/// hotkeys and Esc closes it in toggle mode.
fn palette_and_cheat_sheet(app: &mut App) -> Result<(), String> {
    use egui::{Key, Modifiers};
    let start = app.themes.active_id().to_owned();
    let (answer, _) = app.handle_ctl("palette");
    if answer != "ok" || !app.palette.is_open() {
        return Err(format!("ctl palette answered {answer:?}"));
    }
    let ctx = headless_context();
    let pass = |app: &mut App, events: &[egui::Event]| {
        lum_ui::headless_pass(&ctx, input(events), |ui| app.show_modules(ui.ctx()));
    };
    // an area is only measured on its first pass
    pass(app, &[]);
    pass(app, &[]);
    pass(app, &[egui::Event::Text("theme light".into())]);
    match app.palette.results().first() {
        Some(e) if e.id == "theme:light" => {}
        other => return Err(format!("\"theme light\" found {other:?} first")),
    }
    pass(app, &[key(Key::Enter, Modifiers::NONE)]);
    if app.palette.is_open() || app.themes.active_id() != "light" {
        return Err("Enter did not apply the theme it found".into());
    }
    app.apply_theme(&start);
    app.cheat_sheet("", true);
    pass(app, &[]);
    pass(app, &[]);
    if !app.cheatsheet.is_open() {
        return Err("the cheat sheet did not open".into());
    }
    pass(app, &[key(Key::Escape, Modifiers::NONE)]);
    if app.cheatsheet.is_open() {
        return Err("Esc did not close the cheat sheet".into());
    }
    Ok(())
}

/// A card from `ctl notify`, the warning from `ctl alert` closed with Enter, the overlay's prewarm, and the quit
/// question (Cancel keeps it running).
fn messages(app: &mut App) -> Result<(), String> {
    use egui::{Key, Modifiers};
    app.cards_on();
    let ctx = headless_context();
    let pass = |app: &mut App, events: &[egui::Event]| {
        let output = lum_ui::headless_pass(&ctx, input(events), |ui| app.show_modules(ui.ctx()));
        ctx.tessellate(output.shapes, output.pixels_per_point).len()
    };
    for line in
        ["notify Smoke test | A card from the smoke test", "alert Smoke test | A warning in the middle"]
    {
        let (answer, _) = app.handle_ctl(line);
        if answer != "ok" {
            return Err(format!("ctl {line} answered {answer:?}"));
        }
    }
    pass(app, &[]);
    if app.toasts.len() != 1 || pass(app, &[]) == 0 {
        return Err("the card was not drawn".into());
    }
    pass(app, &[key(Key::Enter, Modifiers::NONE)]);
    if !app.alerts.is_empty() {
        return Err("Enter did not close the warning".into());
    }
    app.handle(crate::launcher::AppMsg::Prewarm);
    pass(app, &[]);
    app.confirm_quit();
    pass(app, &[]);
    pass(app, &[]);
    pass(app, &[key(Key::Escape, Modifiers::NONE)]);
    if app.quitting {
        return Err("Cancel on the quit question quit".into());
    }
    app.toasts.clear();
    Ok(())
}

/// A theme switch by name, next and back: the tokens for egui change, and the active theme is reported.
fn themes(app: &mut App) -> Result<(), String> {
    let start = app.themes.active_id().to_owned();
    let (answer, _) = app.handle_ctl("theme light");
    if answer != "ok: Light" {
        return Err(format!("ctl theme light answered {answer:?}"));
    }
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(true);
    lum_ui::setup(&ctx, lum_ui::Tokens::dark());
    let tokens = app.take_restyle().ok_or("switching themes made no new tokens")?;
    if !tokens.p.is_light() {
        return Err("Light is not light".into());
    }
    tokens.install(&ctx);
    if !lum_ui::Tokens::of(&ctx).p.is_light() {
        return Err("the new tokens were not installed".into());
    }
    let (answer, _) = app.handle_ctl("theme back");
    if answer != format!("ok: {}", app.themes.get(&start).map_or("", |t| t.name())) {
        return Err(format!("ctl theme back answered {answer:?}"));
    }
    if app.take_restyle().is_none() {
        return Err("going back made no new tokens".into());
    }
    let (answer, _) = app.handle_ctl("theme no such theme");
    if !answer.starts_with("error:") {
        return Err(format!("an unknown theme answered {answer:?}"));
    }
    Ok(())
}

/// The dashboard drawn without windows: a box with a heading and an item, the page open on it, the item in the
/// palette and the tray, and a group with nothing in it run without trouble.
fn dashboard(app: &mut App) -> Result<(), String> {
    use serde_json::json;
    let tab = app.dashboard.add_tab("Smoke", 96, 232, "", "");
    app.dashboard.add_item(&tab, lum_core::dashboard::DashboardStore::blank_item("heading", "Things"), -1);
    let mut item = lum_core::dashboard::DashboardStore::blank_item("item", "Say hello");
    item.insert("path".into(), json!("echo"));
    item.insert("hotkey".into(), json!("ctrl+alt+9"));
    app.dashboard.add_item(&tab, item, -1);
    let group = lum_core::dashboard::DashboardStore::blank_item("group", "Nothing yet");
    app.dashboard.add_item(&tab, group, -1);
    app.open_overlay("dashboard");
    let ctx = headless_context();
    // the first frame of an open is still fading in and paints nothing
    let mut drawn = 0;
    for _ in 0..3 {
        let output = lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
        drawn = ctx.tessellate(output.shapes, output.pixels_per_point).len();
    }
    if drawn == 0 {
        return Err("the dashboard drew nothing".into());
    }
    if !app.dashboard_palette_entries().iter().any(|e| e.id.starts_with("item:") && e.label == "Say hello") {
        return Err("the item is not in the command palette".into());
    }
    let has_menu = app
        .tray_menu()
        .iter()
        .any(|m| matches!(m, lum_platform::tray::MenuItem::Sub { text, .. } if text.contains("Dashboard")));
    if !has_menu {
        return Err("the tray menu has no Dashboard submenu".into());
    }
    let group_id = app
        .dashboard
        .tab(&tab)
        .and_then(|t| t.items().find(|i| i.kind() == "group").map(|i| i.id().to_owned()));
    let Some(group_id) = group_id else { return Err("the group is gone".into()) };
    app.run_dashboard_item(&group_id, None);
    app.hide_overlay();
    let output = lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
    let _ = output;
    app.dashboard.delete_tabs(&[tab]);
    Ok(())
}

/// The Snippets, Calculator, Timers, Notes and Reminders drawn without windows: their panels opened by their actions, their pages on
/// the overlay, and a sum in the palette.
fn everyday_tools(app: &mut App) -> Result<(), String> {
    for module in ["snippets", "calculator", "timer", "notes", "reminders", "qr"] {
        if !app.host.is_running(module) {
            return Err(format!("{module} is not running"));
        }
    }
    for action in
        ["snippets:open", "calculator:toggle", "timer:new_stopwatch", "notes:toggle", "qr:clipboard"]
    {
        let answer = app.run_action(action);
        if answer != "ok" {
            return Err(format!("{action}: {answer}"));
        }
    }
    let ctx = headless_context();
    let painted = |app: &mut App| -> usize {
        let mut shapes = 0;
        for _ in 0..3 {
            let output = lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
            shapes = ctx.tessellate(output.shapes, output.pixels_per_point).len();
        }
        shapes
    };
    if painted(app) == 0 {
        return Err("the panels drew nothing".into());
    }
    for page in ["clipboard", "calculator", "timers", "notes", "today", "stats", "calendar"] {
        app.open_overlay(page);
        if painted(app) == 0 {
            return Err(format!("the {page} page drew nothing"));
        }
        app.hide_overlay();
        painted(app);
    }
    let live = app.host.running_refs().flat_map(|m| m.palette_live("2+3*4")).collect::<Vec<_>>();
    if !live.iter().any(|e| e.label == "= 14") {
        return Err("a sum typed into the palette gets no answer".into());
    }
    if !app.host.running_refs().flat_map(|m| m.palette_entries()).any(|e| e.id.starts_with("note:")) {
        return Err("the palette lists no notes".into());
    }
    Ok(())
}

/// The colour picker, the ruler and text grab opened by their actions on stand-in screenshots (two screens, one
/// at 2x), drawn, and closed with Esc.
fn screen_tools(app: &mut App) -> Result<(), String> {
    use lum_ui::window::{self, Screen};
    for module in ["colorpicker", "textgrab", "ruler"] {
        if !app.host.is_running(module) {
            return Err(format!("{module} is not running"));
        }
    }
    let ctx = headless_context();
    window::install_capture(&ctx, std::sync::Arc::new(lum_platform::capture::Fake::default()));
    let screen = |name: &str, x: f32| {
        Screen::new(name, egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(640.0, 400.0)))
    };
    window::install_screens(&ctx, vec![screen("A", 0.0), screen("B", 640.0).with_scale(2.0)]);
    let drawn = |app: &mut App| {
        let output = lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
        ctx.tessellate(output.shapes, output.pixels_per_point).len()
    };
    // other tools' panels may still be fading out: count once things are still
    let settled = |app: &mut App| (0..5).map(|_| drawn(app)).last().unwrap_or(0);
    for action in ["colorpicker:pick", "ruler:measure", "textgrab:grab"] {
        let before = settled(app);
        let answer = app.run_action(action);
        if answer != "ok" {
            return Err(format!("{action}: {answer}"));
        }
        let mut shapes = 0;
        for _ in 0..40 {
            let output = lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
            shapes = ctx.tessellate(output.shapes, output.pixels_per_point).len();
            // a picture for each of the two screens at least
            if shapes >= before + 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if shapes < before + 2 {
            return Err(format!("{action} showed no frozen screens"));
        }
        lum_ui::headless_pass(&ctx, input(&[key(egui::Key::Escape, egui::Modifiers::NONE)]), |ui| {
            app.show_modules(ui.ctx())
        });
        if settled(app) > before {
            return Err(format!("Esc left {action}'s frozen screens behind"));
        }
    }
    Ok(())
}

/// The Settings page drawn section by section: every shown field gets a widget, and applying what changed does
/// not trouble the app.
fn settings_page(app: &mut App) -> Result<(), String> {
    use lum_core::settings::FieldType;
    app.open_settings("");
    let ctx = headless_context();
    let pass = |app: &mut App| {
        let output = lum_ui::headless_pass(&ctx, input(&[]), |ui| app.show_modules(ui.ctx()));
        ctx.tessellate(output.shapes, output.pixels_per_point).len()
    };
    pass(app);
    let sections: Vec<(String, usize)> = app
        .settings
        .schema()
        .sections
        .iter()
        .map(|s| (s.key.clone(), s.fields.iter().filter(|f| f.kind != FieldType::Hidden).count()))
        .collect();
    for (key, shown) in sections {
        app.settings_ui.open_section(&key);
        pass(app);
        if pass(app) == 0 {
            return Err(format!("the {key} section drew nothing"));
        }
        let drawn = app.settings_ui.drawn().len();
        if drawn != shown {
            return Err(format!("the {key} section drew {drawn} of its {shown} settings"));
        }
    }
    app.settings_saved(&["general".to_owned()]);
    pass(app);
    app.hide_overlay();
    pass(app);
    Ok(())
}
