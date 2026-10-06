// SPDX-License-Identifier: MIT
//! Importer tests on synthetic dumps with the model's mock kernel.

use mitcad_f3d::design::ir::Dump;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureDef};
use serde_json::{Value, json};

use super::*;

fn dump(value: Value) -> Dump {
    Dump::from_json(&value.to_string()).expect("a dump")
}

fn param(name: &str, expression: &str, value: f64, unit: &str) -> Value {
    json!({"kind": "parameter", "name": name, "expression": expression, "value": value, "unit": unit})
}

/// A 40 x 20 mm rectangle on XY from (0, 0) (cm in the IR), its width
/// dimensioned with d1.
fn rectangle_sketch(index: i64) -> Value {
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b});
    json!({
        "index": index, "name": format!("Sketch{}", index + 1), "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 0.0, 0.0), p("p1", 4.0, 0.0), p("p2", 4.0, 2.0), p("p3", 0.0, 2.0)],
            "curves": [line("c0", "p0", "p1"), line("c1", "p1", "p2"),
                       line("c2", "p2", "p3"), line("c3", "p3", "p0")],
            "constraints": [
                {"id": "k0", "type": "HorizontalConstraint", "refs": {"entities": ["c0"]}},
                {"id": "k1", "type": "VerticalConstraint", "refs": {"entities": ["c1"]}},
                {"id": "k2", "type": "PolygonConstraint", "refs": {"entities": ["c0", "c1"]}}
            ],
            "dimensions": [
                {"id": "d0", "type": "SketchLinearDimension",
                 "parameter": param("d1", "40 mm", 4.0, "mm"),
                 "props": {"orientation": "HorizontalDimensionOrientation"},
                 "refs": {"entities": ["p0", "p1"]}, "textPosition": [2.0, -0.5, 0.0]}
            ]
        }
    })
}

fn extrude(index: i64, sketch: i64, operation: &str, distance: &str) -> Value {
    json!({
        "index": index, "name": format!("Extrude{index}"), "objectType": "ExtrudeFeature",
        "detail": {
            "operation": operation,
            "profile": [{"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": sketch}],
            "extentType": "OneSideFeatureExtentType",
            "extentOne": {"_type": "DistanceExtentDefinition",
                          "distance": param(distance, "10 mm", 1.0, "mm")},
            "taperAngleOne": param("d3", "0.0 deg", 0.0, "deg")
        },
        "_f3d": {"extrude": {"operation_code": 4, "direction": 1.0}}
    })
}

fn block_dump() -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "block.f3d"},
        "parameters": {
            "user": [{"name": "width", "expression": "height * 2", "value": 4.0, "unit": "mm"},
                     {"name": "height", "expression": "20 mm", "value": 2.0, "unit": "mm"}],
            "model": [
                {"name": "d1", "expression": "width", "value": 4.0, "unit": "mm",
                 "createdBy": {"kind": "sketch_dimension", "sketch": "Sketch1",
                               "sketch_timeline_index": 0, "id": "d0"}},
                {"name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm",
                 "createdBy": {"kind": "feature", "objectType": "ExtrudeFeature",
                               "name": "Extrude1", "timeline_index": 1}},
                {"name": "d3", "expression": "0.0 deg", "value": 0.0, "unit": "deg",
                 "createdBy": {"kind": "feature", "objectType": "ExtrudeFeature",
                               "name": "Extrude1", "timeline_index": 1}}
            ]
        },
        "timeline": {"items": [rectangle_sketch(0), extrude(1, 0, "NewBodyFeatureOperation", "d2")]}
    }))
}

fn import(doc: &mut Document<MockKernel>, dump: &Dump) -> DesignReport {
    import_design(doc, dump, &mut NoGeometry, &Options::default())
}

#[test]
fn parameters_keep_names_and_expressions_in_dependency_order() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_dump());
    assert_eq!(report.parameters.imported, 5, "{:?}", report.parameters);
    assert!(report.parameters.literal.is_empty());
    assert!(
        report.parameters.mismatched.is_empty(),
        "{:?}",
        report.parameters
    );
    let p = |name: &str| {
        let params = doc.parameters();
        let id = params.find(name).expect(name);
        let p = params.get(id).unwrap();
        (p.expression().to_owned(), p.value())
    };
    assert_eq!(p("width"), ("height * 2".to_owned(), 40.0));
    assert_eq!(p("d1"), ("width".to_owned(), 40.0));
    // Model parameters belong to the items that made them.
    let params = doc.parameters();
    let owner = |name: &str| params.owner(params.find(name).unwrap());
    assert_eq!(owner("d1"), Some(FeatureUid(1)));
    assert_eq!(owner("d2"), Some(FeatureUid(2)));
    assert_eq!(owner("width"), None);
}

#[test]
fn sketches_and_extrudes_replay_with_parameters() {
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &block_dump());
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    // The polygon constraint is left out, so the sketch is partial.
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert!(
        report.items[0]
            .note
            .as_deref()
            .unwrap()
            .contains("PolygonConstraint")
    );
    let sketch = doc.feature(FeatureUid(1)).unwrap();
    assert_eq!(sketch.name, "Sketch1");
    let FeatureDef::Sketch(s) = &sketch.def else {
        panic!("a sketch");
    };
    assert_eq!(s.entities.len(), 8);
    assert_eq!(s.constraints.len(), 2);
    assert_eq!(s.dimensions.len(), 1);
    assert!(s.frame.is_none());
    let extrude = doc.feature(FeatureUid(2)).unwrap();
    assert_eq!(extrude.name, "Extrude1");
    let def = doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap();
    let def: Value = serde_json::from_str(&def).unwrap();
    assert_eq!(def["def"]["extent"]["distance"], "d2");
    assert_eq!(def["def"]["operation"], "new_body");
    assert_eq!(doc.bodies().len(), 1);
    // A parameter edit flows through the imported expressions.
    doc.set_parameter_expression("height", "30 mm", None)
        .unwrap();
    assert_eq!(doc.status(FeatureUid(1)).map(|s| s.is_ok()), Some(true));
}

/// The mock kernel with mass properties: a solid's "volume" and "area"
/// come from its history and face names with feature numbers left out,
/// so the same operations give the same values in another document, and
/// a body read back from B-rep data keeps the values of the one written.
#[derive(Default)]
struct TestKernel {
    mock: MockKernel,
    /// Mock B-rep history → the written shape's fingerprint.
    written: std::cell::RefCell<std::collections::HashMap<String, String>>,
    /// Whether edges have middles (see edge_point), so that the edge
    /// fingerprints of external dumps resolve.
    edge_points: bool,
}

use mitcad_model::testing::MockShape;
use mitcad_model::{
    BodyKind, BooleanOp, BooleanOutput, BoundingBox, Chamfer, EdgeName, ExtrudeFeatureSpec,
    ExtrudeSpec, FilletSet, KernelError, MassProperties, Plane,
};

/// Feature numbers left out: `F12` becomes `F`.
fn without_uids(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == 'F' {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
        }
    }
    out
}

impl TestKernel {
    fn fingerprint(&self, shape: &MockShape) -> String {
        if let Some(inner) = shape.history.strip_prefix("import(")
            && let Some((_, rest)) = inner.split_once(',')
        {
            let original = &rest[..rest.len() - 1];
            if let Some(f) = self.written.borrow().get(original) {
                return f.clone();
            }
        }
        let mut faces: Vec<String> = shape
            .faces
            .iter()
            .map(|f| without_uids(&f.to_string()))
            .collect();
        faces.sort();
        format!("{}|{}", without_uids(&shape.history), faces.join(","))
    }
}

