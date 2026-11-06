// SPDX-License-Identifier: MIT
//! Native integration checks of the real CMake registrations, Cargo job
//! cap, and runner overlap. Only synthetic commands, no GUI or corpora.
#![cfg(target_os = "macos")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mitcad-scheduling-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn source(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
    fn run(&self, command: &mut Command) -> Output {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "command {command:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn real_registration_keeps_native_coverage_and_exclusion_with_harnesses_on_or_off() {
    let f = Fixture::new();
    fs::write(
        f.0.join("CMakeLists.txt"),
        r#"
cmake_minimum_required(VERSION 3.25)
project(MacScheduling NONE)
enable_testing()
set(CMAKE_SOURCE_DIR "${REPO}")
set(BUILD_TESTING ON)
set(MITCAD_MACOS_NATIVE_UI ON)
foreach(target mitcad mitcad-cli mitcad-render)
  add_executable(${target} IMPORTED GLOBAL)
  set_target_properties(${target} PROPERTIES IMPORTED_LOCATION /usr/bin/true)
endforeach()
foreach(test app.unit app.render_devices)
  add_test(NAME ${test} COMMAND "${CMAKE_COMMAND}" -E true)
  set_tests_properties(${test} PROPERTIES ENVIRONMENT QT_QPA_PLATFORM=offscreen)
endforeach()
include("${REPO}/cmake/MacTestScheduling.cmake")
foreach(test core.cargo_build core.cargo_build_release)
  add_test(NAME ${test} COMMAND "${CMAKE_COMMAND}" -E true)
  mitcad_macos_cargo_build(${test})
endforeach()
include("${REPO}/cmake/MacUiTests.cmake")
"#,
    )
    .unwrap();
    let mut inventories = Vec::new();
    for (setting, headless) in [("ON", "ON"), ("OFF", "ON"), ("ON", "OFF"), ("OFF", "OFF")] {
        let build = f.0.join(format!("{setting}-{headless}"));
        f.run(
            Command::new("cmake")
                .arg("-S")
                .arg(&f.0)
                .arg("-B")
                .arg(&build)
                .arg(format!("-DREPO={}", f.source().display()))
                .arg(format!("-DMITCAD_MACOS_PARALLEL_HARNESSES={setting}"))
                .arg(format!("-DMITCAD_MACOS_HEADLESS_TESTS={headless}")),
        );
        f.run(
            Command::new("python3")
                .arg(f.source().join("tools/test-macos-test-resources.py"))
                .args(["--ctest", "ctest", "--build"])
                .arg(&build),
        );
        let output = f.run(
            Command::new("ctest")
                .arg("--test-dir")
                .arg(&build)
                .arg("--show-only=json-v1"),
        );
        // Each configuration has its own output tree. Compare the actual
        // commands after normalizing only that legitimate path change.
        let text = String::from_utf8(output.stdout)
            .unwrap()
            .replace(build.to_str().unwrap(), "BUILD");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let tests = json["tests"].as_array().unwrap();
        let harnesses = tests
            .iter()
            .filter(|t| {
                t["properties"].as_array().unwrap().iter().any(|p| {
                    p["name"] == "LABELS"
                        && p["value"]
                            .as_array()
                            .is_some_and(|v| v.iter().any(|l| l == "macos_gui_harness"))
                })
            })
            .count();
        assert_eq!(
            harnesses,
            if setting == "ON" {
                if headless == "ON" { 9 } else { 10 }
            } else {
                0
            }
        );
        let offscreen = tests
            .iter()
            .filter(|t| {
                t["properties"].as_array().unwrap().iter().any(|p| {
                    p["name"] == "LABELS"
                        && p["value"]
                            .as_array()
                            .is_some_and(|v| v.iter().any(|l| l == "macos_headless"))
                })
            })
            .count();
        assert_eq!(offscreen, if headless == "ON" { 7 } else { 0 });
        inventories.push(
            tests
                .iter()
                .map(|t| (t["name"].clone(), t["command"].clone()))
                .collect::<Vec<_>>(),
        );
    }
    for inventory in &inventories[1..] {
        assert_eq!(
            &inventories[0], inventory,
            "parallelism changed test coverage or commands"
        );
    }
}

