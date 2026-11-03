// SPDX-License-Identifier: MIT
//! The platform's means: starting a test in a container that measures its
//! whole process tree (a Job Object on Windows, a systemd scope's cgroup on
//! Linux, sampled descendant/group RSS on macOS), the system's CPU load, its
//! memory and free disk. The parsers of the system's text files are here,
//! tested on every platform.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_process;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

/// How to start one test.
#[derive(Clone, Debug)]
pub struct Spec {
    pub program: String,
    pub args: Vec<String>,
    /// Standard output and error go here.
    pub log: PathBuf,
    /// The hard memory limit of the process tree, bytes.
    pub limit: Option<u64>,
    /// A number of the run's own, for unique unit names.
    pub seq: u64,
    /// Linux: in a systemd scope of its own (cgroup v2).
    pub cgroup: bool,
    /// Variables set for it on top of the runner's environment (a corpus
    /// test's children, `corpus`).
    pub env: Vec<(String, String)>,
}

/// How the process ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exit {
    pub success: bool,
    pub code: Option<i32>,
}

/// What a finished test used.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    /// CPU time of the process tree (user and system), seconds.
    pub cpu: f64,
    /// Peak memory, bytes: of the whole tree with a Job Object or a
    /// cgroup; sampled group/descendant RSS on macOS, else the largest process.
    pub peak: u64,
    /// The platform says it went over its memory limit.
    pub over_memory: bool,
    /// The peak includes the process tree (sampled on macOS, not a hard limit).
    pub tree: bool,
}

/// A wait status (waitpid) as an exit: the code of a normal exit, none
/// for a signal.
pub fn decode_wait_status(status: i32) -> Exit {
    if status & 0x7f == 0 {
        let code = (status >> 8) & 0xff;
        Exit {
            success: code == 0,
            code: Some(code),
        }
    } else {
        Exit {
            success: false,
            code: None,
        }
    }
}

/// The busy and total ticks of the first line of /proc/stat
/// (`cpu  user nice system idle iowait irq softirq steal ...`); idle and
/// iowait are not busy.
pub fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values: Vec<u64> = fields.filter_map(|f| f.parse().ok()).collect();
    if values.len() < 4 {
        return None;
    }
    // guest and guest_nice are already in user and nice.
    let total: u64 = values.iter().take(8).sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((total.saturating_sub(idle), total))
}

/// The load between two (busy, total) samples, 0 to 1.
pub fn load_between(before: (u64, u64), after: (u64, u64)) -> f64 {
    let total = after.1.saturating_sub(before.1);
    if total == 0 {
        return 0.0;
    }
    let busy = after.0.saturating_sub(before.0);
    (busy as f64 / total as f64).clamp(0.0, 1.0)
}

/// A value of /proc/meminfo (`MemTotal:  32768 kB`) in bytes.
pub fn meminfo_bytes(text: &str, key: &str) -> Option<u64> {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
        return Some(kib * 1024);
    }
    None
}

/// The cgroup v2 path in /proc/<pid>/cgroup (`0::/user.slice/...`).
pub fn cgroup_path(text: &str) -> Option<&str> {
    text.lines().find_map(|line| line.strip_prefix("0::"))
}

/// The cgroup path of a process once it is in the scope `<unit>.scope`.
pub fn scope_path<'a>(text: &'a str, unit: &str) -> Option<&'a str> {
    let path = cgroup_path(text)?;
    let last = path.rsplit('/').next()?;
    (last.strip_suffix(".scope") == Some(unit)).then_some(path)
}

