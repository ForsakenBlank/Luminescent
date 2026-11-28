//! Actions: nodes that do something when a pulse arrives.
//!
//! What needs the app (cards, dialogs, the overlay, keys) goes out as an [`Out`]; what waits on the system or the
//! network (commands, web requests, downloads, file copies) runs on a thread of its own through [`Cx::spawn`].

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::engine::{Answer, Cx, NodeRuntime, Out};
use crate::spec::Choices;
use crate::text::{self, MAX_TEXT, clip_chars, expand_user, last_chars};
use crate::work::{Job, JobHandle, JobMsg};

/// The inputs of requests still waiting for the app's answer, by token.
#[derive(Default)]
struct Waiting {
    next: u64,
    inputs: HashMap<u64, String>,
}

impl Waiting {
    fn push(&mut self, input: &str) -> u64 {
        self.next += 1;
        self.inputs.insert(self.next, input.to_owned());
        self.next
    }

    fn take(&mut self, token: u64) -> Option<String> {
        self.inputs.remove(&token)
    }
}

/// Set variable: keeps a value for later (also after a restart). Done carries the new value.
pub struct VarSet;

impl NodeRuntime for VarSet {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let name = cx.raw_str("name").trim().to_owned();
        if name.is_empty() {
            cx.set_status("⚠ give it a name", None);
            return;
        }
        let value = cx.f("value", input);
        let old = cx.get_var(&name);
        let value = match cx.raw_str("mode").as_str() {
            "append" => old + &value,
            "add" => {
                let as_number = |s: &str| if s.is_empty() { Some(0.0) } else { text::py_float(s) };
                match (as_number(&old), as_number(&value)) {
                    (Some(a), Some(b)) => text::fmt_number(a + b),
                    _ => {
                        cx.set_status("⚠ not a number", None);
                        return;
                    }
                }
            }
            "clear" => String::new(),
            _ => value,
        };
        cx.set_var(&name, &value);
        cx.set_status(format!("{name} = {}", clip_chars(&value, 50)), None);
        cx.fire("done", &value);
    }
}

/// Show: shows what comes through (handy to see what a wire carries) and passes it on.
pub struct Show;

impl NodeRuntime for Show {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let line = input.replace('\n', " ⏎ ");
        let shown = clip_chars(&line, 120);
        cx.set_status(if shown.is_empty() { "(empty)" } else { shown }, None);
        tracing::info!("show [{}]: {}", cx.label(), clip_chars(input, 500));
        cx.fire("out", input);
    }
}

/// Run command: the commands one after another, in the background or in a terminal you can watch.
#[derive(Default)]
pub struct Command {
    running: Option<JobHandle>,
    terminal: Option<InTerminal>,
    queue: VecDeque<String>,
    output: String,
}

struct InTerminal {
    marker: PathBuf,
    script: PathBuf,
    started: f64,
}

const RUN: u64 = 1;
const POLL: u64 = 2;

impl Command {
    fn run(&mut self, input: &str, cx: &mut Cx<'_>) {
        let commands = cx.f("commands", input);
        let lines: Vec<String> = text::split_lines(&commands)
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect();
        if lines.is_empty() {
            cx.set_status("⚠ no commands", None);
            return;
        }
        let folder = cx.raw_str("folder").trim().to_owned();
        let mut cwd = expand_user(if folder.is_empty() { "~" } else { &folder });
        if !cwd.is_dir() {
            cwd = expand_user("~");
        }
        let stop_on_error = cx.flag("stop_on_error");
        let shell = cx.config().shell.clone();
        if cx.flag("terminal") {
            self.in_terminal(&lines, &cwd, &shell, stop_on_error, cx);
            return;
        }
        let timeout = cx.config().command_timeout;
        let job = cx.spawn(RUN, move |job| run_lines(job, &lines, &cwd, &shell, stop_on_error, timeout));
        self.running = Some(job);
    }

