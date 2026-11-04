// SPDX-License-Identifier: MIT
//! The end pieces of a modelled thread (mitcad#1231) with the mock kernel
//! and its thread pieces (`MockKernel::thread_pieces`): a chamfer where
//! the thread meets the plain rest of its face, and a fillet on an edge of
//! that chamfer, keep working when a change of the thread's length changes
//! its piece count, up and down; a design that names the end piece by its
//! number along the thread (as designs saved before name it) finds the
//! edge again with a notice, which Auto Fix writes into the definitions;
//! and a number that tells no end leaves the chamfer failing, with the
//! edges between the thread's pieces and the same face suggested first.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

/// The rod's round face: circle c1 extruded by Extrude1 (F2).
const SIDE: &str = "F2:side(c1)";
/// The thread's piece at the high end of the face, counted from that end.
const HIGH: &str = "F3:high_thread(F2:side(c1))#0";
/// The thread's piece at the low end.
const LOW: &str = "F3:low_thread(F2:side(c1))#0";

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

/// The chamfer's face on the edge `chamfered`, as the kernel names it.
fn chamfer_face(chamfered: &str) -> String {
    format!("F4:chamfer({chamfered})")
}

fn has_edge(doc: &Document<MockKernel>, name: &str) -> bool {
    let shape = doc.body_shape(BodyUid::new(FeatureUid(2), 0)).unwrap();
    doc.kernel().count_edges(shape, &name.parse().unwrap()) > 0
}

/// The thread's number along the thread of the piece `piece` (its other
/// name, `F3:thread(F2:side(c1))#<n>`).
fn number_of(doc: &Document<MockKernel>, piece: &str) -> String {
    let shape = doc.body_shape(BodyUid::new(FeatureUid(2), 0)).unwrap();
    let faces = doc.kernel().faces(shape).unwrap();
    let names = &faces
        .iter()
        .find(|f| f.names[0] == piece)
        .unwrap_or_else(|| panic!("no face {piece}"))
        .names;
    assert_eq!(names.len(), 2, "{names:?}");
    names[1].clone()
}

/// A rod of diameter 10 and 20 long (Sketch1 F1, Extrude1 F2) with an
/// M10x1.5 thread (Thread1, F3) modelled 12 long, 4 from its high end: 8
/// turns, 32 pieces, the plain face at both ends.
fn threaded_rod() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().thread_pieces.set(true);
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "thread",
               "faces": [{"body": "F2.b0", "face": SIDE}],
               "thread": {"designation": "M10x1.5", "class": "6g"}, "modeled": true,
               "length": 12, "offset": 4, "chamfer": "both"}}),
    );
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    doc
}

/// Sets the thread's length (its parameter).
fn set_length(doc: &mut Document<MockKernel>, length: f64) {
    let parameters = query(doc, json!({"query": "parameters"}));
    let name = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["owner"] == "F3" && p["comment"].as_str().unwrap().contains("length"))
        .unwrap_or_else(|| panic!("no length in {parameters}"))["name"]
        .clone();
    command(
        doc,
        json!({"cmd": "set_parameter", "name": name, "value": length}),
    );
}

/// Chamfer1 (F4) on `chamfered` and Fillet1 (F5) on `filleted`.
fn chamfer_and_fillet(doc: &mut Document<MockKernel>, chamfered: &str, filleted: &str) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
               "edges": [chamfered],
               "size": {"type": "equal_distance", "distance": 0.2}}}),
    );
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [filleted], "radius": 0.1}}),
    );
}

#[test]
fn the_ends_keep_their_names_when_the_piece_count_changes() {
    let mut doc = threaded_rod();
    // The pieces counted from each end; the high end's number along the
    // thread is the last of the 32.
    assert_eq!(number_of(&doc, HIGH), "F3:thread(F2:side(c1))#63");
    assert_eq!(number_of(&doc, LOW), "F3:thread(F2:side(c1))#32");
    let chamfered = edge(SIDE, HIGH);
    let filleted = edge(SIDE, &chamfer_face(&chamfered));
    chamfer_and_fillet(&mut doc, &chamfered, &filleted);
    for (length, number) in [(15.0, "#79"), (9.0, "#47"), (13.5, "#71")] {
        set_length(&mut doc, length);
        // Another piece count: the numbers along the thread change, the
        // names from the ends do not.
        assert_eq!(
            number_of(&doc, HIGH),
            format!("F3:thread(F2:side(c1)){number}")
        );
        for uid in [3, 4, 5] {
            assert_eq!(status(&doc, uid), FeatureStatus::Ok, "{length}: F{uid}");
            assert!(warnings(&doc, uid).is_empty(), "{length}: F{uid}");
        }
        assert!(has_edge(&doc, &edge(SIDE, LOW)));
    }
}

