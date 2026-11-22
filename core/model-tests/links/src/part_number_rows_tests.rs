// SPDX-License-Identifier: MIT
//! A component's Part number and Description (mitcad#2138) with the mock
//! kernel: the standing rows of Property Sets (`Identity/Part number`,
//! `Identity/Description`, empty and not stored until typed, removed when
//! emptied, only at the component level), what a drawing's Bill of
//! Materials table shows of them and its `empty_columns`, a number added
//! under another set, and the parts list's part number of a library part
//! with options: the component's own first, the library's without one,
//! the read-only `Library/Part number` row the library's in both cases.

use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ValueInput;
use mitcad_model::features::SketchPlane;
use mitcad_model::file::MemoryStore;
use mitcad_model::ids::ComponentUid;
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::properties::PropertyOwner;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const V1: &str = "1111111111111111111111111111111111111111";
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap_or(Value::Null)
}

/// The levels of the `property_sets` answer.
fn levels(doc: &Document<MockKernel>, q: Value) -> Vec<Value> {
    let mut q = q;
    q["query"] = json!("property_sets");
    query(doc, q)["levels"].as_array().unwrap().clone()
}

/// A property of a level, by set and key.
fn find<'a>(level: &'a Value, set: &str, key: &str) -> Option<&'a Value> {
    level["sets"]
        .as_array()?
        .iter()
        .find(|s| s["name"] == set)?["properties"]
        .as_array()?
        .iter()
        .find(|p| p["key"] == key)
}

/// How many rows of a level have `key`, in any set.
fn rows_of(level: &Value, key: &str) -> usize {
    level["sets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["properties"].as_array().unwrap())
        .filter(|p| p["key"] == key)
        .count()
}

/// The component's level of `property_sets`.
fn component_level(doc: &Document<MockKernel>, component: &str) -> Value {
    levels(doc, json!({"component": component})).swap_remove(0)
}

/// The parts list's part number of a component.
fn listed_number(doc: &Document<MockKernel>, component: &str) -> Value {
    query(doc, json!({"query": "parts_list"}))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["component"] == component)
        .unwrap_or_else(|| panic!("{component} is not listed"))["part_number"]
        .clone()
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

/// The sheet's texts: the table's cells among them.
fn texts(doc: &Document<MockKernel>) -> Vec<String> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

fn annotation(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

/// A drawing of the whole design from the top with a Bill of Materials
/// that shows the name, Part Number and Description: the table's id.
fn bom_table(doc: &mut Document<MockKernel>) -> u64 {
    command(doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1"}),
    );
    command(
        doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280],
               "table": {"columns": ["item", "name", "part_number", "description"]}}),
    )["annotation"]
        .as_u64()
        .unwrap()
}

