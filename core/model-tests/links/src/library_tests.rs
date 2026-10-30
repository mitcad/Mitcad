// SPDX-License-Identifier: MIT
//! Configuration tables and library parts (mitcad#64, mitcad#63) with the
//! mock kernel and a resolver in memory: tables checked and applied, parts
//! inserted in a row (linked or copied) with their version, licence and
//! designation recorded, read again at the recorded version when the
//! design opens, updated only when asked, and listed as parts.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::file::MemoryStore;
use mitcad_model::ids::ComponentUid;
use mitcad_model::library::{
    LibraryRequest, LibraryRow, LibrarySource, LinkResolver, SupplierPart,
};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Configurations, Document, InsertOptions, LibraryChange, LibraryPart};
use mitcad_model_api::DocumentApi;

const V1: &str = "1111111111111111111111111111111111111111";
const V2: &str = "2222222222222222222222222222222222222222";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

/// A pin: a `d` x `d` block `L` high, with a table of sizes.
fn pin(rows: Value) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "d", "expression": "4 mm"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "L", "expression": "10 mm"}),
    )
    .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &mitcad_model::ValueInput::Name("d".into()),
            &mitcad_model::ValueInput::Name("d".into()),
        )
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "selectors": ["Size", "Length"], "parameters": ["d", "L"], "default": "M4x10",
            "rows": rows}}),
    )
    .unwrap();
    doc
}

fn rows_v1() -> Value {
    json!([
        {"name": "M4x10", "select": {"Size": "M4", "Length": "10"}, "values": {"d": "4 mm", "L": "10 mm"}},
        {"name": "M4x16", "select": {"Size": "M4", "Length": "16"}, "values": {"d": "4 mm", "L": "16 mm"}},
        {"name": "M10x20", "select": {"Size": "M10", "Length": "20"}, "values": {"d": "10 mm", "L": "20 mm"}}
    ])
}

/// The library in memory: the pin's text at two versions, in the second
/// M4x16 is 18 mm long.
#[derive(Default)]
struct Library {
    versions: Mutex<BTreeMap<String, String>>,
    asked: Mutex<Vec<String>>,
    /// The manifest's blocks of the rows (mitcad#949): none unless a test
    /// gives them.
    rows: Mutex<BTreeMap<String, LibraryRow>>,
}

impl Library {
    fn new() -> Arc<Self> {
        let library = Self::default();
        library
            .versions
            .lock()
            .unwrap()
            .insert(V1.to_owned(), pin(rows_v1()).to_json());
        let mut rows = rows_v1();
        rows[1]["values"]["L"] = json!("18 mm");
        library
            .versions
            .lock()
            .unwrap()
            .insert(V2.to_owned(), pin(rows).to_json());
        Arc::new(library)
    }
}

impl LinkResolver for Library {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        self.asked.lock().unwrap().push(request.rev.clone());
        let rev = match request.rev.as_str() {
            "v1.0.0" => V1,
            "v1.1.0" => V2,
            other => other,
        };
        let text = self
            .versions
            .lock()
            .unwrap()
            .get(rev)
            .cloned()
            .ok_or_else(|| format!("version {rev} is not in the library's copy here"))?;
        Ok(LibrarySource {
            text,
            store: MemoryStore::new(),
            rev: rev.to_owned(),
            label: if rev == V1 { "v1.0.0" } else { "v1.1.0" }.to_owned(),
            library: "test-pins".to_owned(),
            url: "https://example.org/pins.git".to_owned(),
            library_name: "Test pins".to_owned(),
            component: request.component.clone(),
            authors: vec!["Pin makers".to_owned()],
            path: "pins/pin.mitcad".to_owned(),
            name: "Square pin".to_owned(),
            standard: "TP 1".to_owned(),
            license: "CC0-1.0".to_owned(),
            designation: String::new(),
            supplier: None,
            part_number: None,
            supplier_url: None,
            build: None,
            rows: self.rows.lock().unwrap().clone(),
        })
    }
}

