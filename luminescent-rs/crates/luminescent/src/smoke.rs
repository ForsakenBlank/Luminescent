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
