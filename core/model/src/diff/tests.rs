// SPDX-License-Identifier: MIT
//! Comparisons of designs (P12c) with the mock kernel: each kind of change
//! between a document's project file before and after commands.

use std::f64::consts::FRAC_PI_2;

use serde_json::{Value, json};

use super::*;
use crate::testing::{MockKernel, MockShape};
use crate::topo::{EdgeName, FaceName};
use crate::transform::Transform;
use crate::{
    BodyKind, BooleanOp, BooleanOutput, ExtrudeFeatureSpec, ExtrudeSpec, FilletSet, KernelError,
    MassProperties,
};

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn run<K: Kernel>(doc: &mut Document<K>, commands: Value) {
    doc.command(&commands.to_string())
        .unwrap_or_else(|e| panic!("{commands}: {e}"));
}

/// Sketch1 with a 60 x 40 rectangle (d1, d2), extruded 20 mm (d3).
fn block<K: Kernel>(kernel: K) -> Document<K> {
    let mut doc = Document::new(kernel);
    run(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
            {"cmd": "add_feature", "def": {"type": "extrude",
             "profiles": [{"sketch": "F1", "region": REGION}],
             "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

/// The comparison of a document's project file before and after
/// `commands`.
fn changed(doc: &mut Document<MockKernel>, commands: Value) -> DesignDiff {
    let before = doc.to_json();
    run(doc, commands);
    diff_files(&before, &doc.to_json()).unwrap()
}

fn texts<T>(items: &[T], text: impl Fn(&T) -> &str) -> Vec<&str> {
    items.iter().map(text).collect()
}

#[test]
fn the_same_design_has_no_differences() {
    let doc = block(MockKernel::default());
    let diff = diff_documents(&doc, &doc);
    assert!(diff.identical);
    assert_eq!(diff.summary, "no changes");
    assert_eq!(diff.to_text(), "No differences\n");
    // A file read back is the same design.
    let reread = diff_files(&doc.to_json(), &doc.to_json()).unwrap();
    assert!(reread.identical, "{reread:?}");
}

#[test]
fn a_parameter_change_shows_in_the_parameter_and_its_feature() {
    let mut doc = block(MockKernel::default());
    let diff = changed(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 25}),
    );
    assert!(!diff.identical);
    assert_eq!(diff.summary, "d3 20 mm -> 25 mm, 1 feature modified");
    let parameter = &diff.parameters[0];
    assert_eq!(diff.parameters.len(), 1);
    assert_eq!(
        (
            parameter.kind,
            parameter.name.as_str(),
            &parameter.fields[..]
        ),
        (Kind::Modified, "d3", &["expression", "value"][..])
    );
    assert_eq!(parameter.owner_name.as_deref(), Some("Extrude1"));
    assert_eq!(parameter.text, "d3 (Extrude1 distance): 20 mm -> 25 mm");
    let feature = &diff.features[0];
    assert_eq!(diff.features.len(), 1);
    assert_eq!((feature.kind, feature.uid), (Kind::Modified, FeatureUid(2)));
    assert!(feature.fields.is_empty(), "{:?}", feature.fields);
    assert_eq!(feature.text, "Extrude1 (F2): distance 20 mm -> 25 mm");
    let json = diff.to_json();
    assert_eq!(
        json["features"][0]["values"],
        json!([{"slot": "extent.distance", "label": "distance", "from_parameter": "d3",
                "to_parameter": "d3", "from": 20.0, "to": 25.0, "from_text": "20 mm",
                "to_text": "25 mm", "text": "distance 20 mm -> 25 mm"}])
    );
    assert_eq!(
        json["parameters"][0]["to"],
        json!({"expression": "25 mm", "unit": "mm", "value": 25.0, "text": "25 mm",
               "comment": "Extrude1 distance", "kind": "model", "owner": "F2", "favorite": false})
    );
    assert_eq!(
        diff.to_text(),
        "Summary: d3 20 mm -> 25 mm, 1 feature modified\n\
         Parameters:\n  d3 (Extrude1 distance): 20 mm -> 25 mm\n\
         Timeline:\n  Extrude1 (F2): distance 20 mm -> 25 mm\n"
    );
}

#[test]
fn parameters_are_added_deleted_renamed_and_follow_others() {
    let mut doc = block(MockKernel::default());
    run(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "wall", "expression": "d3 / 4"},
            {"cmd": "add_parameter", "name": "gap", "value": 2},
            {"cmd": "add_parameter", "name": "old", "value": 7, "comment": "unused"},
        ]),
    );
    let diff = changed(
        &mut doc,
        json!([
            {"cmd": "set_parameter", "name": "d3", "value": 24},
            {"cmd": "rename_parameter", "name": "d3", "new_name": "depth"},
            {"cmd": "rename_parameter", "name": "gap", "new_name": "clearance"},
            {"cmd": "delete_parameter", "name": "old"},
            {"cmd": "add_parameter", "name": "new", "value": 3},
        ]),
    );
    assert_eq!(
        texts(&diff.parameters, |p| &p.text),
        [
            "depth (Extrude1 distance): renamed from d3; 20 mm -> 24 mm",
            "wall: 5 mm -> 6 mm (depth / 4)",
            "clearance: renamed from gap",
            "new = 3 mm: added",
            "old (unused) = 7 mm: deleted",
        ]
    );
    let kinds: Vec<Kind> = diff.parameters.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            Kind::Modified,
            Kind::Modified,
            Kind::Modified,
            Kind::Added,
            Kind::Deleted
        ]
    );
    assert_eq!(diff.parameters[0].renamed_from.as_deref(), Some("d3"));
    assert_eq!(diff.parameters[1].fields, ["value"]);
    assert_eq!(diff.parameters[2].fields, ["name"]);
    // The feature's slot follows the rename: only its value changed.
    assert_eq!(
        texts(&diff.features, |f| &f.text),
        ["Extrude1 (F2): distance 20 mm -> 24 mm"]
    );
    let value = &diff.features[0].values[0];
    assert_eq!(
        (value.from_parameter.as_str(), value.to_parameter.as_str()),
        ("d3", "depth")
    );
    assert_eq!(
        diff.summary,
        "+1 parameter, -1 parameter, 3 parameters changed, 1 feature modified"
    );
}

