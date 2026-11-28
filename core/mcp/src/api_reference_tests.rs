// SPDX-License-Identifier: MIT
//! The check of the generated API reference (mitcad#2606), with the names
//! `model_reference` lists: it is made, every command, query and feature
//! type has a page, no page names a ticket, history, a file or a test or
//! the code inside, its links find their pages, and the pages of
//! `add_feature`, `sketch.add_line`, `bodies`, `measure` and `extrude` have
//! their fields, a JSON example and their result; every query page opens
//! with its purpose and a folder with stale pages is told (mitcad#2636).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde_json::Value;

use crate::Vocabulary;
use crate::api_reference::{ApiReference, OVERVIEW, Page, PageKind, clean, generate, problems};

/// The names MCP permits, as the application gives them to
/// `model_reference` (core/ffi/src/mcp.rs), with the headless server's
/// two print imports.
fn permitted() -> Vocabulary {
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

/// The reference, made once for the tests.
fn reference() -> &'static ApiReference {
    static MADE: OnceLock<ApiReference> = OnceLock::new();
    MADE.get_or_init(|| generate(&permitted(), "0.0.0-test"))
}

fn page(path: &str) -> &'static Page {
    reference()
        .pages
        .iter()
        .find(|page| page.path == path)
        .unwrap_or_else(|| panic!("no page {path}"))
}