#[test]
fn a_saved_number_along_the_thread_finds_the_end_again() {
    // As designs saved before name them: the end piece by its number along
    // the thread, the chamfer's face after it.
    let mut doc = threaded_rod();
    let saved = edge(SIDE, "F3:thread(F2:side(c1))#63");
    let filleted = edge(SIDE, &chamfer_face(&saved));
    chamfer_and_fillet(&mut doc, &saved, &filleted);
    // The number still names the end piece: the chamfer as it was; the
    // fillet finds the chamfer's face, now named after the end's name.
    assert_eq!(status(&doc, 4), FeatureStatus::Ok);
    assert!(warnings(&doc, 4).is_empty());
    assert_eq!(status(&doc, 5), FeatureStatus::Ok);
    let found = edge(SIDE, HIGH);
    let fillet_found = edge(SIDE, &chamfer_face(&found));
    assert_eq!(
        warnings(&doc, 5),
        [format!(
            "edge {filleted} found as {fillet_found} after a change in Chamfer1"
        )]
    );
    // More turns: #63 is a piece in the middle (40 pieces, #40 to #79).
    // Fewer: there is no #63 (24 pieces, #24 to #47). Either way the
    // number lies nearer to the high end's (#79, #47) than to the low
    // end's (#40, #24): the high end's edge with the face.
    for length in [15.0, 9.0] {
        set_length(&mut doc, length);
        assert_eq!(status(&doc, 4), FeatureStatus::Ok, "{length}");
        assert_eq!(
            warnings(&doc, 4),
            [format!(
                "edge {saved} found as {found} after a change in Thread1"
            )],
            "{length}"
        );
        assert_eq!(status(&doc, 5), FeatureStatus::Ok, "{length}");
        assert_eq!(warnings(&doc, 5).len(), 1, "{length}");
    }
    // Auto Fix writes the end's names into both.
    let report = command(
        &mut doc,
        json!({"cmd": "auto_fix", "features": ["F4", "F5"]}),
    );
    assert_eq!(report["left"], json!([]), "{report}");
    let def = |uid: &str| query(&doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(def("F4")["edges"], json!([found]));
    assert_eq!(def("F5")["edges"], json!([fillet_found]));
    for uid in [4, 5] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
        assert!(warnings(&doc, uid).is_empty());
    }
    // And they hold through the next change.
    set_length(&mut doc, 12.0);
    for uid in [4, 5] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
        assert!(warnings(&doc, uid).is_empty());
    }
}

#[test]
fn a_number_that_tells_no_end_suggests_the_threads_edges_with_the_face_first() {
    // #5 is a number of the order before mitcad#490 (below half the 32
    // pieces): it tells neither end, and both ends meet the face.
    let mut doc = threaded_rod();
    let saved = edge(SIDE, "F3:thread(F2:side(c1))#5");
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
               "edges": [saved],
               "size": {"type": "equal_distance", "distance": 0.2}}}),
    );
    let FeatureStatus::Failed(error) = status(&doc, 4) else {
        panic!("{:?}", status(&doc, 4));
    };
    let (_, suggested) = error
        .split_once("(suggestions only, not applied): ")
        .unwrap_or_else(|| panic!("{error}"));
    // The edges between the thread's end pieces and the face first.
    let missing = "F3:thread(F2:side(c1))#5";
    let expected = [
        format!("{} ({HIGH} instead of {missing})", edge(SIDE, HIGH)),
        format!("{} ({LOW} instead of {missing})", edge(SIDE, LOW)),
    ];
    assert!(suggested.starts_with(&expected.join(", ")), "{suggested}");
    // Auto Fix leaves it to be picked again.
    let report = command(&mut doc, json!({"cmd": "auto_fix", "features": ["F4"]}));
    assert_eq!(report["left"][0]["advice"], "pick it again", "{report}");
}