#[test]
fn cargo_cap_applies_to_the_real_cmake_driver_and_keeps_a_lower_caller_limit() {
    let f = Fixture::new();
    let cargo = f.0.join("cargo");
    fs::write(
        &cargo,
        "#!/bin/sh\nprintf '%s' \"$CARGO_BUILD_JOBS\" > \"$JOB_RECORD\"\n",
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o700)).unwrap();
    for (caller, expected) in [(Some("16"), "2"), (Some("1"), "1"), (None, "2")] {
        let record = f.0.join("jobs");
        let mut command = Command::new("cmake");
        command
            .arg(format!("-DCARGO={}", cargo.display()))
            .arg(format!(
                "-DMANIFEST={}",
                f.source().join("core/Cargo.toml").display()
            ))
            .arg(format!("-DLIST={}", f.0.join("tests.tsv").display()))
            .arg("-P")
            .arg(f.source().join("core/cargo-tests.cmake"))
            .args(["--", "build", "synthetic"])
            .env("MITCAD_CARGO_MAX_JOBS", "2")
            .env("MITCAD_CARGO_JOB_MIB", "1")
            .env("MITCAD_CARGO_SWEEP", "0")
            .env("JOB_RECORD", &record);
        if let Some(caller) = caller {
            command.env("CARGO_BUILD_JOBS", caller);
        } else {
            command.env_remove("CARGO_BUILD_JOBS");
        }
        f.run(&mut command);
        assert_eq!(fs::read_to_string(record).unwrap(), expected);
    }
}

