//! Sending a file or folder to the Trash (the Recycle Bin on Windows) instead of deleting it for good.
//!
//! The desktop's own tools do the work, the way the Python app falls back to `gio trash`: they know the Trash
//! folder of whichever disk the file is on, which is easy to get wrong by hand. This waits for the tool, so keep
//! it off the UI thread.

use std::path::Path;
use std::process::Command;

#[cfg(unix)]
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    let path = std::fs::canonicalize(path).map_err(|e| format!("Could not move it to the Trash ({e})."))?;
    let mut last = String::from("Could not move it to the Trash.");
    for (program, args) in [("gio", vec!["trash"]), ("trash-put", Vec::new()), ("kioclient5", vec!["move"])] {
        let mut command = Command::new(program);
        command.args(&args).arg(&path);
        if program == "kioclient5" {
            command.arg("trash:/");
        }
        match command.output() {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stderr).trim().to_owned();
                if !text.is_empty() {
                    last = text;
                }
            }
            Err(_) => {}
        }
    }
    Err(last)
}

#[cfg(windows)]
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "Add-Type -AssemblyName Microsoft.VisualBasic; \
        $p = $env:LUM_TRASH; \
        if (Test-Path -LiteralPath $p -PathType Container) { \
            [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p, 'OnlyErrorDialogs', 'SendToRecycleBin') \
        } else { \
            [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($p, 'OnlyErrorDialogs', 'SendToRecycleBin') }";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("LUM_TRASH", path)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not move it to the Recycle Bin ({e})."))?;
    if out.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(if text.is_empty() { "Could not move it to the Recycle Bin.".to_owned() } else { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().expect("a temp folder");
        assert!(move_to_trash(&dir.path().join("nothing-here")).is_err());
    }
}
