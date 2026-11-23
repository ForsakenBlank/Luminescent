//! The control socket: one command per connection, read until Hyprland hangs up. A request takes about a
//! millisecond, so there is no `hyprctl` process to spawn.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`, when Hyprland is running.
pub fn socket_path() -> Option<PathBuf> {
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").filter(|s| !s.is_empty())?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|s| !s.is_empty())?;
    Some(PathBuf::from(runtime).join("hypr").join(signature).join(".socket.sock"))
}

pub fn available() -> bool {
    socket_path().is_some_and(|p| p.exists())
}

/// Sends one command (`j/clients`, `eval <lua>`, `repl <lua>`) and gives back the whole reply.
pub fn request(command: &str) -> io::Result<String> {
    let path =
        socket_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Hyprland is not running"))?;
    request_at(&path, command)
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

/// A JSON query such as `clients` or `monitors`.
pub fn query(name: &str) -> Option<serde_json::Value> {
    let reply = request(&format!("j/{name}")).ok()?;
    serde_json::from_str(&reply).ok()
}

/// Runs Lua statements inside Hyprland. `Ok` when Hyprland answered "ok", else its error text.
pub fn lua(code: &str) -> Result<(), String> {
    let reply = request(&format!("eval {code}")).map_err(|e| e.to_string())?;
    match reply.trim() {
        "ok" => Ok(()),
        "" => Err("no reply from Hyprland".to_owned()),
        other => Err(other.lines().next().unwrap_or(other).to_owned()),
    }
}

/// A Lua string literal.
pub fn lstr(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn lua_strings_are_escaped() {
        assert_eq!(lstr(r#"a "b" \ c"#), r#""a \"b\" \\ c""#);
        assert_eq!(lstr("two\nlines"), "\"two\\nlines\"");
    }

    #[test]
    fn a_request_reads_until_hyprland_hangs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".socket.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 64];
            let n = conn.read(&mut buf).unwrap();
            assert_eq!(&buf[..n], b"j/monitors");
            conn.write_all(b"[{\"name\": \"DP-1\"}]").unwrap();
        });
        let reply = request_at(&path, "j/monitors").unwrap();
        server.join().unwrap();
        assert_eq!(reply, r#"[{"name": "DP-1"}]"#);
    }
}