fn hash(text: &str, salt: u64) -> f64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (text, salt).hash(&mut h);
    100.0 + (h.finish() % 100_000) as f64 / 10.0
}

impl Kernel for TestKernel {
    type Shape = MockShape;

    fn extrude(&self, spec: &ExtrudeSpec<'_>) -> Result<MockShape, KernelError> {
        self.mock.extrude(spec)
    }

    fn extrude_feature(
        &self,
        spec: &ExtrudeFeatureSpec<'_, MockShape>,
    ) -> Result<MockShape, KernelError> {
        self.mock.extrude_feature(spec)
    }

    fn solids(&self, shape: &MockShape) -> Result<Vec<MockShape>, KernelError> {
        self.mock.solids(shape)
    }

    fn boolean(
        &self,
        op: BooleanOp,
        targets: &[&MockShape],
        tool: &MockShape,
    ) -> Result<BooleanOutput<MockShape>, KernelError> {
        self.mock.boolean(op, targets, tool)
    }

    fn fillet(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        sets: &[FilletSet<'_>],
        rolling: bool,
    ) -> Result<MockShape, KernelError> {
        self.mock.fillet(feature, body, sets, rolling)
    }

    fn chamfer(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        sets: &[Chamfer<'_>],
        corner: mitcad_model::features::ChamferCorner,
    ) -> Result<MockShape, KernelError> {
        self.mock.chamfer(feature, body, sets, corner)
    }

    fn count_edges(&self, shape: &MockShape, edge: &EdgeName) -> usize {
        self.mock.count_edges(shape, edge)
    }

    fn count_faces(
        &self,
        shape: &MockShape,
        face: &mitcad_model::FaceName,
    ) -> Result<usize, KernelError> {
        self.mock.count_faces(shape, face)
    }

    fn bounding_box(&self, shape: &MockShape) -> Result<Option<BoundingBox>, KernelError> {
        self.mock.bounding_box(shape)
    }

    fn face_plane(
        &self,
        shape: &MockShape,
        face: &mitcad_model::FaceName,
    ) -> Result<Plane, KernelError> {
        self.mock.face_plane(shape, face)
    }

    fn face_geometry(
        &self,
        shape: &MockShape,
        face: &mitcad_model::FaceName,
    ) -> Result<mitcad_model::datum::SurfaceGeometry, KernelError> {
        self.mock.face_geometry(shape, face)
    }

    fn import_brep(
        &self,
        feature: FeatureUid,
        data: &[u8],
        first: u32,
    ) -> Result<(MockShape, u32), KernelError> {
        self.mock.import_brep(feature, data, first)
    }

    fn brep_data(&self, shape: &MockShape) -> Result<Vec<u8>, KernelError> {
        self.written
            .borrow_mut()
            .insert(shape.history.clone(), self.fingerprint(shape));
        self.mock.brep_data(shape)
    }

    fn compound(&self, shapes: &[MockShape]) -> Result<MockShape, KernelError> {
        self.mock.compound(shapes)
    }

    fn transform_shape(
        &self,
        shape: &MockShape,
        transform: &mitcad_model::Transform,
        instance: Option<mitcad_model::Instance>,
    ) -> Result<MockShape, KernelError> {
        self.mock.transform_shape(shape, transform, instance)
    }

    fn unite(&self, shapes: &[&MockShape]) -> Result<MockShape, KernelError> {
        self.mock.unite(shapes)
    }

    fn body_kind(&self, shape: &MockShape) -> Result<BodyKind, KernelError> {
        self.mock.body_kind(shape)
    }

    fn mass_properties(&self, shape: &MockShape) -> Result<MassProperties, KernelError> {
        let f = self.fingerprint(shape);
        Ok(MassProperties {
            volume: hash(&f, 1),
            area: hash(&f, 2),
            center: [0.0; 3],
        })
    }

    // Replace face (P5): the faces go, a replace(<first>) face comes.
    fn replace_faces(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        faces: &[mitcad_model::FaceName],
        _target: &mitcad_model::ToolInput<'_, MockShape>,
        tangent_chain: bool,
    ) -> Result<MockShape, KernelError> {
        let names: Vec<String> = faces.iter().map(|f| without_uids(&f.to_string())).collect();
        let mut shape = body.clone();
        shape.history = format!(
            "replace({},{}{})",
            body.history,
            names.join(";"),
            if tangent_chain { ",chain" } else { "" }
        );
        shape.faces.retain(|f| !faces.contains(f));
        shape
            .edges
            .retain(|(a, b)| !faces.contains(a) && !faces.contains(b));
        shape
            .faces
            .push(format!("{feature}:replace({})", faces[0]).parse().unwrap());
        Ok(shape)
    }

    fn loft(&self, spec: &mitcad_model::LoftSpec<'_, MockShape>) -> Result<MockShape, KernelError> {
        self.mock.loft(spec)
    }

    // With edge_points, a point per edge between its faces' points (see
    // edge_point), 1 mm long, so that fingerprints of edges resolve.
    fn edge_middles(
        &self,
        shape: &MockShape,
    ) -> Result<Vec<mitcad_model::EdgeMiddle>, KernelError> {
        if !self.edge_points {
            return Err(KernelError::Unsupported("edge middles"));
        }
        Ok(shape
            .edges
            .iter()
            .map(|(a, b)| mitcad_model::EdgeMiddle {
                name: EdgeName::new(a.clone(), b.clone()).to_string(),
                point: edge_point(a, b),
                tangent: [1.0, 0.0, 0.0],
                length: 1.0,
            })
            .collect())
    }

    // A point per face from its name, on the faces of the shapes that have it.
    fn face_points(&self, shape: &MockShape) -> Result<Vec<mitcad_model::FacePoints>, KernelError> {
        Ok(shape
            .faces
            .iter()
            .map(|f| mitcad_model::FacePoints {
                name: f.to_string(),
                points: vec![face_point(f)],
            })
            .collect())
    }

    fn boundary_distances(
        &self,
        shape: &MockShape,
        points: &[[f64; 3]],
    ) -> Result<Vec<f64>, KernelError> {
        Ok(points
            .iter()
            .map(|p| {
                if shape.faces.iter().any(|f| face_point(f) == *p) {
                    0.0
                } else {
                    1.0
                }
            })
            .collect())
    }
}

/// The test kernel's point inside a face (the same in every document).
fn face_point(face: &mitcad_model::FaceName) -> [f64; 3] {
    let name = without_uids(&face.to_string());
    [hash(&name, 3), hash(&name, 4), hash(&name, 5)]
}

/// The TestKernel's middle of the edge between two faces (mm).
fn edge_point(a: &mitcad_model::FaceName, b: &mitcad_model::FaceName) -> [f64; 3] {
    let (p, q) = (face_point(a), face_point(b));
    [
        0.5 * (p[0] + q[0]),
        0.5 * (p[1] + q[1]),
        0.5 * (p[2] + q[2]),
    ]
}

/// History states and stored bodies given by the test.
#[derive(Default)]
struct TestGeometry {
    states: Vec<Vec<MockShape>>,
    /// The state each item made, by item index.
    items: HashMap<i64, usize>,
    /// States that cannot be built.
    broken: Vec<usize>,
    /// The component (object id) of every body and of the items' states.
    component: Option<u64>,
}

impl StoredGeometry<MockShape> for TestGeometry {
    fn state_count(&mut self) -> usize {
        self.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.items.get(&index).copied()
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        if self.broken.contains(&index) {
            return Err(format!("state {index} cannot be built"));
        }
        Ok(self.states[index]
            .iter()
            .enumerate()
            .map(|(i, shape)| StoredBody {
                shape: shape.clone(),
                name: None,
                source: format!("state {index} body {i}"),
                component: self.component,
                id: None,
            })
            .collect())
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        let last = self.states.len() - 1;
        self.state(last)
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.items
            .contains_key(&index)
            .then(|| self.component.into_iter().collect())
    }
}

fn shapes(doc: &Document<TestKernel>) -> Vec<MockShape> {
    doc.bodies().iter().map(|b| b.shape.clone()).collect()
}

/// Two rectangles side by side: 40 x 20 and 10 x 20 mm.
fn two_rectangles(index: i64) -> Value {
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b});
    json!({
        "index": index, "name": format!("Sketch{}", index + 1), "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 0.0, 0.0), p("p1", 4.0, 0.0), p("p2", 4.0, 2.0), p("p3", 0.0, 2.0),
                       p("p4", 5.0, 0.0), p("p5", 6.0, 0.0), p("p6", 6.0, 2.0), p("p7", 5.0, 2.0)],
            "curves": [line("c0", "p0", "p1"), line("c1", "p1", "p2"),
                       line("c2", "p2", "p3"), line("c3", "p3", "p0"),
                       line("c4", "p4", "p5"), line("c5", "p5", "p6"),
                       line("c6", "p6", "p7"), line("c7", "p7", "p4")]
        }
    })
}

