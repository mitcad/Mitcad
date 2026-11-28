// SPDX-License-Identifier: MIT
//! What the runner takes from ctest: the tests and their properties
//! (`ctest --show-only=json-v1`), the command line that runs one test,
//! and the result line ctest prints for it.

use serde_json::Value;

/// One test of the build tree, with the properties the scheduler needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TestDef {
    pub name: String,
    /// Has a command (ctest lists tests of other configurations without).
    pub available: bool,
    pub disabled: bool,
    pub depends: Vec<String>,
    pub fixtures_setup: Vec<String>,
    pub fixtures_cleanup: Vec<String>,
    pub fixtures_required: Vec<String>,
    pub resource_locks: Vec<String>,
    pub run_serial: bool,
    pub processors: f64,
    pub timeout: Option<f64>,
    pub labels: Vec<String>,
    /// The command's words and ENVIRONMENT (changed.rs).
    pub command: Vec<String>,
    pub environment: Vec<String>,
}

/// A property's value as a list of strings: a JSON list, or one string
/// (ctest writes lists as arrays; a single value may come as a string).
fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Value::String(s) if !s.is_empty() => s.split(';').map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        Value::String(s) => true_word(s),
        _ => false,
    }
}

/// CMake's true constants that ctest might pass through as text.
fn true_word(s: &str) -> bool {
    let upper = s.trim().to_ascii_uppercase();
    upper == "1" || upper == "ON" || upper == "TRUE" || upper == "YES"
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// The tests of `ctest --show-only=json-v1`, in ctest's order.
pub fn parse_tests(json: &str) -> Result<Vec<TestDef>, String> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| format!("ctest's JSON does not parse: {e}"))?;
    let Some(tests) = root.get("tests").and_then(Value::as_array) else {
        return Err("ctest's JSON has no tests".into());
    };
    let mut defs = Vec::new();
    for test in tests {
        let Some(name) = test.get("name").and_then(Value::as_str) else {
            continue;
        };
        let mut def = TestDef {
            name: name.to_owned(),
            available: test.get("command").is_some_and(|c| !strings(c).is_empty()),
            command: test.get("command").map(strings).unwrap_or_default(),
            ..TestDef::default()
        };
        let properties = test.get("properties").and_then(Value::as_array);
        for property in properties.into_iter().flatten() {
            let Some(key) = property.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(value) = property.get("value") else {
                continue;
            };
            match key {
                "DEPENDS" => def.depends = strings(value),
                "FIXTURES_SETUP" => def.fixtures_setup = strings(value),
                "FIXTURES_CLEANUP" => def.fixtures_cleanup = strings(value),
                "FIXTURES_REQUIRED" => def.fixtures_required = strings(value),
                "RESOURCE_LOCK" => def.resource_locks = strings(value),
                "RUN_SERIAL" => def.run_serial = truthy(value),
                "DISABLED" => def.disabled = truthy(value),
                "PROCESSORS" => def.processors = number(value).unwrap_or(0.0),
                "TIMEOUT" => def.timeout = number(value),
                "LABELS" => def.labels = strings(value),
                "ENVIRONMENT" => def.environment = strings(value),
                _ => {}
            }
        }
        defs.push(def);
    }
    Ok(defs)
}

/// The tests a list of names selects (`--tests-from`, mitcad#2579), and
/// the names the listing does not have.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Named {
    pub tests: Vec<TestDef>,
    pub missing: Vec<String>,
}

