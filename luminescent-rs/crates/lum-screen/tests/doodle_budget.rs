//! The doodle's budget (PERFORMANCE.md): drawing stays at 60 fps with 500 strokes on screen. Each frame of a
//! stroke paints its new piece and hands over only the part that changed, so the time a frame takes must not
//! grow with what is already drawn. Run with `--nocapture` to see the numbers.

use std::time::{Duration, Instant};

use lum_screen::doodle::{Canvas, Mark};
use lum_theme::colour::Rgb;

/// A wobbly stroke of `n` points starting near `seed`'s spot.
fn stroke(seed: u32, n: usize) -> Vec<(f32, f32)> {
    let mut x = (seed * 97 % 1800) as f32 + 60.0;
    let mut y = (seed * 53 % 960) as f32 + 60.0;
    (0..n)
        .map(|i| {
            x += ((i as f32 * 0.7 + seed as f32).sin() * 9.0).round();
            y += ((i as f32 * 0.4 + seed as f32).cos() * 7.0).round();
            (x, y)
        })
        .collect()
}

fn frame_times(canvas: &mut Canvas, moves: usize) -> Vec<Duration> {
    let path = stroke(9_999, moves + 1);
    canvas.begin_pen(path[0], 4.0, Rgb::new(255, 0, 0), false, true, None);
    canvas.take_dirty();
    let mut times = Vec::new();
    for p in &path[1..] {
        let started = Instant::now();
        canvas.extend(*p);
        if let Some(area) = canvas.take_dirty() {
            // what the screen uploads each frame
            std::hint::black_box(canvas.region(area));
        }
        times.push(started.elapsed());
    }
    canvas.end();
    times
}

#[test]
fn drawing_over_500_strokes_stays_within_a_60_fps_frame() {
    let mut canvas = Canvas::new(1920, 1080, 1.0);
    for seed in 0..500 {
        let colour = Rgb::new((seed * 37 % 255) as u8, 80, 200);
        canvas.add(Mark::Pen { points: stroke(seed, 60), width: 4.0, colour, erase: false, smooth: true });
    }
    canvas.take_dirty();
    assert_eq!(canvas.marks().len(), 500);

    let mut times = frame_times(&mut canvas, 240);
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    let worst = *times.last().expect("frames");
    let mean = times.iter().sum::<Duration>() / times.len() as u32;

    let undo_started = Instant::now();
    canvas.undo();
    let undo = undo_started.elapsed();
    eprintln!(
        "500 strokes, 1920 x 1080: frame mean {mean:?}, p95 {p95:?}, worst {worst:?}; undo repaint {undo:?}"
    );

    let budget = Duration::from_micros(16_667);
    assert!(p95 < budget, "p95 {p95:?} is over a 60 fps frame");
    assert!(mean < budget / 4, "mean {mean:?}: a frame's work should be a small part of the frame");
}

#[test]
fn a_frame_costs_the_same_with_or_without_500_strokes() {
    let mut empty = Canvas::new(1920, 1080, 1.0);
    let mut full = Canvas::new(1920, 1080, 1.0);
    for seed in 0..500 {
        full.add(Mark::Pen {
            points: stroke(seed, 60),
            width: 4.0,
            colour: Rgb::WHITE,
            erase: false,
            smooth: true,
        });
    }
    full.take_dirty();
    let sum = |t: Vec<Duration>| t.iter().sum::<Duration>();
    // the median of a few rounds, so a busy machine does not decide it
    let mut ratios: Vec<f64> = (0..5)
        .map(|_| {
            sum(frame_times(&mut full, 120)).as_secs_f64() / sum(frame_times(&mut empty, 120)).as_secs_f64()
        })
        .collect();
    ratios.sort_by(f64::total_cmp);
    let ratio = ratios[2];
    eprintln!("frame time with 500 strokes / without: {ratio:.2}");
    assert!(ratio < 2.0, "drawing got {ratio:.2} times slower with 500 strokes on screen");
}