fn timeline_dump(items: Vec<Value>) -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "test.f3d"},
        "parameters": {"model": [
            {"name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm"},
            {"name": "d3", "expression": "0.0 deg", "value": 0.0, "unit": "deg"},
            {"name": "d5", "expression": "2 mm", "value": 0.2, "unit": "mm"}
        ]},
        "timeline": {"items": items}
    }))
}

fn add(doc: &mut Document<TestKernel>, def: Value) {
    let def: FeatureDef<mitcad_model::ValueInput> = serde_json::from_value(def).unwrap();
    let added = doc.add_feature(&def, None).unwrap();
    assert_eq!(doc.status(added.uid).and_then(|s| s.error()), None);
}

#[test]
fn the_history_chooses_the_profile() {
    // The file's design extruded the small rectangle; the decoder only
    // says "one profile of Sketch1", and the larger one is the first guess.
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let full = timeline_dump(items.clone());
    // What the file has: the same sketch with the small rectangle extruded.
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": small}],
                               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        ..TestGeometry::default()
    };

    // Without the history the first guess is used, the larger rectangle.
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &full, &mut NoGeometry, &Options::default());
    assert_eq!(report.items[1].outcome, Outcome::Parametric);
    assert_eq!(report.items[1].verified, None);
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_ne!(def["def"]["profiles"][0]["region"], small);

    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &full, &mut geometry, &Options::default());
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].verified, Some(true));
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["profiles"][0]["region"], small);
    assert_eq!(def["def"]["extent"]["distance"], "d2");
    assert_eq!(report.history.matched, 1);
    assert!(report.history.reached_end);
    assert_eq!(report.bodies.len(), 1);
    assert_eq!(report.bodies[0].volume_difference, Some(0.0));
}

/// A fillet whose edges the decoder does not give, and an extrusion after
/// it.
fn fillet_item(index: i64) -> Value {
    json!({"index": index, "name": "Fillet1", "objectType": "FilletFeature",
           "detail": {"edgeSets": [{"radius": param("d5", "2 mm", 0.2, "mm")}]}})
}

#[test]
fn a_feature_that_cannot_be_replayed_takes_the_files_bodies_and_the_timeline_goes_on() {
    fallback_and_continue(false, false);
}

#[test]
fn with_the_items_states_known_a_failed_item_takes_its_own_state() {
    fallback_and_continue(true, false);
}

#[test]
fn in_a_component_the_fallback_goes_into_the_component() {
    fallback_and_continue(true, true);
}

/// A fillet the decoder gives no edges of between an extrusion and a join;
/// `known`: the history names each item's state (`result_no`);
/// `component`: the bodies are those of component Plate (object 10), placed
/// 5 cm along X.
fn fallback_and_continue(known: bool, component: bool) {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet_item(2),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
    ];
    // The file's states: the block, the block rounded (some fillet), then the
    // second extrusion joined to it.
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let edge = "E{F2:side(c5[c8,c6])|F2:side(c6[c5,c7])}";
    add(
        &mut reference,
        json!({"type": "fillet", "body": "F2.b0", "edges": [edge], "radius": 2}),
    );
    let rounded = shapes(&reference);
    // The join as Mitcad makes it on the stored rounded body.
    reference.undo();
    reference
        .add_base_feature(BaseInput {
            replaces: vec![mitcad_model::BodyUid::new(FeatureUid(2), 0)],
            ..BaseInput::new(vec![ImportBody {
                name: None,
                color: None,
                shape: rounded[0].clone(),
            }])
        })
        .unwrap();
    let region = "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
                               "extent": {"type": "distance", "distance": "d2"}, "flip": false,
                               "operation": "join"}),
    );
    let joined = shapes(&reference);
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), block, rounded, joined],
        items: if known {
            HashMap::from([(1, 1), (2, 2), (3, 3)])
        } else {
            HashMap::new()
        },
        component: component.then_some(10),
        ..TestGeometry::default()
    };
    let mut dump = timeline_dump(items);
    if component {
        let mut value = serde_json::to_value(&dump).unwrap();
        value["components"] = json!([{"name": "Plate", "_f3d": {"object_id": 10}}]);
        value["occurrences"] = json!([{"component": "Plate", "_f3d": {"component_object": 10},
            "transform": [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                          [0.0, 0.0, 0.0, 1.0]]}]);
        dump = Dump::from_json(&value.to_string()).unwrap();
    }

    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &dump, &mut geometry, &Options::default());
    if component {
        // Every item and the fallback in Plate; its body placed with it.
        let plate = doc.assembly().components[0].uid;
        assert!(
            doc.features().all(|f| f.component == plate),
            "{}",
            report.text()
        );
        assert_eq!(doc.body_component(doc.bodies()[0].uid), Some(plate));
        let instances = doc.instances();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].transform.translation, [50.0, 0.0, 0.0]);
    }
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Partial,
            Outcome::Parametric,
            Outcome::Fallback,
            Outcome::Parametric
        ],
        "{}",
        report.text()
    );
    // The fallback replaced the block, keeping its body id.
    let fallback = doc.feature(FeatureUid(3)).unwrap();
    assert_eq!(fallback.name, "Fillet1");
    let FeatureDef::Base(base) = &fallback.def else {
        panic!("a base feature");
    };
    assert_eq!(
        base.replaces,
        [mitcad_model::BodyUid::new(FeatureUid(2), 0)]
    );
    assert_eq!(report.items[3].verified, Some(true));
    assert!(report.history.reached_end, "{}", report.text());
    assert_eq!(doc.bodies().len(), 1);
}

/// The block's history: nothing, the block, the block joined again.
fn block_history(broken: Vec<usize>) -> (Vec<Value>, TestGeometry) {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        extrude(2, 0, "JoinFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let region = "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}";
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": "d2"}, "flip": false,
               "operation": "join"}),
    );
    let geometry = TestGeometry {
        states: vec![Vec::new(), block, shapes(&reference)],
        items: HashMap::from([(1, 1), (2, 2)]),
        broken,
        component: None,
    };
    (items, geometry)
}

