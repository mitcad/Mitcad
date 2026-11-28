// SPDX-License-Identifier: MIT
//! Writes the generated API reference (mitcad#2606) into a folder: the
//! HTML and Markdown pages of every command, query and feature type that
//! `model_reference` lists, the overview, the topics, the index and its
//! search. `tools/api-reference/build.sh` runs it:
//!
//! ```text
//! cargo run -p mitcad-mcp --example api_reference -- <folder> [<report file>]
//! ```
//!
//! The report (default `<folder>.report.txt`, beside the folder, not
//! published) lists what the cleaning left out and the names without an
//! example. The run fails, writing nothing, when a page still names a
//! ticket, history, a file or a test, or when the folder holds files that
//! are not pages of this reference (mitcad#2636: the folder is published
//! as it is, and an earlier run's stale pages would go with it).

use std::path::PathBuf;
use std::process::ExitCode;

use mitcad_mcp::Vocabulary;
use mitcad_mcp::api_reference::{self, PageKind};

/// The names `model_reference` lists, as the application gives them to it
/// (core/ffi/src/mcp.rs), with the headless server's two print imports.
fn listed() -> Vocabulary {
    let owned =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    let mut commands = owned(&mitcad_model_api::classes::external_commands());
    commands.extend(owned(&["import_print_profile", "import_slicer_presets"]));
    Vocabulary {
        queries: owned(&mitcad_model_api::classes::external_queries()),
        commands,
        features: owned(<mitcad_model::FeatureDef>::TYPE_NAMES),
    }
}

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let Some(folder) = arguments.next().map(PathBuf::from) else {
        eprintln!("usage: api_reference <folder> [<report file>]");
        return ExitCode::from(2);
    };
    let report = arguments.next().map(PathBuf::from).unwrap_or_else(|| {
        let mut name = folder.clone().into_os_string();
        name.push(".report.txt");
        PathBuf::from(name)
    });
    let reference = api_reference::generate(&listed(), env!("CARGO_PKG_VERSION"));
    let mut problems = Vec::new();
    for page in &reference.pages {
        for problem in api_reference::problems(&page.markdown()) {
            problems.push(format!("{}: {problem}", page.path));
        }
    }
    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("{problem}");
        }
        eprintln!(
            "the API reference is not clean: {} lines; fix their sentences in core/model-api/src/commands.md",
            problems.len()
        );
        return ExitCode::FAILURE;
    }
    // The folder is published as it is (mitcad#2636): an earlier run's
    // page that is no page any more would be published as a stale page.
    let present = match api_reference::files_in(&folder) {
        Ok(present) => present,
        Err(error) => {
            eprintln!("cannot read {}: {error}", folder.display());
            return ExitCode::FAILURE;
        }
    };
    let foreign = reference.foreign_files(&present);
    if !foreign.is_empty() {
        for path in foreign.iter().take(20) {
            eprintln!("{}", folder.join(path).display());
        }
        eprintln!(
            "{} holds {} files that are not pages of this reference (an earlier run's or others): give a new or empty folder, so that only this reference is published",
            folder.display(),
            foreign.len()
        );
        return ExitCode::FAILURE;
    }
    if let Err(error) = reference.write_to(&folder) {
        eprintln!("cannot write {}: {error}", folder.display());
        return ExitCode::FAILURE;
    }
    if let Err(error) = std::fs::write(&report, reference.report_text()) {
        eprintln!("cannot write {}: {error}", report.display());
        return ExitCode::FAILURE;
    }
    println!(
        "{}: {} pages ({} commands, {} queries, {} feature types, {} topics); {} sentences left out and {} gaps in {}",
        folder.display(),
        reference.pages.len(),
        reference.count(PageKind::Command),
        reference.count(PageKind::Query),
        reference.count(PageKind::Feature),
        reference.count(PageKind::Topic),
        reference.omissions.len(),
        reference.gaps.len(),
        report.display()
    );
    ExitCode::SUCCESS
}