fn part(rev: &str, config: Option<&str>) -> LibraryPart {
    LibraryPart {
        library: "test-pins".to_owned(),
        url: "https://example.org/pins.git".to_owned(),
        rev: rev.to_owned(),
        component: "pin".to_owned(),
        config: config.map(str::to_owned),
        standard: String::new(),
    }
}

/// The height of a component's body: the extrusion's extent, which the
/// mock's B-rep data of a linked part keeps in its history
/// (`import(F1:0..6,prism(F2:0..16))`), else its box.
fn height(doc: &Document<MockKernel>, component: ComponentUid) -> f64 {
    let bodies = doc.component_bodies(component);
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    let shape = doc.body_shape(bodies[0]).unwrap();
    if let Some(at) = shape.history.find("prism(") {
        let range = &shape.history[at..];
        let range = &range[range.find(':').unwrap() + 1..range.find(')').unwrap()];
        let (start, end) = range.split_once("..").unwrap();
        return end.parse::<f64>().unwrap() - start.parse::<f64>().unwrap();
    }
    let bounds = shape.bounds.unwrap();
    bounds.max[2] - bounds.min[2]
}

#[test]
fn configuration_tables_are_checked_and_applied() {
    let mut doc = pin(rows_v1());
    let answer = query(&doc, json!({"query": "configurations"}));
    assert_eq!(answer["current"], "M4x10");
    assert_eq!(
        answer["selector_values"],
        json!({"Size": ["M4", "M10"], "Length": ["10", "16", "20"]})
    );
    assert_eq!(answer["problems"], json!([]));
    doc.recompute();

    let changed = command(
        &mut doc,
        json!({"cmd": "apply_configuration", "name": "M10x20"}),
    )
    .unwrap();
    // The options Size and Length are parameters since mitcad#798.
    assert_eq!(changed["changed"], json!(["L", "Length", "Size", "d"]));
    assert_eq!(doc.undo_label(), Some("Apply M10x20"));
    assert_eq!(
        query(&doc, json!({"query": "configurations"}))["current"],
        "M10x20"
    );
    let error = command(
        &mut doc,
        json!({"cmd": "apply_configuration", "name": "M99"}),
    )
    .unwrap_err();
    assert!(error.contains("no configuration M99"), "{error}");

    // What a table may not do.
    let bad = |rows: Value, parameters: Value| {
        let mut doc = pin(rows_v1());
        command(
            &mut doc,
            json!({"cmd": "set_configurations", "configurations": {
                "selectors": ["Size"], "parameters": parameters, "rows": rows}}),
        )
        .unwrap_err()
    };
    let e = bad(
        json!([{"name": "A", "select": {"Size": "M4"}, "values": {}}]),
        json!(["x"]),
    );
    assert!(e.contains("parameter x of the table does not exist"), "{e}");
    let e = bad(
        json!([{"name": "A", "select": {"Size": "M4"}, "values": {"d": "1 mm"}},
               {"name": "B", "select": {"Size": "M4"}, "values": {"d": "2 mm"}}]),
        json!(["d"]),
    );
    assert!(e.contains("B: another row has the same Size"), "{e}");
    let e = bad(
        json!([{"name": "A", "select": {"Size": "M4"}, "values": {"d": "1 deg"}}]),
        json!(["d"]),
    );
    assert!(e.starts_with("A: d = 1 deg"), "{e}");
    let e = bad(
        json!([{"name": "A", "values": {"d": "1 mm"}}]),
        json!(["d"]),
    );
    assert!(e.contains("A: no value for Size"), "{e}");

    // The table goes with the file and back.
    let text = doc.to_json();
    assert!(text.contains("\"configurations\""), "{text}");
    let reopened = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(reopened.configurations(), doc.configurations());
    assert_eq!(reopened.configurations().rows.len(), 3);
    // A design without a table writes none.
    assert!(
        !Document::new(MockKernel::default())
            .to_json()
            .contains("configurations")
    );
    // An empty table removes it, as an undo step.
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": null}),
    )
    .unwrap();
    assert_eq!(doc.configurations(), &Configurations::default());
    doc.undo().unwrap();
    assert_eq!(doc.configurations().rows.len(), 3);
}

