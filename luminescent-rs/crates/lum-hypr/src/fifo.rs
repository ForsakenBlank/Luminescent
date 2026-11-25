//! The FIFO our binds write their ids into (`LUMI.send` in the bootstrap). Hyprland runs a tiny shell command
//! per key press, so a bind costs nothing on our side until it fires.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::unix::pipe;

/// Makes the FIFO (and its folder, private to us). Something else at that path is replaced.
pub fn create(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_fifo() => return Ok(()),
        Ok(_) => std::fs::remove_file(path)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a nul in the FIFO path"))?;
    // SAFETY: `c` is a valid, nul-terminated path that lives for the call.
    if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Hands every line written into the FIFO to `sink`, for as long as the app runs.
pub async fn listen(path: &Path, mut sink: impl FnMut(String) + Send) -> io::Result<()> {
    create(path)?;
    // opened for reading and writing, so the reader never sees the end of the file when a writer goes away
    let receiver = pipe::OpenOptions::new().read_write(true).open_receiver(path)?;
    let mut lines = BufReader::new(receiver).lines();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if !line.is_empty() {
            sink(line.to_owned());
        }
    }
    Ok(())
}