    /// A script in a terminal window you can watch; a marker file says when it ended and how.
    fn in_terminal(&mut self, lines: &[String], cwd: &Path, shell: &str, stop: bool, cx: &mut Cx<'_>) {
        let marker = std::env::temp_dir().join(format!("lum-auto-{}.code", crate::doc::new_id()));
        match cx.host().clone().run_in_terminal(cwd, lines, shell, stop, &marker) {
            Err(e) => {
                cx.set_status(format!("⚠ {e}"), None);
                cx.fire("failed", &e);
            }
            Ok(script) => {
                cx.set_status("running in a terminal", None);
                self.terminal = Some(InTerminal { marker, script, started: cx.mono() });
                cx.after(0.5, POLL);
            }
        }
    }

    fn next_in_queue(&mut self, cx: &mut Cx<'_>) {
        if let Some(input) = self.queue.pop_front() {
            self.run(&input, cx);
        }
    }

    fn finished(&mut self, failed: bool, code: &str, output: String, cx: &mut Cx<'_>) {
        self.output = output;
        cx.set_status(if failed { format!("failed (exit code {code})") } else { "done".to_owned() }, None);
        let output = self.output.clone();
        cx.fire(if failed { "failed" } else { "done" }, &output);
        self.next_in_queue(cx);
    }
}

impl NodeRuntime for Command {
    fn receive(&mut self, port: &str, input: &str, cx: &mut Cx<'_>) {
        if port == "stop" {
            self.queue.clear();
            if let Some(job) = &self.running {
                job.stop();
            }
            return;
        }
        if self.running.is_some() || self.terminal.is_some() {
            // one run at a time; the next waits its turn
            if self.queue.len() < 20 {
                self.queue.push_back(input.to_owned());
            }
            return;
        }
        self.run(input, cx);
    }

    fn job(&mut self, _token: u64, msg: JobMsg, cx: &mut Cx<'_>) {
        match msg {
            JobMsg::Line(line) => cx.fire("line", &line),
            JobMsg::Status(text) => cx.set_status(text, None),
            JobMsg::Done { ok, text, code } => {
                self.running = None;
                self.finished(!ok, &code, text, cx);
            }
        }
    }

    fn timer(&mut self, _token: u64, cx: &mut Cx<'_>) {
        let Some(t) = &self.terminal else { return };
        if t.marker.exists() {
            let code: i32 = std::fs::read_to_string(&t.marker)
                .ok()
                .and_then(|s| s.split_whitespace().next().map(str::to_owned))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let _ = std::fs::remove_file(&t.marker);
            let _ = std::fs::remove_file(&t.script);
            self.terminal = None;
            self.finished(code != 0, &code.to_string(), code.to_string(), cx);
        } else if cx.mono() - t.started > 86_400.0 {
            self.terminal = None;
        } else {
            cx.after(0.5, POLL);
        }
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        self.output.clone()
    }

    fn stop(&mut self, _cx: &mut Cx<'_>) {
        self.queue.clear();
        if let Some(job) = &self.running {
            job.stop();
        }
    }
}

/// Runs the lines one after another on a worker thread and says how it went.
fn run_lines(job: &Job, lines: &[String], cwd: &Path, shell: &str, stop_on_error: bool, timeout: u64) {
    let mut out = String::new();
    let (mut failed, mut code) = (false, 0);
    for line in lines {
        if job.stopped() {
            break;
        }
        job.send(JobMsg::Status(format!("running: {}", clip_chars(line, 40))));
        let ended = run_one(job, line, cwd, shell, timeout, &mut out);
        if ended != 0 {
            (failed, code) = (true, ended);
            if stop_on_error {
                break;
            }
        }
    }
    let text = last_chars(out.trim(), MAX_TEXT).to_owned();
    job.send(JobMsg::Done { ok: !failed, text, code: code.to_string() });
}

