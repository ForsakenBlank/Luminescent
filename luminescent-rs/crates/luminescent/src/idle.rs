//! `--idle-check <secs>`: start as usual, let the start-up work settle, then measure the process for a while
//! with nothing happening, the way `tools/measure_rust.sh` does from outside. It fails when the app used CPU,
//! woke up about once a second or more, painted anything, or grew past the memory budget.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lum_platform::process;

use crate::launcher::{AppMsg, Mailbox, Passes};

/// The start-up work (the "started" event at 1.5 s, the overlay's prewarm at 3 s, and the open and close of
/// the overlay below) is done by then.
const SETTLE: Duration = Duration::from_secs(5);

/// Over this the process counts as busy; 0.1 % of a core is what one wake-up a second with a little work
/// costs, which is already more than the budget.
const CPU_BUDGET: f64 = 0.1;
const WAKEUP_BUDGET: f64 = 1.0;
const RSS_TARGET_MB: f64 = 60.0;
const RSS_LIMIT_MB: f64 = 100.0;

#[derive(Debug, Clone)]
pub struct Numbers {
    pub secs: f64,
    pub cpu_percent: f64,
    pub wakeups_per_sec: Option<f64>,
    pub rss_mb: f64,
    pub private_mb: Option<f64>,
    pub threads: Option<u32>,
    pub logic_passes: u64,
    pub ui_passes: u64,
    /// The CPU draws the pixels (llvmpipe in CI and cloud machines). Its driver adds 50 MB or more of
    /// libraries to the RSS, so memory over the limit is reported but does not fail the check.
    pub software_renderer: bool,
    /// With windows, the overlay is opened and hidden again before measuring: its first frame took this long.
    pub overlay_ms: Option<f64>,
}

impl Numbers {
    pub fn ok(&self) -> bool {
        self.cpu_percent < CPU_BUDGET
            && self.wakeups_per_sec.is_none_or(|w| w < WAKEUP_BUDGET)
            && (self.rss_mb <= RSS_LIMIT_MB || self.software_renderer)
            && self.ui_passes == 0
            && (self.logic_passes as f64) < self.secs * WAKEUP_BUDGET
    }

    /// What `--idle-check` prints: a readable block and a table row for SPIKE_RESULTS.md / BASELINE.md.
    pub fn report(&self, mode: &str) -> String {
        let wakeups = self.wakeups_per_sec.map_or("n/a".to_owned(), |w| format!("{w:.1}"));
        let threads = self.threads.map_or("n/a".to_owned(), |t| t.to_string());
        let private = self.private_mb.map_or(String::new(), |p| format!(", {p:.0} MB of it private"));
        let software = if self.software_renderer && self.rss_mb > RSS_LIMIT_MB {
            " (over, but a software renderer's libraries are in it, so it does not count)"
        } else {
            ""
        };
        let verdict = if self.ok() { "OK" } else { "OVER BUDGET" };
        let overlay = self.overlay_ms.map_or(String::new(), |ms| {
            format!("\n  overlay      opened and hidden before measuring (first frame after {ms:.1} ms)")
        });
        let desktop =
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| std::env::consts::OS.to_owned());
        format!(
            "IDLE CHECK {secs:.0} s, {mode}\n\
             \x20 CPU          {cpu:.2} % of one core (budget 0.0 %, fails at {CPU_BUDGET} %)\n\
             \x20 wake-ups     {wakeups} a second (budget under {WAKEUP_BUDGET:.0})\n\
             \x20 memory       {rss:.0} MB RSS{private} (target {RSS_TARGET_MB:.0} MB, limit {RSS_LIMIT_MB:.0} MB){software}\n\
             \x20 threads      {threads}\n\
             \x20 UI passes    {logic} woken, {ui} painted (budget 0 painted){overlay}\n\
             \x20 result       {verdict}\n\
             | {desktop} | {mode} | {cpu:.2} % | {wakeups} | {rss:.0} MB | {threads} | {logic} / {ui} | {verdict} |",
            secs = self.secs,
            cpu = self.cpu_percent,
            rss = self.rss_mb,
            logic = self.logic_passes,
            ui = self.ui_passes,
        )
    }
}

pub async fn check(length: Duration, passes: Arc<Passes>, mailbox: Mailbox) {
    // the overlay opens and goes away again first, so the measuring also shows it leaves nothing running
    tokio::time::sleep(Duration::from_secs(1)).await;
    mailbox.send(AppMsg::IdleOverlay(true));
    tokio::time::sleep(Duration::from_secs(1)).await;
    mailbox.send(AppMsg::IdleOverlay(false));
    tokio::time::sleep(SETTLE - Duration::from_secs(2)).await;
    let (before, passes_before, started) = (process::sample(), passes.read(), Instant::now());
    tokio::time::sleep(length).await;
    let (after, passes_after, secs) = (process::sample(), passes.read(), started.elapsed().as_secs_f64());
    let cpu = after.cpu.saturating_sub(before.cpu).as_secs_f64();
    let numbers = Numbers {
        secs,
        cpu_percent: cpu / secs * 100.0,
        wakeups_per_sec: match (before.context_switches, after.context_switches) {
            (Some(a), Some(b)) => Some(b.saturating_sub(a) as f64 / secs),
            _ => None,
        },
        rss_mb: after.rss_bytes as f64 / 1_048_576.0,
        private_mb: after.private_bytes.map(|b| b as f64 / 1_048_576.0),
        threads: after.threads,
        logic_passes: passes_after.0 - passes_before.0,
        ui_passes: passes_after.1 - passes_before.1,
        software_renderer: false,
        overlay_ms: None,
    };
    mailbox.send(AppMsg::IdleReport(numbers));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> Numbers {
        Numbers {
            secs: 20.0,
            cpu_percent: 0.0,
            wakeups_per_sec: Some(0.2),
            rss_mb: 30.0,
            private_mb: Some(20.0),
            threads: Some(5),
            logic_passes: 0,
            ui_passes: 0,
            software_renderer: false,
            overlay_ms: None,
        }
    }

    #[test]
    fn the_budget() {
        assert!(quiet().ok());
        assert!(Numbers { wakeups_per_sec: None, ..quiet() }.ok(), "Windows has no wake-up count");
        assert!(!Numbers { cpu_percent: 0.3, ..quiet() }.ok());
        assert!(!Numbers { wakeups_per_sec: Some(1.2), ..quiet() }.ok());
        assert!(!Numbers { ui_passes: 1, ..quiet() }.ok(), "nothing may paint while nothing changes");
        assert!(!Numbers { rss_mb: 120.0, ..quiet() }.ok());
        assert!(Numbers { rss_mb: 120.0, software_renderer: true, ..quiet() }.ok(), "llvmpipe's libraries");
        assert!(quiet().report("no windows").contains("result       OK"));
    }
}
