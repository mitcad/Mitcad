// SPDX-License-Identifier: MIT
//! Slicers' print settings in real 3MF projects (mitcad#640), never
//! committed: every project that the `index.tsv` of a folder of
//! `MITCAD_3MF_CORPUS` (a `PATH`-like list) lists is imported into a fresh
//! design as the application's Import from Slicer... imports it
//! (`import_print_profile` into the design), checked against its row, then
//! exported in the same dialect and read back: every value read into
//! Mitcad's keys must come back alike.
//!
//! `index.tsv`: comment lines (`#`), a header row naming the columns, then
//! a row per project. The columns checked when a row gives them:
//!
//! - `file`: the project's path relative to the index (required; projects
//!   are named by their content id, the first 8 hex digits of their
//!   SHA-256);
//! - `bytes`: its size;
//! - `dialect`: the dialect the import reads (`bambu_studio`,
//!   `orcaslicer`, `qidi_studio`, `prusaslicer`, `cura`);
//! - `printers`, `processes`, `materials`: how many of each came in;
//! - `printer`, `process`: the names of the printer and the process;
//! - `problems`: the values not read, as the import's summary lists them
//!   (each distinct text once, ending with "(N times)" when it came more
//!   than once), separated by ` | `; empty or `none`: none.
//!
//! Other columns (the original name, the SHA-256, the slicer and its
//! version, the source) are notes. The test is ignored in `core.model`;
//! the ctest `3mf.corpus` runs it (`--ignored`). Skipped without the
//! variable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::print_files_tests::{command, export_to, small_block};
use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::material::Material;
use mitcad_model::print_files::*;
use mitcad_model::print_keys::Dialect;
use mitcad_model::print_profile::PrintProfile;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

/// A row of an index: the index's folder and the row's cells by column.
struct Row {
    dir: PathBuf,
    cells: BTreeMap<String, String>,
}

impl Row {
    /// A cell's text, when the row gives it.
    fn get(&self, column: &str) -> Option<&str> {
        self.cells
            .get(column)
            .map(|cell| cell.trim())
            .filter(|cell| !cell.is_empty())
    }
}

/// The rows of a folder's `index.tsv` (none without one).
fn rows(dir: &Path) -> Vec<Row> {
    let Ok(text) = std::fs::read_to_string(dir.join("index.tsv")) else {
        return Vec::new();
    };
    let mut header: Option<Vec<String>> = None;
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let cells = line.split('\t').map(|cell| cell.trim().to_owned());
        match &header {
            None => header = Some(cells.collect()),
            Some(names) => out.push(Row {
                dir: dir.to_path_buf(),
                cells: names.iter().cloned().zip(cells).collect(),
            }),
        }
    }
    out
}

/// Each distinct problem once, in the order first seen, ending with its
/// count when it came more than once: as the import's summary lists them
/// (mitcad#654, `distinctProblems` in `app/browser/PrintImportSummary.cpp`).
fn distinct(problems: &[String]) -> Vec<String> {
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for problem in problems {
        match seen.iter_mut().find(|(text, _)| *text == problem.as_str()) {
            Some((_, count)) => *count += 1,
            None => seen.push((problem.as_str(), 1)),
        }
    }
    seen.into_iter()
        .map(|(text, count)| {
            if count > 1 {
                format!("{text} ({count} times)")
            } else {
                text.to_owned()
            }
        })
        .collect()
}

/// Values alike: numbers within 1e-6 (and a part in 1e9 of their size),
/// lists and objects item by item, the rest equal.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let x = x.as_f64().unwrap_or(f64::NAN);
            let y = y.as_f64().unwrap_or(f64::NAN);
            (x - y).abs() <= 1e-6 + 1e-9 * x.abs().max(y.abs())
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// The values of `before` that `after` does not have alike.
fn lost(
    what: &str,
    before: &BTreeMap<String, Value>,
    after: &BTreeMap<String, Value>,
    out: &mut Vec<String>,
) {
    for (key, value) in before {
        match after.get(key) {
            Some(back) if same(value, back) => {}
            Some(back) => out.push(format!("{what}: {key} {value} came back as {back}")),
            None => out.push(format!("{what}: {key} {value} did not come back")),
        }
    }
}