/// One line, with what it prints (both streams together) added to `out`. Gives back its exit code.
fn run_one(job: &Job, line: &str, cwd: &Path, shell: &str, timeout: u64, out: &mut String) -> i32 {
    let mut cmd = job.host().shell_command(shell, line);
    cmd.current_dir(cwd).stdin(Stdio::null());
    let pipe = std::io::pipe().and_then(|(r, w)| Ok((r, w.try_clone()?, w)));
    let (reader, stdout, stderr) = match pipe {
        Ok(ends) => ends,
        Err(e) => {
            out.push_str(&format!("could not start: {e}\n"));
            return 127;
        }
    };
    cmd.stdout(stdout).stderr(stderr);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            out.push_str(&format!("could not start: {e}\n"));
            return 127;
        }
    };
    // our copies of the pipe's writing end must go, or the reader would never see the end
    drop(cmd);
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    let lines = job.clone();
    std::thread::spawn(move || {
        let _ = done_tx.send(read_output(reader, &lines));
    });
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if job.stopped() {
            let _ = child.kill();
        } else if timeout > 0 && !timed_out && started.elapsed() >= Duration::from_secs(timeout) {
            timed_out = true;
            out.push_str("\n[stopped: took too long]\n");
            let _ = child.kill();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // Something the command left running in the background may keep the output open: do not wait on it.
    if let Ok(text) = done_rx.recv_timeout(Duration::from_secs(1)) {
        out.push_str(&text);
    }
    // Python's QProcess ends a killed command "normally" with code 0, so a stop counts as done.
    status.and_then(|s| s.code()).unwrap_or(0)
}

fn read_output(reader: impl Read, job: &Job) -> String {
    let mut reader = BufReader::new(reader);
    let mut all = String::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let chunk = String::from_utf8_lossy(&buf);
        let line = chunk.trim_end_matches(['\n', '\r']);
        if !line.trim().is_empty() {
            job.send(JobMsg::Line(line.to_owned()));
        }
        if all.len() < MAX_TEXT * 4 {
            all.push_str(&chunk);
        }
    }
    all
}

/// Open a program, file or URL.
#[derive(Default)]
pub struct Open;

impl NodeRuntime for Open {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let target = cx.f("target", input).trim().to_owned();
        if target.is_empty() {
            cx.set_status("⚠ nothing to open", None);
            return;
        }
        let args = cx.f("args", input);
        cx.send(Out::Open { target, args, name: cx.label(), reply: cx.reply(1) });
    }

    fn answer(&mut self, _token: u64, answer: Answer, cx: &mut Cx<'_>) {
        match answer {
            Answer::Done(target) => {
                cx.set_status("", None);
                cx.fire("done", &target);
            }
            Answer::Failed(e) => cx.set_status(format!("⚠ {e}"), None),
            Answer::Cancelled => {}
        }
    }
}

/// Close a program: every running copy of it.
pub struct CloseProgram;

impl NodeRuntime for CloseProgram {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let name = cx.f("name", input).trim().to_owned();
        if name.is_empty() {
            return;
        }
        let force = cx.flag("force");
        cx.spawn(1, move |job| {
            let ok = job.host().close_program(&name, force);
            job.send(JobMsg::Done { ok, text: name, code: String::new() });
        });
    }

    fn job(&mut self, _token: u64, msg: JobMsg, cx: &mut Cx<'_>) {
        if let JobMsg::Done { ok, text, .. } = msg {
            cx.fire(if ok { "done" } else { "not_running" }, &text);
        }
    }
}

/// Luminescent tool: anything from the Dashboard's Tools tab.
pub struct LumAction;

impl NodeRuntime for LumAction {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        cx.send(Out::RunAction(cx.raw_str("action")));
        cx.fire("done", input);
    }
}

/// Open / close Luminescent.
pub struct Overlay;