#[test]
fn configuration_parameter_renames_follow_keys_and_expression_references() {
    let mut doc = Document::new(MockKernel::default());
    for (name, expression) in [
        ("Width", "10 mm"),
        ("WidthExtra", "2 mm"),
        ("Length", "3 mm"),
    ] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name,
                   "expression": expression, "unit": "mm"}),
        )
        .unwrap();
    }
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": ["Width", "Length"],
            "rows": [{"name": "Large", "values": {
                "Width": "20 mm", "Length": "Width * 2 + WidthExtra"}}]}}),
    )
    .unwrap();
    let before = doc.to_json();
    doc.rename_parameter("Width", "Breadth").unwrap();
    let renamed = doc.to_json();
    let table = doc.configurations();
    assert_eq!(table.parameters, ["Breadth", "Length"]);
    assert_eq!(table.rows[0].values["Breadth"], "20 mm");
    assert!(!table.rows[0].values.contains_key("Width"));
    assert_eq!(table.rows[0].values["Length"], "Breadth * 2 + WidthExtra");
    let mut reopened = Document::from_json(&renamed, MockKernel::default()).unwrap();
    reopened.apply_configuration("Large").unwrap();
    for (name, value) in [("Breadth", 20.0), ("Length", 42.0)] {
        let id = reopened.parameters().find(name).unwrap();
        assert_eq!(reopened.parameters().value(id), Some(value));
    }
    Document::from_json(&reopened.to_json(), MockKernel::default()).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.to_json(), before);
    Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    doc.redo().unwrap();
    assert_eq!(doc.to_json(), renamed);
    Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();

    // References to parameters outside the table's columns follow too.
    doc.rename_parameter("WidthExtra", "Allowance").unwrap();
    assert_eq!(
        doc.configurations().rows[0].values["Length"],
        "Breadth * 2 + Allowance"
    );
    Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
}

#[test]
fn deleting_configured_parameters_or_their_owners_is_atomic() {
    let mut doc = Document::new(MockKernel::default());
    for (name, expression) in [("Width", "10 mm"), ("Allowance", "2 mm")] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name, "expression": expression}),
        )
        .unwrap();
    }
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": ["Width"], "rows": [{"name": "Large", "values": {
                "Width": "Allowance * 2"}}]}}),
    )
    .unwrap();
    for name in ["Width", "Allowance"] {
        let before = doc.to_json();
        let revision = doc.revision();
        let undo = doc.undo_label().map(str::to_owned);
        let count = doc.recompute_count();
        let error = doc.delete_parameter(name).unwrap_err().to_string();
        assert!(error.contains("configurations:"), "{error}");
        assert!(error.contains(name), "{error}");
        assert_eq!(doc.to_json(), before);
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.undo_label(), undo.as_deref());
        assert_eq!(doc.recompute_count(), count);
        Document::from_json(&before, MockKernel::default()).unwrap();
    }

    let mut block = mitcad_model::test_support::block();
    let dimension = block.doc.feature(block.extrude).unwrap().def.params()[0];
    let name = block.doc.parameters().name(dimension);
    command(
        &mut block.doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": [name],
            "rows": [{"name": "Tall", "values": {name.clone(): "20 mm"}}]}}),
    )
    .unwrap();
    let before = block.doc.to_json();
    let revision = block.doc.revision();
    let error = block
        .doc
        .delete_feature(block.extrude, false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("configurations:"), "{error}");
    assert!(error.contains(&name), "{error}");
    assert_eq!(block.doc.to_json(), before);
    assert_eq!(block.doc.revision(), revision);
    Document::from_json(&before, MockKernel::default()).unwrap();
    // The author can explicitly remove the table, then delete its owner.
    block
        .doc
        .set_configurations(Configurations::default())
        .unwrap();
    block.doc.delete_feature(block.extrude, false).unwrap();
    assert!(block.doc.parameters().find(&name).is_none());
    Document::from_json(&block.doc.to_json(), MockKernel::default()).unwrap();
    block.doc.undo().unwrap();
    block.doc.undo().unwrap();
    assert_eq!(block.doc.to_json(), before);
    block.doc.redo().unwrap();
    block.doc.redo().unwrap();
    Document::from_json(&block.doc.to_json(), MockKernel::default()).unwrap();
}