/// A page's section under a `##` heading, as Markdown.
fn part(page: &Page, heading: &str) -> String {
    let markdown = page.markdown();
    let start = markdown
        .find(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("{}: no {heading}", page.path));
    let rest = &markdown[start + heading.len() + 5..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    rest[..end].to_owned()
}

/// The JSON code blocks of a text.
fn json_blocks(markdown: &str) -> Vec<Value> {
    markdown
        .split("```json\n")
        .skip(1)
        .map(|rest| {
            let code = &rest[..rest.find("\n```").unwrap_or(rest.len())];
            serde_json::from_str(code).unwrap_or_else(|error| panic!("{error}: {code}"))
        })
        .collect()
}

#[test]
fn every_name_model_reference_lists_has_a_page() {
    let names = permitted();
    let reference = reference();
    let paths: BTreeSet<&str> = reference
        .pages
        .iter()
        .map(|page| page.path.as_str())
        .collect();
    assert_eq!(
        paths.len(),
        reference.pages.len(),
        "two pages with one path"
    );
    let index = page("index").markdown();
    for (folder, names) in [
        ("commands", &names.commands),
        ("queries", &names.queries),
        ("features", &names.features),
    ] {
        assert!(!names.is_empty());
        for name in names {
            let path = format!("{folder}/{name}");
            assert!(paths.contains(path.as_str()), "no page {path}");
            assert!(
                index.contains(&format!("({path}.md)")),
                "the index lacks {path}"
            );
        }
    }
    assert_eq!(reference.count(PageKind::Command), names.commands.len());
    assert_eq!(reference.count(PageKind::Query), names.queries.len());
    assert_eq!(reference.count(PageKind::Feature), names.features.len());
    assert!(reference.count(PageKind::Topic) > 100);
    // The search finds every page but the index.
    let files: BTreeMap<String, String> = reference.files().into_iter().collect();
    let search = &files["search-index.js"];
    for page in reference
        .pages
        .iter()
        .filter(|page| page.kind != PageKind::Index)
    {
        assert!(
            search.contains(&format!("\"{}.html\"", page.path)),
            "{}",
            page.path
        );
    }
    assert!(files["index.html"].contains("id=\"search\""));
    assert!(files.contains_key("style.css") && files.contains_key("search.js"));
    for page in &reference.pages {
        assert!(files.contains_key(&format!("{}.md", page.path)));
        let html = &files[&format!("{}.html", page.path)];
        assert!(html.starts_with("<!DOCTYPE html>"), "{}", page.path);
        assert!(html.contains("<title>"), "{}", page.path);
    }
}

#[test]
fn every_query_page_states_its_purpose() {
    // mitcad#2636: a query page opens with what the query is for, the
    // query catalog's sentence, not only the section it is in.
    let names = permitted();
    let mut found = Vec::new();
    for name in &names.queries {
        let page = page(&format!("queries/{name}"));
        let Some(purpose) = crate::query_purposes::query_purpose(name) else {
            found.push(format!("{name}: no purpose in query_purposes.rs"));
            continue;
        };
        let markdown = page.markdown();
        let opening = markdown
            .split("\n\n")
            .nth(1)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if opening != purpose {
            found.push(format!("{name}: opens with {opening:?}"));
        }
        if opening.starts_with("A query of") {
            found.push(format!("{name}: says only its section"));
        }
        // The index and the search show its first sentence.
        if page.summary.is_empty() || page.summary.starts_with("A query of") {
            found.push(format!("{name}: summary {:?}", page.summary));
        }
    }
    found.truncate(30);
    assert!(found.is_empty(), "{}", found.join("\n"));
    assert!(
        !reference()
            .gaps
            .iter()
            .any(|gap| gap.contains("no purpose")),
        "{:?}",
        reference().gaps
    );
}

#[test]
fn a_folder_with_stale_pages_is_told() {
    // mitcad#2636: the output folder is published as it is.
    let reference = reference();
    let paths = reference.file_paths();
    let written: BTreeSet<String> = reference
        .files()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(paths, written);
    let mut present: Vec<String> = paths.iter().cloned().collect();
    assert!(reference.foreign_files(&present).is_empty());
    present.push("queries/no_such_query.html".to_owned());
    present.push("notes.txt".to_owned());
    assert_eq!(
        reference.foreign_files(&present),
        vec![
            "notes.txt".to_owned(),
            "queries/no_such_query.html".to_owned()
        ]
    );
    // The files of a folder at any depth, by their paths from it; a folder
    // that is not there holds nothing.
    let sources = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = crate::api_reference::files_in(&sources).unwrap();
    for path in [
        "lib.rs",
        "api_reference/clean.rs",
        "api_reference/markdown.rs",
    ] {
        assert!(found.iter().any(|file| file == path), "{path}");
    }
    assert_eq!(
        crate::api_reference::files_in(&sources.join("no-such-folder")).ok(),
        Some(Vec::new())
    );
}

#[test]
fn no_page_names_a_ticket_history_a_file_or_a_test() {
    let reference = reference();
    let mut found = Vec::new();
    for page in &reference.pages {
        for problem in problems(&page.markdown()) {
            found.push(format!("{}: {problem}", page.path));
        }
        let html = page.html(&reference.version).to_lowercase();
        if html.contains("mitcad#") || html.contains("github #") {
            found.push(format!("{}: a ticket reference in its HTML", page.path));
        }
    }
    found.truncate(30);
    assert!(found.is_empty(), "{}", found.join("\n"));
    // What was left out is reported, for the reference's writers.
    let report = reference.report_text();
    assert!(report.starts_with("MitCad API reference 0.0.0-test: "));
    assert!(report.contains("## Left out: history"));
    assert!(!reference.omissions.is_empty());
}

/// The heading ids of each page as its HTML gives them.
fn ids() -> BTreeMap<String, BTreeSet<String>> {
    let reference = reference();
    reference
        .pages
        .iter()
        .map(|page| {
            let html = page.html(&reference.version);
            let ids = html
                .split(" id=\"")
                .skip(1)
                .filter_map(|rest| rest.split_once('"').map(|(id, _)| id.to_owned()))
                .collect();
            (page.path.clone(), ids)
        })
        .collect()
}

#[test]
fn the_links_find_their_pages_and_headings() {
    let reference = reference();
    let ids = ids();
    let mut broken = Vec::new();
    for page in &reference.pages {
        let markdown = page.markdown();
        // The links outside code: after `](` in the prose of lines that
        // are not in a code block.
        let mut fenced = false;
        let mut targets: Vec<String> = Vec::new();
        for line in markdown.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }
            for (code, part) in clean::spans(line) {
                if code {
                    continue;
                }
                for link in part.split("](").skip(1) {
                    if let Some(end) = link.find(')') {
                        targets.push(link[..end].to_owned());
                    }
                }
            }
        }
        for target in &targets {
            if target.starts_with("https://") {
                continue;
            }
            let target = target.trim_start_matches("../");
            let (file, fragment) = target.split_once('#').unwrap_or((target, ""));
            let Some(path) = file.strip_suffix(".md") else {
                broken.push(format!("{}: {target}", page.path));
                continue;
            };
            match ids.get(path) {
                None => broken.push(format!("{}: {target}", page.path)),
                Some(known) if !fragment.is_empty() && !known.contains(fragment) => {
                    broken.push(format!("{}: {target}", page.path));
                }
                _ => {}
            }
        }
    }
    broken.truncate(30);
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[test]
fn the_overview_tells_how_the_api_works() {
    let overview = page("overview").markdown();
    for heading in [
        "Commands",
        "Queries",
        "Preview",
        "Undo, revisions and the saved state",
        "Values",
        "Identities and names",
    ] {
        assert!(OVERVIEW.contains(&heading));
        assert!(overview.contains(&format!("\n## {heading}\n")), "{heading}");
    }
    assert!(page("index").markdown().contains("(overview.md)"));
}

