// SPDX-License-Identifier: MIT
//! Timeline, body state, recompute cache, undo and redo, with the mock kernel.

use serde_json::json;

use crate::features::{FeatureDef, SketchPlane, ValueInput};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::testing::MockKernel;
use crate::topo::{EdgeName, FaceName, RegionKey, SegmentKey};
use crate::{Document, FeatureStatus, ModelError};

pub(crate) fn num(value: f64) -> ValueInput {
    ValueInput::Number(value)
}

pub(crate) fn def(value: serde_json::Value) -> FeatureDef<ValueInput> {
    serde_json::from_value(value).unwrap()
}

pub(crate) fn extrude(
    sketch: FeatureUid,
    regions: &[&RegionKey],
    distance: f64,
    operation: &str,
    participants: &[BodyUid],
) -> FeatureDef<ValueInput> {
    let profiles: Vec<_> = regions
        .iter()
        .map(|r| json!({"sketch": sketch, "region": r}))
        .collect();
    def(json!({"type": "extrude", "profiles": profiles,
               "extent": {"type": "distance", "distance": distance},
               "operation": operation, "participants": participants}))
}

/// Line i of a rectangle with lines c<first>..c<first+3>.
pub(crate) fn segment(first: u32, i: usize) -> SegmentKey {
    let c = |k: usize| EntityUid(first + ((k + 4) % 4) as u32);
    SegmentKey::between(c(i), c(i + 3), c(i + 1))
}

pub(crate) fn side(feature: FeatureUid, first: u32, i: usize) -> FaceName {
    FaceName::side(feature, segment(first, i))
}

/// The edge along corner i of the extruded rectangle.
pub(crate) fn corner(feature: FeatureUid, first: u32, i: usize) -> EdgeName {
    EdgeName::new(side(feature, first, (i + 3) % 4), side(feature, first, i))
}

/// The edge of line i at the far cap.
pub(crate) fn top_edge(feature: FeatureUid, first: u32, region: &RegionKey, i: usize) -> EdgeName {
    EdgeName::new(
        side(feature, first, i),
        FaceName::end(feature, region.clone()),
    )
}

pub(crate) struct Block {
    pub doc: Document<MockKernel>,
    pub sketch: FeatureUid,
    pub region: RegionKey,
    pub extrude: FeatureUid,
    pub body: BodyUid,
}

