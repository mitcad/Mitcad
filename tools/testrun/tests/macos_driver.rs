// SPDX-License-Identifier: MIT
//! Exercise the actual macOS shell entry point without opening windows or
//! starting tests: its runner prints the arguments for the real parser.
#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use mitcad_testrun::options::{self, Budget, Mode, Options};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mitcad-macos-driver-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("core")).unwrap();
        let runner = root.join("core/mitcad-testrun");
        fs::write(&runner, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }

    fn options(&self, extra: &[&str]) -> Options {
        let driver = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-macos.sh");
        let output = Command::new("/bin/bash")
            .arg(driver)
            .arg("--build")
            .arg(&self.0)
            .args(extra)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        let args = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        options::parse(&args).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn schedule_uses_native_headroom_and_the_history_based_ceiling() {
    let options = Fixture::new().options(&[]);
    assert_eq!(options.mode, Mode::Schedule);
    assert_eq!(options.memory_budget, Budget::Free);
    assert_eq!(options.jobs, None);
    assert_eq!(options.cpu_gate, 0.9);
    assert_eq!(options.sensitive, ["app.ui-*"]);
    assert_eq!(options.sensitive_max, 1);
    assert_eq!(options.sensitive_load, 0.7);
    assert_eq!(options.ui_max, None);
    assert_eq!(options.limit_factor, 0.0);
}

#[test]
fn measure_remains_single_test_and_explicit_overrides_are_kept() {
    let fixture = Fixture::new();
    let measure = fixture.options(&["--measure"]);
    assert_eq!(measure.mode, Mode::Measure);
    assert_eq!(measure.jobs, Some(1));
    assert_eq!(measure.memory_budget, Budget::Share(0.85));

    let explicit = fixture.options(&[
        "-j",
        "6",
        "--memory-budget",
        "12G",
        "--memory-reserve",
        "6G",
        "-R",
        "^core\\.",
    ]);
    assert_eq!(explicit.jobs, Some(6));
    assert_eq!(explicit.memory_budget, Budget::Bytes(12 << 30));
    assert_eq!(explicit.memory_reserve, Some(6 << 30));
    assert_eq!(explicit.select, ["-R", "^core\\."]);
}
