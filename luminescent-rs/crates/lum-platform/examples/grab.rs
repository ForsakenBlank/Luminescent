//! Grabs every monitor and writes each one to a PNG in the current folder, with how long it took:
//! `cargo run -p lum-platform --example grab`.

fn main() {
    let monitors = lum_platform::desktop::monitors();
    let monitors = if monitors.is_empty() { fallback() } else { monitors };
    let started = std::time::Instant::now();
    let shots = lum_platform::capture::system().grab(&monitors);
    println!("{} monitor(s) in {:.1} ms", monitors.len(), started.elapsed().as_secs_f64() * 1000.0);
    for (m, shot) in monitors.iter().zip(shots) {
        match shot {
            Ok(frame) => {
                let name = format!("grab-{}.png", m.name);
                let saved = frame.to_image().save(&name);
                println!("{}: {}x{} -> {name} {saved:?}", m.name, frame.width, frame.height);
            }
            Err(e) => println!("{}: {e}", m.name),
        }
    }
}

/// Outside Hyprland the monitors are not listed; name them through the environment (`GRAB=HEADLESS-1:1280x720`).
fn fallback() -> Vec<lum_platform::desktop::Monitor> {
    let spec = std::env::var("GRAB").unwrap_or_default();
    spec.split(',')
        .filter_map(|one| {
            let (name, size) = one.split_once(':')?;
            let (w, h) = size.split_once('x')?;
            Some(lum_platform::desktop::Monitor {
                name: name.to_owned(),
                x: 0,
                y: 0,
                width: w.parse().ok()?,
                height: h.parse().ok()?,
                scale: 1.0,
                primary: true,
                top_inset: 0,
                bottom_inset: 0,
            })
        })
        .collect()
}