/// Sketch1 (F1) with a 60 x 40 rectangle (c1..c4; d1, d2) extruded 20 mm
/// (d3) by Extrude1 (F2) into Body1 (F2.b0).
pub(crate) fn block() -> Block {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(60.0), &num(40.0))
        .unwrap()
        .region;
    let extrude = doc
        .add_feature(&extrude(sketch, &[&region], 20.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    Block {
        doc,
        sketch,
        region,
        extrude,
        body: BodyUid::new(extrude, 0),
    }
}

impl Block {
    fn history(&self, body: BodyUid) -> String {
        self.doc
            .body_shape(body)
            .map_or_else(|| "none".to_owned(), |s| s.history.clone())
    }

    fn evaluated(&self) -> Vec<FeatureUid> {
        self.doc.stats().evaluated
    }

    fn status(&self, uid: FeatureUid) -> FeatureStatus {
        self.doc.status(uid).cloned().expect("recomputed")
    }

    fn fillet(&mut self, edges: &[&EdgeName], radius: f64) -> FeatureUid {
        let fillet =
            def(json!({"type": "fillet", "body": self.body, "edges": edges, "radius": radius}));
        self.doc.add_feature(&fillet, None).unwrap().uid
    }

    /// Sketch2 with a 20 x 10 rectangle at (10, 10) (c1..c4), joined 30 mm
    /// into the block by Extrude2.
    fn join_boss(&mut self) -> (FeatureUid, RegionKey, FeatureUid) {
        let sketch = self.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = self
            .doc
            .add_rectangle(sketch, [10.0, 10.0], &num(20.0), &num(10.0))
            .unwrap()
            .region;
        let join = extrude(sketch, &[&region], 30.0, "join", &[self.body]);
        let uid = self.doc.add_feature(&join, None).unwrap().uid;
        (sketch, region, uid)
    }

    /// A sketch with a circle (c1) at (30, 20) and the extrude of it.
    fn circle_extrude(&mut self, operation: &str, participants: &[BodyUid]) -> FeatureUid {
        let sketch = self.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = self
            .doc
            .add_circle(sketch, [30.0, 20.0], &num(10.0))
            .unwrap()
            .region;
        let def = extrude(sketch, &[&region], 30.0, operation, participants);
        self.doc.add_feature(&def, None).unwrap().uid
    }
}

#[test]
fn commands_create_features_parameters_and_bodies() {
    let b = block();
    let params = b.doc.parameters();
    let rows: Vec<_> = params
        .iter()
        .map(|p| {
            (
                p.name(),
                p.expression(),
                p.comment(),
                p.value(),
                params.owner(p.id()),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("d1", "60 mm", "Sketch1 width", 60.0, Some(b.sketch)),
            ("d2", "40 mm", "Sketch1 height", 40.0, Some(b.sketch)),
            ("d3", "20 mm", "Extrude1 distance", 20.0, Some(b.extrude)),
        ]
    );
    let names: Vec<_> = b
        .doc
        .features()
        .map(|f| (f.uid.to_string(), f.name.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("F1".to_owned(), "Sketch1".to_owned()),
            ("F2".to_owned(), "Extrude1".to_owned())
        ]
    );
    assert_eq!(
        b.region.to_string(),
        "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
    );
    let bodies = b.doc.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!((bodies[0].uid, bodies[0].name.as_str()), (b.body, "Body1"));
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
    assert_eq!(b.status(b.extrude), FeatureStatus::Ok);
    // Each command recomputed what it changed; nothing is left to do.
    assert_eq!(b.evaluated(), vec![b.extrude]);
    let mut doc = b.doc;
    assert!(doc.recompute().evaluated.is_empty());
}

#[test]
fn a_value_change_recomputes_only_what_uses_it() {
    let mut b = block();
    let other = b.circle_extrude("new_body", &[]); // Sketch2 (F3) with d4, Extrude2 (F4) with d5
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert_eq!(b.evaluated(), vec![b.extrude]);
    assert_eq!(b.history(b.body), "prism(F2:0..35)");
    b.doc.set_parameter("d1", 80.0).unwrap();
    assert_eq!(b.evaluated(), vec![b.sketch, b.extrude]);
    b.doc.set_parameter("d5", 8.0).unwrap();
    assert_eq!(b.evaluated(), vec![other]);
    // An unchanged value is no command at all.
    let undo = b.doc.undo_label().map(str::to_owned);
    b.doc.set_parameter("d5", 8.0).unwrap();
    assert_eq!(b.doc.undo_label().map(str::to_owned), undo);
}

#[test]
fn undo_redo_and_restored_values_reuse_the_cache() {
    let mut b = block();
    b.doc.set_parameter("d3", 35.0).unwrap();
    let extrusions = b.doc.kernel().count("extrude");
    assert_eq!(b.doc.undo().as_deref(), Some("Change d3"));
    assert!(b.evaluated().is_empty());
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
    assert_eq!(b.doc.redo().as_deref(), Some("Change d3"));
    assert!(b.evaluated().is_empty());
    assert_eq!(b.history(b.body), "prism(F2:0..35)");
    b.doc.set_parameter("d3", 20.0).unwrap();
    assert!(b.evaluated().is_empty());
    assert_eq!(b.doc.kernel().count("extrude"), extrusions);

    // Undo goes back through every command, redo forward again.
    let mut labels = Vec::new();
    while let Some(label) = b.doc.undo() {
        labels.push(label);
    }
    assert_eq!(
        labels,
        vec![
            "Change d3",
            "Change d3",
            "Add Extrude1",
            "Add Rectangle to Sketch1",
            "Add Sketch1"
        ]
    );
    assert_eq!(b.doc.features().count(), 0);
    assert!(b.doc.parameters().is_empty() && b.doc.bodies().is_empty());
    while b.doc.redo().is_some() {}
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
    assert_eq!(b.doc.kernel().count("extrude"), extrusions);
    // A new command drops the redo steps.
    b.doc.undo();
    b.doc.set_parameter("d1", 1.0).unwrap();
    assert!(b.doc.redo().is_none());
}

#[test]
fn fillets_follow_their_edges_and_a_body_takes_several() {
    let mut b = block();
    let corner1 = corner(b.extrude, 1, 1);
    let fillet = b.fillet(&[&corner1], 3.0);
    assert_eq!(b.history(b.body), "fillet(prism(F2:0..20),3)");
    b.doc.set_parameter("d1", 80.0).unwrap();
    assert_eq!(b.evaluated(), vec![b.sketch, b.extrude, fillet]);
    assert_eq!(b.status(fillet), FeatureStatus::Ok);

    // A second fillet rounds an edge the first one made.
    let made = EdgeName::new(
        FaceName::fillet(fillet, corner1.clone()),
        FaceName::end(b.extrude, b.region.clone()),
    );
    let second = b.fillet(&[&made], 1.0);
    assert_eq!(b.status(second), FeatureStatus::Ok);
    assert_eq!(b.history(b.body), "fillet(fillet(prism(F2:0..20),3),1)");
    assert_eq!(b.doc.bodies().len(), 1);
}

#[test]
fn a_failed_feature_is_skipped_and_reported() {
    let mut b = block();
    b.doc.kernel().fail.borrow_mut().insert("fillet");
    let fillet = b.fillet(&[&corner(b.extrude, 1, 0)], 50.0);
    assert_eq!(
        b.status(fillet),
        FeatureStatus::Failed("fillet failed".to_owned())
    );
    assert_eq!(
        b.doc.stats().error.unwrap().to_string(),
        "Fillet1: fillet failed"
    );
    // The body passes the failed fillet by, and later features build on it.
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
    let cut = b.circle_extrude("cut", &[b.body]);
    assert_eq!(b.status(cut), FeatureStatus::Ok);
    assert_eq!(b.history(b.body), "cut(prism(F2:0..20),prism(F5:0..30))");

    // Once the fillet works (its radius changes), the chain rebuilds on it.
    b.doc.kernel().fail.borrow_mut().clear();
    b.doc.set_parameter("d4", 2.0).unwrap();
    assert_eq!(b.evaluated(), vec![fillet, cut]);
    assert!(b.doc.stats().error.is_none());
    assert_eq!(
        b.history(b.body),
        "cut(fillet(prism(F2:0..20),2),prism(F5:0..30))"
    );
}

#[test]
fn a_failed_sketch_fails_what_uses_it_until_it_works_again() {
    let mut b = block();
    b.doc.set_parameter("d1", 0.0).unwrap();
    assert_eq!(
        b.status(b.sketch),
        FeatureStatus::Failed("d1 must be greater than zero, got 0".to_owned())
    );
    assert_eq!(
        b.status(b.extrude),
        FeatureStatus::Failed("Sketch1 has no result".to_owned())
    );
    assert!(b.doc.bodies().is_empty());
    assert_eq!(b.doc.stats().error.unwrap().name, "Sketch1");
    // Back to the old value: everything comes from the cache.
    b.doc.set_parameter("d1", 60.0).unwrap();
    assert!(b.evaluated().is_empty());
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
}

#[test]
fn a_lost_edge_names_the_feature_that_removed_it() {
    let mut b = block();
    let (_, _, join) = b.join_boss();
    let corner1 = corner(b.extrude, 1, 1);
    b.doc.kernel().lose.borrow_mut().push(side(b.extrude, 1, 1));
    // The lost face changes nothing the cache knows of; edit the join.
    b.doc.set_parameter("d6", 31.0).unwrap();
    let fillet = b.fillet(&[&corner1], 2.0);
    assert_eq!(
        b.status(fillet),
        FeatureStatus::Failed(format!("edge {corner1} no longer exists after Extrude2"))
    );
    assert_eq!(b.doc.kernel().count("fillet"), 0);
    assert!(b.history(b.body).starts_with("join("));
    let _ = join;

    // An edge that never existed.
    let mut b = block();
    let nowhere = EdgeName::new(side(b.extrude, 1, 0), side(b.extrude, 1, 2));
    let fillet = b.fillet(&[&nowhere], 2.0);
    assert_eq!(
        b.status(fillet),
        FeatureStatus::Failed(format!("the body has no edge {nowhere}"))
    );
}

#[test]
fn join_cut_and_intersect_change_their_participants() {
    let mut b = block();
    let (_, boss_region, join) = b.join_boss();
    assert_eq!(b.doc.bodies().len(), 1);
    assert_eq!(b.history(b.body), "join(prism(F2:0..20),prism(F4:0..30))");
    // A boss edge where it meets the block can be rounded.
    let fillet = b.fillet(&[&top_edge(join, 1, &boss_region, 1)], 1.0);
    assert_eq!(b.status(fillet), FeatureStatus::Ok);

    // A cut that splits the body: the first piece keeps it, the second is a
    // new body named after the next free number.
    b.doc.kernel().split.set(Some(2));
    let cut = b.circle_extrude("cut", &[b.body]);
    let bodies = b.doc.bodies();
    let ids: Vec<_> = bodies
        .iter()
        .map(|b| (b.uid.to_string(), b.name.clone()))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("F2.b0".to_owned(), "Body1".to_owned()),
            (format!("{cut}.b0"), "Body2".to_owned())
        ]
    );
    assert!(b.history(b.body).ends_with("#0"));
    assert!(b.history(BodyUid::new(cut, 0)).ends_with("#1"));

    // A cut through all of a body removes it; all bodies take part.
    b.doc.kernel().split.set(Some(0));
    let remove = b.circle_extrude("cut", &[]);
    assert_eq!(b.status(remove), FeatureStatus::Ok);
    assert!(b.doc.bodies().is_empty());
    b.doc.undo();
    b.doc.undo();
    b.doc.undo();

    // An intersection keeps the common part.
    b.doc.kernel().split.set(None);
    let common = b.circle_extrude("intersect", &[b.body]);
    assert_eq!(b.status(common), FeatureStatus::Ok);
    assert!(b.history(b.body).starts_with("common("));

    // A tool that touches nothing fails, and the bodies stay as they were.
    b.doc.kernel().touch.replace(Some(vec![false, false]));
    let miss = b.circle_extrude("cut", &[]);
    assert_eq!(
        b.status(miss),
        FeatureStatus::Failed("the extrusion does not cut into any participant body".to_owned())
    );
}

