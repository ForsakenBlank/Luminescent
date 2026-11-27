//! Text recognition for Text Grab. Linux runs Tesseract (as the Python app did; it reads the picture from
//! stdin, so no temporary file is left behind). Windows uses its own engine (`Windows.Media.Ocr`) with the
//! languages set up in Windows Settings, and Tesseract only when that has no language at all.
//! Recognising takes a moment: call it from a worker thread.

use lum_screen::{Frame, ocr};

/// The text in the picture, its lines kept or joined with spaces. The error is a sentence for the person.
pub fn recognise(frame: &Frame, keep_lines: bool) -> Result<String, String> {
    #[cfg(windows)]
    {
        match windows_ocr::recognise(frame, keep_lines) {
            Ok(text) => return Ok(text),
            Err(e) if tesseract().is_none() => return Err(e),
            Err(e) => tracing::info!("Windows OCR: {e}; trying Tesseract"),
        }
    }
    tesseract_ocr(frame, keep_lines)
}

/// Where Tesseract is: on the PATH, or on Windows where its installer puts it.
pub fn tesseract() -> Option<std::path::PathBuf> {
    if let Some(found) = crate::terminal::which("tesseract") {
        return Some(found);
    }
    if cfg!(windows) {
        let bases = [
            std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into()),
            std::env::var_os("LOCALAPPDATA").unwrap_or_default(),
        ];
        for base in bases {
            let candidate = std::path::Path::new(&base).join("Tesseract-OCR").join("tesseract.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn tesseract_ocr(frame: &Frame, keep_lines: bool) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let Some(exe) = tesseract() else {
        return Err(if cfg!(windows) {
            "Windows has no OCR language installed. Add one in Windows Settings → Time & language → Language & \
             region (e.g. English with 'Optical character recognition')."
                .to_owned()
        } else {
            "Tesseract is not installed. Run:\nsudo pacman -S tesseract tesseract-data-eng".to_owned()
        });
    };
    let listing = Command::new(&exe).arg("--list-langs").stdin(Stdio::null()).output();
    let langs = match listing {
        Ok(out) => ocr::languages(&String::from_utf8_lossy(&out.stdout))?,
        Err(_) => "eng".to_owned(),
    };
    let png = frame.png()?;
    // --psm 6: one block of text, which is what a dragged box usually is
    let mut child = Command::new(&exe)
        .args(["stdin", "stdout", "-l", &langs, "--psm", "6"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Tesseract failed: {e}"))?;
    let feed = child.stdin.take().map(|mut stdin| std::thread::spawn(move || stdin.write_all(&png)));
    let out = child.wait_with_output().map_err(|e| format!("Tesseract failed: {e}"))?;
    if let Some(feed) = feed {
        let _ = feed.join();
    }
    if !out.status.success() {
        let why: String = String::from_utf8_lossy(&out.stderr).trim().chars().take(200).collect();
        return Err(format!("Tesseract failed: {why}"));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(ocr::join_lines(text.lines(), keep_lines))
}

#[cfg(windows)]
mod windows_ocr {
    use lum_screen::{Frame, ocr};
    use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::DataWriter;

    pub fn recognise(frame: &Frame, keep_lines: bool) -> Result<String, String> {
        let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| no_language())?;
        // Windows' engine reads pictures up to its own size limit; text grab boxes stay far below it
        let mut bgra = frame.rgba.clone();
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let writer = DataWriter::new().map_err(|e| e.message())?;
        writer.WriteBytes(&bgra).map_err(|e| e.message())?;
        let buffer = writer.DetachBuffer().map_err(|e| e.message())?;
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            frame.width as i32,
            frame.height as i32,
            BitmapAlphaMode::Premultiplied,
        )
        .map_err(|e| e.message())?;
        let result = engine.RecognizeAsync(&bitmap).and_then(|op| op.join()).map_err(|e| e.message())?;
        let lines = result.Lines().map_err(|e| e.message())?;
        let texts: Vec<String> =
            lines.into_iter().filter_map(|line| line.Text().ok()).map(|t| t.to_string()).collect();
        Ok(ocr::join_lines(texts.iter().map(String::as_str), keep_lines))
    }

    fn no_language() -> String {
        "Windows has no OCR language installed. Add one in Windows Settings → Time & language → Language & \
         region (e.g. English with 'Optical character recognition')."
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real recognition, when Tesseract is installed here (it is not in CI): a picture of text drawn with the
    /// app's own font must come back as that text.
    #[test]
    fn tesseract_reads_a_picture_of_text_when_it_is_installed() {
        if cfg!(windows) || tesseract().is_none() {
            return;
        }
        let frame = lum_screen::ocr::sample_text_picture("Luminescent reads this");
        let text = recognise(&lum_screen::ocr::prepare(&frame, true), true).expect("recognised");
        assert_eq!(text, "Luminescent reads this");
    }
}