#[test]
fn library_parts_keep_their_version_until_updated() {
    let library = Library::new();
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library.clone()));
    let (screw, _) = doc
        .insert_library_component(&part("v1.0.0", Some("M4x16")), &InsertOptions::default())
        .unwrap();
    // The tag became its commit; the designation names the component and
    // its body.
    let reference = doc
        .assembly()
        .component(screw)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(reference.rev, V1);
    assert_eq!(reference.label, "v1.0.0");
    assert_eq!(reference.config.as_deref(), Some("M4x16"));
    assert_eq!(reference.designation, "TP 1 M4x16");
    assert_eq!(reference.license, "CC0-1.0");
    assert_eq!(doc.assembly().name(screw), "TP 1 M4x16");
    assert_eq!(doc.undo_label(), Some("Insert TP 1 M4x16"));
    let body = doc.component_bodies(screw)[0];
    assert_eq!(doc.body_name(body), "TP 1 M4x16");
    assert!((height(&doc, screw) - 16.0).abs() < 1e-9);
    // Without a row: the table's default.
    let (default, _) = doc
        .insert_library_component(&part(V1, None), &InsertOptions::default())
        .unwrap();
    assert!((height(&doc, default) - 10.0).abs() < 1e-9);
    let error = doc
        .insert_library_component(&part(V1, Some("M5x5")), &InsertOptions::default())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no configuration M5x5 (its rows: M4x10, M4x16, M10x20)"),
        "{error}"
    );

    // Saved and opened again: read at the recorded commit, unchanged.
    let saved = doc.to_json();
    assert!(saved.contains(&format!("\"rev\": \"{V1}\"")), "{saved}");
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.set_link_resolver(Some(library.clone()));
    library.asked.lock().unwrap().clear();
    let messages = reopened.update_links(None).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(*library.asked.lock().unwrap(), [V1, V1]);
    // Another computer without the version keeps the saved bodies.
    library.versions.lock().unwrap().remove(V1);
    let mut elsewhere = Document::from_json(&saved, MockKernel::default()).unwrap();
    elsewhere.set_link_resolver(Some(library.clone()));
    let messages = elsewhere.update_links(None).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(
        messages[0].contains(
            "library test-pins v1.0.0 (1111111) (https://example.org/pins.git) cannot be read"
        ) && messages[0].ends_with("it keeps the bodies saved with the design"),
        "{messages:?}"
    );
    elsewhere.recompute();
    assert!((height(&elsewhere, screw) - 16.0).abs() < 1e-9);
    // Without any resolver too.
    let mut no_libraries = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = no_libraries.update_links(None).unwrap();
    assert!(
        messages[0].contains("no component libraries are set up here"),
        "{messages:?}"
    );

    // An explicit update to the newer version, an undo step; the name
    // follows the designation only when it was the designation.
    library
        .versions
        .lock()
        .unwrap()
        .insert(V1.to_owned(), pin(rows_v1()).to_json());
    let lines = command(
        &mut doc,
        json!({"cmd": "update_library_parts", "changes": [{"component": screw.to_string(), "rev": V2}]}),
    )
    .unwrap();
    assert_eq!(
        lines["changes"],
        json!(["TP 1 M4x16: version v1.0.0 (1111111) -> v1.1.0 (2222222)"])
    );
    assert_eq!(doc.undo_label(), Some("Update TP 1 M4x16 to v1.1.0, M4x16"));
    assert!((height(&doc, screw) - 18.0).abs() < 1e-9);
    assert_eq!(
        doc.assembly()
            .component(screw)
            .unwrap()
            .library
            .as_ref()
            .unwrap()
            .rev,
        V2
    );
    // Another size: the occurrence's part changes, renamed after it.
    doc.update_library_parts(&[LibraryChange {
        component: screw,
        rev: None,
        config: Some("M10x20".to_owned()),
    }])
    .unwrap();
    assert_eq!(doc.assembly().name(screw), "TP 1 M10x20");
    assert!((height(&doc, screw) - 20.0).abs() < 1e-9);
    doc.undo().unwrap();
    assert_eq!(doc.assembly().name(screw), "TP 1 M4x16");
    assert!((height(&doc, screw) - 18.0).abs() < 1e-9);

    // The comparison with the saved file names the change.
    let diff = mitcad_model_diff::diff_files(&saved, &doc.to_json()).unwrap();
    let text = diff.to_text();
    assert!(
        text.contains(&format!("library.rev {V1} -> {V2}")),
        "{text}"
    );
    assert!(text.contains("library.label v1.0.0 -> v1.1.0"), "{text}");
}

