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
    ("hotkeys, tray menu and the overlay", hello_tray),
    ("themes switch and restyle", themes),
    ("themes window, editor and colour dialog", themes_window),
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
    if answer != "ok" || !app.themes_ui.is_open() {
        return Err(format!("ctl overlay themes did not open the window ({answer})"));
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
    let (answer, _) = app.handle_ctl("overlay");
    if answer != "ok" || !app.overlay_open {
        return Err(format!("ctl overlay answered {answer:?}"));
    }
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(true);
    lum_ui::setup(&ctx, lum_ui::Tokens::dark());
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let input = |esc: bool| egui::RawInput {
        screen_rect: Some(screen),
        events: if esc {
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        } else {
            Vec::new()
        },
        ..Default::default()
    };
    lum_ui::headless_pass(&ctx, input(false), |ui| app.show_modules(ui.ctx()));
    if !app.overlay_open {
        return Err("the overlay closed by itself".into());
    }
    lum_ui::headless_pass(&ctx, input(true), |ui| app.show_modules(ui.ctx()));
    if app.overlay_open {
        return Err("Esc did not close the overlay".into());
    }
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