impl NodeRuntime for Overlay {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let page = match cx.raw_str("page") {
            p if p.is_empty() => "dashboard".to_owned(),
            p => p,
        };
        cx.send(Out::Overlay { mode: cx.raw_str("mode"), page });
        cx.fire("done", input);
    }
}

/// Run a dashboard item, Switch dashboard and Switch theme: ask the app, then pass the pulse on.
pub struct AskApp(pub fn(String) -> Out, pub &'static str);

impl NodeRuntime for AskApp {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        cx.send((self.0)(cx.raw_str(self.1)));
        cx.fire("done", input);
    }
}

/// Start a timer: a Timer panel, already running.
#[derive(Default)]
pub struct Timer {
    waiting: Waiting,
}

impl NodeRuntime for Timer {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let typed = cx.f("time", input);
        let Some((ms, label)) = lum_calc::timer::parse_entry(&typed, cx.local()) else {
            cx.set_status("⚠ could not read that time", None);
            return;
        };
        let shown = if label.is_empty() { typed } else { label.clone() };
        let token = self.waiting.push(&shown);
        cx.send(Out::StartTimer { ms, label, reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        let Some(shown) = self.waiting.take(token) else { return };
        match answer {
            Answer::Done(_) => cx.fire("started", &shown),
            Answer::Failed(e) => cx.set_status(format!("⚠ {e}"), None),
            Answer::Cancelled => {}
        }
    }
}

/// Workspace on / off: switches a whole workspace on or off (this one too).
pub struct Workspace;

impl NodeRuntime for Workspace {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let wid = match cx.raw_str("workspace") {
            w if w.is_empty() => cx.workspace_id().to_owned(),
            w => w,
        };
        let Some(ws) = cx.workspace(&wid) else { return };
        let enabled = text::truthy(&ws["enabled"]);
        let on = match cx.raw_str("mode").as_str() {
            "toggle" => !enabled,
            mode => mode == "on",
        };
        cx.fire("done", input);
        cx.set_enabled(&wid, Some(on));
    }
}

/// Show a notification: a card, a desktop notification or a warning in the middle of the screen.
pub struct Notify;

impl NodeRuntime for Notify {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let title = cx.f("title", input);
        let text = cx.f("message", input);
        let out = match cx.raw_str("style").as_str() {
            "center" => Out::Alert {
                title,
                text,
                level: "info".into(),
                seconds: 0.0,
                sound: "none".into(),
                reply: None,
            },
            "desktop" => Out::Toast { title, text, ms: 8000 },
            _ => Out::Card { title, text, ms: 6000 },
        };
        cx.send(out);
        cx.fire("done", input);
    }
}

/// Big message: a message box on top of everything; ok fires when it is closed.
#[derive(Default)]
pub struct Message {
    waiting: Waiting,
}

