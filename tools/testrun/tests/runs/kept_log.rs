// SPDX-License-Identifier: MIT
//! Every line a test writes reaches the log the runner keeps (mitcad#2614).
//!
//! A build tree of its own with two ctests: one writes a long output to
//! stdout with lines on stderr among it (more than the 300 KiB of a failed
//! test's output that ctest keeps) and fails; one writes a line and
//! passes. The runner's program runs them through ctest as it runs the
//! project's tests, and its logs, `failures.tsv` and report are read.
//!
//! The ctest and cmake to run: `MITCAD_TESTRUN_CTEST` and
//! `MITCAD_TESTRUN_CMAKE` (the build's own, set by core/CMakeLists.txt for
//! the ctest `tools.testrun`), else `ctest` and `cmake` on the PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The stdout lines of the failing test, and every how many of them it
/// writes a line on stderr.
const LINES: usize = 4000;
const EVERY: usize = 300;

fn program(variable: &str, default: &str) -> String {
    std::env::var(variable)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// A path as a CMake bracket argument.
fn bracket(path: &Path) -> String {
    format!("[==[{}]==]", path.display())
}

/// The scratch folder of a test, empty.
fn scratch(test: &str) -> PathBuf {
    let root =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{test}-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("build")).unwrap();
    root
}

/// The build tree: the scripts of the two tests and ctest's test file.
fn build_tree(root: &Path, cmake: &str) {
    let pad = ".".repeat(100);
    let noisy = format!(
        "foreach(i RANGE 1 {LINES})\n\
         \x20 message(STATUS \"out ${{i}} {pad}\")\n\
         \x20 math(EXPR rest \"${{i}} % {EVERY}\")\n\
         \x20 if(rest EQUAL 0)\n\
         \x20   message(NOTICE \"err ${{i}}: a catalogue has no translation of a message\")\n\
         \x20 endif()\n\
         endforeach()\n\
         message(FATAL_ERROR \"2 check(s) failed\")\n"
    );
    fs::write(root.join("noisy.cmake"), noisy).unwrap();
    fs::write(
        root.join("quiet.cmake"),
        "message(STATUS \"a quiet line\")\n",
    )
    .unwrap();
    let cmake = bracket(Path::new(cmake));
    let tests = format!(
        "add_test([==[fake.noisy]==] {cmake} -P {})\n\
         add_test([==[fake.quiet]==] {cmake} -P {})\n",
        bracket(&root.join("noisy.cmake")),
        bracket(&root.join("quiet.cmake")),
    );
    fs::write(root.join("build/CTestTestfile.cmake"), tests).unwrap();
}

#[test]
fn every_line_of_a_long_failing_output_is_kept() {
    let root = scratch("every_line_of_a_long_failing_output_is_kept");
    let ctest = program("MITCAD_TESTRUN_CTEST", "ctest");
    let cmake = program("MITCAD_TESTRUN_CMAKE", "cmake");
    build_tree(&root, &cmake);
    let out = root.join("out");
    let run = Command::new(env!("CARGO_BIN_EXE_mitcad-testrun"))
        .arg("--build")
        .arg(root.join("build"))
        .arg("--out")
        .arg(&out)
        .arg("--profiles")
        .arg(root.join("profiles.json"))
        .args([
            "--measure",
            "-j",
            "1",
            "--no-record",
            "--no-cgroup",
            "--disk-floor",
            "0",
        ])
        .arg("--ctest")
        .arg(&ctest)
        .output()
        .unwrap();
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    // One test failed: exit 1, not a usage or listing error.
    assert_eq!(run.status.code(), Some(1), "{said}");

    let log = fs::read_to_string(out.join("logs/fake.noisy.log")).unwrap();
    assert!(log.len() > 300 * 1024, "{} bytes", log.len());
    // Every stdout line, in the order written.
    let mut at = 0;
    for i in 1..=LINES {
        let line = format!("-- out {i} .");
        let Some(found) = log[at..].find(&line) else {
            panic!("stdout line {i} is not in the log after byte {at}");
        };
        at += found + line.len();
    }
    // Every stderr line, in the order written, and the last one.
    let mut at = 0;
    for i in (EVERY..=LINES).step_by(EVERY) {
        let line = format!("err {i}: a catalogue has no translation");
        let Some(found) = log[at..].find(&line) else {
            panic!("stderr line {i} is not in the log after byte {at}");
        };
        at += found + line.len();
    }
    assert!(log.contains("2 check(s) failed"), "the last stderr line");
    // ctest's result line, read as the failure.
    assert!(log.contains("Test #1: fake.noisy"), "the result line");
    let results = fs::read_to_string(out.join("results.tsv")).unwrap();
    assert!(results.contains("fake.noisy\tfailed\t"), "{results}");
    assert!(results.contains("fake.quiet\tpassed\t"), "{results}");

    // A passing test's lines are kept too.
    let quiet = fs::read_to_string(out.join("logs/fake.quiet.log")).unwrap();
    assert!(quiet.contains("-- a quiet line"), "{quiet}");

    // The failure's own lines quoted: the first ten, the rest counted.
    let failures = fs::read_to_string(out.join("failures.tsv")).unwrap();
    let rows: Vec<&str> = failures.lines().skip(1).collect();
    let stderr_lines = LINES / EVERY;
    assert_eq!(rows.len(), 11, "{failures}");
    assert_eq!(
        rows[0],
        format!("fake.noisy\tfailed\terr {EVERY}: a catalogue has no translation of a message")
    );
    let rest = format!(
        "fake.noisy\tfailed\t({} more such lines in the log)",
        stderr_lines - 10
    );
    assert_eq!(rows[10], rest);
    let report = fs::read_to_string(out.join("report.txt")).unwrap();
    assert!(
        report.contains("      | err 300: a catalogue has no translation"),
        "{report}"
    );
    fs::remove_dir_all(&root).unwrap();
}
