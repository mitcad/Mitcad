// SPDX-License-Identifier: MIT
//! An extrude whose operation an edit changes takes the features that work
//! only on its geometry along (mitcad#2185), with the mock kernel: Join to
//! New Component moves its fillets into the component it makes, on its
//! body; New Body sets them on its body; the reverse edits take them back;
//! a feature that also names the old body's own faces stays; undo restores
//! everything in one step; the timeline marks the extrude by the component
//! it makes.

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::test_support::{Block, block, def, extrude, side};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The edge between sides `i` and `j` of a feature made from the block's
/// rectangle (c1..c4).
fn edge(feature: FeatureUid, i: usize, j: usize) -> EdgeName {
    EdgeName::new(side(feature, 1, i), side(feature, 1, j))
}

fn fillet(body: BodyUid, edges: &[EdgeName]) -> Value {
    let edges: Vec<String> = edges.iter().map(ToString::to_string).collect();
    json!({"type": "fillet", "body": body, "edges": edges, "radius": 1})
}

/// The block (Sketch1 F1, Extrude1 F2 into Body1 F2.b0) with Extrude2
/// (F3) joined into Body1, Fillet1 (F4) on the edge of two of Extrude2's
/// sides and Fillet2 (F5) on the edge of Fillet1's face and one of those
/// sides: both work only on Extrude2's geometry.
struct Rig {
    b: Block,
    extrude: FeatureUid,
    fillet1: FeatureUid,
    fillet2: FeatureUid,
}

fn rig() -> Rig {
    let mut b = block();
    let joined = extrude(b.sketch, &[&b.region], 5.0, "join", &[b.body]);
    let ex = b.doc.add_feature(&joined, None).unwrap().uid;
    let first = edge(ex, 0, 1);
    let fillet1 = b
        .doc
        .add_feature(&def(fillet(b.body, std::slice::from_ref(&first))), None)
        .unwrap()
        .uid;
    let rounded = FaceName::fillet(fillet1, first);
    let second = EdgeName::new(rounded, side(ex, 1, 0));
    let fillet2 = b
        .doc
        .add_feature(&def(fillet(b.body, &[second])), None)
        .unwrap()
        .uid;
    for uid in [ex, fillet1, fillet2] {
        assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok), "{uid}");
    }
    Rig {
        b,
        extrude: ex,
        fillet1,
        fillet2,
    }
}

impl Rig {
    fn edit(&mut self, operation: &str) {
        let participants: &[BodyUid] = if operation == "join" {
            &[self.b.body]
        } else {
            &[]
        };
        let edited = extrude(
            self.b.sketch,
            &[&self.b.region],
            5.0,
            operation,
            participants,
        );
        self.b.doc.edit_feature(self.extrude, &edited).unwrap();
    }

    /// The body a fillet works on and the component it is in.
    fn fillet_at(&self, uid: FeatureUid) -> (BodyUid, ComponentUid) {
        let entry = self.b.doc.feature(uid).unwrap();
        let FeatureDef::Fillet(fillet) = &entry.def else {
            panic!("{uid} is no fillet");
        };
        (fillet.body, entry.component)
    }

    fn fillets_at(&self, body: BodyUid, component: ComponentUid) {
        for uid in [self.fillet1, self.fillet2] {
            assert_eq!(self.fillet_at(uid), (body, component), "{uid}");
            assert_eq!(self.b.doc.status(uid), Some(&FeatureStatus::Ok), "{uid}");
        }
    }

    fn timeline_item(&self, uid: FeatureUid) -> Value {
        let timeline = query(&self.b.doc, json!({"query": "timeline"}));
        timeline["features"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["uid"] == json!(uid))
            .cloned()
            .unwrap()
    }
}

#[test]
fn join_edited_to_new_component_takes_its_fillets_into_the_component() {
    let mut r = rig();
    r.edit("new_component");
    let made = r.b.doc.assembly().created_by(r.extrude).unwrap();
    let own = BodyUid::new(r.extrude, 0);
    assert_eq!(r.b.doc.body_component(own), Some(made));
    r.fillets_at(own, made);
    assert_eq!(r.b.doc.status(r.extrude), Some(&FeatureStatus::Ok));
    // Body1 keeps its own history, in the root.
    assert_eq!(r.b.doc.body_component(r.b.body), Some(ComponentUid::ROOT));
    // The timeline: the extrude by the component it makes, the fillets in
    // it.
    let item = r.timeline_item(r.extrude);
    assert_eq!(item["made_component"], json!(made), "{item}");
    assert!(item.get("component").is_none(), "{item}");
    for uid in [r.fillet1, r.fillet2] {
        assert_eq!(r.timeline_item(uid)["component"], json!(made));
    }
    // One undo step brings everything back.
    r.b.doc.undo();
    assert!(r.b.doc.assembly().components.is_empty());
    r.fillets_at(r.b.body, ComponentUid::ROOT);
    assert!(r.timeline_item(r.extrude).get("made_component").is_none());
    r.b.doc.redo();
    r.fillets_at(own, made);
}

