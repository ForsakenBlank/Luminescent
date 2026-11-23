//! The ctl socket: how `luminescent ctl <command>`, a second start, a compositor bind or a script talk to the
//! running app. A Unix socket in `$XDG_RUNTIME_DIR/luminescent/` on Linux, a named pipe on Windows.
//!
//! One request per connection: the client writes one line, the app answers and closes the connection.

use std::io::{self, BufReader, Read, Write};
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use interprocess::local_socket::tokio::{Listener as TokioListener, Stream as TokioStream, prelude::*};
use interprocess::local_socket::traits::Stream as _;
use interprocess::local_socket::{ListenerOptions, Name, Stream};
use lum_core::Paths;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

const MAX_LINE: u64 = 64 * 1024;
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// A line that came in, and where its answer goes.
#[derive(Debug)]
pub struct CtlRequest {
    pub line: String,
    pub reply: oneshot::Sender<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtlEndpoint {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(windows)]
    pipe: String,
}

impl CtlEndpoint {
    /// The endpoint for this data folder. A copy with its own `LUMINESCENT_HOME`, and a dev build, get a name
    /// of their own so they can run next to the real app (the Python lock file is tagged the same way).
    pub fn for_home(paths: &Paths) -> CtlEndpoint {
        let tag = if paths.custom_home || cfg!(debug_assertions) {
            format!("-{:08x}", crc32fast::hash(paths.home.to_string_lossy().as_bytes()))
        } else {
            String::new()
        };
        CtlEndpoint::named(&format!("ctl{tag}"))
    }

    #[cfg(unix)]
    fn named(stem: &str) -> CtlEndpoint {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).map(PathBuf::from);
        let base = runtime.unwrap_or_else(|| match std::env::var("USER") {
            Ok(user) if !user.is_empty() => std::env::temp_dir().join(format!("luminescent-{user}")),
            _ => std::env::temp_dir(),
        });
        CtlEndpoint { path: base.join("luminescent").join(format!("{stem}.sock")) }
    }

    #[cfg(windows)]
    fn named(stem: &str) -> CtlEndpoint {
        CtlEndpoint { pipe: format!("luminescent-{stem}") }
    }

    pub fn describe(&self) -> String {
        #[cfg(unix)]
        return self.path.display().to_string();
        #[cfg(windows)]
        return format!(r"\\.\pipe\{}", self.pipe);
    }

    #[cfg(unix)]
    fn name(&self) -> io::Result<Name<'static>> {
        use interprocess::local_socket::GenericFilePath;
        self.path.clone().to_fs_name::<GenericFilePath>()
    }

    #[cfg(windows)]
    fn name(&self) -> io::Result<Name<'static>> {
        use interprocess::local_socket::GenericNamespaced;
        self.pipe.clone().to_ns_name::<GenericNamespaced>()
    }

    /// Starts listening. Call it inside the tokio runtime, and only while holding the instance lock: a socket
    /// left behind by a crash is replaced.
    pub fn listen(&self) -> io::Result<TokioListener> {
        #[cfg(unix)]
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        ListenerOptions::new().name(self.name()?).try_overwrite(true).create_tokio()
    }

    /// Sends one line to the running app and gives back its answer. Blocks; for the command line.
    pub fn request(&self, line: &str, timeout: Duration) -> io::Result<String> {
        let stream = Stream::connect(self.name()?)?;
        stream.set_recv_timeout(Some(timeout))?;
        stream.set_send_timeout(Some(timeout))?;
        let mut writer = &stream;
        writer.write_all(format!("{}\n", line.trim()).as_bytes())?;
        writer.flush()?;
        let mut answer = String::new();
        BufReader::new(&stream).take(MAX_LINE).read_to_string(&mut answer)?;
        Ok(answer.trim_end().to_owned())
    }
}

/// Answers connections until the runtime shuts down, handing each line to `requests`.
pub async fn serve(listener: TokioListener, requests: crossbeam_channel::Sender<CtlRequest>) {
    loop {
        match listener.accept().await {
            Ok(conn) => {
                let requests = requests.clone();
                tokio::spawn(async move {
                    if let Err(e) = answer(conn, requests).await {
                        tracing::debug!("ctl connection: {e}");
                    }
                });
            }
            Err(e) => tracing::warn!("ctl socket: {e}"),
        }
    }
}

async fn answer(conn: TokioStream, requests: crossbeam_channel::Sender<CtlRequest>) -> io::Result<()> {
    let mut line = String::new();
    tokio::io::BufReader::new(&conn).take(MAX_LINE).read_line(&mut line).await?;
    let (reply, answered) = oneshot::channel();
    let text = if requests.send(CtlRequest { line: line.trim().to_owned(), reply }).is_err() {
        "Luminescent is shutting down.".to_owned()
    } else {
        match tokio::time::timeout(ANSWER_WITHIN, answered).await {
            Ok(Ok(text)) => text,
            _ => "Luminescent did not answer in time.".to_owned(),
        }
    };
    let mut writer = &conn;
    writer.write_all(format!("{text}\n").as_bytes()).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_goes_in_and_its_answer_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_ne!(
            CtlEndpoint::for_home(&Paths::custom(dir.path())),
            CtlEndpoint::for_home(&Paths::custom(dir.path().join("other"))),
            "each test copy has its own socket"
        );
        // the socket file goes in the temp folder, never the real runtime folder
        #[cfg(unix)]
        let (endpoint, other) = (
            CtlEndpoint { path: dir.path().join("ctl.sock") },
            CtlEndpoint { path: dir.path().join("other.sock") },
        );
        #[cfg(windows)]
        let (endpoint, other) =
            (CtlEndpoint::named(&format!("test-{}", std::process::id())), CtlEndpoint::named("test-nobody"));

        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (tx, rx) = crossbeam_channel::unbounded::<CtlRequest>();
        let listener = runtime.block_on(async { endpoint.listen() }).unwrap();
        std::thread::spawn(move || runtime.block_on(serve(listener, tx)));
        std::thread::spawn(move || {
            for request in rx {
                let _ = request.reply.send(format!("got {}", request.line));
            }
        });
        assert_eq!(endpoint.request("ping", Duration::from_secs(5)).unwrap(), "got ping");
        assert_eq!(
            endpoint.request("  action notes:new_note \n", Duration::from_secs(5)).unwrap(),
            "got action notes:new_note"
        );
        assert!(other.request("ping", Duration::from_secs(1)).is_err(), "nobody listens there");
    }
}
