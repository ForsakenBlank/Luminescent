//! `--spike hotkeys`: the phase 3 hotkey checks on a real desktop. Ctrl+Shift+Space is a hotkey and F8 a hold
//! key; every event is printed with how long it took to reach us. Quit Luminescent (both versions) first:
//! this uses the same binds.

use lum_platform::desktop::{Desktop, DesktopEvent};
use lum_platform::hotkeys::{HotkeyEvent, HotkeyManager, Options};
use std::sync::Arc;
use std::time::{Duration, Instant};

enum Wake {
    Hotkeys,
    Desktop(DesktopEvent),
}

pub fn run(secs: u64) -> Result<(), String> {
    println!("SPIKE hotkeys: {}", std::env::consts::OS);
    println!(
        "  Press Ctrl+Shift+Space a few times. Hold F8, then while holding it lock the screen (Win+L on"
    );
    println!("  Windows, your lock key on Hyprland) or switch to another window, unlock and let go: F8 must");
    println!("  report up within a moment even though its release was never seen. Ends after {secs} s.");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let desktop = Desktop::start(&[]);
    println!("  desktop: {}", desktop.describe());
    let (tx, rx) = crossbeam_channel::unbounded();
    let to_us = tx.clone();
    desktop.listen(runtime.handle(), move |e| {
        let _ = to_us.send(Wake::Desktop(e));
    });
    let mut keys = HotkeyManager::start(&desktop, Arc::new(move || drop(tx.send(Wake::Hotkeys))));
    keys.register("spike", "ctrl+shift+space", "press", "the test hotkey", Options::default())?;
    keys.register_hold("spike", "f8", true)?;
    let started = Instant::now();
    let end = started + Duration::from_secs(secs);
    let mut fired = 0;
    while let Ok(wake) = rx.recv_deadline(end) {
        if let Wake::Desktop(e) = &wake {
            keys.desktop_event(e);
        }
        for event in keys.poll() {
            let at = started.elapsed().as_secs_f64();
            match event {
                HotkeyEvent::Fired { combo, .. } => {
                    fired += 1;
                    println!("  {at:7.2} s  {combo} fired");
                }
                HotkeyEvent::HoldDown { key, .. } => println!("  {at:7.2} s  {key} down"),
                HotkeyEvent::HoldUp { key, .. } => println!("  {at:7.2} s  {key} up"),
                HotkeyEvent::Interrupted { key, .. } => {
                    println!("  {at:7.2} s  {key} pressed while F8 is held")
                }
                HotkeyEvent::Refused { combo, reason, .. } => {
                    println!("  {at:7.2} s  {combo} refused: {reason}")
                }
                HotkeyEvent::Released { combo } => println!("  {at:7.2} s  {combo} let go"),
                HotkeyEvent::ReleaseUnknown { combo } => println!("  {at:7.2} s  {combo}: can not tell"),
            }
        }
    }
    let p99 = keys.latency_p99_ms().map_or("no hotkeys fired".to_owned(), |ms| format!("{ms:.2} ms"));
    println!("  {fired} hotkeys, latency p99 {p99} (budget 5 ms)");
    println!(
        "| hotkeys | {} | {fired} fired | p99 {p99} | (did F8 come up after the lock?) |",
        desktop.describe()
    );
    keys.shutdown();
    desktop.shutdown();
    Ok(())
}
