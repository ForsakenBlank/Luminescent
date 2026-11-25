//! Sounds for timers and reminders. `kind` is one of the Settings choices: `none`, `chime`, `alarm` or
//! `custom` (a file: wav, ogg, flac or mp3).
//!
//! Linux: chime and alarm are the freedesktop "complete" and "alarm-clock-elapsed" sounds. Windows: the
//! "Asterisk" and "Exclamation" system sounds. Each sound plays on a thread of its own that ends with it, so
//! nothing stays open between sounds.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SOUND_DIRS: [&str; 2] = ["/usr/share/sounds/freedesktop/stereo", "/usr/share/sounds/ocean/stereo"];
const SYSTEM: [(&str, [&str; 3]); 2] = [
    ("chime", ["complete", "message", "bell"]),
    ("alarm", ["alarm-clock-elapsed", "dialog-warning", "bell"]),
];

static PLAYING: Mutex<Vec<Arc<rodio::Player>>> = Mutex::new(Vec::new());

/// The freedesktop sound for `chime` or `alarm`, if the system has one.
pub fn system_file(kind: &str) -> Option<PathBuf> {
    let (_, names) = SYSTEM.iter().find(|(k, _)| *k == kind)?;
    names.iter().find_map(|name| {
        SOUND_DIRS.iter().find_map(|dir| {
            ["oga", "ogg", "wav"]
                .iter()
                .map(|ext| Path::new(dir).join(format!("{name}.{ext}")))
                .find(|p| p.is_file())
        })
    })
}

pub fn play(kind: &str, file: &str, volume: f32) {
    let kind = if kind.is_empty() { "none".to_owned() } else { kind.to_lowercase() };
    if kind == "none" {
        return;
    }
    let file = file.to_owned();
    let volume = volume.clamp(0.0, 1.0);
    let spawned = std::thread::Builder::new().name("sound".into()).spawn(move || {
        if let Err(e) = play_now(&kind, &file, volume) {
            tracing::warn!("could not play the {kind} sound: {e}");
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("could not play a sound: {e}");
    }
}

fn play_now(kind: &str, file: &str, volume: f32) -> Result<(), String> {
    if kind == "custom" {
        if !file.is_empty() && Path::new(file).is_file() {
            return play_file(Path::new(file), volume);
        }
        tracing::warn!("custom sound file missing: {file:?}, using chime");
        return play_now("chime", "", volume);
    }
    #[cfg(windows)]
    if let Some(alias) = match kind {
        "chime" => Some("SystemAsterisk"),
        "alarm" => Some("SystemExclamation"),
        _ => None,
    } {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
        let wide: Vec<u16> = alias.encode_utf16().chain([0]).collect();
        // SAFETY: a nul-terminated alias name that lives for the call; SND_ASYNC copies what it needs.
        unsafe { PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), SND_ALIAS | SND_ASYNC) };
        return Ok(());
    }
    match system_file(kind) {
        Some(path) => play_file(&path, volume),
        None => Err(format!("no system sound for {kind}")),
    }
}

fn play_file(path: &Path, volume: f32) -> Result<(), String> {
    let mut device = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
    device.log_on_drop(false);
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let source = rodio::Decoder::try_from(file).map_err(|e| e.to_string())?;
    let player = Arc::new(rodio::Player::connect_new(device.mixer()));
    player.set_volume(volume);
    player.append(source);
    PLAYING.lock().map_err(|e| e.to_string())?.push(player.clone());
    player.sleep_until_end();
    if let Ok(mut playing) = PLAYING.lock() {
        playing.retain(|p| !Arc::ptr_eq(p, &player));
    }
    Ok(())
}

/// Stops every sound still playing.
pub fn stop() {
    if let Ok(playing) = PLAYING.lock() {
        for p in playing.iter() {
            p.stop();
        }
    }
}