#[test]
fn new_component_edited_back_to_join_takes_the_fillets_back() {
    let mut r = rig();
    r.edit("new_component");
    r.edit("join");
    assert!(r.b.doc.assembly().components.is_empty());
    r.fillets_at(r.b.body, ComponentUid::ROOT);
    assert!(r.timeline_item(r.extrude).get("made_component").is_none());
    for uid in [r.fillet1, r.fillet2] {
        assert!(r.timeline_item(uid).get("component").is_none());
    }
}

#[test]
fn join_edited_to_new_body_sets_the_fillets_on_its_body() {
    let mut r = rig();
    let own = BodyUid::new(r.extrude, 0);
    r.edit("new_body");
    assert!(r.b.doc.assembly().components.is_empty());
    r.fillets_at(own, ComponentUid::ROOT);
    // New Body to New Component moves them, their body the same.
    r.edit("new_component");
    let made = r.b.doc.assembly().created_by(r.extrude).unwrap();
    r.fillets_at(own, made);
    // And back to New Body, then Join.
    r.edit("new_body");
    assert!(r.b.doc.assembly().components.is_empty());
    r.fillets_at(own, ComponentUid::ROOT);
    r.edit("join");
    r.fillets_at(r.b.body, ComponentUid::ROOT);
}

#[test]
fn features_that_also_name_the_old_body_stay() {
    let mut r = rig();
    // Fillet3: an edge of Extrude2 and one of Extrude1 (Body1's own
    // faces); Fillet4: Extrude1's edge only.
    let mixed = fillet(r.b.body, &[edge(r.extrude, 2, 3), edge(r.b.extrude, 0, 1)]);
    let fillet3 = r.b.doc.add_feature(&def(mixed), None).unwrap().uid;
    let plain = fillet(r.b.body, &[edge(r.b.extrude, 2, 3)]);
    let fillet4 = r.b.doc.add_feature(&def(plain), None).unwrap().uid;
    assert_eq!(r.b.doc.status(fillet3), Some(&FeatureStatus::Ok));
    assert_eq!(r.b.doc.status(fillet4), Some(&FeatureStatus::Ok));
    r.edit("new_component");
    let made = r.b.doc.assembly().created_by(r.extrude).unwrap();
    r.fillets_at(BodyUid::new(r.extrude, 0), made);
    // Both stay on Body1 in the root; the mixed one reports the edge that
    // went with Extrude2, Body1's own fillet still builds.
    assert_eq!(r.fillet_at(fillet3), (r.b.body, ComponentUid::ROOT));
    assert_eq!(r.fillet_at(fillet4), (r.b.body, ComponentUid::ROOT));
    let error = r.b.doc.status(fillet3).unwrap().error().unwrap().to_owned();
    assert!(error.contains("edge"), "{error}");
    assert_eq!(r.b.doc.status(fillet4), Some(&FeatureStatus::Ok));
    // Back to Join: all as before.
    r.edit("join");
    r.fillets_at(r.b.body, ComponentUid::ROOT);
    assert_eq!(r.b.doc.status(fillet3), Some(&FeatureStatus::Ok));
}

#[test]
fn a_feature_of_the_made_component_on_none_of_its_geometry_keeps_it() {
    let mut r = rig();
    r.edit("new_component");
    let made = r.b.doc.assembly().created_by(r.extrude).unwrap();
    r.b.doc.activate_component(made).unwrap();
    let sketch = r.b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let joined = extrude(r.b.sketch, &[&r.b.region], 5.0, "join", &[r.b.body]);
    let error =
        r.b.doc
            .edit_feature(r.extrude, &joined)
            .unwrap_err()
            .to_string();
    assert!(error.contains("has features or occurrences"), "{error}");
    // Refused as a whole: the fillets are where they were.
    r.fillets_at(BodyUid::new(r.extrude, 0), made);
    r.b.doc.delete_feature(sketch, false).unwrap();
    r.edit("join");
    r.fillets_at(r.b.body, ComponentUid::ROOT);
}