#[test]
fn features_are_added_deleted_moved_and_renamed() {
    let mut doc = block(MockKernel::default());
    run(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude",
             "profiles": [{"sketch": "F3", "region": REGION}],
             "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    let diff = changed(
        &mut doc,
        json!([
            {"cmd": "reorder_feature", "uid": "F3", "index": 0},
            {"cmd": "delete_feature", "uid": "F4"},
            {"cmd": "rename_feature", "uid": "F2", "name": "Plate"},
            {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
             "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 3}},
        ]),
    );
    assert_eq!(
        texts(&diff.features, |f| &f.text),
        [
            "Sketch2 (F3): moved to the start",
            "Plate (F2): renamed from Extrude1",
            "Fillet1 (F5, fillet): added after Plate",
            "Extrude2 (F4, extrude): deleted",
        ]
    );
    let kinds: Vec<(Kind, Option<usize>, Option<usize>, bool)> = diff
        .features
        .iter()
        .map(|f| (f.kind, f.from_index, f.to_index, f.moved))
        .collect();
    assert_eq!(
        kinds,
        [
            (Kind::Moved, Some(2), Some(0), true),
            (Kind::Modified, Some(1), Some(2), false),
            (Kind::Added, None, Some(3), false),
            (Kind::Deleted, Some(3), None, false),
        ]
    );
    // The deleted extrusion's dimension and the fillet's new one have the
    // same name but are not the same parameter.
    assert_eq!(
        texts(&diff.parameters, |p| &p.text),
        [
            "d6 (Fillet1 radius) = 3 mm: added",
            "d6 (Extrude2 distance) = 5 mm: deleted",
        ]
    );
    assert_eq!(texts(&diff.bodies, |b| &b.text), ["Body2 (F4.b0): deleted"]);
    assert_eq!(
        diff.summary,
        "+1 parameter, -1 parameter, +1 feature, -1 feature, 1 feature modified, 1 feature \
         moved, -1 body"
    );
}

#[test]
fn definitions_differ_field_by_field() {
    let mut doc = block(MockKernel::default());
    let diff = changed(
        &mut doc,
        json!([
            {"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
             "profiles": [{"sketch": "F1", "region": REGION}],
             "extent": {"type": "distance", "distance": "d3"}, "operation": "new_body",
             "start": {"type": "offset", "offset": 2}}},
            {"cmd": "suppress_feature", "uid": "F2"},
        ]),
    );
    let feature = &diff.features[0];
    assert_eq!(
        texts(&feature.fields, |f| &f.text),
        ["suppressed", "start (none) -> offset (offset d4)"]
    );
    assert_eq!(
        feature.text,
        "Extrude1 (F2): suppressed; start (none) -> offset (offset d4)"
    );
    // A field of a tagged value that keeps its type differs by itself.
    let diff = changed(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": "d3"}, "operation": "new_body",
               "start": {"type": "offset", "offset": "d1"}}}),
    );
    let fields: Vec<(&str, &Value, &Value)> = diff.features[0]
        .fields
        .iter()
        .map(|f| (f.field.as_str(), &f.from, &f.to))
        .collect();
    assert_eq!(fields, [("start.offset", &json!("d4"), &json!("d1"))]);
    // Its value changed too, and the dimension that is no longer used went.
    assert_eq!(
        diff.features[0].text,
        "Extrude1 (F2): offset 2 mm -> 60 mm; start.offset d4 -> d1"
    );
    assert_eq!(diff.parameters[0].kind, Kind::Deleted);
}

