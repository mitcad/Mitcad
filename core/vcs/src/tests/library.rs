// SPDX-License-Identifier: MIT
//! Component libraries (mitcad#64) and the community library (mitcad#63)
//! in temporary folders: a library made with the publishing commands,
//! recorded with git and tagged, fetched from its folder and by a file://
//! URL into a cache, its versions listed and read, its parts inserted and
//! updated through the resolver, versions compared, and a community index
//! searched. Nothing goes over the network; without git these tests are
//! skipped.

use serde_json::{Value, json};

use std::sync::Arc;

use super::remote::{error_class, slashes, system_git, tip};
use super::*;
use crate::library::{LibraryCache, LibraryRepo, LibraryResolver, api};
use mitcad_model::{InsertOptions, LibraryChange, LibraryPart};

/// A PNG file's signature and a little more: enough to be taken for one.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n preview";

fn library_command(cache: &LibraryCache, git: Option<&GitCli>, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, git, None).unwrap()).unwrap()
}

use crate::remote::GitCli;

/// A pin: a `d` x `d` block `L` high with a table of three sizes; the
/// second version makes M4x16 18 mm long.
fn pin(newer: bool) -> String {
    let mut doc = Document::new(MockKernel::default());
    let l16 = if newer { "18 mm" } else { "16 mm" };
    for command in [
        json!({"cmd": "add_parameter", "name": "d", "expression": "4 mm"}),
        json!({"cmd": "add_parameter", "name": "L", "expression": "10 mm"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": "d", "height": "d"}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
            "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}}),
        json!({"cmd": "set_configurations", "configurations": {
        "selectors": ["Size", "Length"], "parameters": ["d", "L"], "default": "M4x10",
        "rows": [
            {"name": "M4x10", "select": {"Size": "M4", "Length": "10"}, "values": {"d": "4 mm", "L": "10 mm"}},
            {"name": "M4x16", "select": {"Size": "M4", "Length": "16"}, "values": {"d": "4 mm", "L": l16}},
            {"name": "M10x20", "select": {"Size": "M10", "Length": "20"}, "values": {"d": "10 mm", "L": "20 mm"}}
        ]}}),
    ] {
        doc.command(&command.to_string()).unwrap();
    }
    doc.to_json()
}

/// A library made with the publishing commands in `scratch/pins`, recorded
/// and tagged `v1.0.0`, then `v1.1.0` with the longer M4x16.
fn make_library(scratch: &Scratch, cache: &LibraryCache) -> PathBuf {
    let dir = scratch.join("pins");
    let answer = library_command(
        cache,
        None,
        json!({"cmd": "library_init", "dir": dir, "id": "test-pins", "name": "Test pins",
               "description": "Square pins for tests.", "license": "CC0-1.0",
               "authors": ["Pin makers"]}),
    );
    assert_eq!(answer["id"], "test-pins", "{answer}");
    assert!(dir.join(".mitcad/project.json").is_file());
    assert!(
        fs::read_to_string(dir.join("LICENSE"))
            .unwrap()
            .contains("CC0-1.0")
    );
    let added = library_command(
        cache,
        None,
        json!({"cmd": "library_add", "dir": dir, "text": pin(false), "id": "pin",
               "name": "Square pin", "category": "pins", "standard": "TP 1",
               "keywords": ["dowel", "square"], "preview": mitcad_model::library::encode_base64(PNG)}),
    );
    assert_eq!(
        added["written"],
        json!(["pins/pin.mitcad", "previews/pin.png", "mitcad-library.json"]),
        "{added}"
    );
    assert_eq!(added["errors"], json!([]), "{added}");
    let checked = library_command(cache, None, json!({"cmd": "library_check", "dir": dir}));
    assert_eq!(checked["ok"], true, "{checked}");
    git(&dir, &["add", "-A"]).unwrap();
    git(&dir, &["commit", "-q", "-m", "Pins 1.0"]).unwrap();
    git(&dir, &["tag", "-a", "v1.0.0", "-m", "1.0"]).unwrap();
    library_command(
        cache,
        None,
        json!({"cmd": "library_add", "dir": dir, "text": pin(true), "id": "pin",
               "name": "Square pin", "category": "pins", "standard": "TP 1",
               "keywords": ["dowel", "square"], "preview": mitcad_model::library::encode_base64(PNG)}),
    );
    git(&dir, &["commit", "-q", "-am", "M4x16 is 18 mm long"]).unwrap();
    git(&dir, &["tag", "v1.1.0"]).unwrap();
    dir
}

/// A folder's file:// URL (`file:///C:/...` on Windows).
fn file_url(dir: &Path) -> String {
    let path = slashes(dir);
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

/// A height of a body of the mock: its extrusion's extent from the
/// history kept in its B-rep data (`prism(F3:0..16)`).
fn height_of(doc: &Document<MockKernel>, component: mitcad_model::ComponentUid) -> f64 {
    let body = doc.component_bodies(component)[0];
    let history = &doc.body_shape(body).unwrap().history;
    let at = history.find("prism(").unwrap();
    let range = &history[at..];
    let range = &range[range.find(':').unwrap() + 1..range.find(')').unwrap()];
    let (start, end) = range.split_once("..").unwrap();
    end.parse::<f64>().unwrap() - start.parse::<f64>().unwrap()
}

#[test]
fn libraries_are_fetched_and_read_at_their_versions() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library");
    let cache = LibraryCache::new(scratch.join("cache"));
    let source = make_library(&scratch, &cache);
    let url = file_url(&source);

    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(
        (fetched["kind"].as_str(), fetched["id"].as_str()),
        (Some("library"), Some("test-pins"))
    );
    assert_eq!(fetched["cloned"], true);
    let labels: Vec<&str> = fetched["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["v1.1.0", "v1.0.0"], "{fetched}");
    let dir = PathBuf::from(fetched["dir"].as_str().unwrap());
    assert!(dir.starts_with(&cache.root) && dir.extension().unwrap() == "git");
    assert!(
        fetched["log"][0].as_str().unwrap().contains("clone --bare"),
        "{fetched}"
    );

    // What the library has at its newest version.
    let shown = library_command(&cache, None, json!({"cmd": "library_show", "url": url}));
    assert_eq!(shown["labels"], json!(["v1.1.0"]), "{shown}");
    let pin = &shown["components"][0];
    assert_eq!(pin["license"], "CC0-1.0");
    assert_eq!(pin["configurations"]["default"], "M4x10");
    assert_eq!(
        pin["configurations"]["selector_values"],
        json!({"Size": ["M4", "M10"], "Length": ["10", "16", "20"]})
    );
    let listed = library_command(&cache, None, json!({"cmd": "library_list"}));
    assert_eq!(listed["libraries"][0]["id"], "test-pins", "{listed}");
    let preview = library_command(
        &cache,
        None,
        json!({"cmd": "library_preview", "id": "test-pins", "component": "pin", "rev": "v1.0.0"}),
    );
    assert_eq!(
        mitcad_model::library::decode_base64(preview["png"].as_str().unwrap()).unwrap(),
        PNG
    );

    // Search by words, standard and licence.
    let search = |command: Value| library_command(&cache, None, command)["components"].clone();
    assert_eq!(
        search(json!({"cmd": "library_search", "text": "dowel"}))[0]["id"],
        "pin"
    );
    assert_eq!(
        search(json!({"cmd": "library_search", "text": "tp 1 square"}))[0]["version"],
        "v1.1.0 (".to_owned() + &tip(&source, "v1.1.0^{}")[..7] + ")"
    );
    assert_eq!(
        search(json!({"cmd": "library_search", "text": "bolt"})),
        json!([])
    );
    assert_eq!(
        search(json!({"cmd": "library_search", "text": "pin", "licenses": ["CC-BY-4.0"]})),
        json!([])
    );
    assert_eq!(
        search(json!({"cmd": "library_search", "text": "pin", "licenses": ["cc0-1.0"]}))[0]["id"],
        "pin"
    );

    // A design inserts the part at v1.0.0 through the resolver: the commit
    // is recorded and read again on open even after the library moved on.
    let resolver = Arc::new(LibraryResolver::new(cache.clone()));
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(resolver.clone()));
    let part = LibraryPart {
        library: "test-pins".to_owned(),
        url: url.clone(),
        rev: "v1.0.0".to_owned(),
        component: "pin".to_owned(),
        config: Some("M4x16".to_owned()),
    };
    let (component, _) = doc
        .insert_library_component(&part, &InsertOptions::default())
        .unwrap();
    let v1 = tip(&source, "v1.0.0^{}");
    let v2 = tip(&source, "v1.1.0^{}");
    let reference = doc
        .assembly()
        .component(component)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(
        (reference.rev.as_str(), reference.label.as_str()),
        (v1.as_str(), "v1.0.0")
    );
    assert_eq!(reference.designation, "TP 1 M4x16");
    assert_eq!(reference.path, "pins/pin.mitcad");
    assert!((height_of(&doc, component) - 16.0).abs() < 1e-9);
    let saved = doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.set_link_resolver(Some(resolver.clone()));
    assert_eq!(reopened.update_links(None).unwrap(), Vec::<String>::new());

    // What updating to v1.1.0 changes, then the update.
    let diff = library_command(
        &cache,
        None,
        json!({"cmd": "library_diff", "id": "test-pins", "url": url, "from": v1, "to": "v1.1.0",
               "parts": [{"component": "pin", "config": "M4x16"}, {"component": "pin", "config": "M10x20"},
                         {"component": "gone", "config": null}]}),
    );
    assert_eq!(diff["to_labels"], json!(["v1.1.0"]), "{diff}");
    assert_eq!(diff["parts"][0]["changed"], true, "{diff}");
    assert_eq!(
        diff["parts"][0]["lines"][0], "M4x16: L 16 mm -> 18 mm",
        "{diff}"
    );
    // Only the part's row: other rows and the table are not the part's.
    assert_eq!(
        diff["parts"][0]["text"], "M4x16: L 16 mm -> 18 mm",
        "{diff}"
    );
    assert_eq!(diff["parts"][1]["text"], "no changes", "{diff}");
    assert_eq!(diff["parts"][2]["missing"], true, "{diff}");
    doc.update_library_parts(&[LibraryChange {
        component,
        rev: Some("v1.1.0".to_owned()),
        config: None,
    }])
    .unwrap();
    assert!((height_of(&doc, component) - 18.0).abs() < 1e-9);
    let reference = doc
        .assembly()
        .component(component)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(reference.rev, v2);

    // A new commit upstream: a second fetch brings it as the unreleased
    // tip; the versions designs use stay.
    let file = source.join("README.md");
    fs::write(&file, "# Test pins\n\nMore to come.\n").unwrap();
    git(&source, &["commit", "-q", "-am", "Readme"]).unwrap();
    let again = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    assert_eq!(
        (again["error"].clone(), again["cloned"].clone()),
        (Value::Null, json!(false)),
        "{again}"
    );
    let versions = again["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 3, "{again}");
    assert_eq!(
        (versions[2]["label"].as_str(), versions[2]["head"].as_bool()),
        (Some(""), Some(true))
    );
    let mut later = Document::from_json(&saved, MockKernel::default()).unwrap();
    later.set_link_resolver(Some(resolver));
    assert_eq!(later.update_links(None).unwrap(), Vec::<String>::new());

    // The same library from its folder's path: another clone, found by
    // its id for designs that recorded another URL.
    let by_path = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&source)}),
    );
    assert_eq!(by_path["error"], Value::Null, "{by_path}");
    assert_ne!(by_path["dir"], fetched["dir"]);
    let (repo, commit) = cache
        .find("test-pins", "https://example.org/elsewhere.git", &v1)
        .unwrap();
    assert_eq!(commit.to_string(), v1);
    assert!(repo.dir().starts_with(&cache.root));
}