#[test]
fn add_feature_with_an_extrude_reads_cleanly() {
    let page = page("commands/add_feature");
    assert_eq!(page.kind, PageKind::Command);
    let fields = part(page, "Fields");
    for field in ["def", "name", "component", "after", "index"] {
        assert!(
            fields.contains(&format!("| `{field}` |")),
            "{field}: {fields}"
        );
    }
    assert!(
        fields.contains("| `def` | feature definition |"),
        "{fields}"
    );
    let examples = json_blocks(&part(page, "Example"));
    assert!(
        examples
            .iter()
            .any(|example| example["cmd"] == "add_feature" && example["def"]["type"] == "extrude"),
        "{examples:?}"
    );
    assert!(part(page, "Result").contains("`uid`"));
    let extrude = page_of_feature("extrude");
    assert!(part(extrude, "Fields").contains("| `profiles` |"));
    let examples = json_blocks(&part(extrude, "Example"));
    assert_eq!(examples[0]["cmd"], "add_feature");
    assert_eq!(examples[0]["def"]["type"], "extrude");
}

fn page_of_feature(name: &str) -> &'static Page {
    page(&format!("features/{name}"))
}

#[test]
fn sketch_add_line_reads_cleanly() {
    let page = page("commands/sketch.add_line");
    let fields = part(page, "Fields");
    for field in ["sketch", "start", "end", "construction", "mode"] {
        assert!(
            fields.contains(&format!("| `{field}` |")),
            "{field}: {fields}"
        );
    }
    let examples = json_blocks(&part(page, "Example"));
    assert!(!examples.is_empty());
    for example in examples {
        assert_eq!(example["cmd"], "sketch.add_line");
    }
}

#[test]
fn the_bodies_and_measure_queries_read_cleanly() {
    let bodies = page("queries/bodies");
    assert_eq!(bodies.kind, PageKind::Query);
    let fields = part(bodies, "Fields");
    for field in ["properties", "volumes", "bbox"] {
        assert!(fields.contains(&format!("`{field}`")), "{field}: {fields}");
    }
    let examples = json_blocks(&part(bodies, "Example"));
    assert!(
        examples.iter().any(|example| example["query"] == "bodies"),
        "{examples:?}"
    );
    let result = part(bodies, "Result");
    assert!(
        result.contains("`volume`") && result.contains("`uid`"),
        "{result}"
    );

    let measure = page("queries/measure");
    let fields = part(measure, "Fields");
    assert!(
        fields.contains("| `a` |") && fields.contains("| `b` |"),
        "{fields}"
    );
    let examples = json_blocks(&part(measure, "Example"));
    assert!(
        examples
            .iter()
            .any(|example| example["query"] == "measure" && example["a"]["body"] == "F2.b0"),
        "{examples:?}"
    );
    assert!(part(measure, "Result").contains("`between`"));
}
