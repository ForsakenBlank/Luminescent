//! The control socket: one command per connection, read until Hyprland hangs up. A request takes about a
//! millisecond, so there is no `hyprctl` process to spawn. These calls block for that long, so they belong on
//! service threads, never the UI thread.

use crate::lua::{self, BindOptions, Cover, Names};
use serde::Deserialize;
use serde_json::Value;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A running Hyprland and the names our Lua uses inside it.
#[derive(Debug, Clone)]
pub struct Hypr {
    dir: PathBuf,
    names: Names,
}

/// Said once per run, so a Hyprland that went away does not fill the log.
static WARNED: AtomicBool = AtomicBool::new(false);

impl Hypr {
    /// The Hyprland this session runs under (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE`).
    pub fn from_env() -> Option<Hypr> {
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|s| !s.is_empty())?;
        let runtime = runtime_dir()?;
        let hypr = Hypr { dir: runtime.join("hypr").join(signature), names: Names::app(&runtime) };
        hypr.available().then_some(hypr)
    }

    /// A Hyprland whose sockets are in `dir` (tests point this at a fake one).
    pub fn at(dir: impl Into<PathBuf>, names: Names) -> Hypr {
        Hypr { dir: dir.into(), names }
    }

    pub fn names(&self) -> &Names {
        &self.names
    }

    pub fn socket_path(&self) -> PathBuf {
        self.dir.join(".socket.sock")
    }

    pub fn event_socket_path(&self) -> PathBuf {
        self.dir.join(".socket2.sock")
    }

    pub fn available(&self) -> bool {
        self.socket_path().exists()
    }

    /// Sends one command (`j/clients`, `eval <lua>`, `repl <lua>`) and gives back the whole reply.
    pub fn request(&self, command: &str) -> io::Result<String> {
        let reply = request_at(&self.socket_path(), command);
        if let Err(e) = &reply
            && !WARNED.swap(true, Ordering::Relaxed)
        {
            tracing::warn!("Hyprland socket not reachable: {e}");
        }
        reply
    }

    /// Runs Lua statements inside Hyprland. `Ok` when Hyprland answered "ok", else its error text.
    pub fn lua(&self, code: &str) -> Result<(), String> {
        let reply = self.request(&format!("eval {code}")).map_err(|e| e.to_string())?;
        match reply.trim() {
            "ok" => Ok(()),
            "" => Err("no reply from Hyprland".to_owned()),
            other => {
                let first = other.lines().next().unwrap_or(other).to_owned();
                tracing::warn!("hyprland lua: {first}");
                Err(first)
            }
        }
    }

    /// Evaluates a Lua expression inside Hyprland and gives it back as text.
    pub fn value(&self, expr: &str) -> Option<String> {
        self.request(&format!("repl return {expr}")).ok().map(|r| r.trim().to_owned())
    }

    /// A JSON query (`clients`, `monitors`, `binds`, `cursorpos`, `activewindow`).
    pub fn query(&self, name: &str) -> Option<Value> {
        let reply = self.request(&format!("j/{name}")).ok()?;
        serde_json::from_str(&reply).ok()
    }

    fn query_as<T: for<'de> Deserialize<'de>>(&self, name: &str) -> Option<T> {
        serde_json::from_value(self.query(name)?).ok()
    }

    /// (Re)installs our table and window rules. Must run before the first window is shown.
    pub fn bootstrap(&self) -> Result<(), String> {
        self.lua(&lua::bootstrap(&self.names))
    }

    /// Binds `keys` so that pressing it writes `id` into our FIFO.
    pub fn bind(&self, id: &str, keys: &str, opts: BindOptions) -> Result<(), String> {
        let code = lua::bind(&self.names, id, keys, opts);
        match self.lua(&code) {
            // a second config reload wiped the table while the binds were going back
            Err(e) if e.contains(&self.names.table) && self.bootstrap().is_ok() => self.lua(&code),
            other => other,
        }
    }

    pub fn unbind(&self, id: &str) -> Result<(), String> {
        self.lua(&lua::unbind(&self.names, id))
    }

    pub fn unbind_all(&self) -> Result<(), String> {
        self.lua(&lua::unbind_all(&self.names))
    }

    /// Quiets our binds on the way out; the slots stay for the next start.
    pub fn shutdown(&self) {
        if self.available() {
            let _ = self.lua(&lua::shutdown(&self.names));
        }
    }

    pub fn move_window(&self, addr: &str, x: i32, y: i32, pinned: bool) -> Result<(), String> {
        self.lua(&lua::move_window(addr, x, y, pinned))
    }

    pub fn clients(&self) -> Vec<Client> {
        self.query_as("clients").unwrap_or_default()
    }

    pub fn client(&self, addr: &str) -> Option<Client> {
        self.clients().into_iter().find(|c| c.address == addr)
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        self.query_as("monitors").unwrap_or_default()
    }

    /// The pointer in layout coordinates, wherever it is.
    pub fn cursor_pos(&self) -> Option<(i32, i32)> {
        #[derive(Deserialize)]
        struct At {
            x: f64,
            y: f64,
        }
        let at: At = self.query_as("cursorpos")?;
        Some((at.x as i32, at.y as i32))
    }

    pub fn move_cursor(&self, x: i32, y: i32) -> Result<(), String> {
        self.lua(&lua::move_cursor(x, y))
    }

    /// True if any of the xkb keysyms (`Alt_L`, `a`) is held. None if Hyprland can not say.
    pub fn key_down(&self, keysyms: &[&str]) -> Option<bool> {
        if keysyms.is_empty() {
            return None;
        }
        match self.value(&lua::key_down(keysyms))?.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Sends a key combination to the focused window (`CTRL`, `V`).
    pub fn send_shortcut(&self, mods: &str, key: &str) -> Result<(), String> {
        self.lua(&lua::send_shortcut(mods, key))
    }

    /// Hyprland binds that are not ours (the user's own config).
    pub fn foreign_binds(&self) -> Vec<Value> {
        let Some(Value::Array(binds)) = self.query("binds") else {
            return Vec::new();
        };
        binds
            .into_iter()
            .filter(|b| b.get("description").and_then(Value::as_str) != Some(lua::BIND_DESC))
            .collect()
    }

    pub fn glass_rule(&self, rounding: i32) -> Result<(), String> {
        self.lua(&lua::glass_rule(&self.names, rounding))
    }

    /// Replaces the cover rules with these.
    pub fn cover_monitors(&self, covers: &[Cover]) -> Result<(), String> {
        self.lua(&lua::clear_cover_rules(&self.names))?;
        self.lua(&lua::cover_rules(&self.names, covers))
    }

    pub fn root_rule(&self, title: &str) -> Result<(), String> {
        self.lua(&lua::root_rule(&self.names, title))
    }
}