impl NodeRuntime for Message {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let token = self.waiting.push(input);
        let (title, text) = (cx.f("title", input), cx.f("text", input));
        cx.send(Out::Message { title, text, reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, _answer: Answer, cx: &mut Cx<'_>) {
        if let Some(input) = self.waiting.take(token) {
            cx.fire("ok", &input);
        }
    }
}

/// Ask me: a box to type in, then carries on with what you typed.
#[derive(Default)]
pub struct Ask {
    waiting: Waiting,
    answer: String,
}

impl NodeRuntime for Ask {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let token = self.waiting.push(input);
        let question = cx.f("question", input);
        let default = cx.f("default", input);
        cx.send(Out::Ask { title: cx.label(), question, default, reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        let Some(input) = self.waiting.take(token) else { return };
        match answer {
            Answer::Done(text) => {
                self.answer = text;
                let var = cx.raw_str("var");
                if !var.trim().is_empty() {
                    cx.set_var(&var, &self.answer);
                }
                let answer = self.answer.clone();
                cx.fire("answer", &answer);
            }
            Answer::Cancelled | Answer::Failed(_) => cx.fire("cancelled", &input),
        }
    }

    fn value(&self, _port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        self.answer.clone()
    }
}

/// Play a sound.
pub struct Sound;

impl NodeRuntime for Sound {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let file = expand_user(&cx.raw_str("file")).to_string_lossy().into_owned();
        cx.send(Out::Sound { kind: cx.raw_str("sound"), file });
        cx.fire("done", input);
    }
}

/// Say it out loud.
#[derive(Default)]
pub struct Speak {
    waiting: Waiting,
}

impl NodeRuntime for Speak {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let text = cx.f("text", input).trim().to_owned();
        if text.is_empty() {
            cx.fire("done", input);
            return;
        }
        let token = self.waiting.push(input);
        cx.send(Out::Speak { text, reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        let Some(input) = self.waiting.take(token) else { return };
        match answer {
            Answer::Failed(e) => cx.set_status(format!("⚠ {e}"), None),
            _ => cx.fire("done", &input),
        }
    }
}

/// Type text: types into the focused window after a short wait.
#[derive(Default)]
pub struct TypeText {
    next: u64,
    waiting: HashMap<u64, (String, String)>,
}

impl NodeRuntime for TypeText {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let text = cx.f("text", input);
        let wait = cx.num("delay_ms", "", 300.0).max(0.0).trunc() / 1000.0;
        self.next += 1;
        self.waiting.insert(self.next, (text, input.to_owned()));
        cx.after(wait, self.next);
    }

    fn timer(&mut self, token: u64, cx: &mut Cx<'_>) {
        if let Some((text, input)) = self.waiting.remove(&token) {
            cx.send(Out::TypeText(text));
            cx.fire("done", &input);
        }
    }
}

/// Press keys: a key combination in the focused window.
pub struct Keys;

impl NodeRuntime for Keys {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let combo = cx.raw_str("combo");
        if !combo.is_empty() {
            cx.send(Out::PressKeys(combo));
        }
        cx.fire("done", input);
    }
}

/// Copy to clipboard.
pub struct SetClipboard;

impl NodeRuntime for SetClipboard {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let text = cx.f("text", input);
        cx.send(Out::SetClipboard(text));
        cx.fire("done", input);
    }
}

/// Web request: done carries the answer's text.
#[derive(Default)]
pub struct Http {
    body: String,
    code: String,
}

