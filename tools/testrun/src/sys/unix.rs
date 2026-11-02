// SPDX-License-Identifier: MIT
//! Linux and macOS. Linux: each test in a systemd scope of its own
//! (`systemd-run --user --scope`, cgroup v2) where the user's systemd
//! runs: its largest working set while it runs (memory.current less the
//! page cache, sampled; the scope goes when the test ends), its OOM kills,
//! and memory.max as the hard limit. Elsewhere, and
//! for the CPU time everywhere: wait4's rusage of ctest and the children it
//! waited for (the peak is then of the largest process, not of the tree).

use std::ffi::CString;
use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::{Exit, Spec, Usage};

/// A test's ctest, in its own process group (and scope).
pub struct Process {
    // Kept so that its pipes stay; never waited through std (wait4 reaps).
    _child: Child,
    pid: i32,
    unit: Option<String>,
    cgroup: Option<PathBuf>,
    peak: u64,
    /// The working set at the last sample (Linux scopes).
    current: Option<u64>,
    oom: bool,
    rusage: Option<(f64, u64)>,
    exit: Option<Exit>,
}

/// Starts the test's ctest.
pub fn spawn(spec: &Spec) -> io::Result<Process> {
    let log = File::create(&spec.log)?;
    let errors = log.try_clone()?;
    let name = format!("mitcad-testrun-{}-{}", std::process::id(), spec.seq);
    let unit = spec.cgroup.then_some(name);
    let mut command = match &unit {
        Some(unit) => {
            let mut scoped = Command::new("systemd-run");
            scoped.args(["--user", "--scope", "--quiet", "--collect"]);
            scoped.arg(format!("--unit={unit}"));
            // Beside the runner, in its slice (a job slice's or a check's
            // own, with their memory ceiling), not in the user's default.
            if let Some(slice) = own_slice() {
                scoped.arg(format!("--slice={slice}"));
            }
            if let Some(limit) = spec.limit {
                scoped.arg(format!("--property=MemoryMax={limit}"));
                scoped.arg("--property=MemorySwapMax=0");
            }
            scoped.arg("--").arg(&spec.program);
            scoped
        }
        None => Command::new(&spec.program),
    };
    command
        .args(&spec.args)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(errors)
        .process_group(0);
    let child = command.spawn()?;
    let pid = child.id() as i32;
    Ok(Process {
        _child: child,
        pid,
        unit,
        cgroup: None,
        peak: 0,
        current: None,
        oom: false,
        rusage: None,
        exit: None,
    })
}

fn seconds(time: libc::timeval) -> f64 {
    time.tv_sec as f64 + time.tv_usec as f64 / 1e6
}

/// ru_maxrss: bytes on macOS, KiB elsewhere.
fn max_rss_bytes(value: libc::c_long) -> u64 {
    let value = value.max(0) as u64;
    if cfg!(target_os = "macos") {
        value
    } else {
        value * 1024
    }
}

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

impl Process {
    /// Linux: the scope's memory while the test runs.
    fn sample_cgroup(&mut self) {
        if !cfg!(target_os = "linux") {
            return;
        }
        let Some(unit) = &self.unit else {
            return;
        };
        if self.cgroup.is_none() {
            let text = fs::read_to_string(format!("/proc/{}/cgroup", self.pid));
            let text = text.unwrap_or_default();
            if let Some(path) = super::scope_path(&text, unit) {
                let relative = path.trim_start_matches('/');
                self.cgroup = Some(Path::new("/sys/fs/cgroup").join(relative));
            }
        }
        let Some(dir) = &self.cgroup else {
            return;
        };
        // The working set (memory.current less the page cache), not
        // memory.peak: the peak counts the cache of every file the test
        // wrote or read (a cargo build's artefacts, a corpus), which the
        // kernel reclaims under the limit, and would inflate the
        // predictions (a fresh cargo test build: 16 GiB peak, mostly cache).
        if let Some(current) = read_u64(&dir.join("memory.current")) {
            let stat = fs::read_to_string(dir.join("memory.stat")).unwrap_or_default();
            let working_set = super::working_set(current, &stat);
            self.peak = self.peak.max(working_set);
            self.current = Some(working_set);
        }
        let events = fs::read_to_string(dir.join("memory.events"));
        let events = events.unwrap_or_default();
        if super::oom_kills(&events) > 0 {
            self.oom = true;
        }
    }

