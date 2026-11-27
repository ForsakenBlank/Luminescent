//! Copying from a tool. The clipboard can wait on whoever owns it, so it is never done on the UI thread.

pub fn copy(text: impl Into<String>) {
    let text = text.into();
    let spawned = std::thread::Builder::new().name("copy".into()).spawn(move || {
        if let Err(e) = lum_platform::clipboard::set_text(&text) {
            tracing::warn!("could not copy: {e}");
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("could not start the copy: {e}");
    }
}