#[test]
fn compiler_fixture_can_overlap_independent_work_without_retries() {
    let f = Fixture::new();
    let script = f.0.join("work.py");
    fs::write(
        &script,
        r#"
import sys, time
from pathlib import Path
root = Path(sys.argv[1])
kind = sys.argv[2]
(root / kind).touch()
deadline = time.monotonic() + 10
other = 'headless' if kind == 'compiler' else 'compiler'
while not (root / other).exists():
    assert time.monotonic() < deadline, 'the independent work was serialized'
    time.sleep(0.01)
"#,
    )
    .unwrap();
    fs::write(f.0.join("CMakeLists.txt"), r#"
cmake_minimum_required(VERSION 3.25)
project(Overlap NONE)
enable_testing()
add_test(NAME core.cargo_build COMMAND python3 "${CMAKE_SOURCE_DIR}/work.py" "${CMAKE_SOURCE_DIR}" compiler)
set_tests_properties(core.cargo_build PROPERTIES LABELS macos_cargo_build RESOURCE_LOCK cargo_build TIMEOUT 15)
add_test(NAME headless COMMAND python3 "${CMAKE_SOURCE_DIR}/work.py" "${CMAKE_SOURCE_DIR}" headless)
set_tests_properties(headless PROPERTIES TIMEOUT 15)
"#).unwrap();
    let build = f.0.join("build");
    f.run(
        Command::new("cmake")
            .arg("-S")
            .arg(&f.0)
            .arg("-B")
            .arg(&build),
    );
    let out = f.0.join("results");
    f.run(
        Command::new(env!("CARGO_BIN_EXE_mitcad-testrun"))
            .arg("--build")
            .arg(build)
            .arg("--schedule")
            .arg("--out")
            .arg(&out)
            .arg("--profiles")
            .arg(f.0.join("profiles.json"))
            .args([
                "--no-record",
                "--memory-budget",
                "24G",
                "--memory-floor",
                "0",
                "--cpu-gate",
                "1",
                "--limit-factor",
                "0",
            ]),
    );
    let rows = fs::read_to_string(out.join("results.tsv")).unwrap();
    assert_eq!(rows.lines().count(), 3);
    assert!(!rows.contains("rerun"));
}

#[test]
fn only_two_isolated_gui_harnesses_are_admitted_without_contention_retries() {
    let f = Fixture::new();
    fs::write(
        f.0.join("work.py"),
        r#"
import fcntl, json, sys, time
from pathlib import Path
root = Path(sys.argv[1])
def change(delta):
    with (root / 'lock').open('a+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = root / 'state'
        state = json.loads(path.read_text()) if path.exists() else {'active': 0, 'peak': 0}
        state['active'] += delta
        state['peak'] = max(state['peak'], state['active'])
        path.write_text(json.dumps(state))
        assert state['peak'] <= 2, 'too many harnesses admitted'
        return state
change(1)
deadline = time.monotonic() + 10
while change(0)['peak'] != 2:
    assert time.monotonic() < deadline, 'harness preparation was serialized'
    time.sleep(0.01)
time.sleep(0.05)
change(-1)
"#,
    )
    .unwrap();
    fs::write(f.0.join("CMakeLists.txt"), r#"
cmake_minimum_required(VERSION 3.25)
project(HarnessAdmission NONE)
enable_testing()
foreach(i RANGE 1 3)
  add_test(NAME app.ui-macos-fixture-${i} COMMAND python3 "${CMAKE_SOURCE_DIR}/work.py" "${CMAKE_SOURCE_DIR}")
  set_tests_properties(app.ui-macos-fixture-${i} PROPERTIES LABELS macos_gui_harness TIMEOUT 15)
endforeach()
"#).unwrap();
    let build = f.0.join("build");
    f.run(
        Command::new("cmake")
            .arg("-S")
            .arg(&f.0)
            .arg("-B")
            .arg(&build),
    );
    let out = f.0.join("results");
    f.run(
        Command::new(env!("CARGO_BIN_EXE_mitcad-testrun"))
            .arg("--build")
            .arg(build)
            .arg("--schedule")
            .arg("--out")
            .arg(&out)
            .arg("--profiles")
            .arg(f.0.join("profiles.json"))
            .args([
                "--no-record",
                "--memory-budget",
                "24G",
                "--memory-floor",
                "0",
                "--cpu-gate",
                "1",
                "--limit-factor",
                "0",
                "--sensitive",
                "app.ui-*",
            ]),
    );
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(f.0.join("state")).unwrap()).unwrap();
    assert_eq!(state["peak"], 2);
    assert_eq!(state["active"], 0);
    assert!(
        !fs::read_to_string(out.join("results.tsv"))
            .unwrap()
            .contains("rerun")
    );
}

#[test]
fn headless_qt_workflows_overlap_each_other_and_the_desktop_lane() {
    let f = Fixture::new();
    fs::write(
        f.0.join("work.py"),
        r#"
import sys, time
from pathlib import Path
root = Path(sys.argv[1])
(root / sys.argv[2]).touch()
deadline = time.monotonic() + 10
while len(list(root.glob('started-*'))) != 4:
    assert time.monotonic() < deadline, 'headless work was held in the desktop/UI lane'
    time.sleep(0.01)
"#,
    )
    .unwrap();
    fs::write(f.0.join("CMakeLists.txt"), r#"
cmake_minimum_required(VERSION 3.25)
project(HeadlessAdmission NONE)
enable_testing()
foreach(i RANGE 1 3)
  add_test(NAME app.ui-macos-headless-${i} COMMAND python3 "${CMAKE_SOURCE_DIR}/work.py" "${CMAKE_SOURCE_DIR}" started-${i})
  set_tests_properties(app.ui-macos-headless-${i} PROPERTIES LABELS macos_headless TIMEOUT 15)
endforeach()
add_test(NAME app.ui-macos-desktop COMMAND python3 "${CMAKE_SOURCE_DIR}/work.py" "${CMAKE_SOURCE_DIR}" started-desktop)
set_tests_properties(app.ui-macos-desktop PROPERTIES RESOURCE_LOCK macos_desktop TIMEOUT 15)
"#).unwrap();
    let build = f.0.join("build");
    f.run(
        Command::new("cmake")
            .arg("-S")
            .arg(&f.0)
            .arg("-B")
            .arg(&build),
    );
    // Migrated workflows may have learned desktop contention in old runs.
    fs::write(
        f.0.join("profiles.json"),
        r#"{"format":1,"tests":{
          "app.ui-macos-headless-1":{"sensitive":true,"ui_room":0},
          "app.ui-macos-headless-2":{"sensitive":true,"ui_room":0},
          "app.ui-macos-headless-3":{"sensitive":true,"ui_room":0}
        }}"#,
    )
    .unwrap();
    let out = f.0.join("results");
    f.run(
        Command::new(env!("CARGO_BIN_EXE_mitcad-testrun"))
            .arg("--build")
            .arg(build)
            .arg("--schedule")
            .arg("--out")
            .arg(&out)
            .arg("--profiles")
            .arg(f.0.join("profiles.json"))
            .args([
                "--no-record",
                "-j",
                "4",
                "--memory-budget",
                "24G",
                "--memory-floor",
                "0",
                "--cpu-gate",
                "1",
                "--limit-factor",
                "0",
                "--sensitive",
                "app.ui-*",
                "--sensitive-max",
                "1",
                "--ui-max",
                "1",
            ]),
    );
    let rows = fs::read_to_string(out.join("results.tsv")).unwrap();
    assert_eq!(rows.lines().count(), 5);
    assert!(!rows.contains("rerun"));
}
