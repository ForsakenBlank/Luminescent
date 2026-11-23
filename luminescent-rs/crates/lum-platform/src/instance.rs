//! One Luminescent per data folder: two copies would fight over hotkeys and overwrite each other's files.
//!
//! The lock is an OS file lock on `data/instance.lock`, so it goes away by itself when the process ends, even
//! after a crash, and there is never a stale lock to clean up.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// Held for as long as the app runs.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

/// Takes the lock, waiting up to `wait` for another copy to let go (a restart hands over within a second or
/// two). `None` means another copy is running.
pub fn acquire(path: &Path, wait: Duration) -> io::Result<Option<InstanceLock>> {
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path)?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(InstanceLock { _file: file })),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100))
            }
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(e)) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_holder_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instance.lock");
        let first = acquire(&path, Duration::ZERO).unwrap();
        assert!(first.is_some());
        assert!(acquire(&path, Duration::from_millis(150)).unwrap().is_none());
        drop(first);
        assert!(acquire(&path, Duration::ZERO).unwrap().is_some());
    }
}