#[test]
fn fetches_refuse_what_is_no_library() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-refused");
    let cache = LibraryCache::new(scratch.join("cache"));
    // A repository without a manifest.
    let plain = scratch.join("plain");
    fs::create_dir_all(&plain).unwrap();
    git(&plain, &["init", "-q"]).unwrap();
    fs::write(plain.join("notes.txt"), "notes\n").unwrap();
    git(&plain, &["add", "-A"]).unwrap();
    git(&plain, &["commit", "-q", "-m", "Notes"]).unwrap();
    let answer = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&plain)}),
    );
    assert_eq!(error_class(&answer), "not_a_project", "{answer}");
    assert!(cache.repos().is_empty());
    // Credentials in the URL, and a folder that is not there.
    let answer = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": "https://user:secret@example.org/lib.git"}),
    );
    assert_eq!(error_class(&answer), "invalid_url", "{answer}");
    assert!(!answer.to_string().contains("secret"));
    let answer = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&scratch.join("missing"))}),
    );
    assert_ne!(answer["error"], Value::Null, "{answer}");
    assert!(cache.repos().is_empty());
}

#[test]
fn manifests_and_library_folders_are_checked() {
    let scratch = Scratch::new("library-check");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("broken");
    fs::create_dir_all(dir.join("parts")).unwrap();
    fs::write(
        dir.join("mitcad-library.json"),
        json!({"format": "mitcad-library", "version": 1, "id": "Broken Library", "name": "",
        "components": [
            {"id": "a", "path": "../outside.mitcad", "name": "A"},
            {"id": "b-part", "path": "parts/b.mitcad", "name": "B", "preview": "b.png",
             "license": "Proprietary"},
            {"id": "b-part", "path": "parts/c.mitcad", "name": "C"}
        ]})
        .to_string(),
    )
    .unwrap();
    // B links another file; C is missing.
    let text = {
        let mut value: Value =
            serde_json::from_str(&Document::new(MockKernel::default()).to_json()).unwrap();
        value["components"] = json!([{"uid": "C1", "name": "other",
            "link": {"path": "other.mitcad", "digest": "0", "feature": "F1"}}]);
        value["features"] = json!([{"uid": "F1", "name": "Base1", "type": "base", "component": "C1",
            "bodies": [{"brep": serde_json::from_str::<Value>(&brep("6 bolt")).unwrap()}]}]);
        value["occurrences"] =
            json!([{"uid": "O1", "component": "C1", "parent": "C0", "number": 1}]);
        value.to_string()
    };
    fs::write(dir.join("parts/b.mitcad"), text).unwrap();
    let answer = library_command(&cache, None, json!({"cmd": "library_check", "dir": dir}));
    assert_eq!(answer["ok"], false);
    let errors: Vec<&str> = answer["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap())
        .collect();
    let has = |text: &str| errors.iter().any(|e| e.contains(text));
    assert!(has("'Broken Library' is no library id"), "{errors:?}");
    assert!(has("the library has no name"), "{errors:?}");
    assert!(has("a: '../outside.mitcad' is no path"), "{errors:?}");
    assert!(has("b-part: the id is used twice"), "{errors:?}");
    assert!(
        has("b-part: parts/b.mitcad links other files"),
        "{errors:?}"
    );
    assert!(has("b-part: b.png is not in the folder"), "{errors:?}");
    assert!(
        has("b-part: parts/c.mitcad is not in the folder"),
        "{errors:?}"
    );
    let warnings = answer["warnings"].to_string();
    assert!(
        warnings.contains("the licence Proprietary is not one a community index accepts"),
        "{warnings}"
    );
    assert!(warnings.contains("no LICENSE file"), "{warnings}");
}

#[test]
fn a_community_index_lists_libraries_to_fetch() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-index");
    let cache = LibraryCache::new(scratch.join("cache"));
    let library = make_library(&scratch, &cache);
    let url = file_url(&library);
    // The entry the library's maintainers propose to an index.
    let entry = library_command(
        &cache,
        None,
        json!({"cmd": "index_entry", "dir": library, "url": url}),
    );
    assert_eq!(entry["path"], "libraries/test-pins.json", "{entry}");
    assert_eq!(entry["labels"], json!(["v1.1.0"]));
    assert_eq!(
        entry["entry"]["components"][0]["tags"],
        json!(["dowel", "square"])
    );
    // An index with it and with a library without a licence.
    let index = scratch.join("index");
    fs::create_dir_all(index.join("libraries")).unwrap();
    fs::write(
        index.join("mitcad-index.json"),
        json!({"format": "mitcad-index", "version": 1, "name": "Test index"}).to_string(),
    )
    .unwrap();
    let mut reviewed = entry["entry"].clone();
    reviewed["reviewed"] = json!([{"rev": tip(&library, "v1.1.0^{}"), "label": "v1.1.0"}]);
    fs::write(index.join("libraries/test-pins.json"), reviewed.to_string()).unwrap();
    fs::write(
        index.join("libraries/brackets.json"),
        json!({"format": "mitcad-index-entry", "version": 1, "id": "brackets", "name": "Brackets",
               "url": "https://example.org/brackets.git", "tags": ["bracket"]})
        .to_string(),
    )
    .unwrap();
    fs::write(index.join("libraries/broken.json"), "{").unwrap();
    git(&index, &["init", "-q"]).unwrap();
    git(&index, &["add", "-A"]).unwrap();
    git(&index, &["commit", "-q", "-m", "Index"]).unwrap();
    let fetched = library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": slashes(&index)}),
    );
    assert_eq!(
        (fetched["error"].clone(), fetched["kind"].clone()),
        (Value::Null, json!("index")),
        "{fetched}"
    );
    assert_eq!(fetched["name"], "Test index");
    assert!(
        fetched["problems"]["warnings"][0]
            .as_str()
            .unwrap()
            .contains("broken.json"),
        "{fetched}"
    );
    let shown = library_command(
        &cache,
        None,
        json!({"cmd": "library_show", "url": slashes(&index)}),
    );
    assert_eq!(shown["entries"].as_array().unwrap().len(), 2, "{shown}");

    let search = |command: Value| library_command(&cache, None, command)["libraries"].clone();
    // Items without a licence are hidden unless asked for.
    let found = search(json!({"cmd": "library_search", "text": ""}));
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    assert_eq!(
        (found[0]["id"].as_str(), found[0]["fetched"].as_bool()),
        (Some("test-pins"), Some(false))
    );
    assert_eq!(found[0]["license_note"], "No conditions.");
    let found = search(json!({"cmd": "library_search", "text": "bracket", "unlicensed": true}));
    assert_eq!(found[0]["id"], "brackets", "{found}");
    // Words match the components an entry lists.
    assert_eq!(
        search(json!({"cmd": "library_search", "text": "dowel"}))[0]["id"],
        "test-pins"
    );
    // Fetched, the entry says so and its components are found too.
    library_command(
        &cache,
        Some(&cli),
        json!({"cmd": "library_fetch", "url": url}),
    );
    let answer = library_command(
        &cache,
        None,
        json!({"cmd": "library_search", "text": "dowel"}),
    );
    assert_eq!(answer["libraries"][0]["fetched"], true, "{answer}");
    assert_eq!(answer["components"][0]["id"], "pin", "{answer}");
    let repo = LibraryRepo::open(&cache.dir_for(&url)).unwrap();
    assert_eq!(repo.versions().unwrap().len(), 2);
}