#[test]
fn an_item_whose_state_cannot_be_rebuilt_is_taken_unchecked() {
    let (items, mut geometry) = block_history(vec![1]);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].verified, None);
    assert!(
        report.items[1]
            .note
            .as_deref()
            .unwrap()
            .contains("not checked"),
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    assert_eq!(report.history.unbuilt, 1);
}

#[test]
fn after_the_time_limit_items_take_their_states() {
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let options = Options {
        time_limit: Some(0.0),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    assert!(
        report.items[1]
            .note
            .as_deref()
            .unwrap()
            .contains("time limit")
    );
    assert_eq!(doc.bodies().len(), 1);
}

#[test]
fn items_the_kernel_hung_on_take_their_states_and_progress_is_shared() {
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let progress = std::sync::Arc::new(crate::Progress::new());
    let options = Options {
        hung_items: vec![1],
        progress: Some(progress.clone()),
        ..Options::default()
    };
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    // (The mock kernel's join on the base feature's body is not the state.)
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains("did not finish"), "{}", report.text());
    assert!(!note(2).contains("did not finish"), "{}", report.text());
    use std::sync::atomic::Ordering;
    assert_eq!(
        progress.item.load(Ordering::Relaxed),
        crate::Progress::FINISHING
    );
    assert!(progress.step.load(Ordering::Relaxed) >= 4);
}

/// The block's import (sketch, new body, join) with these options: the
/// report, the document and the outcomes.
fn import_block(options: &Options) -> (DesignReport, Document<TestKernel>, Vec<Outcome>) {
    let (items, mut geometry) = block_history(Vec::new());
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, options);
    let outcomes = report.items.iter().map(|i| i.outcome).collect();
    (report, doc, outcomes)
}

#[test]
fn after_a_stop_the_features_take_their_states_and_the_report_says_where() {
    let (report, _, outcomes) = import_block(&Options::default());
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.stopped, None);

    // Stopped before it started: the sketch still comes in, the features
    // take their history states.
    let stop = Arc::new(RecomputeMonitor::new());
    stop.cancel();
    let progress = Arc::new(crate::Progress::new());
    let (report, doc, outcomes) = import_block(&Options {
        stop: Some(stop),
        progress: Some(progress.clone()),
        ..Options::default()
    });
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(STOPPED), "{}", report.text());
    assert!(note(2).contains(STOPPED), "{}", report.text());
    let name = report.items[1].name.clone();
    assert_eq!(
        report.stopped,
        Some(StopReport {
            item: Some(1),
            name: Some(name.clone())
        })
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains(&format!("stopped at {name}"))),
        "{}",
        report.text()
    );
    assert_eq!(progress.stopped_at(), Some(1));
    assert_eq!(doc.bodies().len(), 1);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["stopped"]["item"], 1, "{json}");
}

#[test]
fn a_stop_while_a_feature_is_tried_keeps_the_items_before_it() {
    // Cancelled while the first definition is evaluated (made to take 20 s
    // longer): the stop's monitor is the document's while definitions are
    // tried, and cuts the evaluation short.
    let stop = Arc::new(RecomputeMonitor::new());
    stop.set_test_delay(20_000);
    let canceller = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            stop.cancel();
        })
    };
    let started = std::time::Instant::now();
    let (report, doc, outcomes) = import_block(&Options {
        stop: Some(stop),
        ..Options::default()
    });
    canceller.join().unwrap();
    assert!(
        started.elapsed().as_secs_f64() < 10.0,
        "the stop did not cut the evaluation short"
    );
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    assert_eq!(report.stopped.as_ref().and_then(|s| s.item), Some(1));
    assert!(
        doc.monitor().is_none(),
        "the document's monitor is left as it was"
    );
    assert_eq!(doc.bodies().len(), 1);

    // A try run again after a hang stops where the stopped one did: the
    // items before it are replayed.
    let (report, _, outcomes) = import_block(&Options {
        stop_at: Some(2),
        ..Options::default()
    });
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Parametric, Outcome::Fallback],
        "{}",
        report.text()
    );
    assert_eq!(report.stopped.as_ref().and_then(|s| s.item), Some(2));
}

#[test]
fn bodies_an_unreplayed_item_added_in_the_same_step_come_in_as_its_fallback() {
    // The file's history has the sweep's body and the extrusion in one state.
    let items = vec![
        two_rectangles(0),
        json!({"index": 1, "name": "Sweep1", "objectType": "SweepFeature"}),
        extrude(2, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let large = "r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}";
    let small = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";
    for (region, distance) in [(large, 5), (small, 10)] {
        add(
            &mut reference,
            json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
                   "extent": {"type": "distance", "distance": distance}, "operation": "new_body"}),
        );
    }
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes[1..],
        [Outcome::Fallback, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    assert!(report.history.reached_end, "{}", report.text());
    // The extrusion, then the sweep's body as a base feature named after it.
    assert_eq!(doc.bodies().len(), 2);
    let base = doc.feature(FeatureUid(3)).unwrap();
    assert_eq!(base.name, "Sweep1");
    assert!(matches!(base.def, FeatureDef::Base(_)));
}

#[test]
fn without_a_history_a_failure_takes_the_stored_bodies() {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        fillet_item(2),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
    ];
    struct Stored(Vec<MockShape>);
    impl StoredGeometry<MockShape> for Stored {
        fn state_count(&mut self) -> usize {
            0
        }
        fn state(&mut self, _: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
            Err("no history".to_owned())
        }
        fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
            Ok(self
                .0
                .iter()
                .map(|s| StoredBody {
                    shape: s.clone(),
                    name: Some("Block".to_owned()),
                    source: "stored".to_owned(),
                    component: None,
                    id: None,
                })
                .collect())
        }
    }
    let stored = MockShape::imported("stored block", 9);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut Stored(vec![stored]),
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Partial,
            Outcome::Parametric,
            Outcome::Fallback,
            Outcome::Skipped
        ],
        "{}",
        report.text()
    );
    assert_eq!(report.bodies.len(), 1);
    assert_eq!(report.bodies[0].volume_difference, Some(0.0));
}

