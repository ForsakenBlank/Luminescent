//! CPU load, memory and battery in percent, read the cheap way (`/proc`, `/sys` or one Windows call). Each
//! reading is `None` when this computer has none or it can not be read.

/// CPU load between two readings: the first one only learns where the counters are.
#[derive(Debug, Default)]
pub struct CpuMeter {
    last: Option<(u64, u64)>,
}

impl CpuMeter {
    pub fn new() -> CpuMeter {
        CpuMeter::default()
    }

    pub fn percent(&mut self) -> Option<f64> {
        let (idle, total) = imp::cpu_times()?;
        let last = self.last.replace((idle, total))?;
        let (d_idle, d_total) = (idle.saturating_sub(last.0), total.saturating_sub(last.1));
        if d_total == 0 {
            return None;
        }
        Some(100.0 * (1.0 - d_idle as f64 / d_total as f64))
    }
}

/// Memory in use, in percent.
pub fn ram_percent() -> Option<f64> {
    imp::ram_percent()
}

/// The battery's charge, in percent.
pub fn battery_percent() -> Option<f64> {
    imp::battery_percent()
}

#[cfg(target_os = "linux")]
mod imp {
    /// Idle and total ticks of every CPU together (`/proc/stat`'s first line; iowait counts as idle).
    pub fn cpu_times() -> Option<(u64, u64)> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        parse_stat(stat.lines().next()?)
    }

    pub fn parse_stat(line: &str) -> Option<(u64, u64)> {
        let parts: Vec<u64> = line.split_whitespace().skip(1).map_while(|p| p.parse().ok()).collect();
        let idle = parts.get(3)? + parts.get(4).copied().unwrap_or(0);
        Some((idle, parts.iter().sum()))
    }

    pub fn ram_percent() -> Option<f64> {
        parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
    }

    pub fn parse_meminfo(text: &str) -> Option<f64> {
        let field = |name: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
                .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
        };
        let (available, total) = (field("MemAvailable")?, field("MemTotal")?);
        (total > 0.0).then(|| 100.0 * (1.0 - available / total))
    }

    pub fn battery_percent() -> Option<f64> {
        let mut names: Vec<_> = std::fs::read_dir("/sys/class/power_supply")
            .ok()?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("BAT"))
            .collect();
        names.sort();
        let first = names.first()?;
        let text = std::fs::read_to_string(format!("/sys/class/power_supply/{first}/capacity")).ok()?;
        text.trim().parse().ok()
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::GetSystemTimes;

    fn ticks(t: FILETIME) -> u64 {
        (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
    }

    pub fn cpu_times() -> Option<(u64, u64)> {
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut idle, mut kernel, mut user) = (zero, zero, zero);
        // SAFETY: three FILETIMEs the call fills in.
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        // kernel time includes the idle time
        Some((ticks(idle), ticks(kernel) + ticks(user)))
    }

    pub fn ram_percent() -> Option<f64> {
        // SAFETY: the struct is zeroed and sized as the call wants.
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: as above; the call only writes into it.
        (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(f64::from(status.dwMemoryLoad))
    }

    pub fn battery_percent() -> Option<f64> {
        // SAFETY: the struct is zeroed and the call only writes into it.
        let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        // SAFETY: as above.
        if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
            return None;
        }
        // 255: unknown; flag 128: there is no battery
        (status.BatteryLifePercent != 255 && status.BatteryFlag & 128 == 0)
            .then_some(f64::from(status.BatteryLifePercent))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub fn cpu_times() -> Option<(u64, u64)> {
        None
    }

    pub fn ram_percent() -> Option<f64> {
        None
    }

    pub fn battery_percent() -> Option<f64> {
        None
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::imp::{parse_meminfo, parse_stat};

    #[test]
    fn reads_proc_like_python() {
        assert_eq!(parse_stat("cpu  10 0 5 80 5 0 0 0 0 0"), Some((85, 100)));
        let mem = "MemTotal:       16000000 kB\nMemFree:  1 kB\nMemAvailable:    4000000 kB\n";
        assert_eq!(parse_meminfo(mem), Some(75.0));
        assert_eq!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB"), None);
        let mut meter = super::CpuMeter::new();
        assert_eq!(meter.percent(), None, "the first reading only learns the counters");
    }
}
