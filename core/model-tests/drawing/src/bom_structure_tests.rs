// SPDX-License-Identifier: MIT
//! The BOM Structure acting on a Bill of Materials (mitcad#1985), on the
//! mock kernel: a small assembly with one component of each structure, its
//! structured and parts-only views (rows, quantities, item numbers, mass
//! and volume), and a drawing's parts list at both levels with its
//! balloons.
//!
//! The design (each body a 10 mm steel cube, 1000 mm³):
//!
//! - Base: Normal, a body of its own.
//! - Motor: Purchased (the user's choice), holding Rotor.
//! - Frame: Phantom, placed twice, each holding Rail twice.
//! - Fixture: Reference, a body of its own.
//! - Hinge: Inseparable, holding Leaf twice and Pin.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap_or(Value::Null)
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A new component in the active one, active; its uid and occurrence.
fn component(doc: &mut Document<MockKernel>, name: &str) -> (String, String) {
    let made = command(doc, json!({"cmd": "create_component", "name": name}));
    (
        made["component"].as_str().unwrap().to_owned(),
        made["occurrence"].as_str().unwrap().to_owned(),
    )
}

fn activate(doc: &mut Document<MockKernel>, component: &str) {
    command(
        doc,
        json!({"cmd": "activate_component", "component": component}),
    );
}

/// A 10 mm cube at `x`, `y` in the active component.
fn cube(doc: &mut Document<MockKernel>, x: f64, y: f64) -> BodyUid {
    let added = doc
        .add_feature(
            &def(json!({"type": "box", "plane": "xy", "corner": [x, y],
                        "length": 10, "width": 10, "height": 10, "operation": "new_body"})),
            None,
        )
        .unwrap();
    BodyUid::new(added.uid, 0)
}

/// The design's bodies and occurrences a test picks.
struct Design {
    doc: Document<MockKernel>,
    rail: BodyUid,
    fixture: BodyUid,
    leaf: BodyUid,
    /// Frame's second placement and its second Rail.
    frame2: String,
    rail2: String,
    fixture_occurrence: String,
    hinge: String,
    leaf_occurrence: String,
    /// Frame's and Hinge's components.
    frame: String,
    hinge_component: String,
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    component(&mut doc, "Base");
    cube(&mut doc, 0.0, 0.0);
    activate(&mut doc, "C0");
    let (motor, _) = component(&mut doc, "Motor");
    component(&mut doc, "Rotor");
    cube(&mut doc, 20.0, 0.0);
    activate(&mut doc, "C0");
    let (frame, frame1) = component(&mut doc, "Frame");
    let (_, rail1) = component(&mut doc, "Rail");
    let rail = cube(&mut doc, 40.0, 0.0);
    activate(&mut doc, &frame);
    let rail2 = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": rail1,
               "transform": {"translation": [0, 20, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    activate(&mut doc, "C0");
    let frame2 = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": frame1,
               "transform": {"translation": [0, 50, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    let (fixture_component, fixture_occurrence) = component(&mut doc, "Fixture");
    let fixture = cube(&mut doc, 60.0, 0.0);
    activate(&mut doc, "C0");
    let (hinge_component, hinge) = component(&mut doc, "Hinge");
    let (_, leaf_occurrence) = component(&mut doc, "Leaf");
    let leaf = cube(&mut doc, 80.0, 0.0);
    activate(&mut doc, &hinge_component);
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": leaf_occurrence,
               "transform": {"translation": [0, 20, 0]}}),
    );
    component(&mut doc, "Pin");
    cube(&mut doc, 100.0, 0.0);
    activate(&mut doc, "C0");
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "C0", "material": "steel"}),
    );
    // The structures: the user's choices (none of these is a library part).
    for (owner, structure) in [
        (&motor, "Purchased"),
        (&frame, "Phantom"),
        (&fixture_component, "Reference"),
        (&hinge_component, "Inseparable"),
    ] {
        command(
            &mut doc,
            json!({"cmd": "set_property", "owner": owner, "set": "Bill of Materials",
                   "key": "BOM Structure", "value": structure}),
        );
    }
    Design {
        doc,
        rail,
        fixture,
        leaf,
        frame2,
        rail2,
        fixture_occurrence,
        hinge,
        leaf_occurrence,
        frame,
        hinge_component,
    }
}

/// A view's rows as `item name xquantity (structure)`, indented by level.
fn view_rows(answer: &Value) -> Vec<String> {
    answer["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let indent = "  ".repeat(r["level"].as_u64().unwrap() as usize - 1);
            format!(
                "{indent}{} {} x{} ({})",
                r["item"].as_str().unwrap_or("-"),
                r["name"].as_str().unwrap(),
                r["quantity"],
                r["bom_structure"].as_str().unwrap()
            )
        })
        .collect()
}