#[test]
fn sketches_compare_entities_constraints_and_dimensions() {
    let mut doc = block(MockKernel::default());
    let diff = changed(
        &mut doc,
        json!([
            {"cmd": "set_parameter", "name": "d1", "value": 70},
            {"cmd": "sketch.add_line", "sketch": "F1", "start": [100, 0], "end": [120, 0]},
            {"cmd": "sketch.add_constraint", "sketch": "F1",
             "constraint": {"type": "horizontal", "line": "c9"}},
        ]),
    );
    let sketch = &diff.features[0];
    assert_eq!(diff.features.len(), 1, "{:?}", diff.features);
    assert_eq!(sketch.uid, FeatureUid(1));
    let change = sketch.sketch.as_ref().unwrap();
    assert_eq!(
        (
            change.entities.from,
            change.entities.to,
            &change.entities.added[..]
        ),
        (
            8,
            11,
            &["p10".to_owned(), "p11".to_owned(), "c9".to_owned()][..]
        )
    );
    assert_eq!((change.constraints.from, change.constraints.to), (4, 5));
    assert!(change.dimensions.is_empty());
    assert_eq!(change.moved, ["p6", "p7"]);
    assert_eq!(sketch.values[0].text, "k5 length 60 mm -> 70 mm");
    assert_eq!(
        sketch.text,
        "Sketch1 (F1): k5 length 60 mm -> 70 mm; entities 8 -> 11 (added p10, p11, c9), \
         constraints 4 -> 5 (added k7), 2 entities moved (p6, p7)"
    );
    // A dimension bound to another parameter changes itself.
    run(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "width", "value": 70}),
    );
    let diff = changed(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": "k5", "value": "width"}),
    );
    let change = diff.features[0].sketch.as_ref().unwrap();
    assert_eq!(change.dimensions.modified, ["k5"]);
    assert!(change.moved.is_empty());
}

#[test]
fn components_and_placements() {
    let mut doc = block(MockKernel::default());
    let (_, occurrence) = doc
        .create_component(
            Some("Plate"),
            Transform::translation([100.0, 0.0, 0.0]),
            false,
        )
        .unwrap();
    let before = doc.to_json();
    let component = doc.assembly().occurrence(occurrence).unwrap().component;
    doc.rename_component(component, "Bracket").unwrap();
    let turn = Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], FRAC_PI_2).unwrap();
    let placement = Transform::translation([100.0, 50.0, 0.0]).after(&turn);
    doc.set_occurrence_transform(occurrence, placement, false)
        .unwrap();
    doc.create_component(Some("Pin"), Transform::IDENTITY, false)
        .unwrap();
    let diff = diff_files(&before, &doc.to_json()).unwrap();
    assert_eq!(
        texts(&diff.components, |c| &c.text),
        ["Bracket (C1): renamed from Plate", "Pin (C2): added"]
    );
    assert_eq!(
        texts(&diff.occurrences, |o| &o.text),
        [
            "Bracket:1 (O1): placement moved by [0, 50, 0] mm and turned 90 deg",
            "Pin:1 (O2): added"
        ]
    );
    assert_eq!(diff.occurrences[0].fields[0].field, "transform");
    assert_eq!(
        diff.summary,
        "+1 component, 1 component changed, +1 occurrence, 1 occurrence changed"
    );
}

