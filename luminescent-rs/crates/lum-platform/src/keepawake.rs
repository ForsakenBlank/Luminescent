//! Keep awake: stops the screen going dark and the PC going to sleep while it is on. Windows asks for it with
//! `SetThreadExecutionState`, which belongs to the thread that called it; Linux holds a logind inhibitor
//! lock (idle and sleep) for as long as it is on, which hypridle, KDE and GNOME all respect.
//!
//! Both live on one thread of their own, so the lock outlives no thread and asking for it never holds up the
//! UI. The answer comes back through the callback.

use std::sync::Arc;

/// Called with whether keep awake is on now, and the error text when it could not change.
pub type Report = Arc<dyn Fn(bool, Option<String>) + Send + Sync>;

enum Cmd {
    Set(bool, String),
    Stop,
}

pub struct KeepAwake {
    tx: crossbeam_channel::Sender<Cmd>,
    on: bool,
}

impl KeepAwake {
    pub fn start(report: Report) -> KeepAwake {
        let (tx, rx) = crossbeam_channel::unbounded();
        let spawned = std::thread::Builder::new().name("keep-awake".into()).spawn(move || {
            let mut held: Option<imp::Lock> = None;
            for cmd in rx {
                match cmd {
                    Cmd::Set(true, why) if held.is_none() => match imp::Lock::take(&why) {
                        Ok(lock) => {
                            held = Some(lock);
                            tracing::info!("keep awake on");
                            report(true, None);
                        }
                        Err(e) => {
                            tracing::warn!("keep awake: {e}");
                            report(false, Some(e));
                        }
                    },
                    Cmd::Set(false, _) if held.is_some() => {
                        held = None;
                        tracing::info!("keep awake off");
                        report(false, None);
                    }
                    Cmd::Set(..) => {}
                    Cmd::Stop => return,
                }
            }
        });
        if let Err(e) = spawned {
            tracing::error!("could not start keep awake: {e}");
        }
        KeepAwake { tx, on: false }
    }

    /// Asks for it on or off; the answer comes through the report.
    pub fn set(&mut self, on: bool, why: &str) {
        self.on = on;
        let why = if why.is_empty() { "Keep awake is on" } else { why };
        let _ = self.tx.send(Cmd::Set(on, why.to_owned()));
    }

    pub fn toggle(&mut self, why: &str) {
        self.set(!self.on, why);
    }

    /// What was last asked for (the report says whether it worked).
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// The app's own idea after a report (a failed "on" leaves it off).
    pub fn reported(&mut self, on: bool) {
        self.on = on;
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Set(false, String::new()));
        let _ = self.tx.send(Cmd::Stop);
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use zbus::zvariant::OwnedFd;

    /// The inhibitor lock lasts while its file descriptor is open; the bus connection can go.
    pub struct Lock(#[allow(dead_code)] OwnedFd);

    impl Lock {
        pub fn take(why: &str) -> Result<Lock, String> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            runtime.block_on(async {
                let bus = zbus::Connection::system().await.map_err(|e| e.to_string())?;
                let reply = bus
                    .call_method(
                        Some("org.freedesktop.login1"),
                        "/org/freedesktop/login1",
                        Some("org.freedesktop.login1.Manager"),
                        "Inhibit",
                        &("idle:sleep", lum_core::APP_NAME, why, "block"),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let fd: OwnedFd = reply.body().deserialize().map_err(|e| e.to_string())?;
                Ok(Lock(fd))
            })
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };

    pub struct Lock;

    impl Lock {
        pub fn take(_why: &str) -> Result<Lock, String> {
            // SAFETY: no preconditions; the request belongs to this thread, which lives as long as the lock.
            let was = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) };
            if was == 0 { Err("Windows said no".to_owned()) } else { Ok(Lock) }
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            // SAFETY: as above; this clears what take() asked for.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub struct Lock;

    impl Lock {
        pub fn take(_why: &str) -> Result<Lock, String> {
            Err("keep awake is not available here".to_owned())
        }
    }
}