fn request_at(path: &Path, command: &str) -> io::Result<String> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(command.as_bytes())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply)
}

/// `$XDG_RUNTIME_DIR`, or `/run/user/<uid>` when it is not set.
pub fn runtime_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    // SAFETY: getuid has no preconditions and can not fail.
    let uid = unsafe { libc::getuid() };
    Some(PathBuf::from(format!("/run/user/{uid}")))
}

/// One window as `j/clients` describes it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Client {
    pub address: String,
    pub class: String,
    pub title: String,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub pid: i64,
    pub floating: bool,
    pub pinned: bool,
    pub monitor: i64,
    pub focus_history_id: i64,
}

/// One monitor as `j/monitors` describes it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Monitor {
    pub id: i64,
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// The mode in pixels; the layout uses it divided by the scale.
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub transform: u32,
    pub focused: bool,
    /// What bars and docks keep for themselves: left, top, right, bottom.
    pub reserved: Vec<i32>,
}

impl Monitor {
    /// The size in layout coordinates.
    pub fn logical_size(&self) -> (u32, u32) {
        let scale = self.scale.max(0.1);
        let (w, h) = (f64::from(self.width) / scale, f64::from(self.height) / scale);
        // a turned monitor swaps its sides
        let (w, h) = if self.transform % 2 == 1 { (h, w) } else { (w, h) };
        (w.round() as u32, h.round() as u32)
    }

    /// Left, top, right and bottom kept by bars and docks.
    pub fn reserved(&self) -> [i32; 4] {
        match self.reserved[..] {
            [l, t, r, b] => [l, t, r, b],
            _ => [0; 4],
        }
    }
}