impl NodeRuntime for Http {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let url = cx.f("url", input).trim().to_owned();
        let method = match cx.raw_str("method") {
            m if m.is_empty() => "GET".to_owned(),
            m => m,
        };
        let data = cx.f("body", input);
        let mut headers = vec![("User-Agent".to_owned(), "Luminescent".to_owned())];
        for line in text::split_lines(&cx.raw_str("headers")) {
            let (k, v) = line.split_once(':').unwrap_or((line, ""));
            if !k.trim().is_empty() && !v.trim().is_empty() {
                let v = cx.expand(v.trim(), input);
                headers.push((k.trim().to_owned(), v));
            }
        }
        if !data.is_empty()
            && method != "GET"
            && !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        {
            let json = data.trim_start().starts_with(['{', '[']);
            headers
                .push(("Content-Type".into(), if json { "application/json" } else { "text/plain" }.into()));
        }
        cx.set_status(format!("{method} …"), None);
        cx.spawn(1, move |job| {
            let (ok, text, code) = request(&method, &url, &data, &headers);
            job.send(JobMsg::Done { ok, text, code });
        });
    }

    fn job(&mut self, _token: u64, msg: JobMsg, cx: &mut Cx<'_>) {
        let JobMsg::Done { ok, text, code } = msg else { return };
        self.body = text;
        self.code = code;
        cx.set_status(if ok { self.code.clone() } else { format!("⚠ failed {}", self.code) }, None);
        let body = self.body.clone();
        cx.fire(if ok { "done" } else { "failed" }, &body);
    }

    fn value(&self, port: &str, _input: &str, _cx: &mut Cx<'_>) -> String {
        if port == "status" { self.code.clone() } else { self.body.clone() }
    }
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Whether it went well (as Python's urllib: a status under 400), the answer's text and the status code.
fn request(method: &str, url: &str, data: &str, headers: &[(String, String)]) -> (bool, String, String) {
    let mut builder = ureq::http::Request::builder().method(method).uri(url);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let sent = if !data.is_empty() && method != "GET" {
        builder.body(data.as_bytes().to_vec()).map_err(|e| e.to_string()).and_then(run)
    } else {
        builder.body(()).map_err(|e| e.to_string()).and_then(run)
    };
    match sent {
        Ok((status, body)) => (status < 400, body, status.to_string()),
        Err(e) => (false, e, String::new()),
    }
}

fn run(request: ureq::http::Request<impl ureq::AsSendBody>) -> Result<(u16, String), String> {
    let mut response = agent(30).run(request).map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    let _ = response.body_mut().as_reader().take(MAX_TEXT as u64).read_to_end(&mut bytes);
    Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

/// Download a file: done carries the saved file's path.
pub struct Download;

impl NodeRuntime for Download {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let url = cx.f("url", input).trim().to_owned();
        let folder = match cx.raw_str("folder") {
            f if f.is_empty() => "~/Downloads".to_owned(),
            f => f,
        };
        let folder = expand_user(&folder);
        let name = match cx.f("name", input).trim() {
            "" => match url_file_name(&url) {
                n if n.is_empty() => "download".to_owned(),
                n => n,
            },
            n => n.to_owned(),
        };
        let path = folder.join(name);
        cx.set_status("downloading…", None);
        cx.spawn(1, move |job| {
            let got = download(&url, &folder, &path);
            let (ok, text) = match got {
                Ok(()) => (true, path.to_string_lossy().into_owned()),
                Err(e) => (false, e),
            };
            job.send(JobMsg::Done { ok, text, code: String::new() });
        });
    }

    fn job(&mut self, _token: u64, msg: JobMsg, cx: &mut Cx<'_>) {
        let JobMsg::Done { ok, text, .. } = msg else { return };
        if ok {
            cx.set_status("done", None);
            cx.fire("done", &text);
        } else {
            cx.set_status(format!("⚠ {text}"), None);
            cx.fire("failed", &text);
        }
    }
}

/// The last part of a URL's path (`os.path.basename(urlparse(url).path)`).
fn url_file_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.split_once('/').map_or("", |(_, p)| p);
    let path = path.split(['?', '#']).next().unwrap_or_default();
    path.rsplit('/').next().unwrap_or_default().to_owned()
}

fn download(url: &str, folder: &Path, path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let mut response =
        agent(60).get(url).header("User-Agent", "Luminescent").call().map_err(|e| e.to_string())?;
    let status = response.status();
    if status.as_u16() >= 400 {
        return Err(format!(
            "HTTP Error {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or_default()
        ));
    }
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}

/// Copy / move / delete: done carries the new path. Delete moves nothing to the bin.
pub struct FileOp;

impl NodeRuntime for FileOp {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let op = cx.raw_str("op");
        let src = expand_user(cx.f("src", input).trim());
        let dst = expand_user(cx.f("dst", input).trim());
        cx.spawn(1, move |job| {
            let (ok, text) = match file_op(&op, &src, &dst) {
                Ok(done) => (true, done.to_string_lossy().into_owned()),
                Err(e) => (false, e.to_string()),
            };
            job.send(JobMsg::Done { ok, text, code: String::new() });
        });
    }

    fn job(&mut self, _token: u64, msg: JobMsg, cx: &mut Cx<'_>) {
        let JobMsg::Done { ok, text, .. } = msg else { return };
        if ok {
            cx.set_status("", None);
            cx.fire("done", &text);
        } else {
            cx.set_status(format!("⚠ {text}"), None);
            cx.fire("failed", &text);
        }
    }
}

