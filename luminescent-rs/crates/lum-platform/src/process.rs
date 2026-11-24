//! How much this process costs: CPU time, memory, threads and wake-ups. `--idle-check` takes two samples and
//! compares them, the way `tools/measure_rust.sh` reads `/proc` from outside.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// User plus system time of every thread so far.
    pub cpu: Duration,
    pub rss_bytes: u64,
    /// The part of the RSS that is this process's alone (heap, stacks, GPU buffers), without the shared
    /// library pages. Linux only.
    pub private_bytes: Option<u64>,
    pub threads: Option<u32>,
    /// Voluntary plus involuntary context switches of every thread: a fair count of how often the process
    /// woke up. Windows does not keep a per-process count, so it is `None` there.
    pub context_switches: Option<u64>,
}

#[cfg(target_os = "linux")]
pub fn sample() -> Sample {
    // SAFETY: getrusage only writes into the struct we hand it.
    let usage = unsafe {
        let mut usage = std::mem::zeroed::<libc::rusage>();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            usage = std::mem::zeroed();
        }
        usage
    };
    let time = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| -> Option<u64> {
        let line = status.lines().find(|l| l.starts_with(name))?;
        line[name.len()..].split_whitespace().next()?.parse().ok()
    };
    Sample {
        cpu: time(usage.ru_utime) + time(usage.ru_stime),
        rss_bytes: field("VmRSS:").unwrap_or(0) * 1024,
        private_bytes: field("RssAnon:").map(|kb| kb * 1024),
        threads: field("Threads:").and_then(|n| u32::try_from(n).ok()),
        context_switches: Some((usage.ru_nvcsw + usage.ru_nivcsw) as u64),
    }
}

#[cfg(windows)]
pub fn sample() -> Sample {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the pseudo handle from GetCurrentProcess needs no closing, and both calls only write into the
    // structs passed to them, whose sizes are given.
    let (cpu, rss) = unsafe {
        let me = GetCurrentProcess();
        let cpu = if GetProcessTimes(me, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
            Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
        } else {
            Duration::ZERO
        };
        let mut memory = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let rss = if GetProcessMemoryInfo(me, &mut memory, memory.cb) != 0 {
            memory.WorkingSetSize as u64
        } else {
            0
        };
        (cpu, rss)
    };
    Sample { cpu, rss_bytes: rss, private_bytes: None, threads: None, context_switches: None }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn sample() -> Sample {
    Sample { cpu: Duration::ZERO, rss_bytes: 0, private_bytes: None, threads: None, context_switches: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_loop_shows_up_as_cpu_time() {
        let before = sample();
        let start = std::time::Instant::now();
        let mut x = 0u64;
        while start.elapsed() < Duration::from_millis(50) {
            x = std::hint::black_box(x.wrapping_add(1));
        }
        let after = sample();
        if cfg!(any(target_os = "linux", windows)) {
            assert!(after.cpu > before.cpu);
            assert!(after.rss_bytes > 0);
        }
    }
}