#[test]
fn join_merges_the_bodies_it_touches() {
    let mut b = block();
    let second = b.circle_extrude("new_body", &[]); // Body2
    let names: Vec<_> = b.doc.bodies().into_iter().map(|b| b.name).collect();
    assert_eq!(names, vec!["Body1", "Body2"]);
    let bridge = b.circle_extrude("join", &[]);
    assert_eq!(b.status(bridge), FeatureStatus::Ok);
    let bodies = b.doc.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].uid, b.body);
    assert!(b.doc.body_shape(BodyUid::new(second, 0)).is_none());
    // The merged body keeps its name; renaming changes it.
    b.doc.rename_body(b.body, "Base").unwrap();
    assert_eq!(b.doc.bodies()[0].name, "Base");
    assert!(b.doc.rename_body(b.body, " ").is_err());
}

#[test]
fn several_profiles_make_a_body_each() {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let rect = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap();
    let circle = doc.add_circle(sketch, [30.0, 5.0], &num(8.0)).unwrap();
    // Points take ids too: the rectangle's corners are p5..p8.
    assert_eq!(circle.curves, vec![EntityUid(9)]);
    assert_eq!(circle.region.to_string(), "r{c9}");
    let uid = doc
        .add_feature(
            &extrude(
                sketch,
                &[&rect.region, &circle.region],
                5.0,
                "new_body",
                &[],
            ),
            None,
        )
        .unwrap()
        .uid;
    let bodies: Vec<_> = doc.bodies().into_iter().map(|b| (b.uid, b.name)).collect();
    assert_eq!(
        bodies,
        vec![
            (BodyUid::new(uid, 0), "Body1".to_owned()),
            (BodyUid::new(uid, 1), "Body2".to_owned())
        ]
    );
    let profiles = doc.profiles();
    assert_eq!(profiles.len(), 2);
    assert!(profiles.iter().all(|p| p.consumed));
}