#[test]
fn an_external_dump_replays_with_profiles_matched_by_area_and_centroid() {
    // An external dump (SCHEMA.md, made by a test producer):
    // a 20 x 10 mm rectangle with a construction circle, extruded
    // 10 mm, and a construction plane.
    let text = include_str!("../../f3d/tests/data/external_dump_box.json");
    let dump = Dump::from_json(text).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
    let sketch = &report.items[0];
    assert_eq!(sketch.outcome, Outcome::Parametric, "{}", report.text());
    let extrude = &report.items[1];
    assert_eq!(extrude.object_type, "ExtrudeFeature");
    assert_eq!(extrude.outcome, Outcome::Parametric, "{}", report.text());
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    // The rectangle's region, not the circle's (construction).
    assert_eq!(def["def"]["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(def["def"]["extent"]["distance"], "d2");
    let FeatureDef::Sketch(s) = &doc.feature(FeatureUid(1)).unwrap().def else {
        panic!("a sketch");
    };
    assert!(s.entities.iter().any(|e| e.construction));
    assert_eq!(doc.bodies().len(), 1);
}

/// The body fingerprint of an external dump for a replayed body (its volume in
/// cm³, as the test kernel measures it).
fn body_fingerprint(doc: &Document<TestKernel>, uid: &str) -> Value {
    let shape = doc.body_shape(uid.parse().unwrap()).unwrap();
    let volume = doc.kernel().mass_properties(shape).unwrap().volume / 1000.0;
    json!({"kind": "body", "objectType": "BRepBody", "name": doc.body_name(uid.parse().unwrap()),
           "volume": volume})
}

#[test]
fn add_in_body_operations_map_to_mitcad_features() {
    // Two blocks, then a combine, a circular pattern of the first extrude
    // and a mirror of the remaining body, as external dumps give them.
    let first = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        extrude(2, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(first.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let bodies: Vec<String> = reference
        .bodies()
        .iter()
        .map(|b| b.uid.to_string())
        .collect();
    assert_eq!(bodies.len(), 2, "two extrudes of the same region set");
    let target = body_fingerprint(&reference, &bodies[0]);
    let tool = body_fingerprint(&reference, &bodies[1]);
    let mut items = first;
    items.push(
        json!({"index": 3, "name": "Combine1", "objectType": "CombineFeature",
        "detail": {"targetBody": target, "toolBodies": [tool], "operation": "CutFeatureOperation",
                   "isKeepToolBodies": true, "isNewComponent": false}}),
    );
    items.push(json!({"index": 4, "name": "C-Pattern1", "objectType": "CircularPatternFeature",
        "detail": {"inputEntities": [{"kind": "feature", "objectType": "ExtrudeFeature",
                                      "name": "Extrude1", "timeline_index": 1}],
                   "patternEntityType": "FeaturesPatternType",
                   "axis": {"kind": "construction_axis", "name": "Z", "origin": "Z"},
                   "quantity": param("d6", "4", 4.0, ""), "totalAngle": param("d7", "360 deg", std::f64::consts::TAU, "deg"),
                   "isSymmetric": false, "patternComputeOption": "IdenticalPatternCompute",
                   "suppressedElementsIds": [2]}}));
    let mut dump = timeline_dump(items);
    for (name, expression, unit) in [("d6", "4", ""), ("d7", "360 deg", "deg")] {
        dump.parameters
            .as_mut()
            .unwrap()
            .model
            .as_mut()
            .unwrap()
            .push(
                serde_json::from_value(
                    json!({"name": name, "expression": expression, "unit": unit}),
                )
                .unwrap(),
            );
    }
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
    let def = |uid: &str| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    assert_eq!(
        report.items[3].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let combine = def(&report.items[3].features[0]);
    assert_eq!(combine["target"], bodies[0].as_str());
    assert_eq!(combine["tools"], json!([bodies[1]]));
    assert_eq!(combine["operation"], "cut");
    assert_eq!(combine["keep_tools"], true);
    let pattern = &report.items[4];
    assert_eq!(pattern.outcome, Outcome::Parametric, "{}", report.text());
    let pattern = def(&pattern.features[0]);
    assert_eq!(
        pattern["objects"],
        json!({"type": "features", "features": ["F2"]})
    );
    assert_eq!(pattern["axis"], json!("z"));
    assert_eq!(pattern["quantity"], "d6");
    assert_eq!(pattern["angle"], "d7");
    assert_eq!(pattern["compute"], "identical");
    assert_eq!(pattern["suppressed_elements"], json!([2]));
}

#[test]
fn route_b_patterns_and_holes_fall_back() {
    // The stream decoder gives a pattern's angle only: no inputs, no axis.
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "C-Pattern1", "objectType": "CircularPatternFeature",
               "detail": {"totalAngle": param("d7", "360 deg", std::f64::consts::TAU, "deg")}}),
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Skipped,
        "{}",
        report.text()
    );
    assert!(
        report.items[2]
            .note
            .as_deref()
            .unwrap()
            .contains("no inputs"),
        "{}",
        report.text()
    );
}

/// The block dump with components: the root (object 3), Plate (10) placed
/// twice 10 cm apart, each with Pin (20) 1 cm along X in it, and an
/// occurrence of a component of another document.
fn assembly_dump(named: bool) -> Dump {
    let mut value: Value = serde_json::to_value(block_dump()).unwrap();
    let matrix = |x: f64| {
        json!([
            [1.0, 0.0, 0.0, x],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0]
        ])
    };
    let pin = json!({"component": "Pin", "isReferencedComponent": false,
                     "_f3d": {"component_object": 20, "local_transform": matrix(1.0)}, "children": []});
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "Plate", "_f3d": {"object_id": 10}},
        {"name": "Pin", "_f3d": {"object_id": 20}}]);
    value["occurrences"] = json!([
        {"component": "Plate", "transform": matrix(0.0), "isGrounded": true,
         "_f3d": {"component_object": 10}, "children": [pin]},
        {"component": "Plate", "transform": matrix(10.0), "isLightBulbOn": false,
         "_f3d": {"component_object": 10}, "children": [pin]},
        {"component": null, "isReferencedComponent": true, "transform": matrix(0.0),
         "_f3d": {"component_object": 3, "external_key": "00ff"}}]);
    if named {
        value["timeline"]["items"][1]["component"] = json!("Plate");
    }
    Dump::from_json(&value.to_string()).unwrap()
}

/// History data without states: which components items changed.
#[derive(Default)]
struct ComponentHistory {
    items: HashMap<i64, Vec<u64>>,
}

impl StoredGeometry<MockShape> for ComponentHistory {
    fn state_count(&mut self) -> usize {
        0
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        Err(format!("there is no history state {index}"))
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        Ok(Vec::new())
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.items.get(&index).cloned()
    }
}

#[test]
fn components_and_occurrences_come_in_with_their_items() {
    for named in [true, false] {
        let mut doc = Document::new(MockKernel::default());
        // An external dump names the extrude's component; the decoder leaves it
        // to the history, which says the extrude changed Plate's bodies.
        let mut geometry = ComponentHistory::default();
        if !named {
            geometry.items.insert(1, vec![10]);
        }
        let report = import_design(
            &mut doc,
            &assembly_dump(named),
            &mut geometry,
            &Options::default(),
        );
        let c = &report.components;
        assert_eq!(
            (c.components, c.occurrences, c.external),
            (2, 3, 1),
            "{}",
            report.text()
        );
        let a = doc.assembly();
        assert_eq!(a.root_name, "Assembly");
        let names: Vec<String> = a
            .occurrences
            .iter()
            .map(|o| a.occurrence_name(o.uid))
            .collect();
        assert_eq!(names, ["Plate:1", "Pin:1", "Plate:2"]);
        assert!(a.occurrences[0].grounded && !a.occurrences[2].visible);
        assert_eq!(a.occurrences[1].transform.translation, [10.0, 0.0, 0.0]);
        assert_eq!(a.occurrences[2].transform.translation, [100.0, 0.0, 0.0]);
        // The sketch follows the extrude that uses it into Plate.
        let plate = a.components[0].uid;
        assert!(doc.features().all(|f| f.component == plate));
        assert_eq!(report.items[0].component.as_deref(), Some("Plate"));
        assert_eq!(c.items.get("Plate"), Some(&2));
        // The plate's body in each occurrence.
        doc.set_occurrence_visible(a.occurrences[2].uid, true)
            .unwrap();
        let placed: Vec<f64> = doc
            .instances()
            .iter()
            .map(|i| i.transform.translation[0])
            .collect();
        assert_eq!(placed, [0.0, 100.0]);
        assert!(
            report
                .text()
                .contains("components: 2 made, 3 occurrences placed, 1 of other")
        );
    }
}

#[test]
fn undo_takes_back_the_whole_import() {
    let mut doc = Document::new(MockKernel::default());
    let options = Options {
        undo_label: Some("Import block.f3d".to_owned()),
        ..Options::default()
    };
    import_design(&mut doc, &block_dump(), &mut NoGeometry, &options);
    assert_eq!(doc.undo_label(), Some("Import block.f3d"));
    doc.undo();
    assert_eq!(doc.features().count(), 0);
    assert_eq!(doc.parameters().len(), 0);
}

