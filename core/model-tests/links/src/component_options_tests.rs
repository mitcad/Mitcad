// SPDX-License-Identifier: MIT
//! Options of inserted components (mitcad#801) with the mock kernel and a
//! library in memory: a board with the options Color and Length, the rule
//! "Black is short" and the row Standard, inserted with `parameters`
//! bound to the design's parameters or constant; the cascade's moves, a
//! bound value that breaks a rule, two components of one source, a linked
//! design with options, the parts list's designation and the queries.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::file::MemoryStore;
use mitcad_model::ids::ComponentUid;
use mitcad_model::library::{LibraryRequest, LibrarySource, LinkResolver};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, ValueInput};
use mitcad_model_api::DocumentApi;

const V1: &str = "1111111111111111111111111111111111111111";

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

/// A shelf board `W` wide, 20 mm thick and `Length` long, with the options
/// Color (a text) and Length (300 mm to 2000 mm by 10 mm), the rule "Black
/// is short" and the row Standard (White, 1200 mm, with a designation of
/// its own).
fn board() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for (name, expression) in [("W", "200 mm"), ("Color", "'White'")] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name, "expression": expression}),
        )
        .unwrap();
    }
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Length", "expression": "1200 mm",
               "min": "300 mm", "max": "2000 mm", "step": "10 mm"}),
    )
    .unwrap();
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
            "options": ["Color", "Length"],
            "default": "Standard",
            "rows": [{"name": "Standard", "values": {"Color": "'White'", "Length": "1200 mm"},
                      "designation": "Shelf board standard"}],
            "rules": [{"name": "Black is short", "when": "Color == 'Black'",
                       "then": [{"option": "Length", "max": "1000 mm"}]}]}}),
    )
    .unwrap();
    doc
}

/// The library in memory: the board at one version, its designation
/// pattern `Board {Color} {Length}`.
struct Library {
    text: String,
}

impl Library {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            text: board().to_json(),
        })
    }
}

impl LinkResolver for Library {
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        if request.rev != V1 && request.rev != "v1.0.0" {
            return Err(format!("version {} is not here", request.rev));
        }
        Ok(LibrarySource {
            text: self.text.clone(),
            store: MemoryStore::new(),
            rev: V1.to_owned(),
            label: "v1.0.0".to_owned(),
            library: "test-shelves".to_owned(),
            url: "https://example.org/shelves.git".to_owned(),
            library_name: "Test shelves".to_owned(),
            component: request.component.clone(),
            authors: vec!["Shelf makers".to_owned()],
            path: "boards/board.mitcad".to_owned(),
            name: "Shelf board".to_owned(),
            standard: String::new(),
            license: "CC0-1.0".to_owned(),
            designation: "Board {Color} {Length}".to_owned(),
            supplier: None,
            part_number: None,
            supplier_url: None,
            build: None,
            rows: Default::default(),
        })
    }
}

/// A design with the library and the parameters Shelf_W (800 mm), Wall
/// (20 mm), Finish (`'White'`) and Long (1500 mm).
fn shelf() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(Library::new()));
    for (name, expression) in [
        ("Shelf_W", "800 mm"),
        ("Wall", "20 mm"),
        ("Finish", "'White'"),
        ("Long", "1500 mm"),
    ] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name, "expression": expression}),
        )
        .unwrap();
    }
    doc
}

/// Inserts the board with options; the answer.
fn insert(doc: &mut Document<MockKernel>, parameters: Value) -> Value {
    command(
        doc,
        json!({"cmd": "insert_component", "library": {"id": "test-shelves",
               "url": "https://example.org/shelves.git", "rev": V1, "component": "board"},
               "parameters": parameters}),
    )
    .unwrap()
}

fn uid(answer: &Value) -> ComponentUid {
    answer["component"].as_str().unwrap().parse().unwrap()
}

/// The length of a component's body along z.
fn length(doc: &Document<MockKernel>, component: ComponentUid) -> f64 {
    let bodies = doc.component_bodies(component);
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    let shape = doc.body_shape(bodies[0]).unwrap();
    if let Some(bounds) = shape.bounds {
        return bounds.max[2] - bounds.min[2];
    }
    let at = shape.history.find("prism(").unwrap();
    let range = &shape.history[at..];
    let range = &range[range.find(':').unwrap() + 1..range.find(')').unwrap()];
    let (start, end) = range.split_once("..").unwrap();
    end.parse::<f64>().unwrap() - start.parse::<f64>().unwrap()
}

