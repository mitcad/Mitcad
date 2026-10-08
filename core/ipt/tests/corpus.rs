// SPDX-License-Identifier: MIT
//! Real part files (never committed) under MITCAD_IPT_CORPUS (a path list
//! of folders, like PATH): every file opens, its B-rep records split out of
//! their segments (not found by scanning), and every body converts without
//! issues to a neutral B-rep that passes its checks; every segment's
//! records split; every parameter record reads, every expression of the
//! part's parameters translates and evaluates to its stored value, and
//! every feature names its state in the ASM history the B-rep record
//! carries. Skipped without the corpus. Building the bodies with OCCT and comparing them with STEP
//! references is the ctest `ipt.corpus` (tools/cli/ipt-corpus.cmake).

use std::path::{Path, PathBuf};

use mitcad_ipt::{IptFile, read_bodies};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ipt"))
        {
            out.push(path);
        }
    }
}

#[test]
fn corpus_bodies_convert() {
    let Some(corpus) = std::env::var_os("MITCAD_IPT_CORPUS") else {
        eprintln!("ipt corpus: skipped (MITCAD_IPT_CORPUS is not set)");
        return;
    };
    let mut all = Vec::new();
    for dir in std::env::split_paths(&corpus) {
        files(&dir, &mut all);
    }
    all.sort();
    let mut failures = Vec::new();
    let mut bodies = 0;
    for (i, path) in all.iter().enumerate() {
        // Files are named by position, as the corpus is not ours to name.
        let name = format!("file {:02}", i + 1);
        let file = match IptFile::open(path) {
            Ok(f) => f,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        if !file.is_part() {
            failures.push(format!("{name}: not a part's class id"));
        }
        let records = match file.brep_records() {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        if records.iter().any(|r| r.record.is_none()) {
            failures.push(format!("{name}: B-rep records found by scanning"));
        }
        for (record, blob) in records.iter().zip(read_bodies(&records)) {
            let blob = match blob {
                Ok(b) => b,
                Err(e) => {
                    failures.push(format!("{name}: {e}"));
                    continue;
                }
            };
            if let Some(t) = &blob.truncated {
                failures.push(format!("{name} {}: truncated: {t}", record.place()));
            }
            for body in &blob.bodies {
                bodies += 1;
                if !body.issues.is_empty() || body.skipped_faces > 0 || !body.check.is_clean() {
                    failures.push(format!(
                        "{name} {} body {}: {} issues, {} skipped faces, check {:?}",
                        record.place(),
                        body.record,
                        body.issues.len(),
                        body.skipped_faces,
                        body.check.messages
                    ));
                }
            }
        }
    }
    let mut parameters = 0;
    let mut features = 0;
    let mut sketches = 0;
    let mut unknown_states = 0;
    for (i, path) in all.iter().enumerate() {
        let name = format!("file {:02}", i + 1);
        let Ok(file) = IptFile::open(path) else {
            continue;
        };
        for segment in &file.segments {
            match file.segment_data(segment) {
                Ok(d) => {
                    if let Err(e) = d.records {
                        failures.push(format!("{name} {}: {e}", segment.name));
                    }
                }
                Err(e) => failures.push(format!("{name} {}: {e}", segment.name)),
            }
        }
        let design = match mitcad_ipt::design::read(&file, &name) {
            Ok(Some(d)) => d,
            Ok(None) => {
                failures.push(format!("{name}: no definitions segment"));
                continue;
            }
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let p = &design.parameters;
        let e = &design.expressions;
        parameters += p.model + p.user;
        if !p.unread.is_empty() || !e.differ.is_empty() || !e.not_translated.is_empty() {
            failures.push(format!(
                "{name}: parameters not read {:?}, expressions differing {:?}, not translated {:?}",
                p.unread, e.differ, e.not_translated
            ));
        }
        if e.agree != p.model + p.user {
            failures.push(format!(
                "{name}: {} of {} expressions agree",
                e.agree,
                p.model + p.user
            ));
        }
        // Every feature (an item with a state) names a state of the history.
        let records = file.brep_records().unwrap_or_default();
        let states: Vec<i64> = records
            .iter()
            .filter_map(|r| {
                let asm = mitcad_f3d::asm::AsmFile::parse(&r.asm).ok()?;
                mitcad_f3d::asm::history::History::parse(&asm)
                    .ok()
                    .flatten()
            })
            .flat_map(|h| h.states.into_iter().map(|s| s.id))
            .collect();
        for item in &design.items {
            match item.state {
                Some(s) if !states.contains(&s) => unknown_states += 1,
                Some(_) => features += 1,
                None if item.object_type == "Sketch" => sketches += 1,
                None => {}
            }
        }
    }
    eprintln!(
        "ipt corpus: {} files, {bodies} bodies, {parameters} parameters, {sketches} sketches, \
         {features} features with their history states ({unknown_states} naming states the \
         history does not have), {} failures",
        all.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