#[test]
fn copies_of_library_parts_are_editable_and_keep_their_source() {
    let library = Library::new();
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library));
    let options = InsertOptions {
        link: false,
        ..InsertOptions::default()
    };
    let (copy, _) = doc
        .insert_library_component(&part(V1, Some("M10x20")), &options)
        .unwrap();
    // The design with its history, in the row; the source and licence are
    // recorded; the table does not come along.
    let types: Vec<&str> = doc
        .features()
        .filter(|f| f.component == copy)
        .map(|f| f.def.type_name())
        .collect();
    assert_eq!(types, ["sketch", "extrude"]);
    assert!((height(&doc, copy) - 20.0).abs() < 1e-9);
    assert!(doc.configurations().is_empty());
    let reference = doc
        .assembly()
        .component(copy)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(
        (reference.license.as_str(), reference.url.as_str()),
        ("CC0-1.0", "https://example.org/pins.git")
    );
    doc.activate_component(copy).unwrap();
    let error = doc
        .update_library_parts(&[LibraryChange {
            component: copy,
            rev: Some(V2.to_owned()),
            config: None,
        }])
        .unwrap_err()
        .to_string();
    assert!(error.contains("is a copy of a library part"), "{error}");
    // The file keeps the source.
    let reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        reopened
            .assembly()
            .component(copy)
            .unwrap()
            .library
            .as_ref(),
        Some(&reference)
    );
}

#[test]
fn the_parts_list_counts_placements_and_names_library_parts() {
    let library = Library::new();
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library));
    // A bracket with two screws, placed twice; one more screw at the top.
    let bracket = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    )
    .unwrap();
    let (screw, _) = doc
        .insert_library_component(&part(V1, Some("M4x16")), &InsertOptions::default())
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "TP 1 M4x16:1"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": bracket["occurrence"]}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "insert_component", "library": {"id": "test-pins", "url": "https://example.org/pins.git",
               "rev": V1, "component": "pin", "config": "M10x20"}, "link": true}),
    )
    .unwrap();
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    let summary: Vec<(String, u64)> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["designation"].as_str().unwrap().to_owned(),
                r["quantity"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Bracket".to_owned(), 2),
            ("TP 1 M4x16".to_owned(), 4),
            ("TP 1 M10x20".to_owned(), 1)
        ]
    );
    assert_eq!(rows[1]["license"], "CC0-1.0");
    assert_eq!(rows[1]["version"], "v1.0.0 (1111111)");
    assert_eq!(rows[1]["component"], json!(screw));
    let parts = query(&doc, json!({"query": "library_parts"}))["parts"].clone();
    assert_eq!(parts.as_array().unwrap().len(), 2);
    assert_eq!(parts[0]["library"]["config"], "M4x16");
    assert_eq!(parts[0]["linked"], true);
}

