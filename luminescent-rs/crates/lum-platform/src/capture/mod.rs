//! Screenshots of whole monitors at their real pixels, for the tools that freeze the screen (colour picker,
//! ruler, text grab) and the doodle's copy of the screen.
//!
//! On Hyprland (and other wlroots desktops) the screencopy protocol hands over each monitor straight from the
//! compositor; `grim` is the fallback when that fails, as it was the Python app's only way. Windows copies the
//! screen with GDI, which is what Qt's `grabWindow` did there. Every monitor is grabbed on its own thread, so
//! two screens take as long as the slower one.

use std::sync::Arc;
use std::time::Instant;

pub use lum_screen::Frame;

use crate::desktop::Monitor;

#[cfg(windows)]
mod gdi;
#[cfg(any(target_os = "linux", test))]
mod ppm;
#[cfg(target_os = "linux")]
mod screencopy;

/// Something that can take a picture of a monitor. The system's own is [`system`]; tests hand the tools a
/// [`Fake`].
pub trait Capture: Send + Sync {
    /// Every monitor in `monitors`, in the same order. A monitor that could not be grabbed is an error.
    fn grab(&self, monitors: &[Monitor]) -> Vec<Result<Frame, String>>;
}

/// The capture this system has.
pub fn system() -> Arc<dyn Capture> {
    Arc::new(System)
}

struct System;

impl Capture for System {
    fn grab(&self, monitors: &[Monitor]) -> Vec<Result<Frame, String>> {
        let started = Instant::now();
        let shots = imp::grab(monitors);
        tracing::debug!(
            "grabbed {} screen(s) in {:.0} ms",
            shots.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        shots
    }
}

/// Stand-in screens for tests: each monitor is a picture of its size times its scale, a grid of coloured
/// squares so a test can tell where it read a pixel.
#[derive(Debug, Default)]
pub struct Fake {
    /// When set, every grab fails with this.
    pub fail: Option<String>,
}

impl Fake {
    /// The colour the fake puts at a real pixel: red counts columns of 16 pixels, green rows, blue is the
    /// monitor's place in the list with a checkerboard of single pixels on top (so a loupe shows them).
    pub fn colour_at(index: usize, x: u32, y: u32) -> [u8; 3] {
        let checker = if (x + y).is_multiple_of(2) { 0 } else { 40 };
        [(x / 16 % 256) as u8, (y / 16 % 256) as u8, ((index * 60 + checker) % 256) as u8]
    }

    pub fn frame(index: usize, m: &Monitor) -> Frame {
        let (w, h) = real_size(m);
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = Fake::colour_at(index, x, y);
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Frame { width: w, height: h, rgba }
    }
}

impl Capture for Fake {
    fn grab(&self, monitors: &[Monitor]) -> Vec<Result<Frame, String>> {
        monitors
            .iter()
            .enumerate()
            .map(|(i, m)| match &self.fail {
                Some(e) => Err(e.clone()),
                None => Ok(Fake::frame(i, m)),
            })
            .collect()
    }
}

/// A monitor's size in real pixels. Hyprland lists monitors in layout pixels and a scale; Windows lists them
/// in real pixels already.
pub fn real_size(m: &Monitor) -> (u32, u32) {
    if cfg!(windows) {
        return (m.width, m.height);
    }
    let scale = f64::from(m.scale.max(0.1));
    ((f64::from(m.width) * scale).round() as u32, (f64::from(m.height) * scale).round() as u32)
}

/// Runs `grab_one` for each monitor on a thread of its own and collects the results in order.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn each_on_a_thread(
    monitors: &[Monitor],
    grab_one: impl Fn(&Monitor) -> Result<Frame, String> + Sync,
) -> Vec<Result<Frame, String>> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = monitors.iter().map(|m| scope.spawn(|| grab_one(m))).collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("the screenshot thread failed".to_owned())))
            .collect()
    })
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Frame, Monitor, each_on_a_thread, ppm, screencopy};

    pub fn grab(monitors: &[Monitor]) -> Vec<Result<Frame, String>> {
        let names: Vec<&str> = monitors.iter().map(|m| m.name.as_str()).collect();
        match screencopy::grab(&names) {
            Ok(frames) => return frames,
            Err(e) => tracing::info!("screencopy is not available ({e}); using grim"),
        }
        each_on_a_thread(monitors, grim)
    }

    /// One monitor through `grim`, as PPM (no compression to wait for).
    fn grim(m: &Monitor) -> Result<Frame, String> {
        let out = std::process::Command::new("grim")
            .args(["-o", &m.name, "-t", "ppm", "-"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("could not run grim ({e}); install grim"))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            return Err(format!("grim failed: {}", why.trim()));
        }
        ppm::read(&out.stdout)
    }
}

#[cfg(windows)]
mod imp {
    use super::{Frame, Monitor, each_on_a_thread, gdi};

    pub fn grab(monitors: &[Monitor]) -> Vec<Result<Frame, String>> {
        each_on_a_thread(monitors, gdi::grab)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::{Frame, Monitor};

    pub fn grab(monitors: &[Monitor]) -> Vec<Result<Frame, String>> {
        monitors.iter().map(|_| Err("screenshots are not supported here".to_owned())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, w: u32, h: u32, scale: f32) -> Monitor {
        Monitor {
            name: name.into(),
            x: 0,
            y: 0,
            width: w,
            height: h,
            scale,
            primary: false,
            top_inset: 0,
            bottom_inset: 0,
        }
    }

    #[test]
    fn the_fake_grabs_each_monitor_at_its_real_size() {
        let monitors = [monitor("DP-1", 200, 100, 1.0), monitor("DP-2", 200, 100, 1.5)];
        let shots = Fake::default().grab(&monitors);
        let sizes: Vec<_> = shots.iter().map(|s| s.as_ref().map(|f| (f.width, f.height))).collect();
        if cfg!(windows) {
            assert_eq!(sizes, [Ok((200, 100)), Ok((200, 100))]);
        } else {
            assert_eq!(sizes, [Ok((200, 100)), Ok((300, 150))]);
        }
        let second = shots[1].as_ref().expect("a frame");
        let px = second.get(40, 20).expect("inside");
        assert_eq!([px.r, px.g, px.b], Fake::colour_at(1, 40, 20));
        let failing = Fake { fail: Some("no".into()) }.grab(&monitors);
        assert!(failing.iter().all(Result::is_err));
    }

    #[test]
    fn threads_keep_the_order() {
        let monitors = [monitor("a", 10, 10, 1.0), monitor("b", 20, 10, 1.0), monitor("c", 30, 10, 1.0)];
        let shots = each_on_a_thread(&monitors, |m| {
            Frame::new(m.width, 1, vec![0; m.width as usize * 4]).ok_or_else(String::new)
        });
        let widths: Vec<_> = shots.into_iter().map(|s| s.map(|f| f.width)).collect();
        assert_eq!(widths, [Ok(10), Ok(20), Ok(30)]);
    }
}