// Sweeps, pipes and lofts (F3).

#[test]
fn sweeps_pipes_and_lofts_of_the_add_in_replay() {
    // Sketch1: the 40 x 20 rectangle; Sketch2: a line, the path.
    let path_sketch = json!({
        "index": 1, "name": "Sketch2", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [0.0, 5.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"}]
        }
    });
    let line = json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch2",
                      "sketch_timeline_index": 1, "id": "c0"});
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0,
                         "area": 8.0, "centroid": [2.0, 1.0, 0.0]});
    let sweep = json!({"index": 2, "name": "Sweep1", "objectType": "SweepFeature", "detail": {
        "profile": [profile], "operation": "NewBodyFeatureOperation",
        "path": [{"_type": "PathEntity", "entity": line, "isOpposedToEntity": false}],
        "orientation": "ParallelOrientationType", "isSolid": true,
        "distanceOne": param("d7", "0.5", 0.5, ""), "distanceTwo": param("d8", "1", 1.0, ""),
        "twistAngle": param("d9", "30 deg", 0.5235987755982988, "deg"),
        "taperAngle": param("d10", "0 deg", 0.0, "deg"), "isDirectionFlipped": true}});
    let pipe = json!({"index": 3, "name": "Pipe1", "objectType": "PipeFeature", "detail": {
        "path": [{"_type": "PathEntity", "entity": line}], "sectionType": "SquarePipeSectionType",
        "sectionSize": param("d11", "8 mm", 0.8, "mm"), "isHollow": true,
        "sectionThickness": param("d12", "1 mm", 0.1, "mm"),
        "distanceOne": param("d13", "1", 1.0, ""), "distanceTwo": param("d14", "0", 0.0, ""),
        "operation": "NewBodyFeatureOperation"}});
    let loft = json!({"index": 4, "name": "Loft1", "objectType": "LoftFeature", "detail": {
        "loftSections": [
            {"_type": "LoftSection", "index": 1, "entity": {"kind": "construction_point", "name": "Origin",
             "origin": "Origin"}, "endCondition": {"_type": "PointSharpEndCondition"}},
            {"_type": "LoftSection", "index": 0, "entity": profile,
             "endCondition": {"_type": "FreeEndCondition"}}],
        "isClosed": false, "isSolid": true, "operation": "NewBodyFeatureOperation"}});
    let dump = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "sweeps.f3d"},
        "timeline": {"items": [rectangle_sketch(0), path_sketch, sweep, pipe, loft]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes[1..], [Outcome::Parametric; 4], "{}", report.text());
    let def = |uid: u64| {
        let query = json!({"query": "feature", "uid": format!("F{uid}")}).to_string();
        let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
        answer["def"].clone()
    };
    let sweep = def(3);
    assert_eq!(sweep["type"], "sweep");
    assert_eq!(sweep["path"]["sketch"], "F2");
    assert_eq!(sweep["orientation"], "parallel");
    assert_eq!(sweep["flip"], true);
    assert_eq!(sweep["extent"]["type"], "partial");
    assert!(sweep.get("twist_angle").is_some() && sweep.get("taper_angle").is_none());
    let fraction = doc.parameters().get(
        doc.parameters()
            .find(sweep["extent"]["fraction"].as_str().unwrap())
            .unwrap(),
    );
    assert_eq!(fraction.map(|p| p.value()), Some(0.5));
    let pipe = def(4);
    assert_eq!(pipe["section"], "square");
    assert!(pipe.get("thickness").is_some() && pipe.get("extent").is_none());
    let loft = def(5);
    assert_eq!(loft["sections"][0]["type"], "profile");
    assert_eq!(
        loft["sections"][1],
        json!({"type": "point", "point": "origin"})
    );
    assert_eq!(loft["end_condition"], json!({"type": "point_sharp"}));
    assert!(loft.get("start_condition").is_none());
}

#[test]
fn prisms_bound_the_change_of_extrusions() {
    let extrude = |extent: Value| Candidate {
        defs: vec![json!({"type": "extrude", "extent": extent, "operation": "cut"})],
        note: None,
        guess: true,
        predicted: Some(-100.0),
    };
    assert!(bounded(&extrude(
        json!({"type": "distance", "distance": "d1"})
    )));
    let tapered = json!({"type": "two_sides", "side1": {"type": "distance", "taper": "d2"},
                         "side2": {"type": "distance"}});
    assert!(!bounded(&extrude(tapered)));
    assert!(!bounded(&extrude(
        json!({"type": "to_object", "object": {}})
    )));
    // A cut of at most 100 mm³ can remove 90 but not 120.
    assert!(can_change(-100.0, -90.0) && !can_change(-100.0, -120.0));
    assert!(can_change(-100.0, 50.0) && can_change(-100.0, 0.0));
}

#[test]
fn later_definitions_refer_to_earlier_features() {
    let uids: Vec<FeatureUid> = vec!["F7".parse().unwrap()];
    let mut def = json!({"type": "extrude", "profiles": [{"sketch": "$0", "region": "r{c1}"}],
                         "note": "$1", "other": "$x"});
    substitute(&mut def, &uids);
    assert_eq!(def["profiles"][0]["sketch"], "F7");
    // Unknown indices and other strings stay.
    assert_eq!(def["note"], "$1");
    assert_eq!(def["other"], "$x");
}