/// The error of a component's derived part, if it failed.
fn part_error(doc: &Document<MockKernel>, component: ComponentUid) -> Option<String> {
    let part = doc.component_options(component).unwrap().part;
    doc.status(part)
        .and_then(|s| s.error().map(|e| e.to_string()))
}

#[test]
fn a_library_part_bound_to_the_design_follows_its_parameters() {
    let mut doc = shelf();
    let answer = insert(
        &mut doc,
        json!({"Length": "Shelf_W - 2 * Wall", "Color": "Finish"}),
    );
    let board = uid(&answer);
    assert_eq!(answer["moves"], json!([]));
    assert_eq!(answer["refused"], Value::Null);
    // 800 - 2 * 20 mm: a multiple of the source's 10 mm step (a value
    // between steps would be snapped by Length's own limits, mitcad#665).
    assert!((length(&doc, board) - 760.0).abs() < 1e-6);
    // Linked and read only; made by a derived part of a source the design
    // keeps.
    let c = doc.assembly().component(board).unwrap().clone();
    let link = c.link.unwrap();
    assert!(matches!(
        doc.feature(link.feature).unwrap().def,
        FeatureDef::DerivedPart(_)
    ));
    assert_eq!(doc.state().library_sources.0.len(), 1);
    assert!(doc.activate_component(board).is_err());

    // The design's parameter changes, the board follows.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Shelf_W", "expression": "900 mm"}),
    )
    .unwrap();
    assert!((length(&doc, board) - 860.0).abs() < 1e-6);
    assert_eq!(part_error(&doc, board), None);

    // The options as written and as the pickers see them.
    let options = doc.component_options(board).unwrap();
    assert_eq!(options.parameters["Length"], "Shelf_W - 2 * Wall");
    assert_eq!(options.parameters["Color"], "Finish");
    assert!(options.bound.contains("Length") && options.bound.contains("Color"));
    let parts = query(&doc, json!({"query": "library_parts"}))["parts"].clone();
    let shown = &parts[0]["options"];
    assert_eq!(shown["parameters"]["Length"], "Shelf_W - 2 * Wall");
    let length_option = shown["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "Length")
        .unwrap()
        .clone();
    assert_eq!(length_option["bound"], true);
    assert_eq!(length_option["driven"], true);

    // Saved and opened again: no library needed, the binding stays.
    let saved = doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert!((length(&reopened, board) - 860.0).abs() < 1e-6);
    command(
        &mut reopened,
        json!({"cmd": "set_parameter", "name": "Wall", "expression": "25 mm"}),
    )
    .unwrap();
    assert!((length(&reopened, board) - 850.0).abs() < 1e-6);
}

#[test]
fn a_constant_the_rules_do_not_allow_is_moved_with_a_move_entry() {
    let mut doc = shelf();
    let answer = insert(&mut doc, json!({"Color": "'Black'", "Length": "1500 mm"}));
    let board = uid(&answer);
    let moves = answer["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 1, "{answer}");
    assert_eq!(moves[0]["option"], "Length");
    assert_eq!(moves[0]["rule"], "Black is short");
    assert_eq!(moves[0]["to_expression"], "1000 mm");
    assert!(
        moves[0]["text"]
            .as_str()
            .unwrap()
            .contains("Black is short"),
        "{answer}"
    );
    assert_eq!(answer["refused"], Value::Null);
    // The part keeps the moved value.
    assert!((length(&doc, board) - 1000.0).abs() < 1e-6);
    let options = doc.component_options(board).unwrap();
    assert_eq!(options.parameters["Length"], "1000 mm");
    assert!(options.bound.is_empty());
    assert_eq!(part_error(&doc, board), None);

    // Changed afterwards: a constant moved again, as one undo step.
    let changed = command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": board,
               "parameters": {"Length": "1800 mm"}}),
    )
    .unwrap();
    assert_eq!(changed["moves"][0]["to_expression"], "1000 mm");
    assert_eq!(
        doc.undo_label().map(str::to_owned),
        Some(format!("Change Options of {}", doc.assembly().name(board)))
    );
    let changed = command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": board,
               "parameters": {"Color": "'White'", "Length": "1800 mm"}}),
    )
    .unwrap();
    assert_eq!(changed["moves"], json!([]));
    assert!((length(&doc, board) - 1800.0).abs() < 1e-6);
    doc.undo().unwrap();
    assert!((length(&doc, board) - 1000.0).abs() < 1e-6);
}