#[test]
fn the_document_bodies_groups_and_views() {
    let mut doc = block(MockKernel::default());
    let diff = changed(
        &mut doc,
        json!([
            {"cmd": "rename_body", "uid": "F2.b0", "name": "Plate"},
            {"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"},
            {"cmd": "set_marker", "position": 1},
            {"cmd": "set_units", "length": "in"},
            {"cmd": "add_named_view", "name": "Front", "eye": [0, -100, 0], "target": [0, 0, 0],
             "up": [0, 0, 1], "height": 50},
        ]),
    );
    assert_eq!(
        texts(&diff.document, |c| &c.text),
        [
            "units.length mm -> in",
            "marker at the end -> after Sketch1"
        ]
    );
    assert_eq!(
        diff.document[1].to,
        json!({"position": 1, "after": "F1", "end": false})
    );
    assert_eq!(
        texts(&diff.bodies, |b| &b.text),
        ["Plate (F2.b0): material (none) -> aluminum; renamed from Body1"]
    );
    assert_eq!(texts(&diff.views, |v| &v.text), ["Front: added"]);
    // The display state is not part of the design.
    let diff = changed(
        &mut doc,
        json!({"cmd": "set_origin_visible", "visible": true}),
    );
    assert!(diff.identical, "{diff:?}");
}

#[test]
fn base_features_compare_their_data_by_sha256() {
    let brep = |data: &str| serde_json::to_value(crate::features::Brep::new(data.into())).unwrap();
    let mut doc = Document::new(MockKernel::default());
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "base",
               "bodies": [{"name": "Bolt", "brep": brep("6 bolt")}]}}),
    );
    // A project file refers to the data that a single file holds.
    let store = crate::MemoryStore::default();
    let referring = doc.to_project_json(&store).unwrap();
    assert!(referring.contains("\"sha256\""));
    assert!(diff_files(&doc.to_json(), &referring).unwrap().identical);
    let diff = changed(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F1", "def": {"type": "base",
               "bodies": [{"name": "Bolt", "brep": brep("8 longer")}]}}),
    );
    let field = &diff.features[0].fields[0];
    assert_eq!(field.field, "bodies[0].brep");
    assert_eq!(
        field.text,
        "bodies[0].brep B-rep data changed (6 -> 8 bytes)"
    );
    assert_eq!(
        field.to,
        json!({"sha256": crate::Sha256::of(b"8 longer").to_string(), "size": 8})
    );
}

/// The mock kernel with volumes and areas from the bounding boxes.
#[derive(Default)]
struct Measured(MockKernel);

impl Kernel for Measured {
    type Shape = MockShape;

    fn extrude(&self, spec: &ExtrudeSpec<'_>) -> Result<MockShape, KernelError> {
        self.0.extrude(spec)
    }

    fn extrude_feature(
        &self,
        spec: &ExtrudeFeatureSpec<'_, MockShape>,
    ) -> Result<MockShape, KernelError> {
        self.0.extrude_feature(spec)
    }

    fn solids(&self, shape: &MockShape) -> Result<Vec<MockShape>, KernelError> {
        self.0.solids(shape)
    }

    fn boolean(
        &self,
        op: BooleanOp,
        targets: &[&MockShape],
        tool: &MockShape,
    ) -> Result<BooleanOutput<MockShape>, KernelError> {
        self.0.boolean(op, targets, tool)
    }

