// SPDX-License-Identifier: MIT
//! A face joined from pieces on one surface holds all their names
//! (mitcad#1261): the OCCT facade joins a disc's rim and the wall a cut
//! leaves above it on the same cylinder into one face named after both.
//! With the mock kernel the rim's face holds the cut's wall as another
//! name (`MockKernel::face_aliases`) and the cut's own face is gone from
//! the result (`MockKernel::lose`). A Fillet whose edge names the joined
//! face by the cut's wall finds it, and again after the design changes and
//! is recomputed.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

/// The disc's rim: circle c1 of Sketch1 extruded by Extrude1 (F2).
const RIM: &str = "F2:side(c1)";
/// The disc's bottom.
const BOTTOM: &str = "F2:start(r{c1})";
/// The cut's wall on the rim's cylinder: circle c1 of Sketch2 (F3) cut by
/// Extrude2 (F4).
const WALL: &str = "F4:side(c1)";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn status(doc: &Document<MockKernel>, uid: u64) -> FeatureStatus {
    doc.status(FeatureUid(uid)).cloned().expect("recomputed")
}

fn warnings(doc: &Document<MockKernel>, uid: u64) -> Vec<String> {
    doc.warnings(FeatureUid(uid)).to_vec()
}

fn edge(a: &str, b: &str) -> String {
    EdgeName::new(a.parse().unwrap(), b.parse().unwrap()).to_string()
}

/// The names of the body's faces.
fn face_names(doc: &Document<MockKernel>) -> Vec<Vec<String>> {
    let shape = doc.body_shape(BodyUid::new(FeatureUid(2), 0)).unwrap();
    doc.kernel()
        .faces(shape)
        .unwrap()
        .iter()
        .map(|f| f.names.clone())
        .collect()
}

/// Sets the parameter of `owner` whose comment holds `what`.
fn set_parameter(doc: &mut Document<MockKernel>, owner: &str, what: &str, value: f64) {
    let parameters = query(doc, json!({"query": "parameters"}));
    let name = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["owner"] == owner && p["comment"].as_str().unwrap_or("").contains(what))
        .unwrap_or_else(|| panic!("no {what} of {owner} in {parameters}"))["name"]
        .clone();
    command(
        doc,
        json!({"cmd": "set_parameter", "name": name, "value": value}),
    );
}

/// A disc of radius 44, 2 high (Sketch1 F1, Extrude1 F2), and a cut of the
/// same circle from Sketch2 (F3) by Extrude2 (F4), its wall joined into
/// the rim's face as the OCCT facade joins them; then Fillet1 (F5) on the
/// bottom edge named by the cut's wall.
fn rim_with_joined_wall() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel()
        .face_aliases
        .lock()
        .unwrap()
        .insert(RIM.to_owned(), vec![WALL.to_owned()]);
    *doc.kernel().lose.borrow_mut() = vec![WALL.parse().unwrap()];
    for sketch in ["F1", "F3"] {
        command(&mut doc, json!({"cmd": "sketch.create"}));
        command(
            &mut doc,
            json!({"cmd": "sketch.add_circle", "sketch": sketch, "center": [0, 0], "diameter": 88}),
        );
        let operation = if sketch == "F1" { "new_body" } else { "cut" };
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "extrude",
                   "profiles": [{"sketch": sketch, "region": "r{c1}"}],
                   "extent": {"type": "distance", "distance": 2}, "operation": operation}}),
        );
    }
    assert_eq!(status(&doc, 4), FeatureStatus::Ok);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [edge(WALL, BOTTOM)], "radius": 0.5}}),
    );
    doc
}

#[test]
fn a_fillet_on_the_joined_face_finds_its_edge_after_a_recompute() {
    let mut doc = rim_with_joined_wall();
    let rounding = format!("F5:fillet({})", edge(RIM, BOTTOM));
    let has_rounding =
        |doc: &Document<MockKernel>| face_names(doc).iter().any(|names| names[0] == rounding);
    assert_eq!(status(&doc, 5), FeatureStatus::Ok);
    assert!(warnings(&doc, 5).is_empty(), "{:?}", warnings(&doc, 5));
    assert!(has_rounding(&doc), "{:?}", face_names(&doc));
    // The disc made higher and the rounding larger: recomputed, the fillet
    // finds the edge by the cut's wall again, without a notice.
    set_parameter(&mut doc, "F2", "distance", 3.0);
    set_parameter(&mut doc, "F5", "radius", 0.8);
    for uid in [2, 4, 5] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok, "F{uid}");
    }
    assert!(warnings(&doc, 5).is_empty(), "{:?}", warnings(&doc, 5));
    assert!(has_rounding(&doc), "{:?}", face_names(&doc));
}
