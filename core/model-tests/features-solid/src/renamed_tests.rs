// SPDX-License-Identifier: MIT
//! Face, edge and vertex references found again after a sketch change
//! renamed their faces (mitcad#272), with the mock kernel: the owner's
//! fillet, a unique side face, a tie, a face that is gone, and planes on a
//! renamed face.

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
/// The front face of the block: line c1 between c4 and c2.
const FRONT: &str = "F2:side(c1[c4,c2])";

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

fn body() -> BodyUid {
    BodyUid::new(FeatureUid(2), 0)
}

fn faces(doc: &Document<MockKernel>) -> Vec<FaceName> {
    doc.body_shape(body()).expect("the body").faces.clone()
}

/// The one side face of Extrude1 (F2) on `curve`.
fn side(doc: &Document<MockKernel>, curve: &str) -> FaceName {
    let prefix = format!("F2:side({curve}[");
    let found: Vec<FaceName> = faces(doc)
        .into_iter()
        .filter(|f| f.to_string().starts_with(&prefix))
        .collect();
    assert_eq!(found.len(), 1, "{prefix}: {found:?}");
    found[0].clone()
}

fn has_edge(doc: &Document<MockKernel>, edge: &EdgeName) -> bool {
    doc.kernel()
        .count_edges(doc.body_shape(body()).unwrap(), edge)
        > 0
}

fn extrude(doc: &mut Document<MockKernel>, region: &str) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    assert_eq!(status(doc, 2), FeatureStatus::Ok);
}

/// Sketch1 (F1): the outline of a 60 x 40 rectangle (c7 bottom, c8 right,
/// c9 top, c10 left) whose corner at (0, 40) is rounded by the arc c11,
/// with circles of radius 5 at the three other corners (c12 at the origin,
/// c13, c14), extruded by Extrude1 (F2) from the region inside the
/// outline and outside the circles.
fn outline() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let point = |id: &str, at: [f64; 2]| json!({"id": id, "type": "point", "at": at});
    let line = |id: &str, start: &str, end: &str| json!({"id": id, "type": "line", "start": start, "end": end});
    let circle = |id: &str, center: &str| json!({"id": id, "type": "circle", "center": center, "radius": 5.0});
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
            point("p1", [0.0, 0.0]),
            point("p2", [60.0, 0.0]),
            point("p3", [60.0, 40.0]),
            point("p4", [5.0, 40.0]),
            point("p5", [0.0, 35.0]),
            point("p6", [5.0, 35.0]),
            line("c7", "p1", "p2"),
            line("c8", "p2", "p3"),
            line("c9", "p3", "p4"),
            line("c10", "p5", "p1"),
            {"id": "c11", "type": "arc", "center": "p6", "start": "p4", "end": "p5"},
            circle("c12", "p1"),
            circle("c13", "p2"),
            circle("c14", "p3")]}}),
    );
    // The region the arc bounds is the one inside the outline.
    let profiles = query(&doc, json!({"query": "profiles"}));
    let regions: Vec<String> = profiles
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["region"].as_str().unwrap().to_owned())
        .filter(|r| r.contains("c11["))
        .collect();
    assert_eq!(regions.len(), 1, "{profiles}");
    extrude(&mut doc, &regions[0]);
    doc
}

/// Sketch1 (F1) with a 60 x 40 rectangle (c1..c4) extruded by Extrude1
/// (F2).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    extrude(&mut doc, RECT);
    doc
}

/// Shell1 (F3) with the front face removed.
fn shell_front(doc: &mut Document<MockKernel>) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [FRONT], "inside": 2}}),
    );
    assert_eq!(status(doc, 3), FeatureStatus::Ok);
    assert!(warnings(doc, 3).is_empty());
}

fn round_corner(doc: &mut Document<MockKernel>, a: &str, b: &str) -> Value {
    command(
        doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": a, "b": b, "radius": 3}),
    )
}