/// The first profile's name and values.
fn first_profile(profiles: &[PrintProfile]) -> Option<(&str, &BTreeMap<String, Value>)> {
    profiles.first().map(|p| (p.name.as_str(), &p.values))
}

/// Imports a row's project into a fresh design, checks it against the row,
/// exports it in the same dialect and reads that back. The failures, each
/// starting with the file.
fn check(row: &Row) -> Vec<String> {
    let Some(file) = row.get("file") else {
        return vec![format!(
            "{}: a row without a file",
            row.dir.join("index.tsv").display()
        )];
    };
    let mut failures = Vec::new();
    let mut fail = |text: String| failures.push(format!("{file}: {text}"));
    let path = row.dir.join(file);
    let data = match std::fs::read(&path) {
        Ok(data) => data,
        Err(e) => {
            fail(format!("cannot read {}: {e}", path.display()));
            return failures;
        }
    };
    if let Some(bytes) = row.get("bytes")
        && bytes != data.len().to_string()
    {
        fail(format!("{} bytes, the index says {bytes}", data.len()));
    }

    // The import, as the Print Profiles window makes it.
    let mut doc = Document::new(MockKernel::default());
    let import = json!({"cmd": "import_print_profile", "path": path.to_string_lossy(),
                        "into": "design"});
    let result: Value = match doc.command(&import.to_string()) {
        Ok(text) => serde_json::from_str(&text).unwrap(),
        Err(e) => {
            fail(format!("the import failed: {e}"));
            return failures;
        }
    };
    let dialect = result["dialect"].as_str().unwrap_or("").to_owned();
    let report = &result["report"];
    let problems: Vec<String> = report["problems"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let problems = distinct(&problems);
    let count = |field: &str| result[field].as_array().map_or(0, Vec::len);
    let name = |field: &str| result[field][0]["name"].as_str().unwrap_or("").to_owned();
    eprintln!(
        "== {file}: {dialect}; {} printers, {} processes, {} materials; {} settings read, {} kept; \
         {} not read",
        count("printers"),
        count("processes"),
        count("materials"),
        report["mapped"],
        report["kept"],
        problems.len()
    );
    for problem in &problems {
        eprintln!("   not read: {problem}");
    }
    if let Some(expected) = row.get("dialect")
        && expected != dialect
    {
        fail(format!("read as {dialect:?}, the index says {expected}"));
    }
    for field in ["printers", "processes", "materials"] {
        if let Some(expected) = row.get(field)
            && expected != count(field).to_string()
        {
            fail(format!(
                "{} {field}, the index says {expected}",
                count(field)
            ));
        }
    }
    for (column, field) in [("printer", "printers"), ("process", "processes")] {
        if let Some(expected) = row.get(column)
            && expected != name(field)
        {
            fail(format!(
                "the {column} is named {:?}, the index says {expected:?}",
                name(field)
            ));
        }
    }
    let expected: Vec<String> = match row.get("problems") {
        None | Some("none") => Vec::new(),
        Some(text) => text.split(" | ").map(|p| p.trim().to_owned()).collect(),
    };
    if problems != expected {
        fail(format!(
            "the values not read are {problems:?}, the index says {expected:?}"
        ));
    }

    // The round trip: a body of each material in a component of its own
    // (so a filament slot each), the root printing with the printer and
    // the process read; exported in the dialect, read back.
    let Some(dialect_id) = Dialect::from_id(&dialect) else {
        fail(format!("no dialect to export in ({dialect:?})"));
        return failures;
    };
    let materials: Vec<String> = result["materials"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if materials.is_empty() {
        small_block(&mut doc);
    }
    for (i, material) in materials.iter().enumerate() {
        let component = format!("Slot {}", i + 1);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        doc.create_component(
            Some(component.as_str()),
            Transform::translation([20.0 * i as f64, 0.0, 0.0]),
            true,
        )
        .unwrap();
        small_block(&mut doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        command(
            &mut doc,
            json!({"cmd": "set_component_material", "component": component, "material": material}),
        );
    }
    let stem = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let (exported, back_data) = export_to(&mut doc, &format!("{stem}-back.3mf"), dialect_id.id());
    for warning in exported["warnings"].as_array().into_iter().flatten() {
        eprintln!("   export warning: {}", warning.as_str().unwrap_or(""));
    }
    let first = match read_file(file, &data, None) {
        Ok(first) => first,
        Err(e) => {
            fail(format!("the project does not read again: {e}"));
            return failures;
        }
    };
    let back = match read_file(&format!("{stem}-back.3mf"), &back_data, None) {
        Ok(back) => back,
        Err(e) => {
            fail(format!("the export does not read back: {e}"));
            return failures;
        }
    };
    if back.dialect != first.dialect {
        fail(format!(
            "the export reads back as {:?}, the project as {:?}",
            back.dialect, first.dialect
        ));
    }
    let mut losses = Vec::new();
    for (what, before, after) in [
        (
            "the printer",
            first_profile(&first.printers),
            first_profile(&back.printers),
        ),
        (
            "the process",
            first_profile(&first.processes),
            first_profile(&back.processes),
        ),
    ] {
        match (before, after) {
            (None, _) => {}
            (Some(_), None) => losses.push(format!("{what} did not come back")),
            (Some((name, values)), Some((back_name, back_values))) => {
                if name != back_name {
                    losses.push(format!("{what} {name:?} came back as {back_name:?}"));
                }
                lost(what, values, back_values, &mut losses);
            }
        }
    }
    // Each material back: one alike of the export's, else the losses
    // against the one in its slot.
    let mut unused: Vec<usize> = (0..back.materials.len()).collect();
    for (i, (material, _)) in first.materials.iter().enumerate() {
        let what = format!("material {} ({})", i + 1, material.name);
        let against = |other: &Material, out: &mut Vec<String>| {
            if other.name != material.name {
                out.push(format!("{what} came back named {:?}", other.name));
            }
            if !same(&json!(material.density), &json!(other.density)) {
                out.push(format!(
                    "{what}: density {} came back as {}",
                    material.density, other.density
                ));
            }
            lost(&what, &material.filament, &other.filament, out);
        };
        let alike = unused.iter().position(|j| {
            let mut out = Vec::new();
            against(&back.materials[*j].0, &mut out);
            out.is_empty()
        });
        match alike {
            Some(at) => {
                unused.remove(at);
            }
            None => match back.materials.get(i) {
                Some((other, _)) => against(other, &mut losses),
                None => losses.push(format!("{what} did not come back")),
            },
        }
    }
    for loss in losses {
        fail(loss);
    }
    failures
}

#[test]
#[ignore = "the 3MF corpus: the ctest 3mf.corpus runs it (--ignored)"]
fn projects_import_as_indexed_and_their_values_come_back() {
    let Some(corpus) = std::env::var_os("MITCAD_3MF_CORPUS") else {
        eprintln!("3mf corpus: skipped (MITCAD_3MF_CORPUS is not set)");
        return;
    };
    let rows: Vec<Row> = std::env::split_paths(&corpus)
        .flat_map(|dir| rows(&dir))
        .collect();
    if rows.is_empty() {
        eprintln!("3mf corpus: skipped (no index.tsv of MITCAD_3MF_CORPUS lists a project)");
        return;
    }
    let failures: Vec<String> = rows.iter().flat_map(check).collect();
    assert!(
        failures.is_empty(),
        "3mf corpus: {} failures in {} projects:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );
    eprintln!("3mf corpus: {} projects pass their checks", rows.len());
}