#[test]
fn extents_set_the_start_and_end_of_the_extrusion() {
    let cases = [
        (
            json!({"type": "symmetric", "distance": 10.0}),
            "prism(F2:-10..10)",
        ),
        (
            json!({"type": "symmetric", "distance": 10.0, "full_length": true}),
            "prism(F2:-5..5)",
        ),
        (
            // Two sides run against the direction, so that the far cap of
            // side one is the start.
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 20.0},
                   "side2": {"type": "distance", "distance": 5.0}}),
            "prism(F2:-20..5)",
        ),
        (
            json!({"type": "distance", "distance": -7.0}),
            "prism(F2:0..7)",
        ),
    ];
    for (extent, expected) in cases {
        let mut doc = Document::new(MockKernel::default());
        let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = doc
            .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
            .unwrap()
            .region;
        let extrude = def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                                 "extent": extent, "operation": "new_body"}),
        );
        let uid = doc.add_feature(&extrude, None).unwrap().uid;
        let shape = doc.body_shape(BodyUid::new(uid, 0)).unwrap();
        assert_eq!(shape.history, expected);
        if expected == "prism(F2:0..7)" {
            // A negative distance goes down from the sketch plane.
            let bounds = shape.bounds.unwrap();
            assert_eq!((bounds.min[2], bounds.max[2]), (-7.0, 0.0));
        }
    }

    // Through all: past the far side of every body, either way.
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [30.0, 20.0], &num(10.0))
        .unwrap()
        .region;
    for (both_sides, expected) in [(false, "prism(F4:0..26)"), (true, "prism(F4:-26..26)")] {
        let cut = def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                             "extent": {"type": "through_all", "both_sides": both_sides},
                             "operation": "cut"}),
        );
        let uid = b.doc.add_feature(&cut, None).unwrap().uid;
        assert_eq!(b.status(uid), FeatureStatus::Ok);
        assert_eq!(
            b.history(b.body),
            format!("cut(prism(F2:0..20),{expected})")
        );
        b.doc.undo();
    }
    // Flipped, nothing lies in the way.
    let flipped = def(
        json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                             "extent": {"type": "through_all"}, "flip": true, "operation": "cut"}),
    );
    let uid = b.doc.add_feature(&flipped, None).unwrap().uid;
    assert_eq!(
        b.status(uid),
        FeatureStatus::Failed("there are no bodies in the extrude direction".to_owned())
    );
}

#[test]
fn suppressed_and_rolled_back_features_are_skipped() {
    let mut b = block();
    let fillet = b.fillet(&[&corner(b.extrude, 1, 0)], 2.0);
    b.doc.set_suppressed(fillet, true).unwrap();
    assert_eq!(b.status(fillet), FeatureStatus::Suppressed);
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
    assert_eq!(b.doc.undo_label(), Some("Suppress Fillet1"));
    b.doc.set_suppressed(fillet, false).unwrap();
    assert!(b.evaluated().is_empty());
    assert_eq!(b.history(b.body), "fillet(prism(F2:0..20),2)");

    // The marker rolls back the fillet; a new feature goes in at the marker.
    b.doc.set_marker(2).unwrap();
    assert_eq!(b.status(fillet), FeatureStatus::RolledBack);
    assert_eq!(b.history(b.body), "prism(F2:0..20)");
    let cut = b.circle_extrude("cut", &[b.body]);
    let order: Vec<_> = b.doc.features().map(|f| f.name.clone()).collect();
    assert_eq!(
        order,
        vec!["Sketch1", "Extrude1", "Sketch2", "Extrude2", "Fillet1"]
    );
    assert_eq!(b.doc.marker(), 4);
    assert_eq!(b.status(cut), FeatureStatus::Ok);
    b.doc.set_marker(5).unwrap();
    assert_eq!(
        b.history(b.body),
        "fillet(cut(prism(F2:0..20),prism(F5:0..30)),2)"
    );
    assert!(b.doc.set_marker(6).is_err());
}

