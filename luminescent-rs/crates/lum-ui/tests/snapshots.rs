//! Pictures of the widget gallery and the dialogs in Dark and Light, compared with the ones in
//! `tests/snapshots/`. After a change to the look, look at the `.new.png` files, and if they are right run
//! `UPDATE_SNAPSHOTS=1 cargo test -p lum-ui --test snapshots` and commit the pictures.
//!
//! Only Linux compares pixels (CI draws with Mesa's lavapipe there). Other systems' software renderers smooth
//! edges a little differently, so there the gallery is built and stepped but not photographed.

#![allow(clippy::unwrap_used)]

use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use lum_ui::gallery::Gallery;

fn harness(gallery: Gallery, height: f32) -> Harness<'static, Gallery> {
    let builder = Harness::builder().with_size([1280.0, height]).with_pixels_per_point(1.0);
    // only Linux takes pictures, so only Linux needs a GPU adapter (CI's Windows runners may have none)
    let builder = if cfg!(target_os = "linux") { builder.wgpu() } else { builder };
    builder.build_ui_state(|ui, gallery: &mut Gallery| gallery.ui(ui), gallery)
}

fn photograph(harness: &mut Harness<'_, Gallery>, name: &str) {
    harness.run();
    if cfg!(target_os = "linux") {
        // a little slack for Mesa versions that round an edge pixel differently
        let options = SnapshotOptions::new().threshold(1.5).max_failed_pixels(200);
        harness.snapshot_options(name, &options);
    }
}

#[test]
fn gallery_dark() {
    photograph(&mut harness(Gallery::still(false), 1420.0), "gallery_dark");
}

#[test]
fn gallery_light() {
    photograph(&mut harness(Gallery::still(true), 1420.0), "gallery_light");
}

#[test]
fn dialogs() {
    let mut results = SnapshotResults::new();
    for (light, theme) in [(false, "dark"), (true, "light")] {
        for (which, name) in [(2, "confirm"), (4, "ask_text")] {
            let mut gallery = Gallery::still(light);
            gallery.panel.close();
            gallery.open_dialog(which);
            let mut harness = harness(gallery, 700.0);
            photograph(&mut harness, &format!("dialog_{name}_{theme}"));
            results.extend_harness(&mut harness);
        }
    }
    results.unwrap();
}