    /// The exit once ctest has ended; never blocks.
    pub fn poll(&mut self) -> Option<Exit> {
        if self.exit.is_some() {
            return self.exit;
        }
        self.sample_cgroup();
        let mut status: libc::c_int = 0;
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        let waited = unsafe { libc::wait4(self.pid, &mut status, libc::WNOHANG, &mut usage) };
        if waited == self.pid {
            let cpu = seconds(usage.ru_utime) + seconds(usage.ru_stime);
            self.rusage = Some((cpu, max_rss_bytes(usage.ru_maxrss)));
            self.exit = Some(super::decode_wait_status(status));
        } else if waited < 0 {
            self.exit = Some(Exit {
                success: false,
                code: None,
            });
        }
        self.exit
    }

    /// The test's working set now (its scope's, at the last poll); None
    /// without a scope or before its first sample.
    pub fn current(&self) -> Option<u64> {
        self.current
    }

    /// Kills the process group (and the scope).
    pub fn kill(&mut self) {
        if self.exit.is_some() {
            return;
        }
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
        if let Some(unit) = &self.unit {
            let _ = Command::new("systemctl")
                .args(["--user", "kill", "--signal=SIGKILL"])
                .arg(format!("{unit}.scope"))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    /// What the test used (after the exit).
    pub fn usage(&mut self) -> Usage {
        let (cpu, rss) = self.rusage.unwrap_or((0.0, 0));
        Usage {
            cpu,
            peak: self.peak.max(rss),
            over_memory: self.oom,
            tree: self.cgroup.is_some(),
        }
    }
}

/// The system's CPU load between samples.
pub struct CpuSampler {
    last: Option<(u64, u64)>,
}

#[cfg(target_os = "linux")]
fn system_times() -> Option<(u64, u64)> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    super::parse_cpu_line(text.lines().next()?)
}

#[cfg(target_os = "macos")]
fn system_times() -> Option<(u64, u64)> {
    use std::sync::OnceLock;
    unsafe extern "C" {
        fn mach_host_self() -> u32;
        fn host_statistics(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    }
    // HOST_CPU_LOAD_INFO: user, system, idle and nice ticks.
    const HOST_CPU_LOAD_INFO: i32 = 3;
    static HOST: OnceLock<u32> = OnceLock::new();
    let host = *HOST.get_or_init(|| unsafe { mach_host_self() });
    let mut ticks = [0i32; 4];
    let mut count = 4u32;
    let info = ticks.as_mut_ptr();
    let result = unsafe { host_statistics(host, HOST_CPU_LOAD_INFO, info, &mut count) };
    if result != 0 {
        return None;
    }
    let [user, system, idle, nice] = ticks.map(|t| u64::from(t as u32));
    let total = user + system + idle + nice;
    Some((total - idle, total))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn system_times() -> Option<(u64, u64)> {
    None
}

impl CpuSampler {
    pub fn start() -> CpuSampler {
        CpuSampler {
            last: system_times(),
        }
    }

    /// The load since the last sample, 0 to 1.
    pub fn sample(&mut self) -> Option<f64> {
        let now = system_times()?;
        let before = self.last.replace(now)?;
        Some(super::load_between(before, now))
    }
}

/// This process's cgroup v2 path (`/user.slice/...`).
fn own_cgroup() -> Option<String> {
    let text = fs::read_to_string("/proc/self/cgroup").ok()?;
    super::cgroup_path(&text).map(str::to_owned)
}

/// The slice of the user's systemd this process runs in (Linux), where
/// the tests' scopes go: they then stay under that slice's memory ceiling
/// (a job slice's, or a check's own slice), which a scope in the user's
/// default slice would leave.
fn own_slice() -> Option<&'static str> {
    use std::sync::OnceLock;
    static SLICE: OnceLock<Option<String>> = OnceLock::new();
    let slice = SLICE.get_or_init(|| {
        let path = own_cgroup()?;
        super::user_slice(&path).map(str::to_owned)
    });
    slice.as_deref()
}

/// The memory limits on this process's cgroup and every cgroup above it
/// (the lower of memory.max and memory.high), bytes: each as (its level, 0
/// for the process's own cgroup, the limit, the cgroup's working set:
/// memory.current less its page cache).
#[cfg(not(target_os = "macos"))]
fn cgroup_limits() -> Vec<(usize, u64, u64)> {
    let Some(path) = own_cgroup() else {
        return Vec::new();
    };
    let read = |dir: &Path, file: &str| fs::read_to_string(dir.join(file)).unwrap_or_default();
    let mut limits = Vec::new();
    for (index, level) in super::cgroup_levels(&path).into_iter().enumerate() {
        let dir = Path::new("/sys/fs/cgroup").join(level.trim_start_matches('/'));
        let max = read(&dir, "memory.max");
        let Some(limit) = super::cgroup_limit(&max, &read(&dir, "memory.high")) else {
            continue;
        };
        let current = read_u64(&dir.join("memory.current")).unwrap_or(0);
        let used = super::working_set(current, &read(&dir, "memory.stat"));
        limits.push((index, limit, used));
    }
    limits
}

/// The machine's physical memory, bytes; on Linux the lowest memory limit
/// of the cgroups this process runs in where lower (its own scope's, a
/// check's or a job slice's), so that the budget is a share of what the
/// run may use rather than of the whole machine.
#[cfg(not(target_os = "macos"))]
pub fn physical_memory() -> Option<u64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let total = super::meminfo_bytes(&text, "MemTotal")?;
    let lowest = cgroup_limits().into_iter().map(|(_, limit, _)| limit).min();
    Some(lowest.map_or(total, |limit| limit.min(total)))
}

#[cfg(target_os = "macos")]
pub fn physical_memory() -> Option<u64> {
    let mut value: u64 = 0;
    let mut size = size_of::<u64>();
    let name = c"hw.memsize";
    let out = std::ptr::from_mut(&mut value).cast::<libc::c_void>();
    let null = std::ptr::null_mut();
    let result = unsafe { libc::sysctlbyname(name.as_ptr(), out, &mut size, null, 0) };
    (result == 0).then_some(value)
}

/// Windows' commit limit has no counterpart that would raise the budget
/// here: the physical memory (on Linux the lowest cgroup limit the run is
/// under) is the basis.
pub fn commit_limit() -> Option<u64> {
    None
}

/// The physical memory available now, bytes (Linux): the machine's, or
/// less where a cgroup above this process is nearer its limit (the tests'
/// scopes are in its slice, beside its own scope, so the other jobs of a
/// shared job slice count too; its own scope's limit is the budget's).
#[cfg(not(target_os = "macos"))]
pub fn available_memory() -> Option<u64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let machine = super::meminfo_bytes(&text, "MemAvailable")?;
    #[cfg(not(target_os = "macos"))]
    let machine = cgroup_limits()
        .into_iter()
        .filter(|level| level.0 > 0)
        .map(|(_, limit, used)| limit.saturating_sub(used))
        .fold(machine, u64::min);
    Some(machine)
}

/// Native VM statistics and memory pressure, rather than Linux's procfs.
#[cfg(target_os = "macos")]
pub fn available_memory() -> Option<u64> {
    Some(super::macos::available_memory())
}

/// The free disk space of the file system the path is on, bytes.
#[allow(clippy::unnecessary_cast, clippy::useless_conversion)]
pub fn disk_free(path: &Path) -> Option<u64> {
    let name = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(name.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

/// Whether tests can run in systemd scopes of their own (Linux with the
/// user's systemd and cgroup v2).
pub fn cgroups_available() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// How tests are measured here.
pub fn method(cgroup: bool) -> &'static str {
    if cgroup {
        "a systemd scope per test (cgroup working set, memory.max, rusage CPU time)"
    } else {
        "rusage of ctest's children (CPU time; the peak of the largest process)"
    }
}
