// SPDX-License-Identifier: MIT
//! The standards and suppliers of a library's configuration rows
//! (mitcad#949): a manifest with `suppliers` and the components' `rows`
//! (`standards`, `supplier_parts`) read and checked (`library check`
//! refuses an article of an unknown supplier and a date that is none),
//! answered by `library_show`, `library_configure` and `library_search`; a
//! manifest of today without them reads as before; and a library an index
//! offers whose repository an enabled source has, or whose id a fetched
//! library replaces, is not listed beside it. Nothing goes over the
//! network; the parts that fetch are skipped without git.

use serde_json::{Value, json};

use super::remote::{slashes, system_git, tip};
use super::*;
use crate::library::manifest::{Manifest, valid_date};
use crate::library::{LibraryCache, api};
use crate::remote::GitCli;

fn library_command(cache: &LibraryCache, git: Option<&GitCli>, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, git, None).unwrap()).unwrap()
}

/// A socket head screw of three rows: M6x6 (no standard), M6x8 (DIN 912
/// only) and M6x10 (both standards).
fn screw() -> String {
    let mut doc = Document::new(MockKernel::default());
    for command in [
        json!({"cmd": "add_parameter", "name": "d", "expression": "6 mm"}),
        json!({"cmd": "add_parameter", "name": "L", "expression": "10 mm"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": "d", "height": "d"}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
            "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}}),
        json!({"cmd": "set_configurations", "configurations": {
        "selectors": ["Size", "Length"], "parameters": ["d", "L"], "default": "M6x10",
        "rows": [
            {"name": "M6x6", "select": {"Size": "M6", "Length": "6"}, "values": {"d": "6 mm", "L": "6 mm"}},
            {"name": "M6x8", "select": {"Size": "M6", "Length": "8"}, "values": {"d": "6 mm", "L": "8 mm"}},
            {"name": "M6x10", "select": {"Size": "M6", "Length": "10"}, "values": {"d": "6 mm", "L": "10 mm"}}
        ]}}),
    ] {
        doc.command(&command.to_string()).unwrap();
    }
    doc.to_json()
}

fn article(supplier: &str, number: &str) -> Value {
    json!({"supplier": supplier, "part_number": number, "material": "8.8", "finish": "zinc-plated",
           "checked": "2026-10-30"})
}

/// The manifest of a library with the screw: its rows tagged, two
/// suppliers.
fn tagged_manifest(id: &str) -> Value {
    json!({"format": "mitcad-library", "version": 1, "id": id, "name": "Test screws",
    "license": "MIT",
    "suppliers": [{"id": "acme", "name": "Acme", "url": "https://acme.example.org"},
                  {"id": "bolts-r-us", "name": "Bolts R Us"}],
    "categories": [{"id": "screws", "name": "General fasteners/Screws"}],
    "components": [
        {"id": "iso4762", "path": "screws/iso4762.mitcad", "name": "Hexagon socket head cap screw",
         "category": "screws", "standard": "ISO 4762 / DIN 912",
         "designation": "{standard} {Size}x{Length}",
         "rows": {
             "M6x6": {"standards": ["non-standard"], "supplier_parts": [article("acme", "A 6 6")]},
             "M6x8": {"standards": ["DIN 912"], "supplier_parts": [article("acme", "A 6 8")]},
             "M6x10": {"standards": ["ISO 4762", "DIN 912"],
                       "supplier_parts": [article("acme", "A 6 10"), article("bolts-r-us", "BRU-610")]}
         }}
    ]})
}

/// A library folder with the screw and `manifest`, recorded with git and
/// tagged `v1.0.0` when `record`.
fn write_library(dir: &Path, manifest: &Value, record: bool) {
    fs::create_dir_all(dir.join("screws")).unwrap();
    fs::write(dir.join("mitcad-library.json"), manifest.to_string()).unwrap();
    fs::write(dir.join("LICENSE"), "MIT\n").unwrap();
    fs::write(dir.join("screws/iso4762.mitcad"), screw()).unwrap();
    if record {
        git(dir, &["init", "-q"]).unwrap();
        git(dir, &["add", "-A"]).unwrap();
        git(dir, &["commit", "-q", "-m", "Screws"]).unwrap();
        git(dir, &["tag", "v1.0.0"]).unwrap();
    }
}

#[test]
fn rows_standards_and_suppliers_are_read_and_checked() {
    let manifest: Manifest = serde_json::from_value(tagged_manifest("test-screws")).unwrap();
    let problems = manifest.problems();
    assert_eq!(problems.errors, Vec::<String>::new());
    let screw = manifest.component("iso4762").unwrap();
    assert_eq!(screw.rows.len(), 3);
    assert_eq!(
        screw.rows["M6x10"].standards,
        vec!["ISO 4762".to_owned(), "DIN 912".to_owned()]
    );
    // The standards in the component's order (ISO before DIN), whatever
    // the rows' order; non-standard last.
    let order = ["M6x6", "M6x8", "M6x10"].map(str::to_owned);
    assert_eq!(
        screw.row_standards(&order),
        vec!["ISO 4762", "DIN 912", "non-standard"]
    );
    let suppliers: Vec<&str> = screw
        .row_suppliers(&manifest.suppliers)
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(suppliers, ["acme", "bolts-r-us"]);
    // The articles with their suppliers' names.
    let parts = manifest.named_supplier_parts(screw, "M6x10");
    assert_eq!(parts[1].supplier_name, "Bolts R Us");
    assert_eq!(parts[0].text(), "Acme A 6 10 (8.8 zinc-plated)");
    // Written back as read.
    let again: Manifest = serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
    assert_eq!(again, manifest);

    // An article of a supplier the library does not list, a date that is
    // none, an empty part number: errors.
    let mut broken = tagged_manifest("test-screws");
    broken["components"][0]["rows"]["M6x8"]["supplier_parts"] = json!([
        {"supplier": "nobody", "part_number": "N 1", "checked": "2026-10-30"},
        {"supplier": "acme", "part_number": " ", "checked": "30.10.2026"}
    ]);
    broken["suppliers"][1]["url"] = json!("http://bolts.example.org");
    let manifest: Manifest = serde_json::from_value(broken).unwrap();
    let errors = manifest.problems().errors.join("\n");
    assert!(
        errors.contains(
            "iso4762: the row M6x8's supplier_parts[0]: the supplier 'nobody' is not one of the \
             library's suppliers"
        ),
        "{errors}"
    );
    assert!(
        errors.contains("supplier_parts[1]: checked '30.10.2026' is no date (YYYY-MM-DD)"),
        "{errors}"
    );
    assert!(
        errors.contains("supplier_parts[1]: no part number"),
        "{errors}"
    );
    assert!(
        errors.contains("suppliers[1]: the url 'http://bolts.example.org' is no https:// address"),
        "{errors}"
    );
    assert!(valid_date("2026-10-30") && !valid_date("2026-13-01") && !valid_date("26-10-30"));

    // A manifest of today: no rows, no suppliers, the same problems as
    // before; unknown fields are still ignored.
    let today: Manifest = serde_json::from_value(json!({
        "format": "mitcad-library", "version": 1, "id": "test-old", "name": "Old", "license": "MIT",
        "components": [{"id": "pin", "path": "pins/pin.mitcad", "name": "Pin", "standard": "TP 1",
                        "future_field": {"x": 1}}],
        "another_future_field": true}))
    .unwrap();
    assert!(today.problems().is_ok());
    let pin = today.component("pin").unwrap();
    assert!(pin.rows.is_empty() && today.suppliers.is_empty());
    assert!(pin.row_standards(&[]).is_empty());
    let written = serde_json::to_value(&today).unwrap();
    assert_eq!(written.get("suppliers"), None);
    assert_eq!(written["components"][0].get("rows"), None);
}

#[test]
fn library_check_refuses_an_unknown_supplier() {
    let scratch = Scratch::new("library-rows-check");
    let cache = LibraryCache::new(scratch.join("cache"));
    let good = scratch.join("good");
    write_library(&good, &tagged_manifest("test-screws"), false);
    let answer = library_command(&cache, None, json!({"cmd": "library_check", "dir": good}));
    assert_eq!(answer["errors"], json!([]), "{answer}");
    // An unknown supplier's article, a row the design does not have.
    let bad = scratch.join("bad");
    let mut manifest = tagged_manifest("test-screws");
    manifest["components"][0]["rows"]["M6x10"]["supplier_parts"][0]["supplier"] = json!("wurth");
    manifest["components"][0]["rows"]["M7x99"] = json!({"standards": ["DIN 912"]});
    write_library(&bad, &manifest, false);
    let answer = library_command(&cache, None, json!({"cmd": "library_check", "dir": bad}));
    assert_eq!(answer["ok"], false, "{answer}");
    let errors = answer["errors"].to_string();
    assert!(
        errors.contains("the supplier 'wurth' is not one of the library's suppliers"),
        "{errors}"
    );
    assert!(
        errors.contains(
            "iso4762: rows names M7x99, which is no configuration row of screws/iso4762.mitcad"
        ),
        "{errors}"
    );
}

#[test]
fn show_configure_and_search_answer_the_rows_standards_and_suppliers() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-rows-show");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("screws");
    write_library(&dir, &tagged_manifest("test-screws"), true);
    let url = slashes(&dir);
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");

    // library_show: each row's standards and articles, the component's
    // standards and suppliers.
    let shown = library_command(&cache, None, json!({"cmd": "library_show", "url": url}));
    let component = &shown["components"][0];
    assert_eq!(
        component["standards"],
        json!(["ISO 4762", "DIN 912", "non-standard"]),
        "{component}"
    );
    assert_eq!(component["suppliers"][0]["name"], "Acme");
    assert_eq!(component["suppliers"][1]["id"], "bolts-r-us");
    let rows = component["configurations"]["rows"].as_array().unwrap();
    let row = |name: &str| rows.iter().find(|r| r["name"] == name).unwrap().clone();
    assert_eq!(row("M6x8")["standards"], json!(["DIN 912"]));
    assert_eq!(row("M6x10")["supplier_parts"][1]["part_number"], "BRU-610");
    assert_eq!(
        row("M6x10")["supplier_parts"][1]["supplier_name"],
        "Bolts R Us"
    );
    assert_eq!(row("M6x6")["standards"], json!(["non-standard"]));
    assert_eq!(
        shown["manifest"]["suppliers"][0]["url"],
        "https://acme.example.org"
    );

    // library_configure: the designation follows the standard asked for.
    let configure = |config: &str, standard: Option<&str>| {
        let mut command = json!({"cmd": "library_configure", "url": url, "component": "iso4762",
                                 "config": config});
        if let Some(standard) = standard {
            command["standard"] = json!(standard);
        }
        library_command(&cache, None, command)
    };
    let all = configure("M6x10", None);
    assert_eq!(all["designation"], "ISO 4762 M6x10", "{all}");
    assert_eq!(all["standard"], "ISO 4762");
    assert_eq!(all["supplier_parts"].as_array().unwrap().len(), 2);
    let din = configure("M6x10", Some("DIN 912"));
    assert_eq!(din["designation"], "DIN 912 M6x10", "{din}");
    assert_eq!(din["standard"], "DIN 912");
    let rows = din["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["name"], "M6x8");
    assert_eq!(rows[1]["select"]["Length"], "8");
    assert_eq!(rows[1]["standards"], json!(["DIN 912"]));
    let odd = configure("M6x6", Some("DIN 912"));
    assert_eq!(odd["designation"], "M6x6 (non-standard)", "{odd}");

    // library_search: the components a supplier has articles of; their
    // suppliers and standards.
    let search = |command: Value| library_command(&cache, None, command)["components"].clone();
    let found = search(json!({"cmd": "library_search", "supplier": "bolts-r-us"}));
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found[0]["suppliers"][1]["id"], "bolts-r-us");
    assert_eq!(found[0]["standards"][0], "ISO 4762");
    let found = search(json!({"cmd": "library_search", "supplier": "Acme"}));
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    let found = search(json!({"cmd": "library_search", "supplier": "nobody"}));
    assert_eq!(found, json!([]));
    // A supplier's part number finds its component.
    let found = search(json!({"cmd": "library_search", "part_number": "bru-610"}));
    assert_eq!(found[0]["id"], "iso4762", "{found}");
}

#[test]
fn an_index_library_of_a_sources_repository_is_left_out() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-rows-index");
    let cache = LibraryCache::new(scratch.join("cache"));
    // The parts repository: v1.0.0 was the fastener library
    // `test-fasteners`; v2.0.0 is `test-parts`, which replaces it.
    let parts = scratch.join("parts");
    write_library(&parts, &tagged_manifest("test-fasteners"), true);
    let v1 = tip(&parts, "v1.0.0^{}");
    let mut manifest = tagged_manifest("test-parts");
    manifest["replaces"] = json!(["test-fasteners"]);
    fs::write(parts.join("mitcad-library.json"), manifest.to_string()).unwrap();
    git(&parts, &["commit", "-q", "-am", "One library of parts"]).unwrap();
    git(&parts, &["tag", "v2.0.0"]).unwrap();
    // A clone of the old fastener library, never moved past v1.0.0 (a
    // source the user had before).
    let old = scratch.join("old-fasteners");
    git(&parts, &["clone", "-q", &slashes(&parts), &slashes(&old)]).unwrap();
    git(&old, &["reset", "-q", "--hard", &v1]).unwrap();
    git(&old, &["tag", "-d", "v2.0.0"]).unwrap();
    // An index that still offers the old library under its old address
    // (which redirects to the parts repository) with the version it
    // reviewed, and another library.
    let index = scratch.join("index");
    fs::create_dir_all(index.join("libraries")).unwrap();
    fs::write(
        index.join("mitcad-index.json"),
        json!({"format": "mitcad-index", "version": 1, "name": "Test index"}).to_string(),
    )
    .unwrap();
    fs::write(
        index.join("libraries/test-fasteners.json"),
        json!({"format": "mitcad-index-entry", "version": 1, "id": "test-fasteners",
               "name": "Test fasteners", "license": "MIT",
               "url": "https://example.invalid/test/fasteners.git",
               "reviewed": [{"rev": v1, "label": "v1.0.0"}]})
        .to_string(),
    )
    .unwrap();
    fs::write(
        index.join("libraries/brackets.json"),
        json!({"format": "mitcad-index-entry", "version": 1, "id": "brackets", "name": "Brackets",
               "license": "MIT", "url": "https://example.invalid/brackets.git"})
        .to_string(),
    )
    .unwrap();
    git(&index, &["init", "-q"]).unwrap();
    git(&index, &["add", "-A"]).unwrap();
    git(&index, &["commit", "-q", "-m", "Index"]).unwrap();
    for dir in [&parts, &old, &index] {
        let fetched = library_command(
            &cache,
            Some(&cli),
            json!({"cmd": "library_fetch", "url": slashes(dir)}),
        );
        assert_eq!(fetched["error"], Value::Null, "{fetched}");
    }
    // The enabled sources: the parts repository and the index.
    let sources = json!([slashes(&parts), slashes(&index)]);
    let answer = library_command(
        &cache,
        None,
        json!({"cmd": "library_search", "sources": sources}),
    );
    let ids = |key: &str| -> Vec<String> {
        answer[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.get("library")
                    .or_else(|| v.get("id"))
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_owned()
            })
            .collect()
    };
    // Only the parts library's screw: no group of the old clone's.
    assert_eq!(ids("components"), ["test-parts"], "{answer}");
    // The index's old library is not offered; the other one is.
    assert_eq!(ids("libraries"), ["brackets"], "{answer}");
    let left: Vec<&str> = answer["left_out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap())
        .collect();
    assert!(
        left.iter()
            .any(|t| t.starts_with("test-fasteners left out: superseded by test-parts")),
        "{left:?}"
    );
    assert_eq!(left.len(), 2, "{left:?}");

    // Without `replaces`: the same repository as a source's, by the
    // version the index reviewed.
    let mut plain = tagged_manifest("test-parts");
    plain["name"] = json!("Test parts");
    fs::write(parts.join("mitcad-library.json"), plain.to_string()).unwrap();
    git(&parts, &["commit", "-q", "-am", "No replaces"]).unwrap();
    git(&parts, &["tag", "v2.1.0"]).unwrap();
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&parts)}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    let answer = library_command(
        &cache,
        None,
        json!({"cmd": "library_search", "sources": sources}),
    );
    let texts = answer["left_out"].to_string();
    assert!(
        texts.contains("test-fasteners left out: same repository as "),
        "{texts}"
    );
    assert_eq!(answer["libraries"].as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(
        answer["components"].as_array().unwrap().len(),
        1,
        "{answer}"
    );
}
