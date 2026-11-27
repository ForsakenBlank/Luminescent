//! The first picture of a video, for the thumbnail on a card. `ffmpeg` is optional, as it is for the metadata
//! stripper: without it a video card keeps its striped placeholder.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::terminal;

/// The first frame as PNG bytes, or `None` when there is no `ffmpeg` or the file will not play.
pub fn video_frame(path: &Path) -> Option<Vec<u8>> {
    let ffmpeg = terminal::which("ffmpeg")?;
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let out = command.output().ok()?;
    (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_is_not_a_video_gives_nothing() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let path = dir.path().join("not-a-video.mp4");
        std::fs::write(&path, b"text").expect("written");
        assert!(video_frame(&path).is_none());
    }
}