/// A design with the component Bracket (a 40 x 30 x 5 plate, F2.b0)
/// placed once: the design, the component and its occurrence.
fn bracket() -> (Document<MockKernel>, String, String) {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    let component = made["component"].as_str().unwrap().to_owned();
    let occurrence = made["occurrence"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    (doc, component, occurrence)
}

fn stored(doc: &Document<MockKernel>, component: &str) -> usize {
    let component: ComponentUid = component.parse().unwrap();
    doc.property_store()
        .sets(PropertyOwner::Component(component))
        .iter()
        .map(|s| s.properties.len())
        .sum()
}

#[test]
fn a_component_lists_part_number_and_description_until_they_are_typed() {
    let (mut doc, component, occurrence) = bracket();
    // Both standing rows, empty, in Identity; nothing stored.
    let level = component_level(&doc, &component);
    assert_eq!(level["level"], "component");
    for key in ["Part number", "Description"] {
        let row = find(&level, "Identity", key).unwrap_or_else(|| panic!("{key}: {level}"));
        assert_eq!(row["text"], "", "{row}");
        assert_eq!(row["type"], "text", "{row}");
        assert_eq!(row["standing"], true, "{row}");
        assert_eq!(row["read_only"], false, "{row}");
    }
    assert_eq!(stored(&doc, &component), 0);
    assert!(!doc.to_json().contains("Part number"));
    // Not on the occurrence's level nor the body's; the design (the root)
    // has none either.
    let shown = levels(&doc, json!({"occurrence": occurrence}));
    assert_eq!(shown[0]["level"], "occurrence");
    assert_eq!(rows_of(&shown[0], "Part number"), 0, "{}", shown[0]);
    assert_eq!(rows_of(&shown[0], "Description"), 0, "{}", shown[0]);
    assert_eq!(rows_of(&shown[1], "Part number"), 1, "{}", shown[1]);
    let shown = levels(&doc, json!({"body": "F2.b0"}));
    assert_eq!(shown[0]["level"], "body");
    assert_eq!(rows_of(&shown[0], "Part number"), 0, "{}", shown[0]);
    assert_eq!(rows_of(&shown[0], "Description"), 0, "{}", shown[0]);
    let root = component_level(&doc, "C0");
    assert_eq!(rows_of(&root, "Part number"), 0, "{root}");

    // The table: its Part Number and Description cells empty.
    let table = bom_table(&mut doc);
    assert_eq!(
        annotation(&doc, table)["empty_columns"],
        json!(["part_number", "description"])
    );

    // Typed in place: stored, shown in its row, in the parts list and the
    // table.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Identity",
               "key": "Part number", "value": "BR-100"}),
    );
    assert_eq!(stored(&doc, &component), 1);
    let level = component_level(&doc, &component);
    let row = find(&level, "Identity", "Part number").unwrap();
    assert_eq!(row["text"], "BR-100");
    assert_eq!(row.get("standing"), None, "{row}");
    assert_eq!(rows_of(&level, "Part number"), 1, "{level}");
    assert_eq!(
        find(&level, "Identity", "Description").unwrap()["standing"],
        true
    );
    assert_eq!(listed_number(&doc, &component), "BR-100");
    assert!(
        texts(&doc).iter().any(|t| t == "BR-100"),
        "{:?}",
        texts(&doc)
    );
    assert_eq!(
        annotation(&doc, table)["empty_columns"],
        json!(["description"])
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Identity",
               "key": "Description", "value": "Steel bracket"}),
    );
    assert!(
        texts(&doc).iter().any(|t| t == "Steel bracket"),
        "{:?}",
        texts(&doc)
    );
    assert_eq!(annotation(&doc, table)["empty_columns"], json!([]));

    // Emptied: removed again, the standing row back, the cell empty.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Identity",
               "key": "Part number", "value": ""}),
    );
    assert_eq!(stored(&doc, &component), 1, "the Description stays");
    let level = component_level(&doc, &component);
    let row = find(&level, "Identity", "Part number").unwrap();
    assert_eq!(row["standing"], true, "{row}");
    assert_eq!(row["text"], "");
    assert_eq!(listed_number(&doc, &component), "");
    assert!(!texts(&doc).iter().any(|t| t == "BR-100"));
    assert_eq!(
        annotation(&doc, table)["empty_columns"],
        json!(["part_number"])
    );
    // An empty value where nothing is stored stores nothing.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Identity",
               "key": "Part number", "value": "  "}),
    );
    assert_eq!(stored(&doc, &component), 1);
}

#[test]
fn a_part_number_stored_in_another_set_is_listed_once_and_fills_the_cell() {
    let (mut doc, component, _) = bracket();
    // As Add… stores it: in Properties.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Properties",
               "key": "Part number", "type": "text", "value": "P-7"}),
    );
    let level = component_level(&doc, &component);
    assert_eq!(rows_of(&level, "Part number"), 1, "{level}");
    assert_eq!(
        find(&level, "Properties", "Part number").unwrap()["text"],
        "P-7"
    );
    assert!(find(&level, "Identity", "Part number").is_none(), "{level}");
    // Description still stands in Identity.
    assert_eq!(
        find(&level, "Identity", "Description").unwrap()["standing"],
        true
    );
    assert_eq!(listed_number(&doc, &component), "P-7");
    let table = bom_table(&mut doc);
    assert!(texts(&doc).iter().any(|t| t == "P-7"), "{:?}", texts(&doc));
    assert_eq!(
        annotation(&doc, table)["empty_columns"],
        json!(["description"])
    );
    // Emptied in its own set: removed, the standing row back.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": component, "set": "Properties",
               "key": "Part number", "value": ""}),
    );
    assert_eq!(stored(&doc, &component), 0);
    let level = component_level(&doc, &component);
    assert_eq!(
        find(&level, "Identity", "Part number").unwrap()["standing"],
        true
    );
}

