//! Frame timing for the spikes: how long our ui code took, how long eframe says the whole frame took on the
//! CPU, and the time between frames (which includes waiting for the screen).

use std::time::Instant;

#[derive(Debug, Default)]
pub struct FrameStats {
    last: Option<Instant>,
    started: Option<Instant>,
    /// Milliseconds between the starts of two measured frames.
    intervals: Vec<f32>,
    /// Milliseconds our ui code took.
    build: Vec<f32>,
    /// Milliseconds eframe reports for the previous frame (ui, tessellation and handing it to the GPU).
    cpu: Vec<f32>,
}

impl FrameStats {
    /// Call at the start of a frame that counts. `cpu_usage` is `frame.info().cpu_usage`.
    pub fn begin(&mut self, cpu_usage: Option<f32>) {
        let now = Instant::now();
        if let Some(last) = self.last {
            self.intervals.push(ms(now - last));
            if let Some(cpu) = cpu_usage {
                self.cpu.push(cpu * 1000.0);
            }
        }
        self.last = Some(now);
        self.started = Some(now);
    }

    pub fn end(&mut self) {
        if let Some(started) = self.started.take() {
            self.build.push(ms(started.elapsed()));
        }
    }

    /// A piece of work timed by the caller (handling one key in the note).
    pub fn record_build(&mut self, took: std::time::Duration) {
        self.build.push(ms(took));
    }

    /// Lines for work timed with `record_build`.
    pub fn summary_build(&self) -> Vec<String> {
        if self.build.is_empty() {
            return vec!["  nothing measured".to_owned()];
        }
        let build = sorted(&self.build);
        let slow = build.iter().filter(|&&b| b > 16.7).count();
        vec![format!(
            "  per key      median {:.2} ms, p95 {:.2} ms, worst {:.2} ms; {slow} over one 60 Hz frame (16.7 ms)",
            pct(&build, 0.5),
            pct(&build, 0.95),
            build[build.len() - 1]
        )]
    }

    /// `median | p95 | worst` of the work timed with `record_build`.
    pub fn row_build(&self) -> String {
        let build = sorted(&self.build);
        if build.is_empty() {
            return "nothing measured | |".to_owned();
        }
        format!("{:.2} ms | {:.2} ms | {:.2} ms", pct(&build, 0.5), pct(&build, 0.95), build[build.len() - 1])
    }

    /// The next measured frame is not timed against the one before (after a pause).
    pub fn pause(&mut self) {
        self.last = None;
    }

    pub fn frames(&self) -> usize {
        self.build.len()
    }

    pub fn is_empty(&self) -> bool {
        self.build.is_empty()
    }

    /// Lines for the report.
    pub fn summary(&self) -> Vec<String> {
        if self.intervals.is_empty() {
            return vec!["  no frames measured".to_owned()];
        }
        let interval = sorted(&self.intervals);
        let build = sorted(&self.build);
        let cpu = sorted(&self.cpu);
        let typical = pct(&interval, 0.5);
        // a frame counts as dropped when it took clearly longer than the usual one (one refresh)
        let dropped = interval.iter().filter(|&&i| i > typical * 1.5).count();
        let mut lines = vec![
            format!(
                "  frames       {} measured, {:.0} fps typical ({:.1} ms between frames)",
                self.build.len(),
                1000.0 / typical.max(0.001),
                typical
            ),
            format!(
                "  between      p95 {:.1} ms, p99 {:.1} ms, worst {:.1} ms; {dropped} frames over 1.5× the typical one",
                pct(&interval, 0.95),
                pct(&interval, 0.99),
                interval[interval.len() - 1]
            ),
            format!(
                "  ui code      median {:.2} ms, p95 {:.2} ms, worst {:.2} ms",
                pct(&build, 0.5),
                pct(&build, 0.95),
                build[build.len() - 1]
            ),
        ];
        if !cpu.is_empty() {
            lines.push(format!(
                "  whole frame  median {:.2} ms, p95 {:.2} ms, worst {:.2} ms (CPU side, from eframe)",
                pct(&cpu, 0.5),
                pct(&cpu, 0.95),
                cpu[cpu.len() - 1]
            ));
        }
        lines
    }

    /// `fps | p95 between | worst | ui p95 | frame p95` for the results table.
    pub fn row(&self) -> String {
        if self.intervals.is_empty() {
            return "no frames | | | |".to_owned();
        }
        let interval = sorted(&self.intervals);
        let build = sorted(&self.build);
        let cpu = sorted(&self.cpu);
        format!(
            "{:.0} fps | {:.1} ms | {:.1} ms | {:.2} ms | {}",
            1000.0 / pct(&interval, 0.5).max(0.001),
            pct(&interval, 0.95),
            interval[interval.len() - 1],
            pct(&build, 0.95),
            if cpu.is_empty() { "n/a".to_owned() } else { format!("{:.2} ms", pct(&cpu, 0.95)) }
        )
    }
}

fn ms(d: std::time::Duration) -> f32 {
    d.as_secs_f32() * 1000.0
}

fn sorted(values: &[f32]) -> Vec<f32> {
    let mut v = values.to_vec();
    v.sort_by(f32::total_cmp);
    v
}

fn pct(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        let v: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        assert_eq!(pct(&v, 0.5), 51.0);
        assert_eq!(pct(&v, 0.95), 95.0);
        assert_eq!(pct(&v, 1.0), 100.0);
        assert_eq!(pct(&[], 0.5), 0.0);
    }
}