#[test]
fn sketch_texts_keep_style_frame_and_path() {
    // A text in a frame of four construction lines turned 90 degrees, and a
    // text fitted on an arc below it (cm in the IR).
    let p = |id: &str, x: f64, y: f64| json!({"id": id, "xyz": [x, y, 0.0]});
    let line = |id: &str, a: &str, b: &str| {
        json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b,
               "isConstruction": true})
    };
    let sketch = json!({
        "index": 0, "name": "Sketch1", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [p("p0", 0.0, 0.0), p("p1", 0.0, 4.0), p("p2", -2.0, 4.0), p("p3", -2.0, 0.0),
                       p("p4", 5.0, 0.0), p("p5", 7.0, 0.0), p("p6", 3.0, 0.0)],
            "curves": [line("c0", "p0", "p1"), line("c1", "p1", "p2"),
                       line("c2", "p2", "p3"), line("c3", "p3", "p0"),
                       {"id": "c4", "type": "SketchArc", "centerSketchPoint": "p4",
                        "startSketchPoint": "p5", "endSketchPoint": "p6"}],
            "texts": [
                {"id": "t0", "text": "AB", "height": 0.5, "position": [0.0, 0.0, 0.0],
                 "angle": std::f64::consts::FRAC_PI_2, "fontName": "Arial", "textStyle": 3,
                 "isHorizontalFlip": true,
                 "definition": {"_type": "MultiLineTextDefinition",
                                "rectangleLines": [{"kind": "sketch_entity", "id": "c0"},
                                                   {"kind": "sketch_entity", "id": "c1"},
                                                   {"kind": "sketch_entity", "id": "c2"},
                                                   {"kind": "sketch_entity", "id": "c3"}],
                                "horizontalAlignment": 1, "verticalAlignment": "MiddleVerticalAlignment",
                                "characterSpacing": 10.0}},
                {"id": "t1", "text": "C", "height": 0.4, "position": [5.0, 2.0, 0.0],
                 "definition": {"_type": "FitOnPathTextDefinition",
                                "path": {"kind": "sketch_entity", "id": "c4"}, "isAbovePath": false}}
            ]
        }
    });
    let mut doc = Document::new(MockKernel::default());
    import(&mut doc, &timeline_dump(vec![sketch]));
    let s: Value =
        serde_json::from_str(&doc.query(r#"{"query": "sketch", "uid": "F1"}"#).unwrap()).unwrap();
    let t = &s["texts"][0];
    assert_eq!(
        (t["bold"].clone(), t["italic"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(t["flip_x"], true);
    assert_eq!(t["height"], 5.0);
    assert_eq!(
        (t["align"].clone(), t["valign"].clone()),
        (json!("center"), json!("middle"))
    );
    assert_eq!(t["spacing"], 10.0);
    // Turned a quarter: the baseline runs up from (0, 0) along x = 0, and
    // the text's up is -x.
    let id = |stored: &str| {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| {
                let at = &e["at"];
                e["type"] == "point"
                    && match stored {
                        "p0" => at == &json!([0.0, 0.0]),
                        "p1" => at == &json!([0.0, 40.0]),
                        _ => at == &json!([-20.0, 0.0]),
                    }
            })
            .unwrap()["id"]
            .clone()
    };
    assert_eq!(t["frame"], json!([id("p0"), id("p1"), id("p3")]));
    let path = &s["texts"][1]["path"];
    assert_eq!(
        (path["above"].clone(), path["fit"].clone()),
        (json!(false), json!(true))
    );
    assert!(path["curve"].as_str().unwrap().starts_with('c'));
    assert!(
        s["regions"].as_array().unwrap().len() > 2,
        "the letters bound regions"
    );
}

// Lofts with end conditions and rails (P2).

#[test]
fn lofts_with_end_conditions_and_rails_come_in_as_definitions() {
    // Sketch1: the rectangle; Sketch2: a line, the rail.
    let path_sketch = json!({
        "index": 1, "name": "Sketch2", "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [0.0, 0.0, 0.0]}, {"id": "p1", "xyz": [0.0, 5.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1"}]
        }
    });
    let line = json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch2",
                      "sketch_timeline_index": 1, "id": "c0"});
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0,
                         "area": 8.0, "centroid": [2.0, 1.0, 0.0]});
    let loft = json!({"index": 2, "name": "Loft1", "objectType": "LoftFeature", "detail": {
        "loftSections": [
            {"_type": "LoftSection", "index": 0, "entity": profile,
             "endCondition": {"_type": "DirectionEndCondition",
                              "angle": param("d20", "10 deg", 0.17453292519943295, "deg"),
                              "weight": param("d21", "2", 2.0, "")}},
            {"_type": "LoftSection", "index": 1, "entity": {"kind": "construction_point", "name": "Origin",
             "origin": "Origin"},
             "endCondition": {"_type": "PointTangentEndCondition", "weight": param("d22", "0.5", 0.5, "")}}],
        "centerLineOrRails": [[{"_type": "PathEntity", "entity": line}]],
        "isClosed": false, "isSolid": true, "operation": "NewBodyFeatureOperation"}});
    let dump = dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "lofts.f3d"},
        "timeline": {"items": [rectangle_sketch(0), path_sketch, loft]}
    }));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let query = json!({"query": "feature", "uid": "F3"}).to_string();
    let answer: Value = serde_json::from_str(&doc.query(&query).unwrap()).unwrap();
    let def = &answer["def"];
    assert_eq!(def["start_condition"]["type"], "direction");
    assert_eq!(def["end_condition"]["type"], "point_tangent");
    assert_eq!(def["rails"][0]["sketch"], "F2");
    let value = |slot: &Value| {
        let parameters = doc.parameters();
        parameters
            .get(parameters.find(slot.as_str().unwrap()).unwrap())
            .map(|p| p.value())
    };
    assert_eq!(value(&def["start_condition"]["weight"]), Some(2.0));
    assert_eq!(value(&def["end_condition"]["weight"]), Some(0.5));
    // Mitcad's own rules for end conditions: matched against
    // the history more loosely.
    assert_eq!(
        Candidate::new(def.clone()).tolerance(),
        history::CONVENTIONS
    );
    let mut free = def.clone();
    for key in ["start_condition", "end_condition", "rails"] {
        free.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(Candidate::new(free).tolerance(), history::RELATIVE);
}

#[test]
fn a_loft_section_of_edges_is_the_face_they_go_round() {
    // A tangent condition is taken only at a section of edges: here the
    // loop round the block's top, as a Path of its BRepEdges (cm).
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference)[0].clone();
    let top = block
        .faces
        .iter()
        .find(|f| f.to_string().starts_with("F2:end("))
        .unwrap()
        .clone();
    let edge = |a: &mitcad_model::FaceName, b: &mitcad_model::FaceName| {
        let p = edge_point(a, b);
        json!({"_type": "PathEntity", "entity": {"kind": "edge", "objectType": "BRepEdge",
               "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0], "length": 0.1}})
    };
    let round_top: Vec<Value> = block
        .edges
        .iter()
        .filter(|(a, b)| *a == top || *b == top)
        .map(|(a, b)| edge(a, b))
        .collect();
    assert_eq!(round_top.len(), 4);
    let loft = |entity: Value| {
        json!({"index": 2, "name": "Loft1", "objectType": "LoftFeature", "detail": {
            "loftSections": [
                {"_type": "LoftSection", "index": 0, "entity": entity,
                 "endCondition": {"_type": "LoftTangentEndCondition", "weight": param("d9", "1", 1.0, "")}},
                {"_type": "LoftSection", "index": 1, "entity": {"kind": "construction_point",
                 "name": "Origin", "origin": "Origin"},
                 "endCondition": {"_type": "LoftPointSharpEndCondition"}}],
            "isClosed": false, "isSolid": true, "operation": "NewBodyFeatureOperation"}})
    };
    let replay = |entity: Value| {
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        let mut items = block_items.clone();
        items.push(loft(entity));
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        let def = report.items[2].features.first().map(|uid| {
            serde_json::from_str::<Value>(
                &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                    .unwrap(),
            )
            .unwrap()["def"]
                .clone()
        });
        (report, def)
    };
    let (report, def) = replay(json!(round_top));
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def = def.unwrap();
    assert_eq!(
        def["sections"][0],
        json!({"type": "face", "body": "F2.b0", "face": top.to_string()})
    );
    // The dump's type names (`LoftTangentEndCondition`).
    assert_eq!(def["start_condition"]["type"], "tangent");
    assert_eq!(def["end_condition"], json!({"type": "point_sharp"}));
    // One edge of the loop (a BRepEdge as the section) goes round no face.
    let (report, _) = replay(round_top[0]["entity"].clone());
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
    assert!(
        report.text().contains("its edges do not go round one face"),
        "{}",
        report.text()
    );
}

#[test]
fn fillet_and_chamfer_options_come_in_as_their_definitions() {
    // The block's first edge, fingerprinted as external dumps do it (cm).
    let block_items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(block_items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (a, b) = shapes(&reference)[0].edges[0].clone();
    let p = edge_point(&a, &b);
    let edge = json!({"kind": "edge", "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
                      "length": 0.1});
    let fillet = |set: Value| {
        let mut set = set;
        set["edges"] = json!([edge]);
        json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
               "detail": {"edgeSets": [set]}})
    };
    let radius = param("d5", "2 mm", 0.2, "mm");
    let three = param("d6", "3 mm", 0.3, "mm");
    for item in [
        fillet(
            json!({"radius": radius, "continuity": "CurvatureSurfaceContinuityType",
                      "tangencyWeight": param("d7", "1.5", 1.5, "")}),
        ),
        fillet(
            json!({"_type": "VariableRadiusFilletEdgeSet", "startRadius": radius,
                      "endRadius": radius, "midRadii": [three], "midPositions": [0.25]}),
        ),
        fillet(
            json!({"_type": "AsymmetricFilletEdgeSet", "offsetOne": radius,
                      "offsetTwo": three, "isFlipped": true}),
        ),
        json!({"index": 2, "name": "Chamfer1", "objectType": "ChamferFeature",
               "detail": {"edgeSets": [{"edges": [edge], "distance": radius}],
                          "cornerType": "MiterCornerType"}}),
    ] {
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        let mut items = block_items.clone();
        items.push(item);
        let report = import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        );
        let outcome = &report.items[2];
        assert_eq!(outcome.outcome, Outcome::Parametric, "{}", report.text());
        let def: Value = serde_json::from_str::<Value>(
            &doc.query(&format!(
                r#"{{"query": "feature", "uid": "{}"}}"#,
                outcome.features[0]
            ))
            .unwrap(),
        )
        .unwrap()["def"]
            .clone();
        let value = |slot: &Value| {
            let parameters = doc.parameters();
            match slot.as_str() {
                Some(name) => parameters
                    .get(parameters.find(name).unwrap())
                    .map(|p| p.value()),
                None => slot.as_f64(),
            }
        };
        let set = &def["sets"][0];
        match def["type"].as_str().unwrap() {
            "chamfer" => assert_eq!(def["corner"], "miter"),
            _ if set["continuity"] == "curvature" => {
                assert_eq!(value(&set["tangency_weight"]), Some(1.5));
            }
            _ if set["size"]["type"] == "variable" => {
                assert_eq!(set["size"]["mid"][0]["position"], 0.25);
                assert_eq!(value(&set["size"]["mid"][0]["radius"]), Some(3.0));
            }
            _ => {
                assert_eq!(set["size"]["type"], "asymmetric");
                assert_eq!(value(&set["size"]["distance1"]), Some(2.0));
                assert_eq!(value(&set["size"]["distance2"]), Some(3.0));
                assert_eq!(set["size"]["flip"], true);
            }
        }
        // Mitcad's own shapes: matched against the history more loosely.
        assert_eq!(
            Candidate::new(def.clone()).tolerance(),
            history::CONVENTIONS
        );
    }
    let plain = json!({"type": "fillet", "sets": [{"size": {"type": "constant", "radius": 2}}]});
    assert_eq!(Candidate::new(plain).tolerance(), history::RELATIVE);
}