    fn fillet(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        sets: &[FilletSet<'_>],
        rolling: bool,
    ) -> Result<MockShape, KernelError> {
        self.0.fillet(feature, body, sets, rolling)
    }

    fn count_edges(&self, shape: &MockShape, edge: &EdgeName) -> usize {
        self.0.count_edges(shape, edge)
    }

    fn count_faces(&self, shape: &MockShape, face: &FaceName) -> Result<usize, KernelError> {
        self.0.count_faces(shape, face)
    }

    fn body_kind(&self, shape: &MockShape) -> Result<BodyKind, KernelError> {
        self.0.body_kind(shape)
    }

    fn mass_properties(&self, shape: &MockShape) -> Result<MassProperties, KernelError> {
        let bounds = shape.bounds.ok_or(KernelError::failed("no bounds"))?;
        let [x, y, z] = [0, 1, 2].map(|i| bounds.max[i] - bounds.min[i]);
        Ok(MassProperties {
            volume: x * y * z,
            area: 2.0 * (x * y + y * z + z * x),
            center: [0.0; 3],
        })
    }
}

#[test]
fn geometry_compares_volumes_and_areas() {
    let mut from = block(Measured::default());
    let mut to = block(Measured::default());
    run(
        &mut to,
        json!([
            {"cmd": "set_parameter", "name": "d3", "value": 30},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude",
             "profiles": [{"sketch": "F3", "region": REGION}],
             "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    from.recompute();
    to.recompute();
    let mut diff = diff_documents(&from, &to);
    diff.add_geometry(&from, &to);
    let geometry = diff.geometry.as_ref().unwrap();
    assert_eq!(
        texts(&geometry.bodies, |b| &b.text),
        [
            "Body1 (F2.b0): volume 48000 -> 72000 mm^3 (+24000, +50%); area 8800 -> 10800 mm^2 \
             (+2000, +22.73%)",
            "Body2 (F4.b0): added, volume 500 mm^3, area 400 mm^2",
        ]
    );
    assert_eq!(
        (geometry.volume.from, geometry.volume.to),
        (Some(48000.0), Some(72500.0))
    );
    assert!(
        diff.summary.ends_with(", volume 48000 mm^3 -> 72500 mm^3"),
        "{}",
        diff.summary
    );
    assert!(
        diff.to_text().ends_with(
            "Geometry:\n  Body1 (F2.b0): volume 48000 -> 72000 mm^3 (+24000, +50%); area 8800 -> \
             10800 mm^2 (+2000, +22.73%)\n  Body2 (F4.b0): added, volume 500 mm^3, area 400 mm^2\n  \
             Total: volume 48000 -> 72500 mm^3 (+24500, +51.04%); area 8800 -> 11200 mm^2 \
             (+2400, +27.27%)\n"
        ),
        "{}",
        diff.to_text()
    );
    // The same geometry: nothing differs.
    let mut same = diff_documents(&from, &from);
    same.add_geometry(&from, &from);
    assert!(same.identical);
    // A kernel without mass properties: the bodies are not measured.
    let mut plain = block(MockKernel::default());
    plain.recompute();
    let mut unmeasured = diff_documents(&plain, &plain);
    unmeasured.add_geometry(&plain, &plain);
    assert!(!unmeasured.identical);
    assert_eq!(
        unmeasured.geometry.unwrap().errors,
        [
            "from: Body1 (F2.b0): the geometry kernel does not support mass properties",
            "to: Body1 (F2.b0): the geometry kernel does not support mass properties"
        ]
    );
}

#[test]
fn moves_are_the_features_outside_a_longest_ordered_run() {
    assert_eq!(longest_increasing(&[1, 2, 0, 3]), [0, 1, 3]);
    assert_eq!(longest_increasing(&[3, 2, 1, 0]).len(), 1);
    assert_eq!(longest_increasing(&[]), Vec::<usize>::new());
    assert_eq!(slot_label("dimensions[k5].length"), "k5 length");
    assert_eq!(slot_label("extent.distance"), "distance");
    assert_eq!(slot_label("radius"), "radius");
    // Items only in the first list come before the next one both have.
    assert_eq!(
        merged_order(&[Some(0), None, Some(1)], 3),
        [
            (Some(0), Some(0)),
            (Some(1), None),
            (Some(2), Some(1)),
            (None, Some(2))
        ]
    );
}