#[test]
fn the_library_usage_counts_each_library_component_once() {
    let library = Library::new();
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library));
    assert_eq!(
        query(&doc, json!({"query": "library_usage"})),
        json!({"parts": []})
    );
    // A bracket with two pins (M4x16), placed twice, and one more pin of
    // another row at the top: one component of the library, five times.
    let bracket = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    )
    .unwrap();
    doc.insert_library_component(&part(V1, Some("M4x16")), &InsertOptions::default())
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "TP 1 M4x16:1"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    )
    .unwrap();
    let copy = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": bracket["occurrence"]}),
    )
    .unwrap();
    let top = command(
        &mut doc,
        json!({"cmd": "insert_component", "library": {"id": "test-pins", "url": "https://example.org/pins.git",
               "rev": V1, "component": "pin", "config": "M10x20"}, "link": false}),
    )
    .unwrap();
    assert_eq!(
        query(&doc, json!({"query": "library_usage"})),
        json!({"parts": [{"library": "test-pins", "url": "https://example.org/pins.git",
                          "component": "pin", "count": 5,
                          "rows": [{"config": "M4x16", "count": 4}, {"config": "M10x20", "count": 1}]}]})
    );
    // It follows the design as parts are removed.
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": copy["occurrence"]}),
    )
    .unwrap();
    assert_eq!(
        query(&doc, json!({"query": "library_usage"}))["parts"][0]["count"],
        3
    );
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": bracket["occurrence"]}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": top["occurrence"]}),
    )
    .unwrap();
    assert_eq!(
        query(&doc, json!({"query": "library_usage"})),
        json!({"parts": []})
    );
}

#[test]
fn the_library_usage_counts_each_configuration_row_of_a_component() {
    // 35 uses of one row and 2 of another (mitcad#518): the per-row counts
    // add up to the component's, and the rows the design does not use are
    // not listed.
    let library = Library::new();
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library));
    let insert = |doc: &mut Document<MockKernel>, config: &str| {
        command(
            doc,
            json!({"cmd": "insert_component", "library": {"id": "test-pins",
                   "url": "https://example.org/pins.git", "rev": V1, "component": "pin",
                   "config": config}, "link": false}),
        )
        .unwrap()
    };
    let long = insert(&mut doc, "M4x16");
    for _ in 0..34 {
        command(
            &mut doc,
            json!({"cmd": "copy_occurrence", "occurrence": long["occurrence"]}),
        )
        .unwrap();
    }
    let top = insert(&mut doc, "M10x20");
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": top["occurrence"]}),
    )
    .unwrap();
    let usage = query(&doc, json!({"query": "library_usage"}));
    let parts = usage["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1, "{usage}");
    assert_eq!(parts[0]["count"], 37);
    assert_eq!(
        parts[0]["rows"],
        json!([{"config": "M4x16", "count": 35}, {"config": "M10x20", "count": 2}])
    );
}