/// The owner's case: a fillet on the edges where circles meet the lines
/// next to a corner arc keeps them when the arc is deleted and a circle
/// takes its place, with a warning, and the definition keeps its names.
#[test]
fn a_fillet_keeps_its_edges_when_a_sketch_change_renames_their_faces() {
    let mut doc = outline();
    // The top line c9 meets c14 and the arc; the left line c10 the arc and c12.
    let top = side(&doc, "c9");
    let left = side(&doc, "c10");
    assert!(top.to_string().contains("c11]"), "{top}");
    assert!(left.to_string().contains("[c11,"), "{left}");
    let stored = [
        EdgeName::new(side(&doc, "c14"), top.clone()),
        EdgeName::new(left.clone(), side(&doc, "c12")),
    ];
    for edge in &stored {
        assert!(has_edge(&doc, edge), "{edge}");
    }
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": stored, "radius": 1}}),
    );
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    assert!(warnings(&doc, 3).is_empty());

    // The arc goes and a circle takes its place: c9 and c10 now end on it.
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["c11"]}),
    );
    let added = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 40], "diameter": 12}),
    );
    assert_eq!(status(&doc, 2), FeatureStatus::Ok);
    let top_now = side(&doc, "c9");
    let left_now = side(&doc, "c10");
    assert_ne!(top_now, top, "{added}");
    assert_ne!(left_now, left);
    for edge in &stored {
        assert!(!has_edge(&doc, edge), "{edge} is still there");
    }

    // The fillet stays ok, on the same edges, and says what it found.
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    let now = [
        EdgeName::new(side(&doc, "c14"), top_now),
        EdgeName::new(left_now, side(&doc, "c12")),
    ];
    let fillets: Vec<FaceName> = faces(&doc)
        .into_iter()
        .filter(|f| f.role == "fillet")
        .collect();
    assert_eq!(
        fillets,
        now.iter()
            .map(|e| FaceName::fillet(FeatureUid(3), e.clone()))
            .collect::<Vec<_>>()
    );
    let said = warnings(&doc, 3);
    assert_eq!(said.len(), 2, "{said:?}");
    for (warning, (stored, now)) in said.iter().zip(stored.iter().zip(&now)) {
        assert_eq!(
            *warning,
            format!("edge {stored} found as {now} after a change in Sketch1")
        );
    }
    let feature = query(&doc, json!({"query": "feature", "uid": "F3"}));
    assert_eq!(feature["status"], "warning", "{feature}");
    assert_eq!(feature["warnings"].as_array().unwrap().len(), 2);
    // The definition keeps the names it was given.
    match &doc.feature(FeatureUid(3)).unwrap().def {
        FeatureDef::Fillet(fillet) => assert_eq!(fillet.sets[0].edges, stored),
        other => panic!("not a fillet: {other:?}"),
    }

    // Undone, the names find their edges again and nothing is said.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    assert!(warnings(&doc, 3).is_empty());
}

/// A face reference (the shell's removed face) to a side face whose
/// neighbour changed finds the one face of the same curve.
#[test]
fn a_side_face_whose_neighbour_changed_is_found_when_unique() {
    let mut doc = block();
    shell_front(&mut doc);
    // A rounded corner between c1 and c2: c1 now ends on the arc.
    round_corner(&mut doc, "c1", "c2");
    // The face's name now, on the body as Extrude1 leaves it (the shell
    // takes the face away).
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    let front = side(&doc, "c1");
    assert_ne!(front.to_string(), FRONT);
    command(&mut doc, json!({"cmd": "set_marker", "position": 3}));
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    assert_eq!(
        warnings(&doc, 3),
        [format!(
            "face {FRONT} found as {front} after a change in Sketch1"
        )]
    );
    // The shell removed the face it found: c1's face is gone from the body.
    assert!(
        !faces(&doc)
            .iter()
            .any(|f| f.to_string().starts_with("F2:side(c1[")),
        "{:?}",
        faces(&doc)
    );
}

/// A line split into two pieces, neither keeping the reference's
/// neighbours: the reference is ambiguous and fails with the candidates.
#[test]
fn a_tie_after_a_sketch_change_fails_with_the_candidates() {
    let mut doc = block();
    shell_front(&mut doc);
    // Both neighbours change: c1 ends on two arcs, still one face.
    round_corner(&mut doc, "c4", "c1");
    round_corner(&mut doc, "c1", "c2");
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    assert_eq!(warnings(&doc, 3).len(), 1);
    // A notch across c1 splits it into two pieces, each as like the
    // reference.
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, -5],
               "width": 20, "height": 10}),
    );
    let mut pieces: Vec<String> = faces(&doc)
        .iter()
        .map(ToString::to_string)
        .filter(|f| f.starts_with("F2:side(c1["))
        .collect();
    pieces.sort();
    assert_eq!(pieces.len(), 2, "{pieces:?}");
    match status(&doc, 3) {
        FeatureStatus::Failed(message) => assert_eq!(
            message,
            format!(
                "{FRONT} is ambiguous after a sketch change: {}",
                pieces.join(", ")
            )
        ),
        other => panic!("the shell is {other:?}"),
    }
}