/// A replace face of the block's top onto the XY plane, as external dumps give
/// it (no source faces), and the history with the file's result.
fn replace_face_history() -> (Vec<Value>, TestGeometry, String) {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "ReplaceFace1", "objectType": "ReplaceFaceFeature",
               "detail": {"targetFaces": [{"kind": "construction_plane", "name": "XY", "origin": "XY"}],
                          "isTangentChain": true}}),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference);
    let top = block[0]
        .faces
        .iter()
        .find(|f| f.to_string().starts_with("F2:end("))
        .expect("the block's top")
        .to_string();
    add(
        &mut reference,
        json!({"type": "replace_face", "body": "F2.b0", "faces": [top], "target": "xy",
               "tangent_chain": false}),
    );
    let geometry = TestGeometry {
        states: vec![Vec::new(), block, shapes(&reference)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    (items, geometry, top)
}

#[test]
fn replace_faces_come_in_with_the_faces_the_history_lost() {
    let (items, mut geometry, top) = replace_face_history();
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items.clone()),
        &mut geometry,
        &Options::default(),
    );
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(item.verified, Some(true));
    let def: Value = serde_json::from_str::<Value>(
        &doc.query(&format!(
            r#"{{"query": "feature", "uid": "{}"}}"#,
            item.features[0]
        ))
        .unwrap(),
    )
    .unwrap()["def"]
        .clone();
    assert_eq!(def["type"], "replace_face");
    assert_eq!(def["faces"], json!([top]));
    assert_eq!(def["target"], "xy");
    assert_eq!(def["tangent_chain"], false);

    // Without a history the faces are not known: the file's bodies instead.
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
    let note = report.items[2].note.clone().unwrap_or_default();
    assert!(
        note.contains("source faces were not decoded"),
        "{}",
        report.text()
    );
    // Replace faces follow Mitcad's conventions: matched more loosely.
    assert_eq!(
        Candidate::new(json!({"type": "replace_face"})).tolerance(),
        history::CONVENTIONS
    );
}

/// mitcad#6: the file's light bulbs are kept where they differ from Mitcad's
/// default (a sketch hidden while a feature uses it, construction geometry
/// shown), and only there; unknown ones follow the default.
#[test]
fn light_bulbs_are_kept_where_they_differ_from_the_default() {
    let sketch = |index: i64, bulb: Option<bool>| {
        let mut s = rectangle_sketch(index);
        s["detail"]["dimensions"] = json!([]);
        if let Some(on) = bulb {
            s["props"] = json!({"isLightBulbOn": on});
        }
        s
    };
    let plane = |index: i64, on: bool| {
        json!({"index": index, "name": format!("Plane{index}"), "objectType": "ConstructionPlane",
               "detail": {"geometry": {"type": "Plane", "origin": [0.0, 0.0, 1.0],
                                       "normal": [0.0, 0.0, 1.0]}},
               "props": {"isLightBulbOn": on}})
    };
    let items = vec![
        sketch(0, Some(true)), // Sketch1: used and shown: kept
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        sketch(2, Some(false)), // Sketch3: unused and hidden: kept
        sketch(3, Some(false)), // Sketch4: used and hidden: the default
        extrude(4, 3, "NewBodyFeatureOperation", "d2"),
        sketch(5, Some(true)), // Sketch6: unused and shown: the default
        sketch(6, None),       // Sketch7: not known: the default
        plane(7, false),       // hidden: kept
        plane(8, true),        // shown: the default
    ];
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &timeline_dump(items));
    let kept: Vec<(String, bool)> = doc
        .features()
        .filter_map(|f| Some((f.name.clone(), doc.feature_visible(f.uid)?)))
        .collect();
    assert_eq!(
        kept,
        [
            ("Sketch1".to_owned(), true),
            ("Sketch3".to_owned(), false),
            ("Plane7".to_owned(), false)
        ],
        "{}",
        report.text()
    );
    assert_eq!(
        report.light_bulbs,
        LightBulbReport {
            sketch_bulbs: 5,
            sketch_bulbs_unknown: 1,
            construction_bulbs: 2,
            construction_bulbs_unknown: 0,
            bulbs_set: 3,
        }
    );
    assert!(
        report
            .text()
            .contains("light bulbs: 5 sketches (1 unknown)")
    );
    let uid = |name: &str| doc.features().find(|f| f.name == name).unwrap().uid;
    let shown = |doc: &Document<MockKernel>, name: &str| {
        doc.feature_shown(doc.features().find(|f| f.name == name).unwrap().uid)
    };
    assert_eq!(shown(&doc, "Sketch4"), Some(false));
    assert_eq!(shown(&doc, "Sketch6"), Some(true));
    assert_eq!(shown(&doc, "Sketch7"), Some(true));
    assert_eq!(shown(&doc, "Plane8"), Some(true));
    // A sketch that follows the default shows when its consumer goes.
    let extrude4 = uid("Extrude4");
    doc.delete_feature(extrude4, false).unwrap();
    assert_eq!(shown(&doc, "Sketch4"), Some(true));
}