#[test]
fn editing_a_feature_keeps_its_parameters_and_recomputes_after_it() {
    let mut b = block();
    let fillet = b.fillet(&[&corner(b.extrude, 1, 0)], 2.0);
    let count = b.doc.parameters().len();
    // A number in the slot of an owned parameter changes that parameter.
    let edited = extrude(b.sketch, &[&b.region], 30.0, "new_body", &[]);
    assert!(b.doc.edit_feature(b.extrude, &edited).unwrap().is_empty());
    assert_eq!(b.doc.parameters().len(), count);
    assert_eq!(b.evaluated(), vec![b.extrude, fillet]);
    assert_eq!(b.history(b.body), "fillet(prism(F2:0..30),2)");
    assert_eq!(b.doc.undo_label(), Some("Edit Extrude1"));

    // Referring to another parameter releases the old one.
    let mut by_name = extrude(b.sketch, &[&b.region], 0.0, "new_body", &[]);
    if let FeatureDef::Extrude(e) = &mut by_name {
        e.extent = crate::features::Extent::Distance {
            distance: ValueInput::Name("d1".to_owned()),
            taper: None,
        };
    }
    b.doc.edit_feature(b.extrude, &by_name).unwrap();
    assert!(b.doc.parameters().find("d3").is_none());
    assert_eq!(b.history(b.body), "fillet(prism(F2:0..60),2)");
    // An edit that breaks a later feature is refused.
    let mut moved = by_name.clone();
    if let FeatureDef::Extrude(e) = &mut moved {
        e.profiles.clear();
    }
    assert!(b.doc.edit_feature(b.extrude, &moved).is_err());
    let wrong_type = def(
        json!({"type": "fillet", "body": "F2.b0", "edges": [corner(b.extrude, 1, 0)], "radius": 1.0}),
    );
    assert_eq!(
        b.doc
            .edit_feature(b.extrude, &wrong_type)
            .unwrap_err()
            .to_string(),
        "cannot change Extrude1 (extrude) into a fillet"
    );
}

#[test]
fn deleting_refuses_dependents_unless_asked() {
    let mut b = block();
    let fillet = b.fillet(&[&corner(b.extrude, 1, 0)], 2.0);
    let error = b.doc.delete_feature(b.extrude, false).unwrap_err();
    assert_eq!(
        error,
        ModelError::Dependents {
            feature: "Extrude1".to_owned(),
            dependents: vec!["Fillet1".to_owned()]
        }
    );
    assert_eq!(
        error.to_string(),
        "Extrude1 is used by Fillet1; delete them too, or change them first"
    );
    assert_eq!(
        b.doc.delete_feature(b.extrude, true).unwrap(),
        vec![b.extrude, fillet]
    );
    let names: Vec<_> = b.doc.parameters().iter().map(|p| p.name()).collect();
    assert_eq!(names, vec!["d1", "d2"]);
    assert!(b.doc.bodies().is_empty());
    assert_eq!(b.doc.marker(), 1);
    b.doc.undo();
    assert_eq!(b.doc.features().count(), 3);
    assert_eq!(b.doc.bodies()[0].name, "Body1");
    // A feature without dependents just goes.
    assert_eq!(b.doc.delete_feature(fillet, false).unwrap(), vec![fillet]);
    assert!(b.doc.delete_feature(FeatureUid(42), false).is_err());
}

#[test]
fn reordering_keeps_references_pointing_back() {
    let mut b = block();
    let sketch2 = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let fillet = b.fillet(&[&corner(b.extrude, 1, 0)], 2.0);
    let error = b.doc.move_feature(fillet, 1).unwrap_err().to_string();
    assert_eq!(
        error,
        "Fillet1 cannot move there: Fillet1: Extrude1 (F2) does not come before this feature in the timeline"
    );
    assert!(b.doc.move_feature(b.sketch, 2).is_err());
    b.doc.move_feature(sketch2, 0).unwrap();
    let order: Vec<_> = b.doc.features().map(|f| f.name.clone()).collect();
    assert_eq!(order, vec!["Sketch2", "Sketch1", "Extrude1", "Fillet1"]);
    assert_eq!(b.doc.undo_label(), Some("Move Sketch2"));
    assert!(b.evaluated().is_empty());
    assert!(b.doc.move_feature(sketch2, 9).is_err());
}