/// A face whose curve is deleted is gone: the reference fails as before.
#[test]
fn a_face_whose_curve_is_deleted_is_gone() {
    let mut doc = outline();
    let corner = side(&doc, "c14");
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "shell", "body": "F2.b0",
               "faces": [corner], "inside": 2}}),
    );
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["c14"]}),
    );
    assert_eq!(status(&doc, 2), FeatureStatus::Ok);
    // In plain words, the face for agents (mitcad#1612).
    let FeatureStatus::Failed(error) = status(&doc, 3) else {
        panic!("the shell did not fail");
    };
    assert_eq!(error.message, "the body has no such face");
    assert_eq!(error.agent_text(), format!("the body has no face {corner}"));
    assert!(warnings(&doc, 3).is_empty());
}

/// A sketch and a construction plane on a side face whose name changed
/// keep their planes.
#[test]
fn planes_on_a_renamed_face_keep_their_place() {
    let mut doc = block();
    // The front face's name after the change, from a first try.
    round_corner(&mut doc, "c1", "c2");
    let renamed = side(&doc, "c1").to_string();
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(side(&doc, "c1").to_string(), FRONT);
    // The mock kernel has the front face's plane under both names.
    for name in [FRONT.to_owned(), renamed.clone()] {
        doc.kernel().surfaces.borrow_mut().insert(
            name,
            SurfaceGeometry::Plane {
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, -1.0, 0.0],
            },
        );
    }
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FRONT, "body": "F2.b0"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": {"body": "F2.b0", "face": FRONT},
                              "distance": 5}}}),
    );
    let frame = query(&doc, json!({"query": "sketch", "uid": "F3"}))["frame"].clone();
    let datum = doc.datum(FeatureUid(4));
    assert!(datum.is_some());
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    assert_eq!(status(&doc, 4), FeatureStatus::Ok);

    round_corner(&mut doc, "c1", "c2");
    assert_eq!(side(&doc, "c1").to_string(), renamed);
    let said = format!("face {FRONT} found as {renamed} after a change in Sketch1");
    for uid in [3, 4] {
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
        assert_eq!(warnings(&doc, uid), [said.as_str()], "F{uid}");
    }
    assert_eq!(
        query(&doc, json!({"query": "sketch", "uid": "F3"}))["frame"],
        frame
    );
    assert_eq!(doc.datum(FeatureUid(4)), datum);
}

/// An edge whose face a chamfer cut away (mitcad#1187): the fillet still
/// fails, its error suggests the edges of the faces it names, the
/// chamfer's first, and nothing takes one in its place. The query that
/// looks an edge up says the same.
#[test]
fn a_missing_edge_suggests_edges_of_its_faces_and_takes_none() {
    let mut doc = block();
    let name = |text: &str| -> FaceName { text.parse().unwrap() };
    let (top, front) = (name(&format!("F2:end({RECT})")), name(FRONT));
    let edge = EdgeName::new(top.clone(), front.clone());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "chamfer", "body": "F2.b0",
               "edges": [edge], "size": {"type": "equal_distance", "distance": 1}}}),
    );
    assert_eq!(status(&doc, 3), FeatureStatus::Ok);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [edge], "radius": 2}}),
    );
    let FeatureStatus::Failed(message) = status(&doc, 4) else {
        panic!("the fillet did not fail");
    };
    // The chamfer's edges with each face first, then the rest by name.
    let chamfer = FaceName::chamfer(FeatureUid(3), edge.clone());
    let made = |kept: &FaceName, instead: &FaceName| {
        let at = EdgeName::new(kept.clone(), chamfer.clone());
        format!("{at} ({chamfer} instead of {instead})")
    };
    let head = format!(
        "edge {edge} no longer exists after Chamfer1; existing edges of body F2.b0 \
         that share a face with it (suggestions only, not applied): {}, {}, ",
        made(&top, &front),
        made(&front, &top)
    );
    assert!(message.starts_with(&head), "{message}");
    assert!(message.ends_with(" and 3 more"), "{message}");
    // Nothing was taken: the fillet keeps its edge and rounds nothing.
    match &doc.feature(FeatureUid(4)).unwrap().def {
        FeatureDef::Fillet(fillet) => assert_eq!(fillet.sets[0].edges, std::slice::from_ref(&edge)),
        other => panic!("not a fillet: {other:?}"),
    }
    assert_eq!(doc.kernel().count("fillet"), 0);
    assert!(warnings(&doc, 4).is_empty());
    // The feature query and the edge lookup of a query give the same detail.
    let feature = query(&doc, json!({"query": "feature", "uid": "F4"}));
    assert_eq!(feature["error"], message.agent_text());
    let (_, suggestions) = message.split_once("; ").unwrap();
    let lookup = json!({"query": "tangent_chains", "body": "F2.b0", "edge": edge});
    let error = doc.query(&lookup.to_string()).unwrap_err();
    assert_eq!(
        mitcad_model::notice::FeatureError::from_text(&error.to_string()).agent_text(),
        format!("the body has no edge {edge}; {suggestions}")
    );
}