/// The count of OOM kills in a cgroup's memory.events.
pub fn oom_kills(text: &str) -> u64 {
    let value = text.lines().find_map(|line| line.strip_prefix("oom_kill "));
    value.and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

/// A cgroup's memory.max: bytes, or None for "max".
pub fn parse_memory_max(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// A cgroup v2 path and its ancestors, the path itself first, without the
/// root (`/a.slice/b.scope` gives `/a.slice/b.scope` and `/a.slice`).
pub fn cgroup_levels(path: &str) -> Vec<&str> {
    let mut levels = Vec::new();
    let mut path = path.trim_end_matches('/');
    while !path.is_empty() {
        levels.push(path);
        path = path.rfind('/').map_or("", |i| &path[..i]);
    }
    levels
}

/// The slice of the user's systemd that a cgroup path is in: the last
/// `*.slice` below `user@<uid>.service`, e.g. `mitcad-jobs.slice` of
/// `/user.slice/user-1000.slice/user@1000.service/mitcad.slice/
/// mitcad-jobs.slice/job.scope`. None outside the user's systemd (a login
/// session's scope of the system's), where `systemd-run --user` picks its
/// own default.
pub fn user_slice(path: &str) -> Option<&str> {
    let start = path.find("/user@")?;
    let rest = &path[start + 1..];
    let rest = &rest[rest.find(".service/")? + ".service/".len()..];
    rest.split('/').rfind(|c| c.ends_with(".slice"))
}

/// The memory a cgroup may use: the lower of memory.max and memory.high
/// (above high the kernel throttles), None when neither is set.
pub fn cgroup_limit(max: &str, high: &str) -> Option<u64> {
    let limits = [parse_memory_max(max), parse_memory_max(high)];
    limits.into_iter().flatten().min()
}

/// A cgroup's memory in use that cannot simply be dropped: memory.current
/// less its page cache (memory.stat's active_file and inactive_file;
/// shared memory is not on those lists, so it stays counted).
pub fn working_set(current: u64, stat: &str) -> u64 {
    let field = |key: &str| {
        let line = stat.lines().find_map(|l| l.strip_prefix(key));
        line.and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0)
    };
    let cache = field("active_file ").saturating_add(field("inactive_file "));
    current.saturating_sub(cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_statuses_decode() {
        assert_eq!(decode_wait_status(0).code, Some(0));
        assert!(decode_wait_status(0).success);
        let failed = decode_wait_status(8 << 8);
        assert_eq!(failed.code, Some(8));
        assert!(!failed.success);
        // Killed by SIGKILL.
        assert_eq!(decode_wait_status(9).code, None);
    }

    #[test]
    fn cpu_lines_parse() {
        let line = "cpu  100 0 50 800 50 0 0 0 0 0";
        assert_eq!(parse_cpu_line(line), Some((150, 1000)));
        assert_eq!(parse_cpu_line("cpu0 1 2 3 4"), None);
        assert_eq!(load_between((150, 1000), (650, 2000)), 0.5);
        assert_eq!(load_between((1, 1), (1, 1)), 0.0);
    }

    #[test]
    fn meminfo_parses() {
        let text = "MemTotal:       32768 kB\nMemFree:  1 kB\nMemAvailable:   16384 kB\n";
        assert_eq!(meminfo_bytes(text, "MemTotal"), Some(32768 * 1024));
        assert_eq!(meminfo_bytes(text, "MemAvailable"), Some(16384 * 1024));
        assert_eq!(meminfo_bytes(text, "Mem"), None);
    }

    #[test]
    fn cgroups_parse() {
        let text = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/t-1-2.scope\n";
        let path = scope_path(text, "t-1-2").unwrap();
        assert!(path.ends_with("app.slice/t-1-2.scope"));
        assert_eq!(scope_path(text, "t-1-3"), None);
        assert_eq!(scope_path("0::/user.slice/x.service", "t-1-2"), None);
        assert_eq!(cgroup_path("1:name=systemd:/a\n0::/b\n"), Some("/b"));
        let events = "low 0\nhigh 0\nmax 3\noom 1\noom_kill 1\noom_group_kill 0\n";
        assert_eq!(oom_kills(events), 1);
        assert_eq!(oom_kills(""), 0);
        assert_eq!(parse_memory_max("max\n"), None);
        assert_eq!(parse_memory_max("12884901888\n"), Some(12884901888));
    }

    #[test]
    fn cgroup_paths_walk_up() {
        let path =
            "/user.slice/user-1000.slice/user@1000.service/mitcad.slice/mitcad-jobs.slice/j.scope";
        let levels = cgroup_levels(path);
        assert_eq!(levels.len(), 6);
        assert_eq!(levels[0], path);
        assert_eq!(levels[5], "/user.slice");
        assert!(cgroup_levels("/").is_empty());
        assert_eq!(user_slice(path), Some("mitcad-jobs.slice"));
        let app = "/user.slice/user-1000.slice/user@1000.service/app.slice/app-x.scope";
        assert_eq!(user_slice(app), Some("app.slice"));
        let check = "/user.slice/user-1000.slice/user@1000.service/m.slice/m-j.slice/m-j-check7.slice/run-p1.scope";
        assert_eq!(user_slice(check), Some("m-j-check7.slice"));
        // A login session of the system's systemd, or the user's manager itself.
        assert_eq!(
            user_slice("/user.slice/user-1000.slice/session-4.scope"),
            None
        );
        let manager = "/user.slice/user-1000.slice/user@1000.service/init.scope";
        assert_eq!(user_slice(manager), None);
    }

    #[test]
    fn cgroup_limits_and_working_sets() {
        assert_eq!(cgroup_limit("max\n", "max\n"), None);
        assert_eq!(
            cgroup_limit("38654705664\n", "35433480192\n"),
            Some(35433480192)
        );
        assert_eq!(cgroup_limit("1000\n", "max\n"), Some(1000));
        assert_eq!(cgroup_limit("", "2000"), Some(2000));
        let stat = "anon 4000\nfile 3000\nactive_file 2000\ninactive_file 500\nshmem 500\n";
        assert_eq!(working_set(8000, stat), 5500);
        assert_eq!(working_set(100, stat), 0);
        assert_eq!(working_set(100, ""), 100);
    }
}