fn file_op(op: &str, src: &Path, dst: &Path) -> std::io::Result<PathBuf> {
    match op {
        "mkdir" => std::fs::create_dir_all(src).map(|_| src.to_owned()),
        "delete" => {
            if src.is_dir() {
                std::fs::remove_dir_all(src)?;
            } else {
                std::fs::remove_file(src)?;
            }
            Ok(src.to_owned())
        }
        _ => {
            let target = match src.file_name() {
                Some(name) if dst.is_dir() => dst.join(name),
                _ => dst.to_owned(),
            };
            if op == "move" {
                move_path(src, &target)?;
            } else if src.is_dir() {
                copy_tree(src, &target)?;
            } else {
                std::fs::copy(src, &target)?;
            }
            Ok(target)
        }
    }
}

/// `shutil.move`: a rename, or a copy and delete when the rename can not cross between disks.
fn move_path(src: &Path, target: &Path) -> std::io::Result<()> {
    if std::fs::rename(src, target).is_ok() {
        return Ok(());
    }
    if src.is_dir() {
        copy_tree(src, target)?;
        std::fs::remove_dir_all(src)
    } else {
        std::fs::copy(src, target)?;
        std::fs::remove_file(src)
    }
}

/// `shutil.copytree`: fails when the target is there already.
fn copy_tree(src: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir(target)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Write to a file: adds to its end or replaces it.
pub struct WriteFile;

impl NodeRuntime for WriteFile {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let path = expand_user(cx.f("path", input).trim());
        let mut text = cx.f("text", input);
        if cx.flag("newline") {
            text.push('\n');
        }
        let append = cx.raw_str("mode") == "append";
        match write_file(&path, &text, append) {
            Ok(()) => cx.fire("done", &path.to_string_lossy()),
            Err(e) => {
                cx.set_status(format!("⚠ {e}"), None);
                cx.fire("failed", &e.to_string());
            }
        }
    }
}

fn write_file(path: &Path, text: &str, append: bool) -> std::io::Result<()> {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(folder)?;
    let mut file =
        std::fs::OpenOptions::new().create(true).append(append).write(true).truncate(!append).open(path)?;
    file.write_all(text.as_bytes())
}

/// Warning on screen: a big warning in the middle of the screen; ok fires when it closes.
#[derive(Default)]
pub struct Alert {
    waiting: Waiting,
}

impl NodeRuntime for Alert {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let token = self.waiting.push(input);
        let level = match cx.raw_str("level") {
            l if l.is_empty() => "warning".to_owned(),
            l => l,
        };
        let (title, text, seconds) =
            (cx.f("title", input), cx.f("text", input), cx.num("seconds", input, 0.0));
        cx.send(Out::Alert {
            title,
            text,
            level,
            seconds,
            sound: cx.raw_str("sound"),
            reply: Some(cx.reply(token)),
        });
    }

    fn answer(&mut self, token: u64, _answer: Answer, cx: &mut Cx<'_>) {
        if let Some(input) = self.waiting.take(token) {
            cx.fire("ok", &input);
        }
    }
}

/// Volume: the default speakers' volume; the volume port reads it as a number.
#[derive(Default)]
pub struct Volume {
    waiting: Waiting,
}