/// mitcad#949: rows tagged with standards and suppliers' articles. A part
/// inserted with a standard is named by it and keeps it; a size its row
/// lacks takes the row's first; a row of no standard says so; the parts
/// list carries the articles as a column's text.
#[test]
fn library_parts_follow_the_rows_standards_and_list_supplier_parts() {
    let library = Library::new();
    let article = |number: &str| SupplierPart {
        supplier: "acme".to_owned(),
        supplier_name: "Acme".to_owned(),
        part_number: number.to_owned(),
        material: "8.8".to_owned(),
        finish: "zinc-plated".to_owned(),
        checked: "2026-10-30".to_owned(),
    };
    {
        let mut rows = library.rows.lock().unwrap();
        rows.insert(
            "M4x10".to_owned(),
            LibraryRow {
                standards: vec!["TP 1".to_owned(), "DN 1".to_owned()],
                supplier_parts: vec![article("A 4 10")],
            },
        );
        rows.insert(
            "M4x16".to_owned(),
            LibraryRow {
                standards: vec!["DN 1".to_owned()],
                supplier_parts: vec![article("A 4 16"), article("A 4 16 S")],
            },
        );
        rows.insert(
            "M10x20".to_owned(),
            LibraryRow {
                standards: vec!["non-standard".to_owned()],
                supplier_parts: Vec::new(),
            },
        );
    }
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library.clone()));
    // Inserted with the Standard filter's DN 1 (insert_component's
    // `standard`): named and recorded by it.
    let inserted = command(
        &mut doc,
        json!({"cmd": "insert_component", "library": {"id": "test-pins", "url": "https://example.org/pins.git",
               "rev": V1, "component": "pin", "config": "M4x10", "standard": "DN 1"}, "link": true}),
    )
    .unwrap();
    let parts = query(&doc, json!({"query": "library_parts"}))["parts"].clone();
    let reference = &parts[0]["library"];
    assert_eq!(reference["designation"], "DN 1 M4x10", "{inserted}");
    assert_eq!(reference["standard"], "DN 1");
    assert_eq!(reference["supplier_parts"][0]["part_number"], "A 4 10");
    assert_eq!(reference["supplier_parts"][0]["supplier_name"], "Acme");
    // Without a standard asked for: the row's first.
    let mut plain = part(V1, Some("M4x10"));
    let (first, _) = doc
        .insert_library_component(&plain, &InsertOptions::default())
        .unwrap();
    let first_ref = doc
        .assembly()
        .component(first)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(first_ref.designation, "TP 1 M4x10");
    assert_eq!(first_ref.standard, "TP 1");
    // A standard the row lacks: its own first.
    plain.config = Some("M4x16".to_owned());
    plain.standard = "TP 1".to_owned();
    let (din, _) = doc
        .insert_library_component(&plain, &InsertOptions::default())
        .unwrap();
    let din_ref = doc
        .assembly()
        .component(din)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(din_ref.designation, "DN 1 M4x16");
    assert_eq!(din_ref.supplier_parts.len(), 2);
    // A row of no standard.
    plain.config = Some("M10x20".to_owned());
    let (odd, _) = doc
        .insert_library_component(&plain, &InsertOptions::default())
        .unwrap();
    assert_eq!(doc.assembly().name(odd), "M10x20 (non-standard)");
    // Another size keeps the standard it was named by where the row has it.
    let lines = doc
        .update_library_parts(&[LibraryChange {
            component: first,
            rev: None,
            config: Some("M4x16".to_owned()),
        }])
        .unwrap();
    assert_eq!(lines.len(), 1);
    let moved = doc
        .assembly()
        .component(first)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(moved.designation, "DN 1 M4x16");
    assert_eq!(moved.standard, "DN 1");
    // Renamed after it (the name taken by the other part: with a number).
    assert!(doc.assembly().name(first).starts_with("DN 1 M4x16"));
    // The parts list: the articles' text per row.
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    let din_row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["component"] == json!(din))
        .unwrap()
        .clone();
    assert_eq!(
        din_row["supplier_parts_text"],
        "Acme A 4 16 (8.8 zinc-plated); Acme A 4 16 S (8.8 zinc-plated)"
    );
    assert_eq!(din_row["supplier_parts"].as_array().unwrap().len(), 2);
    // The design keeps the standard and the articles: saved and read again.
    let again = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    let kept = again
        .assembly()
        .component(din)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(kept.standard, "DN 1");
    assert_eq!(kept.supplier_parts.len(), 2);
}

/// A library of today without row blocks names its parts as before
/// (mitcad#949): the component's standard, no articles.
#[test]
fn library_parts_without_row_blocks_read_as_before() {
    let library = Library::new();
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(library));
    let mut asked = part(V1, Some("M4x16"));
    asked.standard = "DN 1".to_owned();
    let (screw, _) = doc
        .insert_library_component(&asked, &InsertOptions::default())
        .unwrap();
    let reference = doc
        .assembly()
        .component(screw)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(reference.designation, "TP 1 M4x16");
    assert!(reference.standard.is_empty() && reference.supplier_parts.is_empty());
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(!saved.to_string().contains("supplier_parts"));
}
