//! A stand-in Hyprland for tests: a control socket that records every command and answers from a few
//! settable replies, and an event socket that sends whatever lines the test pushes.

use crate::lua::Names;
use crate::socket::Hypr;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Answer = Box<dyn Fn(&str) -> Option<String> + Send>;

#[derive(Default)]
struct State {
    sent: Vec<String>,
    clients: String,
    monitors: String,
    answer: Option<Answer>,
    listeners: Vec<UnixStream>,
}

pub struct FakeHyprland {
    dir: tempfile::TempDir,
    state: Arc<Mutex<State>>,
}

impl FakeHyprland {
    pub fn start() -> FakeHyprland {
        let dir = tempfile::tempdir().expect("a temp folder");
        let state =
            Arc::new(Mutex::new(State { clients: "[]".into(), monitors: "[]".into(), ..State::default() }));
        let control =
            UnixListener::bind(dir.path().join(".socket.sock")).expect("bind the fake control socket");
        let events =
            UnixListener::bind(dir.path().join(".socket2.sock")).expect("bind the fake event socket");
        let s = state.clone();
        std::thread::spawn(move || {
            for conn in control.incoming() {
                let Ok(mut conn) = conn else { return };
                let mut buf = vec![0u8; 1 << 16];
                let n = conn.read(&mut buf).unwrap_or(0);
                let command = String::from_utf8_lossy(&buf[..n]).into_owned();
                let reply = {
                    let mut st = s.lock().expect("the fake's lock");
                    st.sent.push(command.clone());
                    reply(&st, &command)
                };
                let _ = conn.write_all(reply.as_bytes());
            }
        });
        let s = state.clone();
        std::thread::spawn(move || {
            for conn in events.incoming() {
                let Ok(conn) = conn else { return };
                s.lock().expect("the fake's lock").listeners.push(conn);
            }
        });
        FakeHyprland { dir, state }
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// A client for this fake, with the Rust app's names and a FIFO inside the fake's folder.
    pub fn hypr(&self) -> Hypr {
        Hypr::at(self.dir.path(), Names::app(self.dir.path()))
    }

    pub fn fifo(&self) -> PathBuf {
        Names::app(self.dir.path()).fifo
    }

    pub fn set_clients(&self, json: &str) {
        self.lock().clients = json.to_owned();
    }

    pub fn set_monitors(&self, json: &str) {
        self.lock().monitors = json.to_owned();
    }

    /// Answers commands the default replies do not cover (return None to fall back to them).
    pub fn answer(&self, f: impl Fn(&str) -> Option<String> + Send + 'static) {
        self.lock().answer = Some(Box::new(f));
    }

    /// Every command so far, oldest first.
    pub fn sent(&self) -> Vec<String> {
        self.lock().sent.clone()
    }

    pub fn clear(&self) {
        self.lock().sent.clear();
    }

    /// Waits up to 3 s for a command that `pred` accepts.
    pub fn wait_for(&self, pred: impl Fn(&str) -> bool) -> Option<String> {
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if let Some(c) = self.sent().into_iter().find(|c| pred(c)) {
                return Some(c);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    /// Waits until something has connected to the event socket.
    pub fn wait_for_listener(&self) -> bool {
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if !self.lock().listeners.is_empty() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Sends an event line (`openwindow>>...`) to everyone listening.
    pub fn event(&self, line: &str) {
        let mut st = self.lock();
        st.listeners.retain_mut(|c| c.write_all(format!("{line}\n").as_bytes()).is_ok());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("the fake's lock")
    }
}

fn reply(st: &State, command: &str) -> String {
    if let Some(answer) = st.answer.as_ref().and_then(|f| f(command)) {
        return answer;
    }
    match command {
        "j/clients" => st.clients.clone(),
        "j/monitors" => st.monitors.clone(),
        c if c.starts_with("j/") => "[]".into(),
        c if c.starts_with("eval ") => "ok".into(),
        _ => String::new(),
    }
}