fn row<'a>(answer: &'a Value, name: &str) -> &'a Value {
    answer["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no row {name}: {answer}"))
}

fn close(value: &Value, want: f64) -> bool {
    value.as_f64().is_some_and(|got| (got - want).abs() < 1e-9)
}

/// The mass of `cubes` 10 mm steel cubes, kg (7.85 g/cm³).
fn steel(cubes: f64) -> f64 {
    cubes * 1000.0 * 7.85e-6
}

#[test]
fn the_structured_and_parts_only_views_apply_each_bom_structure() {
    let d = design();
    let doc = &d.doc;
    doc.kernel().measures.set(true);
    let structured = query(
        doc,
        json!({"query": "parts_list", "view": "structured", "properties": ["BOM Structure"]}),
    );
    assert_eq!(structured["view"], "structured");
    // Purchased: one line; Phantom: not listed, its Rails up a level and
    // counted for both its placements; Reference: no number, no quantity;
    // Inseparable: its children below it.
    assert_eq!(
        view_rows(&structured),
        [
            "1 Base x1 (Normal)",
            "2 Motor x1 (Purchased)",
            "3 Rail x4 (Normal)",
            "- Fixture x0 (Reference)",
            "4 Hinge x1 (Inseparable)",
            "  4.1 Leaf x2 (Normal)",
            "  4.2 Pin x1 (Normal)",
        ],
        "{structured}"
    );
    let fixture = row(&structured, "Fixture");
    assert_eq!(fixture["placed"], 1);
    assert_eq!(fixture["item"], Value::Null);
    assert_eq!(fixture["properties"]["BOM Structure"], "Reference");
    // The mass and volume of one, an assembly's with what it holds.
    for (name, cubes) in [("Base", 1.0), ("Motor", 1.0), ("Rail", 1.0), ("Hinge", 3.0)] {
        let r = row(&structured, name);
        assert!(close(&r["volume"], cubes * 1000.0), "{r}");
        assert!(close(&r["mass"], steel(cubes)), "{r}");
    }
    // The design's totals leave the Reference out: Base, Motor's Rotor,
    // four Rails and Hinge's three.
    let totals = &structured["totals"];
    assert!(close(&totals["volume"], 9000.0), "{totals}");
    assert!(close(&totals["mass"], steel(9.0)), "{totals}");

    let parts = query(doc, json!({"query": "parts_list", "view": "parts"}));
    assert_eq!(parts["view"], "parts");
    // Purchased and Inseparable: one item each, their children not listed;
    // the Phantom's Rails counted through both its placements.
    assert_eq!(
        view_rows(&parts),
        [
            "1 Base x1 (Normal)",
            "2 Motor x1 (Purchased)",
            "3 Rail x4 (Normal)",
            "- Fixture x0 (Reference)",
            "4 Hinge x1 (Inseparable)",
        ],
        "{parts}"
    );
    assert_eq!(parts["totals"]["quantity"], 7, "{parts}");
    assert!(close(&parts["totals"]["mass"], steel(9.0)), "{parts}");

    // Filtered by structure, in a view too.
    let references = query(
        doc,
        json!({"query": "parts_list", "view": "parts", "bom_structure": ["Reference"]}),
    );
    assert_eq!(view_rows(&references), ["- Fixture x0 (Reference)"]);
    let error = doc
        .query(&json!({"query": "parts_list", "view": "tree"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("there is no view tree"), "{error}");
    // The list of every component is as before, each row with its
    // structure.
    let list = query(doc, json!({"query": "parts_list"}));
    assert_eq!(row(&list, "Frame")["bom_structure"], "Phantom");
    assert_eq!(row(&list, "Rail")["quantity"], 4);
    assert_eq!(row(&list, "Fixture")["quantity"], 1);
}

#[test]
fn a_structure_changed_changes_both_views() {
    let mut d = design();
    // Hinge Normal again: the parts-only view lists its Leaves and Pin.
    command(
        &mut d.doc,
        json!({"cmd": "remove_property", "owner": d.hinge_component, "set": "Bill of Materials",
               "key": "BOM Structure"}),
    );
    let parts = query(&d.doc, json!({"query": "parts_list", "view": "parts"}));
    assert_eq!(
        view_rows(&parts),
        [
            "1 Base x1 (Normal)",
            "2 Motor x1 (Purchased)",
            "3 Rail x4 (Normal)",
            "- Fixture x0 (Reference)",
            "4 Leaf x2 (Normal)",
            "5 Pin x1 (Normal)",
        ],
        "{parts}"
    );
    // Frame Normal: it is listed with its Rails below it.
    command(
        &mut d.doc,
        json!({"cmd": "set_property", "owner": d.frame, "set": "Bill of Materials",
               "key": "BOM Structure", "value": "Normal"}),
    );
    let structured = query(&d.doc, json!({"query": "parts_list", "view": "structured"}));
    assert_eq!(
        view_rows(&structured)[2..4],
        ["3 Frame x2 (Normal)", "  3.1 Rail x2 (Normal)"],
        "{structured}"
    );
}

/// A named edge of a body, for a balloon's pick.
fn edge_of(doc: &Document<MockKernel>, body: BodyUid) -> String {
    doc.kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .unwrap()
}

fn label(doc: &Document<MockKernel>, id: u64) -> Value {
    let sheet = query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone();
    sheet["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

#[test]
fn a_drawings_parts_list_and_balloons_apply_each_bom_structure() {
    let mut d = design();
    command(&mut d.doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut d.doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1"}),
    );
    let added = command(
        &mut d.doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280], "balloons": true}),
    );
    let list = added["annotation"].as_u64().unwrap();
    // First level: Frame's Rails at its level, four; the Reference without
    // a number; numbered by name.
    let shown = label(&d.doc, list);
    assert_eq!(
        shown["label"], "1 Base x1; REF Fixture; 2 Hinge x1; 3 Motor x1; 4 Rail x4",
        "{shown}"
    );
    // A balloon on each numbered row, none on the Reference.
    let balloons = added["balloons"].as_array().unwrap();
    assert!(balloons.len() <= 4, "{added}");
    for balloon in balloons {
        let text = label(&d.doc, balloon.as_u64().unwrap())["label"].clone();
        assert_ne!(text, "?", "{added}");
    }
    // Balloons picked by hand: the second Frame's second Rail takes Rail's
    // number; a Leaf its Hinge's; the Fixture none.
    let rail_edge = edge_of(&d.doc, d.rail);
    let leaf_edge = edge_of(&d.doc, d.leaf);
    let fixture_edge = edge_of(&d.doc, d.fixture);
    let on_rail = command(
        &mut d.doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": d.rail.to_string(),
                          "occurrence": format!("{}/{}", d.frame2, d.rail2),
                          "edge": rail_edge}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(label(&d.doc, on_rail)["label"], "4");
    let on_leaf = command(
        &mut d.doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": d.leaf.to_string(),
                          "occurrence": format!("{}/{}", d.hinge, d.leaf_occurrence),
                          "edge": leaf_edge}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(label(&d.doc, on_leaf)["label"], "2");
    let on_fixture = command(
        &mut d.doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": d.fixture.to_string(), "occurrence": d.fixture_occurrence,
                          "edge": fixture_edge}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let fixture = label(&d.doc, on_fixture);
    assert_eq!(fixture["broken"], true, "{fixture}");
    assert!(
        fixture["error"]
            .as_str()
            .unwrap()
            .contains("is a reference"),
        "{fixture}"
    );

    // Parts only: Purchased and Inseparable one item each, the Phantom's
    // Rails as parts; the Leaf's balloon still Hinge's.
    command(
        &mut d.doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": list,
               "table": {"levels": "parts"}}),
    );
    let shown = label(&d.doc, list);
    assert_eq!(
        shown["label"], "1 Base x1; REF Fixture; 2 Hinge x1; 3 Motor x1; 4 Rail x4",
        "{shown}"
    );
    assert_eq!(label(&d.doc, on_leaf)["label"], "2");
    // The quantity cell of a Reference says so.
    let sheet = query(&d.doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone();
    let texts: Vec<&str> = sheet["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap())
        .filter_map(|i| i["text"].as_str())
        .collect();
    assert!(texts.contains(&"REF"), "{texts:?}");

    // Hinge Normal again: at parts its Leaves and Pin are rows, numbered.
    command(
        &mut d.doc,
        json!({"cmd": "remove_property", "owner": d.hinge_component, "set": "Bill of Materials",
               "key": "BOM Structure"}),
    );
    let shown = label(&d.doc, list);
    assert_eq!(
        shown["label"], "1 Base x1; REF Fixture; 2 Leaf x2; 3 Motor x1; 4 Pin x1; 5 Rail x4",
        "{shown}"
    );
    assert_eq!(label(&d.doc, on_leaf)["label"], "2");
}