#[test]
fn rejected_commands_change_nothing() {
    let mut b = block();
    let features = b.doc.features().count();
    let params = b.doc.parameters().len();
    let undo = b.doc.undo_label().map(str::to_owned);
    let sketch = b.sketch;
    let body = b.body;
    let region = b.region.clone();
    let rejected: Vec<(FeatureDef<ValueInput>, &str)> = vec![
        (
            extrude(sketch, &[&"r{c9}".parse().unwrap()], 5.0, "new_body", &[]),
            "Sketch1 has no profile r{c9}",
        ),
        (
            extrude(b.extrude, &[&region], 5.0, "new_body", &[]),
            "Extrude1 (F2) is not a sketch",
        ),
        (
            extrude(FeatureUid(9), &[&region], 5.0, "new_body", &[]),
            "feature F9 does not exist",
        ),
        (
            extrude(sketch, &[&region], 0.0, "new_body", &[]),
            "extrude distance must not be zero",
        ),
        (
            extrude(sketch, &[&region], 5.0, "new_body", &[body]),
            "a new body has no participant bodies",
        ),
        (
            extrude(sketch, &[&region, &region], 5.0, "new_body", &[]),
            "is listed more than once",
        ),
        (
            def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                       "extent": {"type": "distance", "distance": "d99"}, "operation": "cut"}),
            ),
            "extent.distance: parameter 'd99' does not exist",
        ),
        (
            def(json!({"type": "fillet", "body": body, "edges": [], "radius": 1.0})),
            "no edges selected",
        ),
        (
            def(
                json!({"type": "fillet", "body": "F1.b0", "edges": [corner(b.extrude, 1, 0)], "radius": 1.0}),
            ),
            "Sketch1 (F1) does not create bodies",
        ),
        (
            def(
                json!({"type": "fillet", "body": body, "edges": [corner(FeatureUid(9), 1, 0)], "radius": 1.0}),
            ),
            "feature F9 does not exist",
        ),
        (
            def(
                json!({"type": "fillet", "body": body, "edges": [corner(b.extrude, 1, 0)], "radius": -1.0}),
            ),
            "fillet radius must be greater than zero, got -1",
        ),
        (
            def(
                json!({"type": "chamfer", "body": body, "edges": [corner(b.extrude, 1, 0)],
                       "size": {"type": "distance_angle", "distance": 1.0, "angle": 2.0}}),
            ),
            "the chamfer angle must be between 0 and 90 degrees",
        ),
    ];
    for (definition, expected) in rejected {
        let error = b
            .doc
            .add_feature(&definition, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
    assert!(
        b.doc
            .add_feature(
                &extrude(sketch, &[&region], 1.0, "new_body", &[]),
                Some("Sketch1")
            )
            .is_err()
    );
    let error = b
        .doc
        .add_rectangle(sketch, [0.0, 0.0], &num(0.0), &num(5.0))
        .unwrap_err();
    assert_eq!(error.to_string(), "length must be greater than zero, got 0");
    assert!(
        b.doc
            .add_circle(sketch, [f64::NAN, 0.0], &num(5.0))
            .is_err()
    );
    assert!(b.doc.add_circle(b.extrude, [0.0, 0.0], &num(5.0)).is_err());
    assert!(b.doc.set_parameter("nope", 1.0).is_err());
    assert!(b.doc.set_parameter("d1", f64::INFINITY).is_err());
    assert!(b.doc.add_parameter("d1", 1.0, "").is_err());
    assert_eq!(
        b.doc.delete_parameter("d1").unwrap_err().to_string(),
        "parameter 'd1' is used by Sketch1"
    );

    assert_eq!(b.doc.features().count(), features);
    assert_eq!(b.doc.parameters().len(), params);
    assert_eq!(b.doc.undo_label().map(str::to_owned), undo);
}

#[test]
fn parameters_and_features_can_be_renamed() {
    let mut b = block();
    b.doc.add_parameter("wall", 2.5, "user").unwrap();
    b.doc.rename_parameter("d3", "height").unwrap();
    // Features refer to parameters by id, so they follow the rename.
    b.doc.set_parameter("height", 25.0).unwrap();
    assert_eq!(b.history(b.body), "prism(F2:0..25)");
    b.doc.rename_feature(b.extrude, "Base").unwrap();
    assert_eq!(b.doc.feature(b.extrude).unwrap().name, "Base");
    assert!(b.doc.rename_feature(b.extrude, "Sketch1").is_err());
    b.doc.delete_parameter("wall").unwrap();
    assert!(b.doc.parameters().find("wall").is_none());
    // Default names count per type and skip used ones.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap();
    assert_eq!(sketch.name, "Sketch2");
}

#[test]
fn expressions_drive_features_and_recompute_only_on_value_changes() {
    let mut b = block();
    // The extrude distance follows the sketch width.
    let changed = b
        .doc
        .set_parameter_expression("d3", "d1 / 3 + 5 mm", None)
        .unwrap();
    assert_eq!(changed, vec!["d3"]);
    assert_eq!(b.history(b.body), "prism(F2:0..25)");
    assert_eq!(b.evaluated(), vec![b.extrude]);
    b.doc.set_parameter("d1", 90.0).unwrap();
    assert_eq!(b.evaluated(), vec![b.sketch, b.extrude]);
    assert_eq!(b.history(b.body), "prism(F2:0..35)");
    // The same value in another form changes the definition, not the
    // geometry: nothing recomputes.
    let recomputes = b.doc.recompute_count();
    assert!(
        b.doc
            .set_parameter_expression("d3", "(d1 / 3 + 5 mm)", None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(b.doc.recompute_count(), recomputes);
    assert_eq!(b.doc.undo_label(), Some("Change d3"));
    // Renames rewrite references; features follow by id.
    b.doc.rename_parameter("d1", "width").unwrap();
    assert_eq!(b.doc.recompute_count(), recomputes);
    let params = b.doc.parameters();
    let d3 = params.get(params.find("d3").unwrap()).unwrap();
    assert_eq!(d3.expression(), "(width / 3 + 5 mm)");
    // A parameter another one uses cannot go; cycles and unit mismatches
    // are refused and change nothing.
    b.doc
        .add_parameter_expression("wall", "2 mm", None, "")
        .unwrap();
    b.doc
        .add_parameter_expression("rim", "wall * 2", None, "")
        .unwrap();
    assert_eq!(
        b.doc.delete_parameter("wall").unwrap_err().to_string(),
        "parameter 'wall' is used by rim"
    );
    assert_eq!(
        b.doc
            .set_parameter_expression("wall", "rim / 2", None)
            .unwrap_err()
            .to_string(),
        "circular reference: wall -> rim -> wall"
    );
    assert!(
        b.doc
            .set_parameter_expression("width", "30 deg", None)
            .unwrap_err()
            .to_string()
            .contains("expected length, found angle"),
    );
    assert!(
        b.doc
            .set_parameter_expression("width", "60", Some(crate::expr::Unit::DEG))
            .unwrap_err()
            .to_string()
            .contains("Sketch1 uses it as a length")
    );
    // An angle parameter has degrees; a unitless one has no unit.
    b.doc
        .add_parameter_expression("tilt", "PI / 4 * 1 rad", None, "")
        .unwrap();
    b.doc
        .add_parameter_expression("count", "3", None, "")
        .unwrap();
    let params = b.doc.parameters();
    let unit = |name: &str| params.get(params.find(name).unwrap()).unwrap().unit();
    assert_eq!(unit("tilt"), crate::expr::Unit::DEG);
    assert_eq!(unit("count"), crate::expr::Unit::NONE);
    // A length slot cannot take an angle parameter.
    let error = b
        .doc
        .add_feature(&extrude(b.sketch, &[&b.region], 1.0, "new_body", &[]), None)
        .map(|_| ());
    assert!(error.is_ok());
    let tilted = def(
        json!({"type": "extrude", "profiles": [{"sketch": b.sketch, "region": b.region}],
                            "extent": {"type": "distance", "distance": "tilt"}, "operation": "new_body"}),
    );
    assert_eq!(
        b.doc.add_feature(&tilted, None).unwrap_err().to_string(),
        "extent.distance: parameter 'tilt' is an angle, not a length"
    );
}

#[test]
fn commands_take_expressions_for_values() {
    let mut b = block();
    // An expression that is not a parameter name becomes the feature's own
    // dimension parameter.
    let fillet = def(json!({"type": "fillet", "body": b.body,
                            "edges": [corner(b.extrude, 1, 0)], "radius": "d3 / 10"}));
    let added = b.doc.add_feature(&fillet, None).unwrap();
    assert_eq!(added.parameters, vec!["d4"]);
    let params = b.doc.parameters();
    let d4 = params.get(params.find("d4").unwrap()).unwrap();
    assert_eq!((d4.expression(), d4.value()), ("d3 / 10", 2.0));
    assert_eq!(params.owner(d4.id()), Some(added.uid));
    let d4 = d4.id();
    // Editing with another expression changes that parameter.
    let fillet = def(json!({"type": "fillet", "body": b.body,
                            "edges": [corner(b.extrude, 1, 0)], "radius": "d3 / 5"}));
    assert!(b.doc.edit_feature(added.uid, &fillet).unwrap().is_empty());
    assert_eq!(b.doc.parameters().value(d4), Some(4.0));
    let bad = def(json!({"type": "fillet", "body": b.body,
                         "edges": [corner(b.extrude, 1, 0)], "radius": "d3 +"}));
    assert!(
        b.doc
            .edit_feature(added.uid, &bad)
            .unwrap_err()
            .to_string()
            .starts_with("radius: parameter 'd4': ")
    );
    // Deleting the fillet deletes its parameter; d3 stays (the extrude).
    b.doc.delete_feature(added.uid, false).unwrap();
    assert!(b.doc.parameters().find("d4").is_none());
    // In an inch document, new dimensions show inches.
    b.doc.set_units(crate::expr::LengthUnit::Inch).unwrap();
    let fillet = def(json!({"type": "fillet", "body": b.body,
                            "edges": [corner(b.extrude, 1, 0)], "radius": 2.54}));
    b.doc.add_feature(&fillet, None).unwrap();
    let params = b.doc.parameters();
    let d4 = params.get(params.find("d4").unwrap()).unwrap();
    assert_eq!((d4.expression(), d4.value()), ("0.1 in", 2.54));
}

#[test]
fn previews_do_not_commit_and_the_commit_reuses_them() {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_circle(sketch, [30.0, 20.0], &num(10.0))
        .unwrap()
        .region;
    let cut = extrude(sketch, &[&region], 30.0, "cut", &[b.body]);
    let features = b.doc.features().count();
    let params = b.doc.parameters().len();
    let report = b.doc.preview_add(&cut, None).unwrap();
    assert_eq!(report.name, "Extrude2");
    assert_eq!(report.status, FeatureStatus::Ok);
    assert!(report.tool);
    assert_eq!(report.bodies, vec![(b.body, "Body1".to_owned(), true)]);
    assert_eq!(
        b.doc.preview_body(b.body).unwrap().history,
        "cut(prism(F2:0..20),prism(F4:0..30))"
    );
    assert_eq!(b.doc.preview_tool().unwrap().history, "prism(F4:0..30)");
    // Nothing changed.
    assert_eq!(b.doc.features().count(), features);
    assert_eq!(b.doc.parameters().len(), params);
    assert_eq!(b.history(b.body), "prism(F2:0..20)");

    let booleans = b.doc.kernel().count("cut");
    b.doc.add_feature(&cut, None).unwrap();
    assert!(b.evaluated().is_empty());
    assert_eq!(b.doc.kernel().count("cut"), booleans);
    assert!(b.doc.preview_body(b.body).is_none());

    // A failing preview reports the error.
    b.doc.kernel().fail.borrow_mut().insert("fillet");
    let fillet = def(
        json!({"type": "fillet", "body": b.body, "edges": [corner(b.extrude, 1, 0)], "radius": 1.0}),
    );
    let report = b.doc.preview_add(&fillet, None).unwrap();
    assert_eq!(
        report.status,
        FeatureStatus::Failed("fillet failed".to_owned())
    );
    // An invalid one is rejected like the command.
    let bad = def(json!({"type": "fillet", "body": b.body, "edges": [], "radius": 1.0}));
    assert!(b.doc.preview_add(&bad, None).is_err());
}

#[test]
fn chamfers_bevel_named_edges() {
    let mut b = block();
    let chamfer = def(json!({"type": "chamfer", "body": b.body,
                             "edges": [top_edge(b.extrude, 1, &b.region, 0)],
                             "size": {"type": "two_distances", "distance1": 1.0, "distance2": 3.0},
                             "flip": true}));
    let added = b.doc.add_feature(&chamfer, None).unwrap();
    assert_eq!(added.name, "Chamfer1");
    assert_eq!(added.parameters, vec!["d4", "d5"]);
    assert_eq!(b.status(added.uid), FeatureStatus::Ok);
    assert_eq!(
        b.history(b.body),
        "chamfer(prism(F2:0..20),TwoDistances { distance1: 1.0, distance2: 3.0 },flip)"
    );
    let comments: Vec<_> = b.doc.parameters().iter().map(|p| p.comment()).collect();
    assert_eq!(comments[3..], ["Chamfer1 distance1", "Chamfer1 distance2"]);
}

// Revisions and the saved state (P8).
#[test]
fn revisions_tell_the_saved_state_through_undo_and_redo() {
    // An empty document has nothing to save.
    assert!(!Document::new(MockKernel::default()).is_modified());

    let mut b = block();
    assert!(b.doc.is_modified());
    b.doc.mark_saved();
    assert!(!b.doc.is_modified());
    let saved = b.doc.revision();

    // A command gives a new revision; undo takes the saved one back and
    // redo the command's.
    b.doc.set_parameter("d3", 35.0).unwrap();
    let changed = b.doc.revision();
    assert_ne!(changed, saved);
    assert!(b.doc.is_modified());
    b.doc.undo();
    assert_eq!(b.doc.revision(), saved);
    assert!(!b.doc.is_modified());
    b.doc.redo();
    assert_eq!(b.doc.revision(), changed);
    assert!(b.doc.is_modified());

    // Saved after the change: undo now leaves the saved state.
    b.doc.mark_saved();
    assert!(!b.doc.is_modified());
    b.doc.undo();
    assert!(b.doc.is_modified());
    // A new command gets a revision no state had, even for the same
    // definition: only undo and redo restore a state.
    b.doc.set_parameter("d3", 35.0).unwrap();
    assert!(![saved, changed].contains(&b.doc.revision()));
    assert!(b.doc.is_modified());
}

#[test]
fn only_a_changed_definition_is_a_new_revision() {
    let mut b = block();
    b.doc.mark_saved();
    let revision = b.doc.revision();
    // Rejected commands and commands that change nothing.
    assert!(b.doc.set_parameter("d99", 1.0).is_err());
    assert!(b.doc.set_marker(99).is_err());
    let rejected = extrude(b.sketch, &[&"r{c9}".parse().unwrap()], 5.0, "new_body", &[]);
    assert!(b.doc.add_feature(&rejected, None).is_err());
    b.doc.set_parameter("d3", 20.0).unwrap();
    b.doc.set_marker(b.doc.marker()).unwrap();
    // Recompute and previews.
    b.doc.recompute();
    let fillet = def(
        json!({"type": "fillet", "body": b.body, "edges": [corner(b.extrude, 1, 0)], "radius": 1.0}),
    );
    b.doc.preview_add(&fillet, None).unwrap();
    assert_eq!(b.doc.revision(), revision);
    b.doc.clear_preview();
    // Saving again, and the project file.
    b.doc.mark_saved();
    let _ = b.doc.to_json();
    assert_eq!(b.doc.revision(), revision);
    assert!(!b.doc.is_modified());

    // Steps merged into one: the state before them keeps its revision, the
    // state after them its own.
    let depth = b.doc.undo_depth();
    b.doc.set_parameter("d1", 70.0).unwrap();
    b.doc.set_parameter("d2", 50.0).unwrap();
    let merged = b.doc.revision();
    b.doc.merge_undo(depth, "Change Size");
    assert_eq!(b.doc.revision(), merged);
    assert!(b.doc.is_modified());
    assert_eq!(b.doc.undo().as_deref(), Some("Change Size"));
    assert_eq!(b.doc.revision(), revision);
    assert!(!b.doc.is_modified());
    b.doc.redo();
    assert_eq!(b.doc.revision(), merged);

    // Commands that recompute nothing are changes too.
    b.doc.undo();
    b.doc.rename_parameter("d1", "width").unwrap();
    assert!(b.doc.is_modified());
    b.doc.undo();
    assert!(!b.doc.is_modified());
}

#[test]
fn a_loaded_document_is_modified_until_marked_saved() {
    let b = block();
    let mut loaded = Document::from_json(&b.doc.to_json(), MockKernel::default()).unwrap();
    // It may be an imported design or recovered work rather than the file
    // it is saved to.
    assert!(loaded.is_modified());
    loaded.recompute();
    loaded.mark_saved();
    assert!(!loaded.is_modified());
    loaded.set_parameter("d3", 35.0).unwrap();
    assert!(loaded.is_modified());
    loaded.undo();
    assert!(!loaded.is_modified());
}
