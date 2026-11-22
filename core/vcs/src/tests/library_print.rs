// SPDX-License-Identifier: MIT
//! Components not for print services (mitcad#2214): a manifest's
//! `non_printable` on the library (every component) and on a component
//! (over the library's, both ways), printable by default; a value that is
//! not true or false refuses the manifest with the reason; `library_show`
//! answers it per component and the resolver gives it to the model, which
//! records it on the part. Nothing goes over the network; the parts that
//! fetch are skipped without git.

use std::sync::Arc;

use serde_json::{Value, json};

use super::remote::{slashes, system_git};
use super::*;
use crate::library::manifest::{Manifest, read_json};
use crate::library::{LibraryCache, LibraryResolver, api};
use mitcad_model::{InsertOptions, LibraryPart};

fn library_command(cache: &LibraryCache, command: Value) -> Value {
    serde_json::from_str(&api::command(&command.to_string(), cache, None, None).unwrap()).unwrap()
}

/// A block 4 mm wide, `L` (10 mm) high, without a table.
fn block() -> String {
    let mut doc = Document::new(MockKernel::default());
    for command in [
        json!({"cmd": "add_parameter", "name": "L", "expression": "10 mm"}),
        json!({"cmd": "sketch.create"}),
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": "4 mm", "height": "4 mm"}),
        json!({"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
            "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}}),
    ] {
        doc.command(&command.to_string()).unwrap();
    }
    doc.to_json()
}

/// A supplier's library of three handles: the library not for print
/// services, `spacer` allowed by its own `false`, `knob` taking the
/// library's.
fn handles_manifest(library: Option<Value>) -> Value {
    let mut manifest = json!({"format": "mitcad-library", "version": 1, "id": "test-handles",
    "name": "Test handles", "license": "CC-BY-4.0", "authors": ["Handle makers"],
    "components": [
        {"id": "handle", "path": "handles/handle.mitcad", "name": "Handle", "non_printable": true},
        {"id": "spacer", "path": "handles/spacer.mitcad", "name": "Spacer", "non_printable": false},
        {"id": "knob", "path": "handles/knob.mitcad", "name": "Knob"}
    ]});
    if let Some(flag) = library {
        manifest["non_printable"] = flag;
    }
    manifest
}

fn manifest(value: &Value) -> Result<Manifest, String> {
    read_json(value.to_string().as_bytes(), "mitcad-library.json")
}

fn flags(manifest: &Manifest) -> Vec<(&str, bool)> {
    manifest
        .components
        .iter()
        .map(|c| (c.id.as_str(), manifest.non_printable_of(c)))
        .collect()
}

#[test]
fn the_library_flag_and_a_components_override_both_ways() {
    // The library not for print services: every component without its
    // own value, `false` allows one.
    let flagged = manifest(&handles_manifest(Some(json!(true)))).unwrap();
    assert!(flagged.problems().is_ok(), "{:?}", flagged.problems());
    assert_eq!(
        flags(&flagged),
        [("handle", true), ("spacer", false), ("knob", true)]
    );
    // A printable library: only the component that says `true`.
    let printable = manifest(&handles_manifest(Some(json!(false)))).unwrap();
    assert_eq!(
        flags(&printable),
        [("handle", true), ("spacer", false), ("knob", false)]
    );
    // Left out: printable by default.
    let default = manifest(&handles_manifest(None)).unwrap();
    assert_eq!(default.non_printable, None);
    assert_eq!(
        flags(&default),
        [("handle", true), ("spacer", false), ("knob", false)]
    );
    // Written back as read; a manifest without it writes none.
    let again: Manifest = serde_json::from_value(serde_json::to_value(&flagged).unwrap()).unwrap();
    assert_eq!(again, flagged);
    let written = serde_json::to_value(&default).unwrap();
    assert_eq!(written.get("non_printable"), None);
    assert_eq!(written["components"][2].get("non_printable"), None);
    assert_eq!(written["components"][1]["non_printable"], false);
}

#[test]
fn a_malformed_flag_is_refused_with_the_reason() {
    for bad in [json!("yes"), json!(1), json!(null), json!(["true"])] {
        // On the library.
        let error = manifest(&handles_manifest(Some(bad.clone()))).unwrap_err();
        assert!(
            error.starts_with("mitcad-library.json: non_printable is true or false, not "),
            "{error}"
        );
        assert!(error.contains(&bad.to_string()), "{error}");
        // On a component.
        let mut value = handles_manifest(None);
        value["components"][2]["non_printable"] = bad.clone();
        let error = manifest(&value).unwrap_err();
        assert!(
            error.contains("non_printable is true or false, not "),
            "{error}"
        );
    }

    // `library check` of a folder refuses it too.
    let scratch = Scratch::new("library-print-check");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("handles");
    let mut value = handles_manifest(None);
    value["components"][0]["non_printable"] = json!("no");
    write_library(&dir, &value, false);
    let error = api::command(
        &json!({"cmd": "library_check", "dir": dir}).to_string(),
        &cache,
        None,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("non_printable is true or false, not \"no\""),
        "{error}"
    );
    // The same folder with a boolean passes.
    write_library(&dir, &handles_manifest(Some(json!(true))), false);
    let answer = library_command(&cache, json!({"cmd": "library_check", "dir": dir}));
    assert_eq!(answer["errors"], json!([]), "{answer}");
}

/// A library folder with the three handles and `manifest`, recorded with
/// git and tagged `v1.0.0` when `record`.
fn write_library(dir: &Path, manifest: &Value, record: bool) {
    fs::create_dir_all(dir.join("handles")).unwrap();
    fs::write(dir.join("mitcad-library.json"), manifest.to_string()).unwrap();
    fs::write(dir.join("LICENSE"), "CC-BY-4.0\n").unwrap();
    for name in ["handle", "spacer", "knob"] {
        fs::write(dir.join(format!("handles/{name}.mitcad")), block()).unwrap();
    }
    if record {
        git(dir, &["init", "-q"]).unwrap();
        git(dir, &["add", "-A"]).unwrap();
        git(dir, &["commit", "-q", "-m", "Handles"]).unwrap();
        git(dir, &["tag", "v1.0.0"]).unwrap();
    }
}

#[test]
fn library_show_and_the_resolver_give_the_flag() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("library-print-show");
    let cache = LibraryCache::new(scratch.join("cache"));
    let dir = scratch.join("handles");
    write_library(&dir, &handles_manifest(Some(json!(true))), true);
    let url = slashes(&dir);
    let fetched: Value = serde_json::from_str(
        &api::command(
            &json!({"cmd": "library_fetch", "url": url}).to_string(),
            &cache,
            Some(&cli),
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(fetched["error"], Value::Null, "{fetched}");

    // library_show: the component's flag, else the library's.
    let shown = library_command(&cache, json!({"cmd": "library_show", "url": url}));
    let flag = |id: &str| {
        shown["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["non_printable"]
            .clone()
    };
    assert_eq!(flag("handle"), true, "{shown}");
    assert_eq!(flag("spacer"), false, "{shown}");
    assert_eq!(flag("knob"), true, "{shown}");

    // Inserted through the resolver: the part's record keeps it; the
    // allowed one records none.
    let resolver = Arc::new(LibraryResolver::new(cache.clone()));
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(resolver));
    let mut insert = |component: &str| {
        let part = LibraryPart {
            library: "test-handles".to_owned(),
            url: url.clone(),
            rev: "v1.0.0".to_owned(),
            component: component.to_owned(),
            config: None,
            standard: String::new(),
        };
        let (uid, _) = doc
            .insert_library_component(&part, &InsertOptions::default())
            .unwrap();
        uid
    };
    let knob = insert("knob");
    let spacer = insert("spacer");
    let record = |uid| {
        doc.assembly()
            .component(uid)
            .unwrap()
            .library
            .clone()
            .unwrap()
    };
    assert!(record(knob).non_printable);
    assert!(!record(spacer).non_printable);
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let text = saved.to_string();
    assert_eq!(text.matches("\"non_printable\":true").count(), 1, "{text}");
}