#[test]
fn a_bound_value_that_breaks_a_rule_fails_the_part_and_the_design_changes() {
    let mut doc = shelf();
    let answer = insert(&mut doc, json!({"Color": "Finish", "Length": "Long"}));
    let board = uid(&answer);
    assert!((length(&doc, board) - 1500.0).abs() < 1e-6);
    assert_eq!(part_error(&doc, board), None);

    // Black is short, and Length is bound: the part fails with the rule,
    // and the design's parameter changes all the same.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Finish", "expression": "'Black'"}),
    )
    .unwrap();
    let finish = doc.parameters().find("Finish").unwrap();
    assert_eq!(doc.parameters().text(finish), Some("Black"));
    let error = part_error(&doc, board).unwrap();
    assert!(error.contains("Black is short"), "{error}");
    let options = doc.component_options(board).unwrap();
    assert!(
        options
            .error
            .as_deref()
            .is_some_and(|e| e.contains("Black is short")),
        "{:?}",
        options.error
    );
    // The parameter is not moved.
    assert_eq!(options.parameters["Length"], "Long");

    // Short enough again: the part is made.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Long", "expression": "900 mm"}),
    )
    .unwrap();
    assert_eq!(part_error(&doc, board), None);
    assert!((length(&doc, board) - 900.0).abs() < 1e-6);
}

#[test]
fn two_components_of_one_source_have_options_of_their_own() {
    let mut doc = shelf();
    let short = uid(&insert(&mut doc, json!({"Length": "500 mm"})));
    let long = uid(&insert(&mut doc, json!({"Length": "700 mm"})));
    assert_ne!(short, long);
    assert!((length(&doc, short) - 500.0).abs() < 1e-6);
    assert!((length(&doc, long) - 700.0).abs() < 1e-6);
    // One source for both.
    assert_eq!(doc.state().library_sources.0.len(), 1);
    // The options belong to the component: every occurrence shares them.
    let first = doc.assembly().occurrences_of(short).next().unwrap().uid;
    let copy = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": first}),
    )
    .unwrap();
    assert!(copy.is_object());
    command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": short,
               "parameters": {"Length": "600 mm"}}),
    )
    .unwrap();
    assert!((length(&doc, short) - 600.0).abs() < 1e-6);
    assert!((length(&doc, long) - 700.0).abs() < 1e-6);
    let list = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    let quantities: BTreeMap<String, u64> = list
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
    assert_eq!(quantities["Board White 600"], 2, "{list}");
    assert_eq!(quantities["Board White 700"], 1, "{list}");
}

#[test]
fn the_parts_list_names_a_combination_by_its_row_else_by_the_pattern() {
    let mut doc = shelf();
    // The row Standard's combination: its own designation.
    let standard = uid(&insert(
        &mut doc,
        json!({"Color": "'White'", "Length": "1200 mm"}),
    ));
    // No row has Black at 800 mm: the pattern with the option values.
    let black = uid(&insert(
        &mut doc,
        json!({"Color": "'Black'", "Length": "800 mm"}),
    ));
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    let row = |c: ComponentUid| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|r| r["component"] == json!(c))
            .unwrap()
            .clone()
    };
    assert_eq!(row(standard)["designation"], "Shelf board standard");
    assert_eq!(row(standard)["config"], "Standard");
    assert_eq!(row(black)["designation"], "Board Black 800");
    assert_eq!(row(black)["config"], Value::Null);
    assert_eq!(row(black)["options"]["row"], Value::Null);
    // The component takes its designation as its name.
    assert_eq!(doc.assembly().name(black), "Board Black 800");
    // Its record follows a change of its options; the name too, as it was
    // the designation.
    command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": black,
               "parameters": {"Color": "'White'", "Length": "1200 mm"}}),
    )
    .unwrap();
    let record = doc
        .assembly()
        .component(black)
        .unwrap()
        .library
        .clone()
        .unwrap();
    assert_eq!(record.designation, "Shelf board standard");
    assert_eq!(record.config.as_deref(), Some("Standard"));
    assert!(
        doc.assembly()
            .name(black)
            .starts_with("Shelf board standard")
    );
}