impl NodeRuntime for Volume {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let token = self.waiting.push(input);
        let amount = cx.num("amount", input, 50.0);
        cx.send(Out::Volume { mode: cx.raw_str("mode"), amount, reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        let Some(input) = self.waiting.take(token) else { return };
        match answer {
            Answer::Done(status) => {
                cx.set_status(status, None);
                cx.fire("done", &input);
            }
            Answer::Failed(e) => cx.set_status(format!("⚠ {e}"), None),
            Answer::Cancelled => {}
        }
    }

    fn value(&self, _port: &str, _input: &str, cx: &mut Cx<'_>) -> String {
        cx.host().volume().map(|(v, _)| format!("{v:.0}")).unwrap_or_default()
    }
}

/// Lock / sleep / shut down, asking first when it should.
#[derive(Default)]
pub struct Power {
    waiting: Waiting,
}

const ASKED: u64 = 1 << 32;

impl Power {
    fn go(&mut self, action: String, input: &str, cx: &mut Cx<'_>) {
        cx.fire("done", input);
        // after the pulse went out, so a done → notification still shows before the screen locks
        let token = self.waiting.push(&action);
        cx.after(0.3, token);
    }
}

impl NodeRuntime for Power {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let action = cx.raw_str("action");
        if cx.flag("ask") && action != "lock" && action != "screen_off" {
            let label = choice_label(cx, "action", &action);
            let token = ASKED | self.waiting.push(input);
            let text = format!("{}: {} now?", cx.label(), label.to_lowercase());
            cx.send(Out::Confirm { title: "Automation".into(), text, yes: label, reply: cx.reply(token) });
            return;
        }
        self.go(action, input, cx);
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        if token & ASKED != 0 {
            let Some(input) = self.waiting.take(token & !ASKED) else { return };
            if matches!(answer, Answer::Done(_)) {
                let action = cx.raw_str("action");
                self.go(action, &input, cx);
            }
        } else if let Answer::Failed(e) = answer {
            cx.set_status(format!("⚠ {e}"), None);
        }
    }

    fn timer(&mut self, token: u64, cx: &mut Cx<'_>) {
        if let Some(action) = self.waiting.take(token) {
            cx.send(Out::Power { action, reply: cx.reply(0) });
        }
    }
}

/// What a choice field shows for `value`.
fn choice_label(cx: &Cx<'_>, key: &str, value: &str) -> String {
    match cx.spec.field(key).map(|f| &f.choices) {
        Some(Choices::Fixed(list)) => list.iter().find(|(v, _)| v == value).map(|(_, l)| l.clone()),
        _ => None,
    }
    .unwrap_or_else(|| value.to_owned())
}

/// Keep awake: on, off or the other way.
#[derive(Default)]
pub struct KeepAwake {
    waiting: Waiting,
}

impl NodeRuntime for KeepAwake {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let on = match cx.raw_str("mode").as_str() {
            "toggle" => None,
            mode => Some(mode == "on"),
        };
        let token = self.waiting.push(input);
        let why = format!("Automation: {}", cx.label());
        cx.send(Out::KeepAwake { on, why, reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        let Some(input) = self.waiting.take(token) else { return };
        match answer {
            Answer::Done(status) => {
                cx.set_status(status, None);
                cx.fire("done", &input);
            }
            Answer::Failed(e) => cx.set_status(format!("⚠ {e}"), None),
            Answer::Cancelled => {}
        }
    }

    fn value(&self, _port: &str, _input: &str, cx: &mut Cx<'_>) -> String {
        if cx.host().keep_awake_on() { "yes".into() } else { "no".into() }
    }
}

/// Open a note: the Notes panel on that note.
#[derive(Default)]
pub struct OpenNote {
    waiting: Waiting,
}

impl NodeRuntime for OpenNote {
    fn receive(&mut self, _port: &str, input: &str, cx: &mut Cx<'_>) {
        let token = self.waiting.push(input);
        cx.send(Out::OpenNote { id: cx.raw_str("note"), reply: cx.reply(token) });
    }

    fn answer(&mut self, token: u64, answer: Answer, cx: &mut Cx<'_>) {
        let Some(input) = self.waiting.take(token) else { return };
        match answer {
            Answer::Done(_) => cx.fire("done", &input),
            Answer::Failed(e) => cx.set_status(format!("⚠ {e}"), None),
            Answer::Cancelled => {}
        }
    }
}