/// A board with the option Length (300 mm to 2000 mm) and the row
/// Standard (1200 mm).
fn board() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "W", "expression": "200 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "1200 mm",
               "min": "300 mm", "max": "2000 mm", "step": "10 mm"}),
    );
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &ValueInput::Name("W".into()),
            &ValueInput::Number(20.0),
        )
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": "Length"}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "options": ["Length"],
            "default": "Standard",
            "rows": [{"name": "Standard", "values": {"Length": "1200 mm"}}]}}),
    );
    doc
}

/// The library in memory: the board, with the library's part number
/// `part_number` when given.
struct Library {
    text: String,
    part_number: Option<String>,
}

impl LinkResolver for Library {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        if request.rev != V1 {
            return Err(format!("version {} is not here", request.rev));
        }
        Ok(LibrarySource {
            text: self.text.clone(),
            store: MemoryStore::new(),
            rev: V1.to_owned(),
            label: "v1.0.0".to_owned(),
            library: "test-boards".to_owned(),
            url: "https://example.org/boards.git".to_owned(),
            library_name: "Test boards".to_owned(),
            component: request.component.clone(),
            authors: vec!["Board makers".to_owned()],
            path: "boards/board.mitcad".to_owned(),
            name: "Board".to_owned(),
            standard: String::new(),
            license: "CC0-1.0".to_owned(),
            designation: "Board {Length}".to_owned(),
            supplier: None,
            part_number: self.part_number.clone(),
            supplier_url: None,
            build: None,
            rows: Default::default(),
            non_printable: false,
        })
    }
}

/// A design with the board inserted at 800 mm, its library giving
/// `part_number`: the design and the board's component.
fn inserted(part_number: Option<&str>) -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Arc::new(Library {
        text: board().to_json(),
        part_number: part_number.map(str::to_owned),
    })));
    let answer = command(
        &mut doc,
        json!({"cmd": "insert_component", "library": {"id": "test-boards",
               "url": "https://example.org/boards.git", "rev": V1, "component": "board"},
               "parameters": {"Length": "800 mm"}}),
    );
    let component = answer["component"].as_str().unwrap().to_owned();
    let uid: ComponentUid = component.parse().unwrap();
    assert!(doc.component_options(uid).is_some(), "a part with options");
    (doc, component)
}

fn own_number(doc: &mut Document<MockKernel>, component: &str, number: &str) {
    command(
        doc,
        json!({"cmd": "set_property", "owner": component, "set": "Identity",
               "key": "Part number", "value": number}),
    );
}

#[test]
fn a_library_part_without_a_library_number_lists_its_own() {
    let (mut doc, board) = inserted(None);
    assert_eq!(listed_number(&doc, &board), "");
    let level = component_level(&doc, &board);
    assert_eq!(
        find(&level, "Identity", "Part number").unwrap()["standing"],
        true
    );
    own_number(&mut doc, &board, "OWN-1");
    assert_eq!(listed_number(&doc, &board), "OWN-1");
    // No library number: no Library/Part number row.
    let level = component_level(&doc, &board);
    assert!(find(&level, "Library", "Part number").is_none(), "{level}");
    let table = bom_table(&mut doc);
    assert!(
        texts(&doc).iter().any(|t| t == "OWN-1"),
        "{:?}",
        texts(&doc)
    );
    assert_eq!(
        annotation(&doc, table)["empty_columns"],
        json!(["description"])
    );
}

#[test]
fn a_library_part_lists_the_library_number_unless_it_has_its_own() {
    // The library's number as before.
    let (mut doc, board) = inserted(Some("LIB-7"));
    assert_eq!(listed_number(&doc, &board), "LIB-7");
    let level = component_level(&doc, &board);
    assert_eq!(
        find(&level, "Library", "Part number").unwrap()["text"],
        "LIB-7"
    );
    // Its own number wins in the parts list and the table; the Library row
    // keeps the library's.
    own_number(&mut doc, &board, "OWN-1");
    assert_eq!(listed_number(&doc, &board), "OWN-1");
    let level = component_level(&doc, &board);
    assert_eq!(
        find(&level, "Library", "Part number").unwrap()["text"],
        "LIB-7"
    );
    assert_eq!(
        find(&level, "Identity", "Part number").unwrap()["text"],
        "OWN-1"
    );
    bom_table(&mut doc);
    let cells = texts(&doc);
    assert!(cells.iter().any(|t| t == "OWN-1"), "{cells:?}");
    assert!(!cells.iter().any(|t| t == "LIB-7"), "{cells:?}");
    // Emptied: the library's again.
    own_number(&mut doc, &board, "");
    assert_eq!(listed_number(&doc, &board), "LIB-7");
}