#[test]
fn the_configure_query_of_a_component_tries_its_values() {
    let mut doc = shelf();
    let board = uid(&insert(
        &mut doc,
        json!({"Color": "Finish", "Length": "900 mm"}),
    ));
    let answer = query(
        &doc,
        json!({"query": "configure", "component": board,
               "parameters": {"Color": "'Black'", "Length": "1500 mm"}}),
    );
    assert_eq!(answer["component"], json!(board));
    let options = answer["options"].as_array().unwrap();
    assert_eq!(options[0]["name"], "Color");
    assert_eq!(options[1]["name"], "Length");
    // The constant tried moves; nothing changed in the design.
    assert_eq!(answer["moves"][0]["option"], "Length");
    let options = doc.component_options(board).unwrap();
    assert_eq!(options.parameters["Length"], "900 mm");
    // A component without options has none.
    let plain = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plain"}),
    )
    .unwrap();
    let error = command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": plain["component"],
               "parameters": {"Length": "1 mm"}}),
    )
    .unwrap_err();
    assert!(error.contains("has no options"), "{error}");
    // A name its source does not have.
    let error = command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": board,
               "parameters": {"Depth": "1 mm"}}),
    )
    .unwrap_err();
    assert!(error.contains("no parameter Depth"), "{error}");
}

#[test]
fn a_linked_design_with_options_follows_the_design_and_its_file() {
    let dir = std::env::temp_dir().join(format!("mitcad-component-options-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("board.mitcad"), board().to_json()).unwrap();
    let mut doc = shelf();
    let answer = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": "board.mitcad",
               "base": dir.to_string_lossy(), "parameters": {"Length": "Shelf_W / 2"}}),
    )
    .unwrap();
    let board_uid = uid(&answer);
    assert_eq!(doc.assembly().name(board_uid), "board");
    assert!((length(&doc, board_uid) - 400.0).abs() < 1e-6);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Shelf_W", "expression": "1000 mm"}),
    )
    .unwrap();
    assert!((length(&doc, board_uid) - 500.0).abs() < 1e-6);
    // The file changes: read again, the binding stays.
    let mut wider = board();
    command(
        &mut wider,
        json!({"cmd": "set_parameter", "name": "W", "expression": "300 mm"}),
    )
    .unwrap();
    std::fs::write(dir.join("board.mitcad"), wider.to_json()).unwrap();
    let messages = doc.update_links(Some(&dir)).unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!((length(&doc, board_uid) - 500.0).abs() < 1e-6);
    assert_eq!(doc.state().library_sources.0.len(), 1);
    // A copy has its own parameters.
    let error = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": "board.mitcad", "link": false,
               "base": dir.to_string_lossy(), "parameters": {"Length": "500 mm"}}),
    )
    .unwrap_err();
    assert!(error.contains("insert it linked"), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_linked_design_with_options_outside_the_project_is_not_read() {
    // mitcad#929: a link out of the design's folder (in no project) is not
    // followed; the design saved with the project stays.
    let dir = std::env::temp_dir().join(format!("mitcad-929-options-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("board.mitcad"), board().to_json()).unwrap();
    let mut doc = shelf();
    let answer = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": dir.join("board.mitcad").to_string_lossy(),
               "base": dir.to_string_lossy(), "parameters": {"Length": "Shelf_W / 2"}}),
    )
    .unwrap();
    let board_uid = uid(&answer);
    // Recorded from the design's folder.
    let link = doc
        .assembly()
        .component(board_uid)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert_eq!(link.path, "board.mitcad");
    let mut value: Value = serde_json::from_str(&doc.to_json()).unwrap();
    for c in value["components"].as_array_mut().unwrap() {
        if c["name"] == "board" {
            c["link"]["path"] = json!("../../outside.mitcad");
        }
    }
    let mut reopened = Document::from_json(&value.to_string(), MockKernel::default()).unwrap();
    let messages = reopened.update_links(Some(&dir)).unwrap();
    let refused = "board: the link outside the project is not followed (../../outside.mitcad); \
                   it keeps the design saved with the project";
    assert_eq!(messages, [refused]);
    std::fs::remove_dir_all(&dir).unwrap();
}