/// The names of a test list: one exact test name a line; empty lines are
/// left out and a line's ends trimmed (a list written on Windows).
pub fn read_names(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The tests of the listing that the list names, in the listing's order,
/// with the tests that set up or clean up the fixtures they require (as
/// ctest adds them to a `-R` selection; a setup's own fixtures too). A
/// list that names no test, or none of the listing's, is refused: a run
/// that was to run tests never passes by running none.
pub fn select_named(defs: Vec<TestDef>, names: &[String]) -> Result<Named, String> {
    use std::collections::HashSet;
    if names.is_empty() {
        return Err("the test list names no tests".into());
    }
    let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut keep: Vec<bool> = defs
        .iter()
        .map(|def| wanted.contains(def.name.as_str()))
        .collect();
    let named = keep.iter().filter(|&&k| k).count();
    if named == 0 {
        return Err(format!(
            "none of the {} tests of the test list is in the build tree's selection",
            wanted.len()
        ));
    }
    // The fixtures' setup and cleanup tests, until no more join.
    loop {
        let required: HashSet<&str> = defs
            .iter()
            .zip(&keep)
            .filter(|pair| *pair.1)
            .flat_map(|(def, _)| def.fixtures_required.iter().map(String::as_str))
            .collect();
        let mut added = false;
        for (def, k) in defs.iter().zip(keep.iter_mut()) {
            if *k {
                continue;
            }
            let serves = def
                .fixtures_setup
                .iter()
                .chain(&def.fixtures_cleanup)
                .any(|f| required.contains(f.as_str()));
            if serves {
                *k = true;
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    let listed: HashSet<&str> = defs.iter().map(|def| def.name.as_str()).collect();
    let mut missing: Vec<String> = Vec::new();
    for name in names {
        if !listed.contains(name.as_str()) && !missing.contains(name) {
            missing.push(name.clone());
        }
    }
    let tests = defs
        .into_iter()
        .zip(keep)
        .filter_map(|(def, k)| k.then_some(def))
        .collect();
    Ok(Named { tests, missing })
}

/// The test name as a regular expression that matches only it.
pub fn exact_regex(name: &str) -> String {
    let mut out = String::from("^");
    for c in name.chars() {
        if "\\.^$|()[]{}*+?".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('$');
    out
}

/// ctest's arguments that run one test of the build tree alone: no
/// fixtures added (the runner orders the setup and cleanup tests itself),
/// every line the test writes in the log.
///
/// `-V`, not `--output-on-failure` (mitcad#2614): ctest prints a failed
/// test's output under `--output-on-failure` only after the test, from
/// what it kept of it, and it keeps 300 KiB of a failed test's output
/// (`CTEST_CUSTOM_MAXIMUM_FAILED_TEST_OUTPUT_SIZE`), so a long output lost
/// lines there. Under `-V` it prints each line of stdout and stderr (one
/// pipe, in the order the test wrote them) as it comes, `<index>: <line>`,
/// with nothing left out, passed or failed.
pub fn run_args(build: &str, config: Option<&str>, name: &str) -> Vec<String> {
    let mut args = vec![
        "--test-dir".to_owned(),
        build.to_owned(),
        "-R".to_owned(),
        exact_regex(name),
        "-FA".to_owned(),
        ".*".to_owned(),
        "-V".to_owned(),
        "--no-tests=error".to_owned(),
    ];
    if let Some(config) = config {
        args.push("-C".to_owned());
        args.push(config.to_owned());
    }
    args
}

/// The result ctest printed for a test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Passed,
    Skipped,
    Failed,
    Timeout,
    NotRun,
}

/// The result line of the test in ctest's output, e.g.
/// `1/1 Test #12: core.model ......   Passed    3.21 sec` or
/// `...***Timeout 1500.04 sec`; None when there is none.
pub fn parse_status(output: &str, name: &str) -> Option<Status> {
    let marker = format!(": {name} ");
    for line in output.lines() {
        // A line of the test's own output (`-V`), which may quote another
        // ctest's result lines.
        if test_line(line).is_some() {
            continue;
        }
        if !line.contains("Test #") || !line.contains(&marker) {
            continue;
        }
        let rest = &line[line.find(&marker)? + marker.len()..];
        let verdict = rest.trim_start_matches([' ', '.']);
        let status = if verdict.starts_with("Passed") {
            Status::Passed
        } else if verdict.contains("Skipped") {
            Status::Skipped
        } else if verdict.contains("Timeout") {
            Status::Timeout
        } else if verdict.contains("Not Run") {
            Status::NotRun
        } else {
            // ***Failed, ***Exception: ..., and anything else.
            Status::Failed
        };
        return Some(status);
    }
    None
}

/// The log says the disk was full.
pub fn says_disk_full(output: &str) -> bool {
    const SIGNS: [&str; 3] = [
        "No space left on device",
        "There is not enough space on the disk",
        "ERROR_DISK_FULL",
    ];
    SIGNS.iter().any(|sign| output.contains(sign))
}

/// A line of the test's own output under `-V` (`<index>: <line>`): the
/// line without its prefix; None for ctest's own lines.
pub fn test_line(line: &str) -> Option<&str> {
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    if rest == ":" {
        return Some("");
    }
    rest.strip_prefix(": ")
}

/// The most failure lines `failure_lines` quotes of one test.
pub const QUOTED_FAILURE_LINES: usize = 10;

/// The longest a quoted failure line is, in characters.
const QUOTE_CHARS: usize = 300;

/// The lines of a failed test's log that name its failures, in the log's
/// order (mitcad#2614): a check that failed (`check failed`, `FAIL`), a
/// translation left out (`has no translation`, `untranslated`), a value
/// that differs (`expected` with `got`), a Rust panic (`panicked at`). At
/// most `max`, each cut to 300 characters; when there are more, a last
/// line counts the rest. Under `-V` only the test's own lines count (not
/// ctest's header of the command and its environment, nor its summary).
pub fn failure_lines(output: &str, max: usize) -> Vec<String> {
    const HEADER: [&str; 4] = [
        "Test command:",
        "Working Directory:",
        "Environment variables:",
        "Test timeout computed to be:",
    ];
    const CTEST: [&str; 5] = [
        "Test #",
        "The following tests FAILED",
        "Errors while running CTest",
        "tests failed out of",
        "Start ",
    ];
    let verbose = output.lines().any(|line| test_line(line).is_some());
    let mut quoted = Vec::new();
    let mut more = 0usize;
    let mut environment = false;
    for line in output.lines() {
        let text = if verbose {
            let Some(text) = test_line(line) else {
                continue;
            };
            // ctest's header before the output: the variables' lines
            // follow "Environment variables:", each indented.
            if HEADER.iter().any(|h| text.starts_with(h)) {
                environment = text.starts_with("Environment variables:");
                continue;
            }
            if environment && text.starts_with(' ') && text.contains('=') {
                continue;
            }
            environment = false;
            text
        } else {
            if CTEST.iter().any(|c| line.contains(c)) {
                continue;
            }
            line
        };
        let text = text.trim_end_matches('\r').trim();
        if !names_failure(text) {
            continue;
        }
        if quoted.len() < max {
            let cut: String = text.chars().take(QUOTE_CHARS).collect();
            quoted.push(cut.replace('\t', " "));
        } else {
            more += 1;
        }
    }
    if more > 0 {
        let lines = if more == 1 { "line" } else { "lines" };
        quoted.push(format!("({more} more such {lines} in the log)"));
    }
    quoted
}

/// A line that names a failure (`failure_lines`).
fn names_failure(text: &str) -> bool {
    const SIGNS: [&str; 5] = [
        "check failed",
        "FAIL",
        "has no translation",
        "untranslated",
        "panicked at",
    ];
    if SIGNS.iter().any(|sign| text.contains(sign)) {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    lower.contains("expected") && lower.contains("got")
}

/// A simple pattern: `*` matches any run of characters, the rest itself.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    if !text.starts_with(first) || text.len() < first.len() + last.len() {
        return false;
    }
    if !text[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
      "kind": "ctestInfo",
      "version": { "major": 1, "minor": 0 },
      "tests": [
        {
          "name": "core.cargo_build",
          "command": ["cmake", "-P", "cargo-tests.cmake"],
          "properties": [
            { "name": "FIXTURES_SETUP", "value": ["cargo_tests_debug"] },
            { "name": "RUN_SERIAL", "value": true },
            { "name": "TIMEOUT", "value": 3600.0 }
          ]
        },
        {
          "name": "core.model",
          "command": ["cmake", "-P", "cargo-tests.cmake", "--", "run"],
          "properties": [
            { "name": "FIXTURES_REQUIRED", "value": ["cargo_tests_debug"] },
            { "name": "RESOURCE_LOCK", "value": ["cargo_build"] },
            { "name": "LABELS", "value": ["rust", "model"] },
            { "name": "PROCESSORS", "value": 4 }
          ]
        },
        { "name": "other.config", "properties": [] },
        {
          "name": "app.ui-windows",
          "command": ["pwsh"],
          "properties": [
            { "name": "DEPENDS", "value": ["core.model"] },
            { "name": "DISABLED", "value": false }
          ]
        }
      ]
    }"#;

    #[test]
    fn tests_and_properties_are_read() {
        let tests = parse_tests(JSON).unwrap();
        let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
        let expected = [
            "core.cargo_build",
            "core.model",
            "other.config",
            "app.ui-windows",
        ];
        assert_eq!(names, expected);
        let build = &tests[0];
        assert!(build.run_serial && build.available);
        assert_eq!(build.fixtures_setup, ["cargo_tests_debug"]);
        assert_eq!(build.timeout, Some(3600.0));
        let model = &tests[1];
        assert_eq!(model.fixtures_required, ["cargo_tests_debug"]);
        assert_eq!(model.resource_locks, ["cargo_build"]);
        // A lock and a fixture make no serial test.
        assert!(!model.run_serial);
        assert_eq!(model.labels, ["rust", "model"]);
        assert_eq!(model.processors, 4.0);
        assert!(!tests[2].available);
        assert_eq!(tests[3].depends, ["core.model"]);
        assert!(!tests[3].disabled);
        assert!(parse_tests("{}").is_err());
    }

    #[test]
    fn names_become_exact_expressions() {
        assert_eq!(exact_regex("core.model"), "^core\\.model$");
        assert_eq!(exact_regex("a+b(c)"), "^a\\+b\\(c\\)$");
        let args = run_args("build/dev", None, "x.y");
        assert_eq!(args[..4], ["--test-dir", "build/dev", "-R", "^x\\.y$"]);
        assert!(args.contains(&"-FA".to_owned()));
        // Every line of the test as it comes, never ctest's kept and cut
        // copy of a failed test's output (mitcad#2614).
        assert!(args.contains(&"-V".to_owned()));
        assert!(!args.contains(&"--output-on-failure".to_owned()));
        let configured = run_args("b", Some("Debug"), "t");
        assert_eq!(configured[configured.len() - 2..], ["-C", "Debug"]);
    }

    #[test]
    fn result_lines_are_read() {
        let cases = [
            ("Test #1: t.x ...   Passed  3.2 sec", Status::Passed),
            ("Test #2: t.x ...***Failed  1.0 sec", Status::Failed),
            ("Test #3: t.x ...***Timeout 1500.0 sec", Status::Timeout),
            ("Test #4: t.x ...***Skipped 0.1 sec", Status::Skipped),
            ("Test #5: t.x ...***Exception: SegFault", Status::Failed),
            ("Test #6: t.x ...***Not Run 0.0 sec", Status::NotRun),
        ];
        for (line, status) in cases {
            assert_eq!(parse_status(line, "t.x"), Some(status), "{line}");
            // Another test's line.
            assert_eq!(parse_status(line, "t"), None, "{line}");
        }
        assert_eq!(parse_status("No tests were found!!!", "t.x"), None);
        // Under -V a line of the test's own output that quotes another
        // ctest's result is not the result.
        let verbose = "12: 1/1 Test #3: t.x ...   Passed  0.1 sec\n\
                       1/1 Test #12: t.x ...***Failed  2.0 sec\n";
        assert_eq!(parse_status(verbose, "t.x"), Some(Status::Failed));
    }

    #[test]
    fn verbose_lines_are_told_from_ctests_own() {
        assert_eq!(
            test_line("444: model_messages.cpp: x"),
            Some("model_messages.cpp: x")
        );
        assert_eq!(test_line("444: "), Some(""));
        assert_eq!(test_line("444:"), Some(""));
        assert_eq!(test_line("1/1 Test #444: app.unit ...   Passed"), None);
        assert_eq!(test_line("    Start 444: app.unit"), None);
        assert_eq!(test_line("Test project /b"), None);
        assert_eq!(test_line("12x: no"), None);
    }

    #[test]
    fn failure_lines_are_quoted_from_a_verbose_log() {
        // A log as ctest -V writes it (mitcad#2614): its header, the test's
        // lines (a translation left out in the middle of a long output),
        // the result and the summary.
        let mut log = String::from(
            "UpdateCTestConfiguration  from :/b/DartConfiguration.tcl\n\
             Test project /b\n\
             test 444\n    Start 444: app.unit\n\n\
             444: Test command: /b/app/test_app\n\
             444: Working Directory: /b/app\n\
             444: Environment variables: \n\
             444:  MITCAD_FAIL_FAST=1\n\
             444:  QT_QPA_PLATFORM=offscreen\n\
             444: Test timeout computed to be: 1500\n",
        );
        for i in 0..1000 {
            log.push_str(&format!("444: a line of output {i}\n"));
        }
        log.push_str(
            "444: model_messages.cpp: mitcad_fi.qm has no translation of \"a sheet\"\n\
             444: model_messages.cpp: mitcad_fi.qm: 1 of 15527 model message templates untranslated\n\
             444: test_app.cpp:12: check failed: a == b\n\
             444:   expected 3, got 4\n\
             444: 3 check(s) failed\n\
             1/1 Test #444: app.unit .........................***Failed   13.36 sec\n\n\
             0% tests passed, 1 tests failed out of 1\n\n\
             The following tests FAILED:\n\t444 - app.unit (Failed)\n\
             Errors while running CTest\n",
        );
        let lines = failure_lines(&log, QUOTED_FAILURE_LINES);
        assert_eq!(
            lines,
            [
                "model_messages.cpp: mitcad_fi.qm has no translation of \"a sheet\"",
                "model_messages.cpp: mitcad_fi.qm: 1 of 15527 model message templates untranslated",
                "test_app.cpp:12: check failed: a == b",
                "expected 3, got 4",
            ]
        );
        // More than the most: the rest counted.
        let many: String = (0..14).map(|i| format!("7: FAIL: case {i}\r\n")).collect();
        let lines = failure_lines(&many, 10);
        assert_eq!(lines.len(), 11);
        assert_eq!(lines[9], "FAIL: case 9");
        assert_eq!(lines[10], "(4 more such lines in the log)");
        // A long line is cut, a tab becomes a space.
        let long = format!("7: FAIL\t{}\n", "x".repeat(1000));
        let lines = failure_lines(&long, 10);
        assert_eq!(lines[0].chars().count(), 300);
        assert!(lines[0].starts_with("FAIL x"));
        // Nothing that names a failure: no lines.
        assert!(failure_lines("7: all good\n1/1 Test #7: t ...***Failed\n", 10).is_empty());
    }

    #[test]
    fn failure_lines_of_a_log_without_verbose_lines() {
        // ctest's --output-on-failure form (an older log): the test's lines
        // unprefixed, ctest's own left out.
        let log = "Test project /b\n    Start 9: x.y\n\
                   1/1 Test #9: x.y ...***Failed  1.0 sec\n\
                   thread 'a' panicked at src/lib.rs:3:5:\n\
                   The following tests FAILED:\n\t  9 - x.y (Failed)\n";
        assert_eq!(
            failure_lines(log, 10),
            ["thread 'a' panicked at src/lib.rs:3:5:"]
        );
    }

    fn def(name: &str) -> TestDef {
        TestDef {
            name: name.to_owned(),
            available: true,
            ..TestDef::default()
        }
    }

    fn names_of(tests: &[TestDef]) -> Vec<&str> {
        tests.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn test_lists_are_read() {
        let names = read_names("core.model\r\n\n  app.unit  \nx.y\n");
        assert_eq!(names, ["core.model", "app.unit", "x.y"]);
        assert!(read_names("\n \n").is_empty());
    }

    #[test]
    fn a_list_selects_its_tests_and_their_fixtures() {
        let mut build = def("core.cargo_build");
        build.fixtures_setup = vec!["cargo".into()];
        let mut prepare = def("data.prepare");
        prepare.fixtures_setup = vec!["data".into()];
        prepare.fixtures_required = vec!["cargo".into()];
        let mut tidy = def("data.tidy");
        tidy.fixtures_cleanup = vec!["data".into()];
        let mut model = def("core.model");
        model.fixtures_required = vec!["data".into()];
        let defs = vec![
            build,
            def("core.other"),
            prepare,
            model,
            tidy,
            def("app.unit"),
        ];
        let names = read_names("app.unit\ncore.model\nno.such\napp.unit\n");
        let named = select_named(defs.clone(), &names).unwrap();
        // The listing's order; the fixtures' setups (a setup's own too) and
        // cleanup joined, the tests the list does not name left out.
        assert_eq!(
            names_of(&named.tests),
            [
                "core.cargo_build",
                "data.prepare",
                "core.model",
                "data.tidy",
                "app.unit"
            ]
        );
        assert_eq!(named.missing, ["no.such"]);
        // A test without fixtures alone.
        let named = select_named(defs.clone(), &["core.other".to_owned()]).unwrap();
        assert_eq!(names_of(&named.tests), ["core.other"]);
        assert!(named.missing.is_empty());
    }

    #[test]
    fn an_empty_or_unmatched_list_is_refused() {
        let defs = vec![def("a.b"), def("c.d")];
        assert!(select_named(defs.clone(), &[]).is_err());
        assert!(select_named(defs.clone(), &read_names("\n\n")).is_err());
        assert!(select_named(defs, &read_names("x.y\nz\n")).is_err());
        // An empty listing (ctest selected nothing) too.
        assert!(select_named(Vec::new(), &read_names("a.b\n")).is_err());
    }

    #[test]
    fn a_long_list_selects_every_test() {
        // A scoped check's plan of 1500 tests (mitcad#2579: one joined
        // `-R` expression of about 1400 names was too big for ctest).
        let defs: Vec<TestDef> = (0..1600)
            .map(|i| def(&format!("group{}.test_{i}", i % 7)))
            .collect();
        let text: String = (0..1500)
            .map(|i| format!("group{}.test_{i}\n", i % 7))
            .collect();
        let named = select_named(defs, &read_names(&text)).unwrap();
        assert_eq!(named.tests.len(), 1500);
        assert!(named.missing.is_empty());
        assert_eq!(named.tests[1499].name, "group1.test_1499");
    }

    #[test]
    fn full_disks_are_seen() {
        assert!(says_disk_full("write: No space left on device"));
        assert!(!says_disk_full("all good"));
    }

    #[test]
    fn globs_match() {
        assert!(glob_match("app.ui-*", "app.ui-windows-timber"));
        assert!(glob_match("*corpus*", "f3d.corpus_occt"));
        assert!(glob_match("core.model", "core.model"));
        assert!(!glob_match("core.model", "core.model.api"));
        assert!(glob_match("a*b*c", "aXbYc"));
        assert!(!glob_match("a*b*c", "aXc"));
        assert!(!glob_match("ab*ba", "aba"));
    }
}
